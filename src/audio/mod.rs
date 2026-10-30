// Copyright [iced-video-player]()
// SPDX-License-Identifier: MIT
// [MIT](http://opensource.org/licenses/MIT)
//
// Modifications:
// Copyright 2024 Alexander Schwarzkopf

pub mod audio;
pub mod audio_player;
pub mod audio_view;
pub mod coverart;

use crate::mpv::MpvError;
use thiserror::Error;

/// Errors from the audio module.
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
