# Improvement Suggestions — media-browser

A prioritized, review-driven list of concrete improvements for the `media-browser` project. Items are grouped by category; each notes the affected location, why it matters, and a suggested approach. High-impact items are flagged as **[High]**. Items already implemented are marked **✅ DONE**.

---

## 1. Security

### 1.1 [High] ✅ DONE — SQL injection / broken queries in database search

> **Status (implemented):** All user-input `LIKE` clauses in `search_video`, `search_audio`, `search_image` and `search_file_metadata` now bind the term as a parameter and use `ESCAPE '\'`; a shared `escape_like_term()` helper neutralises `%`, `_` and `\` in user input. Regression tests added in `sql::tests` (single-quote match, injection rejection, case-insensitive substring).

`src/sql.rs` builds search queries by interpolating raw user input directly into the SQL string with `format!` and a `LIKE '%{}%'` wrapper (see `search_video` ~line 268, `search_audio` ~line 453, `search_image` ~line 681, and the many `INNER JOIN ... LIKE '%{}%'` strings at lines 329, 340, 373, 501, 512, 523, 601, 854).

Problems:
- **SQL injection.** A search term containing a single quote, `--`, or SQL keyword can break or alter the query.
- **Fragile matching.** User input containing `'` or `%`/`_` wildcards changes semantics or errors out.

**Fix:** Use rusqlite's parameterized queries everywhere. Build the SQL with `?` placeholders and pass the term via `params![...]`. Escape LIKE wildcards in the term (`\%`, `\_`) if needed and add `ESCAPE '\'`. This is a mechanical but high-value refactor across `search_video` / `search_audio` / `search_image` / `search_file_metadata` and the `search_*_metadata` helpers.

### 1.2 [High] ✅ DONE — `as` integer casts when formatting SQL
`src/sql.rs` previously contained numeric casts (`as u32`, …) used while composing `WHERE duration > {} AND duration < {}` style clauses (e.g. lines ~314, ~358, ~1176, ~1191). Silent truncation/overflow was possible and these values flowed into unsanitized SQL.

> **Status (implemented):** The numeric search bounds (`from_value`/`to_value`, `from_date`/`to_date`, GPS ranges) are now passed as bound `Value::Integer` parameters instead of being cast and formatted into the SQL string.

**Fix:** Validate/range-check the values at the boundary (reject out-of-range in `SearchData`), keep the native integer type end-to-end, and pass them as bound parameters instead of formatting them in.

---

## 2. Architecture & Modularity

### 2.1 [High] Very large source files
Current line counts:
- `src/app.rs` — ~7203 lines
- `src/sql.rs` — ~6589 lines
- `src/tab.rs` — ~5738 lines
- `src/parsers.rs` — ~2710 lines
- `src/dialog.rs` — ~1721 lines

These files mix concerns (state, message handling, UI construction, persistence, parsing). This slows review, increases merge conflicts, and makes it hard to test a single behavior.

**Suggested split:**
- `sql.rs` → `sql/` module: `search.rs`, `insert.rs`, `queries.rs`, `schema.rs`, `types.rs` (the metadata/search types), with a thin `mod.rs` re-exporting.
- `app.rs` → split message handlers per feature: `app/mod.rs`, `app/scan.rs`, `app/operations.rs`, `app/nav.rs`, `app/search.rs`, `app/trash.rs`.
- `tab.rs` → `tab/` with `location.rs`, `items.rs`, `selection.rs`, `sort.rs`.
- `parsers.rs` → per-format parsers (`parsers/nfo.rs`, `parsers/exif.rs`, `parsers/id3.rs`) behind a common trait.

A common trait for "metadata source" (video/audio/image) would also let `search_*` share logic (see 4.1).

### 2.2 Duplicate search logic across media types
`search_video`, `search_audio`, and `search_image` (and their `_metadata` helpers) are near-identical: acquire the mutex, `prepare`, `query`, `loop rows.next()`, collect ids, then a second loop calling `*_by_id` per id. 

**Fix:** Extract a single generic `search_media(connection, table, join_clause, search)` that all three call, parameterizing table/joins. This removes ~600 lines of duplication and centralizes the (currently repeated) error handling and the N+1 loop (see 3.1).

### 2.3 `SearchData` is a wide bag of booleans
`src/sql.rs` `SearchData` (~line 41) carries ~20 separate `bool` fields (`title`, `director`, `gps_latitude`, …) plus `from_*`/`to_*` fields, a hand-written `Default`, a hand-written `PartialEq`, and a `display()` that repeatedly calls `format!`.

