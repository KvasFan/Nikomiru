// SPDX-License-Identifier: GPL-3.0-only

//! The style subset of DIC-035: the properties a dictionary may set on its
//! structured content, from inline `style` objects and from its `styles.css`.
//! Everything else is dropped. Values are checked and typed: the popup only
//! receives colours, lengths and a few keywords.

use serde::{Serialize, Serializer};

/// A length, in `em` of the current font or in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "u", content = "v", rename_all = "lowercase")]
pub enum Length {
    /// Relative to the font size.
    Em(f32),
    /// Pixels.
    Px(f32),
}

impl Length {
    /// In pixels, with `em` pixels to the em.
    pub fn px(self, em: f32) -> f32 {
        match self {
            Length::Em(v) => v * em,
            Length::Px(v) => v,
        }
    }
}

/// A checked colour: red, green, blue and alpha, not premultiplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha: 0 is transparent.
    pub a: u8,
}

impl Rgba {
    /// An opaque colour from `0xrrggbb`.
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
            a: 255,
        }
    }
}

/// As CSS writes it: `#rrggbb`, or `#rrggbbaa` when not opaque.
impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Rgba { r, g, b, a } = *self;
        let text = if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        };
        serializer.serialize_str(&text)
    }
}

/// `vertical-align` of text: smaller, raised or lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlign {
    /// Subscript.
    Sub,
    /// Superscript (`super`, `top`, `text-top`).
    Super,
}

/// `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    /// `left`, `start`.
    Left,
    /// `right`, `end`.
    Right,
    /// `center`.
    Center,
    /// `justify`.
    Justify,
}

/// A list marker (`list-style-type`), or a string of the dictionary's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListMarker {
    /// No marker.
    None,
    /// •
    Disc,
    /// ◦
    Circle,
    /// ▪
    Square,
    /// 1. 2. 3.
    Decimal,
    /// a. b. c.
    LowerAlpha,
    /// A. B. C.
    UpperAlpha,
    /// i. ii. iii.
    LowerRoman,
    /// I. II. III.
    UpperRoman,
    /// 一、二、三、
    CjkIdeographic,
    /// A quoted string (Jitendex's ①, ②…), without the quotes.
    Text(String),
}

impl ListMarker {
    /// What item `index` (from 0) shows.
    pub fn label(&self, index: usize) -> String {
        let n = index + 1;
        match self {
            ListMarker::None => String::new(),
            ListMarker::Disc => "•".into(),
            ListMarker::Circle => "◦".into(),
            ListMarker::Square => "▪".into(),
            ListMarker::Decimal => format!("{n}."),
            ListMarker::LowerAlpha => format!("{}.", alphabetic(n)),
            ListMarker::UpperAlpha => format!("{}.", alphabetic(n).to_uppercase()),
            ListMarker::LowerRoman => format!("{}.", roman(n)),
            ListMarker::UpperRoman => format!("{}.", roman(n).to_uppercase()),
            ListMarker::CjkIdeographic => format!("{}、", ideographic(n)),
            ListMarker::Text(text) => text.clone(),
        }
    }
}

/// a … z, aa, ab …
fn alphabetic(mut n: usize) -> String {
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'a' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// i, ii, iv … up to 3999; larger numbers in digits.
fn roman(mut n: usize) -> String {
    if !(1..4000).contains(&n) {
        return n.to_string();
    }
    let mut out = String::new();
    for (value, letters) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while n >= value {
            out.push_str(letters);
            n -= value;
        }
    }
    out
}

/// 一 … 九十九; larger numbers in digits.
fn ideographic(n: usize) -> String {
    const DIGITS: [&str; 10] = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    match n {
        1..=9 => DIGITS[n].into(),
        10..=99 => {
            let tens = if n >= 20 { DIGITS[n / 10] } else { "" };
            format!("{tens}十{}", DIGITS[n % 10])
        }
        _ => n.to_string(),
    }
}

