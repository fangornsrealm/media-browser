//! LRC lyrics: discovery, parsing, and active-line lookup for the audio view.
//!
//! An external lyrics file is a plain-text `.lrc` file with the same name as
//! the audio file (e.g. `01 Politik.mp3` next to `01 Politik.lrc`). Each
//! lyric line carries one or more `[mm:ss.xx]` timestamps; tag lines such as
//! `[ar:..]` are metadata; timestamp-less lines continue the previous lyric.
//!
//! Std-only: no external parsing dependency.

/// One timestamped lyric line.
///
/// A source line carrying several stamps yields one `LyricLine` per stamp,
/// all sharing the same `text`.
#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    /// Start time in seconds.
    pub time: f64,
    /// The lyric text. May be empty — a spacer line.
    pub text: String,
}

/// Rendered height (px) of one line in the lyrics column.
///
/// Shared by the view (which renders with these values) and
/// [`scroll_target`] (which sums them), so the scroll math and the layout
/// cannot drift apart.
pub const LINE_PX: f32 = 16.0;
/// Spacing between lines (cosmic `space_xs`).
pub const SPACING_PX: f32 = 8.0;
/// Height of an empty (spacer) line.
pub const SPACER_PX: f32 = 12.0;

/// Rendered height of one [`LyricLine`].
pub fn row_height(line: &LyricLine) -> f32 {
    if line.text.is_empty() {
        SPACER_PX
    } else {
        LINE_PX + SPACING_PX
    }
}

/// Scroll offset (px from the top) that places the line at `active` at the
/// top of the lyrics column viewport.
pub fn scroll_target(lines: &[LyricLine], active: usize) -> f32 {
    lines.iter().take(active).map(row_height).sum()
}

/// Index of the line active at playback position `pos` seconds: the last
/// line with `time <= pos`. `None` before the first line.
///
/// [`parse`] keeps lines sorted by time (stable), so this is a binary
/// search.
pub fn active_index(lines: &[LyricLine], pos: f64) -> Option<usize> {
    let idx = lines.partition_point(|line| line.time <= pos);
    idx.checked_sub(1)
}

/// Parse LRC content into lines.
///
/// - `[mm:ss.xx]` stamps (minutes may be 2+ digits, fractional part 0–3
///   digits) delimit lyric lines; a line may carry several stamps.
/// - Tag stamps (`[ar:..]`, `[ti:..]`, `[offset:..]`, …) are skipped.
///   `[offset:]` is not applied (phase 2, see `docs/plan-lyrics-view.md`).
/// - Timestamp-less lines continue the previous lyric (joined with `\n`);
///   before the first timestamp they are discarded (headers).
/// - Karaoke `<mm:ss.xx>` segments are stripped from the text.
pub fn parse(contents: &str) -> Vec<LyricLine> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut out: Vec<LyricLine> = Vec::new();
    for raw in contents.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }

        // Collect the leading stamps.
        let mut rest = line;
        let mut times: Vec<f64> = Vec::new();
        loop {
            if !rest.starts_with('[') {
                break;
            }
            let Some(close) = rest[1..].find(']') else {
                break;
            };
            let inner = &rest[1..close + 1];
            match stamp_seconds(inner) {
                Some(time) => {
                    times.push(time);
                    rest = &rest[close + 2..];
                }
                None => break,
            }
        }
        let text = rest.trim().to_string();

        if times.is_empty() {
            // Continuation of the previous lyric (or header text, which we
            // discard).
            if let (true, Some(last)) = (!text.is_empty(), out.last_mut()) {
                last.text.push('\n');
                last.text.push_str(&text);
            }
            continue;
        }

        let text = strip_karaoke(&text);
        for time in times {
            out.push(LyricLine {
                time,
                text: text.clone(),
            });
        }
    }

    // Some hand-edited LRC files are not sorted; a stable sort fixes the
    // highlight order without changing equal-time (multi-stamp) order.
    out.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Find and load the external lyrics for an audio file.
///
/// `audio_path` may be a plain path or a `file://` URL (the audio view
/// keeps its path as a [`url::Url`]). Looks for a file with the same name
/// and a `.lrc` extension next to the audio file. Returns `None` when no
/// such file exists, it is unreadable, or it contains no non-empty lyric
/// lines (tag-only files count as "no lyrics").
pub fn from_audio_file(audio_path: &str) -> Option<Vec<LyricLine>> {
    let lrc_path = crate::audio::coverart::same_stem_with_ext(audio_path, "lrc")?;
    let bytes = std::fs::read(&lrc_path).ok()?;
    let lines = parse(&decode(&bytes));
    lines
        .iter()
        .any(|line| !line.text.is_empty())
        .then_some(lines)
}

/// Decode file bytes: strip a UTF-8 BOM, lossy-convert to UTF-8.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Parse the inside of a timestamp bracket: `mm:ss` or `mm:ss.f+`.
fn stamp_seconds(inner: &str) -> Option<f64> {
    let (minutes, rest) = inner.split_once(':')?;
    let (seconds, frac) = match rest.find('.') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    if minutes.is_empty()
        || seconds.is_empty()
        || !minutes.bytes().all(|b| b.is_ascii_digit())
        || !seconds.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    let frac = if frac.is_empty() {
        0.0
    } else {
        let digits: String = frac
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(3)
            .collect();
        if digits.is_empty() {
            return None;
        }
        let value: f64 = digits.parse().ok()?;
        value / 10f64.powi(digits.len() as i32)
    };
    Some(minutes * 60.0 + seconds + frac)
}

