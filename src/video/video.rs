//! mpv-backed video model.
//!
//! Wraps [`crate::mpv::MpvPlayer`] (decode / playback / properties) plus the
//! [`crate::mpv::SwRenderer`] frame pump. The wgpu side
//! ([`crate::pipeline`]) consumes the shared [`FrameSlot`].
//!
//! Volume uses the app's `0.0..=1.0` scale (mpv native is `0..=100`).
//! Positions are [`Duration`] seconds.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::Error;
use crate::mpv::{self, MpvPlayer, Track, TrackKind};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// How long [`Video::new`] waits for the file to finish loading.
const LOAD_TIMEOUT_SECS: f64 = 5.0;

/// How long [`Video::new`] waits for the first rendered frame after load.
const FRAME_SETTLE_MS: u64 = 2000;

/// A video loaded from a URI, backed by libmpv.
#[derive(Debug)]
pub struct Video {
    id: u64,
    player: MpvPlayer,
    renderer: mpv::render::SwRenderer,
    slot: Arc<mpv::render::FrameSlot>,
    alive: Arc<AtomicBool>,
    path: url::Url,
    subtitle_url: Option<url::Url>,
    looping: AtomicBool,
    is_eos: AtomicBool,
    /// Set once `EndOfStream` has been reported for the current EOS.
    eos_notified: AtomicBool,
}

impl Video {
    /// Create a player for `uri` and start playback.
    ///
    /// `subtitle` is an optional external subtitle file.
    pub fn new(uri: &url::Url, subtitle: Option<url::Url>) -> Result<Self, Error> {
        let player = MpvPlayer::new()?;
        Self::new_with_core(player, uri, subtitle)
    }

