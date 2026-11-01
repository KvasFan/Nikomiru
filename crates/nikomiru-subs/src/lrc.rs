// SPDX-License-Identifier: GPL-3.0-only

//! LRC (ABK-002): the timed text of audiobooks and songs. `[mm:ss.xx]` starts
//! a line, and a line may have several; `[offset:ms]` moves them all; other
//! `[tag:value]` lines are metadata, and word stamps `<mm:ss.xx>` go. A line
//! lasts until the next.

use crate::cue::{Cue, Format, Markup, Track};

/// How long the last line stays.
const LAST_MS: i64 = 5_000;

/// `mm:ss`, `mm:ss.x…`, in ms.
fn stamp(text: &str) -> Option<i64> {
    let (minutes, rest) = text.split_once(':')?;
    let (seconds, fraction) = rest.split_once(['.', ':']).unwrap_or((rest, ""));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(minutes) || !digits(seconds) || !(fraction.is_empty() || digits(fraction)) {
        return None;
    }
    let ms: String = format!("{fraction:0<3}").chars().take(3).collect();
    let minutes = minutes.parse::<i64>().ok()?;
    let seconds = seconds.parse::<i64>().ok()?;
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(ms.parse::<i64>().ok()?)
}

/// The text of a line, without its word stamps.
fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after
            .find('>')
            .filter(|end| stamp(&after[..*end]).is_some())
        {
            Some(end) => rest = &after[end + 1..],
            None => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Parses an LRC file.
pub fn parse(text: &str) -> Track {
    let mut offset = 0;
    let mut lines: Vec<(i64, String)> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = line.trim();
        let mut times = Vec::new();
        while let Some(after) = rest.strip_prefix('[') {
            let Some(end) = after.find(']') else {
                break;
            };
            let tag = &after[..end];
            match stamp(tag) {
                Some(ms) => times.push(ms),
                None => {
                    if let Some(value) = tag.strip_prefix("offset:") {
                        offset = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            rest = after[end + 1..].trim_start();
        }
        let text = clean(rest);
        lines.extend(times.into_iter().map(|ms| (ms, text.clone())));
    }
    lines.sort_by_key(|(ms, _)| *ms);
    let mut cues = Vec::new();
    for (i, (start, text)) in lines.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let end = lines
            .get(i + 1)
            .map_or(start.saturating_add(LAST_MS), |(next, _)| *next);
        // a positive offset makes the words come sooner
        cues.push(Cue {
            start_ms: start.saturating_sub(offset).max(0),
            end_ms: end.saturating_sub(offset).max(0),
            plain_text: text.clone(),
            markup: Markup::default(),
            style: None,
            layer: 0,
            order: cues.len(),
        });
    }
    Track {
        format: Format::Lrc,
        cues,
        styles: Vec::new(),
        play_res: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ABK-002
    #[test]
    fn lines_last_until_the_next() {
        let t = parse("[ar:someone]\n[00:01.50]一行目\n[00:03.00]二行目\n");
        let cues: Vec<_> = t
            .cues
            .iter()
            .map(|c| (c.start_ms, c.end_ms, c.plain_text.as_str()))
            .collect();
        assert_eq!(cues, [(1500, 3000, "一行目"), (3000, 8000, "二行目")]);
        assert_eq!(t.format, Format::Lrc);
    }

    // T-ABK-002: several stamps on a line, out of order; an empty line ends one
    #[test]
    fn stamps_and_blanks() {
        let t = parse("[00:04.00][00:01.00]同じ\n[00:02.00]\n[00:03.00]次\n");
        let cues: Vec<_> = t
            .cues
            .iter()
            .map(|c| (c.start_ms, c.end_ms, c.plain_text.as_str()))
            .collect();
        assert_eq!(
            cues,
            [
                (1000, 2000, "同じ"),
                (3000, 4000, "次"),
                (4000, 9000, "同じ")
            ]
        );
    }

    // T-ABK-002
    #[test]
    fn offsets_and_word_stamps() {
        let t = parse("[offset:500]\n[00:02.00]<00:02.00>一<00:02.50>二 a<b\n");
        assert_eq!(t.cues[0].start_ms, 1500);
        assert_eq!(t.cues[0].plain_text, "一二 a<b");
    }

    #[test]
    fn stamps_are_read_at_any_precision() {
        assert_eq!(stamp("01:02"), Some(62_000));
        assert_eq!(stamp("01:02.5"), Some(62_500));
        assert_eq!(stamp("01:02.50"), Some(62_500));
        assert_eq!(stamp("01:02.505"), Some(62_505));
        for bad in ["", "a:b", "ar:name", "1", "01:02.x"] {
            assert_eq!(stamp(bad), None, "{bad}");
        }
    }

    // T-FUZZ-001: numbers no file should hold
    #[test]
    fn huge_numbers_are_no_trouble() {
        assert_eq!(stamp("999999999999999999:00"), None);
        let t = parse("[offset:-9223372036854775808]\n[99999999:00.00]一\n");
        assert_eq!(t.cues.len(), 1);
    }

    #[test]
    fn what_is_not_lrc_is_nothing() {
        assert!(parse("").cues.is_empty());
        assert!(parse("plain text\n[broken\n[]\n").cues.is_empty());
    }
}
