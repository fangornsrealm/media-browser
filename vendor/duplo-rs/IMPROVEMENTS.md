# duplo-rs — Improvements Roadmap

A prioritized, code-grounded list of improvements for the similar-video (and
similar-image) pipeline. Every item names the file it touches and the reason it
matters. Items marked **[verified]** were confirmed by reading the code; items
marked **[likely]** are high-confidence inferences worth confirming with a
benchmark before investing.

Impact legend: **[H]**igh · **[M]**edium · **[L]**ow. Effort: **S** ≤ ~1 hr,
**M** a few hrs, **L** days.

## Status

| § | Item | Status | Commit |
|---|------|--------|--------|
| §3 | `hamming_distance` → `count_ones()` | ✅ Done | `60cab95` |
| §4 | `coef_threshold` → `select_nth_unstable` | ✅ Done | `60cab95` |
| §5 | Negative sensitivity → `i64` persistence | ✅ Done | `60cab95` |
| §2 | One-pass ffmpeg + `scale=128:-2` + temp dir + `Result` | ✅ Done | `e21fe3c` |
| §6 | Hoist `scores` scratch vector + touched-index reset | ✅ Done | `e21fe3c` |
| §10 | `VideoMatches::sort` → `sort_by` | ✅ Done | `e21fe3c` |
| §13 | Remove `process::exit(2)` / `panic!` in presentation | ✅ Done | `e21fe3c` |
| §14 | Video test suite (7 new tests) + 3 bug fixes | ✅ Done | `bd70469` |
| §1 | Compact per-screenshot storage (~200× BLOB reduction) + migration | ✅ Done | `0a4f0ef` |
| §12 | Remove vestigial state (`candidate_cache`, `num_images`) | ✅ Done | `207fe6a` |
| §7 | Cache significant-location list per hash (memoized in `search_matches`) | ✅ Done | `207fe6a` |
| §8 | `delete` incremental via `InMemoryIndex::remove_video` (no full rebuild) | ✅ Done | `207fe6a` |
| §9 | Concurrency: shared stop-flag + Ctrl-C handler + deadlock-free bounded pool | ✅ Done | `4f5e6db` |
| §11 | Marshalling endianness | ⏳ Deferred (format change; local files, same-arch — see note) | — |
| §15 | Observability: `explain_screenshot()` + `print_metrics()` | ✅ Done | `207fe6a` |
| §16.2 | Two-pass scoring (collect candidates, then score) | ✅ Done | (was already implemented) |
| §16.4 | Content-hash dedup (renamed duplicates) | ✅ Done | `9399ecd` |
| §16.1 | Purpose-built mmap index file | ⏳ Deferred (useful only at >50k videos) | — |
| §16.3 | LMDB instead of SQLite | ⏳ Deferred (not needed after §1 compact storage) | — |
| §16.5 | tokio-friendly facade | ⏳ Deferred (tool is not async) | — |

---

## 0. Read this first

- The hot path is: `create_screenshots` (ffmpeg) → `create_hash` → `add()`
  (index build) → `query()` → `search_matches()` (scoring). Improvements to the
  first two dominate wall-clock time; improvements to the last three dominate
  CPU per comparison.
- The single largest *storage* cost is the per-screenshot Haar `CoefMatrix`
  (128×128 × 3 `f64` ≈ 400 KB/screenshot, ~100 MB/video). The single largest
  *wall-clock* cost is screenshotting (one ffmpeg process **per frame**).
- Several of the "quick wins" below are one-line changes with no behavioral
  risk.

Quick-win summary (do these first):

| # | Item | Impact | Effort | Where |
|---|------|--------|--------|-------|
| 1 | Replace manual popcount in `hamming_distance` with `u64::count_ones()` | H | S | `src/hamming.rs` |
| 2 | One-pass ffmpeg screenshotting instead of one process per frame | H | M | `src/files.rs` `create_screenshots` |
| 3 | Quickselect for the kth-largest threshold instead of a full sort | M | S | `src/hash.rs` `coef_threshold` |
| 4 | Fix negative-sensitivity persistence (`f64 as u32` → 0) | M (correctness) | S | `src/videostore.rs` `new()` |
| 5 | Stop panicking when ffmpeg is missing | M (robustness) | S | `src/files.rs` |
| 6 | Reuse/hoist the `scores` scratch vector out of `search_matches` | M | S | `src/videostore.rs` |

---

## 1. Storage: stop persisting the full Haar matrix   [H] [verified]

