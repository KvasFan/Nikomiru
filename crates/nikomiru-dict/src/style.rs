// SPDX-License-Identifier: GPL-3.0-only

//! The style subset of DIC-035: the properties a dictionary may set on its
//! structured content, from inline `style` objects and from its `styles.css`.
//! Everything else is dropped. Values are checked, so QML only receives
//! colours, numbers and a few keywords.

use serde::Serialize;

/// A length, in `em` of the current font or in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "u", content = "v", rename_all = "lowercase")]
pub enum Length {
    /// Relative to the font size.
    Em(f32),
    /// Pixels.
    Px(f32),
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
    /// Text colour (`#rrggbb`, `#rrggbbaa` or a CSS colour name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Background colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
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
    /// `sub` or `super`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<&'static str>,
    /// `left`, `right`, `center` or `justify`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<&'static str>,
    /// Margins.
    #[serde(skip_serializing_if = "is_unset")]
    pub margin: Sides,
    /// Padding.
    #[serde(skip_serializing_if = "is_unset")]
    pub padding: Sides,
    /// Border colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    /// Border width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<Length>,
    /// Corner radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<Length>,
    /// Sides with a border: top, right, bottom, left (all when not set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_sides: Option<[bool; 4]>,
    /// List marker (`ol`/`ul`): `none`, `disc`, `decimal`…, or a quoted string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_marker: Option<String>,
}

fn is_unset(s: &Sides) -> bool {
    *s == Sides::default()
}

/// CSS colour names QML understands too (SVG colour keywords), a common subset.
const COLOR_NAMES: &[&str] = &[
    "black",
    "white",
    "red",
    "green",
    "blue",
    "gray",
    "grey",
    "silver",
    "maroon",
    "purple",
    "fuchsia",
    "lime",
    "olive",
    "yellow",
    "navy",
    "teal",
    "aqua",
    "orange",
    "brown",
    "pink",
    "gold",
    "darkred",
    "darkgreen",
    "darkblue",
    "darkgray",
    "darkgrey",
    "lightgray",
    "lightgrey",
    "dimgray",
    "dimgrey",
    "transparent",
];

/// A checked colour: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()`
/// with numbers, or a known name.
pub fn color(value: &str) -> Option<String> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        return (matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .then_some(v);
    }
    if let Some(args) = v
        .strip_prefix("rgba(")
        .or_else(|| v.strip_prefix("rgb("))
        .and_then(|a| a.strip_suffix(')'))
    {
        let parts: Vec<f32> = args
            .split([',', ' ', '/'])
            .filter(|p| !p.is_empty())
            .map(|p| p.trim_end_matches('%').parse::<f32>())
            .collect::<Result<_, _>>()
            .ok()?;
        if !(3..=4).contains(&parts.len()) {
            return None;
        }
        let channel = |x: f32| x.clamp(0.0, 255.0).round() as u8;
        let alpha = parts
            .get(3)
            .map_or(255, |a| (a.clamp(0.0, 1.0) * 255.0).round() as u8);
        return Some(format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            channel(parts[0]),
            channel(parts[1]),
            channel(parts[2]),
            alpha
        ));
    }
    COLOR_NAMES.contains(&v.as_str()).then_some(v)
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
                self.italic = Some(matches!(lower.as_str(), "italic" | "oblique"));
                matches!(lower.as_str(), "italic" | "oblique" | "normal")
            }
            "text-decoration" | "text-decoration-line" => {
                self.underline = Some(lower.contains("underline"));
                self.strike = Some(lower.contains("line-through"));
                true
            }
            "vertical-align" => match lower.as_str() {
                "sub" => {
                    self.vertical_align = Some("sub");
                    true
                }
                "super" | "top" | "text-top" => {
                    self.vertical_align = Some("super");
                    true
                }
                _ => false,
            },
            "text-align" => {
                let align = match lower.as_str() {
                    "left" | "start" => "left",
                    "right" | "end" => "right",
                    "center" => "center",
                    "justify" => "justify",
                    _ => return false,
                };
                self.text_align = Some(align);
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
                let marker = if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    v.to_string()
                } else {
                    match lower.as_str() {
                        "none" | "disc" | "circle" | "square" | "decimal" | "lower-alpha"
                        | "upper-alpha" | "lower-roman" | "upper-roman" | "cjk-ideographic" => {
                            lower
                        }
                        _ => return false,
                    }
                };
                // at most a few characters: it is drawn as text
                if marker.chars().count() > 8 {
                    return false;
                }
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

    /// The part children inherit: text properties only.
    pub fn inherited(&self) -> Style {
        Style {
            color: self.color.clone(),
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

    #[test]
    fn colours_are_checked() {
        assert_eq!(color("#FFF").as_deref(), Some("#fff"));
        assert_eq!(color("rgb(255, 0, 0)").as_deref(), Some("#ff0000ff"));
        assert_eq!(color("rgba(0 0 0 / 0.5)").as_deref(), Some("#00000080"));
        assert_eq!(color("Red").as_deref(), Some("red"));
        for bad in [
            "url(http://x)",
            "#12345",
            "expression(alert(1))",
            "var(--c)",
            "rgb(1,2)",
        ] {
            assert_eq!(color(bad), None, "{bad}");
        }
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
        assert_eq!(s.border_color.as_deref(), Some("#ccc"));
        assert_eq!(s.list_marker.as_deref(), Some("\"◦\""));
        assert!(s.set("border-style", "none none none solid"));
        assert_eq!(s.border_sides, Some([false, false, false, true]));
        let mut l = Style::default();
        assert!(l.set("borderLeft", "3px solid green"));
        assert_eq!(
            (l.border_sides, l.border_color.as_deref()),
            (Some([false, false, false, true]), Some("green"))
        );
        let inherited = s.inherited();
        assert_eq!(
            (inherited.bold, inherited.margin),
            (Some(true), Sides::default())
        );
    }
}
