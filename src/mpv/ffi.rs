//! Raw FFI bindings for libmpv.
//!
//! Verified against the installed libmpv 0.41.0 headers
//! (`/usr/include/mpv/client.h`, `/usr/include/mpv/render.h`) and a C probe
//! binary that links and calls the same symbols.
//!
//! Scope:
//! - core client API (create / options / command / properties / events),
//! - the **sw** render API (CPU render into an in-memory RGBA buffer —
//!   the only render backend usable without a GPU context; the Vulkan
//!   sub-API does not exist in mpv 0.41),
//! - safe typed wrappers over `mpv_node` (track lists, track info).
//!
//! Threading rules (from `render.h`):
//! - render calls (`mpv_render_*`) must be made from a dedicated thread,
//! - `mpv_render_*` must **never** be called from inside the update
//!   callback,
//! - only one `mpv_render_*` call at a time per render context.
//!
//! Edition 2024 makes `unsafe_op_in_unsafe_fn` deny-by-default; these
//! helpers are already `unsafe fn` and merely forward to other FFI calls,
//! so the legacy style is restored explicitly.
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

// ---------------------------------------------------------------------------
// mpv_format (client.h)
// ---------------------------------------------------------------------------

pub const MPV_FORMAT_NONE: c_int = 0;
pub const MPV_FORMAT_STRING: c_int = 1;
pub const MPV_FORMAT_OSD_STRING: c_int = 2;
pub const MPV_FORMAT_FLAG: c_int = 3;
pub const MPV_FORMAT_INT64: c_int = 4;
pub const MPV_FORMAT_DOUBLE: c_int = 5;

// ---------------------------------------------------------------------------
// mpv_event_id (client.h)
// ---------------------------------------------------------------------------

pub const MPV_EVENT_NONE: c_int = 0;
pub const MPV_EVENT_SHUTDOWN: c_int = 1;
pub const MPV_EVENT_LOG_MESSAGE: c_int = 2;
pub const MPV_EVENT_GET_PROPERTY_REPLY: c_int = 3;
pub const MPV_EVENT_SET_PROPERTY_REPLY: c_int = 4;
pub const MPV_EVENT_COMMAND_REPLY: c_int = 5;
pub const MPV_EVENT_START_FILE: c_int = 6;
pub const MPV_EVENT_END_FILE: c_int = 7;
pub const MPV_EVENT_FILE_LOADED: c_int = 8;
pub const MPV_EVENT_IDLE: c_int = 11;
pub const MPV_EVENT_TICK: c_int = 14;
pub const MPV_EVENT_CLIENT_MESSAGE: c_int = 16;
pub const MPV_EVENT_VIDEO_RECONFIG: c_int = 17;
pub const MPV_EVENT_AUDIO_RECONFIG: c_int = 18;
pub const MPV_EVENT_SEEK: c_int = 20;
pub const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
pub const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;
pub const MPV_EVENT_QUEUE_OVERFLOW: c_int = 24;
pub const MPV_EVENT_HOOK: c_int = 25;

// ---------------------------------------------------------------------------
// Events (client.h)
// ---------------------------------------------------------------------------

