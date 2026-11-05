//use array2d::{Array2D, Error};
use log;
use rusqlite::{params, Connection, Result};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

static WEIGHTS: [[f64; 6]; 3] = [
    [5.00_f64, 0.83, 1.01, 0.52, 0.47, 0.30],
    [19.21, 1.26, 0.44, 0.53, 0.28, 0.14],
    [34.37, 0.36, 0.45, 0.14, 0.18, 0.27],
];
pub const IMAGESCALE: u32 = 128;
pub const INDICESMAX: u32 = 98400;
pub static TOPCOEFS: i32 = 40;
static WEIGHTSUMS: [f64; 6] = [58.58 as f64, 2.45, 1.9, 1.19, 0.93, 0.71];
pub static CTRL_C_PRESSED: bool = false;

// (CandidateCache removed in §12: the in-memory index replaced the LRU.)

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct ScreenshotIndex {
    pub id: String,
    pub video_id: u32,
    pub screenshot_id: u32,
    pub runtime: u32,
}

impl Default for ScreenshotIndex {
    fn default() -> ScreenshotIndex {
        ScreenshotIndex {
            id: String::new(),
            video_id: 0,
            screenshot_id: 0,
            runtime: 0,
        }
    }
}

impl ScreenshotIndex {
    pub fn new() -> Self {
        ScreenshotIndex {
            ..Default::default()
        }
    }

    pub fn from(filename: &str, video_id: u32, screenshot_id: u32, runtime: u32) -> Self {
        let mut v = ScreenshotIndex {
            ..Default::default()
        };
        v.id = filename.to_string();
        v.video_id = video_id;
        v.screenshot_id = screenshot_id;
        v.runtime = runtime;
        v
    }

    // encode the data structure to binary stream
    pub fn encode(&self, to: &mut Vec<u8>) {
        crate::marshal::store_string(&self.id, to);
        crate::marshal::store_u32(self.video_id, to);
        crate::marshal::store_u32(self.screenshot_id, to);
        crate::marshal::store_u32(self.runtime, to);
    }

    // decode data structure from binary stream
    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        self.id = crate::marshal::restore_string(from);
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.video_id = r;
        }
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.screenshot_id = r;
        }
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.runtime = r;
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct Sequence {
    pub video_id: u32,      // index of this video
    pub last_timecode: u32, // time in seconds
    pub sequence: Vec<u32>,
}

impl Default for Sequence {
    fn default() -> Sequence {
        Sequence {
            video_id: 0,
            last_timecode: 0,
            sequence: Vec::new(),
        }
    }
}

impl Sequence {
    pub fn new() -> Self {
        Sequence {
            ..Default::default()
        }
    }

    pub fn from(video_id: u32, timecode: u32, screenshot_id: u32) -> Self {
        let mut v = Sequence {
            ..Default::default()
        };
        v.video_id = video_id;
        v.last_timecode = timecode;
        v.sequence.push(screenshot_id);
        v
    }
}

/// Store is a data structure that holds references to images. It holds visual
/// hashes and references to the images but the images themselves are not held
/// in the data structure.
///
/// A general limit to the store is that it can hold no more than 4,294,967,295
/// images. This is to save RAM space but may be easy to extend by modifying its
/// data structures to hold uint64 indices instead of uint32 indices.
///
/// candidates contains all images in the store or, rather, the candidates for a query.
///
/// All IDs in the store, mapping to candidate indices.
///
/// indices  contains references to the images in the store. It is a slice
/// of slices which contains image indices (into the "candidates" slice).
/// Use the following formula to access an index slice:
///
///	store.indices[sign*ImageScale*ImageScale*haar.ColourChannels + coefIdx*haar.ColourChannels + channel]
///
/// where the variables are as follows:
///
///	sign: Either 0 (positive) or 1 (negative)
///	coefIdx: The index of the coefficient (from 0 to (ImageScale*ImageScale)-1)
///	channel: The colour channel (from 0 to haar.ColourChannels-1)
///
/// sentivity (Hamming distance threshold) for the perceptual hashes.
/// Larger values will allow more images to be seen as *similar*
///
/// start_directory the directory the direcotory list or walk started. Also the directory possible matches will be presented in
/// num_threads     number of threads scanning videos
///
/// modified tells Whether this store was modified since it was loaded/created.
/// num_seconds_between screenshots   default 10. Increase for quicker scanning and less resources.
/// min_similar_screenshots_in_sequence  number of similar screenshots in a row that have to match to count as similar video. Default: 6 or 1 minute
///
/// max_candidates_in_cache  (legacy config; the LRU was removed — the in-memory index serves this role now)
///
/// Per-screenshot metadata that the hot scoring path needs. This is the
/// "small" part of a stored screenshot: enough to compute dhash_distance,
/// histogram_distance, ratio_diff, and to look up the timecode, but not the
/// full CoefMatrix (which is only needed to *build* the index, not to query it).
///
/// Kept as a flat, cache-friendly record so the query loop can stream through
/// millions of entries without pointer chasing.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct ScreenMeta {
    /// The candidate_id (1-based) this screenshot belongs to.
    pub video_id: u32,
    /// The 1-based screenshot index within the candidate.
    pub screenshot_id: u32,
    /// Timecode in seconds.
    pub timecode: u32,
    /// First 64-bit half of the dHash (Y channel).
    pub dhash0: u64,
    /// Second 64-bit half of the dHash (Cb/Cr channels).
    pub dhash1: u64,
    /// 64-bit quantised histogram.
    pub histogram: u64,
    /// width / height ratio.
    pub ratio: f64,
}

impl Default for ScreenMeta {
    fn default() -> ScreenMeta {
        ScreenMeta {
            video_id: 0,
            screenshot_id: 0,
            timecode: 0,
            dhash0: 0,
            dhash1: 0,
            histogram: 0,
            ratio: 0.0,
        }
    }
}

/// Per-video metadata needed by `rate_match` and by the demo to present
/// matches. Tiny; one entry per candidate in the whole corpus is trivial.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct VideoMeta {
    pub candidate_id: u32,
    pub id: String,
    pub runtime: u32,
    pub width: u32,
    pub height: u32,
}

impl Default for VideoMeta {
    fn default() -> VideoMeta {
        VideoMeta {
            candidate_id: 0,
            id: String::new(),
            runtime: 0,
            width: 0,
            height: 0,
        }
    }
}

/// In-memory index that mirrors the `videostore_indices` table.
///
/// `buckets[location]` is the list of *global screenshot indices* (into
/// `screenshots`) that were filed under that `location`. `screenshots[g]` is
/// the per-screenshot metadata. Together they let `search_matches` run
/// entirely in RAM without touching SQLite.
///
/// `video_ids` maps the public 1-based `candidate_id` to a dense 0-based
/// position, which is used by the sequence-tracking logic in `query()`.
#[derive(Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct InMemoryIndex {
    /// buckets[location] -> Vec<global screenshot index>
    pub buckets: Vec<Vec<u32>>,
    /// Global screenshot metadata, indexed by the u32 in `buckets`.
    pub screenshots: Vec<ScreenMeta>,
    /// Per-candidate metadata. Indexed by dense 0-based video position.
    pub metas: Vec<VideoMeta>,
    /// candidate_id (1-based) -> dense 0-based position in `metas`.
    pub video_ids: std::collections::BTreeMap<u32, usize>,
    /// Reverse: dense 0-based position -> candidate_id.
    pub candidate_by_pos: Vec<u32>,
    /// Total number of global screenshot entries (== screenshots.len()).
    pub num_screenshots: u32,
}

impl InMemoryIndex {
    /// Remove a single video (and all its screenshots) from the in-memory index.
    ///
    /// Complexity: O(total_bucket_entries + num_screenshots) — much cheaper than
    /// the full `bulk_load_index` re-stream from SQLite (§8).
    ///
    /// After this call, all global screenshot indices in `buckets` and
    /// `screenshots` are compacted (no gaps), and `video_ids` / `metas` /
    /// `candidate_by_pos` are renumbered so that dense positions remain contiguous.
    pub fn remove_video(&mut self, candidate_id: u32) {
        let video_pos = match self.video_ids.get(&candidate_id) {
            Some(&pos) => pos,
            None => return,
        };

        // Step 1: find which global screenshot indices belong to this video.
        let removed: Vec<u32> = self
            .screenshots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.video_id == candidate_id)
            .map(|(i, _)| i as u32)
            .collect();

