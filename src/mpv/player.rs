//! Safe Rust wrapper around the libmpv client API (see [`super::ffi`]).
//!
//! This module drives decode / playback / properties / events through the
//! mpv client ABI. It is fully testable headless (no display, no GPU):
//! properties and the event queue work without any output driver, which the
//! unit tests below rely on.
//!
//! # Units
//!
//! Values use mpv's native units:
//! * `volume`: `0.0` (silent) … `100.0` (100%); above `100.0` amplifies.
//! * `time_pos` / `duration` / `speed` / seek amounts: seconds (`f64`).
//!
//! The video/audio models map these onto the app's `0.0..=1.0` volume
//! scale.
//!
//! # Threading
//!
//! mpv's client API is thread-safe (except [`MpvPlayer::next_event`], which
//! must be driven from a single thread per handle, and the render context,
//! which has its own thread rules — see [`super::render`]). [`MpvPlayer`]
//! is therefore `Send + Sync`.

use std::ffi::CString;

use super::ffi::{self, MpvError, mpv_handle};

// ---------------------------------------------------------------------------
// Track types
// ---------------------------------------------------------------------------

/// A media track kind, mirroring mpv's `track-list` `type` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Video,
    Audio,
    Sub,
    Other,
}

impl TrackKind {
    /// Decode mpv's `type` string (`"video"`, `"audio"`, `"sub"`, `"other"`).
    pub fn from_mpv(s: &str) -> Self {
        match s {
            "video" => Self::Video,
            "audio" => Self::Audio,
            "sub" => Self::Sub,
            _ => Self::Other,
        }
    }

    /// The mpv property that selects a track of this kind
    /// (`vid` / `aid` / `sid`).
    pub const fn select_property(self) -> &'static str {
        match self {
            Self::Video => "vid",
            Self::Audio => "aid",
            Self::Sub => "sid",
            Self::Other => "tid",
        }
    }

    /// mpv's `type` string.
    pub const fn as_mpv(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Sub => "sub",
            Self::Other => "other",
        }
    }
}

/// An entry of mpv's `track-list` property.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Track id (mpv `id`; pass to [`MpvPlayer::select_track`]).
    pub id: i64,
    /// Human-readable title, if present.
    pub title: Option<String>,
    /// ISO 639-2 language code, if present.
    pub language: Option<String>,
    pub kind: TrackKind,
    /// Whether this track is currently selected.
    pub selected: bool,
}

impl Track {
    fn from_ffi(t: &ffi::Track) -> Self {
        Self {
            id: t.id,
            title: t.title.clone(),
            language: t.language.clone(),
            kind: TrackKind::from_mpv(&t.kind),
            selected: t.selected,
        }
    }

    /// Human-readable label: title, then language name, then
    /// `{fallback} #{index}`.
    pub fn display_name(&self, fallback: &str, index: usize) -> String {
        if let Some(t) = &self.title {
            if !t.is_empty() {
                return t.clone();
            }
        }
        if let Some(lang) = &self.language {
            if !lang.is_empty() {
                return super::language_name(lang)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| lang.clone());
            }
        }
        format!("{fallback} #{index}")
    }
}

