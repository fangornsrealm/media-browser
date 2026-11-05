//! Errors for similarity scans.
//!
//! Per-file problems (undecodable image, unreadable video) are not errors of
//! the scan as a whole: they are reported as
//! [`SimilarProgress::Warning`](super::progress::SimilarProgress::Warning)
//! and the scan continues.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SimilarError {
    #[error("the scan directory {0} does not exist or is not a directory")]
    InvalidScanDir(PathBuf),

    #[error("ffmpeg not found. Install ffmpeg for video similarity search.")]
    FfmpegMissing,

    #[error("failed to create output directory {path}: {source}")]
    OutputDir { path: PathBuf, source: std::io::Error },

    #[error("failed to connect to the video similarity database: {0}")]
    VideoDb(String),

    #[error("I/O error: {0}")]
    Io(std::io::Error),

    #[error("a background scan thread panicked: {0}")]
    Panic(String),

    #[error("scan cancelled")]
    Cancelled,
}

impl From<std::io::Error> for SimilarError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