/// The four sides of a margin or padding.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Sides {
    /// Top.
    pub top: Option<Length>,
    /// Right.
    pub right: Option<Length>,
    /// Bottom.
    pub bottom: Option<Length>,
    /// Left.
    pub left: Option<Length>,
}

/// Allowed style properties. `None` = not set (inherited for text properties).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Style {
    /// Text colour (see [`color`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgba>,
    /// Background colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgba>,
    /// Font size relative to the parent's (1.0 = same).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_scale: Option<f32>,
    /// Bold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Underlined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Struck through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Subscript or superscript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<VerticalAlign>,
    /// Horizontal alignment of the lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    /// Margins.
    #[serde(skip_serializing_if = "is_unset")]
    pub margin: Sides,
    /// Padding.
    #[serde(skip_serializing_if = "is_unset")]
    pub padding: Sides,
    /// Border colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Rgba>,
    /// Border width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<Length>,
    /// Corner radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<Length>,
    /// Sides with a border: top, right, bottom, left (all when not set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_sides: Option<[bool; 4]>,
    /// List marker (`ol`/`ul`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_marker: Option<ListMarker>,
}

fn is_unset(s: &Sides) -> bool {
    *s == Sides::default()
}

/// The CSS named colours (CSS Color 4), with their values.
const COLOR_NAMES: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// The numbers of `rgb()`, `rgba()`, `hsl()` or `hsla()`, with whether each
/// is a percentage: `(1, 2, 3)`, `(1 2 3 / 0.5)`.
fn color_function(v: &str, names: &[&str]) -> Option<Vec<(f32, bool)>> {
    let args = names
        .iter()
        .find_map(|n| v.strip_prefix(n)?.strip_prefix('('))?
        .strip_suffix(')')?;
    let parts: Vec<(f32, bool)> = args
        .split([',', ' ', '/'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (number, percent) = match p.strip_suffix('%') {
                Some(n) => (n, true),
                None => (p.strip_suffix("deg").unwrap_or(p), false),
            };
            number
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .map(|n| (n, percent))
        })
        .collect::<Option<_>>()?;
    (3..=4).contains(&parts.len()).then_some(parts)
}

/// The alpha of a colour function: a fraction or a percentage.
fn alpha((value, percent): (f32, bool)) -> u8 {
    let fraction = if percent { value / 100.0 } else { value };
    (fraction.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// HSL (hue in degrees, saturation and lightness 0–1) to RGB 0–1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let k = |n: f32| (n + h / 30.0).rem_euclid(12.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f32| l - a * (k(n) - 3.0).min(9.0 - k(n)).clamp(-1.0, 1.0);
    [f(0.0), f(8.0), f(4.0)]
}

/// A checked colour: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()`
/// and `hsl()`/`hsla()` with numbers, or a CSS colour name. Anything else
/// (`url()`, `var()`, `currentColor`…) is refused.
pub fn color(value: &str) -> Option<Rgba> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok();
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let short = |i: usize| digit(i).map(|d| d * 17);
        return Some(match hex.len() {
            3 | 4 => Rgba {
                r: short(0)?,
                g: short(1)?,
                b: short(2)?,
                a: if hex.len() == 4 { short(3)? } else { 255 },
            },
            6 | 8 => Rgba {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: if hex.len() == 8 { pair(6)? } else { 255 },
            },
            _ => return None,
        });
    }
    if let Some(parts) = color_function(&v, &["rgba", "rgb"]) {
        let channel = |(n, percent): (f32, bool)| {
            let n = if percent { n * 2.55 } else { n };
            n.clamp(0.0, 255.0).round() as u8
        };
        return Some(Rgba {
            r: channel(parts[0]),
            g: channel(parts[1]),
            b: channel(parts[2]),
            a: parts.get(3).copied().map_or(255, alpha),
        });
    }
    if let Some(parts) = color_function(&v, &["hsla", "hsl"]) {
        let fraction = |(n, _): (f32, bool)| (n / 100.0).clamp(0.0, 1.0);
        let [r, g, b] = hsl_to_rgb(parts[0].0, fraction(parts[1]), fraction(parts[2]));
        let channel = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
        return Some(Rgba {
            r: channel(r),
            g: channel(g),
            b: channel(b),
            a: parts.get(3).copied().map_or(255, alpha),
        });
    }
    if v == "transparent" {
        return Some(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }
    COLOR_NAMES
        .binary_search_by(|(name, _)| name.cmp(&v.as_str()))
        .ok()
        .map(|i| Rgba::hex(COLOR_NAMES[i].1))
}