/// Map an ISO 639 language code (as reported by mpv, usually 3-letter
/// 639-2 like "eng"/"deu") to a display name.
pub fn language_name(code: &str) -> Option<&'static str> {
    let code = code.trim().to_ascii_lowercase();
    let by_len = |len: usize| {
        code.get(..len).and_then(|n| match n {
            "en" => Some("English"),
            "de" => Some("German"),
            "fr" => Some("French"),
            "es" => Some("Spanish"),
            "it" => Some("Italian"),
            "pt" => Some("Portuguese"),
            "ru" => Some("Russian"),
            "ja" => Some("Japanese"),
            "zh" => Some("Chinese"),
            "ko" => Some("Korean"),
            "nl" => Some("Dutch"),
            "pl" => Some("Polish"),
            "tr" => Some("Turkish"),
            "ar" => Some("Arabic"),
            "hi" => Some("Hindi"),
            "sv" => Some("Swedish"),
            "no" => Some("Norwegian"),
            "da" => Some("Danish"),
            "fi" => Some("Finnish"),
            "el" => Some("Greek"),
            "he" => Some("Hebrew"),
            "cs" => Some("Czech"),
            "hu" => Some("Hungarian"),
            "ro" => Some("Romanian"),
            "uk" => Some("Ukrainian"),
            "th" => Some("Thai"),
            "vi" => Some("Vietnamese"),
            "id" => Some("Indonesian"),
            "ms" => Some("Malay"),
            "tl" => Some("Filipino"),
            "eng" => Some("English"),
            "ger" | "deu" => Some("German"),
            "fra" => Some("French"),
            "spa" => Some("Spanish"),
            "ita" => Some("Italian"),
            "por" => Some("Portuguese"),
            "rus" => Some("Russian"),
            "jpn" | "jap" => Some("Japanese"),
            "zho" | "chi" => Some("Chinese"),
            "kor" => Some("Korean"),
            "nld" | "dut" => Some("Dutch"),
            "pol" => Some("Polish"),
            "tur" => Some("Turkish"),
            "ara" => Some("Arabic"),
            "hin" => Some("Hindi"),
            "swe" => Some("Swedish"),
            "nor" => Some("Norwegian"),
            "dan" => Some("Danish"),
            "fin" => Some("Finnish"),
            "ell" | "gre" => Some("Greek"),
            "heb" => Some("Hebrew"),
            "ces" | "cze" => Some("Czech"),
            "hun" => Some("Hungarian"),
            "ron" | "rum" => Some("Romanian"),
            "ukr" => Some("Ukrainian"),
            "tha" => Some("Thai"),
            "vie" => Some("Vietnamese"),
            "ind" => Some("Indonesian"),
            "msa" | "may" => Some("Malay"),
            "tgl" | "fil" => Some("Filipino"),
            _ => None,
        })
    };
    // Prefer the exact (usually 3-letter) code, then fall back to the
    // leading 2-letter form.
    by_len(code.len()).or_else(|| by_len(2))
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// A decoded event from [`MpvPlayer::next_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpvEvent {
    /// One of `ffi::MPV_EVENT_*`.
    pub id: i32,
    /// `0` on success, negative `ffi::MPV_ERROR_*` on error.
    pub error: i32,
    /// Only for `MPV_EVENT_END_FILE`: one of `ffi::MPV_END_FILE_REASON_*`.
    pub end_file_reason: Option<i32>,
}

impl MpvEvent {
    /// `true` if the event is "no event" (poll timeout).
    pub const fn is_none(self) -> bool {
        self.id == ffi::MPV_EVENT_NONE
    }
    pub const fn is_end_file(self) -> bool {
        self.id == ffi::MPV_EVENT_END_FILE
    }
    /// END_FILE with reason `EOF` (natural end of the file).
    pub fn is_end_of_file(self) -> bool {
        self.id == ffi::MPV_EVENT_END_FILE
            && self.end_file_reason == Some(ffi::MPV_END_FILE_REASON_EOF)
    }
    pub const fn is_shutdown(self) -> bool {
        self.id == ffi::MPV_EVENT_SHUTDOWN
    }
    pub const fn is_idle(self) -> bool {
        self.id == ffi::MPV_EVENT_IDLE
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_cstr(s: &str) -> Result<CString, MpvError> {
    CString::new(s).map_err(|_| MpvError(ffi::MPV_ERROR_INVALID_PARAMETER))
}

/// Escape `s` for use inside an mpv command string.
///
/// mpv command strings separate arguments by whitespace and support `"..."`
/// quoting with `\"` and `\\` escapes. We wrap the value in quotes and escape
/// those two metacharacters so paths containing spaces round-trip.
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

// ---------------------------------------------------------------------------
// MpvPlayer
// ---------------------------------------------------------------------------

/// A safe handle to a single mpv core.
///
/// Owns the underlying `mpv_handle`; terminated on drop.
pub struct MpvPlayer {
    handle: *mut mpv_handle,
}

// mpv's client API is documented thread-safe (one thread per
// `mpv_wait_event`, one thread per render context — see `super::render`).
unsafe impl Send for MpvPlayer {}
unsafe impl Sync for MpvPlayer {}

impl std::fmt::Debug for MpvPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpvPlayer").finish()
    }
}

