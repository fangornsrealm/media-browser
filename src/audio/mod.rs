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
pub mod pipeline;

use gstreamer as gst;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Glib(#[from] gst::glib::Error),
    #[error("{0}")]
    Bool(#[from] gst::glib::BoolError),
    #[error("failed to get the gstreamer bus")]
    Bus,
    #[error("failed to get AppSink element with name='{0}' from gstreamer pipeline")]
    AppSink(String),
    #[error("{0}")]
    StateChange(#[from] gst::StateChangeError),
    #[error("{0}")]
    Flow(#[from] gst::FlowError),
    #[error("failed to cast gstreamer element")]
    Cast,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("invalid URI")]
    Uri,
    #[error("failed to get media capabilities")]
    Caps,
    #[error("failed to query media duration or position")]
    Duration,
    #[error("failed to sync with playback")]
    Sync,
    #[error("failed to lock internal sync primitive")]
    Lock,
    #[error("invalid framerate: {0}")]
    Framerate(f64),
}