**Problem.** `create_hash` (`src/hash.rs`) builds a `CoefMatrix` of
`IMAGESCALE×IMAGESCALE = 128×128` coefficients, each a 3-channel `f64`
(`src/haar.rs`). That is ~400 KB **per screenshot**, which is why a one-hour
video is ~100 MB in the `videostore_candidates.data` BLOB. At 10,000 videos
that is on the order of 1 TB — the dominant cost of the whole system.

**Key observation.** At *query* time the full matrix is never used. What the
scoring needs is:
- the list of **significant bucket `location`s** (already computed for the index),
- `dhash[0..2]`, `histogram`, `ratio` (already in `ScreenMeta`),
- `thresholds` (3 `f64`), if you want to be able to re-derive significance.

The matrix is only needed to *derive* the significant buckets at `add()` time.
So after `add()` has built the buckets, the matrix is dead weight in the BLOB.

**Fix.** Store, per screenshot, a compact record instead of the matrix:

```rust
// ~ a few KB per screenshot instead of ~400 KB
struct StoredShot {
    screenshot_id: u32,
    timecode:      u32,
    dhash0:        u64,
    dhash1:        u64,
    histogram:     u64,
    ratio:         f64,
    thresholds:    [f64; 3],            // optional, for re-bucketing
    significant:   Vec<u32>,            // the bucket locations (already derived)
}
```

Expected reduction: ~250 significant locations × 4 B ≈ **1 KB/screenshot**,
i.e. a **~200× cut** in per-video BLOB size (100 MB → ~0.5 MB for an hour).

**Caveats / follow-ups.**
- Re-bucketing on parameter change: if `sensitivity` or `TOPCOEFS` can change
  after the fact, you need the matrix (or at least the top coefficients) to
  recompute thresholds. Decide whether "re-bucketing" is a real requirement;
  if it is, store the top-K coefficients (e.g. 64/channel) rather than all
  16,384 — still a ~50× reduction.
- This is a **format change**. Write a migration that re-encodes existing BLOBs
  (decode old `VideoCandidate`, emit new compact form) so existing databases
  keep working. Gate behind a schema `version` column.
- Once the matrix is gone from the BLOB, `bulk_load_index` no longer has to
  decode 400 KB/screenshot either — startup gets faster for free.

---

## 2. Screenshotting: one ffmpeg pass, not one process per frame   [H] [verified]

**Problem.** `create_screenshots` (`src/files.rs`) loops over timecodes and,
for each, spawns a *new* `ffmpeg -ss <t> ... -frames:v 1` process that seeks
into the file. For a one-hour video at 10 s cadence that is ~360 process
spawns, each with its own seek + decode setup. Process spawning and repeated
seeking dominate parse time.

**Fix.** Emit all frames in a **single sequential decode** using the `fps`
(or `select`) filter, then read the frames out of one pipe/dir:

```text
ffmpeg -i input -vf "fps=1/<N>,scale=128:-2" -q:v 2 -f image2 pipe:1  # raw frames on stdout
# or
ffmpeg -i input -vf "fps=1/<N>" -q:v 2 outdir/%05d.jpg                  # one pass, N-second spacing
```

Benefits:
- One process, one decode, sequential read — typically **5–20× faster** than
  per-frame seeking.
- `-scale=128:-2` lets ffmpeg downscale to the 128-wide target *before* we
  ever touch the full-resolution frame, saving memory and JPEG-decode cost in
  Rust (`create_hash` currently resizes a full frame to 128×128 in
  `imageops::resize`).
- Reading frames from a single source also fixes the temp-file-in-the-video's
  directory issue (currently `{}_%03d.jpeg` is written next to the source).

**Also fix while here:**
- `video_metadata` uses `.expect("failed to execute process")` and the loop
  does too. If `ffmpeg` is absent this **panics the worker thread**. Return a
  `Result` and skip the file with a logged error instead. **[verified]**
- The metadata regex for fps (`, (?P<fps>\d+) fps`) will miss fractional frame
  rates; consider `ffprobe -show_streams` for robust metadata.

---

## 3. Hot-path CPU: `hamming_distance`   [H] [verified]

**Problem.** `src/hamming.rs` implements a hand-rolled popcount that
*allocates a `Vec<u8>` on every call*:

```rust
let vx = Vec::from(x.to_le_bytes());
let (vlow, vhigh) = vx.split_at(4);
...
```

This function is called **twice per surviving screenshot, per query
screenshot position** (`search_matches`), so it runs millions of times per
large query. The allocation + split + multiplies are all unnecessary.

**Fix.** The entire body reduces to one standard-library intrinsic:

```rust
pub fn hamming_distance(left: u64, right: u64) -> i64 {
    (left ^ right).count_ones() as i64
}
```

`count_ones()` compiles to a single `POPCNT` instruction. This is a
straight-line speedup on one of the hottest calls in the codebase, with zero
behavioral change (it is, by construction, a popcount of the XOR).

---

## 4. Threshold computation: quickselect instead of sort   [M] [verified]

**Problem.** `coef_threshold` (`src/hash.rs`) builds a `Vec` of all ~16,384
coefficient values and **fully sorts** it just to read the kth-largest. It is
called 3× per screenshot (once per channel). The test is even named
`test_quick_select`, but the implementation sorts.

**Fix.** Find the kth-largest in expected O(n) with a partial sort / quickselect
(e.g. `itertools::minmax` is not enough; use a `select`-style nth_element, or
a `BinaryHeap` of size k, or `select_nth_unstable`). For k = 40 over n = 16,384
this is a large constant-factor win, computed once per screenshot at hash time.

---

## 5. Correctness: negative sensitivity is persisted as 0   [M] [verified]

**Problem.** In `VideoStore::new` (`src/videostore.rs`) the parameter is read
back and, if it differs from the requested value, re-written with:

```rust
let sensitivity_u32 = v.sensitivity as u32;   // v.sensitivity is f64, e.g. -20.0
... params![&1, &sensitivity_u32, ...]
```

In Rust, `(-20.0f64) as u32` is **0**. So the value persisted to
`videostore_parameters.sensitivity` (a `BIGINT`) is 0, not -20. The column is
already `BIGINT`, so this is purely a cast mistake.

**Fix.** Store it as `i64`:

```rust
let sensitivity_i64 = v.sensitivity as i64;
// ...
params![&1, &sensitivity_i64, &v.start_directory, ...]
```

and read it back as `i64`. Same bug pattern exists in the re-insert branch and
in the initial `connect()` seed (which inserts `-60` correctly, so the seed is
fine — only the rewrite path is broken).

---

## 6. `search_matches` scratch allocation   [M] [verified]

**Problem.** `search_matches` allocates and NaN-fills a `scores` vector sized
to the **entire corpus** (`screenshot_index_global == self.index.num_screenshots`)
on *every* call, and it is called once per query-screenshot position. For a
10,000-video corpus that is a multi-MB allocation + fill, repeated hundreds of
times per query.

**Fix (two options, either helps):**
- **Hoist:** allocate the scratch once per `query()` (or per store, behind the
  lock) and reset only the *touched* indices to `NaN` between positions, using
  a `Vec<usize>` of touched indices.
- **Sparse:** score into a `BTreeMap<u32, f64>` keyed by touched global index
  instead of a dense array. Cleaner, slightly slower per-touch but no corpus
  sized allocation.

The dense+reset approach is usually fastest; the touched-index list is the
key part.

---

## 7. Index build: compute significant locations once, reuse   [M] [likely]

**Observation.** `hash_bucket_locations` is recomputed for a screenshot in
both `add()` and `bulk_load_index`, and the *same* significant-location set is
re-derivable for the query hash in `search_matches`. For a video whose frames
are similar (common), the significant-location list is nearly identical across
frames.

**Ideas.**
- Cache the query hash's `(location, bin)` list once per distinct hash (a small
  `HashMap<u64, Vec<...>>` keyed on a hash fingerprint) so repeated identical
  frames don't re-scan all 16,384 coefficients.
- Precompute the weight-bin per coefficient once per hash rather than inside
  the channel loop (currently `bin` is recomputed per channel).

Lower confidence on magnitude; benchmark before investing.

---

## 8. `delete` is an O(N) full rebuild   [M] [verified]

**Problem.** `VideoStoreInner::delete` removes the candidate + indices from
SQLite, then calls `bulk_load_index` (a full re-stream of every BLOB). For
frequent deletes this is expensive.

**Fix.** Remove the single candidate's entries incrementally: drop its
`ScreenMeta` rows, remove its `g` indices from each of its buckets, and rebuild
the `video_ids`/`candidate_by_pos` maps (or use a stable dense position and
mark the slot deleted). Reserve the full rebuild for the rare "prune a lot"
case.

---

## 9. Concurrency & thread model   [M] [done]

Implemented. The hand-rolled producer pool, the missing back-pressure stop
flag, and the never-wired `CTRL_C_PRESSED` flag are all in place now.

