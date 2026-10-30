//! libmpv integration for media-browser.
//!
//! Layout:
//! - [`ffi`]: raw C bindings, verified against the installed libmpv 0.41
//!   headers (core client API + the **sw** render API + node walker).
//! - [`player`]: safe `MpvPlayer` wrapper (playback control, properties,
//!   tracks, event queue). Fully testable headless.
//! - [`render`]: sw-render worker thread that pulls frames from mpv into
//!   `bgr0` (BGRA) buffers, which the wgpu [`crate::pipeline`] uploads and
//!   composites. This is the "Vulkan route" of the migration plan: wgpu 28
//!   does the GPU compositing, mpv supplies CPU-rendered frames — no
//!   OpenGL/X11, no mpv Vulkan sub-API (which does not exist in 0.41).
//!
//! # Linking
//!
//! `build.rs` emits `cargo:rustc-link-lib=dylib=mpv` so the symbols resolve
//! against the system `libmpv` (Ubuntu `libmpv2` ≥ 0.41).

pub mod ffi;
pub mod player;
pub mod render;

// Re-export the safe API at the module root for convenient access.
pub use player::{LoadOutcome, MpvEvent, MpvPlayer, Track, TrackKind, language_name};

// Re-export the error type and a curated set of the FFI surface (handles,
// structs, and the event/status constants most often needed by callers).
pub use ffi::{
    MPV_ERROR_SUCCESS, MPV_EVENT_END_FILE, MPV_EVENT_FILE_LOADED, MPV_EVENT_SHUTDOWN,
    MPV_EVENT_START_FILE, MpvError, mpv_event, mpv_handle, mpv_node, mpv_render_context,
    mpv_render_param,
};
