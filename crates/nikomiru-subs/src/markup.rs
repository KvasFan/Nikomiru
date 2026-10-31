// SPDX-License-Identifier: GPL-3.0-only

//! Building `plain_text` and its [`Markup`] from the inline syntax of the three
//! formats: HTML-like tags (SRT, WebVTT), ASS override blocks `{\…}` (ASS, and
//! SRT files that carry them, SUB-001) and ASS text escapes `\N \n \h`.

use crate::cue::{Markup, Rgb, Ruby, Run, RunStyle};
use crate::effects::{Clip, Effects, Fade, Move, Syllable, Targets, Transform};

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
    /// What ASS adds to the text (SUB-019), in file coordinates.
    pub effects: Effects,
    /// Where the next syllable of karaoke starts.
    karaoke_ms: i64,
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
        let mut effects = self.effects;
        if let Some(last) = effects.karaoke.last_mut() {
            last.range.end = self.text.len();
        }
        effects.karaoke = std::mem::take(&mut effects.karaoke)
            .into_iter()
            .filter_map(|s| clip(&s.range).map(|range| Syllable { range, ..s }))
            .collect();
        effects.movement = effects.movement.map(|m| Move {
            from: to_fraction(m.from),
            to: to_fraction(m.to),
            ..m
        });
        effects.origin = effects.origin.map(&to_fraction);
        effects.clip = effects.clip.map(|c| Clip {
            min: to_fraction(c.min),
            max: to_fraction(c.max),
            ..c
        });
        let markup = Markup {
            effects: (effects != Effects::default()).then(|| Box::new(effects)),
            runs,
            ruby,
            anchor: self.anchor,
            pos: self.pos.map(&to_fraction),
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
/// `\alpha&H80&`, as opacity.
fn opacity(value: &str) -> Option<f32> {
    let hex = value
        .trim()
        .trim_matches('&')
        .trim_start_matches(['H', 'h']);
    let hex = hex.trim_end_matches('&');
    u8::from_str_radix(hex, 16)
        .ok()
        .map(|a| 1.0 - f32::from(a) / 255.0)
}

/// The rectangle of `\clip`: its corners, or around a drawing's points.
fn clip_of(args: &str, inverse: bool) -> Option<Clip> {
    let inner = args.trim_start_matches('(').trim_end_matches(')');
    let numbers = number_args(args);
    if let [x1, y1, x2, y2] = numbers[..] {
        return Some(Clip {
            min: (x1.min(x2), y1.min(y2)),
            max: (x1.max(x2), y1.max(y2)),
            inverse,
        });
    }
    let drawing = inner.rsplit(',').next()?;
    let points: Vec<f64> = drawing
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for [x, y] in points.as_chunks::<2>().0 {
        min = (min.0.min(*x), min.1.min(*y));
        max = (max.0.max(*x), max.1.max(*y));
    }
    (min.0 <= max.0).then_some(Clip { min, max, inverse })
}

/// What the tags of a `\t` change.
fn targets(tags: &str) -> Targets {
    let mut to = Targets::default();
    let number = |s: &str| s.parse::<f32>().ok();
    for tag in split_tags(tags) {
        if let Some(v) = tag.strip_prefix("frx") {
            to.rotation[0] = number(v);
        } else if let Some(v) = tag.strip_prefix("fry") {
            to.rotation[1] = number(v);
        } else if let Some(v) = tag.strip_prefix("frz").or_else(|| tag.strip_prefix("fr")) {
            to.rotation[2] = number(v);
        } else if let Some(v) = tag.strip_prefix("fscx") {
            to.scale[0] = number(v).map(|n| n / 100.0);
        } else if let Some(v) = tag.strip_prefix("fscy") {
            to.scale[1] = number(v).map(|n| n / 100.0);
        } else if let Some(v) = tag.strip_prefix("fs") {
            to.size = number(v);
        } else if let Some(v) = tag.strip_prefix("alpha").or_else(|| tag.strip_prefix("1a")) {
            to.opacity = opacity(v);
        } else if let Some(v) = tag.strip_prefix("1c").or_else(|| tag.strip_prefix('c')) {
            to.color = ass_color(v);
        }
    }
    to
}

/// `\t(…)`: its times and acceleration, then its tags.
fn transform(args: &str) -> Option<Transform> {
    let inner = args.strip_prefix('(')?.strip_suffix(')').unwrap_or(args);
    let at = inner.find('\\')?;
    let head = number_args(&inner[..at]);
    let (time, accel) = match head[..] {
        [] => (None, 1.0),
        [accel] => (None, accel),
        [t1, t2] => (Some((t1 as i64, t2 as i64)), 1.0),
        [t1, t2, accel, ..] => (Some((t1 as i64, t2 as i64)), accel),
    };
    Some(Transform {
        time,
        accel: accel as f32,
        to: targets(&inner[at..]),
    })
}

/// A tag of [`Effects`]; whether it was one.
fn effect_tag(tag: &str, b: &mut TextBuilder) -> bool {
    let number = |s: &str| s.parse::<f32>().ok();
    let scaled = |s: &str| number(s).map(|n| n / 100.0);
    let start = b.text.len();
    let e = &mut b.effects;
    if let Some(args) = tag.strip_prefix("fade(") {
        if let [a1, a2, a3, t1, t2, t3, t4] = number_args(args)[..] {
            let level = |a: f64| 1.0 - a as f32 / 255.0;
            e.fade.get_or_insert(Fade::Full(
                [level(a1), level(a2), level(a3)],
                [t1 as i64, t2 as i64, t3 as i64, t4 as i64],
            ));
        }
    } else if let Some(args) = tag.strip_prefix("fad(") {
        if let [fade_in, fade_out] = number_args(args)[..] {
            e.fade
                .get_or_insert(Fade::Simple(fade_in as i64, fade_out as i64));
        }
    } else if let Some(args) = tag.strip_prefix("move") {
        let n = number_args(args);
        if let [x1, y1, x2, y2, ..] = n[..] {
            let time = match n[4..] {
                [t1, t2] if t1 != 0.0 || t2 != 0.0 => Some((t1 as i64, t2 as i64)),
                _ => None,
            };
            e.movement.get_or_insert(Move {
                from: (x1, y1),
                to: (x2, y2),
                time,
            });
        }
    } else if let Some(args) = tag.strip_prefix("org") {
        if let [x, y] = number_args(args)[..] {
            e.origin.get_or_insert((x, y));
        }
    } else if let Some(args) = tag.strip_prefix("iclip") {
        e.clip = e.clip.or(clip_of(args, true));
    } else if let Some(args) = tag.strip_prefix("clip") {
        e.clip = e.clip.or(clip_of(args, false));
    } else if let Some(args) = tag.strip_prefix("t(") {
        e.transforms.extend(transform(&format!("({args}")));
    } else if let Some(v) = tag.strip_prefix("frx") {
        e.rotation[0] = number(v).unwrap_or(0.0);
    } else if let Some(v) = tag.strip_prefix("fry") {
        e.rotation[1] = number(v).unwrap_or(0.0);
    } else if let Some(v) = tag.strip_prefix("frz") {
        e.rotation[2] = number(v).unwrap_or(0.0);
    } else if let Some(v) = tag.strip_prefix("fscx") {
        e.scale[0] = scaled(v).unwrap_or(1.0);
    } else if let Some(v) = tag.strip_prefix("fscy") {
        e.scale[1] = scaled(v).unwrap_or(1.0);
    } else if let Some(v) = tag.strip_prefix("alpha").or_else(|| tag.strip_prefix("1a")) {
        e.opacity = opacity(v).unwrap_or(1.0);
    } else if let Some(v) = tag.strip_prefix("2c") {
        e.unsung = ass_color(v);
    } else if let Some((v, sweep)) = tag
        .strip_prefix("kf")
        .map(|v| (v, true))
        .or_else(|| tag.strip_prefix("ko").map(|v| (v, false)))
        .or_else(|| tag.strip_prefix('K').map(|v| (v, true)))
        .or_else(|| tag.strip_prefix('k').map(|v| (v, false)))
        && let Ok(centiseconds) = v.parse::<i64>()
    {
        if let Some(last) = e.karaoke.last_mut() {
            last.range.end = start;
        }
        let begin = b.karaoke_ms;
        b.karaoke_ms += centiseconds * 10;
        b.effects.karaoke.push(Syllable {
            range: start..start,
            time: (begin, b.karaoke_ms),
            sweep,
        });
    } else if let Some(v) = tag.strip_prefix("fr")
        && v.parse::<f32>().is_ok()
    {
        e.rotation[2] = number(v).unwrap_or(0.0);
    } else {
        return false;
    }
    true
}

pub(crate) fn apply_override(block: &str, b: &mut TextBuilder) {
    for tag in split_tags(block) {
        if effect_tag(tag, b) {
            continue;
        }
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

#[cfg(test)]
mod effect_tests {
    use super::*;

    fn cue(text: &str) -> (String, Markup) {
        let mut b = TextBuilder::default();
        push_ass_text(text, &mut b, false);
        b.finish(|(x, y)| (x / 100.0, y / 100.0))
    }

    fn effects(text: &str) -> Effects {
        *cue(text).1.effects.expect("effects")
    }

    // T-SUB-019
    #[test]
    fn the_tags_are_read() {
        assert_eq!(
            effects(r"{\fad(100,200)}a").fade,
            Some(Fade::Simple(100, 200))
        );
        assert_eq!(
            effects(r"{\fade(255,0,128,0,10,20,30)}a").fade,
            Some(Fade::Full([0.0, 1.0, 1.0 - 128.0 / 255.0], [0, 10, 20, 30]))
        );
        assert_eq!(
            effects(r"{\move(10,20,30,40,5,6)}a").movement,
            Some(Move {
                from: (0.1, 0.2),
                to: (0.3, 0.4),
                time: Some((5, 6)),
            })
        );
        assert_eq!(
            effects(r"{\move(10,20,30,40)}a").movement.unwrap().time,
            None
        );
        assert_eq!(effects(r"{\org(50,50)}a").origin, Some((0.5, 0.5)));
        let e = effects(r"{\frz45\frx1\fry2\fscx150\fscy50\alpha&H80&}a");
        assert_eq!(e.rotation, [1.0, 2.0, 45.0]);
        assert_eq!(e.scale, [1.5, 0.5]);
        assert!((e.opacity - (1.0 - 128.0 / 255.0)).abs() < 1e-6);
        assert_eq!(effects(r"{\fr30}a").rotation[2], 30.0);
    }

    // T-SUB-019
    #[test]
    fn a_clip_is_a_rectangle() {
        let c = effects(r"{\clip(10,20,30,40)}a").clip.unwrap();
        assert_eq!((c.min, c.max, c.inverse), ((0.1, 0.2), (0.3, 0.4), false));
        let c = effects(r"{\iclip(30,40,10,20)}a").clip.unwrap();
        assert_eq!((c.min, c.max, c.inverse), ((0.1, 0.2), (0.3, 0.4), true));
        // a drawing comes to the rectangle around it
        let c = effects(r"{\clip(m 10 20 l 30 20 30 40)}a").clip.unwrap();
        assert_eq!((c.min, c.max), ((0.1, 0.2), (0.3, 0.4)));
    }

    // T-SUB-019
    #[test]
    fn a_transform_keeps_its_times_and_tags() {
        let t =
            &effects(r"{\t(100,200,2,\frz90\fscx200\c&H0000FF&\fs40\alpha&HFF&)}a").transforms[0];
        assert_eq!((t.time, t.accel), (Some((100, 200)), 2.0));
        assert_eq!(t.to.rotation[2], Some(90.0));
        assert_eq!(t.to.scale[0], Some(2.0));
        assert_eq!(t.to.size, Some(40.0));
        assert_eq!(t.to.color, Some(Rgb(255, 0, 0)));
        assert_eq!(t.to.opacity, Some(0.0));
        let t = &effects(r"{\t(\frz90)}a").transforms[0];
        assert_eq!((t.time, t.accel), (None, 1.0));
    }

    // T-SUB-019: every character stays in `plain_text`
    #[test]
    fn karaoke_splits_the_text_in_syllables() {
        let (text, markup) = cue(r"{\k10}か{\kf20}ら{\K30}お{\ko5\2c&H00FF00&}け");
        assert_eq!(text, "からおけ");
        let e = markup.effects.unwrap();
        let times: Vec<_> = e
            .karaoke
            .iter()
            .map(|s| (s.range.clone(), s.time, s.sweep))
            .collect();
        assert_eq!(
            times,
            [
                (0..3, (0, 100), false),
                (3..6, (100, 300), true),
                (6..9, (300, 600), true),
                (9..12, (600, 650), false),
            ]
        );
        assert_eq!(e.unsung, Some(Rgb(0, 255, 0)));
    }

    // T-SUB-019: what was read before still is
    #[test]
    fn other_tags_are_not_taken_for_effects() {
        let (text, markup) = cue(r"{\b1\c&H0000FF&\fs30\an8}a");
        assert_eq!(text, "a");
        assert_eq!(markup.effects, None);
        assert_eq!(markup.anchor, Some(8));
        assert_eq!(markup.runs[0].style.color, Some(Rgb(255, 0, 0)));
    }

    #[test]
    fn broken_tags_are_only_ignored() {
        for text in [
            r"{\move(1,2)}a",
            r"{\t()}a",
            r"{\clip()}a",
            r"{\fade(1)}a",
            r"{\k}a",
            r"{\t(}a",
        ] {
            assert_eq!(cue(text).0, "a", "{text}");
        }
    }
}
