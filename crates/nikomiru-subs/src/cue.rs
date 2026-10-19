// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle model (SUB-004): every cue has its times, the `plain_text` used
//! for lookup, mining and counts, and a separate [`Markup`] used only for
//! drawing. Markup ranges are byte ranges of `plain_text`.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// Subtitle file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// SubRip.
    Srt,
    /// WebVTT.
    Vtt,
    /// Advanced SubStation Alpha (and SSA).
    Ass,
}

impl Format {
    /// From a file extension (`srt`, `vtt`, `ass`, `ssa`), any case.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "srt" => Some(Format::Srt),
            "vtt" => Some(Format::Vtt),
            "ass" | "ssa" => Some(Format::Ass),
            _ => None,
        }
    }
}

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#RRGGBB`.
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

/// The style of a run of text. `None` = inherited (from the ASS style of the
/// cue, or the user's default style).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunStyle {
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// Underline.
    pub underline: Option<bool>,
    /// Strike-out.
    pub strike: Option<bool>,
    /// Text colour.
    pub color: Option<Rgb>,
    /// Font family.
    pub font: Option<String>,
    /// Font size, in the units of the file (ASS: PlayResY pixels).
    pub size: Option<f32>,
}

impl RunStyle {
    /// Whether nothing is set.
    pub fn is_plain(&self) -> bool {
        *self == RunStyle::default()
    }
}

/// A styled byte range of `plain_text`.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Byte range in `plain_text`.
    pub range: Range<usize>,
    /// Its style.
    pub style: RunStyle,
}

/// A reading over a byte range of `plain_text` (WebVTT `<ruby>`/`<rt>`). The
/// reading is not part of `plain_text`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ruby {
    /// The base text, as a byte range of `plain_text`.
    pub range: Range<usize>,
    /// The reading.
    pub text: String,
}

/// Horizontal alignment of WebVTT cue text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    /// `start`.
    Start,
    /// `center`.
    Center,
    /// `end`.
    End,
    /// `left`.
    Left,
    /// `right`.
    Right,
}

/// WebVTT `line` setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VttLine {
    /// A percentage of the video height (0–100).
    Percent(f64),
    /// A line number (negative counts from the bottom).
    Number(i32),
}

/// WebVTT cue settings (SUB-002).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VttSettings {
    /// `line`.
    pub line: Option<VttLine>,
    /// `position`, percent of the video width.
    pub position: Option<f64>,
    /// `size`, percent of the video width.
    pub size: Option<f64>,
    /// `align`.
    pub align: Option<TextAlign>,
}

/// What only drawing needs (SUB-004).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markup {
    /// Styled runs, sorted and not overlapping; unstyled text has no run.
    pub runs: Vec<Run>,
    /// Readings.
    pub ruby: Vec<Ruby>,
    /// Numpad anchor 1–9 (ASS `\an`, also `{\an8}` in SRT).
    pub anchor: Option<u8>,
    /// Position as fractions of the play area (ASS `\pos` / PlayRes).
    pub pos: Option<(f64, f64)>,
    /// WebVTT settings.
    pub vtt: Option<VttSettings>,
}

/// A subtitle cue.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// Start, in milliseconds.
    pub start_ms: i64,
    /// End, in milliseconds (`start_ms <= end_ms`).
    pub end_ms: i64,
    /// The text without any markup; lines are separated by `\n`.
    pub plain_text: String,
    /// What drawing needs.
    pub markup: Markup,
    /// ASS style name.
    pub style: Option<String>,
    /// ASS layer (0 for the other formats).
    pub layer: i32,
    /// Position in the file, for stable ordering.
    pub order: usize,
}

/// A grapheme of a cue, as the overlay draws it (SUB-009).
#[derive(Debug, Clone, PartialEq)]
pub struct Grapheme<'a> {
    /// The grapheme (`"\n"` for a line break).
    pub text: &'a str,
    /// Byte offset in `plain_text`.
    pub offset: usize,
    /// The run style that covers it.
    pub style: Option<&'a RunStyle>,
    /// A reading that starts at this grapheme, with the number of graphemes it
    /// covers.
    pub ruby: Option<(&'a str, usize)>,
}

impl Cue {
    /// Whether the cue time `t` (milliseconds, already corrected by the delay)
    /// falls inside the cue.
    pub fn contains(&self, t: i64) -> bool {
        self.start_ms <= t && t < self.end_ms
    }

    /// The graphemes of `plain_text`, each with its offset, style and ruby.
    pub fn graphemes(&self) -> Vec<Grapheme<'_>> {
        let indices: Vec<(usize, &str)> = self.plain_text.grapheme_indices(true).collect();
        indices
            .iter()
            .map(|&(offset, text)| {
                let style = self
                    .markup
                    .runs
                    .iter()
                    .find(|r| r.range.contains(&offset))
                    .map(|r| &r.style);
                let ruby = self
                    .markup
                    .ruby
                    .iter()
                    .find(|r| r.range.start == offset)
                    .map(|r| {
                        let covered = indices.iter().filter(|(o, _)| r.range.contains(o)).count();
                        (r.text.as_str(), covered)
                    });
                Grapheme {
                    text,
                    offset,
                    style,
                    ruby,
                }
            })
            .collect()
    }
}

/// An ASS style (`[V4+ Styles]`), used when "respect ASS styles" is on (SUB-010).
#[derive(Debug, Clone, PartialEq)]
pub struct AssStyle {
    /// Name.
    pub name: String,
    /// Font family.
    pub font: String,
    /// Font size in PlayResY pixels.
    pub size: f32,
    /// Primary (fill) colour.
    pub primary: Rgb,
    /// Outline colour.
    pub outline_color: Rgb,
    /// Shadow/back colour.
    pub back_color: Rgb,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Strike-out.
    pub strike: bool,
    /// Outline width in PlayResY pixels.
    pub outline: f32,
    /// Shadow depth in PlayResY pixels.
    pub shadow: f32,
    /// Numpad alignment 1–9.
    pub alignment: u8,
    /// Vertical margin in PlayResY pixels.
    pub margin_v: i32,
}

/// A parsed subtitle file.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The format.
    pub format: Format,
    /// The cues, sorted by start (equal starts keep the file order).
    pub cues: Vec<Cue>,
    /// ASS styles.
    pub styles: Vec<AssStyle>,
    /// ASS PlayResX/PlayResY (default 384×288); `None` for SRT and WebVTT.
    pub play_res: Option<(u32, u32)>,
}

impl Track {
    /// The ASS style called `name`.
    pub fn style(&self, name: &str) -> Option<&AssStyle> {
        self.styles
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Sorts the cues by start, keeping the file order for equal starts.
    pub(crate) fn sort(cues: &mut [Cue]) {
        cues.sort_by_key(|c| (c.start_ms, c.order));
    }
}
