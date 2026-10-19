// SPDX-License-Identifier: GPL-3.0-only

//! Building `plain_text` and its [`Markup`] from the inline syntax of the three
//! formats: HTML-like tags (SRT, WebVTT), ASS override blocks `{\…}` (ASS, and
//! SRT files that carry them, SUB-001) and ASS text escapes `\N \n \h`.

use crate::cue::{Markup, Rgb, Ruby, Run, RunStyle};

/// Accumulates text with the current style.
#[derive(Debug, Default)]
pub(crate) struct TextBuilder {
    text: String,
    runs: Vec<Run>,
    ruby: Vec<Ruby>,
    /// The style of the text pushed next.
    pub style: RunStyle,
    /// First `\an` seen.
    pub anchor: Option<u8>,
    /// First `\pos(x,y)` seen, in file coordinates.
    pub pos: Option<(f64, f64)>,
    /// ASS drawing mode (`\p1`…): text is not part of the cue.
    pub drawing: bool,
}

impl TextBuilder {
    /// Appends `s` with the current style (dropped in drawing mode).
    pub fn push(&mut self, s: &str) {
        if s.is_empty() || self.drawing {
            return;
        }
        let start = self.text.len();
        self.text.push_str(s);
        let end = self.text.len();
        if self.style.is_plain() {
            return;
        }
        if let Some(last) = self.runs.last_mut()
            && last.range.end == start
            && last.style == self.style
        {
            last.range.end = end;
            return;
        }
        self.runs.push(Run {
            range: start..end,
            style: self.style.clone(),
        });
    }

    /// A line break.
    pub fn newline(&mut self) {
        if !self.drawing {
            self.text.push('\n');
        }
    }

    /// Current length of the text.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// A reading over `range` of the text.
    pub fn add_ruby(&mut self, start: usize, reading: &str) {
        let reading = reading.trim();
        if !reading.is_empty() && start < self.text.len() {
            self.ruby.push(Ruby {
                range: start..self.text.len(),
                text: reading.to_string(),
            });
        }
    }

    /// The text without leading/trailing blank lines and spaces, and the markup
    /// with its ranges moved accordingly. `pos` is converted by `to_fraction`.
    pub fn finish(self, to_fraction: impl Fn((f64, f64)) -> (f64, f64)) -> (String, Markup) {
        let is_blank = |c: char| c.is_whitespace() && c != '\u{a0}';
        let start = self.text.len() - self.text.trim_start_matches(is_blank).len();
        let end = self.text.trim_end_matches(is_blank).len().max(start);
        let text = self.text[start..end].to_string();
        let clip = |r: &std::ops::Range<usize>| {
            let s = r.start.clamp(start, end) - start;
            let e = r.end.clamp(start, end) - start;
            (s < e).then_some(s..e)
        };
        let runs = self
            .runs
            .iter()
            .filter_map(|r| {
                clip(&r.range).map(|range| Run {
                    range,
                    style: r.style.clone(),
                })
            })
            .collect();
        let ruby = self
            .ruby
            .iter()
            .filter_map(|r| {
                clip(&r.range).map(|range| Ruby {
                    range,
                    text: r.text.clone(),
                })
            })
            .collect();
        let markup = Markup {
            runs,
            ruby,
            anchor: self.anchor,
            pos: self.pos.map(to_fraction),
            vtt: None,
        };
        (text, markup)
    }
}

/// An ASS colour `&HBBGGRR&` (or `&HAABBGGRR`, `H…`, decimal), as RGB.
pub(crate) fn ass_color(value: &str) -> Option<Rgb> {
    let v = value.trim().trim_end_matches('&');
    let v = v.trim_start_matches('&');
    let n = if let Some(hex) = v.strip_prefix('H').or_else(|| v.strip_prefix('h')) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        v.parse::<i64>().ok()? as u32
    };
    Some(Rgb(n as u8, (n >> 8) as u8, (n >> 16) as u8))
}

/// SSA legacy `\a` alignment (1–3 bottom, +4 top, +8 middle) as a numpad code.
pub(crate) fn legacy_alignment(a: u8) -> Option<u8> {
    match a {
        1..=3 => Some(a),
        5..=7 => Some(a + 2),
        9..=11 => Some(a - 5),
        _ => None,
    }
}