/// Remove karaoke `<…>` segments from a lyric text.
fn strip_karaoke(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lines() {
        let lines = parse("[00:05.00]Hello\n[00:10.50]World");
        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: 5.0,
                    text: "Hello".to_string()
                },
                LyricLine {
                    time: 10.5,
                    text: "World".to_string()
                },
            ]
        );
    }

    #[test]
    fn multi_stamp_line_expands() {
        let lines = parse("[00:10.00][00:20.00]Same words");
        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: 10.0,
                    text: "Same words".to_string()
                },
                LyricLine {
                    time: 20.0,
                    text: "Same words".to_string()
                },
            ]
        );
    }

    #[test]
    fn header_tags_skipped_and_continuation_joined() {
        let lines = parse(
            "[ar:Artist]\n[ti:Title]\n[00:01.00]First line\ncontinues here\n[00:02.00]Second",
        );
        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: 1.0,
                    text: "First line\ncontinues here".to_string()
                },
                LyricLine {
                    time: 2.0,
                    text: "Second".to_string()
                },
            ]
        );
    }

    #[test]
    fn header_text_before_first_stamp_discarded() {
        let lines = parse("just a header line\n[00:01.00]Line");
        assert_eq!(
            lines,
            vec![LyricLine {
                time: 1.0,
                text: "Line".to_string()
            }]
        );
    }

    #[test]
    fn crlf_and_bom() {
        let contents = "\u{feff}[00:01.00]A\r\n[00:02.00]B\r\n";
        let lines = parse(contents);
        assert_eq!(
            lines,
            vec![
                LyricLine {
                    time: 1.0,
                    text: "A".to_string()
                },
                LyricLine {
                    time: 2.0,
                    text: "B".to_string()
                },
            ]
        );
    }

    #[test]
    fn karaoke_segments_stripped() {
        let lines = parse("[00:01.00]word <00:01.50>word two");
        assert_eq!(
            lines,
            vec![LyricLine {
                time: 1.0,
                text: "word word two".to_string()
            }]
        );
    }

    #[test]
    fn malformed_stamps_become_text() {
        let lines = parse("[ch:Chorus] not a stamp\n[00:01.00]Line");
        assert_eq!(
            lines,
            vec![LyricLine {
                time: 1.0,
                text: "Line".to_string()
            }]
        );
    }

    #[test]
    fn unsorted_lines_sorted_stably() {
        let lines = parse("[00:02.00]B\n[00:01.00]A");
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["A", "B"]
        );
    }

    #[test]
    fn empty_stamped_line_is_spacer() {
        let lines = parse("[00:01.00]A\n[00:02.00]\n[00:03.00]B");
        assert_eq!(lines.len(), 3);
        assert!(lines[1].text.is_empty());
        assert_eq!(scroll_target(&lines, 2), LINE_PX + SPACING_PX + SPACER_PX);
    }

    #[test]
    fn tag_only_file_is_empty() {
        assert!(parse("[ar:Artist]\n[ti:Title]").is_empty());
    }

    #[test]
    fn active_index_edges() {
        let lines = parse("[00:10.00]A\n[00:20.00]B");
        assert_eq!(active_index(&lines, 0.0), None);
        assert_eq!(active_index(&lines, 9.99), None);
        assert_eq!(active_index(&lines, 10.0), Some(0));
        assert_eq!(active_index(&lines, 19.99), Some(0));
        assert_eq!(active_index(&lines, 20.0), Some(1));
        assert_eq!(active_index(&lines, 999.0), Some(1));
        assert_eq!(active_index(&[], 100.0), None);
    }

    #[test]
    fn scroll_target_prefix_sums() {
        let lines = parse("[00:01.00]A\n[00:02.00]\n[00:03.00]B");
        assert_eq!(scroll_target(&lines, 0), 0.0);
        assert_eq!(scroll_target(&lines, 1), LINE_PX + SPACING_PX);
        assert_eq!(scroll_target(&lines, 2), LINE_PX + SPACING_PX + SPACER_PX);
    }

    #[test]
    fn from_audio_file_sample_fixture() {
        // Real-world fixture from the repo's test samples.
        let audio = "testsamples_archiv/01 Politik.mp3";
        if !std::path::Path::new(audio).is_file() {
            return; // fixture not present in this checkout
        }
        let lines = from_audio_file(audio).expect("sample .lrc must parse");
        let first = lines
            .iter()
            .find(|l| !l.text.is_empty())
            .expect("at least one lyric line");
        assert!(
            (first.time - 26.17).abs() < 1e-3,
            "first line at 26.17s, got {}",
            first.time
        );
        assert_eq!(first.text, "Look at Earth from outer space");
        // The blank stamped line at 01:10.33 must survive as a spacer.
        assert!(lines
            .iter()
            .any(|l| (l.time - 70.33).abs() < 1e-3 && l.text.is_empty()));
    }

    #[test]
    fn from_audio_file_accepts_file_url() {
        // The audio view keeps its path as a `file://` URL.
        let audio = "testsamples_archiv/01 Politik.mp3";
        let Some(absolute) = std::path::Path::new(audio).canonicalize().ok() else {
            return; // fixture not present in this checkout
        };
        let url = url::Url::from_file_path(&absolute).expect("file url");
        let lines = from_audio_file(&url.to_string()).expect("url form must parse");
        assert!(lines.iter().any(|l| !l.text.is_empty()));
    }

    #[test]
    fn from_audio_file_missing_is_none() {
        assert!(from_audio_file("/nonexistent/dir/track.mp3").is_none());
    }
}
