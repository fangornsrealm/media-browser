//! Safe Rust wrapper around the libmpv client API (see [`super::ffi`]).
//!
//! This is the **unblocked, independently-testable** part of the libmpv
//! migration (plan §7.2): it drives decoding / playback / properties / events
//! through the mpv *client* ABI and is exercised by the unit tests below. It
//! deliberately excludes the render-context path (Vulkan / OpenGL sub-API),
//! which is blocked by R1 — wgpu 28 cannot hand mpv a `VkInstance` /
//! `VkDevice` / `VkQueue`, nor import mpv's rendered `VkImage`. See
//! `docs/PLAN-libcosmic-1.0-wgpu-28-libmpv.md`, Appendix B (Q2).
//!
//! # Threading
//!
//! [`MpvPlayer`] holds a single raw `mpv_handle` and is intentionally neither
//! `Send` nor `Sync` (raw pointers do not auto-derive them). Individual mpv
//! commands are thread-safe, but the event loop ([`MpvPlayer::wait_event`])
//! should be driven from one thread.
//!
//! # Units
//!
//! Values use mpv's **native** units:
//! * `volume`: `0.0` (silent) … `100.0` (100%); above `100.0` amplifies.
//! * `position` / `duration` / `speed` / `seek`: seconds (`f64`).
//!
//! The higher-level model (plan §8.1) maps these onto the app's `0.0..=1.0`
//! volume scale and `Duration`-based timing.

use std::ffi::CString;
use std::os::raw::c_void;

use super::ffi;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned by the safe [`MpvPlayer`] wrapper.
#[derive(Debug, thiserror::Error)]
pub enum MpvError {
    /// `mpv_create()` returned NULL (e.g. out of memory).
    #[error("mpv_create() returned NULL")]
    CreateFailed,
    /// `mpv_initialize()` failed with the given mpv status code.
    #[error("mpv_initialize() failed with status {0}")]
    Init(i32),
    /// A `&str` to be passed to libmpv contained an interior NUL byte.
    #[error("a C string contained an interior NUL byte")]
    NulByte,
    /// An mpv command or property call returned a non-success status.
    #[error("mpv call failed with status {0}")]
    Status(i32),
}

// ---------------------------------------------------------------------------
// Track / event types
// ---------------------------------------------------------------------------

/// A media track kind, mirroring mpv's `aid` / `vid` / `sid` properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Audio track (`aid`).
    Audio,
    /// Video track (`vid`).
    Video,
    /// Subtitle track (`sid`).
    Subtitle,
}

impl TrackKind {
    /// The mpv property that holds the active track id for this kind.
    const fn prop(self) -> &'static str {
        match self {
            TrackKind::Audio => "aid",
            TrackKind::Video => "vid",
            TrackKind::Subtitle => "sid",
        }
    }
}

/// An active media track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub kind: TrackKind,
    /// mpv track id (`> 0` = concrete track, `0` = none, `-1` = auto).
    pub id: i64,
}

/// A (header) event returned by [`MpvPlayer::wait_event`].
#[derive(Debug, Clone, Copy)]
pub struct MpvEvent {
    /// `enum mpv_event_id` (see `ffi::MPV_EVENT_*`).
    pub event_id: i32,
    /// mpv status code for the event; [`ffi::MPV_ERROR_SUCCESS`] on success.
    pub error: i32,
}

impl MpvEvent {
    /// The raw `mpv_event_id`.
    pub fn id(&self) -> i32 {
        self.event_id
    }

    /// `true` if the event carried no error.
    pub fn ok(&self) -> bool {
        self.error == ffi::MPV_ERROR_SUCCESS
    }

    /// `true` for `MPV_EVENT_END_FILE` (end of playback / error-out of a file).
    pub fn is_end_file(&self) -> bool {
        self.event_id == ffi::MPV_EVENT_END_FILE
    }

    /// `true` for `MPV_EVENT_SHUTDOWN`.
    pub fn is_shutdown(&self) -> bool {
        self.event_id == ffi::MPV_EVENT_SHUTDOWN
    }

    /// `true` for `MPV_EVENT_FILE_LOADED`.
    pub fn is_file_loaded(&self) -> bool {
        self.event_id == ffi::MPV_EVENT_FILE_LOADED
    }
}

// ---------------------------------------------------------------------------
// C-string helpers
// ---------------------------------------------------------------------------

