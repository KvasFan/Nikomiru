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
    /// An embedded PGS or VobSub track read as text (SUB-028, SUB-029):
    /// mpv draws it until its text is ready.
    Read {
        /// mpv track id.
        id: i64,
        /// FFmpeg stream index.
        ff_index: i64,
        /// Codec name.
        codec: String,
    },
}

impl Source {
    /// A local model reads this embedded or explicit paired-file source.
    pub fn is_graphic_read(&self) -> bool {
        matches!(self, Self::Read { .. })
            || matches!(self, Self::File(path) if path.extension().is_some_and(|s| matches!(s.to_string_lossy().to_ascii_lowercase().as_str(), "idx" | "sub")))
    }

    /// Identifier used by the menu.
    pub fn key(&self) -> String {
        match self {
            Source::File(p) => format!("file:{}", p.display()),
            Source::Embedded { id, .. } | Source::Bitmap(id) => format!("track:{id}"),
            Source::Read { id, .. } => format!("read:{id}"),
        }
    }
}

/// What the background loader is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Captured playback position, without subtitle delay, for OCR priority.
    pub position_ms: i64,
    /// Language priority for external VobSub tracks.
    pub preferred_languages: Vec<String>,
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
    /// The fonts attached to the video, for an ASS track (SUB-025).
    pub fonts: Vec<std::path::PathBuf>,
}

/// Cumulative text from a bounded graphic OCR batch (SUB-030).
#[derive(Debug, Clone)]
pub struct Progress {
    /// Recognised text in timeline order.
    pub loaded: Loaded,
    /// Pictures processed, including unreadable ones.
    pub done: usize,
    /// Pictures in the whole track.
    pub total: usize,
}

/// Progressive OCR reports; never a final cache or complete score.
pub type Reports = Box<dyn Fn(Progress) + Send + Sync>;

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

/// mpv can pause on the last frame just before the end, or report the end
/// position just before its pause event (SUB-026, AK-BUG-007).
const PASSED_MS: i64 = 100;

/// Where playback stops for the pause at the end of each line (SUB-026),
/// given the lines' ends (offset counted) and the stop set now: while
/// playing, the stop stays through the endpoint's event margin or until
/// an earlier end comes; once paused there, the next line's end. A seek
/// beyond the margin still advances to the next line.
pub fn auto_pause_end(
    ends: impl Iterator<Item = i64>,
    pos: i64,
    paused: bool,
    current: Option<i64>,
) -> Option<i64> {
    let next = ends.filter(|&end| end > pos + PASSED_MS).min();
    let ahead = if paused { PASSED_MS } else { -PASSED_MS };
    match current {
        Some(end) if end > pos + ahead && next.is_none_or(|n| n >= end) => Some(end),
        _ => next,
    }
}

