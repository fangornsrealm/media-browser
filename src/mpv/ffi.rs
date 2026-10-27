//! Hand-written FFI bindings for the libmpv client API.
//!
//! No `bindgen`, no external crate. The mpv *client* ABI has been stable
//! since mpv 0.17 (2018); we target libmpv v0.41.0 (pre-flight, Appendix B).
//!
//! # Scope
//!
//! This module declares the **core client API** (lifecycle, options,
//! commands, properties, events, memory). That surface is fully usable and is
//! exercised by the safe wrapper in [`super::player`].
//!
//! It also declares the **render-context API** (`mpv_render_context_*`) and
//! the Vulkan render-parameter constants, kept so the FFI surface is complete
//! and forward-compatible.
//!
//! ⚠️ **BLOCKED (R1):** The mpv Vulkan sub-API (`MPV_RENDER_PARAM_VULKAN_*` +
//! `mpv_render_context_*`) requires the application to own and hand over a
//! `VkInstance` / `VkPhysicalDevice` / `VkDevice` / `VkQueue`. wgpu 28 (the
//! renderer libcosmic 1.0 ships) exposes **none** of those handles through
//! its safe public API, and there is no native path to import an mpv-rendered
//! `VkImage` into a wgpu texture. **Do not call the `mpv_render_context_*`
//! functions** until that blocker is resolved — see
//! `docs/PLAN-libcosmic-1.0-wgpu-28-libmpv.md`, Appendix B (Q2).
//!
//! # Soundness notes
//!
//! * All handles are opaque (`[u8; 0]`); we never dereference them directly.
//! * `mpv_event` is larger than its two readable prefix fields; we expose
//!   [`mpv_event_header`] (the stable `int event_id; int error;` prefix) and
//!   only ever read those two, which is sound because they are a valid C
//!   prefix of the real struct.
//! * Strings returned by `mpv_*_string` APIs must be released with
//!   [`mpv_free`]; strings passed *in* must outlive the call.

#![allow(non_camel_case_types, non_upper_case_globals)]

use std::os::raw::{c_char, c_int, c_void};

// ---------------------------------------------------------------------------
// Opaque types
// ---------------------------------------------------------------------------

/// libmpv handle. Never dereferenced directly.
#[repr(C)]
#[derive(Debug)]
pub struct mpv_handle {
    _private: [u8; 0],
}

/// Safe, readable prefix of the C `mpv_event` struct.
///
/// The real struct is:
/// ```c
/// typedef struct mpv_event {
///     int event_id;  // enum mpv_event_id
///     int error;     // MPV_ERROR_* status (0 on success)
///     union { ... } data;  // 64-byte payload
/// } mpv_event;
/// ```
/// Because the first two `int`s are stable and always present, casting
/// `*const mpv_event` to `*const mpv_event_header` to read them is sound.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct mpv_event_header {
    /// `enum mpv_event_id` (see `MPV_EVENT_*` below).
    pub event_id: c_int,
    /// `MPV_ERROR_*` status code; `MPV_ERROR_SUCCESS` (0) on success.
    pub error: c_int,
}

/// Alias so call sites can write `*const mpv_event`.
pub type mpv_event = mpv_event_header;

/// libmpv render context (Vulkan / OpenGL sub-API). See the R1 note above.
#[repr(C)]
#[derive(Debug)]
pub struct mpv_render_context {
    _private: [u8; 0],
}

// ---------------------------------------------------------------------------
// mpv_format — property data format (`client.h`)
// ---------------------------------------------------------------------------
pub const MPV_FORMAT_NONE: c_int = 0;
pub const MPV_FORMAT_STRING: c_int = 1;
pub const MPV_FORMAT_NODE_MAP: c_int = 2;
pub const MPV_FORMAT_NODE_ARRAY: c_int = 3;
pub const MPV_FORMAT_NODE: c_int = 4; // deprecated
pub const MPV_FORMAT_INT64: c_int = 5;
pub const MPV_FORMAT_DOUBLE: c_int = 6;
pub const MPV_FORMAT_NODE_NUM: c_int = 7;
pub const MPV_FORMAT_NODE_BOOL: c_int = 8;
pub const MPV_FORMAT_DATA: c_int = 9;
pub const MPV_FORMAT_OSD_STRING: c_int = 10;

