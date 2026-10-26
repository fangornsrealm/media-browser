use super::Error;
use gstreamer as gst;
use gstreamer_app::prelude::*;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Position in the media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Position {
    /// Position based on time.
    ///
    /// Not the most accurate format for videos.
    Time(Duration),
    /// Position based on nth frame.
    Frame(u64),
}

impl From<Position> for gst::GenericFormattedValue {
    fn from(pos: Position) -> Self {
        match pos {
            Position::Time(t) => gst::ClockTime::from_nseconds(t.as_nanos() as _).into(),
            Position::Frame(f) => gst::format::Default::from_u64(f).into(),
        }
    }
}

impl From<Duration> for Position {
    fn from(t: Duration) -> Self {
        Position::Time(t)
    }
}

impl From<u64> for Position {
    fn from(f: u64) -> Self {
        Position::Frame(f)
    }
}

#[derive(Debug)]
pub(crate) struct Internal {
    pub(crate) id: u64,

    pub(crate) bus: gst::Bus,
    pub(crate) source: gst::Pipeline,
    pub(crate) alive: Arc<AtomicBool>,
    pub(crate) worker: Option<std::thread::JoinHandle<()>>,
    pub(crate) width: u32,
    pub(crate) height: u32,

    pub(crate) duration: Duration,
    pub(crate) speed: f64,

    pub(crate) frame: Arc<Mutex<Vec<u8>>>,

    pub(crate) looping: bool,
    pub(crate) is_eos: bool,
    pub(crate) restart_stream: bool,
}

impl Internal {
    pub(crate) fn seek(&self, position: impl Into<Position>, accurate: bool) -> Result<(), Error> {
        let position = position.into();

        // gstreamer complains if the start & end value types aren't the same
        match &position {
            Position::Time(_) => self.source.seek(
                self.speed,
                gst::SeekFlags::FLUSH
                    | if accurate {
                        gst::SeekFlags::ACCURATE
                    } else {
                        gst::SeekFlags::empty()
                    },
                gst::SeekType::Set,
                gst::GenericFormattedValue::from(position),
                gst::SeekType::Set,
                gst::ClockTime::NONE,
            )?,
            Position::Frame(_) => self.source.seek(
                self.speed,
                gst::SeekFlags::FLUSH
                    | if accurate {
                        gst::SeekFlags::ACCURATE
                    } else {
                        gst::SeekFlags::empty()
                    },
                gst::SeekType::Set,
                gst::GenericFormattedValue::from(position),
                gst::SeekType::Set,
                gst::format::Default::NONE,
            )?,
        };

        Ok(())
    }

    pub(crate) fn set_speed(&mut self, speed: f64) -> Result<(), Error> {
        let Some(position) = self.source.query_position::<gst::ClockTime>() else {
            return Err(Error::Caps);
        };
        if speed > 0.0 {
            self.source.seek(
                speed,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                position,
                gst::SeekType::End,
                gst::ClockTime::from_seconds(0),
            )?;
        } else {
            self.source.seek(
                speed,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds(0),
                gst::SeekType::Set,
                position,
            )?;
        }
        self.speed = speed;
        Ok(())
    }

    pub(crate) fn restart_stream(&mut self) -> Result<(), Error> {
        self.is_eos = false;
        self.set_paused(false);
        self.seek(0, false)?;
        Ok(())
    }

    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.source
            .set_state(if paused {
                gst::State::Paused
            } else {
                gst::State::Playing
            })
            .unwrap(/* state was changed in ctor; state errors caught there */);

        // Set restart_stream flag to make the stream restart on the next Message::NextFrame
        if self.is_eos && !paused {
            self.restart_stream = true;
        }
    }

    pub(crate) fn paused(&self) -> bool {
        self.source.state(gst::ClockTime::ZERO).1 == gst::State::Paused
    }
}

/// A multimedia Audio loaded from a URI (e.g., a local file path or HTTP stream).
#[derive(Debug)]
pub struct Audio(pub(crate) RwLock<Internal>);

impl Drop for Audio {
    fn drop(&mut self) {
        let inner = self.0.get_mut().expect("failed to lock");

        inner
            .source
            .set_state(gst::State::Null)
            .expect("failed to set state");

        inner.alive.store(false, Ordering::SeqCst);
        if let Some(worker) = inner.worker.take() {
            worker.join().expect("failed to stop video thread");
        }
    }
}

impl Audio {
    /// Create a new video player from a given Audio which loads from `uri`.
    /// Note that live sources will report the duration to be zero.
    pub fn new(uri: &str, posterpath: Option<String>) -> Result<Self, Error> {
        gst::init()?;

        let pipeline = format!("playbin uri=\"{}\" ", uri);

        let pipeline = gst::parse::launch(pipeline.as_ref())
            .unwrap()
            .downcast::<gst::Pipeline>()
            .map_err(|_| super::Error::Cast)
            .unwrap();

        Self::from_gst_pipeline(pipeline, posterpath)
    }