/// A length: a number of `em`/`rem`/`px`/`pt`, or a bare number (`em`, as
/// Yomitan's numeric margins). Negative and absurd values are refused.
pub fn length(value: &str) -> Option<Length> {
    let v = value.trim().to_ascii_lowercase();
    let (number, unit) = match v.find(|c: char| c.is_ascii_alphabetic() || c == '%') {
        Some(i) => v.split_at(i),
        None => (v.as_str(), "em"),
    };
    let n: f32 = number.trim().parse().ok()?;
    if !(0.0..=100.0).contains(&n) && unit != "px" || !(0.0..=1000.0).contains(&n) {
        return None;
    }
    match unit {
        "em" | "rem" => Some(Length::Em(n)),
        "px" => Some(Length::Px(n)),
        "pt" => Some(Length::Px(n * 4.0 / 3.0)),
        _ => None,
    }
}

/// A font size as a factor of the parent's: `%`, `em`, keywords.
fn font_scale(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    let scale = match v.as_str() {
        "smaller" | "small" => 0.85,
        "x-small" => 0.7,
        "xx-small" => 0.6,
        "larger" | "large" => 1.2,
        "x-large" => 1.5,
        "xx-large" => 2.0,
        "medium" => 1.0,
        _ => {
            if let Some(p) = v.strip_suffix('%') {
                p.trim().parse::<f32>().ok()? / 100.0
            } else {
                match length(&v)? {
                    Length::Em(e) => e,
                    // a pixel size against a 16 px default
                    Length::Px(px) => px / 16.0,
                }
            }
        }
    };
    (0.3..=4.0).contains(&scale).then_some(scale)
}

fn sides(value: &str) -> Option<[Length; 4]> {
    let parts: Vec<Length> = value
        .split_whitespace()
        .map(length)
        .collect::<Option<_>>()?;
    Some(match parts.as_slice() {
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return None,
    })
}