impl MpvPlayer {
    /// Create a core without initializing it.
    ///
    /// Sets `vo=libmpv` so that video frames are routed through the render
    /// context API (see [`super::render`]) instead of a display VO. Set more
    /// options with [`MpvPlayer::set_option`], then call [`MpvPlayer::init`].
    pub fn uninitialized() -> Result<Self, MpvError> {
        Self::pin_c_numeric_locale();
        let handle = unsafe { ffi::mpv_create() };
        if handle.is_null() {
            return Err(MpvError(ffi::MPV_ERROR_NOMEM));
        }
        let _ = Self::set_option_raw(handle, "vo", "libmpv");
        Ok(Self { handle })
    }

    /// libmpv requires a C locale: under a non-C `LC_NUMERIC` (e.g. `de_DE`)
    /// it prints "Non-C locale detected. This is not supported." and
    /// misparses numeric option/property values (or fails core init). The
    /// C library locale is process-global, so pin `LC_NUMERIC` to C on
    /// every core creation. Keeping it pinned is safe: everything else in
    /// this app (Rust std, ICU collation, chrono) formats numbers
    /// locale-independently.
    fn pin_c_numeric_locale() {
        unsafe {
            libc::setlocale(libc::LC_NUMERIC, b"C\0".as_ptr().cast());
        }
    }

    fn set_option_raw(handle: *mut mpv_handle, name: &str, value: &str) -> Result<(), MpvError> {
        let cname = to_cstr(name)?;
        let cvalue = to_cstr(value)?;
        let rc = unsafe { ffi::mpv_set_option_string(handle, cname.as_ptr(), cvalue.as_ptr()) };
        ffi::result_of(rc)
    }

    /// Create and immediately initialize a core (no pre-init options).
    pub fn new() -> Result<Self, MpvError> {
        Self::new_with_options(&[])
    }

    /// Create, apply `options` (e.g. `("ao", "null")`), and initialize.
    ///
    /// Core startup can fail transiently under load (event queue full,
    /// OOM); such failures are retried a couple of times with backoff.
    /// Option errors (unknown option, bad format) fail immediately.
    pub fn new_with_options(options: &[(&str, &str)]) -> Result<Self, MpvError> {
        let mut backoff = 20u64;
        let mut last = None;
        for _ in 0..3 {
            match Self::create_and_init(options) {
                Ok(p) => return Ok(p),
                Err(e) if Self::TRANSIENT_ERRORS.contains(&e.0) => {
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(backoff));
                    backoff = (backoff * 2).min(80);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.expect("retry loop ran at least once"))
    }

    fn create_and_init(options: &[(&str, &str)]) -> Result<Self, MpvError> {
        let p = Self::uninitialized()?;
        for (k, v) in options {
            p.set_option(k, v)?;
        }
        p.init()?;
        Ok(p)
    }

    /// Apply an `name=value` option. Must be called before [`MpvPlayer::init`].
    pub fn set_option(&self, name: &str, value: &str) -> Result<(), MpvError> {
        Self::set_option_raw(self.handle, name, value)
    }

    /// Initialize the core (starts the decode / audio machinery).
    pub fn init(&self) -> Result<(), MpvError> {
        let rc = unsafe { ffi::mpv_initialize(self.handle) };
        ffi::result_of(rc)
    }

    /// The raw handle (for the render context in [`super::render`]).
    pub fn handle(&self) -> *mut mpv_handle {
        self.handle
    }

    /// libmpv client API version (e.g. `1` for API v1).
    pub fn client_api_version() -> u64 {
        unsafe { ffi::mpv_client_api_version() }
    }

    /// Errors that indicate the core/client was momentarily busy rather
    /// than a genuine failure; [`MpvPlayer::command`] retries these
    /// (libmpv: `-1` event queue full, `-2` OOM, `-4` invalid parameter —
    /// the last one is what `seek` returns while the core is still in a
    /// load/startup transition).
    const TRANSIENT_ERRORS: [i32; 3] = [
        ffi::MPV_ERROR_EVENT_QUEUE_FULL,
        ffi::MPV_ERROR_NOMEM,
        ffi::MPV_ERROR_INVALID_PARAMETER,
    ];

    pub fn command(&self, args: &str) -> Result<(), MpvError> {
        let ccmd = to_cstr(args)?;
        // libmpv is asynchronous: right after a state transition (load,
        // pause/resume, output (re)init) the client can reject a command
        // with a transient error. Retry a bounded number of times with
        // backoff; a genuine failure fails fast with a different code and
        // is never retried.
        let mut backoff = 10u64;
        let mut rc = -1;
        for _ in 0..4 {
            rc = unsafe { ffi::mpv_command_string(self.handle, ccmd.as_ptr()) };
            if rc >= 0 || !Self::TRANSIENT_ERRORS.contains(&rc) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(backoff));
            backoff = (backoff * 2).min(50);
        }
        ffi::result_of(rc)
    }