/// Event union header. Layout (verified): `{ event_id, error, reply_userdata, data }`.
#[repr(C)]
pub struct mpv_event {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

/// `MPV_EVENT_END_FILE` payload.
#[repr(C)]
pub struct mpv_event_end_file {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: i64,
}

/// END_FILE reasons (client.h).
pub const MPV_END_FILE_REASON_EOF: c_int = 0;
pub const MPV_END_FILE_REASON_STOP: c_int = 2;
pub const MPV_END_FILE_REASON_QUIT: c_int = 3;
pub const MPV_END_FILE_REASON_ERROR: c_int = 4;
pub const MPV_END_FILE_REASON_REDIRECT: c_int = 5;

/// `MPV_EVENT_LOG_MESSAGE` payload.
#[repr(C)]
pub struct mpv_event_log_message {
    pub level: *const c_char,
    pub prefix: *const c_char,
    pub text: *const c_char,
}

/// `MPV_EVENT_PROPERTY_CHANGE` payload.
#[repr(C)]
pub struct mpv_event_property {
    pub name: *mut c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

// ---------------------------------------------------------------------------
// mpv_node (client.h)
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct mpv_node {
    pub u: mpv_node_u,
    pub format: c_int,
}

#[repr(C)]
pub union mpv_node_u {
    pub string: *mut c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double_: f64,
    pub list: *mut mpv_node_list,
    pub ba: *mut mpv_byte_array,
}

#[repr(C)]
pub struct mpv_node_list {
    pub num: c_int,
    pub values: *mut mpv_node,
    pub keys: *mut *mut c_char,
}

#[repr(C)]
pub struct mpv_byte_array {
    pub data: *mut c_void,
    pub size: usize,
}

// ---------------------------------------------------------------------------
// Render API (render.h) — `sw` backend only
// ---------------------------------------------------------------------------

/// `mpv_render_param` pair. Layout (verified): `{ int type; void *data; }`.
#[repr(C)]
pub struct mpv_render_param {
    pub r#type: c_int,
    pub data: *mut c_void,
}

/// `MPV_RENDER_PARAM_INVALID` — also the terminator for param arrays.
pub const MPV_RENDER_PARAM_INVALID: c_int = 0;
/// API selector string, e.g. `"sw"`.
pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
/// `sw` only: target surface size `{ w, h }` (`mpv_render_size`).
pub const MPV_RENDER_PARAM_SW_SIZE: c_int = 17;
/// `sw` only: format string, e.g. `"bgr0"`.
pub const MPV_RENDER_PARAM_SW_FORMAT: c_int = 18;
/// `sw` only: bytes per line (`size_t`).
pub const MPV_RENDER_PARAM_SW_STRIDE: c_int = 19;
/// `sw` only: pixel data pointer (`void *`).
pub const MPV_RENDER_PARAM_SW_POINTER: c_int = 20;

/// Size passed via `MPV_RENDER_PARAM_SW_SIZE`.
#[repr(C)]
pub struct mpv_render_size {
    pub w: c_int,
    pub h: c_int,
}

/// Update callback signature: `void (*fn)(void *context)`.
pub type MpvRenderUpdateFn = Option<unsafe extern "C" fn(*mut c_void)>;

/// Update info returned by `mpv_render_context_update`.
#[repr(C)]
pub struct mpv_render_update_info {
    pub frame_id: u64,
}

// ---------------------------------------------------------------------------
// mpv_error (client.h)
// ---------------------------------------------------------------------------

pub const MPV_ERROR_SUCCESS: c_int = 0;
pub const MPV_ERROR_EVENT_QUEUE_FULL: c_int = -1;
pub const MPV_ERROR_NOMEM: c_int = -2;
pub const MPV_ERROR_UNINITIALIZED: c_int = -3;
pub const MPV_ERROR_INVALID_PARAMETER: c_int = -4;
pub const MPV_ERROR_OPTION_NOT_FOUND: c_int = -5;
pub const MPV_ERROR_OPTION_FORMAT: c_int = -6;
pub const MPV_ERROR_OPTION_ERROR: c_int = -7;
pub const MPV_ERROR_PROPERTY_NOT_FOUND: c_int = -8;
pub const MPV_ERROR_PROPERTY_FORMAT: c_int = -9;
pub const MPV_ERROR_PROPERTY_UNAVAILABLE: c_int = -10;
pub const MPV_ERROR_PROPERTY_ERROR: c_int = -11;
pub const MPV_ERROR_COMMAND: c_int = -12;
pub const MPV_ERROR_LOADING_FAILED: c_int = -13;
pub const MPV_ERROR_AO_INIT_FAILED: c_int = -14;
pub const MPV_ERROR_VO_INIT_FAILED: c_int = -15;
pub const MPV_ERROR_NOTHING_TO_PLAY: c_int = -16;
pub const MPV_ERROR_UNKNOWN_FORMAT: c_int = -17;
pub const MPV_ERROR_UNSUPPORTED: c_int = -18;
pub const MPV_ERROR_NOT_IMPLEMENTED: c_int = -19;
pub const MPV_ERROR_GENERIC: c_int = -20;

// ---------------------------------------------------------------------------
// Function declarations (client.h, render.h)
// ---------------------------------------------------------------------------

unsafe extern "C" {
    pub fn mpv_client_api_version() -> u64;
    pub fn mpv_create() -> *mut mpv_handle;
    pub fn mpv_terminate_destroy(handle: *mut mpv_handle);
    pub fn mpv_initialize(handle: *mut mpv_handle) -> c_int;
    pub fn mpv_set_option_string(
        handle: *mut mpv_handle,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn mpv_set_option(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *const c_void,
    ) -> c_int;
    pub fn mpv_command_string(handle: *mut mpv_handle, args: *const c_char) -> c_int;
    pub fn mpv_command_node(handle: *mut mpv_handle, args: *const mpv_node) -> c_int;
    pub fn mpv_set_property(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *const c_void,
    ) -> c_int;
    pub fn mpv_set_property_string(
        handle: *mut mpv_handle,
        name: *const c_char,
        data: *const c_char,
    ) -> c_int;
    pub fn mpv_get_property(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    pub fn mpv_get_property_string(handle: *mut mpv_handle, name: *const c_char) -> *mut c_char;
    /// Note: this build of libmpv takes the event **id** (enum), not a name.
    pub fn mpv_request_event(handle: *mut mpv_handle, event: c_int, enable: c_int) -> c_int;
    pub fn mpv_request_log_messages(handle: *mut mpv_handle, min_level: *const c_char) -> c_int;
    pub fn mpv_wait_event(handle: *mut mpv_handle, timeout: f64) -> *mut mpv_event;
    pub fn mpv_wakeup(handle: *mut mpv_handle);
    /// One-argument `mpv_free(void *data)` in this build.
    pub fn mpv_free(data: *mut c_void);
    pub fn mpv_observe_property(
        handle: *mut mpv_handle,
        reply_userdata: u64,
        name: *const c_char,
        format: c_int,
    ) -> c_int;
    pub fn mpv_unobserve_property(handle: *mut mpv_handle, registered_reply_userdata: u64)
    -> c_int;
    pub fn mpv_set_wakeup_callback(
        handle: *mut mpv_handle,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        context: *mut c_void,
    );
    pub fn mpv_get_property_osd_string(handle: *mut mpv_handle, name: *const c_char)
    -> *mut c_char;
    /// `args` is a NUL-terminated array of `char *` (i.e. `const char **`).
    pub fn mpv_command_ret(
        handle: *mut mpv_handle,
        args: *const *const c_char,
        result: *mut *mut mpv_node,
    ) -> c_int;
    pub fn mpv_error_string(error: c_int) -> *const c_char;

    // --- render API (sw backend) ---
    pub fn mpv_render_context_create(
        rc: *mut *mut mpv_render_context,
        handle: *mut mpv_handle,
        params: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_set_parameter(
        ctx: *mut mpv_render_context,
        param: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_get_info(
        ctx: *mut mpv_render_context,
        param: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_set_update_callback(
        ctx: *mut mpv_render_context,
        callback: MpvRenderUpdateFn,
        callback_ctx: *mut c_void,
    );
    pub fn mpv_render_context_update(ctx: *mut mpv_render_context) -> u64;
    pub fn mpv_render_context_render(
        ctx: *mut mpv_render_context,
        params: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_report_swap(ctx: *mut mpv_render_context);
    pub fn mpv_render_context_free(ctx: *mut mpv_render_context);
}

/// Opaque handle for the mpv core.
#[repr(C)]
#[allow(improper_ctypes_definitions)]
pub struct mpv_handle {
    _private: [u8; 0],
}

/// Opaque handle for a render context.
#[repr(C)]
#[allow(improper_ctypes_definitions)]
pub struct mpv_render_context {
    _private: [u8; 0],
}

/// Error wrapper around an `mpv_error` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpvError(pub c_int);

impl std::fmt::Display for MpvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = unsafe {
            let p = mpv_error_string(self.0);
            if p.is_null() {
                return write!(f, "mpv error {}", self.0);
            }
            CStr::from_ptr(p).to_string_lossy()
        };
        write!(f, "{s} ({})", self.0)
    }
}

impl std::error::Error for MpvError {}

/// Convert an `mpv_error` code into a `Result`.
pub fn result_of(code: c_int) -> Result<(), MpvError> {
    if code < 0 {
        Err(MpvError(code))
    } else {
        Ok(())
    }
}

/// Copy a NUL-terminated `char *` owned by mpv into a Rust `String`.
pub unsafe fn from_raw(s: *const c_char) -> Option<String> {
    if s.is_null() {
        return None;
    }
    Some(CStr::from_ptr(s).to_string_lossy().into_owned())
}

/// Convenience: `get_property` for string properties.
pub unsafe fn get_property_string(
    handle: *mut mpv_handle,
    name: &str,
) -> Result<Option<String>, MpvError> {
    let cname = CString::new(name).map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    let p = mpv_get_property_string(handle, cname.as_ptr());
    if p.is_null() {
        return Ok(None);
    }
    let s = from_raw(p).unwrap_or_default();
    mpv_free(p as *mut c_void);
    Ok(Some(s))
}

/// Convenience: `set_property` for string properties.
pub unsafe fn set_property_string(
    handle: *mut mpv_handle,
    name: &str,
    value: &str,
) -> Result<(), MpvError> {
    let cname = CString::new(name).map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    let cvalue = CString::new(value).map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    result_of(mpv_set_property_string(
        handle,
        cname.as_ptr(),
        cvalue.as_ptr(),
    ))
}

/// Convenience: `command` for a single string argument.
pub unsafe fn command_string(handle: *mut mpv_handle, cmd: &str) -> Result<(), MpvError> {
    let ccmd = CString::new(cmd).map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    result_of(mpv_command_string(handle, ccmd.as_ptr()))
}

// ---------------------------------------------------------------------------
// Track info (from the `track-list` property)
// ---------------------------------------------------------------------------

/// A single entry of mpv's `track-list` property.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: i64,
    pub title: Option<String>,
    pub language: Option<String>,
    /// `type` of the track, e.g. `"video"`, `"audio"`, `"sub"`, `"other"`.
    pub kind: String,
    pub selected: bool,
}

/// The `track-list` property as parsed `Track`s.
///
/// Retries a few times: during core state transitions (load start,
/// track switch) mpv can briefly report the property as unavailable.
pub unsafe fn track_list(handle: *mut mpv_handle) -> Result<Vec<Track>, MpvError> {
    let cname = CString::new("track-list").map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    // Retry a few times: during core state transitions (load start, track
    // switch) the property can be briefly unavailable or its string form
    // briefly malformed.
    let mut last_err = MpvError(MPV_ERROR_PROPERTY_UNAVAILABLE);
    let mut backoff = 10u64;
    for _ in 0..4 {
        let ptr = mpv_get_property_string(handle, cname.as_ptr());
        if !ptr.is_null() {
            let json = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            mpv_free(ptr as *mut c_void);
            match parse_tracks(&json) {
                Ok(v) => return Ok(v),
                Err(e) => last_err = e,
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(backoff));
        backoff = (backoff * 2).min(50);
    }
    Err(last_err)
}

/// Parse a `track-list` JSON array (mpv's string encoding of the property)
/// into `Track`s. Pure and testable without a core.
pub fn parse_tracks(json: &str) -> Result<Vec<Track>, MpvError> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|_| MpvError(MPV_ERROR_INVALID_PARAMETER))?;
    Ok(arr
        .into_iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_i64()?;
            Some(Track {
                id,
                title: v.get("title").and_then(|t| t.as_str()).map(str::to_string),
                language: v.get("lang").and_then(|t| t.as_str()).map(str::to_string),
                kind: v
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("other")
                    .to_string(),
                selected: v.get("selected").and_then(|s| s.as_bool()).unwrap_or(false),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tests (pure layout checks — safe to run without a running core)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_layout_matches_c_abi() {
        // { int event_id; int error; uint64 reply_userdata; void* data; }
        assert_eq!(std::mem::size_of::<mpv_event>(), 24);
        assert_eq!(std::mem::offset_of!(mpv_event, event_id), 0);
        assert_eq!(std::mem::offset_of!(mpv_event, error), 4);
        assert_eq!(std::mem::offset_of!(mpv_event, reply_userdata), 8);
        assert_eq!(std::mem::offset_of!(mpv_event, data), 16);
    }

    #[test]
    fn end_file_layout_matches_c_abi() {
        // { int reason; int error; int64 playlist_entry_id; }
        assert_eq!(std::mem::size_of::<mpv_event_end_file>(), 16);
        assert_eq!(std::mem::offset_of!(mpv_event_end_file, reason), 0);
        assert_eq!(std::mem::offset_of!(mpv_event_end_file, error), 4);
        assert_eq!(
            std::mem::offset_of!(mpv_event_end_file, playlist_entry_id),
            8
        );
    }

    #[test]
    fn render_param_layout_matches_c_abi() {
        // { int type; void* data; }
        assert_eq!(std::mem::size_of::<mpv_render_param>(), 16);
        assert_eq!(std::mem::offset_of!(mpv_render_param, r#type), 0);
        assert_eq!(std::mem::offset_of!(mpv_render_param, data), 8);
    }

    #[test]
    fn sw_format_constants() {
        assert_eq!(MPV_RENDER_PARAM_SW_SIZE, 17);
        assert_eq!(MPV_RENDER_PARAM_SW_FORMAT, 18);
        assert_eq!(MPV_RENDER_PARAM_SW_STRIDE, 19);
        assert_eq!(MPV_RENDER_PARAM_SW_POINTER, 20);
    }

    #[test]
    fn result_of_maps_errors() {
        assert!(result_of(0).is_ok());
        assert!(result_of(MPV_ERROR_SUCCESS).is_ok());
        let err = result_of(MPV_ERROR_NOMEM).unwrap_err();
        assert_eq!(err.0, MPV_ERROR_NOMEM);
        // The human-readable prefix depends on the libmpv build; only assert
        // that the code round-trips into the message.
        assert!(err.to_string().contains("(-2)"));
    }

    #[test]
    fn parse_tracks_reads_mpv_json() {
        // Shape as returned by this libmpv for a file with one audio track.
        let json = r#"[{"id":1,"type":"audio","audio-channels":1,"selected":true,"main-selection":0,"decoder":"mp3float","codec":"mp3"}]"#;
        let tracks = parse_tracks(json).expect("parse");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, 1);
        assert_eq!(tracks[0].kind, "audio");
        assert!(tracks[0].selected);
        assert_eq!(tracks[0].title, None);
        assert_eq!(tracks[0].language, None);
    }

    #[test]
    fn parse_tracks_full_entry_and_missing_fields() {
        let json = r#"[
            {"id":2,"type":"video","title":"Main","lang":"eng","selected":false},
            {"id":3,"type":"sub","selected":true},
            {"no":"id"}
        ]"#;
        let tracks = parse_tracks(json).expect("parse");
        assert_eq!(tracks.len(), 2, "entries without an id are skipped");
        assert_eq!(tracks[0].id, 2);
        assert_eq!(tracks[0].kind, "video");
        assert_eq!(tracks[0].title.as_deref(), Some("Main"));
        assert_eq!(tracks[0].language.as_deref(), Some("eng"));
        assert!(!tracks[0].selected);
        assert_eq!(tracks[1].kind, "sub");
        assert!(tracks[1].selected);
    }

    #[test]
    fn parse_tracks_rejects_non_json() {
        assert!(parse_tracks("not json").is_err());
        assert!(parse_tracks("{\"not\":\"an array\"}").is_err());
    }
}
