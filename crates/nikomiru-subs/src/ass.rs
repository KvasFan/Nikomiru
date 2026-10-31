// SPDX-License-Identifier: GPL-3.0-only

//! ASS and SSA (SUB-003, basic for 1.0): `[Script Info]` (PlayResX/Y, default
//! 384×288; WrapStyle), `[V4+ Styles]` / `[V4 Styles]`, `[Events]` Dialogue lines
//! with the columns of their `Format`, and the overrides of
//! [`crate::markup::apply_override`]. Lines whose only content is a drawing
//! (`\p1`) produce no cue.

use crate::cue::{AssStyle, Cue, Format, Rgb, Track};
use crate::markup::{TextBuilder, ass_color, legacy_alignment, push_ass_text};
use crate::srt::lines;
use crate::time::ass_ms;

/// Columns of `[Events]` when the file has no `Format` line.
const DEFAULT_EVENTS: [&str; 10] = [
    "layer", "start", "end", "style", "name", "marginl", "marginr", "marginv", "effect", "text",
];

/// Columns of `[V4+ Styles]` when the file has no `Format` line.
const DEFAULT_STYLES: [&str; 23] = [
    "name",
    "fontname",
    "fontsize",
    "primarycolour",
    "secondarycolour",
    "outlinecolour",
    "backcolour",
    "bold",
    "italic",
    "underline",
    "strikeout",
    "scalex",
    "scaley",
    "spacing",
    "angle",
    "borderstyle",
    "outline",
    "shadow",
    "alignment",
    "marginl",
    "marginr",
    "marginv",
    "encoding",
];

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Info,
    Styles { legacy: bool },
    Events,
    Other,
}

fn columns(format: &str) -> Vec<String> {
    format
        .split(',')
        .map(|c| c.trim().to_ascii_lowercase())
        .collect()
}

/// The fields of a line, by column; the last column takes the rest of the line.
fn fields<'a, 'c>(line: &'a str, columns: &'c [String]) -> Vec<(&'c str, &'a str)> {
    let values = line.splitn(columns.len().max(1), ',');
    columns.iter().map(String::as_str).zip(values).collect()
}

fn get<'a>(fields: &[(&str, &'a str)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(k, _)| *k == name)
        .map_or("", |(_, v)| v.trim())
}

fn flag(value: &str) -> bool {
    value.trim().parse::<i64>().is_ok_and(|v| v != 0)
}

fn style(fields: &[(&str, &str)], legacy: bool) -> AssStyle {
    let number = |name: &str, default: f32| get(fields, name).parse().unwrap_or(default);
    let color = |name: &str, default: Rgb| ass_color(get(fields, name)).unwrap_or(default);
    let alignment = get(fields, "alignment").parse::<u8>().unwrap_or(2);
    let alignment = if legacy {
        legacy_alignment(alignment).unwrap_or(2)
    } else if (1..=9).contains(&alignment) {
        alignment
    } else {
        2
    };
    AssStyle {
        name: get(fields, "name").trim_start_matches('*').to_string(),
        font: get(fields, "fontname").to_string(),
        size: number("fontsize", 18.0),
        primary: color("primarycolour", Rgb(255, 255, 255)),
        secondary: color("secondarycolour", Rgb(255, 0, 0)),
        outline_color: color("outlinecolour", Rgb(0, 0, 0)),
        back_color: color("backcolour", Rgb(0, 0, 0)),
        bold: flag(get(fields, "bold")),
        italic: flag(get(fields, "italic")),
        underline: flag(get(fields, "underline")),
        strike: flag(get(fields, "strikeout")),
        outline: number("outline", 2.0),
        shadow: number("shadow", 2.0),
        alignment,
        margin_v: get(fields, "marginv").parse().unwrap_or(0),
    }
}

/// PlayResX/Y with libass's defaults: 384×288, or 4:3 from the one that is set.
fn play_res(x: Option<u32>, y: Option<u32>) -> (u32, u32) {
    match (x, y) {
        (Some(x), Some(y)) if x > 0 && y > 0 => (x, y),
        (Some(x), _) if x > 0 => (x, x * 3 / 4),
        (_, Some(y)) if y > 0 => (y * 4 / 3, y),
        _ => (384, 288),
    }
}