- **Bounded, deadlock-free producer pool.** The old producer reaped workers in
  FIFO order with `VecDeque::pop_front().join()`, which **deadlocked**: it
  joined the first-spawned worker while that worker was blocked on a full
  `sync_channel`, so the other workers (the ones that could drain the channel)
  were never joined. The pool was replaced with an `Arc<AtomicUsize>` in-flight
  counter (a semaphore): the producer waits for a free slot, spawns, and only
  after the whole file list is exhausted joins *all* workers in any order.
  Workers hand off with `try_send` so a full/closed channel can never wedge a
  worker. (rayon was intentionally **not** added — it is only a transitive
  dependency via `image`/`ravif`, and the std semaphore is sufficient here.)
- **Shared stop flag (back-pressure).** `VideoStore` now owns an
  `Arc<AtomicBool>` exposed via `stop_flag()`, `request_stop()`, and
  `stop_requested()`. A single flag is cloned into every worker, so any thread
  (or a signal handler) can abort the whole pipeline promptly.
- **Ctrl-C / graceful shutdown.** `demo_similar_videos` installs a `SIGINT`
  handler (`signal(2, …)`, no new crate dependency) that sets a global
  `AtomicBool`; a forwarder thread polls it and calls `store.request_stop()`.
  The producer, the query workers, and the writer all check
  `stop_requested()` at the top of their loops and wind down immediately.
  (The old `CTRL_C_PRESSED: bool` statics in `store.rs` / `videostore.rs`
  remain as harmless vestigial constants — see §12.)

---

## 10. `VideoMatches` sorting   [L] [verified]

**Problem.** `VideoMatches::sort` (`src/videomatches.rs`) is an O(n²) bubble
sort, and its `swap` clones the whole `VideoMatch` (including the `String id`)
twice per swap.

**Fix.** Use `self.m.sort_by(|a, b| a.score.partial_cmp(&b.score)
.unwrap_or(std::cmp::Ordering::Equal))`. Matches are usually few, but the
`String` clones in the hot swap are pure waste; `std::mem::swap` or the
library `sort_by` avoids them.

---

## 11. Marshalling endianness   [L] [verified]

**Problem.** `src/marshal.rs` uses `to_be_bytes`/`from_be_bytes`. On little-
endian x86 this forces a byte swap for every scalar.

**Fix.** Use native-endian (`to_ne_bytes`/`from_ne_bytes`) if the files are
never shared across architectures (they are local). Keep big-endian only if
cross-platform portability of the `.store` files matters. Low priority.

---

## 12. Vestigial state   [L] [verified]

After the in-memory-index refactor, a few fields are maintained but no longer
on the hot path:
- `candidate_cache: CandidateCache` — the old ~10 GB LRU. No longer consulted
  in `search_matches`/`rate_match` (both read `self.index` now). Remove it and
  the `max_candidates_in_cache` parameter, or document it as unused.
- `num_indices: BTreeMap<u32, usize>` and `num_index_values: u32` — updated
  per index-row in `add()` but the in-memory `InMemoryIndex` is now the source
  of truth. Confirm nothing reads them, then drop the per-insert map churn.
- `num_images` — appears unused.

Removing these shrinks `add()` and removes a source of confusion.

---

## 13. Robustness & UX

- **`present_video_matches`** (`src/files.rs`) calls `std::process::exit(2)`
  if it cannot create the output directory. Return a `Result` and let the
  caller decide; a missing output dir shouldn't abort a long scan.
- **Temp screenshot location.** Screenshots are written into the *source*
  video's directory. Use a per-run temp dir (`tempfile`) and clean up on drop.
- **Long paths.** The temp name is `{source_path}_%03d.jpeg`; long source
  paths can exceed OS limits. A temp dir avoids this.
- **`expect` on file I/O** in `dump_binary`/`slurp_binary` (`src/videostore.rs`)
  panic on failure; prefer `Result`.
- **`VideoMatch` `PartialOrd` is derived over a struct containing `f64`.**
  NaN scores will misorder. Add a custom `Ord` that treats NaN as "largest"
  (non-match), or guarantee no NaN reaches `sort`.

---

## 14. Testing gaps   [M]

The video path has one test (`test_video_store_concurrency`). The image path is
better covered. Add video-store tests for:
- **Sequence detection:** two real (different) similar frames produce a match
  only when `min_similar_screenshots_in_sequence` is met; below the threshold
  they do not. This directly guards the "6-in-a-row" rule.
- **`delete` / `exchange`** correctness (currently untested for videos).
- **Re-query after `add`** (the store reflects new data without a restart).
- **Sensitivity monotonicity:** lowering sensitivity never increases the match
  count.
