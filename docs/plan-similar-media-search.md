# Implementation Plan: Similar Image & Video Search

## Overview

Integrate the `duplo-rs` similarity engine into media-browser, providing two distinct
user-facing workflows that mirror the behavior of the standalone `duplo-rs` demo apps:

| Media type | Result presentation | User decision model |
|---|---|---|
| **Images** | Pairs written to a `similar_images/` directory. The "better" file is a **hardlink** (`*_KEEP_*`); the "worse" file is **moved** (`*_REMOVE_*`). The user reviews pairs and deletes the whole pair (or the `_REMOVE_` file) to accept, or restores `_REMOVE_` to reject. | Deterministic: the engine picks the winner. |
| **Videos** | An **HTML file** per new video written to `similar_videos/<name>.html` containing `<video>` preview elements, file paths, resolution, duration, and metadata for every matched video. | Non-deterministic: the user reviews the HTML and makes the final call. |

Both flows are **incremental and resumable**: the duplo store persists across runs, so the
user can stop a long scan and continue later.

---

## 1. Dependency Integration

### 1.1 Add `duplo-rs` to `Cargo.toml`

```rust
// Cargo.toml  [dependencies]
duplo-rs = { git = "https://github.com/fangornsrealm/duplo-rs.git" }
```

Notes:
- `duplo-rs` brings its own `rusqlite` (bundled) and `image` 0.25.x — both are already
  in the media-browser dependency tree, so the resolver will unify them.
- `duplo-rs` depends on `walkdir`, `imagesize`, `base64`, `build_html`, `hamming_rs`,
  and `rand`.  `walkdir` and `imagesize` are already present.  The rest are small
  leaf crates with no conflicts.
- **ffmpeg** must be available on `$PATH` at runtime for video screenshot extraction.
  This is already a soft requirement (gstreamer plugins for playback) and can be
  surfaced as a startup warning if `ffmpeg` is not found.

### 1.2 Version pinning / lock

Pin to a specific commit SHA of `duplo-rs` in `Cargo.lock` (or a tag once available)
so builds are reproducible.  Add a comment in `Cargo.toml`:

```rust
// duplo-rs: similarity engine.  Pin to a release tag when 1.0 is published.
duplo-rs = { git = "https://github.com/fangornsrealm/duplo-rs.git", rev = "abcdef0" }
```

---

## 2. New Module: `src/similar/`

Create a dedicated module tree to keep the integration isolated and testable:

```
src/similar/
├── mod.rs            # Public API: SimilarScan, SimilarConfig, run_image_scan, run_video_scan
├── config.rs         # SimilarConfig (sensitivity, screenshot interval, sequence length, etc.)
├── image_scan.rs     # Image similarity scan logic (wraps duplo_rs::store::Store)
├── video_scan.rs     # Video similarity scan logic (wraps duplo_rs::videostore::VideoStore)
├── progress.rs       # Progress report types sent back to the UI thread
└── errors.rs         # SimilarError enum (thiserror)
```

### 2.1 `SimilarConfig`

```rust
// src/similar/config.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimilarConfig {
    /// Image sensitivity: 0 (most strict) .. 100 (most permissive).
    /// Internally mapped to duplo's `f64` threshold (default -60).
    pub image_sensitivity: u8,        // default 50

    /// Video sensitivity: 0 .. 100.  (default -20 internally)
    pub video_sensitivity: u8,        // default 50

    /// Seconds between video screenshots.  (default 10)
    pub video_screenshot_interval: u32,

    /// Minimum consecutive matching screenshots for a video match.  (default 6)
    pub video_min_sequence: u32,

    /// Number of threads for video screenshot extraction.  (default 1)
    pub video_num_threads: u32,

    /// Max candidates kept in the video comparison cache.  (default 100)
    pub video_max_cache: usize,

    /// Base directory under which `similar_images/` and `similar_videos/` are created.
    /// Defaults to the scanned directory.
    pub output_dir: Option<PathBuf>,

    /// Whether to recurse into subdirectories.  (default false)
    pub recursive: bool,
}

impl Default for SimilarConfig { /* … */ }
```