    /// Load a file. `mode` is `"replace"` (default), `"append"`, etc.
    pub fn loadfile(&self, path: &str, mode: &str) -> Result<(), MpvError> {
        let cmd = format!("loadfile {} {}", mpv_quote(path), mode);
        self.command(&cmd)
    }

    /// Wait until the core's playback state machine is fully up (`playtime`
    /// available). Commands such as `seek` fail with
    /// `MPV_ERROR_N_COMMAND` if issued during the startup transition after
    /// `FILE_LOADED` (audio/video output setup still in flight); this
    /// blocks until that window has closed.
    ///
    /// Returns `true` once ready, `false` on timeout (core still loading
    /// or no playable stream).
    pub fn wait_playback_ready(&self, timeout: f64) -> bool {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs_f64(timeout.max(0.0));
        loop {
            if self.time_pos().ok().flatten().is_some() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // -- transport ----------------------------------------------------------

    pub fn play(&self) -> Result<(), MpvError> {
        self.command("set pause no")
    }
    pub fn pause(&self) -> Result<(), MpvError> {
        self.command("set pause yes")
    }
    pub fn toggle_pause(&self) -> Result<(), MpvError> {
        self.command("cycle pause")
    }
    pub fn stop(&self) -> Result<(), MpvError> {
        self.command("stop")
    }

    /// Seek by a relative amount (seconds; negative = backwards).
    ///
    /// Uses the binary `seek` property (f64) rather than a command string —
    /// mpv parses command arguments with the C locale's `strtod`, which
    /// mangles dotted decimals under non-C locales (e.g. `de_DE`).
    pub fn seek_relative(&self, seconds: f64) -> Result<(), MpvError> {
        self.set_property_double("seek", seconds)
    }

    /// Seek to an absolute position (seconds). Locale-safe (binary
    /// `time-pos` property, see [`Self::seek_relative`]).
    pub fn seek_absolute(&self, seconds: f64) -> Result<(), MpvError> {
        self.set_property_double("time-pos", seconds)
    }

    // -- tracks -------------------------------------------------------------

    /// List all tracks (video / audio / subtitle) of the loaded file.
    pub fn tracks(&self) -> Result<Vec<Track>, MpvError> {
        unsafe { ffi::track_list(self.handle) }
            .map(|v| v.into_iter().map(|t| Track::from_ffi(&t)).collect())
    }

    /// Select the track `id` of `kind` (e.g. `TrackKind::Audio`, id `2`).
    pub fn select_track(&self, kind: TrackKind, id: i64) -> Result<(), MpvError> {
        let prop = kind.select_property();
        self.command(&format!("set {prop} {id}"))
    }

    /// Set the external subtitle file (`sub-file` property).
    pub fn set_subtitle_file(&self, path: &str) -> Result<(), MpvError> {
        self.command(&format!("set sub-file {}", mpv_quote(path)))
    }

    /// Cycle to the next/previous track of `kind` (`direction` = `"up"`/`"down"`).
    pub fn cycle_track(&self, kind: TrackKind, direction: &str) -> Result<(), MpvError> {
        let prop = kind.select_property();
        self.command(&format!("cycle {prop} {direction}"))
    }

    // -- properties ---------------------------------------------------------

    pub fn get_property_flag(&self, name: &str) -> Result<Option<bool>, MpvError> {
        let cname = to_cstr(name)?;
        let mut v: i32 = 0;
        let rc = unsafe {
            ffi::mpv_get_property(
                self.handle,
                cname.as_ptr(),
                ffi::MPV_FORMAT_FLAG,
                &mut v as *mut _ as *mut std::os::raw::c_void,
            )
        };
        match ffi::result_of(rc) {
            Ok(()) => Ok(Some(v != 0)),
            Err(_) => Ok(None),
        }
    }

    pub fn get_property_double(&self, name: &str) -> Result<Option<f64>, MpvError> {
        let cname = to_cstr(name)?;
        let mut v: f64 = 0.0;
        let rc = unsafe {
            ffi::mpv_get_property(
                self.handle,
                cname.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                &mut v as *mut _ as *mut std::os::raw::c_void,
            )
        };
        match ffi::result_of(rc) {
            Ok(()) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    pub fn get_property_string(&self, name: &str) -> Result<Option<String>, MpvError> {
        unsafe { ffi::get_property_string(self.handle, name) }
    }

    pub fn set_property_string(&self, name: &str, value: &str) -> Result<(), MpvError> {
        unsafe { ffi::set_property_string(self.handle, name, value) }
    }

    pub fn set_property_double(&self, name: &str, value: f64) -> Result<(), MpvError> {
        let cname = to_cstr(name)?;
        let rc = unsafe {
            ffi::mpv_set_property(
                self.handle,
                cname.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                &value as *const f64 as *const std::os::raw::c_void,
            )
        };
        ffi::result_of(rc)
    }

    pub fn set_property_flag(&self, name: &str, value: bool) -> Result<(), MpvError> {
        let cname = to_cstr(name)?;
        let v = value as i32;
        let rc = unsafe {
            ffi::mpv_set_property(
                self.handle,
                cname.as_ptr(),
                ffi::MPV_FORMAT_FLAG,
                &v as *const i32 as *const std::os::raw::c_void,
            )
        };
        ffi::result_of(rc)
    }

    // -- common state (sugar over the generic accessors) --------------------

    pub fn is_paused(&self) -> Result<Option<bool>, MpvError> {
        self.get_property_flag("pause")
    }
    pub fn time_pos(&self) -> Result<Option<f64>, MpvError> {
        self.get_property_double("time-pos")
    }
    pub fn duration(&self) -> Result<Option<f64>, MpvError> {
        self.get_property_double("duration")
    }
    /// Native mpv volume scale (0..=100).
    pub fn volume(&self) -> Result<Option<f64>, MpvError> {
        self.get_property_double("volume")
    }
    pub fn set_volume(&self, v: f64) -> Result<(), MpvError> {
        self.set_property_double("volume", v)
    }
    pub fn muted(&self) -> Result<Option<bool>, MpvError> {
        self.get_property_flag("mute")
    }
    pub fn set_muted(&self, m: bool) -> Result<(), MpvError> {
        self.set_property_flag("mute", m)
    }
    pub fn speed(&self) -> Result<Option<f64>, MpvError> {
        self.get_property_double("speed")
    }
    pub fn set_speed(&self, s: f64) -> Result<(), MpvError> {
        self.set_property_double("speed", s)
    }
    /// `true` when a file is loaded and playing (not idle).
    pub fn is_playing_file(&self) -> Result<Option<bool>, MpvError> {
        self.get_property_flag("idle-active")
            .map(|r| r.map(|idle| !idle))
    }
}

/// Outcome of [`MpvPlayer::wait_file_loaded`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// `MPV_EVENT_FILE_LOADED` was observed.
    Loaded,
    /// `MPV_EVENT_END_FILE` was observed before the file could load
    /// (bad URI, missing file, unsupported format, ...).
    Failed,
    /// No load event within the deadline.
    Timeout,
}

impl LoadOutcome {
    /// Human-readable description (for error messages).
    pub fn as_str(self) -> &'static str {
        match self {
            LoadOutcome::Loaded => "loaded",
            LoadOutcome::Failed => "load failed (mpv end-file)",
            LoadOutcome::Timeout => "timed out waiting for load event",
        }
    }
}

impl MpvPlayer {
    /// Block until the current file finishes loading (or fails).
    ///
    /// Waits up to `timeout` seconds for `MPV_EVENT_FILE_LOADED` or
    /// `MPV_EVENT_END_FILE`; other events are drained and discarded. Call
    /// **after** `loadfile` so the load events being waited on are fresh.
    ///
    /// Note: `MPV_EVENT_IDLE` is deliberately **not** treated as terminal —
    /// the core-start IDLE can be queued asynchronously and arrive right
    /// after `loadfile`, which would otherwise look like a failed load.
    ///
    /// Consumes the events it sees; callers that need `FILE_LOADED`
    /// (e.g. to refresh tracks) must do so before calling this.
    pub fn wait_file_loaded(&self, timeout: f64) -> LoadOutcome {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs_f64(timeout.max(0.0));
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                return LoadOutcome::Timeout;
            }
            let remaining = (deadline - now).as_secs_f64();
            let Some(ev) = self.next_event(remaining) else {
                return LoadOutcome::Timeout;
            };
            match ev.id {
                ffi::MPV_EVENT_FILE_LOADED => return LoadOutcome::Loaded,
                ffi::MPV_EVENT_END_FILE => return LoadOutcome::Failed,
                ffi::MPV_EVENT_SHUTDOWN => return LoadOutcome::Timeout,
                _ => continue,
            }
        }
    }
}

