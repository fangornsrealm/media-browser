//! Image similarity scan (wraps `duplo_rs::store::Store`).
//!
//! Mirrors the `demo_similar_images` flow of duplo-rs:
//! 1. each image is hashed and compared against the persistent store;
//! 2. on a match, the "better" image (larger pixel area; on a tie the
//!    newer file) is kept in place and represented by a `*_KEEP_*` hardlink
//!    in `similar_images/`, the other is *moved* to `*_REMOVE_*`;
//! 3. unmatched images are added to the store so future runs skip them.
//!
//! The store is persisted before returning, so the scan can be cancelled
//! and resumed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use futures::channel::mpsc as futures_mpsc;

use super::{
    data_dir, list_media_files, panic_payload_to_string, SimilarKind,
};
use crate::similar::config::SimilarConfig;
use crate::similar::errors::SimilarError;
use crate::similar::progress::SimilarProgress;

/// Decides which of two similar images is the "better" one (the one kept in
/// place) when a pair is presented.  The default heuristic is pixel area,
/// mirroring the duplo-rs demo app; the trait exists so the preference can
/// be swapped (e.g. for metadata-based or directory-based rules) without
/// touching the scan logic.
pub trait ImageQualityHeuristic {
    /// True when `candidate_a` is the better of the two.
    fn is_better(&self, candidate_a: &Path, candidate_b: &Path) -> bool;
}

/// Default: the image with the larger pixel area wins; ties and unreadable
/// sizes lose (the newer file is then preferred by the caller).
pub struct PixelAreaHeuristic;

impl ImageQualityHeuristic for PixelAreaHeuristic {
    fn is_better(&self, candidate_a: &Path, candidate_b: &Path) -> bool {
        match (imagesize::size(candidate_a), imagesize::size(candidate_b)) {
            (Ok(a), Ok(b)) => (a.width * a.height) > (b.width * b.height),
            _ => false,
        }
    }
}

/// Result of a completed image similarity scan.
#[derive(Debug)]
pub struct ImageScanResult {
    pub pairs_found: usize,
    pub files_scanned: usize,
}

/// Scan `scan_dir` for images similar to each other.
///
/// Runs synchronously; call from a background thread. Progress is sent to
/// `progress_tx`; set `cancel_flag` to stop between files (the store is
/// persisted either way, so the scan can be resumed).
pub fn run_image_scan(
    config: &SimilarConfig,
    scan_dir: &Path,
    progress_tx: &futures_mpsc::UnboundedSender<SimilarProgress>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<ImageScanResult, SimilarError> {
    if !scan_dir.is_dir() {
        return Err(SimilarError::InvalidScanDir(scan_dir.to_path_buf()));
    }
    let out_dir = config
        .output_dir
        .clone()
        .map(|dir| dir.join(SimilarKind::Image.output_dir_name()))
        .unwrap_or_else(|| scan_dir.join(SimilarKind::Image.output_dir_name()));
    std::fs::create_dir_all(&out_dir).map_err(|source| SimilarError::OutputDir {
        path: out_dir.clone(),
        source,
    })?;

    let filelist = list_media_files(scan_dir, config.recursive, SimilarKind::Image)?;

    let store_dir = data_dir()?;
    let storepath = store_dir.join(SimilarKind::Image.store_file_name());

    // duplo-rs's store helpers panic (e.g. store file unreadable) instead of
    // returning errors; a background-thread panic must not kill the app.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scan_images(config, &out_dir, &storepath, &filelist, progress_tx, cancel_flag)
    }));
    match result {
        Ok(result) => result,
        Err(payload) => {
            Err(SimilarError::Panic(panic_payload_to_string(&payload)))
        }
    }
}

fn scan_images(
    config: &SimilarConfig,
    out_dir: &Path,
    storepath: &Path,
    filelist: &[PathBuf],
    progress_tx: &futures_mpsc::UnboundedSender<SimilarProgress>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<ImageScanResult, SimilarError> {
    let storepath_str = storepath.to_string_lossy().to_string();
    let mut store = duplo_rs::store::Store::new(config.image_threshold());
    if storepath.is_file() {
        store.slurp_binary(&storepath_str);
    }

    let mut pairs_found = 0usize;
    for (i, file) in filelist.iter().enumerate() {
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = progress_tx.unbounded_send(SimilarProgress::Cancelled);
            store.dump_binary(&storepath_str);
            return Err(SimilarError::Cancelled);
        }

        let filepath = duplo_rs::files::osstring_to_string(file.as_os_str());

        if store.has(&filepath) {
            // Already scanned in a previous run (the store is persistent);
            // skipping also prevents a file from matching itself on a
            // re-scan of the same directory.
            let _ = progress_tx.unbounded_send(SimilarProgress::FileProgress {
                done: i + 1,
                total: filelist.len(),
                last_match: None,
            });
            continue;
        }

        let hash = duplo_rs::files::process_image(file);
        let (matches, failed_id, failed_hash) =
            duplo_rs::files::find_similar_images(&store, &filepath, &hash);

        if let Some(best) = matches.m.first() {
            // The first (best-scoring) match decides the pair, same as the
            // demo app. The quality heuristic decides which file is kept;
            // on a tie (heuristic returns false) the new file is treated as
            // the better one.
            let match_better = PixelAreaHeuristic
                .is_better(std::path::Path::new(&best.id), std::path::Path::new(&filepath));
            let out_dir_buf = out_dir.to_path_buf();
            if match_better {
                // The stored file is the better one: move the new file out,
                // keep the stored store entry as-is.
                duplo_rs::files::present_pairs(&out_dir_buf, &filepath, &best.id);
            } else {
                // The new file is the better one: move the stored file out
                // and replace the store entry.
                duplo_rs::files::present_pairs(&out_dir_buf, &best.id, &filepath);
                store.delete(&best.id);
                store.add(&filepath, &hash);
            }
            pairs_found += 1;
            let _ = progress_tx.unbounded_send(SimilarProgress::PairCreated {
                path: out_dir.to_path_buf(),
            });
        } else {
            // No match: remember this image for future runs.
            store.add(&failed_id, &failed_hash);
        }

        let _ = progress_tx.unbounded_send(SimilarProgress::FileProgress {
            done: i + 1,
            total: filelist.len(),
            last_match: None,
        });
    }
    store.dump_binary(&storepath_str);
    Ok(ImageScanResult {
        pairs_found,
        files_scanned: filelist.len(),
    })
}
