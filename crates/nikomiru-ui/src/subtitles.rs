// SPDX-License-Identifier: GPL-3.0-only

//! The subtitles of the player page, without Qt: the loaded track, the cues
//! active at the position, what the overlay and the list panel draw, and the
//! blur/hide state (SUB-008…SUB-016). mpv draws no text subtitle; graphic
//! tracks (PGS, VobSub) are left to mpv and have no lookup (SPECIFICATION §6.4).

use std::path::PathBuf;

use nikomiru_subs::timeline::Timeline;
use nikomiru_subs::{AssStyle, Cue, Format, Track};
use serde_json::{Value, json};

/// Default font size: 36 px for a video 1080 px high (SUB-010).
pub const DEFAULT_FONT_SIZE: f64 = 36.0;

/// Where the subtitles come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A file (next to the video, or opened by the user).
    File(PathBuf),
    /// An embedded text track: mpv id, FFmpeg index and codec.
    Embedded {
        /// mpv track id.
        id: i64,
        /// FFmpeg stream index.
        ff_index: i64,
        /// Codec name.
        codec: String,
    },
    /// An embedded graphic track, drawn by mpv.
    Bitmap(i64),
}

impl Source {
    /// Identifier used by the menu.
    pub fn key(&self) -> String {
        match self {
            Source::File(p) => format!("file:{}", p.display()),
            Source::Embedded { id, .. } | Source::Bitmap(id) => format!("track:{id}"),
        }
    }
}

/// What the background loader is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The video.
    pub video: PathBuf,
    /// The subtitles to load (a file or an embedded text track).
    pub source: Source,
    /// An encoding forced by the user (SUB-005).
    pub encoding: Option<String>,
    /// Video duration, for the extraction timeout (EXT-003).
    pub duration_ms: i64,
}

/// A loaded track: the parsed file and the encoding that was detected.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The parsed track.
    pub track: Track,
    /// The encoding (files only).
    pub encoding: Option<String>,
}

/// Obscuring of the main subtitle (SUB-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Obscure {
    /// Visible.
    #[default]
    None,
    /// Blurred.
    Blur,
    /// Hidden.
    Hidden,
}

impl Obscure {
    /// From the `subtitles.obscure` setting.
    pub fn from_config(name: &str) -> Self {
        match name {
            "blur" => Obscure::Blur,
            "hidden" => Obscure::Hidden,
            _ => Obscure::None,
        }
    }

    /// Name for the QML layer.
    pub fn name(self) -> &'static str {
        match self {
            Obscure::None => "none",
            Obscure::Blur => "blur",
            Obscure::Hidden => "hidden",
        }
    }
}

/// The blur strength for a font size (SUB-014).
pub fn blur_sigma(font_size: f64) -> f64 {
    (0.45 * font_size).max(12.0)
}

/// What the main subtitle looks like now: the mode applies only during
/// playback, with no popup open and without the mouse over it (SUB-014,
/// BUG-2198, BUG-2235).
pub fn obscured(mode: Obscure, playing: bool, popup_open: bool, hovered: bool) -> Obscure {
    if !playing || popup_open || hovered {
        Obscure::None
    } else {
        mode
    }
}

/// The subtitles of the current file.
#[derive(Debug, Default)]
pub struct Subtitles {
    timeline: Timeline,
    format: Option<Format>,
    styles: Vec<AssStyle>,
    play_res: Option<(u32, u32)>,
    /// What is shown.
    pub source: Option<Source>,
    /// The detected or forced encoding (files).
    pub encoding: Option<String>,
    /// An encoding forced by the user, applied to the next load.
    pub forced_encoding: Option<String>,
    /// The user's obscuring mode.
    pub mode: Obscure,
    /// The mouse is over the subtitles.
    pub hovered: bool,
    active: Vec<usize>,
    /// The selected grapheme: cue and byte offset (M3 looks it up).
    pub selection: Option<(usize, usize)>,
}

impl Subtitles {
    /// No subtitles yet, with the user's obscuring mode.
    pub fn with_mode(mode: Obscure) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    /// The timeline (cues and offset).
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// The timeline, to move the offset.
    pub fn timeline_mut(&mut self) -> &mut Timeline {
        &mut self.timeline
    }

