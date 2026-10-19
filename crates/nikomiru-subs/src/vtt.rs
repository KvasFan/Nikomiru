// SPDX-License-Identifier: GPL-3.0-only

//! WebVTT (SUB-002): header, cue identifiers, cue settings (`line`,
//! `position`, `size`, `align`), `<c> <v> <lang> <b> <i> <u> <ruby> <rt>` tags
//! and timestamp tags; `NOTE`, `STYLE` and `REGION` blocks are ignored.

use crate::cue::{Cue, Format, TextAlign, Track, VttLine, VttSettings};
use crate::markup::{TextBuilder, push_html_line};
use crate::srt::lines;
use crate::time::clock_ms;

fn percent(value: &str) -> Option<f64> {
    let v: f64 = value.strip_suffix('%')?.parse().ok()?;
    (0.0..=100.0).contains(&v).then_some(v)
}

fn settings(text: &str) -> VttSettings {
    let mut s = VttSettings::default();
    for token in text.split_whitespace() {
        let Some((key, value)) = token.split_once(':') else {
            continue;
        };
        // `line:10%,start` and `position:30%,line-left`: only the number is used
        let value = value.split(',').next().unwrap_or("");
        match key {
            "line" => {
                s.line = percent(value)
                    .map(VttLine::Percent)
                    .or_else(|| value.parse().ok().map(VttLine::Number));
            }
            "position" => s.position = percent(value),
            "size" => s.size = percent(value),
            "align" => {
                s.align = match value {
                    "start" => Some(TextAlign::Start),
                    "center" | "middle" => Some(TextAlign::Center),
                    "end" => Some(TextAlign::End),
                    "left" => Some(TextAlign::Left),
                    "right" => Some(TextAlign::Right),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    s
}

/// `start --> end settings…`.
fn timing(line: &str) -> Option<(i64, i64, VttSettings)> {
    let (start, rest) = line.split_once("-->")?;
    let rest = rest.trim_start();
    let end_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let start = clock_ms(start)?;
    let end = clock_ms(&rest[..end_len])?;
    Some((start, end, settings(&rest[end_len..])))
}

/// Parses a WebVTT file.
pub fn parse(text: &str) -> Track {
    let lines = lines(text);
    let mut cues = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        // a block starts after blank lines
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        let block_start = i;
        while i < lines.len() && !lines[i].trim().is_empty() {
            i += 1;
        }
        let block = &lines[block_start..i];
        let Some(first) = block.first() else { continue };
        let keyword = first.split_whitespace().next().unwrap_or("");
        if matches!(keyword, "NOTE" | "STYLE" | "REGION") || first.starts_with("WEBVTT") {
            continue;
        }
        let (timing_at, parsed) = match timing(first) {
            Some(t) => (0, t),
            None => match block.get(1).and_then(|l| timing(l)) {
                Some(t) => (1, t),
                None => continue,
            },
        };
        let (start_ms, end_ms, settings) = parsed;
        let mut b = TextBuilder::default();
        let (mut styles, mut ruby, mut rt) = (Vec::new(), None, None);
        for (n, line) in block[timing_at + 1..].iter().enumerate() {
            if n > 0 {
                b.newline();
            }
            push_html_line(line.trim(), &mut b, &mut styles, &mut ruby, &mut rt);
        }
        let (plain_text, mut markup) = b.finish(|p| p);
        if plain_text.is_empty() || end_ms < start_ms {
            continue;
        }
        if settings != VttSettings::default() {
            markup.vtt = Some(settings);
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
        format: Format::Vtt,
        cues,
        styles: Vec::new(),
        play_res: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_settings_notes_and_voices() {
        let t = parse(
            "WEBVTT - title\n\nNOTE a comment\nover lines\n\nSTYLE\n::cue { color: red }\n\nid-1\n00:01.000 --> 00:02.500 line:10% position:20%,line-left align:start\n<v Taro>こんにちは</v>\n<c.yellow>世界</c>\n",
        );
        assert_eq!(t.cues.len(), 1);
        let c = &t.cues[0];
        assert_eq!((c.start_ms, c.end_ms), (1000, 2500));
        assert_eq!(c.plain_text, "こんにちは\n世界");
        let vtt = c.markup.vtt.as_ref().unwrap();
        assert_eq!(vtt.line, Some(VttLine::Percent(10.0)));
        assert_eq!(vtt.position, Some(20.0));
        assert_eq!(vtt.align, Some(TextAlign::Start));
    }
}