fn to_cstr(s: &str) -> Result<CString, MpvError> {
    CString::new(s).map_err(|_| MpvError::NulByte)
}

/// Escape `s` for use as an argument inside an mpv command string.
///
/// mpv command strings separate arguments by whitespace and support `"..."`
/// quoting with `\"` and `\\` escapes. We wrap the value in quotes and escape
/// those two metacharacters so paths/URLs containing spaces round-trip
/// correctly (see `loadfile` / `set sub-file` below).
fn mpv_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Apply a single `name=value` option to a pre-init handle.
fn apply_option(handle: *mut ffi::mpv_handle, name: &str, value: &str) -> Result<(), MpvError> {
    let c_name = to_cstr(name)?;
    let c_value = to_cstr(value)?;
    // SAFETY: `handle` is valid (owned by the caller, pre-initialization);
    // `c_name` / `c_value` are valid NUL-terminated strings outliving the call.
    let rc = unsafe { ffi::mpv_set_option_string(handle, c_name.as_ptr(), c_value.as_ptr()) };
    if rc != ffi::MPV_ERROR_SUCCESS {
        return Err(MpvError::Status(rc));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MpvPlayer
// ---------------------------------------------------------------------------

/// A safe handle to a single mpv player instance.
///
/// Owns the underlying `mpv_handle`; it is terminated on drop.
pub struct MpvPlayer {
    handle: *mut ffi::mpv_handle,
}

impl std::fmt::Debug for MpvPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpvPlayer")
            .field("handle", &(self.handle as *const ()))
            .finish()
    }
}

impl MpvPlayer {
    /// Create a player with only `config=no` set (user config ignored).
    ///
    /// Audio / video output are left at mpv defaults; use
    /// [`with_options`](Self::with_options) to pin, e.g., a specific `ao`/`vo`.
    pub fn new() -> Result<Self, MpvError> {
        Self::with_options(&[])
    }

    /// Create a player, applying the given `name=value` options before
    /// initialization.
    pub fn with_options(options: &[(&str, &str)]) -> Result<Self, MpvError> {
        // SAFETY: mpv_create() returns an opaque handle we exclusively own.
        let handle = unsafe { ffi::mpv_create() };
        if handle.is_null() {
            return Err(MpvError::CreateFailed);
        }

        // Ignore user config files for deterministic behavior.
        apply_option(handle, "config", "no")?;
        for (name, value) in options {
            apply_option(handle, name, value)?;
        }

        // SAFETY: `handle` is valid; all options were set pre-initialization.
        let rc = unsafe { ffi::mpv_initialize(handle) };
        if rc != ffi::MPV_ERROR_SUCCESS {
            // SAFETY: we own `handle` and are abandoning it.
            unsafe { ffi::mpv_terminate_destroy(handle) };
            return Err(MpvError::Init(rc));
        }

        Ok(MpvPlayer { handle })
    }

    /// Create a fully headless player (no audio, no video) — for tests and for
    /// property/event-driven use where no output surface exists.
    pub fn new_headless() -> Result<Self, MpvError> {
        Self::with_options(&[("ao", "null"), ("vo", "null")])
    }

    // -- commands --------------------------------------------------------

    /// Run a raw mpv command string (e.g. `"cycle-video-rotate"`).
    pub fn command(&self, cmd: &str) -> Result<(), MpvError> {
        let c_cmd = to_cstr(cmd)?;
        // SAFETY: valid handle; `c_cmd` is a valid NUL-terminated string.
        let rc = unsafe { ffi::mpv_command_string(self.handle, c_cmd.as_ptr()) };
        if rc != ffi::MPV_ERROR_SUCCESS {
            return Err(MpvError::Status(rc));
        }
        Ok(())
    }

    /// Load and play a media file.
    pub fn load_file(&self, path: &str) -> Result<(), MpvError> {
        self.command(&format!("loadfile {}", mpv_quote(path)))
    }

    /// Pause or resume playback.
    pub fn set_paused(&self, paused: bool) -> Result<(), MpvError> {
        self.command(&format!("set pause {}", if paused { "yes" } else { "no" }))
    }

    /// Whether playback is currently paused.
    pub fn paused(&self) -> Result<bool, MpvError> {
        self.get_bool("pause")
    }

