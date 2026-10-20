// SPDX-License-Identifier: GPL-3.0-only

//! The popup content made by Nikomiru's `popup_view` and
//! `nikomiru_dict::structured` (the JSON the Qt popup draws), read back for
//! the iced and egui popups, and the StyledText subset that Rust writes for
//! plain paragraphs.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Content {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub expression: String,
    pub furigana: Vec<Furigana>,
    #[serde(default)]
    pub trace: Vec<Trace>,
    #[serde(default)]
    pub term_tags: Vec<Tag>,
    #[serde(default)]
    pub frequencies: Vec<Frequency>,
    pub blocks: Vec<Section>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Furigana {
    pub t: String,
    pub r: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trace {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Frequency {
    pub dictionary: String,
    pub values: Vec<Value>,
}

/// One dictionary's glossaries for an entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub dictionary: String,
    pub expanded: bool,
    pub glossaries: Vec<Glossary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Glossary {
    pub tags: Vec<Tag>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
pub struct Length {
    pub u: Unit,
    pub v: f32,
}

/// `em` or `px`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Em,
    Px,
}

impl Length {
    pub fn px(self, base: f32) -> f32 {
        match self.u {
            Unit::Em => self.v * base,
            Unit::Px => self.v,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
pub struct Sides {
    pub top: Option<Length>,
    pub right: Option<Length>,
    pub bottom: Option<Length>,
    pub left: Option<Length>,
}

impl Sides {
    /// Top, right, bottom, left in pixels.
    pub fn px(&self, base: f32) -> [f32; 4] {
        [self.top, self.right, self.bottom, self.left].map(|l| l.map_or(0.0, |l| l.px(base)))
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Style {
    pub color: Option<String>,
    pub background: Option<String>,
    pub font_scale: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub vertical_align: Option<String>,
    #[serde(default)]
    pub margin: Sides,
    #[serde(default)]
    pub padding: Sides,
    pub border_color: Option<String>,
    pub border_width: Option<Length>,
    pub border_radius: Option<Length>,
    pub border_sides: Option<[bool; 4]>,
    pub list_marker: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", content = "to", rename_all = "lowercase")]
pub enum Target {
    Lookup(String),
    External(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Run {
    Text {
        t: String,
        #[serde(default)]
        s: Style,
    },
    Ruby {
        t: String,
        rt: String,
        #[serde(default)]
        s: Style,
    },
    Link {
        t: String,
        target: Target,
        #[serde(default)]
        s: Style,
    },
    Image {
        image: Value,
    },
    Br,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub row: u32,
    pub col: u32,
    pub header: bool,
    pub colspan: u32,
    pub rowspan: u32,
    #[serde(default)]
    pub s: Style,
    pub natural_em: f32,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Block {
    Text {
        html: String,
        #[serde(default)]
        links: Vec<Target>,
    },
    Flow {
        runs: Vec<Run>,
    },
    Box {
        #[serde(default)]
        s: Style,
        blocks: Vec<Block>,
    },
    List {
        ordered: bool,
        #[serde(default)]
        s: Style,
        items: Vec<Vec<Block>>,
        #[serde(default)]
        markers: Vec<Option<String>>,
    },
    Table {
        #[serde(default)]
        s: Style,
        columns: u32,
        rows: Vec<Vec<Cell>>,
    },
    Details {
        open: bool,
        summary: Vec<Run>,
        blocks: Vec<Block>,
    },
    Pairs {
        items: Vec<(String, Vec<String>)>,
    },
}

/// The popups to measure: one per line of `path`, each a popups array as
/// Nikomiru's `popups_json` writes it (content as an object or a string).
pub fn load(path: &str) -> Result<Vec<(String, Content)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut out = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let popups: Vec<Value> = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let Some(p) = popups.first() else { continue };
        let content = match &p["content"] {
            Value::String(s) => serde_json::from_str(s),
            other => serde_json::from_value(other.clone()),
        }
        .map_err(|e| format!("content: {e}"))?;
        let query = p["query"].as_str().unwrap_or_default().to_string();
        out.push((query, content));
    }
    Ok(out)
}

/// An RGBA colour.
pub type Rgba = [u8; 4];

/// A colour as `Style` writes it (`#rgb`, `#rrggbb`, `#aarrggbb` or a name).
pub fn color(s: &str) -> Option<Rgba> {
    if let Some(hex) = s.strip_prefix('#') {
        let n = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
        return match hex.len() {
            3 => Some([n(0, 1)? * 17, n(1, 1)? * 17, n(2, 1)? * 17, 255]),
            6 => Some([n(0, 2)?, n(2, 2)?, n(4, 2)?, 255]),
            8 => Some([n(2, 2)?, n(4, 2)?, n(6, 2)?, n(0, 2)?]),
            _ => None,
        };
    }
    Some(match s {
        "black" => [0, 0, 0, 255],
        "white" => [255, 255, 255, 255],
        "red" => [255, 0, 0, 255],
        "green" => [0, 128, 0, 255],
        "blue" => [0, 0, 255, 255],
        "gray" | "grey" => [128, 128, 128, 255],
        "silver" | "lightgray" | "lightgrey" => [192, 192, 192, 255],
        "dimgray" | "dimgrey" | "darkgray" | "darkgrey" => [105, 105, 105, 255],
        "purple" => [128, 0, 128, 255],
        "brown" => [165, 42, 42, 255],
        "goldenrod" => [218, 165, 32, 255],
        "orange" => [255, 165, 0, 255],
        "yellow" => [255, 255, 0, 255],
        "navy" => [0, 0, 128, 255],
        "teal" => [0, 128, 128, 255],
        "maroon" => [128, 0, 0, 255],
        "olive" => [128, 128, 0, 255],
        "lime" => [0, 255, 0, 255],
        "transparent" => [0, 0, 0, 0],
        _ => return None,
    })
}

/// A piece of a StyledText paragraph.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Span {
        text: String,
        color: Option<Rgba>,
        scale: f32,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        link: Option<usize>,
    },
    Break,
}

/// The StyledText that `nikomiru_dict::structured` writes: `b`, `i`, `u`,
/// `s`, `font color/size`, `br`, `a href="N"` and the entities it escapes.
pub fn styled_text(html: &str) -> Vec<Piece> {
    const SIZES: [f32; 7] = [0.7, 0.8, 1.0, 1.2, 1.5, 2.0, 2.4];
    #[derive(Clone, Default)]
    struct State {
        color: Option<Rgba>,
        scale: Option<f32>,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        link: Option<usize>,
    }
    let mut stack: Vec<(String, State)> = Vec::new();
    let mut st = State::default();
    let mut out = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, st: &State, out: &mut Vec<Piece>| {
        if !text.is_empty() {
            out.push(Piece::Span {
                text: std::mem::take(text),
                color: st.color,
                scale: st.scale.unwrap_or(1.0),
                bold: st.bold,
                italic: st.italic,
                underline: st.underline,
                strike: st.strike,
                link: st.link,
            });
        }
    };
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(tag_rest) = rest.strip_prefix('<') {
            let end = tag_rest.find('>').unwrap_or(tag_rest.len());
            let tag = &tag_rest[..end];
            rest = tag_rest.get(end + 1..).unwrap_or("");
            flush(&mut text, &st, &mut out);
            if tag == "br" {
                out.push(Piece::Break);
            } else if let Some(name) = tag.strip_prefix('/') {
                while let Some((open, saved)) = stack.pop() {
                    st = saved;
                    if open == name {
                        break;
                    }
                }
            } else {
                let name = tag.split(' ').next().unwrap_or_default().to_string();
                stack.push((name.clone(), st.clone()));
                let attr = |key: &str| {
                    let k = format!("{key}=\"");
                    let i = tag.find(&k)? + k.len();
                    Some(&tag[i..i + tag[i..].find('"')?])
                };
                match name.as_str() {
                    "b" => st.bold = true,
                    "i" => st.italic = true,
                    "u" => st.underline = true,
                    "s" => st.strike = true,
                    "a" => st.link = attr("href").and_then(|h| h.parse().ok()),
                    "font" => {
                        if let Some(c) = attr("color").and_then(color) {
                            st.color = Some(c);
                        }
                        if let Some(n) = attr("size").and_then(|n| n.parse::<usize>().ok()) {
                            st.scale = SIZES.get(n.wrapping_sub(1)).copied();
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(entity_rest) = rest.strip_prefix('&') {
            let end = entity_rest.find(';').unwrap_or(0);
            let c = match &entity_rest[..end] {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "nbsp" => '\u{a0}',
                _ => '&',
            };
            text.push(c);
            rest = if c == '&' && end == 0 {
                entity_rest
            } else {
                &entity_rest[end + 1..]
            };
        } else {
            let next = rest.find(['<', '&']).unwrap_or(rest.len());
            text.push_str(&rest[..next]);
            rest = &rest[next..];
        }
    }
    flush(&mut text, &st, &mut out);
    out
}

/// The marker of list item `index`: its own, the list's, or the default.
pub fn marker(own: Option<&str>, list: Option<&str>, ordered: bool, index: usize) -> String {
    let m = own.or(list).unwrap_or("");
    if let Some(quoted) = m.strip_prefix('"').and_then(|m| m.strip_suffix('"')) {
        return quoted.to_string();
    }
    match m {
        "none" => String::new(),
        "circle" => "◦".into(),
        "square" => "▪".into(),
        "disc" => "•".into(),
        "decimal" => format!("{}.", index + 1),
        _ if ordered => format!("{}.", index + 1),
        _ => "•".into(),
    }
}

/// The colour of a tag chip, by category (as `Chip.qml`).
pub fn chip_color(category: &str) -> Rgba {
    match category {
        "popular" => [0x5a, 0x3b, 0x1f, 255],
        "partOfSpeech" => [0x2a, 0x3f, 0x5f, 255],
        "frequent" => [0x3d, 0x5a, 0x2a, 255],
        "archaism" | "dialect" => [0x4a, 0x3a, 0x5a, 255],
        _ => [0x3a, 0x3d, 0x44, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_text_pieces() {
        let p = styled_text(
            r##"<font color="#777" size="2"><b>1 &lt; 2</b></font>&nbsp;<a href="0">来る</a><br>x"##,
        );
        assert_eq!(p.len(), 5, "{p:?}");
        assert!(
            matches!(&p[0], Piece::Span { text, bold: true, scale, color: Some([0x77, 0x77, 0x77, 255]), .. } if text == "1 < 2" && *scale == 0.8)
        );
        assert!(matches!(&p[2], Piece::Span { text, link: Some(0), .. } if text == "来る"));
        assert_eq!(p[3], Piece::Break);
    }
}
