// SPDX-License-Identifier: GPL-3.0-only

//! SubRip (SUB-001): BOM, CRLF/LF/CR, missing or out-of-order indices, blocks
//! not separated by blank lines, `{…}` override blocks (honoured like ASS, then
//! removed: BUG-105), `<i> <b> <u> <s> <font>` tags, `,` or `.` in timestamps.

use crate::cue::{Cue, Format, Track};
use crate::markup::{TextBuilder, push_html_line};
use crate::time::clock_ms;

/// `start --> end`, with anything after the end (SRT coordinates) ignored.
pub(crate) fn timing(line: &str) -> Option<(i64, i64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((clock_ms(start)?, clock_ms(end)?))
}

fn is_index(line: &str) -> bool {
    !line.is_empty() && line.bytes().all(|c| c.is_ascii_digit())
}

/// Lines of a text file with any line ending, without a leading BOM.
pub(crate) fn lines(text: &str) -> Vec<&str> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(['\r', '\n']) {
        out.push(&rest[..at]);
        let skip = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        rest = &rest[at + skip..];
    }
    out.push(rest);
    out
}

/// Parses an SRT file.
pub fn parse(text: &str) -> Track {
    let lines = lines(text);
    let mut cues = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((start_ms, end_ms)) = timing(lines[i]) else {
            i += 1;
            continue;
        };
        i += 1;
        let mut b = TextBuilder::default();
        let (mut styles, mut ruby, mut rt) = (Vec::new(), None, None);
        let mut first = true;
        while i < lines.len() {
            let line = lines[i].trim();
            let next_is_timing = lines.get(i + 1).is_some_and(|n| timing(n).is_some());
            if line.is_empty() || timing(line).is_some() || (is_index(line) && next_is_timing) {
                break;
            }
            if !first {
                b.newline();
            }
            first = false;
            push_html_line(line, &mut b, &mut styles, &mut ruby, &mut rt);
            i += 1;
        }
        // `\pos` in SRT uses the ASS default play area
        let (plain_text, markup) = b.finish(|(x, y)| (x / 384.0, y / 288.0));
        if plain_text.is_empty() || end_ms < start_ms {
            continue;
        }
        let order = cues.len();
        cues.push(Cue {
            start_ms,
            end_ms,
            plain_text,
            markup,
            style: None,
            layer: 0,
            order,
        });
    }
    Track::sort(&mut cues);
    Track {
        format: Format::Srt,
        cues,
        styles: Vec::new(),
        play_res: None,
    }
}

/// Writes `cues` (start, end in milliseconds, text with its lines apart by
/// `\n`) as SRT, for text made by Nikomiru (SUB-028). `<` and `{`, which
/// the parser takes for markup, become their full-width forms; empty lines
/// inside a text are dropped, since they would end the cue.
pub fn write(cues: &[(i64, i64, String)]) -> String {
    fn clock(ms: i64) -> String {
        let ms = ms.max(0);
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    }
    let mut out = String::new();
    for (n, (start, end, text)) in cues.iter().enumerate() {
        let text: Vec<String> = text
            .lines()
            .map(|l| l.trim().replace('<', "\u{ff1c}").replace('{', "\u{ff5b}"))
            .filter(|l| !l.is_empty())
            .collect();
        if text.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            n + 1,
            clock(*start),
            clock(*end),
            text.join("\n")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-028: written and read back the same
    #[test]
    fn written_cues_read_back() {
        let cues = vec![
            (1_000, 2_500, "行くぞ！\n\n待って<".to_string()),
            (3_723_004, 3_724_000, "{二行目}".to_string()),
            (5_000, 6_000, " \n ".to_string()),
        ];
        let text = write(&cues);
        assert!(text.contains("01:02:03,004 --> 01:02:04,000"), "{text}");
        let track = parse(&text);
        let read: Vec<_> = track
            .cues
            .iter()
            .map(|c| (c.start_ms, c.end_ms, c.plain_text.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                (1_000, 2_500, "行くぞ！\n待って\u{ff1c}"),
                (3_723_004, 3_724_000, "\u{ff5b}二行目}"),
            ]
        );
    }

    #[test]
    fn blocks_with_and_without_indices() {
        let t = parse(
            "\u{feff}1\r\n00:00:01,000 --> 00:00:02,000\r\nuno\r\ndue\r\n\r\n00:00:03,000 --> 00:00:04,000\nsenza indice\n7\n00:00:00,500 --> 00:00:00,900\nprima\n",
        );
        let texts: Vec<&str> = t.cues.iter().map(|c| c.plain_text.as_str()).collect();
        assert_eq!(texts, ["prima", "uno\ndue", "senza indice"]);
        assert_eq!((t.cues[1].start_ms, t.cues[1].end_ms), (1000, 2000));
    }

    #[test]
    fn negative_durations_and_empty_cues_are_dropped() {
        let t = parse(
            "1\n00:00:05,000 --> 00:00:04,000\nback\n\n2\n00:00:06,000 --> 00:00:07,000\n{\\an8}\n\n3\n00:00:08,000 --> 00:00:09,000\nok\n",
        );
        assert_eq!(t.cues.len(), 1);
        assert_eq!(t.cues[0].plain_text, "ok");
    }
}