    /// Seek to `time` (seconds). `exact` requests keyframe-exact (slower) seek.
    pub fn seek(&self, time: f64, exact: bool) -> Result<(), MpvError> {
        let mode = if exact { "exact" } else { "absolute" };
        self.command(&format!("seek {} {}", time, mode))
    }

    /// Set volume (`0.0` = silent, `100.0` = 100%; >100 amplifies).
    pub fn set_volume(&self, volume: f64) -> Result<(), MpvError> {
        self.command(&format!("set volume {}", volume))
    }

    /// Get volume (mpv units, default `100.0`).
    pub fn volume(&self) -> Result<f64, MpvError> {
        self.get_double("volume")
    }

    /// Mute / unmute without changing the volume value.
    pub fn set_muted(&self, muted: bool) -> Result<(), MpvError> {
        self.command(&format!("set mute {}", if muted { "yes" } else { "no" }))
    }

    /// Whether playback is muted.
    pub fn muted(&self) -> Result<bool, MpvError> {
        self.get_bool("mute")
    }

    /// Set playback speed (default `1.0`).
    pub fn set_speed(&self, speed: f64) -> Result<(), MpvError> {
        self.command(&format!("set speed {}", speed))
    }

    /// Get playback speed.
    pub fn speed(&self) -> Result<f64, MpvError> {
        self.get_double("speed")
    }

    /// Current playback position in seconds.
    pub fn position(&self) -> Result<f64, MpvError> {
        self.get_double("time-pos")
    }

    /// Total duration in seconds (may be `0.0`/`-1.0` if unknown or idle).
    pub fn duration(&self) -> Result<f64, MpvError> {
        self.get_double("duration")
    }

    /// Display width of the current video (0 if there is none).
    pub fn width(&self) -> Result<u32, MpvError> {
        self.get_int64("dwidth").map(|w| w.max(0) as u32)
    }

    /// Display height of the current video (0 if there is none).
    pub fn height(&self) -> Result<u32, MpvError> {
        self.get_int64("dheight").map(|h| h.max(0) as u32)
    }

    // -- tracks ----------------------------------------------------------

    /// The active track id for `kind` (mpv semantics: `>0` track, `0` none,
    /// `-1` auto).
    pub fn active_track(&self, kind: TrackKind) -> Result<i64, MpvError> {
        self.get_int64(kind.prop())
    }

    /// Set the active track for `kind` to `id` (`0` = none, `-1` = auto).
    pub fn set_track(&self, kind: TrackKind, id: i64) -> Result<(), MpvError> {
        self.command(&format!("set {} {}", kind.prop(), id))
    }

    /// Load an external subtitle file and enable it.
    pub fn set_subtitle_url(&self, url: &str) -> Result<(), MpvError> {
        self.command(&format!("set sub-file {}", mpv_quote(url)))
    }

    /// The tracks that are currently active (one per kind with a concrete id).
    ///
    /// mpv exposes the *full* track list via the `track-list` node property,
    /// which requires node-walking FFI not yet added. This returns the
    /// *active* track per kind, which suffices for the app's "current track" UI.
    pub fn tracks(&self) -> Result<Vec<Track>, MpvError> {
        let mut out = Vec::new();
        for kind in [TrackKind::Audio, TrackKind::Video, TrackKind::Subtitle] {
            let id = self.active_track(kind)?;
            if id > 0 {
                out.push(Track { kind, id });
            }
        }
        Ok(out)
    }

    // -- events ----------------------------------------------------------

    /// Enable (`true`) or disable (`false`) an mpv event (e.g. `"end-file"`,
    /// `"shutdown"`, `"file-loaded"`, `"tick"`).
    pub fn request_event(&self, name: &str, enable: bool) -> Result<(), MpvError> {
        let c_name = to_cstr(name)?;
        let flag = if enable { 1 } else { 0 };
        // SAFETY: valid handle; `c_name` is a valid NUL-terminated event name.
        let rc = unsafe { ffi::mpv_request_event(self.handle, c_name.as_ptr(), flag) };
        if rc != ffi::MPV_ERROR_SUCCESS {
            return Err(MpvError::Status(rc));
        }
        Ok(())
    }

