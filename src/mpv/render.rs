//! Software-render frame pump for libmpv.
//!
//! mpv 0.41 has no Vulkan render sub-API in this world; its render backends
//! are `opengl` (needs an OpenGL context / X11 — unavailable under Wayland
//! here) and `sw` (CPU render into an in-memory RGB surface — always
//! available, no display needed). This module drives the **sw** backend on a
//! dedicated worker thread:
//!
//! ```text
//! mpv core ──update callback──▶ dirty flag
//!                                      │
//! worker thread ──(poll ~25 ms)────────┤
//!      │  mpv_render_context_update + render (bgr0)
//!      ▼
//!   FrameSlot ──Arc<Vec<u8>> frame──▶ wgpu texture upload (crate::pipeline)
//! ```
//!
//! Threading rules (from `render.h`):
//! - all `mpv_render_*` calls happen on the worker thread only,
//! - the update callback never calls `mpv_render_*` (it only sets a flag).
//!
//! Edition 2024 makes `unsafe_op_in_unsafe_fn` deny-by-default; the
//! trampoline below is already `unsafe` and merely forwards FFI calls.
#![allow(unsafe_op_in_unsafe_fn)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use super::ffi::{
    self, MPV_RENDER_PARAM_API_TYPE, MPV_RENDER_PARAM_INVALID, MPV_RENDER_PARAM_SW_FORMAT,
    MPV_RENDER_PARAM_SW_POINTER, MPV_RENDER_PARAM_SW_SIZE, MPV_RENDER_PARAM_SW_STRIDE, MpvError,
    mpv_handle, mpv_render_context,
};

// ---------------------------------------------------------------------------
// Shared frame slot
// ---------------------------------------------------------------------------

/// A single rendered video frame (BGRA / `bgr0`, 4 bytes per pixel).
#[derive(Debug, Clone)]
pub struct Frame {
    /// Pixel data, `stride * height` bytes (row 0 at offset 0, row `y` at
    /// `stride * y`). BGRA byte order.
    pub bytes: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Bytes per row (multiple of 64).
    pub stride: u32,
    /// Monotonic frame counter (0 = none yet).
    pub frame_id: u64,
}

/// Thread-safe slot holding the latest rendered frame.
///
/// Cheap to clone (shares the frame payload); the model layer snapshots it
/// on each UI frame and passes it to the wgpu pipeline for upload.
#[derive(Debug)]
pub struct FrameSlot {
    current: Mutex<Option<Frame>>,
    signaled: AtomicBool,
    /// Wake-up for [`FrameSlot::wait_for_new`].
    cv: Condvar,
}

impl FrameSlot {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            current: Mutex::new(None),
            signaled: AtomicBool::new(false),
            cv: Condvar::new(),
        })
    }

    /// Take the latest frame (if any).
    pub fn snapshot(&self) -> Option<Frame> {
        self.current.lock().ok()?.clone()
    }

    /// `true` (and cleared) if a new frame was published since the last call.
    pub fn take_signal(&self) -> bool {
        self.signaled.swap(false, Ordering::SeqCst)
    }

    /// Block until a new frame is published or `timeout` elapses.
    /// Returns `true` if a frame is now available.
    pub fn wait_for_new(&self, timeout: Duration) -> bool {
        let mut guard = self.current.lock().unwrap();
        let deadline = Instant::now() + timeout;
        loop {
            if guard.is_some() || self.signaled.load(Ordering::SeqCst) {
                self.signaled.store(false, Ordering::SeqCst);
                return guard.is_some();
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (g, _) = match self.cv.wait_timeout(guard, deadline - now) {
                Ok(v) => v,
                Err(_) => return false,
            };
            guard = g;
        }
    }

    pub(crate) fn publish(&self, frame: Frame) {
        let mut guard = self.current.lock().unwrap();
        *guard = Some(frame);
        drop(guard);
        self.signaled.store(true, Ordering::SeqCst);
        self.cv.notify_all();
    }
}