Persist `SimilarConfig` in the existing `Config` struct (see §5) so the user's
preferences survive app restarts.

### 2.2 Progress channel

Background scans run on a dedicated `std::thread` (or a `tokio::task::spawn_blocking`
group) and report progress back through a bounded `std::sync::mpsc::Sender`:

```rust
// src/similar/progress.rs
#[derive(Debug, Clone)]
pub enum SimilarProgress {
    /// Scanned N of Total files.  Optionally include the most recent match description.
    FileProgress { done: usize, total: usize, last_match: Option<String> },
    /// A new pair / HTML file was written.
    PairCreated { path: PathBuf },
    /// The scan finished.
    Complete { pairs_found: usize, html_files: usize },
    /// A fatal or per-file error (non-fatal; scan continues).
    Warning { message: String },
    /// The scan was cancelled by the user.
    Cancelled,
}
```

The UI subscribes via `cosmic::app::Task::perform` + a `mpsc::Receiver` polled in a
`Subscription`, or more simply via a `tokio::sync::mpsc` channel whose receiver is
drained in a dedicated `Task` that re-dispatches `Message::SimilarProgress(…)`.

---

## 3. Image Similarity Scan

### 3.1 Core function

```rust
// src/similar/image_scan.rs
pub fn run_image_scan(
    config: &SimilarConfig,
    scan_dir: &Path,
    progress_tx: &mpsc::Sender<SimilarProgress>,
    cancel_flag: &AtomicBool,
) -> Result<ImageScanResult, SimilarError>
```

**Algorithm** (mirrors `demo_similar_images`):

1. **Build file list** — walk `scan_dir` (recursively if `config.recursive`),
   collecting image extensions: `png, jpg, jpeg, bmp, gif, webp, tif, tiff`.
   Reuse `ignore` crate or `walkdir` (already a dependency) to respect
   `.gitignore`-style rules and skip hidden dirs.

2. **Load / create store** —
   - Store path: `<data_local_dir>/media-browser/similar_images.store`
     (or `scan_dir.join("similar_images.store")` if the user prefers local).
   - If the file exists, `store.slurp_binary(&path)`.
   - Otherwise `duplo_rs::store::Store::new(sensitivity_f64)`.

3. **Create output directory** — `scan_dir/similar_images/` (or
   `config.output_dir` if set).  `fs::create_dir_all`.

4. **Process loop** (one file at a time, respecting `cancel_flag`):
   ```
   for file in filelist:
       if cancel_flag.load(): break;
       hash = duplo_rs::files::process_image(&file);
       (matches, failed_id, failed_hash) = duplo_rs::files::find_similar_images(&store, &path, &hash);
       for match in matches:
           // Determine "better" by pixel count (w*h), matching demo behavior
           if match_size > source_size:
               duplo_rs::files::present_pairs(&out_dir, &source_path, &match_path);
           else:
               duplo_rs::files::present_pairs(&out_dir, &match_path, &source_path);
               store.delete(&match_path);
               store.add(&source_path, &hash);
               break;
       if matches.is_empty():
           store.add(&failed_id, &failed_hash);
       progress_tx.send(FileProgress { done, total, … });
   ```

5. **Persist store** — `store.dump_binary(&store_path)`.

6. **Report** — `Complete { pairs_found, … }`.

### 3.2 "Better image" heuristic

The demo uses **pixel count** (width × height).  This is simple and effective.
The plan keeps this as the default but structures the code so the heuristic
can be swapped (e.g. prefer the file with more metadata, or the one in a
"primary" directory).  Define a trait:

```rust
pub trait ImageQualityHeuristic {
    fn is_better(&self, candidate_a: &Path, candidate_b: &Path) -> bool;
}

/// Default: larger pixel area wins.
pub struct PixelAreaHeuristic;
impl ImageQualityHeuristic for PixelAreaHeuristic { /* imagesize::size → w*h */ }
```

### 3.3 Output directory contents

After a scan, `similar_images/` contains pairs like:

