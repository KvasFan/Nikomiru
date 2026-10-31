// SPDX-License-Identifier: GPL-3.0-only

//! The subtitles of the player page, apart from their drawing: the loaded track, the cues
//! active at the position, what the overlay and the list panel draw, and the
//! blur/hide state (SUB-008…SUB-016). mpv draws no text subtitle; graphic
//! tracks (PGS, VobSub) are left to mpv and have no lookup (SPECIFICATION §6.4).

use std::path::PathBuf;

use nikomiru_subs::effects::Frame;
use nikomiru_subs::timeline::Timeline;
use nikomiru_subs::{AssStyle, Cue, Format, Track};

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
    /// For the secondary subtitle (SUB-017): it does not cancel the main
    /// one's load, nor is cancelled by it.
    pub secondary: bool,
}

/// An alignment of the cues with the audio (SUB-018): the audio stream to
/// listen to, and the cues without their offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignRequest {
    /// The video, or the file of an external audio track.
    pub input: PathBuf,
    /// The audio stream's ff-index in it; `None`: the first.
    pub stream: Option<i64>,
    /// The cues' start and end, in milliseconds, without the offset.
    pub cues: Vec<(i64, i64)>,
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

    /// Name for the test harness (`state.subtitles.obscure`).
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

/// An active cue as the overlay draws it (SUB-009, SUB-010).
#[derive(Debug, Clone)]
pub struct Shown<'a> {
    /// Index of the cue.
    pub index: usize,
    /// The cue.
    pub cue: &'a Cue,
    /// Its ASS style, only when ASS styles are respected.
    pub style: Option<&'a AssStyle>,
    /// Numpad anchor 1–9: `\an`, or the style's alignment when respected.
    pub anchor: Option<u8>,
    /// The bytes to highlight: the text the popup matched, or the clicked
    /// grapheme while the lookup runs (POP-002).
    pub highlight: Option<std::ops::Range<usize>>,
    /// Its ASS effects at this moment, only when ASS styles are respected
    /// (SUB-019), and the ms since it started.
    pub frame: Option<(Frame, i64)>,
}