impl Style {
    /// Sets a property from its CSS name (`font-weight`) or its Yomitan name
    /// (`fontWeight`); returns whether it is allowed and valid.
    pub fn set(&mut self, property: &str, value: &str) -> bool {
        let name: String = property
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['-', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        let v = value.trim().trim_end_matches("!important").trim();
        let lower = v.to_ascii_lowercase();
        let side = |s: &mut Sides, which: &str| -> bool {
            let Some(l) = length(v) else { return false };
            match which {
                "top" => s.top = Some(l),
                "right" => s.right = Some(l),
                "bottom" => s.bottom = Some(l),
                "left" => s.left = Some(l),
                _ => return false,
            }
            true
        };
        let all = |s: &mut Sides| -> bool {
            let Some([t, r, b, l]) = sides(v) else {
                return false;
            };
            *s = Sides {
                top: Some(t),
                right: Some(r),
                bottom: Some(b),
                left: Some(l),
            };
            true
        };
        match name.as_str() {
            "color" => color(v).map(|c| self.color = Some(c)).is_some(),
            "background-color" | "background" => {
                color(v).map(|c| self.background = Some(c)).is_some()
            }
            "font-size" => font_scale(v).map(|s| self.font_scale = Some(s)).is_some(),
            "font-weight" => {
                let bold = match lower.as_str() {
                    "bold" | "bolder" => true,
                    "normal" | "lighter" => false,
                    n => match n.parse::<u32>() {
                        Ok(w) => w >= 600,
                        Err(_) => return false,
                    },
                };
                self.bold = Some(bold);
                true
            }
            "font-style" => {
                let italic = match lower.as_str() {
                    "italic" | "oblique" => true,
                    "normal" => false,
                    _ => return false,
                };
                self.italic = Some(italic);
                true
            }
            "text-decoration" | "text-decoration-line" => {
                self.underline = Some(lower.contains("underline"));
                self.strike = Some(lower.contains("line-through"));
                true
            }
            "vertical-align" => {
                self.vertical_align = Some(match lower.as_str() {
                    "sub" => VerticalAlign::Sub,
                    "super" | "top" | "text-top" => VerticalAlign::Super,
                    _ => return false,
                });
                true
            }
            "text-align" => {
                self.text_align = Some(match lower.as_str() {
                    "left" | "start" => TextAlign::Left,
                    "right" | "end" => TextAlign::Right,
                    "center" => TextAlign::Center,
                    "justify" => TextAlign::Justify,
                    _ => return false,
                });
                true
            }
            "margin" => all(&mut self.margin),
            "padding" => all(&mut self.padding),
            m if m.starts_with("margin-") => side(&mut self.margin, &m["margin-".len()..]),
            p if p.starts_with("padding-") => side(&mut self.padding, &p["padding-".len()..]),
            "border-color" => color(v).map(|c| self.border_color = Some(c)).is_some(),
            "border-width" => length(v).map(|l| self.border_width = Some(l)).is_some(),
            "border-radius" => length(v.split_whitespace().next().unwrap_or_default())
                .map(|l| self.border_radius = Some(l))
                .is_some(),
            "border-style" => {
                // `none none none solid`: the sides that have a border
                let sides: Vec<bool> = v
                    .split_whitespace()
                    .map(|p| !matches!(p.to_ascii_lowercase().as_str(), "none" | "hidden"))
                    .collect();
                let sides = match sides.as_slice() {
                    [a] => [*a; 4],
                    [a, b] => [*a, *b, *a, *b],
                    [a, b, c] => [*a, *b, *c, *b],
                    [a, b, c, d] => [*a, *b, *c, *d],
                    _ => return false,
                };
                self.border_sides = Some(sides);
                true
            }
            side @ ("border-top" | "border-right" | "border-bottom" | "border-left") => {
                // `border-left: 3px solid green`: that side only
                let index = ["border-top", "border-right", "border-bottom", "border-left"]
                    .iter()
                    .position(|n| *n == side)
                    .unwrap_or(3);
                let mut sides = self.border_sides.unwrap_or([false; 4]);
                sides[index] = !v.split_whitespace().any(|p| p == "none" || p == "hidden");
                self.border_sides = Some(sides);
                for part in v.split_whitespace() {
                    if let Some(l) =
                        length(part).filter(|_| part.chars().any(|c| c.is_ascii_digit()))
                    {
                        self.border_width = Some(l);
                    } else if let Some(c) = color(part) {
                        self.border_color = Some(c);
                    }
                }
                true
            }
            "border" => {
                // `border: 1px solid #ccc`: a width and a colour, the style is
                // always solid
                let mut any = false;
                for part in v.split_whitespace() {
                    if let Some(l) =
                        length(part).filter(|_| part.chars().any(|c| c.is_ascii_digit()))
                    {
                        self.border_width = Some(l);
                        any = true;
                    } else if let Some(c) = color(part) {
                        self.border_color = Some(c);
                        any = true;
                    } else if part == "none" {
                        self.border_width = Some(Length::Px(0.0));
                        any = true;
                    }
                }
                any
            }
            "list-style-type" | "list-style" => {
                let marker = if let Some(text) = v
                    .strip_prefix('"')
                    .and_then(|t| t.strip_suffix('"'))
                    .or_else(|| v.strip_prefix('\'')?.strip_suffix('\''))
                {
                    // at most a few characters: it is drawn as text
                    if text.chars().count() > 6 || text.chars().any(char::is_control) {
                        return false;
                    }
                    ListMarker::Text(text.to_string())
                } else {
                    match lower.as_str() {
                        "none" => ListMarker::None,
                        "disc" => ListMarker::Disc,
                        "circle" => ListMarker::Circle,
                        "square" => ListMarker::Square,
                        "decimal" => ListMarker::Decimal,
                        "lower-alpha" | "lower-latin" => ListMarker::LowerAlpha,
                        "upper-alpha" | "upper-latin" => ListMarker::UpperAlpha,
                        "lower-roman" => ListMarker::LowerRoman,
                        "upper-roman" => ListMarker::UpperRoman,
                        "cjk-ideographic" | "cjk-decimal" => ListMarker::CjkIdeographic,
                        _ => return false,
                    }
                };
                self.list_marker = Some(marker);
                true
            }
            _ => false,
        }
    }

    /// `other` on top of `self` (later declarations win).
    pub fn merge(&mut self, other: &Style) {
        macro_rules! take {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f.clone_from(&other.$f); } )* };
        }
        take!(
            color,
            background,
            font_scale,
            bold,
            italic,
            underline,
            strike,
            vertical_align,
            text_align,
            border_color,
            border_width,
            border_radius,
            border_sides,
            list_marker
        );
        for (mine, theirs) in [
            (&mut self.margin, &other.margin),
            (&mut self.padding, &other.padding),
        ] {
            mine.top = theirs.top.or(mine.top);
            mine.right = theirs.right.or(mine.right);
            mine.bottom = theirs.bottom.or(mine.bottom);
            mine.left = theirs.left.or(mine.left);
        }
    }

    /// The text style of an element whose parent's text style is `self`: the
    /// element's own style on top, relative font sizes multiplied.
    pub fn with_own(&self, own: &Style) -> Style {
        let mut style = self.clone();
        style.merge(own);
        if let (Some(parent), Some(mine)) = (self.font_scale, own.font_scale) {
            style.font_scale = Some((parent * mine).clamp(0.3, 4.0));
        }
        style
    }

    /// The part children inherit: text properties only.
    pub fn inherited(&self) -> Style {
        Style {
            color: self.color,
            font_scale: self.font_scale,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            text_align: self.text_align,
            ..Style::default()
        }
    }

    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Option<Rgba> {
        Some(Rgba { r, g, b, a })
    }

    #[test]
    fn colours_are_checked() {
        assert_eq!(color("#FFF"), rgba(255, 255, 255, 255));
        assert_eq!(color("#f008"), rgba(255, 0, 0, 0x88));
        assert_eq!(color("#123456"), rgba(0x12, 0x34, 0x56, 255));
        assert_eq!(color("#ff000080"), rgba(255, 0, 0, 0x80));
        assert_eq!(color("rgb(255, 0, 0)"), rgba(255, 0, 0, 255));
        assert_eq!(color("rgba(0 0 0 / 0.5)"), rgba(0, 0, 0, 128));
        assert_eq!(color("rgb(50%, 100%, 0%)"), rgba(128, 255, 0, 255));
        assert_eq!(color("rgba(0, 0, 0, 25%)"), rgba(0, 0, 0, 64));
        assert_eq!(color("hsl(120, 100%, 25%)"), rgba(0, 128, 0, 255));
        assert_eq!(color("hsla(0deg 100% 50% / 0.5)"), rgba(255, 0, 0, 128));
        assert_eq!(color("Red"), rgba(255, 0, 0, 255));
        assert_eq!(color("darkSlateGray"), rgba(0x2f, 0x4f, 0x4f, 255));
        assert_eq!(color("transparent"), rgba(0, 0, 0, 0));
        for bad in [
            "url(http://x)",
            "#12345",
            "#ggg",
            "expression(alert(1))",
            "var(--c)",
            "rgb(1,2)",
            "rgb(1,2,3,4,5)",
            "currentcolor",
            "notacolour",
        ] {
            assert_eq!(color(bad), None, "{bad}");
        }
        assert!(
            COLOR_NAMES.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted, for the binary search"
        );
    }

    #[test]
    fn list_markers_count_in_their_own_system() {
        let label = |m: ListMarker, i: usize| m.label(i);
        assert_eq!(label(ListMarker::Decimal, 0), "1.");
        assert_eq!(label(ListMarker::LowerAlpha, 27), "ab.");
        assert_eq!(label(ListMarker::UpperAlpha, 2), "C.");
        assert_eq!(label(ListMarker::LowerRoman, 3), "iv.");
        assert_eq!(label(ListMarker::UpperRoman, 1993), "MCMXCIV.");
        assert_eq!(label(ListMarker::CjkIdeographic, 0), "一、");
        assert_eq!(label(ListMarker::CjkIdeographic, 9), "十、");
        assert_eq!(label(ListMarker::CjkIdeographic, 20), "二十一、");
        assert_eq!(label(ListMarker::Circle, 5), "◦");
        assert_eq!(label(ListMarker::None, 5), "");
        let mut s = Style::default();
        assert!(s.set("listStyleType", "'①'"));
        assert_eq!(s.list_marker, Some(ListMarker::Text("①".into())));
        assert!(!s.set("listStyleType", "\"a very long marker\""));
        assert!(!s.set("listStyleType", "hebrew"));
    }

    #[test]
    fn lengths_and_sizes() {
        assert_eq!(length("0.5em"), Some(Length::Em(0.5)));
        assert_eq!(length("3"), Some(Length::Em(3.0)));
        assert_eq!(length("12px"), Some(Length::Px(12.0)));
        assert_eq!(length("-1em"), None);
        assert_eq!(length("calc(1em + 2px)"), None);
        assert_eq!(font_scale("80%"), Some(0.8));
        assert_eq!(font_scale("small"), Some(0.85));
        assert_eq!(font_scale("1000%"), None);
    }

    // T-POP-009 (style subset)
    #[test]
    fn only_allowed_properties_are_kept() {
        let mut s = Style::default();
        assert!(s.set("fontWeight", "bold"));
        assert!(s.set("margin-left", "1em"));
        assert!(s.set("padding", "0.1em 0.3em"));
        assert!(s.set("border", "1px solid #ccc"));
        assert!(s.set("listStyleType", "\"◦\""));
        assert!(!s.set("position", "absolute"));
        assert!(!s.set("background-image", "url(http://evil/x.png)"));
        assert!(!s.set("background", "url(http://evil/x.png)"));
        assert!(!s.set("behavior", "url(x.htc)"));
        assert!(!s.set("cursor", "pointer"));
        assert_eq!(s.bold, Some(true));
        assert_eq!(s.margin.left, Some(Length::Em(1.0)));
        assert_eq!(s.padding.right, Some(Length::Em(0.3)));
        assert_eq!(s.border_color, rgba(0xcc, 0xcc, 0xcc, 255));
        assert_eq!(s.list_marker, Some(ListMarker::Text("◦".into())));
        assert!(s.set("border-style", "none none none solid"));
        assert_eq!(s.border_sides, Some([false, false, false, true]));
        let mut l = Style::default();
        assert!(l.set("borderLeft", "3px solid green"));
        assert_eq!(
            (l.border_sides, l.border_color),
            (Some([false, false, false, true]), rgba(0, 128, 0, 255))
        );
        // an unknown font style changes nothing
        let mut italic = Style::default();
        assert!(italic.set("fontStyle", "italic"));
        assert!(!italic.set("fontStyle", "sideways"));
        assert_eq!(italic.italic, Some(true));
        let inherited = s.inherited();
        assert_eq!(
            (inherited.bold, inherited.margin),
            (Some(true), Sides::default())
        );
    }

    #[test]
    fn relative_font_sizes_multiply_down_the_tree() {
        let mut parent = Style::default();
        assert!(parent.set("fontSize", "150%"));
        let mut own = Style::default();
        assert!(own.set("fontSize", "0.5em"));
        let child = parent.inherited().with_own(&own);
        assert_eq!(child.font_scale, Some(0.75));
        // a block's children keep the size
        assert_eq!(child.inherited().font_scale, Some(0.75));
    }
}