```
a3Kx9_REMOVE_cat_original.jpg
a3Kx9_KEEP_cat_original.jpg      ← hardlink (same inode as original)
b7Pz2_REMOVE_puppy.png
b7Pz2_KEEP_puppy.png
```

- `KEEP` = hardlink to the file the engine decided to retain.  Deleting the
  hardlink does **not** remove the original.
- `REMOVE` = the **original file, moved** here.  Deleting it is a real deletion.
- Same random 5-char prefix → files sort together in any file manager.

The user reviews in a file manager or in media-browser's own tab pointed at
`similar_images/`.  To **accept** a pair: delete both files.  To **reject**:
move the `_REMOVE_` file back to its original location (or delete only the
`_KEEP_` hardlink, which is harmless).

### 3.4 Resume / restart

Because the store is persisted to disk, a re-run of the scan will:
- Skip files already in the store (they will have zero new matches).
- Continue comparing new files against the accumulated hash set.

This is identical to the demo app's behavior and is critical for collections
of millions of images where a full scan takes weeks.

---

## 4. Video Similarity Scan

### 4.1 Core function

```rust
// src/similar/video_scan.rs
pub fn run_video_scan(
    config: &SimilarConfig,
    scan_dir: &Path,
    progress_tx: &mpsc::Sender<SimilarProgress>,
    cancel_flag: &AtomicBool,
) -> Result<VideoScanResult, SimilarError>
```

**Algorithm** (mirrors `demo_similar_videos`, simplified for single-threaded
startup but preserving the producer → query → writer pipeline):

1. **Build file list** — walk `scan_dir`, collect `mkv, mp4, avi, mov, webm`.

2. **Connect / create video store** —
   - DB path: `<data_local_dir>/media-browser/similar_videos.sqlite3`
   - `duplo_rs::videostore::connect(&db_path)`
   - `duplo_rs::videostore::VideoStore::new(client, sensitivity, &dir, threads, interval, min_seq, max_cache)`

3. **Create output directory** — `scan_dir/similar_videos/`.

4. **Process loop** (bounded concurrency via `num_threads`):
   ```
   // Producer: spawn ffmpeg workers (bounded by num_threads)
   //   Each worker calls duplo_rs::files::process_video(...)
   //   which extracts screenshots via ffmpeg, hashes them, and returns
   //   a VideoCandidate.

   // Query: for each new VideoCandidate, call store.query(&video)
   //   which returns VideoMatches.

   // Writer (serial): for each (video, matches):
   //   - Build a `compare: Vec<VideoCandidate>` (new video + all matches)
   //   - duplo_rs::files::present_video_matches(&out_dir, &compare)
   //     → writes <stem>.html with <video> tags, file paths, resolution, duration
   //   - store.add(&video.id, &video, video.runtime)
   //   - Send progress
   ```

5. **Content-fingerprint dedup** — before `store.add`, check
   `store.content_fingerprint_lookup(&video)`.  If the same content already
   exists under a different filename, skip the add (avoids duplicate entries
   from renamed copies).

6. **Report** — `Complete { html_files, … }`.

### 4.2 HTML output format

`present_video_matches` (from duplo-rs) produces a self-contained HTML page:

```html
<html>
  <head><title>Comparing similar videos for MyVideo</title></head>
  <body>
    <h1>Comparing similar videos for MyVideo</h1>
    <table id="Matches">
      <tr><th>Video</th><th>Info</th></tr>
      <tr>
        <td><video controls width="320" src="/path/to/MyVideo.mp4"></video></td>
        <td><a href="/path/to/MyVideo.mp4">…</a>
            <p>Resolution: 1920x1080</p>
            <p>Duration: 3600</p></td>
      </tr>
      <!-- … one row per matched video … -->
    </table>
  </body>
</html>
```

- Works in Chromium-based browsers (all formats) and Firefox (MP4).
- The user opens the HTML file in a browser, scrubs through the previews,
  and decides which files to keep/delete.

### 4.3 ffmpeg dependency