/// The tags of an override block, split at backslashes; parenthesised
/// arguments (which may contain backslashes, as in `\t(…\fs20)`) stay whole.
fn split_tags(block: &str) -> Vec<&str> {
    let mut tags = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    for (i, c) in block.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '\\' if depth == 0 => {
                if let Some(s) = start {
                    tags.push(block[s..i].trim());
                }
                start = Some(i + 1);
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        tags.push(block[s..].trim());
    }
    tags.into_iter().filter(|t| !t.is_empty()).collect()
}

fn flag(rest: &str) -> Option<Option<bool>> {
    match rest {
        "" => Some(None),
        "0" => Some(Some(false)),
        "1" => Some(Some(true)),
        _ => None,
    }
}

fn number_args(args: &str) -> Vec<f64> {
    args.trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .filter_map(|a| a.trim().parse().ok())
        .collect()
}

/// Applies the tags of one override block (`{…}` without the braces). The SUB-003
/// subset is honoured; every other tag is ignored without error (SUB-019).
pub(crate) fn apply_override(block: &str, b: &mut TextBuilder) {
    for tag in split_tags(block) {
        // longest names first: `pos` before `p`, `fscx` before `fs`, `an` before `a`
        if let Some(args) = tag.strip_prefix("pos") {
            if b.pos.is_none()
                && let [x, y] = number_args(args)[..]
            {
                b.pos = Some((x, y));
            }
        } else if tag.starts_with("pbo") {
        } else if let Some(level) = tag.strip_prefix('p')
            && let Ok(level) = level.parse::<u32>()
        {
            b.drawing = level > 0;
        } else if let Some(n) = tag.strip_prefix("an") {
            if b.anchor.is_none()
                && let Ok(n) = n.parse::<u8>()
                && (1..=9).contains(&n)
            {
                b.anchor = Some(n);
            }
        } else if let Some(n) = tag.strip_prefix('a')
            && let Ok(n) = n.parse::<u8>()
        {
            if b.anchor.is_none() {
                b.anchor = legacy_alignment(n);
            }
        } else if let Some(name) = tag.strip_prefix("fn") {
            b.style.font = (!name.is_empty()).then(|| name.to_string());
        } else if tag.starts_with("fsc") || tag.starts_with("fsp") {
        } else if let Some(size) = tag.strip_prefix("fs") {
            if size.is_empty() {
                b.style.size = None;
            } else if let Ok(size) = size.parse::<f32>()
                && size > 0.0
            {
                b.style.size = Some(size);
            }
        } else if let Some(w) = tag.strip_prefix('b')
            && (w.is_empty() || w.bytes().all(|c| c.is_ascii_digit()))
        {
            b.style.bold = match w.parse::<u32>() {
                Err(_) => None,
                Ok(0) => Some(false),
                Ok(1) => Some(true),
                Ok(weight) => Some(weight >= 600),
            };
        } else if let Some(v) = tag.strip_prefix('i').and_then(flag) {
            b.style.italic = v;
        } else if let Some(v) = tag.strip_prefix('u').and_then(flag) {
            b.style.underline = v;
        } else if let Some(v) = tag.strip_prefix('s').and_then(flag) {
            b.style.strike = v;
        } else if let Some(c) = tag.strip_prefix("1c").or_else(|| tag.strip_prefix('c')) {
            b.style.color = if c.is_empty() { None } else { ass_color(c) };
        } else if tag.starts_with('r') && !tag.starts_with("ro") {
            // `\r` / `\rStyle`: back to the line's style
            b.style = RunStyle::default();
        }
    }
}

/// Text with ASS escapes and override blocks (an ASS `Dialogue` text, or an
/// SRT line). `soft_breaks` makes `\n` a line break (ASS WrapStyle 2) instead
/// of a space.
pub(crate) fn push_ass_text(text: &str, b: &mut TextBuilder, soft_breaks: bool) {
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('{') {
            match after.find('}') {
                Some(end) => {
                    let block = &after[..end];
                    if block.contains('\\') {
                        apply_override(block, b);
                    }
                    // a block without tags is a comment: dropped (BUG-105)
                    rest = &after[end + 1..];
                }
                None => {
                    b.push("{");
                    rest = after;
                }
            }
            continue;
        }
        if let Some(after) = rest.strip_prefix('\\') {
            match after.chars().next() {
                Some('N') => b.newline(),
                Some('n') if soft_breaks => b.newline(),
                Some('n') => b.push(" "),
                Some('h') => b.push("\u{a0}"),
                _ => {
                    b.push("\\");
                    rest = after;
                    continue;
                }
            }
            rest = &after[1..];
            continue;
        }
        let next = rest.find(['{', '\\']).unwrap_or(rest.len());
        b.push(&rest[..next]);
        rest = &rest[next..];
    }
}