/// Parses an ASS or SSA file.
pub fn parse(text: &str) -> Track {
    let mut section = Section::Other;
    let (mut res_x, mut res_y, mut wrap_style) = (None, None, 0u32);
    let mut style_columns: Vec<String> = DEFAULT_STYLES.iter().map(|s| s.to_string()).collect();
    let mut event_columns: Vec<String> = DEFAULT_EVENTS.iter().map(|s| s.to_string()).collect();
    let mut styles = Vec::new();
    let mut dialogues: Vec<(usize, Vec<(String, String)>)> = Vec::new();

    for line in lines(text) {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim().to_ascii_lowercase();
            section = match name.as_str() {
                "script info" => Section::Info,
                "v4+ styles" => Section::Styles { legacy: false },
                "v4 styles" => Section::Styles { legacy: true },
                "events" => Section::Events,
                _ => Section::Other,
            };
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        match (section, key.as_str()) {
            (Section::Info, "playresx") => res_x = value.trim().parse().ok(),
            (Section::Info, "playresy") => res_y = value.trim().parse().ok(),
            (Section::Info, "wrapstyle") => wrap_style = value.trim().parse().unwrap_or(0),
            (Section::Styles { .. }, "format") => style_columns = columns(value),
            (Section::Styles { legacy }, "style") => {
                styles.push(style(&fields(value.trim_start(), &style_columns), legacy));
            }
            (Section::Events, "format") => event_columns = columns(value),
            (Section::Events, "dialogue") => {
                let f = fields(value.trim_start(), &event_columns)
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                dialogues.push((dialogues.len(), f));
            }
            _ => {}
        }
    }

    let (px, py) = play_res(res_x, res_y);
    let mut cues = Vec::new();
    for (order, f) in dialogues {
        let f: Vec<(&str, &str)> = f.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let (Some(start_ms), Some(end_ms)) = (ass_ms(get(&f, "start")), ass_ms(get(&f, "end")))
        else {
            continue;
        };
        if end_ms < start_ms {
            continue;
        }
        let text = f.iter().find(|(k, _)| *k == "text").map_or("", |(_, v)| *v);
        let mut b = TextBuilder::default();
        push_ass_text(text, &mut b, wrap_style == 2);
        let (plain_text, markup) = b.finish(|(x, y)| (x / f64::from(px), y / f64::from(py)));
        if plain_text.is_empty() {
            continue;
        }
        let style = get(&f, "style").trim_start_matches('*');
        cues.push(Cue {
            start_ms,
            end_ms,
            plain_text,
            markup,
            style: (!style.is_empty()).then(|| style.to_string()),
            layer: get(&f, "layer").parse().unwrap_or(0),
            order,
        });
    }
    Track::sort(&mut cues);
    Track {
        format: Format::Ass,
        cues,
        styles,
        play_res: Some((px, py)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Noto Sans,72,&H00FFFFFF,&H000000FF,&H00000000,&H64000000,-1,0,0,0,100,100,0,0,1,3,1,2,10,10,40,1\nStyle: Sign,Arial,48,&H0000FFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,8,10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

    #[test]
    fn events_styles_and_play_res() {
        let t = parse(&format!(
            "{HEADER}Dialogue: 1,0:00:01.00,0:00:03.50,Sign,,0,0,0,,{{\\pos(960,540)}}看板, with comma\nDialogue: 0,0:00:01.00,0:00:02.00,Default,Taro,0,0,0,,一行目\\N二行目\nComment: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,hidden\nDialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,{{\\p1}}m 0 0 l 10 10{{\\p0}}\n"
        ));
        assert_eq!(t.play_res, Some((1920, 1080)));
        assert_eq!(t.cues.len(), 2);
        // equal starts keep the file order
        assert_eq!(t.cues[0].plain_text, "看板, with comma");
        assert_eq!(t.cues[0].layer, 1);
        assert_eq!(t.cues[0].markup.pos, Some((0.5, 0.5)));
        assert_eq!(t.cues[1].plain_text, "一行目\n二行目");
        let sign = t.style("sign").unwrap();
        assert_eq!((sign.alignment, sign.primary), (8, Rgb(255, 255, 0)));
        assert!(t.style("Default").unwrap().bold);
    }

    #[test]
    fn defaults_without_play_res_or_format() {
        let t = parse("[Events]\nDialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,{\\an7}x\n");
        assert_eq!(t.play_res, Some((384, 288)));
        assert_eq!(t.cues[0].markup.anchor, Some(7));
        assert_eq!(play_res(Some(1280), None), (1280, 960));
    }
}