    /// Replaces the track; the offset is kept (SUB-011).
    pub fn set_track(&mut self, source: Source, loaded: Loaded) {
        let delay = self.timeline.delay_ms;
        self.format = Some(loaded.track.format);
        self.styles = loaded.track.styles;
        self.play_res = loaded.track.play_res;
        self.timeline = Timeline::new(loaded.track.cues);
        self.timeline.delay_ms = delay;
        self.source = Some(source);
        self.encoding = loaded.encoding;
        self.active.clear();
        self.selection = None;
    }

    /// No text subtitles (none chosen, or a graphic track drawn by mpv).
    pub fn clear(&mut self, source: Option<Source>) {
        self.timeline = Timeline::default();
        self.timeline.delay_ms = 0;
        self.format = None;
        self.styles.clear();
        self.play_res = None;
        self.source = source;
        self.encoding = None;
        self.active.clear();
        self.selection = None;
    }

    /// Updates the active cues; returns the cues that just appeared.
    pub fn update(&mut self, pos_ms: i64) -> Vec<usize> {
        let active = self.timeline.active(pos_ms);
        if active == self.active {
            return Vec::new();
        }
        let shown = active
            .iter()
            .copied()
            .filter(|i| !self.active.contains(i))
            .collect();
        if self
            .selection
            .is_some_and(|(cue, _)| !active.contains(&cue))
        {
            self.selection = None;
        }
        self.active = active;
        shown
    }

    /// The active cues.
    pub fn active(&self) -> &[usize] {
        &self.active
    }

    /// The cue `i`.
    pub fn cue(&self, i: usize) -> Option<&Cue> {
        self.timeline.cues().get(i)
    }