/// What the main subtitle looks like now: the mode applies only during
/// playback, with no popup open and without the mouse over it (SUB-014,
/// BUG-2198, BUG-2235), unless the mouse is not to reveal it (SUB-022).
pub fn obscured(
    mode: Obscure,
    playing: bool,
    popup_open: bool,
    hovered: bool,
    reveal_on_hover: bool,
) -> Obscure {
    if !playing || popup_open || (hovered && reveal_on_hover) {
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
    /// The text is a partial OCR snapshot, not a complete track.
    pub incomplete: bool,
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
        self.incomplete = false;
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
        self.incomplete = false;
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

/// Map existing OCR cues into an expanded/joined snapshot. A cue remains
/// identifiable by its text and a containing time span, even when earlier
/// pictures are inserted or neighbouring identical text joins it.
pub(crate) fn progress_cues(old: &[Cue], new: &[Cue]) -> Vec<Option<usize>> {
    let index: std::collections::BTreeMap<_, _> = new
        .iter()
        .enumerate()
        .map(|(i, cue)| ((cue.plain_text.as_str(), cue.start_ms), (i, cue.end_ms)))
        .collect();
    old.iter()
        .map(|cue| {
            let ((text, _), (i, end)) = index
                .range(..=(cue.plain_text.as_str(), cue.start_ms))
                .next_back()?;
            (*text == cue.plain_text && *end >= cue.end_ms).then_some(*i)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SUB-014
    // T-SUB-026
    #[test]
    fn the_pause_is_at_the_end_of_the_next_line() {
        // lines ending at 2 s, 4 s and 4.5 s (one inside another)
        let ends = || [4_000, 2_000, 4_500].into_iter();
        assert_eq!(auto_pause_end(ends(), 0, false, None), Some(2_000));
        // playing up to it: it stays, even in its last moments
        assert_eq!(
            auto_pause_end(ends(), 1_990, false, Some(2_000)),
            Some(2_000)
        );
        // stopped on its last frame: the next line's
        assert_eq!(
            auto_pause_end(ends(), 1_960, true, Some(2_000)),
            Some(4_000)
        );
        // paused by hand in the middle: it stays
        assert_eq!(
            auto_pause_end(ends(), 1_000, true, Some(2_000)),
            Some(2_000)
        );
        // back to an earlier line: its end
        assert_eq!(auto_pause_end(ends(), 100, false, Some(4_000)), Some(2_000));
        // past it by a seek: the next one
        assert_eq!(
            auto_pause_end(ends(), 3_000, false, Some(2_000)),
            Some(4_000)
        );
        // after the last line: none
        assert_eq!(auto_pause_end(ends(), 5_000, false, Some(4_500)), None);
        assert_eq!(auto_pause_end(std::iter::empty(), 0, false, None), None);
    }

    // T-SUB-026, AK-BUG-007: a position event can reach the end before
    // mpv's pause event. Advancing the end then cancels the pending stop.
    #[test]
    fn the_stop_waits_for_the_pause_event_at_the_endpoint() {
        let ends = || [2_500, 5_500, 8_500].into_iter();
        for pos in [2_500, 2_520, 2_580] {
            assert_eq!(auto_pause_end(ends(), pos, false, Some(2_500)), Some(2_500));
            assert_eq!(auto_pause_end(ends(), pos, true, Some(2_500)), Some(5_500));
        }
        // A seek well beyond the old end still arms the next line.
        assert_eq!(
            auto_pause_end(ends(), 3_000, false, Some(2_500)),
            Some(5_500)
        );
    }

    #[test]
    fn blur_strength_and_when_it_applies() {
        assert_eq!(blur_sigma(20.0), 12.0);
        assert_eq!(blur_sigma(40.0), 18.0);
        assert_eq!(
            obscured(Obscure::Blur, true, false, false, true),
            Obscure::Blur
        );
        assert_eq!(
            obscured(Obscure::Blur, false, false, false, true),
            Obscure::None,
            "paused"
        );
        assert_eq!(
            obscured(Obscure::Blur, true, true, false, true),
            Obscure::None,
            "popup open"
        );
        assert_eq!(
            obscured(Obscure::Hidden, true, false, true, true),
            Obscure::None,
            "hovered"
        );
        assert_eq!(
            obscured(Obscure::Hidden, true, false, false, true),
            Obscure::Hidden
        );
        // T-SUB-022: the mouse reveals nothing; a pause and a popup still do
        assert_eq!(
            obscured(Obscure::Blur, true, false, true, false),
            Obscure::Blur
        );
        assert_eq!(
            obscured(Obscure::Hidden, false, false, true, false),
            Obscure::None
        );
        assert_eq!(
            obscured(Obscure::Hidden, true, true, true, false),
            Obscure::None
        );
    }

    fn loaded(text: &str, format: Format) -> Loaded {
        Loaded {
            track: nikomiru_subs::parse(format, text),
            encoding: Some("UTF-8".into()),
            fonts: Vec::new(),
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

#[cfg(test)]
mod progressive_tests {
    use super::*;
    // T-SUB-030: inserted earlier cues and joined spans retain popup identity.
    #[test]
    fn cumulative_text_maps_cues_by_text_and_time() {
        let read = |text: &str| nikomiru_subs::parse(Format::Srt, text).cues;
        let old =
            read("1\n00:00:05,000 --> 00:00:06,000\n猫\n\n2\n00:00:09,000 --> 00:00:10,000\n犬\n");
        let new = read(
            "1\n00:00:01,000 --> 00:00:02,000\n本\n\n2\n00:00:04,000 --> 00:00:07,000\n猫\n\n3\n00:00:09,000 --> 00:00:10,000\n犬\n",
        );
        assert_eq!(progress_cues(&old, &new), [Some(1), Some(2)]);
        assert_eq!(progress_cues(&new, &old), [None, None, Some(1)]);
    }
}