impl Default for FrameSlot {
    fn default() -> Self {
        Self {
            current: Mutex::new(None),
            signaled: AtomicBool::new(false),
            cv: Condvar::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

const TICK: Duration = Duration::from_millis(25);
/// If a file is loaded but no frame has been published for this long, force
/// a render (covers missed update callbacks, e.g. after track switches).
const STALE_LIMIT: Duration = Duration::from_millis(500);
/// Cap the sw-render target size to keep CPU load sane.
const MAX_DIM: u32 = 1920;

/// State shared with the raw update-callback trampoline.
struct CbState {
    dirty: Arc<AtomicBool>,
}

/// Shared state owned by the worker thread (pointed at by the update
/// callback trampoline; freed on shutdown).
struct WorkerState {
    ctx: *mut mpv_render_context,
    handle: *mut mpv_handle,
    slot: Arc<FrameSlot>,
    running: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
    frame_id: AtomicU64,
    last_publish: Mutex<Instant>,
    cb: CbState,
}

unsafe impl Send for WorkerState {}

/// `sw` render pixel format: BGRA, 4 bytes per pixel.
const SW_FORMAT: &str = "bgr0";
/// API selector string for the software renderer.
const SW_API: &str = "sw";

unsafe extern "C" fn update_trampoline(callback_ctx: *mut std::os::raw::c_void) {
    let state = &*(callback_ctx as *const CbState);
    state.dirty.store(true, Ordering::SeqCst);
}

/// Size query helpers for the sw render target (native video size, capped).
fn query_target_size(handle: *mut mpv_handle) -> Option<(u32, u32)> {
    let get = |name: &str| -> Option<f64> {
        let c = std::ffi::CString::new(name).ok()?;
        let mut v: f64 = 0.0;
        let rc = unsafe {
            ffi::mpv_get_property(
                handle,
                c.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                &mut v as *mut _ as *mut std::os::raw::c_void,
            )
        };
        if rc < 0 {
            return None;
        }
        (v > 0.0).then_some(v)
    };
    let w = get("dwidth")? as u32;
    let h = get("dheight")? as u32;
    if w == 0 || h == 0 {
        return None;
    }
    // Cap the long edge to keep the CPU sw-render tractable.
    let scale = if w.max(h) > MAX_DIM {
        MAX_DIM as f64 / w.max(h) as f64
    } else {
        1.0
    };
    let (w, h) = ((w as f64 * scale) as u32, (h as f64 * scale) as u32);
    Some((w.max(2), h.max(2)))
}

/// Drives the mpv `sw` render API on a worker thread, publishing BGRA
/// frames into a [`FrameSlot`].
///
/// Owns the render context; drop it (or call [`SwRenderer::shutdown`]) to
/// stop the worker and free the context.
pub struct SwRenderer {
    slot: Arc<FrameSlot>,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for SwRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwRenderer").finish()
    }
}

impl SwRenderer {
    /// Create the `sw` render context and start the worker thread.
    ///
    /// The core must already be initialized (see [`super::MpvPlayer::init`]).
    pub fn spawn(player: &super::player::MpvPlayer) -> Result<Self, MpvError> {
        let handle = player.handle();
        let dirty = Arc::new(AtomicBool::new(false));
        let slot = FrameSlot::new();
        let running = Arc::new(AtomicBool::new(true));

        // Build the worker state on the stack; hand it to the thread by
        // value (it is `Send`).
        let cb = CbState {
            dirty: Arc::clone(&dirty),
        };

        // Create the render context (sw backend) before starting the thread
        // so failures don't leak a worker.
        let mut ctx: *mut mpv_render_context = std::ptr::null_mut();
        // NUL-terminated C string, must outlive the create call.
        let api_c = std::ffi::CString::new(SW_API)
            .map_err(|_| MpvError(ffi::MPV_ERROR_INVALID_PARAMETER))?;
        let api = api_c.as_ptr() as *mut std::os::raw::c_void;
        let params = [
            ffi::mpv_render_param {
                r#type: MPV_RENDER_PARAM_API_TYPE,
                data: api,
            },
            ffi::mpv_render_param {
                r#type: MPV_RENDER_PARAM_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        let rc = unsafe { ffi::mpv_render_context_create(&mut ctx, handle, params.as_ptr()) };
        ffi::result_of(rc)?;
        debug_assert!(!ctx.is_null());

        let state = WorkerState {
            ctx,
            handle,
            slot: Arc::clone(&slot),
            running: Arc::clone(&running),
            dirty: Arc::clone(&dirty),
            frame_id: AtomicU64::new(0),
            last_publish: Mutex::new(Instant::now()),
            cb,
        };

        let thread = std::thread::Builder::new()
            .name("mpv-sw-render".into())
            .spawn(move || worker_loop(state))
            .map_err(|_| MpvError(ffi::MPV_ERROR_NOMEM))?;

        Ok(Self {
            slot,
            running,
            thread: Some(thread),
        })
    }

    /// The shared frame slot for the UI / pipeline side.
    pub fn slot(&self) -> Arc<FrameSlot> {
        Arc::clone(&self.slot)
    }

    /// Stop the worker and free the render context.
    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SwRenderer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Worker thread main loop.
fn worker_loop(mut state: WorkerState) {
    // Install the update callback (fires on the core thread when a new
    // frame is ready). We only set a flag here — never call mpv_render_*
    // from the callback.
    unsafe {
        ffi::mpv_render_context_set_update_callback(
            state.ctx,
            Some(update_trampoline),
            std::ptr::addr_of_mut!(state.cb).cast::<std::os::raw::c_void>(),
        );
    }

    loop {
        if !state.running.load(Ordering::SeqCst) {
            break;
        }

        // Wait for the dirty flag up to one tick.
        let start = Instant::now();
        while !state.dirty.load(Ordering::Acquire) {
            if start.elapsed() > TICK {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        let was_dirty = state.dirty.swap(false, Ordering::SeqCst);

        // Stale-frame safety net: a file is loaded but nothing has been
        // published recently (e.g. missed callback after a track switch).
        let mut stale_force = false;
        if !was_dirty {
            let last = *state.last_publish.lock().unwrap();
            if last.elapsed() > STALE_LIMIT {
                if let Some(false) = player_idle(state.handle) {
                    stale_force = true;
                }
            }
        }

        if was_dirty || stale_force {
            render_once(&mut state);
        }
    }

    // Tear down the render context (worker thread owns all mpv_render_* calls).
    unsafe {
        ffi::mpv_render_context_set_update_callback(state.ctx, None, std::ptr::null_mut());
        ffi::mpv_render_context_free(state.ctx);
    }
}

/// `true` when the core is idle (no file loaded), `false` when a file is
/// active, `None` when the property is unavailable.
fn player_idle(handle: *mut mpv_handle) -> Option<bool> {
    let c = std::ffi::CString::new("idle-active").ok()?;
    let mut v: i32 = 0;
    let rc = unsafe {
        ffi::mpv_get_property(
            handle,
            c.as_ptr(),
            ffi::MPV_FORMAT_FLAG,
            &mut v as *mut _ as *mut std::os::raw::c_void,
        )
    };
    (rc >= 0).then_some(v != 0)
}

/// Render the current mpv frame into a fresh BGRA buffer and publish it.
fn render_once(state: &mut WorkerState) {
    let (w, h) = match query_target_size(state.handle) {
        Some(s) => s,
        None => return, // nothing to render (no video track)
    };
    let stride = align_up(w * 4, 64);
    let bytes = vec![0u8; (stride as u32 * h) as usize];

    let size = ffi::mpv_render_size {
        w: w as i32,
        h: h as i32,
    };
    let format_c = std::ffi::CString::new(SW_FORMAT).unwrap();
    let format_ptr = format_c.as_ptr() as *mut std::os::raw::c_void;
    // SW_STRIDE is `size_t *` (usize), per render.h.
    let stride_us = stride as usize;
    let stride_ptr = &stride_us as *const usize as *mut std::os::raw::c_void;
    // SAFETY: `bytes` lives until the call returns; alignment (8) >= the
    // bgr0 pixel alignment (4).
    let bytes_ptr = bytes.as_ptr() as *mut std::os::raw::c_void;
    let params = [
        ffi::mpv_render_param {
            r#type: MPV_RENDER_PARAM_SW_SIZE,
            data: &size as *const _ as *mut _,
        },
        ffi::mpv_render_param {
            r#type: MPV_RENDER_PARAM_SW_FORMAT,
            data: format_ptr,
        },
        ffi::mpv_render_param {
            r#type: MPV_RENDER_PARAM_SW_STRIDE,
            data: stride_ptr,
        },
        ffi::mpv_render_param {
            r#type: MPV_RENDER_PARAM_SW_POINTER,
            data: bytes_ptr,
        },
        ffi::mpv_render_param {
            r#type: MPV_RENDER_PARAM_INVALID,
            data: std::ptr::null_mut(),
        },
    ];

    unsafe {
        ffi::mpv_render_context_update(state.ctx);
        let rc = ffi::mpv_render_context_render(state.ctx, params.as_ptr());
        if rc < 0 {
            log::warn!("mpv sw render failed: {rc}");
            return;
        }
        ffi::mpv_render_context_report_swap(state.ctx);
    }

    let frame_id = state.frame_id.fetch_add(1, Ordering::SeqCst) + 1;
    let frame = Frame {
        bytes: Arc::new(bytes),
        width: w,
        height: h,
        stride,
        frame_id,
    };
    log::debug!(
        "[mpv] publish: {}x{} stride={} id={}",
        w,
        h,
        stride,
        frame_id
    );
    *state.last_publish.lock().unwrap() = Instant::now();
    state.slot.publish(frame);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_to_multiple() {
        assert_eq!(align_up(4, 64), 64);
        assert_eq!(align_up(64, 64), 64);
        assert_eq!(align_up(65, 64), 128);
        assert_eq!(align_up(1920 * 4, 64), 7680);
        assert_eq!(align_up(1, 64), 64);
    }

    #[test]
    fn frame_slot_snapshot_roundtrip() {
        let slot = FrameSlot::new();
        assert!(slot.snapshot().is_none());

        let frame = Frame {
            bytes: Arc::new(vec![0xA5u8; 64 * 4 * 2]),
            width: 64,
            height: 2,
            stride: 64 * 4,
            frame_id: 1,
        };
        slot.publish(frame.clone());
        let got = slot.snapshot().expect("frame present");
        assert_eq!(got.width, 64);
        assert_eq!(got.height, 2);
        assert_eq!(got.stride, 64 * 4);
        assert_eq!(got.frame_id, 1);
        assert_eq!(got.bytes.as_ref(), &vec![0xA5u8; 64 * 4 * 2]);

        // Sharing works: two snapshots alias the same allocation.
        let a = slot.snapshot().unwrap().bytes;
        let b = slot.snapshot().unwrap().bytes;
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn frame_slot_signal_semantics() {
        let slot = FrameSlot::new();
        assert!(!slot.take_signal());
        slot.publish(Frame {
            bytes: Arc::new(vec![0; 16]),
            width: 4,
            height: 1,
            stride: 16,
            frame_id: 7,
        });
        assert!(slot.take_signal());
        assert!(!slot.take_signal());
    }

    #[test]
    fn frame_slot_wait_for_new() {
        let slot = FrameSlot::new();
        let slot2 = Arc::clone(&slot);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            slot2.publish(Frame {
                bytes: Arc::new(vec![0xFF; 32]),
                width: 8,
                height: 1,
                stride: 32,
                frame_id: 3,
            });
        });
        let ok = slot.wait_for_new(Duration::from_secs(2));
        assert!(ok);
        assert_eq!(slot.snapshot().unwrap().frame_id, 3);
        worker.join().unwrap();
    }

    #[test]
    fn frame_slot_wait_times_out() {
        let slot = FrameSlot::new();
        let start = Instant::now();
        assert!(!slot.wait_for_new(Duration::from_millis(20)));
        assert!(start.elapsed() >= Duration::from_millis(15));
    }
}

fn align_up(n: u32, a: u32) -> u32 {
    ((n + a - 1) / a) * a
}

#[cfg(test)]
#[test]
fn sw_api_and_format_strings() {
    assert_eq!(SW_API, "sw");
    assert_eq!(SW_FORMAT, "bgr0");
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::mpv::MpvPlayer;
    use std::time::Duration;

    #[test]
    fn sw_render_delivers_frames() {
        let path = std::env::temp_dir().join("mb_sw_probe.mp4");
        if !path.exists() {
            let ok = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=duration=2:size=320x240:rate=10",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:v",
                    "libx264",
                    "-an",
                ])
                .arg(&path)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                eprintln!("skip: no ffmpeg");
                return;
            }
        }
        let p = MpvPlayer::new().expect("player");
        let renderer = SwRenderer::spawn(&p).expect("renderer");
        p.loadfile(&path.to_string_lossy(), "replace")
            .expect("loadfile");
        p.play().expect("play");
        let slot = renderer.slot();
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut got = None;
        while Instant::now() < deadline && got.is_none() {
            slot.wait_for_new(Duration::from_millis(200));
            got = slot.snapshot().filter(|f| f.bytes.iter().any(|b| *b != 0));
        }
        let f = got.expect("sw render delivered no non-black frame within 6s");
        assert_eq!((f.width, f.height), (320, 240));
    }
}
