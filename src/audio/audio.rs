//! mpv-backed audio model.
//!
//! Wraps [`crate::mpv::MpvPlayer`] for decode / playback. There is no video
//! output; the widget displays cover art (see [`super::coverart`]) as a
//! static [`crate::mpv::render::Frame`] blitted by [`crate::pipeline`].
//!
//! Volume uses the app's `0.0..=1.0` scale (mpv native is `0..=100`).
//! Positions are [`Duration`] seconds.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::Error;
use crate::mpv::{self, MpvPlayer};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// How long [`Audio::new`] waits for the file to finish loading.
const LOAD_TIMEOUT_SECS: f64 = 5.0;

/// An audio file loaded from a path, backed by libmpv.
pub struct Audio {
    id: u64,
    player: MpvPlayer,
    /// Cover art frame (BGRA) for the widget.
    cover: Arc<crate::mpv::render::Frame>,
    alive: Arc<AtomicBool>,
    path: String,
    posterpath: Option<String>,
    looping: AtomicBool,
    is_eos: AtomicBool,
    /// Set once `EndOfStream` has been reported for the current EOS.
    eos_notified: AtomicBool,
}

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audio")
            .field("id", &self.id)
            .field("path", &self.path)
            .finish()
    }
}

impl Audio {
    /// Create a player for the file at `uri` (a path or URL string) and
    /// start playback. `posterpath` is an optional cover-art image.
    pub fn new(uri: &str, posterpath: Option<String>) -> Result<Self, Error> {
        let player = MpvPlayer::new()?;
        Self::new_with_core(player, uri, posterpath)
    }