/// The subtitles of the current file.
#[derive(Debug, Default)]
pub struct Subtitles {
    timeline: Timeline,
    /// The video's position at the last update.
    pos_ms: i64,
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
    /// Subtitles turned off with V: nothing is drawn, not even on hover, and
    /// the chosen track stays for when they are turned on again.
    pub disabled: bool,
    active: Vec<usize>,
    /// The selected grapheme: cue and byte offset.
    pub selection: Option<(usize, usize)>,
    /// The text the open popup matched (POP-002): cue and byte range.
    pub highlight: Option<(usize, std::ops::Range<usize>)>,
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
        self.highlight = None;
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
        self.highlight = None;
    }

    /// Updates the active cues; returns the cues that just appeared.
    pub fn update(&mut self, pos_ms: i64) -> Vec<usize> {
        self.pos_ms = pos_ms;
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

    /// The active cues in drawing order (layer, then file order), with what
    /// the overlay needs to draw them; none while turned off with V (SUB-014).
    /// `respect_ass` adds the ASS style of each cue (SUB-010).
    pub fn shown(&self, respect_ass: bool) -> impl Iterator<Item = Shown<'_>> + '_ {
        let active: &[usize] = if self.disabled { &[] } else { &self.active };
        active.iter().filter_map(move |&index| {
            let cue = self.cue(index)?;
            let style = respect_ass.then(|| self.style_of(cue)).flatten();
            let elapsed = self.pos_ms - self.timeline.delay_ms - cue.start_ms;
            let frame = cue
                .markup
                .effects
                .as_ref()
                .filter(|_| respect_ass)
                .map(|e| (e.at(elapsed, cue.end_ms - cue.start_ms), elapsed));
            Some(Shown {
                index,
                cue,
                style,
                anchor: cue.markup.anchor.or(style.map(|s| s.alignment)),
                highlight: self.highlight_range(index),
                frame,
            })
        })
    }

    /// The bytes of cue `i` to highlight: the matched text, or the clicked
    /// grapheme while the lookup runs.
    fn highlight_range(&self, i: usize) -> Option<std::ops::Range<usize>> {
        match (&self.highlight, self.selection) {
            (Some((c, range)), _) if *c == i => Some(range.clone()),
            (_, Some((c, o))) if c == i => Some(o..o + 1),
            _ => None,
        }
    }

    /// The play area of ASS files (PlayResY scales sizes and positions).
    pub fn play_res(&self) -> Option<(u32, u32)> {
        self.play_res
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

/// The byte offset in `text` of the character at index `pos` (egui's text
/// cursors count characters).
pub fn char_to_byte(text: &str, pos: usize) -> usize {
    text.char_indices()
        .nth(pos)
        .map_or(text.len(), |(byte, _)| byte)
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
        let shown: Vec<Shown<'_>> = s.shown(false).collect();
        assert_eq!(
            (shown[0].index, shown[0].highlight.clone()),
            (0, Some(3..4))
        );
        let mut graphemes = shown[0].cue.graphemes().into_iter();
        let first = graphemes.next().unwrap();
        assert_eq!(
            (first.text, first.offset, first.style.and_then(|s| s.bold)),
            ("食", 0, Some(true))
        );
        let second_line = graphemes.skip_while(|g| g.text != "\n").nth(1).unwrap();
        assert_eq!(second_line.text, "二");
        assert_eq!(shown[1].anchor, Some(8));
        // V: nothing is drawn, the cues stay active for when it comes back
        s.disabled = true;
        assert_eq!(s.shown(false).count(), 0);
        assert_eq!(s.active(), [0, 1]);
        s.disabled = false;
        // leaving the cue drops the selection
        s.update(3500);
        assert_eq!(s.selection, None);
        assert_eq!(s.timeline().start_pos(1), 2000);
    }

    // T-SUB-010
    #[test]
    fn ass_styles_only_when_respected() {
        let ass = "[Script Info]\nPlayResY: 1080\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Sign,Serif,48,&H0000D7FF,&H0,&H0,&H0,-1,0,0,0,100,100,0,0,1,2,0,8,10,10,30,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Sign,,0,0,0,,{\\c&HFF0000&}青{\\c}看板\n";
        let mut s = Subtitles::default();
        s.set_track(Source::File("a.ass".into()), loaded(ass, Format::Ass));
        s.update(1500);
        // default style: no ASS style or alignment (the overlay also leaves out
        // the inline colour then, view::overlay's tests)
        let plain: Vec<Shown<'_>> = s.shown(false).collect();
        assert!(plain[0].style.is_none());
        assert_eq!(plain[0].anchor, None);
        // respected: the style's font, size, colour and top alignment, and inline colours
        let styled: Vec<Shown<'_>> = s.shown(true).collect();
        assert_eq!(styled[0].anchor, Some(8));
        let style = styled[0].style.unwrap();
        assert_eq!(
            (
                style.font.as_str(),
                style.size,
                style.primary.hex(),
                style.margin_v
            ),
            ("Serif", 48.0, "#FFD700".to_string(), 30)
        );
        let first = styled[0].cue.graphemes()[0].clone();
        assert_eq!(
            first.style.and_then(|s| s.color).map(|c| c.hex()),
            Some("#0000FF".to_string())
        );
        assert_eq!(s.play_res(), Some((1440, 1080)));
    }

    #[test]
    fn clocks_and_character_positions() {
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
        let text = "a😀日本";
        assert_eq!(char_to_byte(text, 0), 0);
        assert_eq!(char_to_byte(text, 1), 1);
        assert_eq!(char_to_byte(text, 2), 5);
        assert_eq!(char_to_byte(text, 3), 8);
        assert_eq!(char_to_byte(text, 99), text.len());
    }
}