/// Named colours of HTML `<font color=…>` that subtitle files use.
fn named_color(name: &str) -> Option<Rgb> {
    Some(match name.to_ascii_lowercase().as_str() {
        "white" => Rgb(255, 255, 255),
        "black" => Rgb(0, 0, 0),
        "red" => Rgb(255, 0, 0),
        "lime" => Rgb(0, 255, 0),
        "green" => Rgb(0, 128, 0),
        "blue" => Rgb(0, 0, 255),
        "yellow" => Rgb(255, 255, 0),
        "cyan" | "aqua" => Rgb(0, 255, 255),
        "magenta" | "fuchsia" => Rgb(255, 0, 255),
        "silver" => Rgb(192, 192, 192),
        "gray" | "grey" => Rgb(128, 128, 128),
        "orange" => Rgb(255, 165, 0),
        "purple" => Rgb(128, 0, 128),
        "maroon" => Rgb(128, 0, 0),
        "navy" => Rgb(0, 0, 128),
        "olive" => Rgb(128, 128, 0),
        "teal" => Rgb(0, 128, 128),
        "pink" => Rgb(255, 192, 203),
        _ => return None,
    })
}

/// `#RGB`, `#RRGGBB`, `RRGGBB` or a colour name.
pub(crate) fn html_color(value: &str) -> Option<Rgb> {
    let v = value.trim().trim_matches(['"', '\'']);
    let hex = v.strip_prefix('#').unwrap_or(v);
    let digits = |h: &str| u32::from_str_radix(h, 16).ok();
    match hex.len() {
        6 if hex.bytes().all(|c| c.is_ascii_hexdigit()) => {
            let n = digits(hex)?;
            Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
        }
        3 if v.starts_with('#') && hex.bytes().all(|c| c.is_ascii_hexdigit()) => {
            let n = digits(hex)?;
            let x = |s: u32| ((n >> s) & 0xf) as u8 * 17;
            Some(Rgb(x(8), x(4), x(0)))
        }
        _ => named_color(v),
    }
}

/// The value of attribute `name` in the inside of a tag (`font color="red"`).
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))?;
    let value = &tag[at + name.len() + 1..];
    let value = value.trim_start();
    if let Some(quoted) = value.strip_prefix('"') {
        return quoted.split('"').next();
    }
    if let Some(quoted) = value.strip_prefix('\'') {
        return quoted.split('\'').next();
    }
    value.split_whitespace().next()
}

/// Decodes HTML entities (`&amp;`, `&#123;`, `&#x1F;`, `&nbsp;`…).
pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let entity = after
            .find(';')
            .filter(|&end| end <= 10)
            .map(|end| (&after[..end], end));
        let decoded = entity.and_then(|(name, _)| match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "lrm" => Some('\u{200e}'),
            "rlm" => Some('\u{200f}'),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        });
        match (decoded, entity) {
            (Some(c), Some((_, end))) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One line of SRT or WebVTT text: HTML-like tags, entities, and the ASS override
/// blocks and escapes that files converted from ASS carry (BUG-105). `styles`
/// is the stack of the open tags, kept across lines.
pub(crate) fn push_html_line(
    line: &str,
    b: &mut TextBuilder,
    styles: &mut Vec<RunStyle>,
    ruby_start: &mut Option<usize>,
    in_rt: &mut Option<String>,
) {
    let mut rest = line;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<')
            && let Some(end) = after.find('>')
        {
            handle_tag(&after[..end], b, styles, ruby_start, in_rt);
            rest = &after[end + 1..];
            continue;
        }
        // the text up to the next tag (a lone `<` is text)
        let next = rest
            .char_indices()
            .skip(1)
            .find(|&(_, c)| c == '<')
            .map_or(rest.len(), |(i, _)| i);
        let text = decode_entities(&rest[..next]);
        if let Some(reading) = in_rt.as_mut() {
            reading.push_str(&text);
        } else {
            push_ass_text(&text, b, false);
        }
        rest = &rest[next..];
    }
}