**Fix:**
- Replace the boolean flags with a `HashSet<SearchType>` (a `SearchType` enum already exists in the same file) — this shrinks the struct, makes `Default`/`PartialEq` free via derive, and removes the risk of forgetting a flag (see bug 5.2).
- Rewrite `display()` with a `String` buffer / `write!` instead of chained `format!`.
- Add `Serialize`/`Deserialize` (already available via serde in the crate) so stored searches round-trip cleanly.

### 2.4 `Action` enum + manual `message()` mapping
`src/app.rs` defines a large `Action` enum and a hand-written `impl Action { fn message(&self, entity_opt) -> Message }` that maps each variant to a `Message`. This is a maintenance hotspot and duplicates the menu/keybind definitions.

**Fix:** Derive the relationship in one place. Either build the menu/keybind entries from the same `Action` list (data-driven), or invert so `Message` is constructed at the call site and `Action` only carries identity for lookup. Centralizing reduces the chance of an action that is keyboard-bound but not menu-bound (or vice versa).

---

## 3. Performance

### 3.1 [High] ✅ DONE — N+1 query pattern in search
`src/sql.rs` `search_video_metadata` (~line 202) collects all matching ids in one query, then loops:
```
for video_id in ids {
    let filedata  = file_by_id(connection, video_id);
    let videodata = video_by_id(connection, &pathstr, video_id);
    ...
}
```
This issues `2 * ids.len()` additional round-trips. Large collections (the README notes users can scan entire media libraries) make this a real latency and memory cost.

**Status:** All three `search_*_metadata` helpers (`search_video_metadata`, `search_audio_metadata`, `search_image_metadata`) were rewritten to return fully populated rows from a single `SELECT ... JOIN file_metadata ...` query — no per-id `file_by_id()` / `*_by_id()` follow-up calls, no repeated mutex acquisitions. Every `search_video` / `search_audio` / `search_image` SELECT was updated with the `INNER JOIN file_metadata` and full column lists (including the multi-table paths for actors/directors/tags/artists/albums). `search_file_metadata` needed no change (it only ever selects `metadata_id`). 6 new regression tests cover image title, tag join, injection rejection, and GPS range paths.

**Fix:** Replace with a single `SELECT ... FROM video_metadata JOIN file_metadata ... WHERE ...` (and the join for people/actors/directors/tags where relevant) so rows are returned fully populated in one query. This is the single biggest runtime win available for the "search the database" feature.

### 3.2 [High] ✅ PARTIALLY DONE — Single shared DB connection behind a mutex
The codebase threads `std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>` around (12 references) and the README explicitly notes that opening/scanning is slow while a background scan writes. A single serialized connection is a bottleneck between the UI, the file watcher, and background scans.

**Fix:** Introduce a small connection pool (`deadpool-sqlite` or `r2d2_sqlite`) so readers and writers can proceed concurrently; or at minimum use SQLite's WAL mode (`PRAGMA journal_mode=WAL`) plus `busy_timeout` to reduce writer/reader contention. Pair with 3.1 to cut query count.

**Status (minimal fix applied):** `connect()` now sets `PRAGMA journal_mode=WAL` and `PRAGMA busy_timeout=5000` right after opening the database, before schema creation — readers no longer block on writers and locked-DB failures now wait up to 5 s instead of erroring immediately. The heavier option (a real connection pool) is still open.

### 3.3 [High] ⬜ OPEN — Thread/runtime mix
`src/app.rs` and `src/video/video.rs` mix `std::thread::spawn` and `tokio::task::spawn_blocking`. Using `spawn_blocking` for CPU/IO bound work (scans, thumbnail generation, ffmpeg calls) is the right instinct; the raw `std::thread::spawn` sites (e.g. `video/video.rs:320`, `cmd.rs:77`, `mounter/gvfs.rs:284`) should be reviewed to ensure they don't block a worker unexpectedly and that joins/cancellation are handled (several `_joinhandle` results are dropped).

**Fix:** Standardize on a task pool for blocking work and add cancellation so a closed tab / stopped player can cancel an in-flight scan or thumbnail job.

**Status:** Audited all `std::thread::spawn` sites (`app.rs` ×3 — recursive scans and rename-pattern jobs; `cmd.rs` — pipe tee; `mounter/gvfs.rs` — glib main loop; `video/video.rs` — GStreamer playback). All are detached long-lived workers where dropping the join handle is intentional (fire-and-forget background work), so none were converted; the scan sites in particular hold a `Mutex<Connection>` and are serialized by design, which WAL (3.2) now unblocks for readers. A cancellation mechanism for in-flight scans/thumbnails is still missing.

