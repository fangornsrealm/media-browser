# Implementation Plan: Lyrics Display in the Audio View

## Overview

When the audio file has an external LRC file with the same name
(`<stem>.lrc` next to `<stem>.mp3`), the audio view shows an **optional right
column** with the timestamped lyrics:

- the active line is highlighted and the column follows playback;
- the user can wheel-scroll the list (auto-follow pauses);
- clicking a line seeks playback to its timestamp;
- no `.lrc` file → the column is absent and the layout is byte-for-byte what it
  is today.

No new crate dependencies (std-only parser). Reuses the existing 5 Hz
`Message::NewFrame` ticker, the existing `AudioView::Message::Seek` arm, and
the repo's `widget::scrollable` pattern (`tab.rs:4856`).

---

## 1. Current state (what the plan builds on)

| Fact | Where |
|---|---|
| `AudioView` holds playback state; `load()` is the **single choke point** for every file open (browser open → `open_message` → `update(Open)` → `load()`; `audio_message`'s `Open` arm → `load()` directly; Next/Previous → `open_path` → same) | `src/audio/audio_view.rs:63-155`, `src/app/mod.rs:2340`, `src/app/views.rs:594` |
| `Message::NewFrame` is published every 200 ms while playing and already updates `AudioView.position` | `src/audio/audio_player.rs:19`, `src/audio/audio_view.rs:255` |
| `AudioView::Message::Seek(secs)` already implements the seek body (set position, pause-safe `audio.seek`, `update_controls`) | `src/audio/audio_view.rs:225-240` |
| App-level dispatch forwards unknown audio-view messages: `_ => self.audio_view.update(msg)` — new `audio_view::Message` variants need **no** changes in `views.rs`/`mod.rs` | `src/app/views.rs:706`, `src/app/views.rs:119-135` |
| `view_audio_view()` builds one container (black background, `mouse_area`, popover control bar) | `src/app/mod.rs:1815-2210` |
| `widget::scrollable(...).id(..).on_scroll(msg)` is the established pattern | `src/tab.rs:4856-4859` |
| The DB already records `.lrc` paths during scans (`lyrics` table, `parsers.rs` `audio_metadata` finds `<stem>.lrc`), but only for scanned dirs — the view **must not** depend on it | `src/sql/audio.rs:32,669`, `src/parsers.rs:1300-1322` |

Decision: discover the `.lrc` file on the filesystem inside `AudioView::load()`
(same primitive the scanner uses: `file_stem` + `.lrc`). The DB `lyrics` table
is left untouched — it already exists and stays as-is.

---

## 2. New module: `src/audio/lyrics.rs`

### 2.1 API

```rust
//! LRC lyrics: discovery, parsing, active-line lookup. Std-only.

/// One timestamped lyric line (a line with several stamps yields one
/// `LyricLine` per stamp, sharing the text).
pub struct LyricLine {
    pub time: f64,   // seconds
    pub text: String,
}

/// Parse LRC content into lines, sorted by first appearance (already so in the
/// file; do NOT re-sort — LRC files are ordered, and re-sorting breaks
/// multi-stamp lines and interludes).
pub fn parse(contents: &str) -> Vec<LyricLine>;

/// `<audio>.mp3` -> `<audio>.lrc`: read (UTF-8 lossy, BOM stripped, `\r`
/// trimmed), parse. `None` if the file is missing, unreadable, or contains no
/// non-empty lyric lines (tag-only files count as "no lyrics").
/// `audio_path` may be a plain path or a `file://` URL — the audio view
/// keeps its path as a `url::Url`, so both forms are handled.
pub fn from_audio_file(audio_path: &str) -> Option<Vec<LyricLine>>;

/// Last index with `time <= pos`; `None` before the first line.
pub fn active_index(lines: &[LyricLine], pos: f64) -> Option<usize>;

// Layout constants shared by the view and the scroll math so they cannot drift:
pub const LINE_PX: f32 = 16.0;    // cosmic body text height
pub const SPACING_PX: f32 = 8.0;  // column spacing (space_xs)
pub const SPACER_PX: f32 = 12.0;  // empty line
pub fn row_height(line: &LyricLine) -> f32; // SPACER_PX if empty else LINE_PX + SPACING_PX
pub fn scroll_target(lines: &[LyricLine], active: usize) -> f32; // sum of row_height[..active]
```

### 2.2 Parser rules (phase 1)

- Timestamps: `[mm:ss.xx]` — minutes may be 2+ digits, seconds 1–2 digits,
  fractional part 0–3 digits (`[1:5.2]`, `[01:00.36]` both valid). Hand-written
  scanner over the line (no `regex` needed; `regex` is in the tree if preferred).
- A line may carry several stamps `[00:10.00][00:20.00]Text` → one
  `LyricLine` per stamp.
- Header tags `[ar:..]`, `[ti:..]`, `[al:..]`, `[offset:+2.30]` are **skipped**
  (never emitted). `[offset:]` support is a phase-2 extension, see §8.
- Text-only lines (no stamp) are **continuations** of the previous timed line:
  append to that line's `text` joined by `"\n"` (multi-line lyrics).
  Text-only lines before the first timestamp are discarded (headers).
- Empty text after a stamp → spacer line (kept; rendered as vertical space).
- Karaoke `<mm:ss.xx>` segments inside the text are stripped from the text
  (cheap token filter; phase 1 does not do per-word highlighting).
- Decode: `fs::read` bytes → strip UTF-8 BOM → `String::from_utf8_lossy` →
  trim `\r` per line (the repo's own sample `testsamples_archiv/01 Politik.lrc`
  is a real-world fixture for this).

### 2.3 Unit tests (in `lyrics.rs`)

- **Sample file**: parse `testsamples_archiv/01 Politik.lrc` → ≥ 10 lines;
  first line text `Look at Earth from outer space` at `26.17`; the blank
  stamped line at `01:10.33` survives as a spacer.
- **URL form**: `from_audio_file` accepts the `file://` URL the audio view
  passes it (plain-path and URL forms give identical results).
- Inline fixtures: multi-stamp line; header tags skipped; continuation lines
  joined; CRLF + BOM; malformed stamp ignored; karaoke `<...>` stripped;
  tag-only file → `from_audio_file`/`parse` yields no usable lines.
- `active_index`: before first → `None`; between lines → correct index; after
  last → last index.
- `scroll_target`: prefix sums match the constants; empty-line spacers counted.

---

## 3. `AudioView` state (`src/audio/audio_view.rs`)

New fields (plus `new()` init and `close()` reset):

```rust
pub lyrics: Vec<crate::audio::lyrics::LyricLine>,
/// Stable id of the lyrics `widget::scrollable`, for `scroll_to`.
pub lyrics_scroll_id: cosmic::widget::Id,
/// Last scroll offset commanded / observed for the lyrics column.
pub lyrics_scroll_sent: f32,
/// Lyrics column follows playback until the user scrolls manually.
pub lyrics_follow: bool,
/// Active lyrics line index at the last followed frame.
pub lyrics_last_active: usize,
```

- **`load()`** — after `self.audio_opt = Some(audio);` succeeds:
  ```rust
  self.lyrics = crate::audio::lyrics::from_audio_file(&audiopath).unwrap_or_default();
  self.lyrics_scroll_sent = 0.0;
  self.lyrics_follow = true;
  self.lyrics_last_active = 0;
  ```
  (On open failure nothing is set — `close()` already cleared `lyrics`.)

  Note: `audiopath` here is the `file://` URL string
  (`audiopath_opt: Option<url::Url>`); `from_audio_file` handles both plain
  paths and URLs.

- **Scroll decision** — `AudioView::update` returns `()`, so it cannot emit
  the iced `scroll_to` command. Instead `AudioView` exposes a pure decision
  method and the app issues the command:
  ```rust
  /// `Some(offset)` iff the lyrics column should be (re-)centered on the
  /// active line now; `None` while following with no change, while the user
  /// is manually scrolling, or before the first line.
  pub fn lyrics_scroll_target(&mut self) -> Option<f32>
  ```
  It returns `None` while `lyrics` is empty, before the first line, while
  `!lyrics_follow` and the position is still on the same line, or when the
  target moved by < 1 px from `lyrics_scroll_sent` (no command spam). On a
  new line it re-arms `lyrics_follow` and updates `lyrics_last_active`.

- **New message variants** (both handled in `AudioView::update`):
  - `LyricSeek(usize)` → re-arm follow at the clicked line
    (`lyrics_follow = true`, `lyrics_last_active = idx`,
    `lyrics_scroll_sent = scroll_target(idx)`), then
    `self.update(Message::Seek(line.time))` — reusing the existing seek arm
    (pause state preserved, controls refreshed).
  - `LyricsScroll(f32)` → `lyrics_follow = false; lyrics_scroll_sent = offset;`

---

## 4. Layout: `view_audio_view()` (`src/app/mod.rs:1815`)

The existing code builds `popover` and wraps it in one black container. Keep
that element as `main_area` and conditionally add the right column:

```rust
let main_area: Element<_> = widget::container(popover)          // existing, unchanged
    .width(Length::Fill)
    .height(Length::Fill)
    .class(/* existing black custom class */)
    .into();

// No external lyrics file: the layout is exactly what it was before.
if self.audio_view.lyrics.is_empty() {
    return main_area;
}

// ... rows as sketched below (spacer / mouse_area rows) ...

let lyrics_panel: Element<_> = widget::container(
    widget::column::with_capacity(2)
        .align_x(Alignment::Start)
        .push(widget::text::body(fl!("lyrics")))                // caption
        .push(
            widget::scrollable(widget::column::with_children(lines).spacing(lyrics::SPACING_PX))
                .id(self.audio_view.lyrics_scroll_id.clone())   // stable id, see below
                .on_scroll(|viewport| Message::AudioMessage(
                    crate::audio::audio_view::Message::LyricsScroll(
                        viewport.absolute_offset().y,
                    )))
                .width(Length::Fill)
                .height(Length::Fill),
        ),
)
.width(Length::Fixed(320.0))
.height(Length::Fill)
.padding(space_m)
.class(theme::Container::Background)                             // visible against black
.into();

widget::row::with_children(vec![main_area, lyrics_panel]).into()
```

Scrolling — **important iced 0.14 detail**: `widget::scrollable` has no
`.position()`/set-offset API. The offset is commanded with the
`widget::scrollable::scroll_to(id, AbsoluteOffset { .. })` **task**, which is
why the scrollable carries the stable `lyrics_scroll_id` (created once in
`AudioView::new()`; `Id::unique()` per rebuild would break it):

- **Follow playback** — `App::new_frame()` (`src/app/views.rs`) already
  returns a `Task<Message>` and runs on every 200 ms audio frame:
  ```rust
  if let Some(offset) = self.audio_view.lyrics_scroll_target() {
      return widget::scrollable::scroll_to(self.audio_view.lyrics_scroll_id.clone(),
          widget::scrollable::AbsoluteOffset { x: None, y: Some(offset) });
  }
  ```
- **Click-to-seek** — `App::audio_message()` intercepts `LyricSeek` (it has to
  be intercepted because the arm must *return* the `scroll_to` task, which the
  `_ =>` forward arm cannot do): forward to `AudioView::update`, then return
  `scroll_to(lyrics_scroll_id, lyrics_scroll_sent)`. `LyricsScroll` needs no
  task and falls through the explicit arm to `AudioView::update`.

Notes:

- **No new app-level `Message` variant** — `on_press`/`on_scroll` route
  through the existing `Message::AudioMessage(audio_view::Message::…)`
  envelope.
- `LyricSeek` preserves pause state (existing `Seek` arm behavior), unlike the
  app-level `Message::Seek` which force-unpauses — deliberate.
- Column width fixed 320 px; player area stays `Fill`. Fullscreen path works
  unchanged (row fills the window).
- If `text::Style::Custom` (accent color) is preferred over `heading` for the
  active line, it is a one-line swap in the loop.

---

## 5. i18n

Add one key to all three locales (repo convention: every user-visible string
is an `fl!` key):

| File | Key |
|---|---|
| `i18n/en/media_browser.ftl` | `lyrics = Lyrics` |
| `i18n/de/media_browser.ftl` | `lyrics = Liedtext` |
| `i18n/sv/media_browser.ftl` | `lyrics = Texter` |

---

## 6. Files touched

| File | Change |
|---|---|
| `src/audio/lyrics.rs` | **new**: parser, discovery, `active_index`, scroll math, unit tests |
| `src/audio/mod.rs` | `pub mod lyrics;` |
| `src/audio/audio_view.rs` | state fields, `new()`/`close()`/`load()`, `lyrics_scroll_target()`, `LyricSeek`/`LyricsScroll` variants + arms |
| `src/app/mod.rs` | `view_audio_view()`: wrap in row, add lyrics column |
| `src/app/views.rs` | `new_frame()`: follow-playback `scroll_to` task; `audio_message()`: `LyricSeek`/`LyricsScroll` arms (task-returning); `lyrics_integration` end-to-end test |
| `i18n/{en,de,sv}/media_browser.ftl` | `lyrics` key |

No `Cargo.toml` change (std-only). No DB migration. No changes in
`types.rs`/`config.rs`.

---

## 7. Edge cases & risks

- **CRLF / BOM / non-UTF-8** `.lrc` → loader handles (lossy UTF-8, BOM strip,
  `\r` trim); covered by tests.
- **Tag-only or empty `.lrc`** → treated as no lyrics, column hidden.
- **`[offset:]`** (some tags shift the whole song) → ignored in phase 1;
  lines highlight up to the offset early. Phase 2.
- **Unscanned / non-DB files** → the filesystem hook in `load()` works with or
  without a DB row; the scanner's existing `lyrics` table and
  `special_files` behavior are untouched.
- **Scroll math drift** → impossible by construction: view and
  `scroll_target` both use the shared constants in `lyrics.rs`.
- **5 Hz update cadence** → highlight steps at ≤5/s; matches the existing
  position-text update rate, no extra render pressure.
- **Pre-existing quirk (out of scope):** `AudioView` poster discovery appends
  `.png` to the *full* path (`"track.mp3.png"`), while the scanner uses
  `file_stem`. Lyrics discovery uses the correct primitive
  (`Path::with_extension("lrc")`).
- **Large `.lrc`** (thousands of lines) → parser is O(n); iced text column
  handles it; the `scrollable` virtualizes nothing, same as the tab list view.

---

## 8. Verification

1. `cargo test` — new `lyrics` unit tests (incl. the real
   `testsamples_archiv/01 Politik.lrc` fixture and the `file://` URL form)
   plus the existing audio tests (headless libmpv, as today).
2. **End-to-end** — `app::views::lyrics_integration::lyrics_column_flow`
   (`src/app/views.rs`): builds the real `App` headless, opens a generated
   mp3 + same-name `.lrc` through the `Message::AudioMessage(Open)` path, and
   asserts discovery, a successful `view()` build of the audio view *with*
   the lyrics column, `LyricSeek` (position + follow re-arm + scroll target),
   `LyricsScroll` (follow off, same-line suppression), follow resumption on a
   new line (then settling, no repeated commands), and that a file *without*
   an `.lrc` loads with an empty column and still builds. Fixture is
   `ffmpeg`-generated (same soft dependency as the `audio::audio` tests);
   the test skips without it.
3. `cargo clippy` — new non-test code adds no warnings (test-module
   `expect()`s match the repo's existing test style).
4. Manual smoke in the real UI (when a display is available):
   - open `testsamples_archiv/01 Politik.mp3` (has `01 Politik.lrc`) →
     right column visible with lyrics;
   - play → active line highlights and column follows;
   - wheel-scroll → follow stops; playback passes to the next line →
     follow resumes;
   - click a line → seeks, playback continues from there;
   - open `testsamples_archiv/cre225-permakultur.mp3` (no `.lrc`) → no
     column, layout identical to today;
   - Next/Previous track buttons swap the lyrics per file;
   - fullscreen with lyrics visible → column persists, controls unchanged.

---

## 9. Future extensions (explicitly out of scope)

- `[offset:]` correction.
- Settings toggle (cosmic-config `Config`) to force-hide the column.
- Centered follow instead of top-follow.
- Per-word karaoke highlighting from `<mm:ss.xx>` stamps.
- Reading the DB `lyrics` table (incl. `.srt`) as a fallback source.
