//! Video similarity scan (wraps `duplo_rs::videostore::VideoStore`).
//!
//! Pipeline:
//!
//! 1. **Parse** — a bounded pool of at most `video_num_threads` workers
//!    runs the ffmpeg screenshot extraction (`duplo_rs::files::process_video`);
//!    results are collected directly from the join handles.
//! 2. **Commit** — every parsed video is added to the store serially
//!    (SQLite write contention stays at one).  Renamed copies of content
//!    that is already stored are reported, not added.
//! 3. **Match** — each newly committed video is queried against the full
//!    store (its own entry filtered out); matches become one HTML report
//!    per video in `similar_videos/`.
//!
//! Videos already in the store are skipped, so the scan can be cancelled
//! and resumed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use futures::channel::mpsc as futures_mpsc;

use super::{
    data_dir, ffmpeg_available, list_media_files, panic_payload_to_string,
    SimilarKind,
};
use crate::similar::config::SimilarConfig;
use crate::similar::errors::SimilarError;
use crate::similar::progress::SimilarProgress;

/// Result of a completed video similarity scan.
#[derive(Debug)]
pub struct VideoScanResult {
    pub html_files: usize,
    pub files_scanned: usize,
}

/// Scan `scan_dir` for videos similar to each other.
///
/// Requires `ffmpeg` on `$PATH`. Runs synchronously; call from a background
/// thread. Progress is sent to `progress_tx`; set `cancel_flag` to stop
/// (already-parsed videos stay in the store, so the scan can be resumed).
pub fn run_video_scan(
    config: &SimilarConfig,
    scan_dir: &Path,
    progress_tx: &futures_mpsc::UnboundedSender<SimilarProgress>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<VideoScanResult, SimilarError> {
    if !scan_dir.is_dir() {
        return Err(SimilarError::InvalidScanDir(scan_dir.to_path_buf()));
    }
    if !ffmpeg_available() {
        return Err(SimilarError::FfmpegMissing);
    }
    let out_dir = config
        .output_dir
        .clone()
        .map(|dir| dir.join(SimilarKind::Video.output_dir_name()))
        .unwrap_or_else(|| scan_dir.join(SimilarKind::Video.output_dir_name()));
    std::fs::create_dir_all(&out_dir).map_err(|source| SimilarError::OutputDir {
        path: out_dir.clone(),
        source,
    })?;

    let filelist = list_media_files(scan_dir, config.recursive, SimilarKind::Video)?;
    let dbpath = data_dir()?.join(SimilarKind::Video.store_file_name());
    let dbpath_str = dbpath.to_string_lossy().to_string();

    // duplo-rs panics on internal errors (e.g. store file unreadable)
    // instead of returning them; a background-thread panic must not kill
    // the app.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        scan_videos(
            config,
            scan_dir,
            &out_dir,
            &dbpath_str,
            &filelist,
            progress_tx,
            cancel_flag,
        )
    }));
    match result {
        Ok(result) => result,
        Err(payload) => {
            Err(SimilarError::Panic(panic_payload_to_string(&payload)))
        }
    }
}

