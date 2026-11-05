//! Progress reports sent from a background similarity scan to the UI thread.

use std::path::PathBuf;

/// A single progress report from a similarity scan.
///
/// Sent from the scan thread over a `tokio::sync::mpsc` channel and
/// re-dispatched as `crate::app::Message::SimilarProgress`.
#[derive(Debug, Clone)]
pub enum SimilarProgress {
    /// `done` of `total` files have been processed.
    FileProgress {
        done: usize,
        total: usize,
        /// Description of the most recent match, if any.
        last_match: Option<String>,
    },
    /// A result file (image pair or video HTML report) was written into the
    /// given output directory.
    PairCreated { path: PathBuf },
    /// The scan finished; totals of the run.
    Complete { pairs_found: usize, html_files: usize },
    /// A non-fatal per-file problem; the scan continues.
    Warning { message: String },
    /// The scan was cancelled by the user.
    Cancelled,
}
