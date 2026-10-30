//! Video playback module (libmpv-backed).

pub mod video;
pub mod video_player;
pub mod video_view;

use crate::mpv::MpvError;
use thiserror::Error;

/// Errors from the video module.
#[derive(Debug, Error)]
pub enum Error {
    #[error("libmpv error: {0}")]
    Mpv(#[from] MpvError),

    #[error("failed to lock internal sync primitive")]
    Lock,

    #[error("invalid URI")]
    Uri,

    #[error("track not found")]
    NoTrack,

    #[error("file failed to load: {0}")]
    LoadFailed(String),
}
