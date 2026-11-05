//! Similarity search for images and videos, built on the `duplo-rs` engine.
//!
//! Two user-facing flows:
//! - **Images** ([`run_image_scan`]): near-duplicate pairs are written to a
//!   `similar_images/` directory. The "better" file stays in place and is
//!   represented by a `*_KEEP_*` hardlink; the "worse" file is *moved* to
//!   `*_REMOVE_*`. Deleting the pair accepts, restoring `_REMOVE_` rejects.
//! - **Videos** ([`run_video_scan`]): one self-contained HTML report per
//!   matching video is written to `similar_videos/` for manual review in a
//!   browser.
//!
//! Both scans are **incremental and resumable**: the duplo stores persist
//! in `~/.local/share/media-browser/` across runs, so a stopped scan can be
//! resumed by starting it again.

pub mod config;
pub mod errors;
pub mod image_scan;
pub mod progress;
pub mod video_scan;

pub use config::SimilarConfig;
pub use errors::SimilarError;
pub use image_scan::{
    run_image_scan, ImageQualityHeuristic, ImageScanResult, PixelAreaHeuristic,
};
pub use progress::SimilarProgress;
pub use video_scan::{run_video_scan, VideoScanResult};

use std::path::{Path, PathBuf};

/// The kind of media a similarity scan looks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimilarKind {
    Image,
    Video,
}

impl SimilarKind {
    /// Recognized file extensions (lowercase, without dot).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Image => &[
                "png", "jpg", "jpeg", "bmp", "gif", "webp", "tif", "tiff",
            ],
            Self::Video => &["mkv", "mp4", "avi", "mov", "webm"],
        }
    }

    /// Name of the per-scan output directory (`similar_images/` or
    /// `similar_videos/`).
    pub fn output_dir_name(self) -> &'static str {
        match self {
            Self::Image => "similar_images",
            Self::Video => "similar_videos",
        }
    }

    /// File name of the persistent store inside the data directory.
    pub fn store_file_name(self) -> &'static str {
        match self {
            Self::Image => "similar_images.store",
            Self::Video => "similar_videos.sqlite3",
        }
    }

    /// Human-readable plural (for progress text), i18n key argument.
    pub fn media_name(self) -> String {
        match self {
            Self::Image => crate::fl!("similar-media-images"),
            Self::Video => crate::fl!("similar-media-videos"),
        }
    }
}

/// The app data directory (`~/.local/share/media-browser`), created if
/// missing. Holds the persistent similarity stores (same base as the
/// thumbnails directory).
pub fn data_dir() -> Result<PathBuf, std::io::Error> {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("media-browser");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Collect media files under `scan_dir` (recursively if `recursive`).
///
/// Hidden files/directories and `.gitignore` rules are respected
/// (`ignore` crate defaults). The scan's own output directories
/// (`similar_images/`, `similar_videos/`) are pruned so re-runs never match
/// against their own results. Unreadable entries are logged and skipped.
pub fn list_media_files(
    scan_dir: &Path,
    recursive: bool,
    kind: SimilarKind,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let extensions = kind.extensions();
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(scan_dir)
        .standard_filters(true)
        // max_depth(Some(1)) = the directory itself only (non-recursive).
        .max_depth(if recursive { None } else { Some(1) })
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some("similar_images" | "similar_videos")
            )
        })
        .build()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                log::warn!("similar scan: cannot access entry: {err}");
                continue;
            }
        };
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.path();
        let is_media = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext.to_ascii_lowercase().as_str()));
        if !is_media {
            continue;
        }
        match std::fs::canonicalize(path) {
            Ok(canonical) => out.push(canonical),
            Err(err) => {
                log::warn!(
                    "similar scan: cannot resolve path {}: {err}",
                    path.display()
                );
            }
        }
    }
    out.sort();
    Ok(out)
}

/// True when an executable `ffmpeg` is found on `$PATH`.
///
/// Cheap (no process spawn) so it can be checked every time a context menu
/// is built.
pub fn ffmpeg_available() -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("ffmpeg");
        let Ok(metadata) = std::fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 != 0 {
                return true;
            }
        }
        #[cfg(not(unix))]
        {
            return true;
        }
    }
    false
}