- **Threshold edge cases:** `coef_threshold` with k ≥ n, and the zero-threshold
  (degenerate image) case — a regression test already exists for the latter
  (`test_degenerate_image_significance`); add one for the image store too.
- **A small end-to-end with a real short video** (skip if ffmpeg absent) to
  guard the ffmpeg interface.

---

## 15. Observability

- Log/emit **metrics**: videos/s, avg query ms, index size (screenshots,
  bucket entries), match counts, and the per-stage timings (parse vs. query vs.
  commit). `search_matches` already has the shape to time stages cheaply.
- A `--dry-run` / `--explain` mode that prints the top bucket hits and scores
  for one query screenshot would make sensitivity tuning far less of a black
  box (the current `eprintln!`-style debugging I used is the prototype).

---

## 16. Longer-term / architectural

- **Purpose-built index file.** At >50k videos, a flat `mmap`'d file
  (location → sorted (video, shot) array, fixed-width entries + a per-location
  offset table) will beat SQLite for this exact access pattern. The schema is
  simple enough that this is a contained project.
- **Two-pass scoring.** Pass 1: collect the *candidate* (video, shot) set from
  bucket hits. Pass 2: score only those. This is the original duplo paper's
  approach and cuts scoring cost sharply when buckets are long.
- **LMDB** instead of SQLite for the candidate BLOBs if the matrix-storage fix
  (§1) is *not* done — LMDB's mmap + lock-free readers suit this workload.
- **Incremental / resumable scans with content hashes.** Currently a re-run
  skips by *filename*. A stable content hash (e.g. of the first/last frame)
  would detect "same file, new name" and avoid re-parsing renamed duplicates.
- **`Send`/`Sync` is in place; next is a `tokio`-friendly facade** if the tool
  ever moves into an async service.

---

## 17. Suggested order of attack

1. **Quick wins (§1, §2, §3, §4, §5, §6)** — a day or two, no behavior change,
   immediate wins on parse time, comparison CPU, and a real bug fix.
2. **Storage (§1)** — the biggest long-term lever; do it with a migration +
   version column.
3. **Testing (§14)** — lock in the current behavior before/after §1–§2 so the
   perf changes are provably behavior-preserving.
4. **Concurrency polish (§9), robustness (§13), observability (§15).**
5. **Longer-term (§16)** once the corpus outgrows SQLite.

---

*Notes on confidence.* §1–§6 and §10–§12 were confirmed by reading the code and
are safe to act on. §7 and §2's "5–20×" figure are estimates — benchmark the
before/after on a representative video before committing to the larger rewrite.
§5 is a genuine correctness bug (negative sensitivity persisted as 0) and the
cheap fix is high value.

*Additional bugs found by §14 tests (fixed in `bd70469`):*
- `delete()` read `video_id` from column 1 of a 1-column SELECT (off-by-one)
- `exchange()` SQL was missing `SET` keyword: `UPDATE … filename = ?1 WHERE …`
- `delete()` never decremented `num_candidates`, so `size()` returned a stale value

*§11 deferred.* Marshalling uses big-endian (`to_be_bytes`). Switching to
native-endian would be a **format change** that breaks reading of every
existing `.store` and SQLite BLOB written in big-endian. Since the files are
local and always read back on the same architecture, the byte-swap cost is
negligible and the risk of data loss is not worth it. Revisit only if
cross-architecture portability of the `.store` files becomes a requirement.

*Additional bug found by §8 (fixed in this batch):*
- `delete()` derived the 1-based `candidate_id` from `self.ids` (which mixes
  0-based and 1-based values depending on the code path that populated it),
  so `InMemoryIndex::remove_video` was called with the wrong id and the
  wrong video's screenshots were removed. Fixed by reading the authoritative
  `candidate_id` from the `videostore_candidates` table.

*§16 status notes.*
- **Two-pass scoring (§16.2)** was already the architecture: `search_matches`
  accumulates cheap weighted scores into `scores[]` (Pass 1), then computes
  the expensive hamming-distance only for survivors (Pass 2). No change needed.
- **Content-hash dedup (§16.4)**: `VideoCandidate::content_fingerprint()`
  hashes (width, height, runtime, first+last screenshot dhash/histogram) to
  detect renamed duplicates. The demo writer calls
  `content_fingerprint_lookup()` before `add()` and skips the duplicate.
- **Mmap index (§16.1)**, **LMDB (§16.3)**, **tokio (§16.5)**: deferred.
  The mmap index only outperforms SQLite at >50k videos; LMDB is unnecessary
  after §1 eliminated the 400 KB BLOBs; the tool is synchronous end-to-end.