### 3.4 Memory
`main.rs` sets `M_MMAP_THRESHOLD` and offers an optional `tikv-jemallocator` feature, and the README describes a past crash on large images. Consider:
- Enabling the jemalloc feature by default for release builds (it's already a dependency).
- Capping decoded image size before upload to the GPU pipeline (the 9000px thumbnail path in the README) to avoid OOM on large panoramas.
- Auditing the `rayon` usage for unbounded parallelism over very large directory listings.

---

## 4. Correctness & Robustness

### 4.1 [High] `unwrap()` / `expect()` volume
The tree contains ~172 `unwrap()` and ~58 `expect()` calls. In a desktop app that touches the filesystem, D-Bus, and GStreamer, these are likely crash paths under normal variation (missing home dir, offline network drives, permission errors).

**Fix:** Prefer `?` with a proper error type (see 4.4), `log::warn!` + graceful degradation for non-fatal cases, and reserve `expect` only for true invariants. A CI lint (clippy `unwrap_used` as a warning→deny) will help keep the count down.

### 4.2 ✅ DONE — Duplicated `gps_longitude` in `SearchData` equality
`src/sql.rs` `impl PartialEq for SearchData` (~line 164) compared `self.gps_longitude == other.gps_longitude` twice. It was currently harmless but a copy/paste smell.

> **Status (implemented):** The duplicated comparison was removed, and `sql::tests::search_data_partial_eq_distinguishes_gps` guards that `gps_longitude` and `gps_latitude` independently affect equality.

Using `derive(PartialEq)` (after 2.3) removes this class of bug entirely.

### 4.2a ✅ DONE — Date-range searches silently dropped
The `creation_date` / `modification_date` branches in `search_items` and the `release_date` branch in `search_image` were gated on `search.from_string.len() != 0`, but the `search_items` dispatcher clears `from_string` after converting it into `from_date`. As a result, any search expressed as a date (e.g. a single or range of dates) was silently dropped and returned no results.

> **Status (implemented):** Those branches now gate on `search.from_date != 0` (the field that is actually populated), so date searches work. The `filepath` branch now correctly gates on `!search.from_string.is_empty()`.

### 4.3 `daemonize` hardcoded and debug hostilities
`src/main.rs` / `src/lib.rs` `main()` forks the process into a daemon (`fork::daemon(true, true)`) with `daemonize` hardcoded to `true`. This detaches stdout, complicates debugging and `just run`, and is atypical for a desktop app.

**Fix:** Gate daemonization behind a CLI flag (e.g. `--daemon`) or the packaged `.desktop` invocation, and default to a foreground process for `cargo run` / debugging. Also note that `src/lib.rs` defines a `desktop()` entrypoint that `main()` never calls — either wire it in (e.g. selected by a feature/flag) or remove it to avoid dead code.

### 4.4 Error-handling style
Both entrypoints return `Box<dyn std::error::Error>` and the crate defines an `err_str` helper plus `thiserror` as a dependency. The styles are mixed.

**Fix:** Adopt `anyhow` for application-level error propagation in `main`/`desktop` and `thiserror` for library error types. This gives context-rich messages (`anyhow!` with `#[source]`) and a consistent `?` path, and makes the `Box<dyn Error>` boundaries disappear.

### 4.5 Logging consistency
The tree mixes the `log` crate with `println!`/`eprintln!` (e.g. `src/video/video_view.rs:294,314,389`, `src/app.rs:3470,3509`, `src/sql.rs:5913`, `src/parsers.rs:69`). `env_logger` is already initialized in both `desktop()` and `main()`.

**Fix:** Replace `println!`/`eprintln!` with `log`/`tracing` at appropriate levels, and initialize the logger once (a small `fn init_logging()` shared by both entrypoints) rather than twice.

### 4.6 `#[rustfmt::skip]` on entrypoints
`src/lib.rs` applies `#[rustfmt::skip]` to `main()` and `desktop()`. This defeats `just dev`'s `cargo fmt` and hides formatting drift.

**Fix:** Remove the skips and fix the underlying formatting so the whole crate is consistently formatted.

---

## 5. Testing

### 5.1 Partially done — Thin test coverage
Only `src/tab.rs` (~16 tests) and `src/operation/mod.rs` had `mod tests`. `sql.rs` (the heart of the product), `parsers.rs`, and `app.rs` have essentially no tests, and `Cargo.toml` dev-dependencies (`cap-std`, `cap-tempfile`) are commented out.

> **Status (implemented):** `sql.rs` now has a `mod tests` covering `escape_like_term`, the `SearchData` equality regression, and `search_video` against an in-memory DB (single-quote titles, SQL-injection rejection, case-insensitive substring). `parsers.rs` and `app.rs` tests remain open.

**Suggested additions:**
- **`sql.rs`:** unit tests against an in-memory SQLite connection for `insert_search`/`searches`/`delete_search`/`update_search` and for `search_video`/`search_audio`/`search_image` — including a regression test that a search term with a single quote or `%` does not error or leak other rows (directly covers 1.1).
- **`parsers.rs`:** golden-file tests for NFO, EXIF, and ID3 parsing using the fixtures already in `testsamples/` and `samples/`.
- **`SearchData`:** round-trip (serialize/deserialize) and equality tests (covers 2.3 / 4.2).

### 5.2 Re-enable snapshot/golden testing
The commented `cap-std`/`cap-tempfile` dev-deps suggest sandboxed fs tests were intended. Re-introduce them for the file-operation code paths (`MoveToTrash`, rename, recursive scan) so tests don't touch the real filesystem.

### 5.3 Property/edge cases for search ranges
`SearchData` supports from/to value and date ranges. Add tests for: `from == to`, `from > to`, zero values, and mixed string+value fields, to pin down the intended semantics (currently only `if search.from_value != 0` guards exist).

---

## 6. Dependencies & Build

### 6.1 [High] Reproducible builds / git dependencies
Several dependencies come from git, plus a patched crates.io crate:
- `fs_extra` (git, pop-os), `trash` (git, `cosmic` branch), `libcosmic` (git), and `[patch.crates-io] filetime` (git).

Git deps without a locked rev make builds less reproducible and vulnerable to upstream branch changes.

**Fix:** Prefer tagged upstream releases where they exist (track `libcosmic`, `fs_extra`, `trash` releases). If git is unavoidable, pin an exact `rev` instead of a moving `branch`, and keep `Cargo.lock` committed (it is) — document how to update. Remove the commented-out `[patch]` blocks to reduce noise.

### 6.2 Mixed compression backends (C vs pure Rust)
`Cargo.toml` marks `bzip2` and `liblzma` as optional with `#TODO: replace with pure Rust crate`, alongside `flate2`, `tar`, and `zip`. This is a build-portability and supply-chain consideration.

**Fix:** Track pure-Rust replacements (e.g. `libflate`, `xz2`, `bzip2-rs`) and gate the C-backed ones behind a non-default feature so the default build has fewer native deps.

### 6.3 Edition / MSRV
`edition = "2021"` with `rust-version = "1.80"`. The code already uses modern idioms (`let ... else`, `non_exhaustive`-friendly enums).

**Fix:** Consider moving to `edition = "2024"` (raises the MSRV accordingly) to pick up gen-blocks, `unsafe_op_in_unsafe_fn` defaulting, and `if let`/`else` refinements — or explicitly document why 2021 is retained.

### 6.4 ✅ DONE (minimal) — No CI / lint gate
There is no `.github` workflow. The `justfile` has `check` (clippy `-W clippy::pedantic`) and `dev` (fmt) recipes but nothing enforces them.

**Status:** Added `.github/workflows/ci.yml` with two jobs: `fmt` (`cargo fmt --all --check`) and `clippy-test` (system deps + `cargo clippy --lib` + `cargo test --lib`, with a cargo cache). The fmt and clippy steps are **informational** (`continue-on-error: true`) on purpose: the tree currently carries ~161 pre-existing formatting diffs across ~35 files and 10 pre-existing clippy errors (8× `never_loop` in `app.rs`/`tab.rs` plus two others), so a hard gate would fail on unrelated debt. The real gate is `cargo test --lib`, which passes. Next steps: land a dedicated `cargo fmt` pass and fix the clippy errors, then flip both `continue-on-error` flags off; consider a deb/rpm build matrix and `clippy::unwrap_used`/`expect_used` lints to curb 4.1.

**Fix (original):** Add a GitHub Actions workflow that runs `cargo fmt --check`, `just check` (clippy pedantic, `--all-features`), `cargo test`, and a matrix build (deb + rpm) on PRs. Add `clippy::unwrap_used` and `clippy::expect_used` as lints to curb 4.1.

### 6.5 `just run` builds `--release`
`justfile` `run` recipe compiles with `cargo build --release` then executes the release binary. That is slow for iterative UI work.

**Fix:** Add a `just dev-fast` that builds a debug profile (with `[profile.dev] opt-level = 1` already set) for fast iteration, keeping `run` for the near-release experience.

---

## 7. Code Quality & Idioms

- **`lazy_static` + `once_cell`** are both present. `once_cell` is sufficient for most needs; consider dropping `lazy_static` to reduce surface.
- **`html-escape` + `xml` + `regex`** all parse/generate markup in `parsers.rs`. Ensure the chosen parser is the only one touching untrusted input, and that the `regex` patterns are precompiled (they likely are via `lazy_static`) and never built per-call from user data.
- **`paste` and `slotmap`** — verify both are still needed; `slotmap` is core to the tab model, but `paste` may be replaceable.
- **Dead/commented code:** remove commented-out dependencies (`iced_video_player`), commented `use iced_video_player` block in `app.rs`, and commented `[patch]` sections to keep the tree honest.
- **`Box<dyn Error>` boundaries** in ~37 places — consolidate per 4.4.

---

## 8. User Experience & Features

These align with the open items already tracked in `Todo.md` and the README; flagged as product suggestions:

- **Playback speed control** (listed as open for both video and audio) — a small, high-value addition for the players.
- **Lyrics (`*.lrc`) as subtitles in the audio player** (open item) — completes the audio experience.
- **Zoom-in for the image viewer** (open item) — currently limited by the `image_viewer` widget; consider a dedicated viewer.
- **Filter/sort by release date & media-type-only views** (open items) — natural extensions now that the search engine (`SearchData`) already models `ReleaseDate` and type flags.
- **Graceful "Move to Trash" of a media item's metadata** — the README notes trashing a file leaves NFO/thumbnails/DB entries behind. A "remove this media + its sidecars from the DB" action would close a real gap and match user expectations.
- **Cancellation UI for recursive scans** — the README warns scans block and are slow; surface a cancel button and a progress percentage (progress is currently only in the log).
- **De-duplication / similar-media discovery** (stretch goal) — gate behind a "run offline in the background" mode and write results to a report rather than blocking the UI, consistent with the `duplo-rs` note in `Todo.md`.

---

## 9. Documentation

- **Add a `CONTRIBUTING.md`** describing the `just` recipes, the GStreamer/ffmpeg prerequisites, and how to run the test suite; currently this knowledge is spread across `README.md` and `justfile`.
- **Architectural doc** (`docs/ARCHITECTURE.md`) explaining the data flow: file watcher → scan → `sql.rs` → `tab.rs` model → iced UI, and the metadata schema. This would accelerate onboarding and the modularization in section 2.
- **Database schema reference** — document the tables and the `SearchType`/`SearchData` mapping so SQL changes are reviewed against a spec.
- **Changelog** — add a `CHANGELOG.md` (the version is already `0.9.0`) to make the "beta → 1.0" transition transparent to users.

---

## 10. Suggested Prioritized Roadmap

**Phase 1 — Safety & correctness (do first)**
1. ✅ Parameterize all SQL (`1.1`), remove `as`-cast formatting (`1.2`).
2. ⬜ Reduce `unwrap`/`expect` crash paths on common inputs (`4.1`); ✅ fix the `PartialEq` dup (`4.2`).
3. ✅ (partial) Add regression tests for search (including injection) and `SearchData` equality (`5.1`); ⬜ `5.3`.
4. ✅ (minimal) Add a minimal CI (fmt + clippy + test) (`6.4`) — workflow added; fmt/clippy are non-blocking pending the existing fmt/lint debt, tests are the hard gate.

**Phase 2 — Performance**
5. ✅ Eliminate the N+1 search queries with joins (`3.1`) — all three `search_*_metadata` helpers now use single `JOIN file_metadata` queries with inline row extraction.
6. ✅ (partial) Introduce SQLite WAL + a connection pool or bounded busy-timeout (`3.2`) — WAL + `busy_timeout=5000` enabled in `connect()`; connection pool still open.
7. ⬜ Centralize blocking work on `spawn_blocking` with cancellation (`3.3`) — spawn sites audited (all intentional fire-and-forget workers); cancellation still missing.

**Phase 3 — Architecture & quality**
8. Split `sql.rs` and `app.rs` into modules; unify the three search implementations (`2.1`, `2.2`).
9. Refactor `SearchData` to a `HashSet<SearchType>` with derived traits (`2.3`); clean up the `Action`/`Message` mapping (`2.4`).
10. Standardize error handling with `anyhow`/`thiserror` and consolidate logging (`4.4`, `4.5`).

**Phase 4 — DX & product**
11. Pin/replace git dependencies, consider pure-Rust compression (`6.1`, `6.2`), edition 2024 (`6.3`).
12. Wire in or remove the unused `desktop()` entrypoint and make daemonization opt-in (`4.3`).
13. Deliver the high-value open features: playback speed, lyrics, trash-with-sidecars, scan cancellation/progress (section 8).

---

*Prepared as a self-review of the codebase. Line references are approximate to the current working tree and should be re-checked as the files are refactored.*