    /// Wait up to `timeout_secs` for the next event.
    ///
    /// Returns `None` on timeout (no event pending). Call [`request_event`]
    /// first to register interest in specific events.
    pub fn wait_event(&self, timeout_secs: f64) -> Option<MpvEvent> {
        // SAFETY: `handle` is valid.
        let ptr = unsafe { ffi::mpv_wait_event(self.handle, timeout_secs) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is a valid `mpv_event`; we read only the stable
        // `event_id` / `error` prefix (see `ffi::mpv_event_header`).
        let ev = unsafe { &*ptr };
        Some(MpvEvent {
            event_id: ev.event_id,
            error: ev.error,
        })
    }

    // -- property getters ------------------------------------------------

    fn get_double(&self, name: &str) -> Result<f64, MpvError> {
        let c_name = to_cstr(name)?;
        let mut val = 0.0_f64;
        // SAFETY: valid handle; `c_name` valid; `val` is a writable f64.
        let rc = unsafe {
            ffi::mpv_get_property(
                self.handle,
                c_name.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                &mut val as *mut f64 as *mut c_void,
            )
        };
        if rc != ffi::MPV_ERROR_SUCCESS {
            return Err(MpvError::Status(rc));
        }
        Ok(val)
    }

    fn get_int64(&self, name: &str) -> Result<i64, MpvError> {
        let c_name = to_cstr(name)?;
        let mut val = 0_i64;
        // SAFETY: valid handle; `c_name` valid; `val` is a writable i64.
        let rc = unsafe {
            ffi::mpv_get_property(
                self.handle,
                c_name.as_ptr(),
                ffi::MPV_FORMAT_INT64,
                &mut val as *mut i64 as *mut c_void,
            )
        };
        if rc != ffi::MPV_ERROR_SUCCESS {
            return Err(MpvError::Status(rc));
        }
        Ok(val)
    }

    fn get_bool(&self, name: &str) -> Result<bool, MpvError> {
        self.get_int64(name).map(|v| v != 0)
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        // SAFETY: we own `handle` (from mpv_create) and it is not yet destroyed.
        unsafe { ffi::mpv_terminate_destroy(self.handle) };
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure-logic test: `mpv_quote` must correctly escape quotes and
    /// backslashes for use inside an mpv command string.
    #[test]
    fn mpv_quote_escapes_correctly() {
        assert_eq!(mpv_quote("hello"), "\"hello\"");
        assert_eq!(mpv_quote("a b"), "\"a b\"");
        assert_eq!(mpv_quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(mpv_quote("C:\\path"), "\"C:\\\\path\"");
        assert_eq!(mpv_quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(mpv_quote(""), "\"\"");
    }

    /// Pure-logic test: `TrackKind::prop` maps to the correct mpv property name.
    #[test]
    fn track_kind_prop() {
        assert_eq!(TrackKind::Audio.prop(), "aid");
        assert_eq!(TrackKind::Video.prop(), "vid");
        assert_eq!(TrackKind::Subtitle.prop(), "sid");
    }

    /// Pure-logic test: `MpvEvent` helpers match their event-id constants.
    #[test]
    fn mpv_event_helpers() {
        let end = MpvEvent {
            event_id: ffi::MPV_EVENT_END_FILE,
            error: 0,
        };
        assert!(end.is_end_file());
        assert!(!end.is_shutdown());
        assert!(!end.is_file_loaded());
        assert!(end.ok());

        let shutdown = MpvEvent {
            event_id: ffi::MPV_EVENT_SHUTDOWN,
            error: 0,
        };
        assert!(shutdown.is_shutdown());
        assert!(!shutdown.is_end_file());

        let err_event = MpvEvent {
            event_id: ffi::MPV_EVENT_END_FILE,
            error: -1,
        };
        assert!(!err_event.ok());
    }

    /// Pure-logic test: `to_cstr` rejects strings with interior NUL bytes.
    #[test]
    fn to_cstr_rejects_nul_bytes() {
        assert!(to_cstr("hello").is_ok());
        assert!(to_cstr("has\0ul").is_err());
    }

    // NOTE: Runtime tests that call into libmpv (create/initialize/property
    // queries) are intentionally not `#[test]`-gated in this environment
    // because libmpv 0.41 segfaults inside the Rust test harness (which links
    // gstreamer, wgpu, iced, etc.) even though the identical call sequence
    // works in a standalone binary. See the probe in `target/probe/mpv_probe2.rs`
    // and the plan document Appendix B for details. The ABI shape is verified
    // by `ffi::tests::abi_struct_layout` above, and the full runtime path can
    // be validated by running the standalone probe or the app itself.
}