- **Required at runtime** for video scanning.
- At startup, check `which ffmpeg` (or `Command::new("ffmpeg").arg("-version")`).
  If missing, show a warning toast and disable the video-scan menu item.
- Screenshot extraction is the most expensive step.  The `num_threads` setting
  bounds concurrent ffmpeg processes.  Default to `1` (the demo default) to
  avoid overwhelming the disk/CPU.

### 4.4 Memory considerations

- The video store keeps candidate data in a bounded cache (`max_candidates_in_cache`).
  Default 100 candidates × ~100 MB = ~10 GB RAM.  The user should be warned
  (in settings) not to exceed ~⅓ of system RAM.
- For very large collections, the user can reduce `video_max_cache` or increase
  `video_screenshot_interval` (fewer screenshots per video → less data).

---

## 5. Configuration & Settings UI

### 5.1 Persist `SimilarConfig`

Add to the existing `Config` struct in `src/config.rs`:

```rust
pub struct Config {
    // … existing fields …
    pub similar: SimilarConfig,   // new
}
```

`SimilarConfig` derives `CosmicConfigEntry, Serialize, Deserialize` so it is
automatically saved/loaded by the cosmic-config system.  Default values come
from `SimilarConfig::default()`.

### 5.2 Settings page

Add a **"Similarity Search"** section to the existing Settings context page
(`fn settings` in `src/app/mod.rs`, line ~1027):

- Image sensitivity slider (0–100)
- Video sensitivity slider (0–100)
- Video screenshot interval (seconds, text input)
- Video min sequence length (text input)
- Video threads (text input)
- Video cache size (text input)
- Recursive scan checkbox
- "Delete image store" button (analogous to existing "Delete metadata")
- "Delete video store" button

Each control emits a `Message::SimilarConfig*` variant that updates
`self.config.similar` and calls `self.update_config()` to persist.

### 5.3 i18n strings

Add to `i18n/en/media_browser.ftl` (and `de`, `sv`):

```ftl
# Similarity Search
similar-search = Similarity Search
similar-image-scan = Scan for similar images
similar-video-scan = Scan for similar videos
similar-image-sensitivity = Image sensitivity
similar-video-sensitivity = Video sensitivity
similar-video-interval = Seconds between screenshots
similar-video-min-sequence = Min matching screenshots
similar-video-threads = Screenshot threads
similar-video-cache = Candidate cache size
similar-recursive = Include subdirectories
similar-scan-running = Scanning for similar {media}…
similar-scan-complete = Found {$count} similar {media}
similar-scan-cancelled = Similarity scan cancelled
similar-scan-error = Similarity scan error: {$error}
similar-open-results = Open results directory
similar-delete-image-store = Delete image similarity database
similar-delete-video-store = Delete video similarity database
similar-ffmpeg-missing = ffmpeg is required for video similarity search but was not found.
```

---

## 6. UI Integration

### 6.1 Message variants