// ---------------------------------------------------------------------------
// mpv_event_id (`client.h`)
// ---------------------------------------------------------------------------
pub const MPV_EVENT_UNKNOWN: c_int = 0;
pub const MPV_EVENT_SHUTDOWN: c_int = 1;
pub const MPV_EVENT_CLIENT_MESSAGE: c_int = 2;
pub const MPV_EVENT_LOG_MESSAGE: c_int = 3;
pub const MPV_EVENT_GET_PROPERTY: c_int = 4;
pub const MPV_EVENT_SET_PROPERTY: c_int = 5;
pub const MPV_EVENT_COMMAND: c_int = 6;
pub const MPV_EVENT_VIDEO_FRAMEDROP: c_int = 7;
pub const MPV_EVENT_END_FILE: c_int = 8;
pub const MPV_EVENT_FILE_LOADED: c_int = 9;
pub const MPV_EVENT_TRACKS_CHANGED: c_int = 10;
pub const MPV_EVENT_TRACKS_UPDATED: c_int = 11;
pub const MPV_EVENT_PLAYBACK_RESTART: c_int = 12;
pub const MPV_EVENT_META_CHANGED: c_int = 13;
pub const MPV_EVENT_SEEKER: c_int = 14;
pub const MPV_EVENT_PROPERTY_CHANGE: c_int = 15;
pub const MPV_EVENT_IDLE: c_int = 16;
pub const MPV_EVENT_MARKER: c_int = 17;
pub const MPV_EVENT_HOOK: c_int = 18;
pub const MPV_EVENT_COMMAND_REPLY: c_int = 19;

// ---------------------------------------------------------------------------
// mpv_render_param_type (`render.h`)
// NOTE: consumed as `uint64_t` via `mpv_render_param::r#type`.
// ---------------------------------------------------------------------------
pub const MPV_RENDER_PARAM_INVALID: u64 = 0;
pub const MPV_RENDER_PARAM_API_TYPE: u64 = 1;
pub const MPV_RENDER_PARAM_W: u64 = 2;
pub const MPV_RENDER_PARAM_H: u64 = 3;
pub const MPV_RENDER_PARAM_WL_EGL_SURFACE: u64 = 4;
pub const MPV_RENDER_PARAM_HYBRIDIZATION: u64 = 5;
pub const MPV_RENDER_PARAM_OPENGL_CONTEXT: u64 = 6;
pub const MPV_RENDER_PARAM_OSMESA_BUFFER: u64 = 7;
pub const MPV_RENDER_PARAM_VULKAN_INSTANCE: u64 = 8;
pub const MPV_RENDER_PARAM_VULKAN_PHYSICAL_DEVICE: u64 = 9;
pub const MPV_RENDER_PARAM_VULKAN_DEVICE: u64 = 10;
pub const MPV_RENDER_PARAM_VULKAN_QUEUE_FAMILY_INDEX: u64 = 11;
pub const MPV_RENDER_PARAM_VULKAN_QUEUE: u64 = 12;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_WIDTH: u64 = 13;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_HEIGHT: u64 = 14;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_IMAGE_COUNT: u64 = 15;
pub const MPV_RENDER_PARAM_VULKAN_SWAPCHAIN_IMAGE: u64 = 16;
pub const MPV_RENDER_PARAM_LUID: u64 = 17;
pub const MPV_RENDER_PARAM_D3D11_DEVICE: u64 = 18;
pub const MPV_RENDER_PARAM_D3D11_DEVICE_CHILD: u64 = 19;
pub const MPV_RENDER_PARAM_D3D11_SWAPCHAIN: u64 = 20;
pub const MPV_RENDER_PARAM_D3D11_SWAPCHAIN_LUID: u64 = 21;
pub const MPV_RENDER_PARAM_D3D12_DEVICE: u64 = 22;
pub const MPV_RENDER_PARAM_D3D12_COMMAND_QUEUE: u64 = 23;
pub const MPV_RENDER_PARAM_D3D12_SWAPCHAIN: u64 = 24;
pub const MPV_RENDER_PARAM_D3D12_SWAPCHAIN_COMMAND_QUEUE: u64 = 25;

// ---------------------------------------------------------------------------
// MPV_ERROR_* status codes (subset, `client.h`)
// ---------------------------------------------------------------------------
pub const MPV_ERROR_SUCCESS: c_int = 0;
pub const MPV_ERROR_UNINITIALIZED: c_int = -1;
pub const MPV_ERROR_EVENT: c_int = -2;
pub const MPV_ERROR_NOMEM: c_int = -3;
pub const MPV_ERROR_VALUE: c_int = -4;
pub const MPV_ERROR_INVALID_PARAM: c_int = -5;
pub const MPV_ERROR_LOADING_FAILED: c_int = -6;
pub const MPV_ERROR_GENERIC: c_int = -100;

// ---------------------------------------------------------------------------
// Structs passed by value
// ---------------------------------------------------------------------------