    fn style_of(&self, cue: &Cue) -> Option<&AssStyle> {
        let name = cue.style.as_deref()?;
        self.styles
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// What the overlay draws (SUB-009, SUB-010): every active cue in drawing
    /// order, with its placement and its graphemes. `respect_ass` adds the ASS
    /// style of the cue.
    pub fn overlay(&self, respect_ass: bool) -> Value {
        let cues: Vec<Value> = self
            .active
            .iter()
            .filter_map(|&i| self.cue(i).map(|c| (i, c)))
            .map(|(i, cue)| {
                let style = respect_ass.then(|| self.style_of(cue)).flatten();
                let anchor = cue.markup.anchor.or(style.map(|s| s.alignment));
                let mut lines: Vec<Vec<Value>> = vec![Vec::new()];
                for g in cue.graphemes() {
                    if g.text == "\n" {
                        lines.push(Vec::new());
                        continue;
                    }
                    let mut item = json!({ "t": g.text, "o": g.offset });
                    if let Some(s) = g.style {
                        let m = item.as_object_mut();
                        if let Some(m) = m {
                            if let Some(b) = s.bold {
                                m.insert("b".into(), b.into());
                            }
                            if let Some(v) = s.italic {
                                m.insert("i".into(), v.into());
                            }
                            if let Some(v) = s.underline {
                                m.insert("u".into(), v.into());
                            }
                            if let Some(v) = s.strike {
                                m.insert("s".into(), v.into());
                            }
                            if respect_ass {
                                if let Some(c) = s.color {
                                    m.insert("c".into(), c.hex().into());
                                }
                                if let Some(f) = &s.font {
                                    m.insert("fn".into(), f.clone().into());
                                }
                                if let Some(sz) = s.size {
                                    m.insert("fs".into(), f64::from(sz).into());
                                }
                            }
                        }
                    }
                    if let Some((reading, covered)) = g.ruby
                        && let Some(m) = item.as_object_mut()
                    {
                        m.insert("ruby".into(), reading.into());
                        m.insert("rubyN".into(), covered.into());
                    }
                    if let Some(last) = lines.last_mut() {
                        last.push(item);
                    }
                }
                json!({
                    "cue": i,
                    "layer": cue.layer,
                    "anchor": anchor,
                    "pos": cue.markup.pos.map(|(x, y)| json!([x, y])),
                    "vtt": cue.markup.vtt.as_ref().map(|v| json!({
                        "line": v.line.map(|l| match l {
                            nikomiru_subs::cue::VttLine::Percent(p) => json!({ "percent": p }),
                            nikomiru_subs::cue::VttLine::Number(n) => json!({ "number": n }),
                        }),
                        "position": v.position,
                        "align": v.align.map(|a| format!("{a:?}").to_lowercase()),
                    })),
                    "style": style.map(|s| json!({
                        "font": s.font,
                        "size": s.size,
                        "color": s.primary.hex(),
                        "outline": s.outline,
                        "outline_color": s.outline_color.hex(),
                        "bold": s.bold,
                        "italic": s.italic,
                        "margin_v": s.margin_v,
                    })),
                    "selected": self.selection.and_then(|(c, o)| (c == i).then_some(o)),
                    "lines": lines,
                })
            })
            .collect();
        Value::Array(cues)
    }

    /// The play area of ASS files (PlayResY scales sizes and positions).
    pub fn play_res(&self) -> Option<(u32, u32)> {
        self.play_res
    }

    /// Every cue for the list panel (SUB-015): start in milliseconds (with the
    /// offset) and text.
    pub fn list(&self) -> Value {
        Value::Array(
            self.timeline
                .cues()
                .iter()
                .enumerate()
                .map(|(i, c)| json!({ "i": i, "start": self.timeline.start_pos(i), "text": c.plain_text }))
                .collect(),
        )
    }
}

/// `m:ss` or `h:mm:ss`.
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The byte offset in `text` of the UTF-16 position `pos` (QML text positions).
pub fn utf16_to_byte(text: &str, pos: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= pos {
            return byte;
        }
        units += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-014
    #[test]
    fn blur_strength_and_when_it_applies() {
        assert_eq!(blur_sigma(20.0), 12.0);
        assert_eq!(blur_sigma(40.0), 18.0);
        assert_eq!(obscured(Obscure::Blur, true, false, false), Obscure::Blur);
        assert_eq!(
            obscured(Obscure::Blur, false, false, false),
            Obscure::None,
            "paused"
        );
        assert_eq!(
            obscured(Obscure::Blur, true, true, false),
            Obscure::None,
            "popup open"
        );
        assert_eq!(
            obscured(Obscure::Hidden, true, false, true),
            Obscure::None,
            "hovered"
        );
        assert_eq!(
            obscured(Obscure::Hidden, true, false, false),
            Obscure::Hidden
        );
    }

    fn loaded(text: &str, format: Format) -> Loaded {
        Loaded {
            track: nikomiru_subs::parse(format, text),
            encoding: Some("UTF-8".into()),
        }
    }

    #[test]
    fn active_cues_selection_and_overlay() {
        let mut s = Subtitles::default();
        s.set_track(
            Source::File("a.srt".into()),
            loaded(
                "1\n00:00:01,000 --> 00:00:03,000\n<b>食べ</b>ました\n二行目\n\n2\n00:00:02,000 --> 00:00:04,000\n{\\an8}上\n",
                Format::Srt,
            ),
        );
        assert_eq!(s.update(1500), [0]);
        assert_eq!(s.update(1600), Vec::<usize>::new(), "no change");
        assert_eq!(s.update(2500), [1]);
        s.selection = Some((0, 3));
        let overlay = s.overlay(false);
        let first = &overlay[0];
        assert_eq!(first["cue"], 0);
        assert_eq!(first["selected"], 3);
        assert_eq!(
            first["lines"][0][0],
            json!({ "t": "食", "o": 0, "b": true })
        );
        assert_eq!(first["lines"][1][0]["t"], "二");
        assert_eq!(overlay[1]["anchor"], 8);
        // leaving the cue drops the selection
        s.update(3500);
        assert_eq!(s.selection, None);
        assert_eq!(s.list()[1]["start"], 2000);
    }

    #[test]
    fn clocks_and_utf16_positions() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
        let text = "a😀日本";
        assert_eq!(utf16_to_byte(text, 0), 0);
        assert_eq!(utf16_to_byte(text, 1), 1);
        assert_eq!(utf16_to_byte(text, 3), 5);
        assert_eq!(utf16_to_byte(text, 4), 8);
        assert_eq!(utf16_to_byte(text, 99), text.len());
    }
}