Add to the existing `Message` enum (in `src/app/types.rs` or `src/tab.rs`
where the other `Message` lives — it is the `App`'s message type):

```rust
pub enum Message {
    // … existing variants …

    // ── Similarity search ──
    /// Start an image similarity scan on the selected directory.
    SimilarImageScan,
    /// Start a video similarity scan on the selected directory.
    SimilarVideoScan,
    /// Progress report from a running similarity scan.
    SimilarProgress(SimilarProgress),
    /// Cancel the currently running similarity scan.
    SimilarCancel,
    /// Open the results directory in a new tab.
    SimilarOpenResults(PathBuf),
    /// Update a similarity config field.
    SimilarConfigUpdate(SimilarConfigUpdate),
    /// Delete the similarity store (image or video).
    SimilarDeleteStore(bool),  // true = image, false = video
}
```

### 6.2 Menu items

Add to the **context menu** (right-click on a folder in `src/menu.rs`) and the
**menu bar** (File → Similarity Search):

- **"Find Similar Images"** → `Message::SimilarImageScan`
- **"Find Similar Videos"** → `Message::SimilarVideoScan`

These items are enabled only when the active tab is a `Location::Path` and
the selected item is a directory.  Follow the same pattern as the existing
`RecursiveScanDirectories` menu item (line ~143 in `menu.rs`).

Also add to the **nav-bar context menu** (right-click on a sidebar location)
so the user can kick off a scan on any favorite folder:
- `NavMenuAction::SimilarImageScan(entity)` / `NavMenuAction::SimilarVideoScan(entity)`

### 6.3 Handler methods

Add to `impl App` (in a new file `src/app/similar.rs`, following the
existing module split pattern):

```rust
// src/app/similar.rs
impl App {
    /// Handle Message::SimilarImageScan
    pub fn similar_image_scan(&mut self) -> Task<Message> {
        let entity = self.tab_model.active();
        let Some(tab) = self.tab_model.data::<Tab>(entity) else {
            return Task::none();
        };
        let Some(path) = &tab.location.path_opt() else {
            return Task::none();
        };

        let config = self.config.similar.clone();
        let (tx, rx) = mpsc::channel::<SimilarProgress>();
        let cancel = Arc::new(AtomicBool::new(false));
        self.similar_cancel_flag = Some(cancel.clone());

        // Spawn the scan on a background thread.
        let scan_path = path.to_path_buf();
        std::thread::spawn(move || {
            similar::run_image_scan(&config, &scan_path, &tx, &cancel)
                .log_errors();
        });

        // Drain the progress channel and forward to the UI.
        Task::perform(
            async move {
                while let Ok(msg) = rx.recv().await {
                    message::app(Message::SimilarProgress(msg)).await;
                }
            },
            |_| Task::none(),
        )
    }

    /// Handle Message::SimilarVideoScan  (analogous, calls run_video_scan)
    /// Handle Message::SimilarCancel     (sets cancel flag)
    /// Handle Message::SimilarProgress   (updates progress bar, toasts)
    /// Handle Message::SimilarOpenResults (opens a new tab at the path)
    /// Handle Message::SimilarDeleteStore (queues Operation::Delete on the store file)
}
```

### 6.4 Progress display

Reuse the existing **progress notification** infrastructure:
- `self.progress_operations.insert(id)` when the scan starts.
- Update a progress ratio via `Controller::set_progress` as files are processed.
- On completion, show a toast: *"Found 42 similar image pairs.  [Open Results]"*
  or *"Found 3 similar videos.  [Open HTML]"*.
- On cancellation: toast *"Similarity scan cancelled."*

For the progress bar, register the scan as a pending operation with a synthetic
`Operation::SimilarScan` variant (see §6.5) so it appears in the footer
progress bar alongside copy/move/delete operations.

### 6.5 Operation variant (optional but recommended)

Add to the `Operation` enum in `src/operation/mod.rs`:

```rust
pub enum Operation {
    // … existing variants …
    /// Run a similarity scan (image or video) on a directory.
    SimilarScan {
        kind: SimilarKind,      // Image | Video
        path: PathBuf,
        config: SimilarConfig,
    },
}
```

This gives the scan:
- A `pending_text()` / `completed_text()` for the footer.
- Cancel / pause support via the existing `Controller`.
- Consistent progress notification.

The `perform` method for `SimilarScan` would spawn the scan thread and bridge
progress to the `Controller`.

**Alternatively** (simpler, less coupling): keep the scan outside the
`Operation` system and manage its own progress + cancel via the
`SimilarProgress` message channel.  This is the approach the existing
`RecursiveScanDirectories` already takes (it spawns a raw `std::thread`
and does not go through `Operation`).  **Recommendation: follow the
`RecursiveScanDirectories` pattern** for v1 to minimize risk, and refactor
into the `Operation` system in a later iteration.

### 6.6 Results display

Two options, in order of increasing effort:

**Option A (v1, minimal):**
- On scan completion, show a toast with an "Open Results" button.
- Clicking it calls `self.open_tab_entity` with a new `Tab` whose
  `Location::Path` is the `similar_images/` or `similar_videos/` directory.
- The user reviews pairs / HTML files in the standard file-browser tab.
- For video HTML files, double-clicking opens them in the default browser
  (existing `OpenInExternalApp` behavior).

**Option B (v2, richer):**
- Add a new `Location::SimilarResults { kind, scan_dir }` variant.
- Render a custom view that:
  - For **images**: shows pairs side-by-side with thumbnails, a "Keep" /
    "Remove" / "Reject" button per pair.  "Remove" deletes the `_REMOVE_`
    file and the `_KEEP_` hardlink.  "Reject" restores the `_REMOVE_` file
    to its original location.
  - For **videos**: embeds the HTML in an in-app web view (e.g. via
    `webview` crate) or lists the matched videos with thumbnails and
    links to open in the system player.

**Recommendation: ship Option A for v1.**  It requires no new rendering
code and leverages the existing tab / file-browser infrastructure.  Option B
can be added incrementally.

---

## 7. Background Processing & Threading Model

### 7.1 Image scan

- Runs on a single `std::thread` (the duplo image store is not
  `Send`-safe for concurrent writes; the demo processes sequentially).
- Each iteration: decode image → hash → query store → present pair.
- The `image` crate decode is the bottleneck; it is already
  `spawn_blocking`-friendly but runs in its own thread here.
- Progress is sent after each file (or every N files to reduce message
  volume for large collections).

### 7.2 Video scan

- **Producer**: up to `num_threads` ffmpeg screenshot-extraction workers
  (each in its own `std::thread`).  Bounded by an `AtomicUsize` in-flight
  counter (same pattern as the demo's §9 code).
- **Query workers**: pull parsed `VideoCandidate`s from a shared
  `Mutex<mpsc::Receiver>`, call `store.query(&video)` (parallel reads via
  the store's internal read lock).
- **Writer**: single serial committer that calls `store.add` and
  `present_video_matches`.  This avoids SQLite write contention.
- `cancel_flag` is polled by the producer before spawning new workers, by
  query workers before pulling from the channel, and by the writer before
  committing.

### 7.3 Cancellation

- A shared `Arc<AtomicBool>` cancel flag.
- The UI "Cancel" button sets the flag.
- Workers check the flag at safe points (between files, between screenshots).
- In-flight ffmpeg processes are killed by dropping the `Child` handle
  (or sending SIGTERM on Unix).
- The store is left in a consistent state: partially processed files are
  not added; the user can resume on the next run.

### 7.4 Blocking the UI

- All heavy work (image decode, ffmpeg, SQLite) runs outside the
  `iced`/`cosmic` update loop.
- The UI thread only receives `SimilarProgress` messages via the
  `Task::perform` async bridge.
- This is the same pattern used by `RecursiveScanDirectories` and the
  existing `Operation::perform` (which uses `tokio::task::spawn_blocking`).

---

## 8. Error Handling

### 8.1 Error types

```rust
// src/similar/errors.rs
#[derive(Debug, thiserror::Error)]
pub enum SimilarError {
    #[error("ffmpeg not found. Install ffmpeg for video similarity search.")]
    FfmpegMissing,

    #[error("failed to create output directory {path}: {source}")]
    OutputDir { path: PathBuf, source: std::io::Error },

    #[error("failed to load image store: {0}")]
    StoreLoad(String),

    #[error("failed to save image store: {0}")]
    StoreSave(String),

    #[error("failed to connect to video database: {0}")]
    VideoDb(String),

    #[error("failed to process file {path}: {0}")]
    FileProcessing { path: PathBuf, source: String },

    #[error("scan cancelled")]
    Cancelled,
}
```

### 8.2 Per-file errors

- Individual file failures (undecodable image, corrupt video) are logged
  and reported as `SimilarProgress::Warning` but do **not** abort the scan.
- This matches the demo apps' behavior: they `continue` on per-file errors.

### 8.3 Store corruption

- If `store.slurp_binary` or `videostore::connect` fails, show a
  `DialogPage::FailedOperation`-style dialog offering to delete the
  corrupt store and start fresh.

---

## 9. Data Locations

| Data | Path | Notes |
|---|---|---|
| Image store | `~/.local/share/media-browser/similar_images.store` | Binary blob, loaded/saved as a unit. |
| Video store | `~/.local/share/media-browser/similar_videos.sqlite3` | SQLite DB, survives restarts. |
| Image results | `<scan_dir>/similar_images/` | Hardlinks + moved files. |
| Video results | `<scan_dir>/similar_videos/` | HTML files. |
| Temp screenshots | `/tmp/duplo_vid_<id>_<n>/` | Created & deleted by duplo-rs internally. |

All paths respect `$XDG_DATA_HOME` (already handled by `dirs::data_local_dir()`
which is used elsewhere in the codebase, e.g. `delete_metadata` in `scan.rs`).

---

## 10. Testing Strategy

### 10.1 Unit tests

- **`similar::config`**: default values, serde round-trip.
- **`similar::image_scan`**: with a small set of test images (create synthetic
  PNGs in a tempdir), verify:
  - Identical images produce a pair.
  - Non-similar images do not.
  - Store persists and reloads correctly.
  - Cancel flag stops the scan.
- **`similar::video_scan`**: with `ffmpeg` available, verify:
  - Two identical MP4s produce an HTML file with 2 `<video>` rows.
  - Unrelated videos do not match.
  - Content-fingerprint dedup skips renamed copies.
  - Cancel flag stops the scan.

### 10.2 Integration tests

- Launch the app in test mode (existing `test_utils` in `mod.rs`), simulate
  a right-click → "Find Similar Images" on a tempdir with 3 test images
  (2 identical, 1 different).  Assert the `similar_images/` directory
  contains exactly one pair.

### 10.3 Manual / QA checklist

- [ ] Image scan on a 1000-image folder completes; pairs are correct.
- [ ] Image scan can be cancelled mid-run; store is intact.
- [ ] Re-run of image scan skips already-processed images.
- [ ] Video scan on 5 MP4s (2 identical, 3 different) produces 1 HTML file.
- [ ] HTML file opens in Firefox and Chromium; video previews work.
- [ ] Video scan with `num_threads=3` does not spawn >3 ffmpeg processes.
- [ ] ffmpeg missing → warning toast, video scan menu item disabled.
- [ ] Config values persist across app restarts.
- [ ] "Delete store" buttons work and the next scan starts fresh.
- [ ] i18n strings present in `en`, `de`, `sv`.

---

## 11. Implementation Phases

| Phase | Scope | Est. effort |
|---|---|---|
| **1** | Add `duplo-rs` dep, create `src/similar/` module, implement `run_image_scan`, wire up config + i18n.  No UI yet (CLI-testable). | 2–3 days |
| **2** | Implement `run_video_scan` with the producer/query/writer pipeline.  CLI-testable. | 3–4 days |
| **3** | UI: menu items, messages, progress toasts, cancel, "Open Results" tab.  Settings page. | 2–3 days |
| **4** | Edge cases: error dialogs, ffmpeg check, store cleanup, i18n polish. | 1–2 days |
| **5** | Tests: unit + integration + manual QA. | 2–3 days |
| **6** | *(Optional)* In-app pair reviewer (Option B) for images. | 3–5 days |

**Total: ~2–3 weeks** for a solid v1 (phases 1–5).

---

## 12. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `duplo-rs` API changes (pre-1.0) | Pin to a commit SHA.  Wrap the API behind `src/similar/` so only one module needs updating. |
| ffmpeg missing on user system | Check at startup; disable video scan with a clear toast. |
| RAM exhaustion with large video collections | Default `video_max_cache=100`; warn in settings; document the ⅓-RAM rule. |
| Hardlink failures on network / non-POSIX filesystems | `present_pairs` already falls back to a copy (per the doc comment).  Surface the fallback in the progress toast. |
| SQLite write contention in video scan | Single writer thread (the demo's §9 writer pattern). |
| Long scans blocking the store file | The store is written atomically (`dump_binary` to a temp file, then rename).  A crash mid-save does not corrupt the existing store. |
| User deletes the `similar_images/` dir while scan is running | The scan creates it with `create_dir_all` on each `present_pairs` call.  If the dir is deleted mid-scan, subsequent pairs recreate it.  Acceptable. |

---

## 13. Future Enhancements (out of scope for v1)

- **In-app pair reviewer**: side-by-side image thumbnails with per-pair
  keep/reject buttons, eliminating the need to open a file manager.
- **Video HTML viewer**: embed the results HTML in an in-app web view.
- **Batch mode**: scan multiple directories in one go.
- **GPU-accelerated hashing**: if `duplo-rs` ever adds it.
- **Dedup report**: a summary page showing total space recoverable.
- **Scheduled scans**: cron-like integration or a "scan on startup" option.
- **Exclude lists**: user-configured paths/globs to skip.

---

## 14. Files to create / modify (checklist)

### New files
- [x] `src/similar/mod.rs`
- [x] `src/similar/config.rs`
- [x] `src/similar/image_scan.rs`
- [x] `src/similar/video_scan.rs`
- [x] `src/similar/progress.rs`
- [x] `src/similar/errors.rs`
- [x] `src/app/similar.rs` (message handlers)

### Modified files
- [x] `Cargo.toml` — add `duplo-rs` dependency
- [x] `src/lib.rs` — add `mod similar;`
- [x] `src/app/mod.rs` — add `mod similar;` to the app module list; add `similar_cancel_flag` field to `App`
- [x] `src/app/types.rs` — add `Message::Similar*` variants; add `SimilarKind` enum
- [x] `src/menu.rs` — add "Find Similar Images" / "Find Similar Videos" context-menu items
- [x] `src/app/mod.rs` (`fn update`) — dispatch new `Message::Similar*` arms
- [x] `src/config.rs` — add `similar: SimilarConfig` to `Config`
- [x] `i18n/en/media_browser.ftl` — add similarity search strings
- [x] `i18n/de/media_browser.ftl` — add German strings
- [x] `i18n/sv/media_browser.ftl` — add Swedish strings
- [x] `debian/control` or packaging — ensure `ffmpeg` is in `Recommends` (it is in `Depends`, which is stronger)

---

## 15. Implementation notes (status)

Implemented end-to-end; all plan checkboxes above are complete.

- **duplo-rs is vendored** at `vendor/duplo-rs` (rev `1f2f3fb`, applied via
  `[patch]` in `Cargo.toml`, following the existing `vendor/window_clipboard`
  convention). The vendored copy carries three fixes that are not yet in
  upstream:
  1. `Store::decode` read the index count and sensitivity in the **opposite
     order** from `Store::encode`. Reading back a persisted image store
     therefore turned the index count into a ~1.4e19 loop bound and grew
     `indices` until the process was OOM-killed. Decode order is fixed and a
     `count != INDICESMAX` guard rejects corrupt streams.
  2. `Store::decode` **appended** the loaded index buckets to the buckets a
     fresh `Store::new` already prefilled, leaving the loaded ones
     unreachable from `query`. Decode now replaces them.
  3. `files::present_pairs` named the `KEEP` file after the `REMOVE`
     candidate (both pair files had the same name). Fixed.

  When these land in `duplo-rs` upstream, bump the pinned rev and delete the
  vendored copy + `[patch]` entry.

- **Video scan pipeline** differs from the demo on purpose: parsing stays
  parallel (`video_num_threads` ffmpeg workers), but commit + match are
  serial, with matching done **after** all new videos are committed. The
  demo's query-while-parsing pipeline races itself (a video can be queried
  before its twin is added, silently missing same-run pairs).

- **Renamed copies** (plan §4.5): the copy is not added to the store again,
  but its match report is still produced — the user needs to see that a new
  file duplicates a stored one. A re-scan regenerates that report (artifact
  overwrite, no new store entry).

- **Upstream `duplo-rs` also contains** an unrelated copy-paste smell worth
  fixing there: `Store::decode` does not bail out when
  `restore_hash_string_usize` fails mid-stream (it keeps reading at a
  shifted offset). Not reachable with well-formed streams; noted for the
  upstream PR.