    /// Creates a new video based on an existing GStreamer pipeline and appsink.
    ///
    /// **Note:** Many functions of [`Audio`] assume a `playbin` pipeline.
    /// Non-`playbin` pipelines given here may not have full functionality.
    pub fn from_gst_pipeline(
        pipeline: gst::Pipeline,
        posterpath: Option<String>,
    ) -> Result<Self, Error> {
        gst::init()?;
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);

        pipeline.set_state(gst::State::Playing)?;

        // wait for up to 5 seconds until the decoder gets the source capabilities
        pipeline.state(gst::ClockTime::from_seconds(5)).0?;

        // extract resolution and framerate
        // TODO(jazzfool): maybe we want to extract some other information too?

        let duration = Duration::from_nanos(
            pipeline
                .query_duration::<gst::ClockTime>()
                .map(|duration| duration.nseconds())
                .unwrap_or(0),
        );
        let realposterpath;
        if posterpath.is_some() {
            realposterpath = posterpath;
        } else {
            realposterpath = Some("res/audio_dummy.jpg".to_string());
        }

        let (v, width, height) = crate::audio::coverart::frame_from_image(realposterpath.clone());
        let frame = Arc::new(Mutex::new(v));
        let alive = Arc::new(AtomicBool::new(true));

        Ok(Audio(RwLock::new(Internal {
            id,

            bus: pipeline.bus().unwrap(),
            source: pipeline,
            alive,
            worker: None,
            width,
            height,

            duration,
            speed: 1.0,

            frame,

            looping: false,
            is_eos: false,
            restart_stream: false,
        })))
    }

    pub(crate) fn read(&self) -> impl Deref<Target = Internal> + '_ {
        self.0.read().expect("lock")
    }

    pub(crate) fn write(&self) -> impl DerefMut<Target = Internal> + '_ {
        self.0.write().expect("lock")
    }

    pub(crate) fn get_mut(&mut self) -> impl DerefMut<Target = Internal> + '_ {
        self.0.get_mut().expect("lock")
    }

    /// Set the volume multiplier of the audio.
    /// `0.0` = 0% volume, `1.0` = 100% volume.
    ///
    /// This uses a linear scale, for example `0.5` is perceived as half as loud.
    pub fn set_volume(&mut self, volume: f64) {
        self.get_mut().source.set_property("volume", volume);
        self.set_muted(self.muted()); // for some reason gstreamer unmutes when changing volume?
    }

    /// Get the volume multiplier of the audio.
    pub fn volume(&self) -> f64 {
        self.read().source.property("volume")
    }

    /// Set if the audio is muted or not, without changing the volume.
    pub fn set_muted(&mut self, muted: bool) {
        self.get_mut().source.set_property("mute", muted);
    }

    /// Get the size/resolution of the video as `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        (self.read().width, self.read().height)
    }

    /// Get if the audio is muted or not.
    pub fn muted(&self) -> bool {
        self.read().source.property("mute")
    }

    /// Get if the stream ended or not.
    pub fn eos(&self) -> bool {
        self.read().is_eos
    }

    /// Get if the media will loop or not.
    pub fn looping(&self) -> bool {
        self.read().looping
    }

    /// Set if the media will loop or not.
    pub fn set_looping(&mut self, looping: bool) {
        self.get_mut().looping = looping;
    }

    /// Set if the media is paused or not.
    pub fn set_paused(&mut self, paused: bool) {
        self.get_mut().set_paused(paused)
    }

    /// Get if the media is paused or not.
    pub fn paused(&self) -> bool {
        self.read().paused()
    }

    /// Jumps to a specific position in the media.
    /// Passing `true` to the `accurate` parameter will result in more accurate seeking,
    /// however, it is also slower. For most seeks (e.g., scrubbing) this is not needed.
    pub fn seek(&mut self, position: impl Into<Position>, accurate: bool) -> Result<(), Error> {
        self.get_mut().seek(position, accurate)
    }

    /// Set the playback speed of the media.
    /// The default speed is `1.0`.
    pub fn set_speed(&mut self, speed: f64) -> Result<(), Error> {
        self.get_mut().set_speed(speed)
    }

    /// Get the current playback speed.
    pub fn speed(&self) -> f64 {
        self.read().speed
    }

    /// Get the current playback position in time.
    pub fn position(&self) -> Duration {
        Duration::from_nanos(
            self.read()
                .source
                .query_position::<gst::ClockTime>()
                .map_or(0, |pos| pos.nseconds()),
        )
    }

    /// Get the media duration.
    pub fn duration(&self) -> Duration {
        self.read().duration
    }

    /// Restarts a stream; seeks to the first frame and unpauses, sets the `eos` flag to false.
    pub fn restart_stream(&mut self) -> Result<(), Error> {
        self.get_mut().restart_stream()
    }

    /// Get the underlying GStreamer pipeline.
    pub fn pipeline(&self) -> gst::Pipeline {
        self.read().source.clone()
    }
}