        // Step 2: remove those entries from every bucket.
        if !removed.is_empty() {
            let removed_set: std::collections::BTreeSet<u32> = removed.iter().copied().collect();
            for bucket in self.buckets.iter_mut() {
                bucket.retain(|&g| !removed_set.contains(&g));
            }
            // Step 3: renumber surviving bucket entries.
            // For a survivor at old position `p`, its new position is
            // `p - count(removed indices <= p)`.
            let removed_sorted: Vec<u32> = removed; // already sorted (BTreeSet iter)
            for bucket in self.buckets.iter_mut() {
                for g in bucket.iter_mut() {
                    let offset = removed_sorted.partition_point(|&r| r <= *g);
                    *g -= offset as u32;
                }
            }
            // Step 4: compact the screenshots array.
            self.screenshots.retain(|s| s.video_id != candidate_id);
        }

        // Step 5: remove the video from the per-video arrays and renumber.
        self.metas.remove(video_pos);
        self.candidate_by_pos.remove(video_pos);
        self.video_ids.remove(&candidate_id);
        for (k, pos) in self
            .video_ids
            .iter()
            .map(|(k, &v)| (*k, v))
            .collect::<Vec<_>>()
        {
            if pos > video_pos {
                self.video_ids.insert(k, pos - 1);
            }
        }

        self.num_screenshots = self.screenshots.len() as u32;
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct VideoStoreInner {
    pub num_candidates: u32,

    pub ids: std::collections::BTreeMap<String, usize>,
    pub video_ids: std::collections::BTreeMap<u32, usize>,

    pub num_indices: std::collections::BTreeMap<u32, usize>,
    pub num_index_values: u32,
    pub sensitivity: f64,
    pub start_directory: String,
    pub num_threads: u32,

    pub modified: bool,
    pub num_seconds_between_screenshots: u32,
    pub min_similar_screenshots_in_sequence: u32,
    pub max_candidates_in_cache: usize,
    /// In-memory mirror of `videostore_indices` + per-screenshot metadata.
    /// Populated once at load time and appended to incrementally in `add()`.
    /// This is the hot path: `search_matches` reads from here, not SQLite.
    pub index: InMemoryIndex,
}

impl Default for VideoStoreInner {
    fn default() -> VideoStoreInner {
        VideoStoreInner {
            num_candidates: 0,
            ids: std::collections::BTreeMap::new(),
            video_ids: std::collections::BTreeMap::new(),
            num_indices: std::collections::BTreeMap::new(),
            num_index_values: 0,
            start_directory: ".".to_string(),
            num_threads: 1,
            sensitivity: -100.0,
            modified: false,
            num_seconds_between_screenshots: 10,
            min_similar_screenshots_in_sequence: 6,
            max_candidates_in_cache: 100,
            index: InMemoryIndex::default(),
        }
    }
}

impl VideoStoreInner {
    pub fn new(
        connection: &mut rusqlite::Connection,
        sensitivity: f64,
        start_directory: &str,
        num_threads: u32,
        num_seconds_between_screenshots: u32,
        min_similar_screenshots_in_sequence: u32,
        max_candidates_in_cache: usize,
    ) -> Self {
        let mut v = VideoStoreInner {
            ..Default::default()
        };
        v.sensitivity = sensitivity;
        v.start_directory = start_directory.to_string();
        v.num_threads = num_threads;
        v.num_seconds_between_screenshots = num_seconds_between_screenshots;
        v.min_similar_screenshots_in_sequence = min_similar_screenshots_in_sequence;
        v.max_candidates_in_cache = max_candidates_in_cache;
        let query = "SELECT * FROM videostore_parameters WHERE config_id = 1";
        match connection.prepare(query) {
            Ok(mut statement) => {
                match statement.query(params![]) {
                    Ok(mut rows) => loop {
                        match rows.next() {
                            Ok(Some(row)) => {
                                let s_opt = row.get(1);
                                if s_opt.is_ok() {
                                    v.sensitivity = s_opt.unwrap();
                                }
                                let s_opt = row.get(2);
                                if s_opt.is_ok() {
                                    v.start_directory = s_opt.unwrap();
                                }
                                let s_opt = row.get(3);
                                if s_opt.is_ok() {
                                    v.num_threads = s_opt.unwrap();
                                }
                                let s_opt = row.get(4);
                                if s_opt.is_ok() {
                                    v.num_seconds_between_screenshots = s_opt.unwrap();
                                }
                                let s_opt = row.get(5);
                                if s_opt.is_ok() {
                                    v.min_similar_screenshots_in_sequence = s_opt.unwrap();
                                }
                                let s_opt = row.get(6);
                                if s_opt.is_ok() {
                                    v.max_candidates_in_cache = s_opt.unwrap();
                                }
                                break;
                            }
                            Ok(None) => {
                                log::warn!("No data read from parameters.");
                                break;
                            }
                            Err(error) => {
                                log::error!("Failed to read a row from parameters: {}", error);
                                break;
                            }
                        }
                    },
                    Err(err) => {
                        log::error!("could not read line from parameter database: {}", err);
                    }
                }
                if v.sensitivity != sensitivity
                    || v.num_seconds_between_screenshots != num_seconds_between_screenshots
                    || v.min_similar_screenshots_in_sequence != min_similar_screenshots_in_sequence
                    || v.max_candidates_in_cache != max_candidates_in_cache
                {
                    // change the parameters in the database
                    let query_delete = "DELETE FROM videostore_parameters WHERE config_id = 1";
                    match connection.execute(query_delete, params![]) {
                        Ok(retval) => log::warn!("Deleted {} data from parameters.", retval),
                        Err(error) => {
                            log::error!("Failed to delete data from parameter database: {}", error);
                            return v;
                        }
                    }
                    let sensitivity_i64 = v.sensitivity as i64;
                    match connection.execute(
                        "INSERT INTO videostore_parameters (config_id, sensitivity, start_directory, num_threads, num_seconds_between_screenshots, min_similar_screenshots_in_sequence, max_candidates_in_cache) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![&1, &sensitivity_i64, &v.start_directory, &v.num_threads, &v.num_seconds_between_screenshots, &v.min_similar_screenshots_in_sequence, &v.max_candidates_in_cache],
                    ) {
                        Ok(retval) => log::warn!("Inserted {} data into parameter.", retval),
                        Err(error) => {
                            log::error!("Failed to insert data into parameter database: {}", error);
                            return v;
                        }
                    }
                    v.sensitivity = sensitivity;
                    v.start_directory = start_directory.to_string();
                    v.num_threads = num_threads;
                    v.num_seconds_between_screenshots = num_seconds_between_screenshots;
                    v.min_similar_screenshots_in_sequence = min_similar_screenshots_in_sequence;
                    v.max_candidates_in_cache = max_candidates_in_cache;
                }
            }
            Err(error) => {
                log::error!("Could not prepare statement {}: {}", query, error);
                return v;
            }
        }
        let query_candidates = "SELECT candidate_id, filename, video_id FROM videostore_candidates";
        match connection.prepare(query_candidates) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => loop {
                    match rows.next() {
                        Ok(Some(row)) => {
                            let mut candidate_id: usize;
                            let id;
                            let video_id;
                            match row.get(0) {
                                Ok(s) => candidate_id = s,
                                Err(error) => {
                                    log::error!("Failed to read candiate_id for video: {}", error);
                                    continue;
                                }
                            }
                            match row.get(1) {
                                Ok(s) => id = s,
                                Err(error) => {
                                    log::error!("Failed to read id for video: {}", error);
                                    continue;
                                }
                            }
                            match row.get(2) {
                                Ok(s) => video_id = s,
                                Err(error) => {
                                    log::error!("Failed to read video_id for video: {}", error);
                                    continue;
                                }
                            }
                            candidate_id -= 1;
                            v.ids.insert(id, candidate_id as usize);
                            v.video_ids.insert(video_id, candidate_id as usize);
                            v.num_candidates += 1;
                        }
                        Ok(None) => {
                            log::warn!("No data read from candidates.");
                            break;
                        }
                        Err(error) => {
                            log::error!("Failed to read a row from candidates: {}", error);
                            break;
                        }
                    }
                },
                Err(err) => {
                    log::error!("could not read line from candidate database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Could not prepare statement {}: {}",
                    query_candidates,
                    error
                );
                return v;
            }
        }
        // fill the num_indices  and number of indices values
        let query_index_location_count =
            "SELECT location, COUNT(index_id) FROM videostore_indices GROUP BY location";
        match connection.prepare(query_index_location_count) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => loop {
                    match rows.next() {
                        Ok(Some(row)) => {
                            let location;
                            let num_entries;
                            match row.get(0) {
                                Ok(s) => location = s,
                                Err(error) => {
                                    log::error!("Failed to read location for indices: {}", error);
                                    continue;
                                }
                            }
                            match row.get(1) {
                                Ok(s) => num_entries = s,
                                Err(error) => {
                                    log::error!("Failed to read count for indices: {}", error);
                                    continue;
                                }
                            }
                            v.num_indices.insert(location, num_entries);
                        }
                        Ok(None) => {
                            log::warn!("No data read from indices.");
                            break;
                        }
                        Err(error) => {
                            log::error!("Failed to read a row from indices: {}", error);
                            break;
                        }
                    }
                },
                Err(err) => {
                    log::error!("could not read line from indices database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Could not prepare statement {}: {}",
                    query_index_location_count,
                    error
                );
                return v;
            }
        }

