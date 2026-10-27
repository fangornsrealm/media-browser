//! libmpv integration for media-browser.
//!
//! This module provides a hand-written FFI binding to the libmpv *client*
//! API ([`ffi`]) and a safe, ergonomic wrapper over it ([`player`]).
//!
//! It is the **unblocked** portion of the libmpv migration described in
//! `docs/PLAN-libcosmic-1.0-wgpu-28-libmpv.md` (§7.1 / §7.2): it covers
//! playback control, properties, tracks, and the event loop, and is fully
//! testable headless against the installed libmpv (v0.41.0).
//!
//! # What is deliberately excluded
//!
//! The **render-context path** (mpv's Vulkan / OpenGL sub-API, plan §7.3) is
//! **blocked by R1**: wgpu 28 (shipped by libcosmic 1.0) exposes no public
//! API to hand mpv a `VkInstance` / `VkDevice` / `VkQueue`, and no native
//! way to import mpv's rendered `VkImage` into a wgpu texture. The FFI
//! declarations for that path are present in [`ffi`] so the surface stays
//! complete, but the safe wrapper here intentionally does not drive it. See
//! the plan's Appendix B (Q2) for details and alternatives.
//!
//! # Linking
//!
//! `build.rs` must emit `cargo:rustc-link-lib=dylib=mpv` for native targets
//! so the symbols declared in [`ffi`] resolve at link time.
//!
//! # Feature gating
//!
//! Per plan §3.2 this module can be gated behind an `mpv` feature flag. Until
//! that feature is added to `Cargo.toml` (and `lib.rs` opts in via
//! `#[cfg(feature = "mpv")] mod mpv;`), this module is compiled
//! unconditionally.

pub mod ffi;
pub mod player;

// Re-export the safe API at the module root for convenient access.
pub use player::{MpvError, MpvEvent, MpvPlayer, Track, TrackKind};

// Re-export a curated set of the FFI surface (handles, structs, and the
// event/status constants most often needed by callers).
pub use ffi::{
    mpv_event, mpv_handle, mpv_node, mpv_render_context, mpv_render_param, MPV_ERROR_SUCCESS,
    MPV_EVENT_END_FILE, MPV_EVENT_FILE_LOADED, MPV_EVENT_SHUTDOWN,
};