fn handle_tag(
    tag: &str,
    b: &mut TextBuilder,
    styles: &mut Vec<RunStyle>,
    ruby_start: &mut Option<usize>,
    in_rt: &mut Option<String>,
) {
    let tag = tag.trim();
    if let Some(close) = tag.strip_prefix('/') {
        let name = close.trim().to_ascii_lowercase();
        match name.as_str() {
            "rt" => {
                if let (Some(start), Some(reading)) = (*ruby_start, in_rt.take()) {
                    b.add_ruby(start, &reading);
                }
            }
            "ruby" => *ruby_start = None,
            _ => {
                if let Some(previous) = styles.pop() {
                    b.style = previous;
                }
            }
        }
        return;
    }
    // WebVTT karaoke timestamps `<00:00:01.000>`
    if tag.starts_with(|c: char| c.is_ascii_digit()) {
        return;
    }
    let name: String = tag
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    match name.as_str() {
        "ruby" => {
            *ruby_start = Some(b.len());
            return;
        }
        "rt" => {
            *in_rt = Some(String::new());
            return;
        }
        "br" => {
            b.newline();
            return;
        }
        _ => {}
    }
    styles.push(b.style.clone());
    match name.as_str() {
        "b" => b.style.bold = Some(true),
        "i" => b.style.italic = Some(true),
        "u" => b.style.underline = Some(true),
        "s" => b.style.strike = Some(true),
        "font" => {
            if let Some(color) = attribute(tag, "color").and_then(html_color) {
                b.style.color = Some(color);
            }
            if let Some(face) = attribute(tag, "face") {
                b.style.font = Some(face.to_string());
            }
        }
        // c, v, lang, span and unknown tags: kept for nesting, no style
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ass(text: &str) -> (String, Markup) {
        let mut b = TextBuilder::default();
        push_ass_text(text, &mut b, false);
        b.finish(|p| p)
    }

    #[test]
    fn overrides_set_the_markup_and_leave_the_text() {
        let (text, m) = ass(r"{\an8\pos(192,10)}上{\b1\c&H0000FF&}太字{\b0\c}普通\N二行目");
        assert_eq!(text, "上太字普通\n二行目");
        assert_eq!(m.anchor, Some(8));
        assert_eq!(m.pos, Some((192.0, 10.0)));
        assert_eq!(&text[m.runs[0].range.clone()], "太字");
        assert_eq!(m.runs[0].style.bold, Some(true));
        assert_eq!(m.runs[0].style.color, Some(Rgb(255, 0, 0)));
        // `\b0` is an explicit "not bold", even if the ASS style is bold
        assert_eq!(&text[m.runs[1].range.clone()], "普通");
        assert_eq!(m.runs[1].style.bold, Some(false));
        assert_eq!(m.runs[1].style.color, None);
    }

    #[test]
    fn drawings_comments_and_unknown_tags_add_no_text() {
        let (text, _) =
            ass(r"{\p1}m 0 0 l 100 0 100 100{\p0}字{comment}{\t(0,500,\fs40)\fad(10,20)\k20}幕");
        assert_eq!(text, "字幕");
        let (text, _) = ass(r"a\hb\nc\\d");
        assert_eq!(text, "a\u{a0}b c\\\\d");
    }

    #[test]
    fn html_tags_entities_and_ruby() {
        let mut b = TextBuilder::default();
        let (mut styles, mut ruby, mut rt) = (Vec::new(), None, None);
        push_html_line(
            "<i>斜体</i> &amp; <font color=\"#00ff00\">緑</font> <ruby>漢<rt>かん</rt></ruby>{\\an8}",
            &mut b,
            &mut styles,
            &mut ruby,
            &mut rt,
        );
        let (text, m) = b.finish(|p| p);
        assert_eq!(text, "斜体 & 緑 漢");
        assert_eq!(m.anchor, Some(8));
        assert_eq!(&text[m.runs[0].range.clone()], "斜体");
        assert_eq!(m.runs[1].style.color, Some(Rgb(0, 255, 0)));
        assert_eq!(m.ruby[0].text, "かん");
        assert_eq!(&text[m.ruby[0].range.clone()], "漢");
    }

    #[test]
    fn colours() {
        assert_eq!(ass_color("&H00FF8000&"), Some(Rgb(0, 128, 255)));
        assert_eq!(html_color("#f00"), Some(Rgb(255, 0, 0)));
        assert_eq!(html_color("yellow"), Some(Rgb(255, 255, 0)));
        assert_eq!(decode_entities("a&#x41;&#66;&bogus;"), "aAB&bogus;");
    }
}