impl Drop for MpvPlayer {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { ffi::mpv_terminate_destroy(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

// ---------------------------------------------------------------------------
// Event queue
// ---------------------------------------------------------------------------

impl MpvPlayer {
    /// Wait up to `timeout` seconds for the next event.
    ///
    /// Returns `None` on timeout (i.e. `MPV_EVENT_NONE`). The returned
    /// event's data is owned by the library and freed on the next call —
    /// copy out anything you need (we already do: see `MpvEvent`).
    ///
    /// Must not be called concurrently from multiple threads on this handle.
    pub fn next_event(&self, timeout: f64) -> Option<MpvEvent> {
        let e = unsafe { ffi::mpv_wait_event(self.handle, timeout) };
        if e.is_null() {
            return None;
        }
        let ev = unsafe { &*e };
        let mut end_reason = None;
        if ev.event_id == ffi::MPV_EVENT_END_FILE {
            let d = unsafe { &*(ev.data as *const ffi::mpv_event_end_file) };
            end_reason = Some(d.reason);
        }
        Some(MpvEvent {
            id: ev.event_id,
            error: ev.error,
            end_file_reason: end_reason,
        })
    }

    /// Drain all currently queued events (non-blocking).
    pub fn poll_events(&self) -> Vec<MpvEvent> {
        let mut out = Vec::new();
        while let Some(e) = self.next_event(0.0) {
            if e.is_none() {
                break;
            }
            out.push(e);
        }
        out
    }

    /// Interrupt a blocking [`MpvPlayer::next_event`] on this handle.
    pub fn wakeup(&self) {
        unsafe { ffi::mpv_wakeup(self.handle) };
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// These run against the real libmpv without any display or audio output:
// a core created with `mpv_create()` starts idle, so property and event
// access is fully functional headless.
// ---------------------------------------------------------------------------

/// A 1-second silent wav written by hand (no ffmpeg needed).
fn silent_wav() -> std::path::PathBuf {
    let path = std::env::temp_dir().join("mb_wait_load_test.wav");
    if path.exists() {
        return path;
    }
    // 100 samples of 16-bit silence, 8 kHz mono → tiny, always loadable.
    let sample_rate: u32 = 8000;
    let data = vec![0u8; 200];
    let mut buf = Vec::with_capacity(44 + data.len());
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&data);
    let _ = std::fs::write(&path, buf);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_escapes_metacharacters() {
        assert_eq!(mpv_quote("a b"), "\"a b\"");
        assert_eq!(mpv_quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(mpv_quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(mpv_quote(""), "\"\"");
    }

    #[test]
    fn track_kind_mapping_roundtrip() {
        assert_eq!(TrackKind::from_mpv("video"), TrackKind::Video);
        assert_eq!(TrackKind::from_mpv("audio"), TrackKind::Audio);
        assert_eq!(TrackKind::from_mpv("sub"), TrackKind::Sub);
        assert_eq!(TrackKind::from_mpv("weird"), TrackKind::Other);
        assert_eq!(TrackKind::Video.as_mpv(), "video");
        assert_eq!(TrackKind::Audio.select_property(), "aid");
        assert_eq!(TrackKind::Video.select_property(), "vid");
        assert_eq!(TrackKind::Sub.select_property(), "sid");
    }

    #[test]
    fn client_api_version_is_positive() {
        assert!(MpvPlayer::client_api_version() > 0);
    }

    #[test]
    fn create_init_and_drop() {
        let p = MpvPlayer::new().expect("core should initialize headless");
        assert!(!p.handle().is_null());
        drop(p);
    }

    #[test]
    fn pause_toggle_and_properties() {
        let p = MpvPlayer::new().expect("init");
        p.pause().expect("pause");
        assert_eq!(p.is_paused().ok().flatten(), Some(true));
        p.play().expect("play");
        assert_eq!(p.is_paused().ok().flatten(), Some(false));
        p.toggle_pause().expect("toggle");
        assert_eq!(p.is_paused().ok().flatten(), Some(true));
        p.toggle_pause().expect("toggle back");
        assert_eq!(p.is_paused().ok().flatten(), Some(false));
    }

    #[test]
    fn volume_mute_speed_roundtrip() {
        let p = MpvPlayer::new().expect("init");
        p.set_volume(42.0).expect("set volume");
        let v = p.volume().ok().flatten();
        assert!(v.is_some() && (v.unwrap() - 42.0).abs() < 0.5);

        p.set_muted(true).expect("mute");
        assert_eq!(p.muted().ok().flatten(), Some(true));
        p.set_muted(false).expect("unmute");
        assert_eq!(p.muted().ok().flatten(), Some(false));

        p.set_speed(1.5).expect("set speed");
        let s = p.speed().ok().flatten();
        assert!(s.is_some() && (s.unwrap() - 1.5).abs() < 0.01);
    }

    #[test]
    fn property_unavailable_returns_none() {
        let p = MpvPlayer::new().expect("init");
        // No file loaded: time-pos / duration are unavailable.
        assert_eq!(p.time_pos().ok().flatten(), None);
        assert_eq!(p.duration().ok().flatten(), None);
    }

    #[test]
    fn tracks_empty_or_unavailable_when_idle() {
        let p = MpvPlayer::new().expect("init");
        match p.tracks() {
            Ok(list) => assert!(list.is_empty()),
            // Some builds report the property as unavailable while idle.
            Err(MpvError(e)) => {
                assert!(
                    e == ffi::MPV_ERROR_PROPERTY_UNAVAILABLE
                        || e == ffi::MPV_ERROR_PROPERTY_NOT_FOUND,
                    "unexpected error: {e}"
                );
            }
        }
    }

    #[test]
    fn next_event_returns_none_on_timeout() {
        let p = MpvPlayer::new().expect("init");
        let ev = p.next_event(0.0);
        assert!(ev.map(|e| e.is_none()).unwrap_or(true));
        let drained = p.poll_events();
        assert!(drained.iter().all(|e| !e.is_none()));
    }

    #[test]
    fn wait_file_loaded_reports_loaded() {
        let p = MpvPlayer::new().expect("init");
        p.poll_events();
        let path = silent_wav();
        assert!(
            p.loadfile(path.to_str().expect("path"), "replace").is_ok(),
            "loadfile command must be accepted"
        );
        assert_eq!(p.wait_file_loaded(5.0), LoadOutcome::Loaded);
    }

    #[test]
    fn wait_file_loaded_reports_failure() {
        let p = MpvPlayer::new().expect("init");
        p.poll_events();
        p.loadfile("/nonexistent/mb_no_such_file_xyz.mp4", "replace")
            .expect("loadfile command is accepted even for bad paths");
        assert_eq!(p.wait_file_loaded(5.0), LoadOutcome::Failed);
    }

    #[test]
    fn wait_file_loaded_zero_timeout_returns_timeout() {
        let p = MpvPlayer::new().expect("init");
        p.poll_events();
        assert_eq!(p.wait_file_loaded(0.0), LoadOutcome::Timeout);
    }

    #[test]
    fn command_with_bad_name_errors() {
        let p = MpvPlayer::new().expect("init");
        let r = p.command("no-such-command-xyz");
        assert!(r.is_err());
    }

    #[test]
    fn set_option_before_init_rejects_bad_option() {
        let p = MpvPlayer::uninitialized().expect("create");
        // A guaranteed-unknown option must be rejected pre-init.
        assert!(p.set_option("definitely-not-an-option-xyz", "1").is_err());
        drop(p);
    }
}

#[cfg(test)]
mod concurrency {
    use super::*;
    use crate::mpv::render::SwRenderer;
    use std::sync::atomic::{AtomicBool, Ordering as O};

    /// Regression test for libmpv thread handling: two cores with active
    /// software-render workers while `mpv_set_property`/`mpv_get_property`
    /// are hammered from two additional threads. The property readback must
    /// always match the value just set.
    ///
    /// Note on teardown: render workers must be joined **before** the core
    /// is terminated — `mpv_terminate_destroy` while a worker is inside
    /// `mpv_render_*` aborts inside libmpv.
    #[test]
    fn property_access_concurrent_with_rendering() {
        let clip = std::env::temp_dir().join("mb_video_model_test.mp4");
        if !clip.exists() {
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
                .arg(&clip)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                eprintln!("skip: no ffmpeg and no test clip");
                return;
            }
        }
        let mut players = Vec::new();
        let mut renderers = Vec::new();
        for _ in 0..2 {
            let p = MpvPlayer::new_with_options(&[("ao", "null")]).expect("init");
            let r = SwRenderer::spawn(&p).expect("renderer");
            p.pause().expect("pre");
            p.loadfile(clip.to_string_lossy().as_ref(), "replace")
                .expect("loadfile");
            assert_eq!(p.wait_file_loaded(5.0), LoadOutcome::Loaded);
            p.play().expect("play");
            renderers.push(r);
            players.push(p);
        }
        let fail = std::sync::Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();
        for p in &players {
            let h = p.handle() as usize;
            let f = std::sync::Arc::clone(&fail);
            handles.push(std::thread::spawn(move || {
                let h = h as *mut ffi::mpv_handle;
                let name = std::ffi::CStr::from_bytes_with_nul(b"mute\0").unwrap();
                for i in 0..30 {
                    let want = (i % 2 == 0) as i32;
                    let s = unsafe {
                        ffi::mpv_set_property(
                            h,
                            name.as_ptr(),
                            ffi::MPV_FORMAT_FLAG,
                            &want as *const i32 as *mut std::os::raw::c_void,
                        )
                    };
                    let mut got = 0i32;
                    let r = unsafe {
                        ffi::mpv_get_property(
                            h,
                            name.as_ptr(),
                            ffi::MPV_FORMAT_FLAG,
                            &mut got as *mut i32 as *mut std::os::raw::c_void,
                        )
                    };
                    if s < 0 || r < 0 || got != want {
                        f.store(true, O::SeqCst);
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }));
        }
        for t in handles {
            t.join().expect("hammer thread");
        }
        assert!(
            !fail.load(O::SeqCst),
            "concurrent property access corrupted state"
        );
        // Teardown: workers first, then cores.
        for r in &mut renderers {
            r.shutdown();
        }
        drop(renderers);
        drop(players);
    }
}