        let query_index_count = "SELECT COUNT(index_id) FROM videostore_indices";
        match connection.prepare(query_index_count) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => loop {
                    match rows.next() {
                        Ok(Some(row)) => match row.get(0) {
                            Ok(s) => v.num_index_values = s,
                            Err(error) => {
                                log::error!("Failed to read count for indices: {}", error);
                                continue;
                            }
                        },
                        Ok(None) => {
                            log::warn!("No data read from indices.");
                            break;
                        }
                        Err(error) => {
                            log::error!("Failed to read a row from indices: {}", error);
                            break;
                        }
                    }
                },
                Err(err) => {
                    log::error!("could not read line from indices database: {}", err);
                }
            },
            Err(error) => {
                log::error!(
                    "Could not prepare statement {}: {}",
                    query_index_count,
                    error
                );
                return v;
            }
        }
        v.bulk_load_index(connection);
        v
    }

    /// Expand a single screenshot's hash into the list of `location` buckets it
    /// contributes to. Mirrors the inner loop of `add()` exactly, so the in-memory
    /// index stays consistent with the SQLite `videostore_indices` table.
    ///
    /// Delegates to `compute_score_plan`, which is the single source of truth for
    /// the significant `(location, bin)` pairs (shared with the query hot path, §7).
    fn hash_bucket_locations(
        hash: &crate::hash::Hash,
    ) -> Vec<(u32 /* location */, u32 /* weight_bin */)> {
        Self::compute_score_plan(hash)
    }

    /// Bulk-load the in-memory index from the database.
    ///
    /// This is called once at startup (in `new()`) and after any `slurp_binary`.
    /// It streams the candidate blobs (compact, no matrix) to extract per-screenshot
    /// metadata (dhash, histogram, ratio, timecode), then reads the
    /// `videostore_indices` table to build the bucket structure. After this call,
    /// `search_matches` and `rate_match` no longer need to hit SQLite for the hot path.
    ///
    /// The method is `O(total_screenshots + total_index_entries)` and is
    /// intentionally a one-shot, not a per-query cost.
    pub fn bulk_load_index(&mut self, connection: &mut rusqlite::Connection) {
        let mut idx = InMemoryIndex::default();
        idx.buckets = vec![Vec::new(); (INDICESMAX as usize) + 1];

        // Build a reverse lookup candidate_id -> filename from self.ids,
        // which `new()` already populated.
        let id_by_candidate: std::collections::BTreeMap<u32, String> = self
            .ids
            .iter()
            .filter_map(|(filename, candidate_idx)| {
                let candidate_id = *candidate_idx as u32 + 1;
                Some((candidate_id, filename.clone()))
            })
            .collect();

        // Phase 1: read candidate blobs to get per-screenshot metadata and video info.
        // Build a lookup from (filename, screenshot_id) -> global screenshot index.
        let mut shot_lookup: std::collections::HashMap<(String, u32), u32> =
            std::collections::HashMap::new();

        let query = "SELECT candidate_id, data FROM videostore_candidates ORDER BY candidate_id";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => loop {
                    match rows.next() {
                        Ok(Some(row)) => {
                            let candidate_id: u32 = match row.get(0) {
                                Ok(v) => v,
                                Err(e) => {
                                    log::error!("bulk_load_index: candidate_id: {}", e);
                                    break;
                                }
                            };
                            let blob: Vec<u8> = match row.get(1) {
                                Ok(v) => v,
                                Err(e) => {
                                    log::error!("bulk_load_index: data blob: {}", e);
                                    break;
                                }
                            };
                            let mut candidate = crate::videocandidate::VideoCandidate::new();
                            // decode_any handles both v1 (with matrix) and v2 (compact) blobs.
                            candidate.decode_any(&mut std::io::Cursor::new(blob));

                            let video_pos = idx.metas.len() as u32;
                            let filename = id_by_candidate
                                .get(&candidate_id)
                                .cloned()
                                .unwrap_or_else(|| candidate.id.clone());
                            idx.metas.push(VideoMeta {
                                candidate_id,
                                id: filename.clone(),
                                runtime: candidate.runtime,
                                width: candidate.width,
                                height: candidate.height,
                            });
                            idx.candidate_by_pos.push(candidate_id);
                            idx.video_ids.insert(candidate_id, video_pos as usize);

                            for shot in &candidate.screenshots {
                                let g = idx.screenshots.len() as u32;
                                idx.screenshots.push(ScreenMeta {
                                    video_id: candidate_id,
                                    screenshot_id: shot.screenshot_id,
                                    timecode: shot.timecode,
                                    dhash0: shot.hash.dhash.first().copied().unwrap_or(0),
                                    dhash1: shot.hash.dhash.get(1).copied().unwrap_or(0),
                                    histogram: shot.hash.histogram,
                                    ratio: shot.hash.ratio,
                                });
                                shot_lookup.insert((filename.clone(), shot.screenshot_id), g);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            log::error!("bulk_load_index: row: {}", e);
                            break;
                        }
                    }
                },
                Err(e) => {
                    log::error!("bulk_load_index: query: {}", e);
                    return;
                }
            },
            Err(e) => {
                log::error!("bulk_load_index: prepare: {}", e);
                return;
            }
        }

        // Phase 2: read the index table to build the bucket structure.
        // This replaces the old approach of re-deriving locations from the
        // (now absent) Haar matrix in each blob.
        let idx_query = "SELECT location, filename, screenshot_id FROM videostore_indices";
        match connection.prepare(idx_query) {
            Ok(mut statement) => match statement.query(params![]) {
                Ok(mut rows) => loop {
                    match rows.next() {
                        Ok(Some(row)) => {
                            let location: u32 = match row.get(0) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let filename: String = match row.get(1) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            let screenshot_id: u32 = match row.get(2) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if let Some(&g) = shot_lookup.get(&(filename, screenshot_id)) {
                                if (location as usize) < idx.buckets.len() {
                                    idx.buckets[location as usize].push(g);
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            log::error!("bulk_load_index: index row: {}", e);
                            break;
                        }
                    }
                },
                Err(e) => log::error!("bulk_load_index: index query: {}", e),
            },
            Err(e) => log::error!("bulk_load_index: index prepare: {}", e),
        }

        idx.num_screenshots = idx.screenshots.len() as u32;
        log::warn!(
            "bulk_load_index: {} candidates, {} screenshots, {} bucket slots",
            idx.metas.len(),
            idx.screenshots.len(),
            idx.buckets.iter().map(|b| b.len()).sum::<usize>()
        );
        self.index = idx;
    }

    /// Append a single candidate to the in-memory index. Called from `add()`
    /// after the SQLite insert succeeds, so the in-memory view and the DB stay
    /// in lock-step.
    ///
    /// **Precondition:** `video` must carry the *full* Haar matrix in each
    /// screenshot's hash. `hash_bucket_locations` reads
    /// `hash.matrix.coefs` to derive the significant bucket locations; a
    /// compact-decoded candidate (which omits the matrix) would silently
    /// produce an empty index entry. In practice this is safe because
    /// `append_index` is only called from `add()`, which receives the
    /// original in-memory `VideoCandidate` before the matrix is dropped.
    pub fn append_index(
        &mut self,
        video: &crate::videocandidate::VideoCandidate,
        candidate_id: u32,
    ) {
        debug_assert!(
            video
                .screenshots
                .iter()
                .all(|s| !s.hash.matrix.coefs.is_empty()),
            "append_index requires the full Haar matrix; got a compact candidate"
        );
        let video_pos = self.index.metas.len() as u32;
        self.index.metas.push(VideoMeta {
            candidate_id,
            id: video.id.clone(),
            runtime: video.runtime,
            width: video.width,
            height: video.height,
        });
        self.index.candidate_by_pos.push(candidate_id);
        self.index
            .video_ids
            .insert(candidate_id, video_pos as usize);
        for shot in &video.screenshots {
            let g = self.index.screenshots.len() as u32;
            self.index.screenshots.push(ScreenMeta {
                video_id: candidate_id,
                screenshot_id: shot.screenshot_id,
                timecode: shot.timecode,
                dhash0: shot.hash.dhash.first().copied().unwrap_or(0),
                dhash1: shot.hash.dhash.get(1).copied().unwrap_or(0),
                histogram: shot.hash.histogram,
                ratio: shot.hash.ratio,
            });
            // Derives significant locations from the full matrix (available
            // only because the caller passed the original in-memory candidate).
            for (location, _bin) in Self::hash_bucket_locations(&shot.hash) {
                self.index.buckets[location as usize].push(g);
            }
        }
        self.index.num_screenshots = self.index.screenshots.len() as u32;
    }

    pub fn has(&self, id: &str) -> bool {
        if self.ids.contains_key(id) {
            return true;
        }
        false
    }

    /// Add adds an image (via its hash) to the store.
    /// The provided ID of the video and the index of the screenshot is the value
    /// that will be returned as the result of a similarity query. If an ID is
    /// already in the store, it is not added again.
    pub fn add(
        &mut self,
        connection: &mut rusqlite::Connection,
        id: &str,
        video: &crate::videocandidate::VideoCandidate,
        _runtime: u32,
    ) {
        if self.ids.contains_key(id) {
            return;
        }
        let candidate_id = self.num_candidates + 1;
        // Store the compact format (no Haar matrix) to save ~400 KB/screenshot.
        // The matrix is only needed to derive bucket locations at add() time,
        // which we do below from the in-memory hash before it's dropped.
        let mut blob = Vec::new();
        video.encode_compact(&mut blob);
        log::warn!(
            "Inserting Video information of length {} and data size of {} KB",
            video.runtime,
            blob.len() / 1024
        );

        let tx = match connection.transaction() {
            Ok(tx) => tx,
            Err(error) => {
                log::error!(
                    "Failed to begin transaction for video {}: {}",
                    video.id,
                    error
                );
                return;
            }
        };

        match tx.execute(
            "INSERT INTO videostore_candidates (candidate_id, filename, video_id, data) VALUES (?1, ?2, ?3, ?4)",
            params![&candidate_id, &video.id, &video.index, &blob],
        ) {
            Ok(_retval) => {}
            Err(error) => {
                log::error!("Failed to insert video {} into database: {}", video.id, error);
                return;
            }
        }

        // Derive each screenshot's significant bucket locations ONCE here, while
        // the in-memory hash still carries the full matrix. The persisted blob
        // (compact, no matrix) cannot re-derive these, so this is the single
        // place the matrix matters after `add()` begins.
        for i in 0..video.screenshots.len() {
            let screenshot_id = video.screenshots[i].screenshot_id;
            for (location, _bin) in Self::hash_bucket_locations(&video.screenshots[i].hash) {
                if !self.num_indices.contains_key(&location) {
                    self.num_indices.insert(location, 0);
                }
                let arrayindex = self.num_indices[&location] as u32 + 1;
                let index_id = self.num_index_values + 1;
                match tx.execute(
                    "INSERT INTO videostore_indices (index_id, location, arrayindex, filename, video_id, screenshot_id, runtime) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![&index_id, location, &arrayindex, &video.id, &video.index, &screenshot_id, &video.runtime],
                ) {
                    Ok(_retval) => {}
                    Err(error) => {
                        log::error!("Failed to insert index for video {}: {}", video.id, error);
                        return;
                    }
                }
                *self.num_indices.get_mut(&location).unwrap() += 1;
                self.num_index_values += 1;
            }
        }

        if let Err(error) = tx.commit() {
            log::error!(
                "Failed to commit transaction for video {}: {}",
                video.id,
                error
            );
            return;
        }

        // Update in-memory state after successful commit.
        self.num_candidates += 1;
        self.video_ids.insert(video.index, candidate_id as usize);
        self.ids.insert(id.to_string(), candidate_id as usize);
        self.append_index(video, candidate_id);
        self.modified = true;
    }

    pub fn ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        for (id, _) in self.ids.iter() {
            v.push(id.clone());
        }
        v
    }

    /// Delete removes an image from the store so it will not be returned during a
    /// query anymore. Note that the candidate slot still remains occupied but its
    /// index will be removed from all index lists. This also means that Size() will
    /// not decrease. This is an expensive operation. If the provided ID could not be
    /// found, nothing happens.
    pub fn delete(&mut self, connection: &mut rusqlite::Connection, id: &str) {
        if !self.ids.contains_key(id) {
            return;
        }
        // Get the index.
        //let index = self.ids[id];
        // Read the 1-based candidate_id from the DB (the source of truth). The
        // in-memory `self.ids` values use inconsistent 0/1-based conventions
        // depending on whether the row was added vs. loaded, so we don't derive
        // the candidate_id from them.
        let mut candidate_id: Option<u32> = None;
        let query = "SELECT candidate_id, video_id FROM videostore_candidates WHERE filename = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => match statement.query(params![id]) {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next() {
                        if candidate_id.is_none() {
                            if let Ok(cid) = row.get::<_, u32>(0) {
                                candidate_id = Some(cid);
                            }
                        }
                        if let Ok(vid) = row.get::<_, u32>(1) {
                            self.video_ids.remove(&vid);
                        }
                    }
                }
                Err(err) => {
                    log::error!(
                        "could not read line from videostore_candidates database: {}",
                        err
                    );
                }
            },
            Err(error) => {
                log::error!("Failed to get video_id for {} from database: {}", id, error);
                return;
            }
        }
        self.modified = true;
        // clear the entry in the candidates list without deleting it
        let ret = connection.execute(
            "DELETE FROM videostore_candidates WHERE filename = ?1",
            params![&id],
        );
        if ret.is_err() {
            log::error!("Failed to delete candidate {}!", id);
            return;
        }
        self.ids.remove(id);
        self.num_candidates -= 1;

        // Remove from all index lists.
        let ret = connection.execute(
            "DELETE FROM videostore_indices WHERE filename = ?1",
            params![&id],
        );
        if ret.is_err() {
            log::error!("Failed to delete indices for {}!", id);
            return;
        }
        // Incrementally remove from the in-memory index (§8).
        if let Some(cid) = candidate_id {
            self.index.remove_video(cid);
        }
    }

    /// Exchange exchanges the ID of an image for a new one. If the old ID could not
    /// be found, nothing happens. If the new ID already existed prior to the
    /// exchange, the function returns immediately.
    ///
    pub fn exchange(
        &mut self,
        connection: &mut rusqlite::Connection,
        oldid: &str,
        newid: &str,
    ) -> bool {
        if !self.ids.contains_key(oldid) {
            return false;
        }
        if self.ids.contains_key(newid) {
            return false;
        }
        let index = self.ids[oldid];
        // update the ids
        self.ids.remove(oldid);
        self.ids.insert(newid.to_string(), index);

        // Update the candidate.
        let ret = connection.execute(
            "UPDATE videostore_candidates SET filename = ?1 WHERE filename = ?2",
            params![&newid, &oldid],
        );
        if ret.is_err() {
            log::error!("Failed to update candidate {}!", oldid);
            return false;
        }
        true
    }

    /// Compute a cheap fingerprint for cache-keying the significant-location
    /// list. Two hashes with the same dhash and thresholds will have the same
    /// significant-coefficient pattern in practice.
    fn hash_fingerprint(hash: &crate::hash::Hash) -> u64 {
        use std::hash::{Hash as _, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if let Some(d0) = hash.dhash.first() {
            d0.hash(&mut h);
        }
        if let Some(d1) = hash.dhash.get(1) {
            d1.hash(&mut h);
        }
        for ch in 0..hash.thresholds.c.len() {
            hash.thresholds.c[ch].to_bits().hash(&mut h);
        }
        h.finish()
    }

    /// Compute the list of significant `(location, bin)` pairs for a hash.
    /// This is the per-coefficient work that `search_matches` used to redo
    /// on every query position; now it's computed once and cached (§7).
    fn compute_score_plan(hash: &crate::hash::Hash) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        if hash.matrix.coefs.len() < 2 {
            return out;
        }
        for coefindex in 1..hash.matrix.coefs.len() {
            let coef = &hash.matrix.coefs[coefindex];
            let y = coefindex / hash.matrix.width as usize;
            let x = coefindex % hash.matrix.width as usize;
            let mut bin = y as u32;
            if (x as u32) > bin {
                bin = x as u32;
            }
            if bin > 5 {
                bin = 5;
            }
            for colorindex in 0..coef.c.len() {
                let colorcoef = coef.c[colorindex];
                if !crate::hash::coefficient_is_significant(
                    colorcoef,
                    hash.thresholds.c[colorindex],
                ) {
                    continue;
                }
                let sign = if colorcoef < 0.0 { 1u32 } else { 0u32 };
                let location = sign * IMAGESCALE * IMAGESCALE * crate::haar::COLOURCHANNELS
                    + coefindex as u32 * crate::haar::COLOURCHANNELS
                    + colorindex as u32
                    + 1;
                out.push((location, bin));
            }
        }
        out
    }

    /// Find all similar screenshots for a single screenshot.
    /// Reads buckets and per-screenshot metadata from the in-memory index.
    ///
    /// `location_cache` memoizes the significant `(location, bin)` list per
    /// distinct hash so that identical frames across screenshot positions
    /// don't re-scan all 16,384 coefficients (§7).
    fn search_matches(
        &self,
        _client: &mut rusqlite::Connection,
        hash: &crate::hash::Hash,
        _video_ids: &std::collections::BTreeMap<u32, usize>,
        scores: &mut Vec<f64>,
        touched: &mut Vec<usize>,
        location_cache: &mut std::collections::HashMap<u64, Vec<(u32, u32)>>,
    ) -> crate::videomatches::VideoMatches {
        let mut ms = crate::videomatches::VideoMatches::new();
        if self.index.num_screenshots == 0 {
            return ms;
        }

        // Get (or compute and cache) the significant-location plan for this hash.
        let fp = Self::hash_fingerprint(hash);
        let plan: Vec<(u32, u32)>;
        if let Some(cached) = location_cache.get(&fp) {
            plan = cached.clone();
        } else {
            plan = Self::compute_score_plan(hash);
            location_cache.insert(fp, plan.clone());
        }

        // Examine hash buckets using the precomputed plan.
        for &(location, bin) in &plan {
            let bucket = &self.index.buckets[location as usize];
            for &g in bucket {
                if scores[g as usize].is_nan() {
                    let mut score: f64 = 0.0;
                    for colorid in 0..3 {
                        score += WEIGHTS[colorid][0];
                    }
                    scores[g as usize] = score;
                    touched.push(g as usize);
                }
                scores[g as usize] -= WEIGHTSUMS[bin as usize];
            }
        }
        // Create matches from only the touched (scored) indices — avoids a
        // full-corpus scan on every query position.
        for &g in touched.iter() {
            let score = scores[g];
            if score > self.sensitivity {
                continue;
            }
            let shot = &self.index.screenshots[g];
            let mut m = crate::videomatches::VideoMatch::new();
            m.video_id = shot.video_id;
            m.screenshot_id = shot.screenshot_id;
            m.timecode = shot.timecode;
            m.score = score;
            m.ratio_diff = shot.ratio.log(10.0).abs() - hash.ratio.log(10.0);
            m.dhash_distance = crate::hamming::hamming_distance(shot.dhash0, hash.dhash[0])
                + crate::hamming::hamming_distance(shot.dhash1, hash.dhash[1]);
            m.histogram_distance = crate::hamming::hamming_distance(shot.histogram, hash.histogram);
            // Look up the candidate's filename via the in-memory metas.
            if let Some(&video_pos) = self.index.video_ids.get(&shot.video_id) {
                m.id = self.index.metas[video_pos].id.clone();
            }
            if m.score < self.sensitivity {
                ms.m.push(m);
            }
        }
        // Reset touched scores to NaN so the next call starts clean.
        for &g in touched.iter() {
            scores[g] = f64::NAN;
        }
        touched.clear();
        ms.sort();
        ms
    }

    /// create a score value for a *similar* video in comparison to duration, resulution. ...
    /// and return a Match
    fn rate_match(
        &self,
        _client: &mut rusqlite::Connection,
        new_video: &crate::videocandidate::VideoCandidate,
        match_id: u32,
        screenshot_id: u32,
        num_matches: usize,
        time_between_screenshots: u32,
    ) -> crate::videomatches::VideoMatch {
        let mut m = crate::videomatches::VideoMatch::new();
        // Read the matched candidate's metadata from the in-memory index.
        if let Some(&video_pos) = self.index.video_ids.get(&match_id) {
            let matched = &self.index.metas[video_pos];
            m.id = matched.id.clone();
            m.video_id = match_id;
            m.screenshot_id = screenshot_id;
            m.timecode = screenshot_id * time_between_screenshots;
            m.score = -60.0
                - 100.0 * (num_matches as f64 * time_between_screenshots as f64)
                    / matched.runtime as f64
                + ((new_video.width - matched.width) * (new_video.width - matched.width)) as f64;
        }
        m
    }

    /// Query performs a similarity search on the given image hashes and returns
    /// all potential matches. The returned slice will sort the match with the best score as its
    /// first element.
    ///
    /// Videos consist of one screenshot every ten seconds.
    /// A Match contains a portion of at least a minute (six similar screenshots in a row)
    /// The longer the sequence the better the match.
    ///
    pub fn query(
        &self,
        client: &mut rusqlite::Connection,
        video: &crate::videocandidate::VideoCandidate,
    ) -> crate::videomatches::VideoMatches {
        let mut ms = crate::videomatches::VideoMatches::new();
        if self.num_candidates == 0 {
            return ms;
        }
        let mut sequences: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();
        let mut active_sequence_counter = std::collections::BTreeMap::new();
        let screenshot_index_global = self.index.num_screenshots;
        // Hoist the scoring scratch vector: allocate once per query, reset only
        // touched indices between positions (avoids O(corpus) alloc+fill per call).
        let mut scores: Vec<f64> = vec![f64::NAN; screenshot_index_global as usize];
        let mut touched: Vec<usize> = Vec::new();
        // §7: cache significant-location plans per distinct hash so identical
        // frames across positions don't re-scan all 16,384 coefficients.
        let mut location_cache: std::collections::HashMap<u64, Vec<(u32, u32)>> =
            std::collections::HashMap::new();

        // search for each screenshot of the current video in the store
        let report_interval = (5 * 60 / self.num_seconds_between_screenshots) as usize;
        for screenshot_pos in 0..video.screenshots.len() {
            let hash = &video.screenshots[screenshot_pos].hash;
            let matches = self.search_matches(
                client,
                hash,
                &self.index.video_ids,
                &mut scores,
                &mut touched,
                &mut location_cache,
            );
            let mut previous_videos = std::collections::BTreeSet::new();
            for (key, _) in sequences.iter() {
                previous_videos.insert(*key);
            }
            let mut new_videos = std::collections::BTreeSet::new();
            if screenshot_pos % report_interval == 0 {
                log::warn!("Comparing new video with the databse. Currently at position {} Minutes into the video",
                            (screenshot_pos as u32 / self.min_similar_screenshots_in_sequence) as u32);
            }
            // `matches` is sorted best-first and may contain several entries for
            // the same video (different screenshots of that video matching this
            // query screenshot). For each video we want exactly ONE representative
            // screenshot_id: the one that *extends the current sequence* if
            // possible (i.e. screenshot_id == last + 1), otherwise the best-scored
            // one. Picking the best score blindly breaks continuity whenever a
            // video has several equally-similar screenshots (e.g. a self-match of
            // near-identical frames), which is why we prefer the contiguous one.
            //
            // Collect, per video, the candidate screenshot_ids sorted best-first.
            let mut by_video: std::collections::BTreeMap<u32, Vec<u32>> =
                std::collections::BTreeMap::new();
            for i in 0..matches.len() {
                by_video
                    .entry(matches.m[i].video_id)
                    .or_default()
                    .push(matches.m[i].screenshot_id);
            }

            for (video_id, shot_ids) in by_video {
                new_videos.insert(video_id);
                if !active_sequence_counter.contains_key(&video_id) {
                    active_sequence_counter.insert(video_id, 1 as u32);
                } else if let Some(x) = active_sequence_counter.get_mut(&video_id) {
                    *x += 1;
                }
                // Choose the representative screenshot_id for this position.
                let chosen = if let Some(x) = sequences.get(&video_id) {
                    let want = x.last().copied().map(|last| last + 1);
                    // Prefer a shot_id that continues the sequence.
                    match want.and_then(|w| shot_ids.iter().copied().find(|&s| s == w)) {
                        Some(s) => s,
                        // No contiguous continuation: keep the best-scored one
                        // (first in the best-first list).
                        None => shot_ids[0],
                    }
                } else {
                    shot_ids[0]
                };
                if !sequences.contains_key(&video_id) {
                    sequences.insert(video_id, Vec::from([chosen]));
                } else if let Some(x) = sequences.get_mut(&video_id) {
                    if x[x.len() - 1] + 1 == chosen {
                        x.push(chosen);
                    } else {
                        // broken sequence: restart from this screenshot
                        x.clear();
                        x.push(chosen);
                    }
                }
            }
            // remove the videos from the sequence that were not a match
            let dropped_videos = previous_videos.difference(&new_videos);
            for id in dropped_videos {
                // check if the sequence was longer than a minute amd add the ones long enough
                if sequences[id].len() > 0 {
                    let videomatch = self.rate_match(
                        client,
                        &video,
                        *id,
                        screenshot_pos as u32,
                        sequences[id].len(),
                        self.num_seconds_between_screenshots,
                    );
                    if videomatch.id.len() != 0 {
                        ms.m.push(videomatch);
                    }
                } else {
                    let name = self
                        .index
                        .video_ids
                        .get(id)
                        .map(|&p| self.index.metas[p].id.clone())
                        .unwrap_or_else(|| format!("<candidate {}>", id));
                    log::error!(
                        "Candidate {} had {} matching Screenshots!",
                        name,
                        sequences[id].len()
                    );
                }
                sequences.remove(id);
            }
            new_videos.clear();
        }
        // done parsing, add everything with more than 5 matches in a row to the list
        for (video_id, v) in sequences {
            if v.len() >= self.min_similar_screenshots_in_sequence as usize {
                let mut m = crate::videomatches::VideoMatch::new();
                if let Some(&video_pos) = self.index.video_ids.get(&video_id) {
                    let matchedvideo = &self.index.metas[video_pos];
                    m.id = matchedvideo.id.clone();
                    m.video_id = video_id;
                    m.screenshot_id = 0;
                    m.timecode = 0;
                    m.score = -60.0 - 100.0 * (v.len() as f64 * 10.0) / matchedvideo.runtime as f64
                        + ((video.width - matchedvideo.width) * (video.width - matchedvideo.width))
                            as f64;
                }
                if m.id.len() != 0 {
                    ms.m.push(m);
                }
            }
        }
        ms.sort();
        ms
    }

    pub fn return_candidate(
        &self,
        connection: &mut rusqlite::Connection,
        video_id: u32,
    ) -> (u32, crate::videocandidate::VideoCandidate) {
        let mut v = crate::videocandidate::VideoCandidate::new();
        let mut candidate_id = 0;
        // The DB stores video_id = candidate_id (1-based).
        // The caller may pass either a 0-based index (from store.ids) or a
        // 1-based candidate_id. Try the given value first, then +1.
        let query = "SELECT candidate_id, data FROM videostore_candidates WHERE video_id = ?1";
        for attempt in [video_id, video_id.saturating_add(1)] {
            if candidate_id != 0 {
                break;
            }
            match connection.prepare(query) {
                Ok(mut statement) => match statement.query(params![&attempt]) {
                    Ok(mut rows) => {
                        if let Ok(Some(row)) = rows.next() {
                            candidate_id = row.get(0).unwrap_or(0);
                            if let Ok(blob) = row.get::<_, Vec<u8>>(1) {
                                v.decode_any(&mut std::io::Cursor::new(blob));
                            }
                        }
                    }
                    Err(err) => {
                        log::error!(
                            "could not read line from videostore_candidates database: {}",
                            err
                        );
                    }
                },
                Err(err) => {
                    log::error!("could not prepare SQL statement: {}", err);
                }
            }
        }
        (candidate_id, v)
    }

    pub fn return_indice(
        &self,
        connection: &mut rusqlite::Connection,
        location: u32,
    ) -> Vec<ScreenshotIndex> {
        let mut v = Vec::new();
        let query = "SELECT filename, video_id, screenshot_id, runtime FROM videostore_indices WHERE location = ?1";
        match connection.prepare(query) {
            Ok(mut statement) => {
                match statement.query(params![&location]) {
                    Ok(mut rows) => {
                        loop {
                            match rows.next() {
                                Ok(Some(row)) => {
                                    let mut s = ScreenshotIndex::new();
                                    match row.get(0) {
                                        Ok(val) => s.id = val,
                                        Err(error) => {
                                            log::error!("Failed to read id for video: {}", error);
                                            continue;
                                        }
                                    }
                                    match row.get(1) {
                                        Ok(val) => s.video_id = val,
                                        Err(error) => {
                                            log::error!(
                                                "Failed to read video_id for video: {}",
                                                error
                                            );
                                            continue;
                                        }
                                    }
                                    match row.get(2) {
                                        Ok(val) => s.screenshot_id = val,
                                        Err(error) => {
                                            log::error!(
                                                "Failed to read screenshot_id for video: {}",
                                                error
                                            );
                                            continue;
                                        }
                                    }
                                    match row.get(3) {
                                        Ok(val) => s.runtime = val,
                                        Err(error) => {
                                            log::error!(
                                                "Failed to read runtime for video: {}",
                                                error
                                            );
                                            continue;
                                        }
                                    }
                                    v.push(s);
                                }
                                Ok(None) => {
                                    //log::warn!("No data read from indices.");
                                    break;
                                }
                                Err(error) => {
                                    log::error!("Failed to read a row from indices: {}", error);
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        log::error!(
                            "could not read line from videostore_indices database: {}",
                            err
                        );
                    }
                }
            }
            Err(err) => {
                log::error!("could not prepare SQL statement: {}", err);
            }
        }
        v
    }

    pub fn size(&self) -> usize {
        self.num_candidates as usize
    }

    pub fn modified(&self) -> bool {
        self.modified
    }

    // encode the data structure to binary stream
    pub fn encode(&self, to: &mut Vec<u8>) {
        crate::marshal::store_u32(self.num_candidates, to);
        crate::marshal::store_hash_string_usize(&self.ids, to);
        crate::marshal::store_hash_u32_usize(&self.video_ids, to);
        crate::marshal::store_hash_u32_usize(&self.num_indices, to);
        crate::marshal::store_u32(self.num_index_values, to);
        crate::marshal::store_f64(self.sensitivity, to);
        crate::marshal::store_bool(self.modified, to);
    }

    // decode data structure from binary stream
    pub fn decode(&mut self, from: &mut std::io::Cursor<Vec<u8>>) {
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.num_candidates = r;
        }
        self.ids
            .extend(crate::marshal::restore_hash_string_usize(from));
        self.video_ids
            .extend(crate::marshal::restore_hash_u32_usize(from));
        self.num_indices
            .extend(crate::marshal::restore_hash_u32_usize(from));
        if let Some(r) = crate::marshal::restore_u32(from) {
            self.num_index_values = r;
        }
        if let Some(r) = crate::marshal::restore_f64(from) {
            self.sensitivity = r;
        }
        if let Some(r) = crate::marshal::restore_bool(from) {
            self.modified = r;
        }
        self.modified = false;
    }

    // Write binary stream to file
    pub fn dump_binary(&self, storefile: &str) {
        let mut buffer = Vec::new();
        self.encode(&mut buffer);
        let path = std::path::Path::new(&storefile);
        let display = path.display();
        // Open a file in write-only mode, returns `io::Result<File>`
        let mut write_file = match std::fs::File::create(&path) {
            Err(why) => panic!("couldn't create {}: {}", display, why),
            Ok(write_file) => write_file,
        };
        let ret = write_file.write(&mut buffer);
        if ret.is_err() {
            log::error!("failed to write binary data to file {}!", &storefile)
        }
        let ret = write_file.flush();
        if ret.is_err() {
            log::error!(
                "failed to empty the write buffer for binary data to file {}!",
                &storefile
            )
        }
    }

    // read binary stream from file
    pub fn slurp_binary(&mut self, storefile: &str, client: &mut rusqlite::Connection) {
        let path = std::path::Path::new(&storefile);
        let display = path.display();
        let mut input_file = match std::fs::File::open(&path) {
            Err(why) => panic!("couldn't open {}: {}", display, why),
            Ok(read_file) => read_file,
        };
        let mut buf = Vec::new();
        let ret = input_file.read_to_end(&mut buf);
        if ret.is_err() {
            log::error!("Failed to read test file back in.");
            return;
        }
        let mut read_file = std::io::Cursor::new(buf);
        self.decode(&mut read_file);
        let mut keys = Vec::new();
        for (key, _val) in self.ids.iter_mut() {
            keys.push(key.to_string());
        }
        for key in keys {
            let filepath = std::path::Path::new(&key);
            if !filepath.is_file() {
                // video has vanished, remove it from the store
                self.delete(client, &key);
            }
        }
        // Rebuild the in-memory index from the database, since the binary
        // file does not contain it.
        self.bulk_load_index(client);
    }
}

// ============================================================================
// VideoStore: thread-safe wrapper around VideoStoreInner.
//
//   - The mutable state lives in VideoStoreInner, guarded by an RwLock so that
//     any number of reader threads can call `query()` concurrently while a
//     single writer thread serialises `add()` / `delete()` / `exchange()`.
//   - The SQLite connection is shared through Arc<Mutex<Connection>>. With
//     WAL mode, concurrent readers and a single writer are safe.
//
// This is what makes the demo's producer/consumer pipeline possible: N query
// workers share the same store, and the single-writer invariant you asked for
// is enforced by the RwLock's write guard.
// ============================================================================
#[derive(Debug)]
pub struct VideoStore {
    inner: RwLock<VideoStoreInner>,
    conn: Arc<Mutex<rusqlite::Connection>>,
    /// Shared stop flag.  Set to `true` to signal every worker (producer,
    /// query, writer) to wind down promptly.  Wired to Ctrl-C in the demo
    /// and usable as a programmatic abort mechanism from any thread.
    stop_flag: Arc<AtomicBool>,
}

impl VideoStore {
    /// Create a new VideoStore, loading existing data from the database and
    /// building the in-memory index. The connection is owned by the store from
    /// now on (cloned into an Arc).
    pub fn new(
        connection: rusqlite::Connection,
        sensitivity: f64,
        start_directory: &str,
        num_threads: u32,
        num_seconds_between_screenshots: u32,
        min_similar_screenshots_in_sequence: u32,
        max_candidates_in_cache: usize,
    ) -> Self {
        let conn = Arc::new(Mutex::new(connection));
        // Take the connection, build the inner store, then return it to the Arc.
        let mut inner = {
            let mut conn_guard = conn.lock().unwrap();
            let inner = VideoStoreInner::new(
                &mut *conn_guard,
                sensitivity,
                start_directory,
                num_threads,
                num_seconds_between_screenshots,
                min_similar_screenshots_in_sequence,
                max_candidates_in_cache,
            );
            inner
        };
        // `new()` already called bulk_load_index internally.
        let _ = &mut inner;
        let store = VideoStore {
            inner: RwLock::new(inner),
            conn,
            stop_flag: Arc::new(AtomicBool::new(false)),
        };
        // Migrate existing v1 blobs (full matrix) to v2 compact format.
        // This is a no-op if the database already has the `schema_version`
        // column set to >= 2, or if the table is empty.
        store.migrate_to_compact_storage();
        store
    }

    /// Migrate the database from v1 (full Haar matrix per screenshot, ~400 KB)
    /// to v2 (compact, no matrix, ~2 KB per screenshot). Idempotent: safe to
    /// call multiple times.
    ///
    /// The `videostore_indices` table already stores the derived bucket
    /// locations, so no index re-computation is needed. We simply re-encode
    /// each `videostore_candidates.data` BLOB from v1 to v2.
    fn migrate_to_compact_storage(&self) {
        let mut conn_guard = self.conn.lock().unwrap();

        // Check if schema_version column exists.
        let has_version_col: bool = conn_guard
            .prepare("SELECT name FROM pragma_table_info('videostore_parameters') WHERE name = 'schema_version'")
            .and_then(|mut stmt| stmt.exists(params![]))
            .unwrap_or(false);

        if !has_version_col {
            let _ = conn_guard.execute(
                "ALTER TABLE videostore_parameters ADD COLUMN schema_version BIGINT DEFAULT 1",
                (),
            );
            // Mark all existing rows as v1.
            let _ = conn_guard.execute(
                "UPDATE videostore_parameters SET schema_version = 1 WHERE schema_version IS NULL",
                (),
            );
        }

        let current_version: i64 = conn_guard
            .query_row(
                "SELECT COALESCE(MAX(schema_version), 1) FROM videostore_parameters",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);

        if current_version >= 2 {
            return; // Already migrated.
        }

        // Re-encode all v1 blobs to v2 compact format.
        let count: i64 = conn_guard
            .query_row("SELECT COUNT(*) FROM videostore_candidates", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);

        if count == 0 {
            // Nothing to migrate; just bump the version.
            let _ = conn_guard.execute("UPDATE videostore_parameters SET schema_version = 2", ());
            return;
        }

        log::warn!(
            "Migrating {} candidate blob(s) from v1 to v2 compact format…",
            count
        );

        let tx = match conn_guard.transaction() {
            Ok(tx) => tx,
            Err(e) => {
                log::error!("Migration: failed to begin transaction: {}", e);
                return;
            }
        };

        // Read all candidate IDs and their blobs, re-encode, and write back.
        let mut rows: Vec<(i64, Vec<u8>)> = Vec::new();
        {
            let stmt = tx
                .prepare("SELECT candidate_id, data FROM videostore_candidates")
                .ok();
            if let Some(mut stmt) = stmt {
                if let Ok(rows_iter) = stmt.query_map(params![], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                }) {
                    for row in rows_iter.flatten() {
                        rows.push(row);
                    }
                }
            }
        }

        let mut migrated = 0;
        for (candidate_id, blob) in &rows {
            // Detect v1: first byte is NOT 2 (v2 marker).
            if blob.first() == Some(&2) {
                continue; // Already v2.
            }
            // Decode v1 (full format), re-encode as v2 (compact).
            let mut candidate = crate::videocandidate::VideoCandidate::new();
            candidate.decode(&mut std::io::Cursor::new(blob.clone()));
            let mut compact = Vec::new();
            candidate.encode_compact(&mut compact);
            if let Err(e) = tx.execute(
                "UPDATE videostore_candidates SET data = ?1 WHERE candidate_id = ?2",
                params![&compact, candidate_id],
            ) {
                log::error!(
                    "Migration: failed to update candidate {}: {}",
                    candidate_id,
                    e
                );
                continue;
            }
            migrated += 1;
        }

        // Bump the schema version.
        let _ = tx.execute("UPDATE videostore_parameters SET schema_version = 2", ());

        if let Err(e) = tx.commit() {
            log::error!("Migration: failed to commit: {}", e);
            return;
        }

        log::warn!(
            "Migration complete: {} blob(s) re-encoded to compact format",
            migrated
        );
    }

    /// Explain the scoring for a single screenshot hash: print the top bucket
    /// hits with their accumulated scores. Useful for sensitivity tuning (§15).
    pub fn explain_screenshot(&self, hash: &crate::hash::Hash, top_n: usize) {
        let inner = self.inner.read().unwrap();
        let index = &inner.index;
        if index.num_screenshots == 0 {
            eprintln!("explain: index is empty");
            return;
        }

        let plan = VideoStoreInner::compute_score_plan(hash);
        if plan.is_empty() {
            eprintln!("explain: no significant coefficients for this hash");
            return;
        }

        let mut scores: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
        let base: f64 = (0..3).map(|c| WEIGHTS[c][0]).sum();
        for &(location, bin) in &plan {
            if (location as usize) >= index.buckets.len() {
                continue;
            }
            for &g in &index.buckets[location as usize] {
                let entry = scores.entry(g).or_insert(base);
                *entry -= WEIGHTSUMS[bin as usize];
            }
        }

        let mut sorted: Vec<(u32, f64)> = scores.into_iter().collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        eprintln!(
            "explain: {} scored candidates (showing top {})",
            sorted.len(),
            top_n
        );
        eprintln!("  sensitivity threshold: {}", inner.sensitivity);
        for &(g, score) in sorted.iter().take(top_n) {
            if (g as usize) >= index.screenshots.len() {
                continue;
            }
            let shot = &index.screenshots[g as usize];
            let name = index
                .video_ids
                .get(&shot.video_id)
                .and_then(|&pos| index.metas.get(pos))
                .map(|m| m.id.as_str())
                .unwrap_or("<unknown>");
            let dhash_dist = crate::hamming::hamming_distance(
                shot.dhash0,
                hash.dhash.first().copied().unwrap_or(0),
            ) + crate::hamming::hamming_distance(
                shot.dhash1,
                hash.dhash.get(1).copied().unwrap_or(0),
            );
            let hist_dist = crate::hamming::hamming_distance(shot.histogram, hash.histogram);
            eprintln!(
                "  g={} {} shot={} tc={}s score={:.2} dhash={} hist={}",
                g, name, shot.screenshot_id, shot.timecode, score, dhash_dist, hist_dist
            );
        }
    }

    /// Print a one-line metrics summary for the in-memory index (§15).
    pub fn print_metrics(&self) {
        let inner = self.inner.read().unwrap();
        let total_entries: usize = inner.index.buckets.iter().map(|b| b.len()).sum();
        let non_empty: usize = inner.index.buckets.iter().filter(|b| !b.is_empty()).count();
        eprintln!(
            "metrics: {} videos | {} screenshots | {} non-empty buckets | {} total bucket entries",
            inner.num_candidates, inner.index.num_screenshots, non_empty, total_entries
        );
    }

    /// Run a similarity query. Takes a read lock, so many can run in parallel.
    pub fn query(
        &self,
        video: &crate::videocandidate::VideoCandidate,
    ) -> crate::videomatches::VideoMatches {
        let inner = self.inner.read().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.query(&mut *conn_guard, video)
    }

    /// Add a video to the store. Takes a write lock, so only one at a time.
    pub fn add(&self, id: &str, video: &crate::videocandidate::VideoCandidate, runtime: u32) {
        let mut inner = self.inner.write().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.add(&mut *conn_guard, id, video, runtime)
    }

    pub fn delete(&self, id: &str) {
        let mut inner = self.inner.write().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.delete(&mut *conn_guard, id)
    }

    // ------------------------------------------------------------------
    // §9 — graceful shutdown / back-pressure support.
    // ------------------------------------------------------------------

    /// Returns a `Clone`-able handle to the store's stop flag so that worker
    /// threads can check it without holding the store lock.
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Convenience: set the stop flag to `true`.  Every thread holding a
    /// clone of the flag will observe it on the next check.
    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if a stop has been requested.
    pub fn stop_requested(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst)
    }

    pub fn exchange(&self, oldid: &str, newid: &str) -> bool {
        let mut inner = self.inner.write().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.exchange(&mut *conn_guard, oldid, newid)
    }

    pub fn has(&self, id: &str) -> bool {
        let inner = self.inner.read().unwrap();
        inner.has(id)
    }

    pub fn ids(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap();
        inner.ids()
    }

    pub fn size(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.size()
    }

    pub fn modified(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.modified()
    }

    /// Fetch a full candidate (all screenshots + hashes) for presentation.
    pub fn return_candidate(&self, video_id: u32) -> (u32, crate::videocandidate::VideoCandidate) {
        let inner = self.inner.read().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.return_candidate(&mut *conn_guard, video_id)
    }

    /// Read-only access to a configuration field (for the demo's progress logic).
    pub fn num_candidates(&self) -> u32 {
        let inner = self.inner.read().unwrap();
        inner.num_candidates
    }

    pub fn num_threads(&self) -> u32 {
        let inner = self.inner.read().unwrap();
        inner.num_threads
    }

    pub fn num_seconds_between_screenshots(&self) -> u32 {
        let inner = self.inner.read().unwrap();
        inner.num_seconds_between_screenshots
    }

    /// Look up the 0-based candidate index for a filename (for the demo).
    pub fn ids_lookup(&self, id: &str) -> Option<usize> {
        let inner = self.inner.read().unwrap();
        inner.ids.get(id).copied()
    }

    /// Test/debug helper: report how many screenshots are in the in-memory
    /// index and the total number of bucket entries.
    pub fn debug_index_stats(
        &self,
    ) -> (
        u32,   /*num_screenshots*/
        usize, /*total_bucket_entries*/
    ) {
        let inner = self.inner.read().unwrap();
        (
            inner.index.num_screenshots,
            inner.index.buckets.iter().map(|b| b.len()).sum(),
        )
    }

    /// Look up a stored video by content fingerprint (renamed-duplicate
    /// detection, §16.4). Returns the filename of the first stored video whose
    /// (width, height, runtime, first+last screenshot dhash/histogram) matches
    /// the given candidate, or `None` if no match is found.
    ///
    /// This is O(num_videos) over the in-memory index — cheap because it only
    /// reads the compact `VideoMeta` + first/last `ScreenMeta` per video.
    pub fn content_fingerprint_lookup(
        &self,
        candidate: &crate::videocandidate::VideoCandidate,
    ) -> Option<String> {
        let inner = self.inner.read().unwrap();
        let target = candidate.content_fingerprint();
        let index = &inner.index;

        for meta in index.metas.iter() {
            // Reconstruct the fingerprint from the stored meta + first/last screenshot.
            use std::hash::{Hash as _, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            meta.width.hash(&mut h);
            meta.height.hash(&mut h);
            meta.runtime.hash(&mut h);
            // Find this video's screenshots.
            let video_shots: Vec<_> = index
                .screenshots
                .iter()
                .filter(|s| s.video_id == meta.candidate_id)
                .collect();
            if let Some(first) = video_shots.first() {
                first.dhash0.hash(&mut h);
                first.dhash1.hash(&mut h);
                first.histogram.hash(&mut h);
            }
            if let Some(last) = video_shots.last() {
                last.dhash0.hash(&mut h);
                last.dhash1.hash(&mut h);
                last.histogram.hash(&mut h);
            }
            if h.finish() == target {
                return Some(meta.id.clone());
            }
        }
        None
    }

    /// Test/debug helper: how many screenshots are filed under a given location.
    pub fn debug_bucket_len(&self, location: u32) -> usize {
        let inner = self.inner.read().unwrap();
        if (location as usize) < inner.index.buckets.len() {
            inner.index.buckets[location as usize].len()
        } else {
            0
        }
    }

    /// Dump the store to a binary file.
    pub fn dump_binary(&self, storefile: &str) {
        let inner = self.inner.read().unwrap();
        inner.dump_binary(storefile)
    }

    /// Load the store from a binary file.
    pub fn slurp_binary(&self, storefile: &str) {
        let mut inner = self.inner.write().unwrap();
        let mut conn_guard = self.conn.lock().unwrap();
        inner.slurp_binary(storefile, &mut *conn_guard)
    }
}