/// Render a panic payload into a loggable string.
pub(crate) fn panic_payload_to_string(
    payload: &Box<dyn std::any::Any + Send>,
) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("unknown panic")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The end-to-end tests mutate process-global environment
    /// (XDG_DATA_HOME, PATH) and must not run in parallel.
    static E2E_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn list_media_files_respects_depth_extensions_and_output_dirs() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join("similar_images")).unwrap();
        for name in [
            "top.png",
            "top.txt",
            "sub/nested.jpg",
            "similar_images/inside.png",
        ] {
            std::fs::write(root.join(name), b"x").unwrap();
        }

        let flat = list_media_files(root, false, SimilarKind::Image).unwrap();
        assert_eq!(flat, vec![root.join("top.png").canonicalize().unwrap()]);

        let deep = list_media_files(root, true, SimilarKind::Image).unwrap();
        assert_eq!(
            deep,
            vec![
                root.join("sub/nested.jpg").canonicalize().unwrap(),
                root.join("top.png").canonicalize().unwrap(),
            ]
        );

        // Video extension set must not leak into image results.
        std::fs::write(root.join("clip.mp4"), b"x").unwrap();
        assert!(list_media_files(root, true, SimilarKind::Image)
            .unwrap()
            .iter()
            .all(|p| p.extension().is_some_and(|e| e != "mp4")));
        let videos = list_media_files(root, false, SimilarKind::Video).unwrap();
        assert_eq!(videos, vec![root.join("clip.mp4").canonicalize().unwrap()]);
    }

    /// End-to-end scan flows. Combined into one test because the image and
    /// video scans share the process-global environment (XDG_DATA_HOME,
    /// PATH), which must not be mutated in parallel.
    #[test]
    fn e2e_image_and_video_scans() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::os::unix::fs::MetadataExt;

        let _env_guard = E2E_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Point the persistent stores at a throwaway data directory.
        let data_home = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_DATA_HOME", data_home.path()) };
        let original_path = std::env::var_os("PATH").expect("PATH set in test env");

        // ffmpeg_available() must follow PATH.
        let empty = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("PATH", empty.path()) };
        assert!(!ffmpeg_available());
        let bin = tempfile::tempdir().unwrap();
        let fake = bin.path().join("ffmpeg");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))
            .unwrap();
        unsafe { std::env::set_var("PATH", bin.path()) };
        assert!(ffmpeg_available());
        unsafe { std::env::set_var("PATH", original_path) };

        let (progress_tx, progress_rx) =
            futures::channel::mpsc::unbounded::<SimilarProgress>();
        let cancel = Arc::new(AtomicBool::new(false));

        // ------------------------------------------------------------------
        // Image scan: one duplicate pair, one unrelated image.
        // ------------------------------------------------------------------
        let scan = tempfile::tempdir().unwrap();
        let scan = scan.path();
        let pattern = image::RgbaImage::from_fn(128, 128, |x, y| {
            image::Rgba([(x * 3) as u8, (y * 5) as u8, ((x + y) / 2) as u8, 255])
        });
        pattern.save(scan.join("base.png")).unwrap();
        std::fs::copy(scan.join("base.png"), scan.join("duplicate.png")).unwrap();
        image::RgbaImage::from_pixel(128, 128, image::Rgba([10, 20, 30, 255]))
            .save(scan.join("solid.png"))
            .unwrap();

        let config = SimilarConfig {
            image_sensitivity: 100, // duplo demo default (-60)
            ..Default::default()
        };
        let first = image_scan::run_image_scan(
            &config,
            scan,
            &progress_tx,
            cancel.clone(),
        )
        .expect("image scan run 1");
        assert_eq!(first.pairs_found, 1);
        assert_eq!(first.files_scanned, 3);

        let out_dir = scan.join("similar_images");
        let pair: Vec<_> = std::fs::read_dir(&out_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(pair.len(), 2);
        // Same random 5-char prefix so the pair sorts together.
        assert_eq!(pair[0].get(..5), pair[1].get(..5));
        assert!(pair.iter().any(|n| n.contains("_KEEP_")));
        assert!(pair.iter().any(|n| n.contains("_REMOVE_")));
        // Exactly one of the two originals remains in the scan dir.
        let base_survives = scan.join("base.png").exists();
        let dup_survives = scan.join("duplicate.png").exists();
        assert!(base_survives ^ dup_survives);
        let survivor = if base_survives {
            scan.join("base.png")
        } else {
            scan.join("duplicate.png")
        };
        let keep = out_dir
            .join(pair.iter().find(|n| n.contains("KEEP")).unwrap().clone());
        let remove = out_dir
            .join(pair.iter().find(|n| n.contains("REMOVE")).unwrap().clone());
        // KEEP is a hardlink to the surviving original; deleting it is
        // harmless.
        let keep_meta = std::fs::metadata(&keep).unwrap();
        assert!(keep_meta.nlink() >= 2);
        assert_eq!(
            std::fs::read(&keep).unwrap(),
            std::fs::read(&survivor).unwrap()
        );
        // REMOVE is the moved original (identical content here).
        assert_eq!(std::fs::read(&remove).unwrap(), std::fs::read(&keep).unwrap());
        // The persistent store survived in the throwaway data directory.
        let data_dir = data_dir().unwrap();
        assert_eq!(data_dir, data_home.path().join("media-browser"));
        assert!(data_dir.join(SimilarKind::Image.store_file_name()).is_file());

        // Re-scan: both files are already stored -> no new pairs.
        let second = image_scan::run_image_scan(
            &config,
            scan,
            &progress_tx,
            cancel.clone(),
        )
        .expect("image scan run 2");
        assert_eq!(second.pairs_found, 0);
        let still = std::fs::read_dir(&out_dir).unwrap().count();
        assert_eq!(still, 2);

        // Cancellation is surfaced as an error, not a panic.
        cancel.store(true, Ordering::SeqCst);
        let cancelled = image_scan::run_image_scan(&config, scan, &progress_tx, cancel.clone());
        assert!(matches!(cancelled, Err(SimilarError::Cancelled)));
        cancel.store(false, Ordering::SeqCst);

        // ------------------------------------------------------------------
        // Video scan: two identical clips -> one HTML report.
        //
        // The duplicate pair is NOT the first two files in sort order: an
        // early revision handed parsed videos from a worker pool to the
        // commit stage through a 2-slot channel and silently dropped every
        // video beyond the first two, so scans with more than two files
        // found nothing.
        // ------------------------------------------------------------------
        let ffmpeg_ok = std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !ffmpeg_ok {
            eprintln!("ffmpeg not available; skipping video e2e assertions");
            return;
        }

        let vscan = tempfile::tempdir().unwrap();
        let vscan = vscan.path();
        let make_clip = |vscan: &std::path::Path, name: &str, filter: &str| {
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("{filter}=duration=1:size=64x64:rate=10"),
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(vscan.join(name))
                .status()
                .expect("ffmpeg spawn");
            assert!(status.success(), "ffmpeg generation failed for {name}");
        };
        // Two unrelated clips first (sort order), then the duplicate pair.
        make_clip(vscan, "aaa.mp4", "smptebars");
        make_clip(vscan, "bbb.mp4", "rgbtestsrc");
        make_clip(vscan, "pair_a.mp4", "testsrc");
        make_clip(vscan, "pair_b.mp4", "testsrc");

        let vconfig = SimilarConfig {
            video_sensitivity: 100, // duplo demo default (-20)
            video_screenshot_interval: 1,
            video_min_sequence: 1,
            video_num_threads: 1,
            video_max_cache: 10,
            ..Default::default()
        };
        let first = video_scan::run_video_scan(
            &vconfig,
            vscan,
            &progress_tx,
            cancel.clone(),
        )
        .expect("video scan run 1");
        assert_eq!(first.html_files, 1);

        let html = vscan.join("similar_videos").join("pair_b.html");
        let body = std::fs::read_to_string(&html).expect("html report missing");
        assert!(body.contains("Comparing similar videos for pair_b"));
        assert!(body.contains("pair_a.mp4"));
        assert!(body.contains("pair_b.mp4"));
        assert!(body.contains("<video"));
        // The unrelated clips must not have produced reports either.
        assert!(!vscan.join("similar_videos").join("aaa.html").exists());
        assert!(!vscan.join("similar_videos").join("bbb.html").exists());

        // Re-scan: aaa, bbb and pair_a are already stored (skipped).
        // pair_b is a renamed copy of pair_a, so it is not added again,
        // but it is still reported — its HTML is regenerated (same name).
        let second = video_scan::run_video_scan(
            &vconfig,
            vscan,
            &progress_tx,
            cancel.clone(),
        )
        .expect("video scan run 2");
        assert_eq!(second.html_files, 1);
        let still = std::fs::read_dir(vscan.join("similar_videos"))
            .unwrap()
            .count();
        assert_eq!(still, 1);
        // Both runs delivered their progress over the channel.  (Drained
        // synchronously with a no-op waker; the live sender keeps the
        // channel Pending once the buffer is empty.)
        use futures::Stream as _;
        let mut file_progresses = 0usize;
        let mut pair_creates = 0usize;
        let mut rx = progress_rx;
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            let mut pinned = std::pin::Pin::new(&mut rx);
            match pinned.as_mut().poll_next(&mut cx) {
                std::task::Poll::Ready(Some(msg)) => match msg {
                    SimilarProgress::FileProgress { .. } => file_progresses += 1,
                    SimilarProgress::PairCreated { .. } => pair_creates += 1,
                    _ => {}
                },
                std::task::Poll::Ready(None) | std::task::Poll::Pending => break,
            }
        }
        assert!(file_progresses >= 8, "each run must report all 4 files");
        assert!(pair_creates >= 2);
    }

    /// Regression: a scan over real (46-minute) MKV episodes whose
    /// duplicate pair is NOT the first two files in sort order must still
    /// report that pair (the old channel relay dropped every parsed video
    /// beyond the first two, so such scans found nothing).
    /// Skipped when the local test library is not present (CI-safe).
    #[test]
    fn real_videos_scan_finds_late_duplicate_pair() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        let _env_guard = E2E_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let lib = std::path::Path::new(
            "testsamples/Supernatural (2005)/Season 1",
        );
        let src_pilot = lib.join("Supernatural - S01E01 - Pilot.mkv");
        let src_wendigo = lib.join("Supernatural - S01E02 - Wendigo.mkv");
        let src_water = lib.join("Supernatural - S01E03 - Dead in the Water.mkv");
        if ![&src_pilot, &src_wendigo, &src_water].iter().all(|p| p.is_file()) {
            eprintln!("real_videos_scan_finds_late_duplicate_pair: test library missing, skipping");
            return;
        }
        if !ffmpeg_available() {
            eprintln!("real_videos_scan_finds_late_duplicate_pair: ffmpeg missing, skipping");
            return;
        }

        let data_home = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("XDG_DATA_HOME", data_home.path()) };

        let scan = tempfile::tempdir().unwrap();
        let scan = scan.path();
        // Sort order: aaa, bbb, dup_a, dup_b — the duplicate pair is last.
        for (src, name) in [
            (&src_wendigo, "aaa.mkv"),
            (&src_water, "bbb.mkv"),
            (&src_pilot, "dup_a.mkv"),
            (&src_pilot, "dup_b.mkv"),
        ] {
            std::fs::copy(src, scan.join(name)).unwrap();
        }

        let (progress_tx, progress_rx) =
            futures::channel::mpsc::unbounded::<SimilarProgress>();
        let cancel = Arc::new(AtomicBool::new(false));

        // Stricter than the default so unrelated episodes don't blur
        // together (sensitivity 0 = duplo demo threshold -120).
        let config = SimilarConfig {
            video_sensitivity: 0,
            ..Default::default()
        };
        let result = video_scan::run_video_scan(
            &config,
            scan,
            &progress_tx,
            cancel.clone(),
        );
        match &result {
            Ok(r) => println!(
                "real_videos: Ok(html_files={}, files_scanned={})",
                r.html_files, r.files_scanned
            ),
            Err(e) => println!("real_videos: Err({e})"),
        }

        let out_dir = scan.join("similar_videos");
        if out_dir.is_dir() {
            let reports: Vec<String> = std::fs::read_dir(&out_dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            println!("real_videos: reports: {reports:?}");
        }

        let mut counts = (0usize, 0usize);
        let mut rx = progress_rx;
        use futures::Stream as _;
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            let mut pinned = std::pin::Pin::new(&mut rx);
            match pinned.as_mut().poll_next(&mut cx) {
                std::task::Poll::Ready(Some(msg)) => match msg {
                    SimilarProgress::FileProgress { .. } => counts.0 += 1,
                    SimilarProgress::PairCreated { .. } => counts.1 += 1,
                    m => println!("real_videos: progress {m:?}"),
                },
                std::task::Poll::Ready(None) | std::task::Poll::Pending => break,
            }
        }
        println!(
            "real_videos: file_progresses={} pair_creates={}",
            counts.0, counts.1
        );

        let result = result.expect("video scan");
        // Every file must reach the commit stage (the old channel relay
        // dropped all but the first two).
        assert_eq!(counts.0, 4, "all four files must be processed");
        // The true duplicate pair must be reported.
        let html = out_dir.join("dup_b.html");
        let body = std::fs::read_to_string(&html).expect("dup_b.html report missing");
        assert!(body.contains("Comparing similar videos for dup_b"));
        assert!(body.contains("dup_a.mkv"));
        // Note: unrelated episodes may also be reported — the engine
        // flags any ~1 minute of shared content (e.g. the series intro),
        // which is tunable via the sensitivity setting.
    }
}