fn scan_videos(
    config: &SimilarConfig,
    scan_dir: &Path,
    out_dir: &Path,
    dbpath: &str,
    filelist: &[PathBuf],
    progress_tx: &futures_mpsc::UnboundedSender<SimilarProgress>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<VideoScanResult, SimilarError> {
    // Connect + create the store before any worker touches it.  The
    // connection type belongs to duplo-rs's own rusqlite; it never crosses
    // a named boundary here.
    let connection = duplo_rs::videostore::connect(dbpath)
        .map_err(|err| SimilarError::VideoDb(err.to_string()))?;
    let start_directory = scan_dir.to_string_lossy().to_string();
    let store = Arc::new(duplo_rs::videostore::VideoStore::new(
        connection,
        config.video_threshold(),
        &start_directory,
        config.video_num_threads_clamped(),
        config.video_screenshot_interval_clamped(),
        config.video_min_sequence,
        config.video_max_cache,
    ));

    let total = filelist.len();
    let mut done = 0usize;
    let mut html_files = 0usize;
    let num_workers = config.video_num_threads_clamped() as usize;
    let cancelled = || cancel_flag.load(Ordering::SeqCst) || store.stop_requested();

    let starting_candidate = store.num_candidates();
    let mut video_id_counter = starting_candidate + 1;

    let filelist = Arc::new(filelist.to_vec());

    // ------------------------------------------------------------------
    // Producer: walk the file list, spawn bounded ffmpeg workers, and
    // collect their results via the join handles.  (A bounded channel
    // relay to a consumer stage used to silently drop every parsed video
    // that did not fit the buffer, so scans with more than
    // `2 * video_num_threads` files found nothing.)
    // ------------------------------------------------------------------
    let mut parsed: Vec<(u32, duplo_rs::videocandidate::VideoCandidate)> =
        Vec::new();
    {
        let filelist = filelist.clone();
        let store = store.clone();
        let mut handles: Vec<
            std::thread::JoinHandle<Option<(u32, duplo_rs::videocandidate::VideoCandidate)>>,
        > = Vec::new();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = num_workers;
        let mut filepos = 0usize;

        loop {
            if cancelled() || filepos >= filelist.len() {
                break;
            }
            // Wait for a free slot before spawning.
            while in_flight.load(Ordering::SeqCst) >= max_in_flight {
                if cancelled() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            if cancelled() {
                break;
            }

            let filepath = filelist[filepos].clone();
            let filestring = duplo_rs::files::osstring_to_string(filepath.as_os_str());
            let next_pos = filepos + 1;
            filepos = next_pos;

            if store.ids_lookup(&filestring).is_some() {
                // Already scanned in a previous run.
                done += 1;
                let _ = progress_tx.unbounded_send(SimilarProgress::FileProgress {
                    done,
                    total,
                    last_match: None,
                });
                continue;
            }

            let video_id = video_id_counter;
            video_id_counter += 1;
            let num_seconds = store.num_seconds_between_screenshots();
            let inflight = in_flight.clone();
            let handle = std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    duplo_rs::files::process_video(
                        &filepath,
                        video_id as usize,
                        total as u32,
                        num_seconds,
                        next_pos as u32,
                    )
                }));
                let outcome = match result {
                    Ok(video) => Some((video_id, video)),
                    Err(payload) => {
                        log::error!(
                            "similar scan: ffmpeg worker for {} panicked: {}",
                            filepath.display(),
                            panic_payload_to_string(&payload)
                        );
                        None
                    }
                };
                inflight.fetch_sub(1, Ordering::SeqCst);
                outcome
            });
            in_flight.fetch_add(1, Ordering::SeqCst);
            handles.push(handle);
        }
        for handle in handles {
            if let Ok(Some(item)) = handle.join() {
                parsed.push(item);
            }
            // Progress moves during the (slow) screenshot-extraction phase,
            // not only after it.
            done += 1;
            let _ = progress_tx.unbounded_send(SimilarProgress::FileProgress {
                done,
                total,
                last_match: None,
            });
        }
    }
    parsed.sort_by_key(|(video_id, _)| *video_id);

    // ------------------------------------------------------------------
    // Stage 2 — commit the parsed videos to the store serially.
    // Committing before matching (instead of querying while parsing is in
    // flight) guarantees that two similar videos added in the same run are
    // both visible when they are compared.
    // ------------------------------------------------------------------

    let mut new_videos: Vec<duplo_rs::videocandidate::VideoCandidate> =
        Vec::new();
    for (_video_id, video) in parsed.iter() {
        if cancelled() {
            store.request_stop();
            let _ = progress_tx.unbounded_send(SimilarProgress::Cancelled);
            return Err(SimilarError::Cancelled);
        }

        // Renamed copy: the same content is already stored under a
        // different filename.  Don't add a duplicate store entry (plan
        // §4.5), but still report the match — the video is new to this
        // scan, and its report compares it against the stored original.
        // (Files already in the store were never parsed: the producer
        // skips them before spawning a worker.)
        if let Some(existing_id) = store
            .content_fingerprint_lookup(video)
            .filter(|existing_id| *existing_id != video.id)
        {
            log::info!(
                "similar scan: {} is a renamed copy of {} — not adding a duplicate store entry",
                video.id,
                existing_id
            );
            new_videos.push(video.clone());
            continue;
        }
        store.add(&video.id, video, video.runtime);
        new_videos.push(video.clone());
    }

    // ------------------------------------------------------------------
    // Stage 3 — match the newly committed videos against the full store.
    // The video's own store entry is excluded from the comparison set.
    // ------------------------------------------------------------------
    for video in new_videos.iter() {
        if cancelled() {
            store.request_stop();
            let _ = progress_tx.unbounded_send(SimilarProgress::Cancelled);
            return Err(SimilarError::Cancelled);
        }
        let matches = store.query(video);
        let others: Vec<_> = matches
            .m
            .iter()
            .filter(|match_item| match_item.id != video.id)
            .cloned()
            .collect();
        if others.is_empty() {
            continue;
        }

        let mut compare: Vec<duplo_rs::videocandidate::VideoCandidate> =
            Vec::new();
        compare.push(video.clone());
        for match_item in others.iter() {
            if let Some(index) = store.ids_lookup(&match_item.id) {
                let (_, candidate) = store.return_candidate(index as u32);
                if !candidate.id.is_empty() {
                    compare.push(candidate);
                }
            }
        }
        if compare.len() >= 2 {
            log::info!(
                "similar scan: {} is similar to: {}",
                video.id,
                compare
                    .iter()
                    .skip(1)
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let out_dir_buf = out_dir.to_path_buf();
            duplo_rs::files::present_video_matches(&out_dir_buf, &compare);
            html_files += 1;
            let _ = progress_tx.unbounded_send(SimilarProgress::PairCreated {
                path: out_dir.to_path_buf(),
            });
        }
    }

    Ok(VideoScanResult {
        html_files,
        files_scanned: total,
    })
}