/// One `mpv_render_param` entry for the `mpv_render_context_*` APIs.
///
/// `r#type` is an `mpv_render_param_type` value (a `u64`); `data` points at
/// the value that type expects (e.g. a `VkInstance` handle for
/// `MPV_RENDER_PARAM_VULKAN_INSTANCE`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mpv_render_param {
    pub r#type: u64,
    pub data: *const c_void,
}

/// A structured `mpv_node` (used by `mpv_get_property` for e.g. `track-list`).
///
/// `format` is an `mpv_format` value; `data` is the payload (a pointer for
/// node/string/data formats, an inline value for int64/double).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mpv_node {
    pub format: c_int,
    pub data: *mut c_void,
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------
unsafe extern "C" {
    // -- lifecycle -------------------------------------------------------
    pub fn mpv_create() -> *mut mpv_handle;
    pub fn mpv_terminate_destroy(handle: *mut mpv_handle);
    pub fn mpv_initialize(handle: *mut mpv_handle) -> c_int;

    // -- options & commands ---------------------------------------------
    pub fn mpv_set_option_string(
        handle: *mut mpv_handle,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;
    pub fn mpv_command_string(handle: *mut mpv_handle, command: *const c_char) -> c_int;

    // -- properties ------------------------------------------------------
    pub fn mpv_get_property(
        handle: *mut mpv_handle,
        name: *const c_char,
        format: c_int,
        data: *mut c_void,
    ) -> c_int;
    pub fn mpv_get_property_string(handle: *mut mpv_handle, name: *const c_char) -> *mut c_char;

    // -- events ----------------------------------------------------------
    pub fn mpv_request_event(handle: *mut mpv_handle, event: *const c_char, flag: c_int) -> c_int;
    pub fn mpv_wait_event(handle: *mut mpv_handle, timeout: f64) -> *const mpv_event;

    // -- memory ----------------------------------------------------------
    pub fn mpv_free(handle: *mut mpv_handle, data: *mut c_void);
    pub fn mpv_free_node(handle: *mut mpv_handle, data: *mut c_void);
    pub fn mpv_get_error_string(error: c_int) -> *const c_char;

    // -- render context (Vulkan / OpenGL sub-API) — BLOCKED, see R1 ------
    pub fn mpv_render_context_create(
        handle: *mut mpv_handle,
        rc: *mut *mut mpv_render_context,
        api: *const c_char,
        params: *const mpv_render_param,
    ) -> c_int;
    pub fn mpv_render_context_update(rc: *mut mpv_render_context);
    pub fn mpv_render_context_render(rc: *mut mpv_render_context, params: *const mpv_render_param);
    pub fn mpv_render_context_report_swap(rc: *mut mpv_render_context);
    pub fn mpv_render_context_free(handle: *mut mpv_handle, rc: *mut mpv_render_context);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ABI sanity test: verifies the **shape** of the structs we pass to and
    /// from libmpv (sizes, alignment, and field ordering) **without calling
    /// into libmpv at all**. This is safe in any environment and catches the
    /// most common FFI bugs (wrong `#[repr(C)]`, wrong field types, wrong
    /// field order) that would otherwise corrupt memory at runtime.
    #[test]
    fn abi_struct_layout() {
        // `mpv_render_param` is `struct { uint64_t type; void *data; }`.
        // 64-bit: 8 (type) + 8 (ptr) = 16 bytes, aligned to 8.
        assert_eq!(std::mem::size_of::<mpv_render_param>(), 16);
        assert_eq!(std::mem::align_of::<mpv_render_param>(), 8);

        // `mpv_node` is `struct { int format; union {...} data; }`.
        // 64-bit: 4 (format) + 4 (padding) + 8 (ptr) = 16 bytes.
        assert_eq!(std::mem::size_of::<mpv_node>(), 16);
        assert_eq!(
            std::mem::offset_of!(mpv_node, format),
            0,
            "mpv_node::format must be the first field"
        );
        assert_eq!(
            std::mem::offset_of!(mpv_node, data),
            8,
            "mpv_node::data must follow the 4-byte format + 4-byte pad"
        );

        // The readable `mpv_event` prefix is two leading `int`s (32-bit each)
        // at offset 0 and 4.
        assert_eq!(std::mem::size_of::<mpv_event_header>(), 8);
        assert_eq!(std::mem::offset_of!(mpv_event_header, event_id), 0);
        assert_eq!(std::mem::offset_of!(mpv_event_header, error), 4);

        // Opaque handles must be zero-sized so they behave like C pointers.
        assert_eq!(std::mem::size_of::<mpv_handle>(), 0);
        assert_eq!(std::mem::size_of::<mpv_render_context>(), 0);
    }
}