    /// Build the model around an already-created core.
    ///
    /// Tests use this to run with `ao=null` (no sound card needed); the
    /// core is otherwise created exactly like [`Audio::new`].
    pub(crate) fn new_with_core(
        player: MpvPlayer,
        uri: &str,
        posterpath: Option<String>,
    ) -> Result<Self, Error> {
        let poster = posterpath
            .clone()
            .or(Some("res/audio_dummy.jpg".to_string()));
        let (bytes, width, height) = super::coverart::cover_from_image(poster);
        let stride = (width as u32 * 4).max(1);
        let cover = Arc::new(crate::mpv::render::Frame {
            bytes: Arc::new(bytes),
            width,
            height,
            stride,
            frame_id: 1,
        });
        // Hold playback until the file is ready so it cannot run past the
        // end while we wait, then start from the beginning.
        player.pause()?;
        // Drop stale events (e.g. the core-start IDLE) so the load events
        // we wait on below are fresh.
        player.poll_events();
        player.loadfile(uri, "replace")?;
        let outcome = player.wait_file_loaded(LOAD_TIMEOUT_SECS);
        if !matches!(outcome, mpv::LoadOutcome::Loaded) {
            log::error!("audio load failed for {uri}: {outcome:?}");
            return Err(Error::LoadFailed(outcome.as_str().to_string()));
        }
        player.play()?;
        // `FILE_LOADED` can arrive while the core is still finishing its
        // startup transition (output setup); transport commands such as
        // `seek` fail with MPV_ERROR_N_COMMAND in that window.
        if !player.wait_playback_ready(1.0) {
            log::warn!("audio core not playback-ready within 1s for {uri}");
        }

        Ok(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            player,
            cover,
            alive: Arc::new(AtomicBool::new(true)),
            path: uri.to_string(),
            posterpath,
            looping: AtomicBool::new(false),
            is_eos: AtomicBool::new(false),
            eos_notified: AtomicBool::new(false),
        })
    }

    /// The path this audio was loaded from.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The poster path, if any was provided.
    pub fn posterpath(&self) -> Option<String> {
        self.posterpath.clone()
    }

    /// Stable id for the pipeline texture slot.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Shared liveness flag (pipeline trims dead entries).
    pub fn alive(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.alive)
    }

    /// The cover-art frame (BGRA).
    pub fn cover(&self) -> &crate::mpv::render::Frame {
        &self.cover
    }

    /// Cover art size (width, height).
    pub fn size(&self) -> (u32, u32) {
        (self.cover.width, self.cover.height)
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

    pub fn speed(&self) -> f64 {
        self.player.speed().unwrap_or(Some(1.0)).unwrap_or(1.0)
    }

    pub fn set_speed(&self, speed: f64) -> Result<(), Error> {
        if speed > 0.0 {
            self.player.set_speed(speed)?;
        }
        Ok(())
    }

    // -- volume / mute (app scale 0.0..=1.0) --------------------------------

    /// Volume in `0.0..=1.0`.
    pub fn volume(&self) -> f64 {
        (self.player.volume().unwrap_or(Some(100.0)).unwrap_or(100.0) / 100.0).clamp(0.0, 1.0)
    }

    /// Set volume in `0.0..=1.0`.
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

    // -- looping / EOS ------------------------------------------------------

    pub fn looping(&self) -> bool {
        self.looping.load(Ordering::SeqCst)
    }

    pub fn set_looping(&self, looping: bool) {
        self.looping.store(looping, Ordering::SeqCst);
    }

    /// Whether the stream reached the end.
    pub fn eos(&self) -> bool {
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

    /// All tracks of the loaded file.
    pub fn tracks(&self) -> Result<Vec<crate::mpv::Track>, Error> {
        Ok(self.player.tracks()?)
    }

    /// Select the `index`-th audio track.
    pub fn select_audio(&self, index: usize) -> Result<(), Error> {
        let tracks = self.player.tracks()?;
        let track = tracks
            .iter()
            .filter(|t| t.kind == crate::mpv::TrackKind::Audio)
            .nth(index)
            .ok_or(Error::NoTrack)?;
        self.player
            .select_track(crate::mpv::TrackKind::Audio, track.id)?;
        Ok(())
    }

    /// Index of the currently selected audio track (in the audio-track list).
    pub fn current_audio(&self) -> i32 {
        let Ok(tracks) = self.player.tracks() else {
            return -1;
        };
        let mut idx = 0i32;
        for t in tracks {
            if t.kind == crate::mpv::TrackKind::Audio {
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
    pub fn handle_events(&self) {
        for e in self.player.poll_events() {
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
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.player.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// Run against the real libmpv headlessly. Skipped without `ffmpeg`.

#[cfg(test)]
mod tests {
    use super::*;

    fn test_audio() -> Option<std::path::PathBuf> {
        let path = std::env::temp_dir().join("mb_audio_model_test.mp3");
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
                "sine=frequency=440:duration=2",
                "-c:a",
                "libmp3lame",
            ])
            .arg(&path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then(|| path.clone()).filter(|_| path.exists())
    }

    #[test]
    fn model_lifecycle_and_controls() {
        let Some(path) = test_audio() else {
            eprintln!("skip: no ffmpeg");
            return;
        };
        let uri = url::Url::from_file_path(&path).expect("uri");
        // `ao=null`: playback must not depend on a working sound card.
        let player = MpvPlayer::new_with_options(&[("ao", "null")]).expect("init");
        let audio = Audio::new_with_core(player, &uri.to_string(), None).expect("open audio");

        // `new()` waits for the load event, so metadata is available now.
        let dur = audio.duration().as_secs_f64();
        assert!(dur >= 1.0, "duration={dur}");
        assert!(
            audio.size().0 > 0,
            "cover art should be present (dummy fallback)"
        );
        assert!(
            audio.cover().bytes.len()
                >= audio.cover().width as usize * audio.cover().height as usize * 4
        );

        audio.set_paused(true).expect("pause");
        assert!(audio.paused());
        audio.set_paused(false).expect("resume");
        assert!(!audio.paused());

        audio.seek(Duration::from_millis(500), true).expect("seek");
        std::thread::sleep(Duration::from_millis(100));
        assert!(audio.position().as_secs_f64() < 2.0);

        audio.set_volume(0.3).expect("volume");
        assert!((audio.volume() - 0.3).abs() < 0.05);
        audio.set_volume(1.0).expect("volume");

        assert!(audio.toggle_muted());
        assert!(audio.muted());
        audio.toggle_muted();
        assert!(!audio.muted());

        let tracks = audio.tracks().expect("tracks");
        assert!(
            tracks
                .iter()
                .any(|t| t.kind == crate::mpv::TrackKind::Audio)
        );

        drop(audio);
    }
}