    /// Build the model around an already-created core.
    ///
    /// Tests use this to run with `ao=null` (no sound card needed); the
    /// core is otherwise created exactly like [`Video::new`].
    pub(crate) fn new_with_core(
        player: MpvPlayer,
        uri: &url::Url,
        subtitle: Option<url::Url>,
    ) -> Result<Self, Error> {
        let subtitle_url = subtitle.clone();
        if let Some(sub) = subtitle {
            if let Ok(p) = sub.to_file_path() {
                if let Some(s) = p.to_str() {
                    player.set_subtitle_file(s).map_err(|e| {
                        log::warn!("failed to set subtitle {s}: {e}");
                        Error::Mpv(e)
                    })?;
                }
            }
        }
        let renderer = mpv::render::SwRenderer::spawn(&player)?;
        let slot = renderer.slot();
        // Hold playback until the file is ready so it cannot run past the
        // end while we wait, then start from the beginning.
        player.pause()?;
        // Drop stale events (e.g. the core-start IDLE) so the load events
        // we wait on below are fresh.
        player.poll_events();
        player.loadfile(&uri.to_string(), "replace")?;
        let outcome = player.wait_file_loaded(LOAD_TIMEOUT_SECS);
        if !matches!(outcome, mpv::LoadOutcome::Loaded) {
            log::error!("video load failed for {uri}: {outcome:?}");
            return Err(Error::LoadFailed(outcome.as_str().to_string()));
        }
        player.play()?;
        // Let the pipeline settle: block briefly until the first rendered
        // frame is published (or the deadline expires). Property access in
        // the first moments after load races the render worker otherwise.
        let start = std::time::Instant::now();
        while slot.snapshot().is_none()
            && start.elapsed() < std::time::Duration::from_millis(FRAME_SETTLE_MS)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let subtitle: Option<url::Url> = subtitle_url;
        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            player,
            renderer,
            slot,
            alive: Arc::new(AtomicBool::new(true)),
            path: uri.clone(),
            subtitle_url: subtitle,
            looping: AtomicBool::new(false),
            is_eos: AtomicBool::new(false),
            eos_notified: AtomicBool::new(false),
        })
    }

    /// The path this video was loaded from.
    pub fn path(&self) -> &url::Url {
        &self.path
    }

    /// The external subtitle URL, if any.
    pub fn subtitle_url(&self) -> Option<url::Url> {
        self.subtitle_url.clone()
    }

    /// Stable id for the pipeline texture slot.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Shared liveness flag (pipeline trims dead entries).
    pub fn alive(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.alive)
    }

    /// The shared frame slot (latest sw-rendered BGRA frame).
    pub fn frame_slot(&self) -> Arc<mpv::render::FrameSlot> {
        Arc::clone(&self.slot)
    }

    // -- transport ----------------------------------------------------------

    pub fn paused(&self) -> bool {
        self.player.is_paused().ok().flatten().unwrap_or(false)
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), Error> {
        if paused {
            self.player.pause()?;
        } else {
            self.player.play()?;
            // Resuming after end-of-file restarts from the beginning.
            if self.is_eos.swap(false, Ordering::SeqCst) {
                self.player.seek_absolute(0.0)?;
            }
        }
        self.eos_notified.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn position(&self) -> Duration {
        self.player
            .time_pos()
            .unwrap_or(Some(0.0))
            .map(|s| Duration::try_from_secs_f64(s).unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn duration(&self) -> Duration {
        self.player
            .duration()
            .unwrap_or(Some(0.0))
            .map(|s| Duration::try_from_secs_f64(s).unwrap_or_default())
            .unwrap_or_default()
    }

    /// Seek to an absolute position. `accurate` is accepted for API
    /// compatibility (mpv seeks are accurate by default).
    pub fn seek(&self, position: Duration, _accurate: bool) -> Result<(), Error> {
        self.player.seek_absolute(position.as_secs_f64())?;
        self.eos_notified.store(false, Ordering::SeqCst);
        Ok(())
    }

    // -- volume / mute (app scale 0.0..=1.0) --------------------------------

    pub fn volume(&self) -> f64 {
        (self.player.volume().unwrap_or(Some(100.0)).unwrap_or(100.0) / 100.0).clamp(0.0, 1.0)
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), Error> {
        self.player
            .set_volume((volume.clamp(0.0, 1.0) * 100.0) as f64)?;
        Ok(())
    }

    pub fn muted(&self) -> bool {
        self.player.muted().ok().flatten().unwrap_or(false)
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), Error> {
        self.player.set_muted(muted)?;
        Ok(())
    }

    /// Toggle mute; returns the new state.
    pub fn toggle_muted(&self) -> bool {
        let m = !self.muted();
        if let Err(e) = self.set_muted(m) {
            log::warn!("failed to toggle mute: {e}");
        }
        self.muted()
    }

    // -- looping ------------------------------------------------------------

    pub fn looping(&self) -> bool {
        self.looping.load(Ordering::SeqCst)
    }

    pub fn set_looping(&self, looping: bool) {
        self.looping.store(looping, Ordering::SeqCst);
    }

    pub fn is_eos(&self) -> bool {
        self.is_eos.load(Ordering::SeqCst)
    }

    /// `true` exactly once per end-of-file: on the first call after EOS
    /// until the next seek / resume / start.
    pub fn take_eos_event(&self) -> bool {
        self.is_eos.load(Ordering::SeqCst)
            && self
                .eos_notified
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
    }

    // -- tracks ---------------------------------------------------------------

    /// All tracks (audio / video / subtitle) of the loaded file.
    pub fn tracks(&self) -> Result<Vec<Track>, Error> {
        Ok(self.player.tracks()?)
    }

    /// Select the `index`-th audio track.
    pub fn select_audio(&self, index: usize) -> Result<(), Error> {
        self.select_nth(TrackKind::Audio, index)
    }

    /// Select the `index`-th subtitle track (or disable when `index == 0`
    /// and there is no such track).
    pub fn select_subtitle(&self, index: usize) -> Result<(), Error> {
        self.select_nth(TrackKind::Sub, index)
    }

    fn select_nth(&self, kind: TrackKind, index: usize) -> Result<(), Error> {
        let tracks = self.player.tracks()?;
        let track = tracks
            .iter()
            .filter(|t| t.kind == kind)
            .nth(index)
            .ok_or(Error::NoTrack)?;
        self.player.select_track(kind, track.id)?;
        Ok(())
    }

    /// Index of the currently selected audio track (in the audio-track list).
    pub fn current_audio(&self) -> i32 {
        self.current_index(TrackKind::Audio)
    }

    /// Index of the currently selected subtitle track.
    pub fn current_subtitle(&self) -> i32 {
        self.current_index(TrackKind::Sub)
    }

    fn current_index(&self, kind: TrackKind) -> i32 {
        let Ok(tracks) = self.player.tracks() else {
            return -1;
        };
        let mut idx = 0i32;
        for t in tracks {
            if t.kind == kind {
                if t.selected {
                    return idx;
                }
                idx += 1;
            }
        }
        -1
    }

    // -- events ---------------------------------------------------------------

    /// Drain pending mpv events and maintain EOS / looping state.
    ///
    /// Must be called from a single thread (the UI update loop).
    pub fn handle_events(&self) -> Vec<mpv::MpvEvent> {
        let events = self.player.poll_events();
        for e in &events {
            if e.id == mpv::MPV_EVENT_START_FILE {
                self.is_eos.store(false, Ordering::SeqCst);
                self.eos_notified.store(false, Ordering::SeqCst);
            }
            if e.is_end_file() {
                self.is_eos.store(true, Ordering::SeqCst);
                if self.looping.load(Ordering::SeqCst) && e.is_end_of_file() {
                    if self.player.seek_absolute(0.0).is_ok() {
                        let _ = self.player.play();
                        self.is_eos.store(false, Ordering::SeqCst);
                    }
                }
            }
            if e.is_shutdown() {
                break;
            }
        }
        events
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        // Drop order matters: stop the render worker (frees the render
        // context on its own thread) before terminating the core.
        self.renderer.shutdown();
        let _ = self.player.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// These run against the real libmpv headlessly (no display, `ao=null` is
// not needed since audio output is optional for a video-only clip). They are
// skipped if `ffmpeg` is unavailable (CI without codec tooling).

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate a 3-second 320x240 H.264 clip; returns `None` if ffmpeg is
    /// missing (test skipped).
    fn test_clip() -> Option<std::path::PathBuf> {
        let path = std::env::temp_dir().join("mb_video_model_test.mp4");
        if path.exists() {
            return Some(path);
        }
        let ok = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=10",
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
        ok.then(|| path.clone()).filter(|_| path.exists())
    }

    #[test]
    fn model_lifecycle_and_controls() {
        let Some(path) = test_clip() else {
            eprintln!("skip: no ffmpeg");
            return;
        };
        let uri = url::Url::from_file_path(&path).expect("uri");
        // `ao=null`: playback must not depend on a working sound card.
        let player = MpvPlayer::new_with_options(&[("ao", "null")]).expect("init");
        let video = Video::new_with_core(player, &uri, None).expect("open video");

        // `new()` waits for the load event, so metadata is available now.
        let dur = video.duration().as_secs_f64();
        assert!((1.0..=3.5).contains(&dur), "duration={dur}");

        // Pause / resume.
        video.set_paused(true).expect("pause");
        assert!(video.paused());
        let p1 = video.position().as_secs_f64();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!((video.position().as_secs_f64() - p1).abs() < 0.2);
        video.set_paused(false).expect("resume");
        assert!(!video.paused());

        // Seek.
        video
            .seek(std::time::Duration::from_secs_f64(1.0), true)
            .expect("seek");
        std::thread::sleep(std::time::Duration::from_millis(100));
        let pos = video.position().as_secs_f64();
        assert!((0.0..=2.5).contains(&pos), "pos={pos}");

        // Volume roundtrip (0..1 scale).
        video.set_volume(0.4).expect("volume");
        assert!((video.volume() - 0.4).abs() < 0.05);
        video.set_volume(1.0).expect("volume");

        // Mute roundtrip (`toggle_muted` returns the new state).
        assert!(video.toggle_muted());
        assert!(video.muted());
        assert!(!video.toggle_muted());
        assert!(!video.muted());

        // Tracks: at least one video track for this clip.
        let tracks = video.tracks().expect("tracks");
        assert!(tracks.iter().any(|t| t.kind == TrackKind::Video));

        // A frame should be published shortly after start.
        let slot = video.frame_slot();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut frame = None;
        while std::time::Instant::now() < deadline && frame.is_none() {
            let _ = slot.wait_for_new(std::time::Duration::from_millis(100));
            if let Some(f) = slot.snapshot() {
                if f.bytes.iter().any(|b| *b != 0) {
                    frame = Some(f);
                }
            }
        }
        let f = frame.expect("no frame published");
        assert_eq!(f.width, 320);
        assert_eq!(f.height, 240);

        drop(video);
    }
}