// Compile-time guarantee: VideoStore must be shareable across threads.
// `Send` lets us move it into a thread / Arc; `Sync` lets multiple reader
// threads call `query()` concurrently while one writer calls `add()`.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VideoStore>();
};

/// Make a connection to the database
/// This requires a running PostgreSQL server.
/// Also there has to be a valid user and a Database / Schema.
///
pub fn connect(dbpath: &str) -> Result<rusqlite::Connection, rusqlite::Error> {
    let path = std::path::Path::new(dbpath);
    let connection;
    if !path.is_file() {
        connection = Connection::open(dbpath)?;
        println!("{}", connection.is_autocommit());
        match connection.execute(
            "CREATE TABLE videostore_candidates (
                candidate_id UNSIGNED BIG INT PRIMARY KEY NOT NULL,
                filename TEXT NOT NULL unique,
                video_id UNSIGNED BIG INT NOT NULL,
                data BLOB
            )",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table candidates: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_videostore_candidates_filename ON videostore_candidates (filename)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on candidates: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_videostore_candidat_video_id ON videostore_candidates (video_id)",
            (),
        ) {
            Ok(_) => {}
            Err(error) => {
                log::error!("Failed to create index on candidates: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE videostore_indices (
                index_id UNSIGNED BIG INT PRIMARY KEY NOT NULL,
                location UNSIGNED BIG INT NOT NULL,
                arrayindex UNSIGNED BIG INT NOT NULL,
                filename TEXT NOT NULL,
                video_id UNSIGNED BIG INT NOT NULL,
                screenshot_id UNSIGNED BIG INT NOT NULL,
                runtime UNSIGNED BIG INT NOT NULL
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_videostore_indices_location ON videostore_indices (filename)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_videostore_indices_filename ON videostore_indices (filename)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "CREATE INDEX index_videostore_indices_video_id ON videostore_indices (video_id)",
            (),
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create index on indices: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "
            CREATE TABLE videostore_parameters (
                config_id UNSIGNED BIG INT PRIMARY KEY NOT NULL,
                sensitivity BIGINT,
                start_directory TEXT,
                num_threads UNSIGNED BIG INT,
                num_seconds_between_screenshots UNSIGNED BIG INT,
                min_similar_screenshots_in_sequence UNSIGNED BIG INT,
                max_candidates_in_cache UNSIGNED BIG INT
            )",
            [],
        ) {
            Ok(_ret) => {}
            Err(error) => {
                log::error!("Failed to create table parameters: {}", error);
                return Err(error);
            }
        }
        match connection.execute(
            "INSERT INTO videostore_parameters (config_id, sensitivity, start_directory, num_threads, num_seconds_between_screenshots, min_similar_screenshots_in_sequence, max_candidates_in_cache) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![&1, &-60, &"./", &2, &10, &6, &100],
        ) {
            Ok(retval) => log::warn!("Inserted {} data into parameter.", retval),
            Err(error) => {
                log::error!("Failed to insert data into parameter database: {}", error);
                return Err(error);
            }
        }
    } else {
        connection = Connection::open(dbpath)?;
    }
    // Enable WAL mode for concurrent readers + a single writer, and
    // NORMAL synchronous to avoid fsync on every commit (safe with WAL).
    let _ = connection.pragma_update(None, "journal_mode", "WAL");
    let _ = connection.pragma_update(None, "synchronous", "NORMAL");
    Ok(connection)
}
