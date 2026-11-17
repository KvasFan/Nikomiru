// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle list (SUB-015): every cue with its time, the current one
//! highlighted, search, a click on the time seeks and a click on the text looks
//! the word up. The words already made into cards are underlined (LKP-061);
//! readings are not drawn here, as they would change the rows' heights.
//!
//! A film has a few thousand cues: their heights are measured once per list,
//! query and width, and only the rows in view are laid out and drawn.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use egui::text::{CCursorRange, LayoutJob};
use egui::{
    Area, Color32, CursorIcon, FontId, Frame, Galley, Id, Key, Margin, Order, Pos2, Rect, RichText,
    Role, ScrollArea, Sense, TextEdit, Vec2, WidgetInfo, pos2, vec2,
};

use nikomiru_subs::timeline::Timeline;

use super::palette;
use super::theme;
use crate::popup;
use crate::session::Session;
use crate::subtitle_study::{
    DurationBounds, LineShape, ReadingSpeed, StudyOrder, TextLength, TimeWindow,
};
use crate::subtitles::clock;
use crate::ui::Command;

/// The underline of the words already made into cards, as in the overlay.
const BACKGROUND: Color32 = Color32::from_rgba_unmultiplied_const(0x14, 0x14, 0x18, 0xe6);
const CURRENT: Color32 = Color32::from_rgba_unmultiplied_const(0x4a, 0x90, 0xe2, 0x3a);
const TIME: Color32 = Color32::from_rgb(0x9a, 0xb8, 0xff);
/// Inside a row: 4 around the cells, 6 more right of the text.
const PAD: f32 = 4.0;
const RIGHT: f32 = 6.0;
const ROW_SPACING: f32 = 2.0;

/// The sizes of a row, after the text's (SUB-023): at 16 pt, the times at
/// 14 pt in a column 52 pt wide.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sizes {
    text: f32,
    time: f32,
    time_width: f32,
    /// Where the text starts in a row.
    text_x: f32,
}

impl Sizes {
    fn of(text: f32) -> Self {
        let k = text / 16.0;
        Sizes {
            text,
            time: 14.0 * k,
            time_width: 52.0 * k,
            text_x: 52.0 * k + 54.0,
        }
    }
}

/// Which lines the list shows, while the words are coloured (KNW-013).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LineFilter {
    /// Every line.
    #[default]
    All,
    /// The lines with a word not known.
    New,
    /// The lines with one word not known (KNW-003).
    PlusOne,
}

impl LineFilter {
    const ALL: [LineFilter; 3] = [LineFilter::All, LineFilter::New, LineFilter::PlusOne];

    /// Whether a line with these marks is shown; a line not split yet is
    /// shown with the new ones.
    fn keeps(self, marks: Option<&crate::words::Marks>) -> bool {
        match (self, marks) {
            (LineFilter::All, _) | (LineFilter::New, None) => true,
            (LineFilter::New, Some(m)) => !m.unknown.is_empty(),
            (LineFilter::PlusOne, m) => m.is_some_and(|m| m.plus_one),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Scope {
    window: TimeWindow,
    speed: ReadingSpeed,
    duration: DurationBounds,
    text_length: TextLength,
    unique: bool,
    shape: LineShape,
    context: u8,
    exact: bool,
    order: StudyOrder,
}

/// The rows on show: their cues and where each one starts.
#[derive(Debug, Default)]
struct Rows {
    /// The list version, the query, the filter and, with a filter, the
    /// words' revision the rows were picked for.
    picked: Option<(u64, String, LineFilter, u64, Scope)>,
    /// Changes whenever other rows are picked.
    generation: u64,
    chronological: Vec<(usize, usize)>,
    /// The width the heights were measured at.
    width: f32,
    /// The text size they were measured at.
    size: f32,
    /// The cues shown, in order.
    cues: Vec<usize>,
    /// The top of each row in the list, and the list's height last.
    tops: Vec<f32>,
}

#[derive(Debug, Default)]
pub(crate) struct StudyLabels {
    tools: String,
    invert_shown: String,
    select_shown: String,
    clear: String,
    export_selected: String,
    vtt_shown: String,
    vtt_selected: String,
    bilingual_shown: String,
    bilingual_selected: String,
    html_labels: [String; 2],
    bilingual_srt_shown: String,
    bilingual_srt_selected: String,
    select_line: String,
    copy_export: String,
    copy_shown: String,
    copy_selected: String,
    copied: String,
    copy_error: String,
    from_here: String,
    to_here: String,
    time_labels: [String; 5],
    reset_window: String,
    examples: String,
    clear_examples: String,
    study_groups: [String; 3],
    practice_controls: [String; 2],
    pause_label: String,
    queue_labels: [String; 3],
    practice_labels: [String; 2],
    vocabulary_labels: [String; 3],
    mask_labels: [String; 4],
    context_labels: [String; 4],
    shapes: [String; 3],
    unique_label: String,
    preset_labels: [String; 4],
    portable: [String; 2],
    order_labels: [String; 4],
    translation_labels: [String; 3],
    exact_label: String,
    text_labels: [String; 4],
    duration_labels: [String; 4],
    speed: String,
    min_speed: String,
    max_speed: String,
    reset_speed: String,
    statistics: String,
    stats_shown: String,
    stats_selected: String,
    stats_lines: String,
    stats_characters: String,
    stats_dialogue: String,
    previous_match: String,
    next_match: String,
}

impl StudyLabels {
    fn translated(tr: &super::popups::Translate<'_>) -> Self {
        Self {
            tools: tr("sub-study-tools", &[]),
            portable: ["study-presets-import", "study-presets-export"].map(|k| tr(k, &[])),
            practice_controls: ["sub-practice-skip", "sub-practice-restart"].map(|k| tr(k, &[])),
            pause_label: tr("sub-practice-pause", &[]),
            translation_labels: ["sub-preview-title", "sub-preview-show", "sub-preview-empty"]
                .map(|k| tr(k, &[])),
            time_labels: [
                "sub-time-edit",
                "sub-time-from",
                "sub-time-to",
                "sub-time-apply",
                "sub-time-invalid",
            ]
            .map(|k| tr(k, &[])),
            order_labels: [
                "sub-order-source",
                "sub-order-short",
                "sub-order-long",
                "sub-order-text",
            ]
            .map(|k| tr(k, &[])),
            exact_label: tr("sub-search-exact", &[]),
            queue_labels: [
                "sub-practice-shown",
                "sub-practice-selected",
                "sub-practice-passes",
            ]
            .map(|k| tr(k, &[])),
            context_labels: [
                "sub-context-exact",
                "sub-context-one",
                "sub-context-two",
                "sub-context-hit",
            ]
            .map(|k| tr(k, &[])),
            preset_labels: [
                "sub-presets",
                "vocabulary-preset-name",
                "vocabulary-preset-store",
                "vocabulary-preset-delete",
            ]
            .map(|k| tr(k, &[])),
            shapes: ["sub-shape-any", "sub-shape-single", "sub-shape-multiple"].map(|k| tr(k, &[])),
            unique_label: tr("sub-unique-captions", &[]),
            text_labels: [
                "sub-text-tools",
                "sub-text-min",
                "sub-text-max",
                "sub-text-reset",
            ]
            .map(|k| tr(k, &[])),
            study_groups: [
                "sub-practice-tools",
                "sub-vocabulary-tools",
                "sub-html-tools",
            ]
            .map(|k| tr(k, &[])),
            practice_labels: ["sub-practice-current", "sub-practice-stop"].map(|k| tr(k, &[])),
            html_labels: ["sub-html-shown", "sub-html-selected"].map(|k| tr(k, &[])),
            vocabulary_labels: [
                "sub-vocabulary-shown",
                "sub-vocabulary-selected",
                "sub-vocabulary-all",
            ]
            .map(|k| tr(k, &[])),
            mask_labels: [
                "sub-mask-text",
                "sub-reveal-shown",
                "sub-mask-again",
                "sub-reveal-line",
            ]
            .map(|k| tr(k, &[])),
            duration_labels: [
                "sub-duration-tools",
                "sub-duration-min",
                "sub-duration-max",
                "sub-duration-reset",
            ]
            .map(|k| tr(k, &[])),
            select_shown: tr("sub-select-shown", &[]),
            invert_shown: tr("sub-invert-shown", &[]),
            clear: tr("sub-selection-clear", &[]),
            export_selected: tr("sub-export-selected", &[]),
            vtt_shown: tr("sub-vtt-shown", &[]),
            vtt_selected: tr("sub-vtt-selected", &[]),
            bilingual_shown: tr("sub-bilingual-shown", &[]),
            bilingual_selected: tr("sub-bilingual-selected", &[]),
            bilingual_srt_shown: tr("sub-bilingual-srt-shown", &[]),
            bilingual_srt_selected: tr("sub-bilingual-srt-selected", &[]),
            select_line: tr("sub-select-line", &[]),
            copy_shown: tr("sub-copy-shown", &[]),
            copy_export: tr("sub-copy-export", &[]),
            copy_selected: tr("sub-copy-selected", &[]),
            copied: tr("sub-transcript-copied", &[]),
            copy_error: tr("sub-export-unavailable", &[]),
            from_here: tr("sub-window-from", &[]),
            to_here: tr("sub-window-to", &[]),
            reset_window: tr("sub-window-reset", &[]),
            examples: tr("vocabulary-examples", &[]),
            clear_examples: tr("vocabulary-examples-clear", &[]),
            speed: tr("sub-speed-tools", &[]),
            min_speed: tr("sub-speed-min", &[]),
            max_speed: tr("sub-speed-max", &[]),
            reset_speed: tr("sub-speed-reset", &[]),
            statistics: tr("sub-stats-title", &[]),
            stats_shown: tr("sub-stats-shown", &[]),
            stats_selected: tr("sub-stats-selected", &[]),
            stats_lines: tr("sub-stats-lines", &[]),
            stats_characters: tr("sub-stats-characters", &[]),
            stats_dialogue: tr("sub-stats-dialogue", &[]),
            previous_match: tr("sub-previous-match", &[]),
            next_match: tr("sub-next-match", &[]),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MetricKey {
    version: u64,
    revision: u64,
    delay: i64,
    query: String,
    filter: LineFilter,
    scope: Scope,
    concordance: Option<(String, u64)>,
    selection: u64,
}
#[derive(Debug)]
struct Metrics {
    key: MetricKey,
    data: Result<
        (
            crate::subtitle_study::Statistics,
            crate::subtitle_study::Statistics,
        ),
        &'static str,
    >,
}

/// What the list keeps between frames.
#[derive(Debug, Default)]
struct TimeDraft {
    seen: TimeWindow,
    from: String,
    to: String,
    error: bool,
}

#[derive(Debug)]
struct TranslationPreview {
    key: (u64, i64, u64, i64),
    cue: usize,
    text: String,
}

pub(crate) struct SecondaryScene<'a> {
    cues: &'a [nikomiru_subs::Cue],
    delay: i64,
    version: u64,
    complete: bool,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    query: String,
    preset_name: String,
    translation: Option<TranslationPreview>,
    practice_passes: u8,
    rows: Rows,
    selected: BTreeSet<usize>,
    selection_anchor: Option<usize>,
    masked: bool,
    revealed: BTreeSet<usize>,
    selection_revision: u64,
    metrics: Option<Metrics>,
    navigation: Option<(MetricKey, Option<crate::subtitle_study::Navigation>)>,
    selection_version: Option<u64>,
    copied: Option<bool>,
    window: TimeWindow,
    time_draft: Option<TimeDraft>,
    speed: ReadingSpeed,
    duration: DurationBounds,
    text_length: TextLength,
    unique: bool,
    shape: LineShape,
    context: u8,
    exact: bool,
    order: StudyOrder,
    last_concordance: Option<(String, u64)>,
    /// The current row the list last scrolled to, with its rows' generation.
    followed: Option<(u64, usize)>,
    /// The search field had the keyboard in the last frame.
    search_focused: bool,
    /// The cues of the rows in view, last asked for their words.
    asked: Vec<usize>,
    /// The Words tab is shown (KNW-009).
    words_tab: bool,
    word_scope: Option<u64>,
    /// The lines shown (KNW-013).
    filter: LineFilter,
    /// What it keeps.
    words: super::word_list::State,
    /// The rows drawn in the last frame.
    #[cfg(test)]
    drawn: Vec<Drawn>,
}

impl State {
    #[cfg(feature = "test-harness")]
    pub(crate) fn set_study_filters(
        &mut self,
        preset: &nikomiru_core::config::SubtitleStudyPreset,
    ) {
        self.apply_preset(preset);
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn study_mask(&mut self, enabled: bool) {
        self.masked = enabled;
        self.revealed.clear();
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn study_selection(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn study_reveals(&self) -> Vec<usize> {
        self.revealed.iter().copied().collect()
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn set_study_query(&mut self, query: String) {
        self.query = query;
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn study_rows(&self) -> &[usize] {
        &self.rows.cues
    }
    fn rendered_scope_current(&self, session: &Session) -> bool {
        let Some((version, query, filter, revision, scope)) = self.rows.picked.as_ref() else {
            return false;
        };
        let expected = if session.word_colors_on() {
            self.filter
        } else {
            LineFilter::All
        };
        let marks = if expected == LineFilter::All {
            0
        } else {
            session.words_revision()
        };
        !self.words_tab
            && *version == session.sub_list_version
            && query == &self.query
            && *filter == expected
            && *revision == marks
            && *scope == self.scope()
            && self.last_concordance
                == session
                    .subtitle_examples()
                    .map(|c| (c.key.clone(), c.revision))
    }
    fn scope(&self) -> Scope {
        Scope {
            window: self.window,
            speed: self.speed,
            duration: self.duration,
            text_length: self.text_length,
            unique: self.unique,
            shape: self.shape,
            context: self.context,
            exact: self.exact,
            order: self.order,
        }
    }
    pub(crate) fn study_action(
        &mut self,
        session: &Session,
        action: crate::shortcuts::Action,
    ) -> Option<Command> {
        use crate::shortcuts::Action;
        if !self.rendered_scope_current(session) {
            return None;
        }
        if matches!(action, Action::StudyPrevious | Action::StudyNext) {
            return self
                .matching_target(session, action == Action::StudyNext)
                .map(Command::ListTime);
        }
        let position = (session.view.time_pos * 1000.0).round() as i64;
        let cue = session
            .subs
            .timeline()
            .active(position)
            .into_iter()
            .find(|i| self.rows.cues.contains(i))?;
        match action {
            Action::StudyReveal if self.masked => {
                self.revealed.insert(cue);
            }
            Action::StudySelect => {
                if !self.selected.remove(&cue) {
                    self.selected.insert(cue);
                }
                self.selection_anchor = Some(cue);
                self.selection_revision = self.selection_revision.wrapping_add(1);
            }
            _ => {}
        }
        None
    }
    pub(crate) fn matching_target(&self, session: &Session, forward: bool) -> Option<usize> {
        if !self.rendered_scope_current(session) {
            return None;
        }
        crate::subtitle_study::Navigation::new(
            session.subs.timeline().cues(),
            &self.rows.cues,
            session.subs.timeline().delay_ms,
        )?
        .neighbour((session.view.time_pos * 1000.0).round() as i64, forward)
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct Drawn {
    cue: usize,
    row: Rect,
    time: Rect,
    text: Rect,
    galley: Arc<Galley>,
    current: bool,
    mined: usize,
    plus_one: bool,
}

/// What the list shows: the session's part of it.
pub(crate) struct Scene<'a> {
    pub timeline: &'a Timeline,
    pub secondary: Option<SecondaryScene<'a>>,
    /// Changes whenever the cues change (`Session::sub_list_version`).
    pub version: u64,
    /// The playback position.
    pub pos_ms: i64,
    pub practice_status: Option<String>,
    pub practice_pause_ms: u16,
    pub practice_left: Option<u8>,
    pub title: String,
    /// How much of the subtitle the learner understands, or how far its
    /// count is (KNW-007).
    pub score: Option<String>,
    pub reading: Option<(String, Option<f32>)>,
    pub reading_cancel: String,
    pub export_ready: bool,
    pub bilingual_ready: bool,
    pub concordance: Option<&'a crate::subtitle_study::Concordance>,
    pub export_label: String,
    pub study: StudyLabels,
    /// The Lines and Words tabs, while the words are coloured (KNW-009);
    /// the words once all lines are split.
    pub tabs: Option<(String, String)>,
    /// The filters of the lines, in the order of `LineFilter::ALL`
    /// (KNW-013), and the words' revision, which changes their marks.
    pub filters: [String; 3],
    pub marks_version: u64,
    pub word_scope: Option<u64>,
    pub words: Option<&'a [crate::words::ListedWord]>,
    pub word_labels: super::word_list::Labels,
    pub search_hint: String,
    pub close: String,
    /// The words of a text underlined, in bytes, each with why, and whether
    /// it has one new word (LKP-061, KNW-002, KNW-003).
    pub marks: &'a dyn Fn(&str) -> Option<crate::words::Marks>,
    /// `[subtitles] list_text_size` and `list_width` (SUB-023).
    pub text_size: f32,
    pub width: f32,
}

fn text_job(text: &str, width: f32, size: f32) -> LayoutJob {
    LayoutJob::simple(
        text.to_string(),
        FontId::proportional(size),
        Color32::WHITE,
        width,
    )
}

fn time_job(ms: i64, size: f32) -> LayoutJob {
    LayoutJob::simple_singleline(clock(ms), FontId::monospace(size), TIME)
}

impl Rows {
    /// Picks the rows again when the cues or the query change, and measures
    /// them again when the width changes too.
    fn update(
        &mut self,
        ctx: &egui::Context,
        scene: &Scene<'_>,
        query: &str,
        filter: LineFilter,
        scope: Scope,
        width: f32,
    ) {
        let timeline = scene.timeline;
        let revision = if filter == LineFilter::All {
            0
        } else {
            scene.marks_version
        };
        let fresh = self.picked.as_ref().is_none_or(|(v, q, f, r, w)| {
            *v != scene.version || q != query || *f != filter || *r != revision || *w != scope
        });
        if fresh {
            self.cues = pick_indices(scene, query, filter, scope);
            self.chronological = self
                .cues
                .iter()
                .copied()
                .enumerate()
                .map(|(row, cue)| (cue, row))
                .collect();
            self.chronological.sort_unstable();
            self.picked = Some((scene.version, query.to_string(), filter, revision, scope));
            self.generation += 1;
        } else if self.width == width && self.size == scene.text_size {
            return;
        }
        self.width = width;
        self.size = scene.text_size;
        let sizes = Sizes::of(scene.text_size);
        let text_width = (width - sizes.text_x - RIGHT).max(sizes.text);
        let cues = timeline.cues();
        self.tops = ctx.fonts_mut(|fonts| {
            let time = fonts.layout_job(time_job(0, sizes.time)).size().y;
            let mut top = 0.0;
            let mut tops = Vec::with_capacity(self.cues.len() + 1);
            for &i in &self.cues {
                tops.push(top);
                let text = fonts
                    .layout_job(text_job(&cues[i].plain_text, text_width, sizes.text))
                    .size()
                    .y;
                top += text.max(time) + 2.0 * PAD + ROW_SPACING;
            }
            tops.push((top - ROW_SPACING).max(0.0));
            tops
        });
    }

    fn current_row(&self, timeline: &Timeline, position: i64) -> Option<usize> {
        let current = timeline.list_position(position)?;
        let at = self
            .chronological
            .partition_point(|(cue, _)| *cue <= current)
            .checked_sub(1)?;
        self.chronological.get(at).map(|(_, row)| *row)
    }

    /// The rows that meet the band `top..bottom` of the list.
    fn between(&self, top: f32, bottom: f32) -> std::ops::Range<usize> {
        let n = self.cues.len();
        let first = self.tops[1..=n].partition_point(|&end| end <= top);
        let last = self.tops[..n].partition_point(|&start| start < bottom);
        first..last.max(first)
    }

    /// Row `k` in the list, its spacing left out.
    fn rect(&self, k: usize, origin: Pos2) -> Rect {
        let end = if k + 1 == self.cues.len() {
            self.tops[k + 1]
        } else {
            self.tops[k + 1] - ROW_SPACING
        };
        Rect::from_min_max(
            origin + vec2(0.0, self.tops[k]),
            origin + vec2(self.width, end),
        )
    }
}

/// The character under `pos` (relative to the galley), if any: a click right
/// of a line or between rows picks nothing.
fn char_at(galley: &Galley, pos: Vec2) -> Option<usize> {
    let mut start = 0;
    for row in &galley.rows {
        let count = row.glyphs.len();
        if (row.pos.y..row.pos.y + row.size.y).contains(&pos.y) {
            let x = pos.x - row.pos.x;
            return row
                .glyphs
                .iter()
                .position(|g| (g.pos.x..g.max_x()).contains(&x))
                .map(|k| start + k);
        }
        start += count + usize::from(row.ends_with_newline);
    }
    None
}

/// The rects of chars `chars` of `galley`, one per row they are on.
fn char_rects(galley: &Galley, chars: Range<usize>) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut start = 0;
    for row in &galley.rows {
        let count = row.glyphs.len();
        let (from, to) = (chars.start.max(start), chars.end.min(start + count));
        if from < to {
            let glyphs = &row.glyphs[from - start..to - start];
            let left = glyphs[0].pos.x;
            let right = glyphs.iter().map(|g| g.max_x()).fold(left, f32::max);
            out.push(Rect::from_min_max(
                row.pos + vec2(left, 0.0),
                row.pos + vec2(right, row.size.y),
            ));
        }
        start += count + usize::from(row.ends_with_newline);
    }
    out
}

/// Draws the list on the right, from the top down to the control bar.
/// Returns whether it stays open.
pub(crate) fn show(
    ctx: &egui::Context,
    s: &Session,
    state: &mut State,
    search_focus: &mut bool,
    bar_height: f32,
    out: &mut Vec<Command>,
) -> bool {
    let tr = |id: &str| s.translator().tr(id, &[]);
    let marks = |text: &str| s.list_marks(text);
    let words = s.word_list();
    let coloured = s.word_colors_on();
    let ticked = state.words.ticked.len();
    let mut word_labels =
        super::word_list::Labels::translated(&|key, args| s.translator().tr(key, args), ticked);
    word_labels.examples = Some(tr("vocabulary-examples"));
    word_labels.source = s.view.title.clone();
    word_labels.source_version = Some(s.sub_list_version);
    let scene = Scene {
        timeline: s.subs.timeline(),
        secondary: Some(SecondaryScene {
            cues: s.secondary.timeline().cues(),
            delay: s.secondary.timeline().delay_ms,
            version: s.secondary_version(),
            complete: !s.secondary.incomplete,
        }),
        version: s.sub_list_version,
        concordance: s.subtitle_examples(),
        pos_ms: (s.view.time_pos * 1000.0).round() as i64,
        practice_pause_ms: s.practice_pause_ms(),
        practice_left: s.practice_remaining(),
        practice_status: s.practice_remaining().map(|n| {
            format!(
                "{} · {}{}",
                s.translator()
                    .tr("sub-practice-remaining", &[("count", n.to_string())]),
                s.translator().tr(
                    "sub-practice-cues",
                    &[(
                        "count",
                        s.practice_cues_remaining().unwrap_or(0).to_string()
                    )]
                ),
                s.practice_wait_ms()
                    .map(|ms| format!(
                        " · {}",
                        s.translator()
                            .tr("sub-practice-wait", &[("count", ms.to_string())])
                    ))
                    .unwrap_or_default()
            )
        }),
        title: tr("subtitle-list"),
        score: score_line(s),
        reading: s.graphic_read_status(),
        reading_cancel: tr("sub-reading-cancel"),
        export_ready: s.subtitle_export_ready(),
        bilingual_ready: s.subtitle_export_ready()
            && !s.secondary.incomplete
            && !s.secondary.timeline().cues().is_empty(),
        export_label: tr("sub-export"),
        study: StudyLabels::translated(&|key, args| s.translator().tr(key, args)),
        tabs: coloured.then(|| (tr("subtitle-list-lines"), tr("subtitle-list-words"))),
        filters: [
            tr("subtitle-lines-all"),
            tr("subtitle-lines-new"),
            tr("subtitle-lines-plus-one"),
        ],
        marks_version: s.words_revision(),
        word_scope: s.vocabulary_scope_id(),
        words: words.as_deref().map(Vec::as_slice),
        word_labels,
        search_hint: tr("subtitle-search"),
        close: tr("subtitle-list-close"),
        marks: &marks,
        text_size: s.config().subtitles.list_text_size,
        width: s.config().subtitles.list_width,
    };
    draw(ctx, &scene, state, search_focus, bar_height, out)
}

/// The score of the main subtitle as the list's header says it (KNW-007).
fn score_line(s: &Session) -> Option<String> {
    let percent = |p: Option<usize>| p.map_or_else(|| "–".into(), |p| p.to_string());
    let t = s.translator();
    Some(match s.score()? {
        Ok(score) => t.tr(
            "subtitle-score",
            &[
                ("known", percent(score.known_percent())),
                ("clear", percent(score.clear_percent())),
            ],
        ),
        Err(done) => {
            let of = s.subs.timeline().cues().iter();
            let of = of.filter(|c| !c.plain_text.trim().is_empty()).count();
            t.tr(
                "subtitle-score-counting",
                &[("done", done.to_string()), ("of", of.to_string())],
            )
        }
    })
}

fn translation_key(scene: &Scene<'_>) -> Option<(u64, i64, u64, i64)> {
    let s = scene
        .secondary
        .as_ref()
        .filter(|s| s.complete && !s.cues.is_empty() && scene.export_ready)?;
    Some((scene.version, scene.timeline.delay_ms, s.version, s.delay))
}
fn translation_window(ctx: &egui::Context, scene: &Scene<'_>, state: &mut State) {
    if state
        .translation
        .as_ref()
        .is_some_and(|p| Some(p.key) != translation_key(scene))
    {
        state.translation = None;
    }
    let Some(preview) = &state.translation else {
        return;
    };
    let mut open = true;
    egui::Window::new(format!(
        "{} · {}",
        scene.study.translation_labels[0],
        clock(scene.timeline.start_pos(preview.cue))
    ))
    .id(Id::unique("subtitle-translation-preview"))
    .open(&mut open)
    .default_width(360.0)
    .show(ctx, |ui| {
        ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            ui.label(&preview.text);
        });
    });
    if !open {
        state.translation = None;
    }
}

pub(crate) fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    state: &mut State,
    search_focus: &mut bool,
    bar_height: f32,
    out: &mut Vec<Command>,
) -> bool {
    if state.selection_version != Some(scene.version) {
        state.selected.clear();
        state.selection_anchor = None;
        state.revealed.clear();
        state.selection_revision = state.selection_revision.wrapping_add(1);
        state.copied = None;
        state.window = TimeWindow::default();
        state.time_draft = None;
        state.speed = ReadingSpeed::default();
        state.duration = DurationBounds::default();
        state.text_length = TextLength::default();
        state.unique = false;
        state.shape = LineShape::default();
        state.context = 0;
        state.exact = false;
        state.order = StudyOrder::default();
        state.selection_version = Some(scene.version);
    }
    if state.word_scope != scene.word_scope {
        state.word_scope = scene.word_scope;
        state.words = Default::default();
        state.query.clear();
        state.words_tab = scene.word_scope.is_some();
    }
    let key = scene.concordance.map(|c| (c.key.clone(), c.revision));
    if key != state.last_concordance {
        if key.is_some() {
            state.words_tab = false;
            state.query.clear();
        }
        state.rows.picked = None;
        state.last_concordance = key;
    }
    let screen = ctx.content_rect();
    let width = scene.width.min(0.6 * screen.width());
    let height = (screen.height() - bar_height).max(0.0);
    let mut open = true;
    Area::new(Id::unique("subtitle-list"))
        .order(Order::Middle)
        .fixed_pos(pos2(screen.right() - width, screen.top()))
        .show(ctx, |ui| {
            palette::on_video(ui);
            Frame::new()
                .fill(BACKGROUND)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width((width - 20.0).max(0.0));
                    ui.set_height((height - 20.0).max(0.0));
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&scene.title).family(theme::bold()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close = ui.button("✕").on_hover_text(&scene.close);
                            close.widget_info(|| {
                                WidgetInfo::labeled(Role::Button, true, &scene.close)
                            });
                            if close.clicked() {
                                open = false;
                            }
                        });
                    });

                    if ui
                        .add_enabled(scene.export_ready, egui::Button::new(&scene.export_label))
                        .clicked()
                    {
                        out.push(Command::SubtitleExport);
                    }
                    if let Some((text, fraction)) = &scene.reading {
                        super::ocr_progress::show(ui, text, *fraction, &scene.reading_cancel, out);
                    }
                    if let Some(score) = &scene.score {
                        ui.label(RichText::new(score).small().color(TIME));
                    }
                    // KNW-009: the lines or their words
                    if let Some((lines, words)) = &scene.tabs {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(!state.words_tab, lines).clicked() {
                                state.words_tab = false;
                            }
                            if ui.selectable_label(state.words_tab, words).clicked() {
                                state.words_tab = true;
                            }
                        });
                    }
                    // KNW-013: the lines with a new word, or with one
                    if scene.tabs.is_some() && !state.words_tab {
                        ui.horizontal_wrapped(|ui| {
                            for (filter, label) in LineFilter::ALL.iter().zip(&scene.filters) {
                                if ui
                                    .selectable_label(state.filter == *filter, label)
                                    .clicked()
                                {
                                    state.filter = *filter;
                                }
                            }
                        });
                    }
                    if let Some(c) = scene.concordance {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("{}: {}", scene.study.examples, c.label));
                            for (radius, label) in
                                scene.study.context_labels[..3].iter().enumerate()
                            {
                                ui.selectable_value(&mut state.context, radius as u8, label);
                            }
                            if ui.button(&scene.study.clear_examples).clicked() {
                                out.push(Command::SubtitleExamplesClear);
                            }
                        });
                    }
                    if scene.word_scope.is_some()
                        && ui.button(&scene.study.vocabulary_labels[2]).clicked()
                    {
                        out.push(Command::VocabularyScopeClear);
                    }
                    open &= search(ui, state, search_focus, &scene.search_hint);
                    if scene.tabs.is_some() && state.words_tab {
                        if let Some(all) = scene.words {
                            super::word_list::show(
                                ui,
                                all,
                                &mut state.query,
                                &scene.word_labels,
                                &mut state.words,
                                out,
                            );
                        }
                    } else {
                        study_tools(ui, scene, state, out);
                        rows(ui, scene, state, out);
                    }
                });
        });
    translation_window(ctx, scene, state);
    open
}

/// The search field: Ctrl+F gives it the keyboard with the text selected; Esc
/// clears it, then closes the list.
fn search(ui: &mut egui::Ui, state: &mut State, focus: &mut bool, hint: &str) -> bool {
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let mut field = TextEdit::singleline(&mut state.query)
        .hint_text(hint)
        .desired_width(f32::INFINITY)
        .show(ui);
    let mut open = true;
    // egui takes the keyboard from the field on Esc, before the frame
    if state.search_focused && escape && !field.response.has_focus() {
        if state.query.is_empty() {
            open = false;
        } else {
            state.query.clear();
            field.response.request_focus();
        }
    }
    if std::mem::take(focus) {
        field.response.request_focus();
        field
            .state
            .cursor
            .set_char_range(Some(CCursorRange::select_all(&field.galley)));
        field.state.store(ui.ctx(), field.response.id);
    }
    state.search_focused = field.response.has_focus();
    open
}

fn pick_indices(scene: &Scene<'_>, query: &str, filter: LineFilter, scope: Scope) -> Vec<usize> {
    let query = nikomiru_text::search::Query::new(query);
    let mut seen = std::collections::HashSet::new();
    let context = scene.concordance.and_then(|c| {
        crate::subtitle_study::context_indices(
            scene.timeline.cues().len(),
            &c.indices,
            scope.context,
        )
    });
    let mut indices: Vec<_> = scene
        .timeline
        .cues()
        .iter()
        .enumerate()
        .filter(|(i, cue)| {
            scene.concordance.is_none_or(|_| {
                context
                    .as_ref()
                    .is_some_and(|indices| indices.binary_search(i).is_ok())
            }) && scope.window.keeps(cue, scene.timeline.delay_ms)
                && scope.speed.keeps(cue)
                && scope.duration.keeps(cue)
                && scope.text_length.keeps(&cue.plain_text)
                && scope.shape.keeps(&cue.plain_text)
                && (if scope.exact {
                    query.matches_exact(&[&cue.plain_text])
                } else {
                    query.matches(&[&cue.plain_text])
                })
                && (scene.tabs.is_none() || filter.keeps((scene.marks)(&cue.plain_text).as_ref()))
        })
        .filter(|(_, cue)| {
            !scope.unique
                || seen.insert(
                    cue.plain_text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
        })
        .map(|(i, _)| i)
        .collect();
    crate::subtitle_study::order_indices(scene.timeline.cues(), &mut indices, scope.order);
    indices
}

fn shown_indices(scene: &Scene<'_>, state: &State) -> Vec<usize> {
    pick_indices(
        scene,
        &state.query,
        state.filter,
        Scope {
            window: state.window,
            speed: state.speed,
            duration: state.duration,
            text_length: state.text_length,
            unique: state.unique,
            shape: state.shape,
            context: state.context,
            exact: state.exact,
            order: state.order,
        },
    )
}

fn metric_key(scene: &Scene<'_>, state: &State) -> MetricKey {
    MetricKey {
        version: scene.version,
        revision: scene.marks_version,
        delay: scene.timeline.delay_ms,
        query: state.query.clone(),
        filter: state.filter,
        scope: Scope {
            window: state.window,
            speed: state.speed,
            duration: state.duration,
            text_length: state.text_length,
            unique: state.unique,
            shape: state.shape,
            context: state.context,
            exact: state.exact,
            order: state.order,
        },
        concordance: state.last_concordance.clone(),
        selection: state.selection_revision,
    }
}

fn matching_navigation(
    ui: &mut egui::Ui,
    scene: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let mut key = metric_key(scene, state);
    key.selection = 0;
    if state.navigation.as_ref().is_none_or(|(old, _)| old != &key) {
        let indices = shown_indices(scene, state);
        let navigation = crate::subtitle_study::Navigation::new(
            scene.timeline.cues(),
            &indices,
            scene.timeline.delay_ms,
        );
        state.navigation = Some((key, navigation));
    }
    if let Some((_, nav)) = &state.navigation {
        ui.horizontal_wrapped(|ui| {
            for (forward, label) in [
                (false, &scene.study.previous_match),
                (true, &scene.study.next_match),
            ] {
                let target = nav
                    .as_ref()
                    .and_then(|n| n.neighbour(scene.pos_ms, forward));
                if ui
                    .add_enabled(target.is_some(), egui::Button::new(label))
                    .clicked()
                    && let Some(cue) = target
                {
                    out.push(Command::ListTime(cue));
                }
            }
        });
    }
}

fn study_statistics(ui: &mut egui::Ui, scene: &Scene<'_>, state: &mut State) {
    let key = metric_key(scene, state);
    if state.metrics.as_ref().is_none_or(|m| m.key != key) {
        let shown = shown_indices(scene, state);
        let selected: Vec<_> = state.selected.iter().copied().collect();
        let data = crate::subtitle_study::statistics(
            scene.timeline.cues(),
            &shown,
            scene.timeline.delay_ms,
        )
        .and_then(|a| {
            crate::subtitle_study::statistics(
                scene.timeline.cues(),
                &selected,
                scene.timeline.delay_ms,
            )
            .map(|b| (a, b))
        });
        state.metrics = Some(Metrics { key, data });
    }
    if let Some(metrics) = &state.metrics {
        match metrics.data {
            Ok((shown, selected)) => {
                for (label, stats) in [
                    (&scene.study.stats_shown, shown),
                    (&scene.study.stats_selected, selected),
                ] {
                    let duration = format!(
                        "{}.{:03}",
                        clock(i64::try_from(stats.dialogue_ms).unwrap_or(i64::MAX)),
                        stats.dialogue_ms % 1000
                    );
                    ui.label(format!(
                        "{label}: {} {} · {} {} · {}: {duration}",
                        stats.cues,
                        scene.study.stats_lines,
                        stats.characters,
                        scene.study.stats_characters,
                        scene.study.stats_dialogue
                    ));
                }
            }
            Err(_) => {
                ui.label(&scene.study.copy_error);
            }
        }
    }
}

// Range endpoints are original indices, but membership follows the shown order.
fn select_range(
    selected: &mut BTreeSet<usize>,
    shown: &[usize],
    anchor: &mut Option<usize>,
    cue: usize,
    checked: bool,
    shift: bool,
) {
    let clicked = shown.iter().position(|i| *i == cue);
    let previous = anchor.and_then(|a| shown.iter().position(|i| *i == a));
    let range = match (shift, previous, clicked) {
        (true, Some(a), Some(b)) => &shown[a.min(b)..=a.max(b)],
        (_, _, Some(b)) => {
            *anchor = Some(cue);
            &shown[b..=b]
        }
        _ => {
            *anchor = None;
            return;
        }
    };
    for &i in range {
        if checked {
            selected.insert(i);
        } else {
            selected.remove(&i);
        }
    }
}

pub(super) fn sync_study_presets(
    ctx: &egui::Context,
    saved: &[nikomiru_core::config::SubtitleStudyPreset],
) {
    ctx.data_mut(|d| d.insert_temp(Id::unique("subtitle-named-presets"), saved.to_vec()));
}

impl State {
    fn capture_preset(&self) -> nikomiru_core::config::SubtitleStudyPreset {
        nikomiru_core::config::SubtitleStudyPreset {
            context: self.context,
            exact: self.exact,
            order: StudyOrder::ALL
                .iter()
                .position(|o| *o == self.order)
                .unwrap_or(0) as u8,
            name: self.preset_name.trim().to_string(),
            query: self.query.clone(),
            filter: match self.filter {
                LineFilter::All => 0,
                LineFilter::New => 1,
                LineFilter::PlusOne => 2,
            },
            from: self.window.from,
            to: self.window.to,
            min_speed: self.speed.min,
            max_speed: self.speed.max,
            min_duration: self.duration.min,
            max_duration: self.duration.max,
            min_text: self.text_length.min,
            max_text: self.text_length.max,
            unique: self.unique,
            shape: match self.shape {
                LineShape::Any => 0,
                LineShape::Single => 1,
                LineShape::Multiple => 2,
            },
        }
    }
    fn apply_preset(&mut self, p: &nikomiru_core::config::SubtitleStudyPreset) {
        if !p.valid() {
            return;
        }
        self.query.clone_from(&p.query);
        self.context = p.context;
        self.exact = p.exact;
        self.order = StudyOrder::ALL[usize::from(p.order)];
        self.filter =
            [LineFilter::All, LineFilter::New, LineFilter::PlusOne][usize::from(p.filter)];
        self.window = TimeWindow {
            from: p.from,
            to: p.to,
        };
        self.speed = ReadingSpeed {
            min: p.min_speed,
            max: p.max_speed,
        };
        self.duration = DurationBounds {
            min: p.min_duration,
            max: p.max_duration,
        };
        self.text_length = TextLength {
            min: p.min_text,
            max: p.max_text,
        };
        self.unique = p.unique;
        self.shape = [LineShape::Any, LineShape::Single, LineShape::Multiple][usize::from(p.shape)];
        self.rows.picked = None;
    }
}

fn study_presets(
    ui: &mut egui::Ui,
    state: &mut State,
    labels: &StudyLabels,
    out: &mut Vec<Command>,
) {
    egui::CollapsingHeader::new(&labels.preset_labels[0])
        .id_salt("subtitle-persistent-presets")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(&labels.portable[0]).clicked() {
                    out.push(Command::PresetsImport);
                }
                if ui.button(&labels.portable[1]).clicked() {
                    out.push(Command::PresetsExport);
                }
            });

            let saved = ui
                .ctx()
                .data(|d| {
                    d.get_temp::<Vec<nikomiru_core::config::SubtitleStudyPreset>>(Id::unique(
                        "subtitle-named-presets",
                    ))
                })
                .unwrap_or_default();
            ui.add(
                egui::TextEdit::singleline(&mut state.preset_name)
                    .char_limit(64)
                    .hint_text(&labels.preset_labels[1]),
            )
            .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &labels.preset_labels[1]));
            let p = state.capture_preset();
            if ui
                .add_enabled(
                    p.valid() && (saved.len() < 16 || saved.iter().any(|old| old.name == p.name)),
                    egui::Button::new(&labels.preset_labels[2]),
                )
                .clicked()
            {
                out.push(Command::SubtitleStudyPresetSave(p));
            }
            for p in saved {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(&p.name).clicked() {
                        state.apply_preset(&p);
                        ui.ctx().request_repaint();
                    }
                    if ui
                        .button(format!("{}: {}", labels.preset_labels[3], p.name))
                        .clicked()
                    {
                        out.push(Command::SubtitleStudyPresetDelete(p.name.clone()));
                    }
                });
            }
        });
}

fn editable_window(ui: &mut egui::Ui, scene: &Scene<'_>, state: &mut State) {
    egui::CollapsingHeader::new(&scene.study.time_labels[0])
        .id_salt("subtitle-editable-window")
        .show(ui, |ui| {
            if state
                .time_draft
                .as_ref()
                .is_none_or(|d| d.seen != state.window)
            {
                let format = |ms: Option<i64>| {
                    ms.map(|v| format!("{}.{:03}", v / 1000, v % 1000))
                        .unwrap_or_default()
                };
                state.time_draft = Some(TimeDraft {
                    seen: state.window,
                    from: format(state.window.from),
                    to: format(state.window.to),
                    error: false,
                });
            }
            let Some(draft) = &mut state.time_draft else {
                return;
            };
            for (label, text) in [
                (&scene.study.time_labels[1], &mut draft.from),
                (&scene.study.time_labels[2], &mut draft.to),
            ] {
                ui.horizontal(|ui| {
                    let label = ui.label(label);
                    ui.add(
                        TextEdit::singleline(text)
                            .char_limit(32)
                            .desired_width(180.0),
                    )
                    .labelled_by(label.id);
                });
            }
            if ui.button(&scene.study.time_labels[3]).clicked() {
                if let Some(window) = crate::subtitle_study::parse_window(&draft.from, &draft.to) {
                    state.window = window;
                    draft.seen = window;
                    draft.error = false;
                } else {
                    draft.error = true;
                }
            }
            if draft.error {
                ui.label(&scene.study.time_labels[4]);
            }
        });
}

fn study_tools(ui: &mut egui::Ui, scene: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    egui::CollapsingHeader::new(&scene.study.tools)
        .id_salt("subtitle-study-tools")
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("subtitle-study-tools-scroll")
                .max_height(ui.ctx().content_rect().height() * 0.5)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    study_presets(ui, state, &scene.study, out);
                    editable_window(ui, scene, state);
                    matching_navigation(ui, scene, state, out);
                    egui::CollapsingHeader::new(&scene.study.study_groups[0])
                        .id_salt("subtitle-study-group-0")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let label = ui.label(&scene.study.pause_label);
                                let mut pause = scene.practice_pause_ms;
                                let r = ui
                                    .add(
                                        egui::DragValue::new(&mut pause)
                                            .range(0..=10_000)
                                            .speed(250.0),
                                    )
                                    .labelled_by(label.id);
                                if r.changed() {
                                    out.push(Command::PracticePause(pause));
                                }
                            });

                            if !matches!(state.practice_passes, 2 | 3 | 5) {
                                state.practice_passes = 2;
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(&scene.study.queue_labels[2]);
                                for passes in [2, 3, 5] {
                                    ui.selectable_value(
                                        &mut state.practice_passes,
                                        passes,
                                        passes.to_string(),
                                    );
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                for (label, indices) in [
                                    (&scene.study.queue_labels[0], shown_indices(scene, state)),
                                    (
                                        &scene.study.queue_labels[1],
                                        state.selected.iter().copied().collect::<Vec<_>>(),
                                    ),
                                ] {
                                    if ui
                                        .add_enabled(
                                            scene.export_ready
                                                && !indices.is_empty()
                                                && indices.len() <= 1000,
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        out.push(Command::PracticeQueue {
                                            version: scene.version,
                                            indices,
                                            passes: state.practice_passes,
                                        });
                                    }
                                }
                            });

                            ui.horizontal_wrapped(|ui| {
                                let target = scene.timeline.active(scene.pos_ms).first().copied();
                                for passes in [2, 3, 5] {
                                    if ui
                                        .add_enabled(
                                            scene.export_ready && target.is_some(),
                                            egui::Button::new(format!(
                                                "{} ×{passes}",
                                                scene.study.practice_labels[0]
                                            )),
                                        )
                                        .clicked()
                                        && let Some(cue) = target
                                    {
                                        out.push(Command::Practice {
                                            version: scene.version,
                                            cue,
                                            passes,
                                        });
                                    }
                                }
                                if let Some(status) = &scene.practice_status {
                                    ui.label(status);
                                    if ui
                                        .add_enabled(
                                            scene.practice_left.is_some_and(|n| n > 0),
                                            egui::Button::new(&scene.study.practice_controls[0]),
                                        )
                                        .clicked()
                                    {
                                        out.push(Command::PracticeSkip);
                                    }
                                    if ui.button(&scene.study.practice_controls[1]).clicked() {
                                        out.push(Command::PracticeRestart);
                                    }
                                    if ui.button(&scene.study.practice_labels[1]).clicked() {
                                        out.push(Command::PracticeCancel);
                                    }
                                }
                            });
                        });
                    egui::CollapsingHeader::new(&scene.study.study_groups[1])
                        .id_salt("subtitle-study-group-1")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let shown = shown_indices(scene, state);
                                for (label, indices) in [
                                    (&scene.study.vocabulary_labels[0], shown),
                                    (
                                        &scene.study.vocabulary_labels[1],
                                        state.selected.iter().copied().collect(),
                                    ),
                                ] {
                                    if ui
                                        .add_enabled(
                                            scene.tabs.is_some()
                                                && scene.export_ready
                                                && !indices.is_empty(),
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        out.push(Command::VocabularyScope {
                                            version: scene.version,
                                            indices,
                                        });
                                    }
                                }
                            });
                        });
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .checkbox(&mut state.masked, &scene.study.mask_labels[0])
                            .changed()
                        {
                            state.revealed.clear();
                        }
                        if ui
                            .add_enabled(
                                state.masked,
                                egui::Button::new(&scene.study.mask_labels[1]),
                            )
                            .clicked()
                        {
                            state.revealed.extend(shown_indices(scene, state));
                        }
                        if ui
                            .add_enabled(
                                state.masked && !state.revealed.is_empty(),
                                egui::Button::new(&scene.study.mask_labels[2]),
                            )
                            .clicked()
                        {
                            state.revealed.clear();
                        }
                    });
                    egui::CollapsingHeader::new(&scene.study.text_labels[0])
                        .id_salt("subtitle-text-filters")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (order, label) in
                                    StudyOrder::ALL.into_iter().zip(&scene.study.order_labels)
                                {
                                    ui.selectable_value(&mut state.order, order, label);
                                }
                            });
                            ui.checkbox(&mut state.exact, &scene.study.exact_label);
                            ui.checkbox(&mut state.unique, &scene.study.unique_label);
                            ui.horizontal_wrapped(|ui| {
                                for (shape, label) in
                                    [LineShape::Any, LineShape::Single, LineShape::Multiple]
                                        .into_iter()
                                        .zip(&scene.study.shapes)
                                {
                                    ui.selectable_value(&mut state.shape, shape, label);
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                let mut min = state.text_length.min.is_some();
                                if ui.checkbox(&mut min, &scene.study.text_labels[1]).changed() {
                                    state.text_length.min = min.then_some(0);
                                }
                                if let Some(value) = &mut state.text_length.min {
                                    ui.add(egui::DragValue::new(value).range(0..=1_000_000))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label = Some(scene.study.text_labels[1].clone());
                                            info
                                        });
                                }
                                let mut max = state.text_length.max.is_some();
                                if ui.checkbox(&mut max, &scene.study.text_labels[2]).changed() {
                                    state.text_length.max = max.then_some(40);
                                }
                                if let Some(value) = &mut state.text_length.max {
                                    ui.add(egui::DragValue::new(value).range(0..=1_000_000))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label = Some(scene.study.text_labels[2].clone());
                                            info
                                        });
                                }
                                if ui.button(&scene.study.text_labels[3]).clicked() {
                                    state.text_length = TextLength::default();
                                }
                            });
                        });
                    egui::CollapsingHeader::new(&scene.study.duration_labels[0])
                        .id_salt("subtitle-duration-tools")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut min = state.duration.min.is_some();
                                if ui
                                    .checkbox(&mut min, &scene.study.duration_labels[1])
                                    .changed()
                                {
                                    state.duration.min = min.then_some(0);
                                }
                                if let Some(value) = &mut state.duration.min {
                                    ui.add(egui::DragValue::new(value).range(0..=86_400_000))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label =
                                                Some(scene.study.duration_labels[1].clone());
                                            info
                                        });
                                }
                                let mut max = state.duration.max.is_some();
                                if ui
                                    .checkbox(&mut max, &scene.study.duration_labels[2])
                                    .changed()
                                {
                                    state.duration.max = max.then_some(5000);
                                }
                                if let Some(value) = &mut state.duration.max {
                                    ui.add(egui::DragValue::new(value).range(0..=86_400_000))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label =
                                                Some(scene.study.duration_labels[2].clone());
                                            info
                                        });
                                }
                                if ui.button(&scene.study.duration_labels[3]).clicked() {
                                    state.duration = DurationBounds::default();
                                }
                            });
                        });
                    egui::CollapsingHeader::new(&scene.study.statistics)
                        .id_salt("subtitle-study-statistics")
                        .show(ui, |ui| study_statistics(ui, scene, state));
                    egui::CollapsingHeader::new(&scene.study.speed)
                        .id_salt("subtitle-reading-speed")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut min = state.speed.min.is_some();
                                if ui.checkbox(&mut min, &scene.study.min_speed).changed() {
                                    state.speed.min = min.then_some(0);
                                }
                                if let Some(value) = &mut state.speed.min {
                                    ui.add(egui::DragValue::new(value).range(0..=500))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label = Some(scene.study.min_speed.clone());
                                            info
                                        });
                                }
                                let mut max = state.speed.max.is_some();
                                if ui.checkbox(&mut max, &scene.study.max_speed).changed() {
                                    state.speed.max = max.then_some(20);
                                }
                                if let Some(value) = &mut state.speed.max {
                                    ui.add(egui::DragValue::new(value).range(0..=500))
                                        .widget_info(|| {
                                            let mut info =
                                                WidgetInfo::drag_value(true, *value as f64);
                                            info.label = Some(scene.study.max_speed.clone());
                                            info
                                        });
                                }
                                if ui
                                    .add_enabled(
                                        state.speed != ReadingSpeed::default(),
                                        egui::Button::new(&scene.study.reset_speed),
                                    )
                                    .clicked()
                                {
                                    state.speed = ReadingSpeed::default();
                                }
                            });
                        });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(&scene.study.from_here).clicked() {
                            state.window.from = Some(scene.pos_ms.max(0));
                        }
                        if let Some(from) = state.window.from {
                            ui.label(clock(from));
                        }
                        if ui.button(&scene.study.to_here).clicked() {
                            state.window.to = Some(scene.pos_ms.max(0));
                        }
                        if let Some(to) = state.window.to {
                            ui.label(clock(to));
                        }
                        if ui
                            .add_enabled(
                                state.window != TimeWindow::default(),
                                egui::Button::new(&scene.study.reset_window),
                            )
                            .clicked()
                        {
                            state.window = TimeWindow::default();
                        }
                    });
                    egui::CollapsingHeader::new(&scene.study.study_groups[2])
                        .id_salt("subtitle-study-group-2")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for (label, indices) in [
                                    (&scene.study.html_labels[0], shown_indices(scene, state)),
                                    (
                                        &scene.study.html_labels[1],
                                        state.selected.iter().copied().collect(),
                                    ),
                                ] {
                                    if ui
                                        .add_enabled(
                                            scene.bilingual_ready && !indices.is_empty(),
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        out.push(Command::SubtitleRowsExport {
                                            version: scene.version,
                                            indices,
                                            format: crate::subtitle_export::ExportFormat::Html,
                                        });
                                    }
                                }
                            });
                        });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(&scene.study.invert_shown).clicked() {
                            for i in shown_indices(scene, state) {
                                if !state.selected.remove(&i) {
                                    state.selected.insert(i);
                                }
                            }
                            state.selection_anchor = None;
                            state.selection_revision = state.selection_revision.wrapping_add(1);
                        }
                        if ui.button(&scene.study.select_shown).clicked() {
                            state.selected.extend(shown_indices(scene, state));
                            state.selection_revision = state.selection_revision.wrapping_add(1);
                        }
                        if ui
                            .add_enabled(
                                !state.selected.is_empty(),
                                egui::Button::new(&scene.study.clear),
                            )
                            .clicked()
                        {
                            state.selected.clear();
                            state.selection_revision = state.selection_revision.wrapping_add(1);
                        }
                    });
                    egui::CollapsingHeader::new(&scene.study.copy_export)
                        .id_salt("subtitle-copy-export-tools")
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                let mut copy = None;
                                if ui
                                    .add_enabled(
                                        scene.export_ready,
                                        egui::Button::new(&scene.study.copy_shown),
                                    )
                                    .clicked()
                                {
                                    copy = Some(shown_indices(scene, state));
                                }
                                if ui
                                    .add_enabled(
                                        scene.export_ready && !state.selected.is_empty(),
                                        egui::Button::new(&scene.study.copy_selected),
                                    )
                                    .clicked()
                                {
                                    copy = Some(state.selected.iter().copied().collect());
                                }
                                if let Some(indices) = copy {
                                    match crate::subtitle_export::transcript(
                                        scene.timeline.cues(),
                                        &indices,
                                        scene.timeline.delay_ms,
                                        scene.export_ready,
                                    ) {
                                        Ok(text) => {
                                            ui.ctx().copy_text(text);
                                            state.copied = Some(true);
                                        }
                                        Err(_) => state.copied = Some(false),
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        scene.export_ready && !state.selected.is_empty(),
                                        egui::Button::new(&scene.study.export_selected),
                                    )
                                    .clicked()
                                {
                                    out.push(Command::SubtitleRowsExport {
                                        version: scene.version,
                                        indices: state.selected.iter().copied().collect(),
                                        format: crate::subtitle_export::ExportFormat::Srt,
                                    });
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                for selected in [false, true] {
                                    let label = if selected {
                                        &scene.study.vtt_selected
                                    } else {
                                        &scene.study.vtt_shown
                                    };
                                    if ui
                                        .add_enabled(
                                            scene.export_ready
                                                && (!selected || !state.selected.is_empty()),
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        let indices = if selected {
                                            state.selected.iter().copied().collect()
                                        } else {
                                            shown_indices(scene, state)
                                        };
                                        out.push(Command::SubtitleRowsExport {
                                            version: scene.version,
                                            indices,
                                            format: crate::subtitle_export::ExportFormat::Vtt,
                                        });
                                    }
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                for selected in [false, true] {
                                    let label = if selected {
                                        &scene.study.bilingual_selected
                                    } else {
                                        &scene.study.bilingual_shown
                                    };
                                    if ui
                                        .add_enabled(
                                            scene.bilingual_ready
                                                && (!selected || !state.selected.is_empty()),
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        let indices = if selected {
                                            state.selected.iter().copied().collect()
                                        } else {
                                            shown_indices(scene, state)
                                        };
                                        out.push(Command::SubtitleRowsExport {
                                            version: scene.version,
                                            indices,
                                            format: crate::subtitle_export::ExportFormat::Bilingual,
                                        });
                                    }
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                for selected in [false, true] {
                                    let label = if selected {
                                        &scene.study.bilingual_srt_selected
                                    } else {
                                        &scene.study.bilingual_srt_shown
                                    };
                                    if ui
                                        .add_enabled(
                                            scene.bilingual_ready
                                                && (!selected || !state.selected.is_empty()),
                                            egui::Button::new(label),
                                        )
                                        .clicked()
                                    {
                                        let indices = if selected {
                                            state.selected.iter().copied().collect()
                                        } else {
                                            shown_indices(scene, state)
                                        };
                                        out.push(Command::SubtitleRowsExport {
                                            version: scene.version,
                                            indices,
                                            format:
                                                crate::subtitle_export::ExportFormat::BilingualSrt,
                                        });
                                    }
                                }
                            });
                            if let Some(ok) = state.copied {
                                ui.label(if ok {
                                    &scene.study.copied
                                } else {
                                    &scene.study.copy_error
                                });
                            }
                        });
                });
        });
}

fn rows(ui: &mut egui::Ui, scene: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let timeline = scene.timeline;
    let moving = ui.ui_contains_pointer()
        && ui.input(|i| i.smooth_scroll_delta != Vec2::ZERO || i.pointer.is_decidedly_dragging());
    ScrollArea::vertical()
        .id_salt("subtitle-list-rows")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let ctx = ui.ctx().clone();
            let width = ui.max_rect().width();
            let filter = if scene.tabs.is_some() {
                state.filter
            } else {
                LineFilter::All
            };
            state.rows.update(
                &ctx,
                scene,
                &state.query,
                filter,
                Scope {
                    window: state.window,
                    speed: state.speed,
                    duration: state.duration,
                    text_length: state.text_length,
                    unique: state.unique,
                    shape: state.shape,
                    context: state.context,
                    exact: state.exact,
                    order: state.order,
                },
                width,
            );
            let rows = &state.rows;
            ui.set_height(rows.tops.last().copied().unwrap_or(0.0));
            let origin = ui.max_rect().min;
            let current = rows.current_row(timeline, scene.pos_ms);

            let sizes = Sizes::of(scene.text_size);
            let text_width = (width - sizes.text_x - RIGHT).max(sizes.text);
            let painter = ui.painter().clone();
            #[cfg(test)]
            state.drawn.clear();
            let in_view = rows.between(viewport.min.y, viewport.max.y);
            let cues: Vec<usize> = rows.cues[in_view.clone()].to_vec();
            if cues != state.asked {
                state.asked.clone_from(&cues);
                out.push(Command::ListRows(cues));
            }
            for k in in_view {
                let cue = rows.cues[k];
                let rect = rows.rect(k, origin);
                if current == Some(k) {
                    painter.rect_filled(rect, 4.0, CURRENT);
                }
                let id = Id::unique(("subtitle-list-row", cue));

                let mut selected = state.selected.contains(&cue);
                let check = ui.put(
                    Rect::from_min_size(rect.min + vec2(PAD, PAD), vec2(18.0, 18.0)),
                    egui::Checkbox::new(&mut selected, ""),
                );
                check.widget_info(|| {
                    WidgetInfo::selected(
                        Role::CheckBox,
                        true,
                        selected,
                        format!("{} {}", scene.study.select_line, cue + 1),
                    )
                });
                if check.changed() {
                    state.selection_revision = state.selection_revision.wrapping_add(1);
                    select_range(
                        &mut state.selected,
                        &rows.cues,
                        &mut state.selection_anchor,
                        cue,
                        selected,
                        ui.input(|i| i.modifiers.shift),
                    );
                }

                let start = timeline.start_pos(cue);
                let time: Arc<Galley> = painter.layout_job(time_job(start, sizes.time));
                let time_rect = Rect::from_min_size(
                    rect.min + vec2(PAD + 22.0, PAD),
                    vec2(sizes.time_width, time.size().y),
                );
                painter.galley(time_rect.min, Arc::clone(&time), TIME);
                let label = time.text().to_string();
                let clicked = ui
                    .interact(time_rect, id.with("time"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                clicked.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
                if clicked.clicked() {
                    out.push(Command::ListTime(cue));
                }

                let available = translation_key(scene).is_some();
                let translation = ui
                    .add_enabled_ui(available, |ui| {
                        ui.put(
                            Rect::from_min_size(
                                rect.min + vec2(sizes.text_x - 22.0, PAD),
                                vec2(20.0, 20.0),
                            ),
                            egui::Button::new("文"),
                        )
                    })
                    .inner;
                translation.widget_info(|| {
                    WidgetInfo::labeled(
                        Role::Button,
                        available,
                        format!("{} {}", scene.study.translation_labels[1], cue + 1),
                    )
                });
                if translation.clicked()
                    && let (Some(key), Some(other)) =
                        (translation_key(scene), scene.secondary.as_ref())
                {
                    let text = crate::subtitle_export::translation_for(
                        timeline.cues(),
                        cue,
                        timeline.delay_ms,
                        other.cues,
                        other.delay,
                        true,
                    )
                    .map(|s| {
                        if s.is_empty() {
                            scene.study.translation_labels[2].clone()
                        } else {
                            s
                        }
                    })
                    .unwrap_or_else(|_| scene.study.copy_error.clone());
                    state.translation = Some(TranslationPreview { key, cue, text });
                }

                let hidden = state.masked && !state.revealed.contains(&cue);
                let reveal = format!("{} {}", scene.study.mask_labels[3], cue + 1);
                let plain = if hidden {
                    &reveal
                } else {
                    &timeline.cues()[cue].plain_text
                };
                let text: Arc<Galley> = painter.layout_job(text_job(plain, text_width, sizes.text));
                let at = rect.min + vec2(sizes.text_x, PAD);
                let text_rect = Rect::from_min_size(at, text.size());
                painter.galley(at, Arc::clone(&text), Color32::WHITE);
                let marks = if hidden { None } else { (scene.marks)(plain) }.unwrap_or_default();
                let underlines: Vec<(Rect, crate::words::Underline)> = marks
                    .underlines()
                    .flat_map(|(bytes, why)| {
                        let char_of = |b: usize| plain.get(..b).map_or(0, |s| s.chars().count());
                        char_rects(&text, char_of(bytes.start)..char_of(bytes.end))
                            .into_iter()
                            .map(move |r| (r, why))
                    })
                    .map(|(r, why)| {
                        let line = Rect::from_min_max(pos2(r.min.x, r.max.y - 2.0), r.max);
                        (line.translate(at.to_vec2()), why)
                    })
                    .collect();
                for (line, why) in &underlines {
                    painter.rect_filled(*line, 1.0, palette::underline(*why));
                }
                // KNW-003: a line with one new word, under its time
                if marks.plus_one {
                    let badge = painter.layout_no_wrap(
                        "+1".into(),
                        FontId::proportional(sizes.time * 0.9),
                        palette::underline(crate::words::Underline::Unknown),
                    );
                    painter.galley(
                        pos2(time_rect.min.x, time_rect.max.y + 2.0),
                        badge,
                        Color32::WHITE,
                    );
                }
                let clicked = ui
                    .interact(text_rect, id.with("text"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                let exact_hit = scene
                    .concordance
                    .is_some_and(|c| c.indices.binary_search(&cue).is_ok());
                let accessible = if exact_hit {
                    format!("{}: {plain}", scene.study.context_labels[3])
                } else {
                    plain.to_string()
                };
                if exact_hit && state.context > 0 {
                    painter.rect_stroke(
                        rect,
                        4.0,
                        egui::Stroke::new(1.0, TIME),
                        egui::StrokeKind::Inside,
                    );
                }
                clicked.widget_info(|| WidgetInfo::labeled(Role::Button, true, &accessible));
                if clicked.clicked() && hidden {
                    state.revealed.insert(cue);
                    ui.ctx().request_repaint();
                }
                if clicked.clicked()
                    && !hidden
                    && let Some(pointer) = clicked.interact_pointer_pos()
                    && let Some(pos) = char_at(&text, pointer - at)
                {
                    // the popup is placed next to the row (POP-006)
                    let anchor = popup::Rect {
                        x: f64::from(rect.min.x),
                        y: f64::from(rect.min.y),
                        w: f64::from(rect.width()),
                        h: f64::from(rect.height()),
                    };
                    out.push(Command::ListText { cue, pos, anchor });
                }
                #[cfg(test)]
                state.drawn.push(Drawn {
                    cue,
                    row: rect,
                    time: time_rect,
                    text: text_rect,
                    galley: text,
                    current: current == Some(k),
                    mined: underlines.len(),
                    plus_one: marks.plus_one,
                });
            }

            // the list follows the current row, unless the user is scrolling
            if let Some(k) = current {
                let follow = (rows.generation, k);
                if state.followed != Some(follow) {
                    state.followed = Some(follow);
                    if !moving {
                        ui.scroll_to_rect(rows.rect(k, origin), None);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use nikomiru_subs::Format;

    struct Test {
        timeline: Timeline,
        secondary: Option<Timeline>,
        concordance: Option<crate::subtitle_study::Concordance>,
        pos_ms: i64,
        state: State,
        focus: bool,
        closed: bool,
        out: Vec<Command>,
        /// `list_text_size` and `list_width` (SUB-023).
        sizes: (f32, f32),
        /// The header's score (KNW-007).
        score: Option<String>,
        /// The tabs and filters are shown (KNW-009, KNW-013).
        tabs: bool,
    }

    // T-SUB-035: hidden selections keep original indices and exports are explicit.
    #[test]
    fn selected_rows_survive_search_and_export_original_indices() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.get_by_label("sub-select-line 2").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-export-selected").scroll_to_me();
        h.run();
        h.get_by_label("sub-export-selected").click();
        h.run();
        assert!(h.state().out.iter().any(
            |c| matches!(c, Command::SubtitleRowsExport { version: 1, indices, .. } if indices == &[1])
        ));
        h.get_by_label("sub-selection-clear").scroll_to_me();
        h.run();
        h.get_by_label("sub-selection-clear").click();
        h.run();
        assert!(h.state().state.selected.is_empty());
    }

    // T-SUB-036: copy shown follows search; copy selected includes hidden ticks without changing playback.
    #[test]
    fn copying_a_transcript_is_explicit_and_preserves_hidden_selection() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.get_by_label("sub-select-line 2").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-shown").click();
        h.step();
        assert!(h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text.contains("猫") && !text.contains("犬"))));
        h.run();
        h.get_by_label("sub-copy-selected").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-selected").click();
        h.step();
        assert!(h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text.contains("犬") && !text.contains("猫"))));
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
    }

    // T-SUB-037: chosen WebVTT scopes carry original indices and the active timeline version.
    #[test]
    fn webvtt_actions_capture_the_requested_scope() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.get_by_label("sub-select-line 2").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-vtt-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-vtt-shown").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::SubtitleRowsExport { indices, format: crate::subtitle_export::ExportFormat::Vtt, .. } if indices == &[0])));
        h.get_by_label("sub-vtt-selected").scroll_to_me();
        h.run();
        h.get_by_label("sub-vtt-selected").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::SubtitleRowsExport { indices, format: crate::subtitle_export::ExportFormat::Vtt, .. } if indices == &[1])));
    }

    // T-SUB-038: the bilingual action uses the same filtered original indices.
    #[test]
    fn bilingual_action_captures_shown_rows() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-bilingual-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-bilingual-shown").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::SubtitleRowsExport { indices, format: crate::subtitle_export::ExportFormat::Bilingual, .. } if indices == &[0])));
    }

    // T-SUB-039: view and shown exports share a time window; hidden ticks survive until source changes.
    #[test]
    fn study_time_controls_filter_rows_without_seeking() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "猫"), (3000, "犬"), (6000, "鳥")]),
            2500,
            false,
        );
        h.run();
        h.get_by_label("sub-select-line 1").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 1").click();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-window-from").scroll_to_me();
        h.run();
        h.get_by_label("sub-window-from").click();
        h.run();
        h.state_mut().pos_ms = 5000;
        h.get_by_label("sub-window-to").scroll_to_me();
        h.run();
        h.get_by_label("sub-window-to").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-vtt-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-vtt-shown").click();
        h.run();
        assert!(
            h.state().out.iter().any(
                |c| matches!(c, Command::SubtitleRowsExport { indices, .. } if indices == &[1])
            )
        );
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ListTime(_)))
        );
        h.get_by_label("sub-window-reset").scroll_to_me();
        h.run();
        h.get_by_label("sub-window-reset").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1, 2]);
        h.state_mut().state.window.from = Some(1000);
        h.state_mut().state.selection_version = Some(0);
        h.run();
        assert!(h.state().state.selected.is_empty());
        assert_eq!(h.state().state.window, TimeWindow::default());
    }

    // T-KNW-037: entering examples switches tabs, clears the word query and scopes original rows.
    #[test]
    fn concordance_scopes_lines_and_can_be_cleared() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "猫"), (3000, "食べた"), (6000, "犬")]),
            0,
            false,
        );
        h.run();
        h.state_mut().state.words_tab = true;
        h.state_mut().state.query = "たべる".into();
        h.state_mut().concordance = Some(crate::subtitle_study::Concordance {
            key: "食べる\nたべる".into(),
            label: "食べる".into(),
            indices: vec![1],
            version: 1,
            revision: 7,
        });
        h.run();
        assert!(!h.state().state.words_tab);
        assert!(h.state().state.query.is_empty());
        assert_eq!(h.state().state.rows.cues, [1]);
        h.get_by_label("vocabulary-examples-clear").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SubtitleExamplesClear))
        );
        h.state_mut().concordance = None;
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1, 2]);
    }

    // T-SUB-059: explicit translation hints preserve masked primary text and invalidate on delay/source changes.
    #[test]
    fn translation_preview_does_not_reveal_primary_listening_text() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "秘密の猫")]), 0, false);
        h.state_mut().secondary = Some(timeline(&[(1000, "a secret cat")]));
        h.state_mut().state.masked = true;
        h.run();
        h.get_by_label("sub-preview-show 1").click();
        h.run();
        h.get_by_label("a secret cat");
        assert!(h.query_by_label("秘密の猫").is_none());
        assert!(h.state().state.revealed.is_empty());
        h.state_mut().timeline.delay_ms = 100;
        h.run();
        assert!(h.state().state.translation.is_none());
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ListTime(_)))
        );
    }

    // T-SUB-058: invalid drafts leave the active scope untouched until a complete valid Apply.
    #[test]
    fn time_window_drafts_apply_together_and_keep_hidden_ticks() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(0);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-time-edit").scroll_to_me();
        h.run();
        h.get_by_label("sub-time-edit").click();
        h.run_steps(3);
        {
            let d = h.state_mut().state.time_draft.as_mut().unwrap();
            d.from = "3".into();
            d.to = "2".into();
        }
        h.get_by_label("sub-time-apply").scroll_to_me();
        h.run();
        h.get_by_label("sub-time-apply").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1]);
        h.get_by_label("sub-time-invalid");
        h.state_mut().state.time_draft.as_mut().unwrap().to = "4".into();
        h.get_by_label("sub-time-apply").scroll_to_me();
        h.run();
        h.get_by_label("sub-time-apply").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
    }

    // T-SUB-057: resorting maps current highlights to original cue identities and preserves ticks.
    #[test]
    fn reordered_study_rows_track_original_current_cues() {
        let mut h = harness(
            timeline(&[(1000, "猫猫"), (3000, "犬"), (5000, "鳥鳥鳥")]),
            3500,
            false,
        );
        h.run();
        h.state_mut().state.selected.insert(2);
        h.state_mut().state.order = StudyOrder::Text;
        h.run();
        assert_eq!(h.state().state.rows.cues, [1, 0, 2]);
        assert_eq!(
            h.state()
                .state
                .drawn
                .iter()
                .find(|r| r.current)
                .map(|r| r.cue),
            Some(1)
        );
        assert_eq!(h.state().state.selected, BTreeSet::from([2]));
        assert_eq!(h.state().state.capture_preset().order, 3);
    }

    // T-SUB-056: whole-caption matching shares normalisation and every shown-snapshot predicate.
    #[test]
    fn exact_caption_search_preserves_original_text_and_ticks() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "ネコ"), (3000, "ネコです")]), 0, false);
        h.run();
        h.state_mut().state.query = "ﾈｺ".into();
        h.state_mut().state.selected.insert(1);
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1]);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-text-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-text-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-search-exact").scroll_to_me();
        h.run();
        h.get_by_label("sub-search-exact").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0]);
        assert!(h.state().state.capture_preset().exact);
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
        h.state_mut().state.query = "".into();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1]);
    }

    // T-SUB-055: queue buttons capture filtered original indices and preserve hidden-selected order.
    #[test]
    fn practice_queue_actions_capture_the_requested_scope() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(1);
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-practice-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-practice-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-practice-selected").scroll_to_me();
        h.run();
        h.get_by_label("sub-practice-selected").click();
        h.run();
        assert!(h.state().out.iter().any(
            |c| matches!(c,Command::PracticeQueue { version:1,indices,passes:2 } if indices == &[1])
        ));
        h.get_by_label("sub-practice-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-practice-shown").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c,Command::PracticeQueue { indices, .. } if indices == &[0]))
        );
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
    }

    // T-SUB-053: context scopes preserve exact hit identity and compose later text filters.
    #[test]
    fn word_examples_offer_neighbours_and_identify_exact_hits() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "前"), (3000, "猫"), (5000, "後")]),
            0,
            false,
        );
        h.run();
        h.state_mut().concordance = Some(crate::subtitle_study::Concordance {
            key: "猫\nねこ".into(),
            label: "猫".into(),
            indices: vec![1],
            version: 1,
            revision: 1,
        });
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        h.get_by_label("sub-context-one").scroll_to_me();
        h.run();
        h.get_by_label("sub-context-one").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1, 2]);
        h.get_by_label("sub-context-hit: 猫");
        h.state_mut().state.query = "後".into();
        h.run();
        assert_eq!(h.state().state.rows.cues, [2]);
        assert_eq!(h.state().state.capture_preset().context, 1);
    }

    // T-SUB-052: explicit preset application restores scopes without selection/reveal identities.
    #[test]
    fn subtitle_presets_restore_filters_and_preserve_current_ticks() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬\n猫")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(0);
        h.state_mut().state.masked = true;
        h.state_mut().state.revealed.insert(0);
        sync_study_presets(
            &h.ctx,
            &[nikomiru_core::config::SubtitleStudyPreset {
                name: "Two lines".into(),
                shape: 2,
                query: "猫".into(),
                ..Default::default()
            }],
        );
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-presets").scroll_to_me();
        h.run();
        h.get_by_label("sub-presets").click();
        h.run_steps(3);
        h.get_by_label("Two lines").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
        assert!(h.state().state.masked && h.state().state.revealed.contains(&0));
        let captured = h.state().state.capture_preset();
        assert_eq!(captured.shape, 2);
        h.get_by_label("vocabulary-preset-delete: Two lines")
            .click();
        h.run();
        assert!(
            h.state().out.iter().any(
                |c| matches!(c,Command::SubtitleStudyPresetDelete(name) if name == "Two lines")
            )
        );
    }

    // T-SUB-051: shape changes invalidate rows and keep hidden selections.
    #[test]
    fn multiline_scope_composes_with_other_filters() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "猫\n犬")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(0);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-text-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-text-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-shape-multiple").scroll_to_me();
        h.run();
        h.get_by_label("sub-shape-multiple").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        h.state_mut().state.query = "鳥".into();
        h.run();
        assert!(h.state().state.rows.cues.is_empty());
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
    }

    // T-SUB-050: first occurrence is chosen after other filters, without rewriting original text/indices.
    #[test]
    fn unique_captions_follow_the_filtered_scope_and_preserve_hidden_ticks() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "猫 犬"), (3000, "猫  犬"), (5000, "猫 犬！")]),
            0,
            false,
        );
        h.run();
        h.state_mut().state.selected.insert(1);
        h.state_mut().state.unique = true;
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 2]);
        h.state_mut().state.window.from = Some(2500);
        h.run();
        assert_eq!(h.state().state.rows.cues, [1, 2]);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-text-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-text-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-unique-captions").scroll_to_me();
        h.run();
        h.get_by_label("sub-unique-captions").click();
        h.run();
        assert!(!h.state().state.unique);
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
        h.state_mut().state.window = Default::default();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1, 2]);
    }

    // T-SUB-049: cached list rows and captured shown exports share text bounds.
    #[test]
    fn caption_length_filters_keep_hidden_ticks_and_reset() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬 🐈")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(0);
        h.state_mut().state.text_length.min = Some(2);
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-vtt-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-vtt-shown").click();
        h.run();
        assert!(
            h.state().out.iter().any(
                |c| matches!(c, Command::SubtitleRowsExport { indices, .. } if indices == &[1])
            )
        );
        h.get_by_label("sub-text-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-text-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-text-reset").scroll_to_me();
        h.run();
        h.get_by_label("sub-text-reset").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1]);
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
    }

    // T-SUB-048: repeat actions bind the active original cue and explicit finite count.
    #[test]
    fn repeat_current_line_captures_version_and_original_index() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 3500, false);
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-practice-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-practice-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-practice-current ×3").scroll_to_me();
        h.run();
        h.get_by_label("sub-practice-current ×3").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(
            c,
            Command::Practice {
                version: 1,
                cue: 1,
                passes: 3
            }
        )));
    }

    // T-SUB-047: range order follows shown identities, retains hidden ticks and resets missing anchors.
    #[test]
    fn range_selection_uses_shown_order_and_inversion_preserves_hidden_ticks() {
        use egui_kittest::kittest::Queryable;
        let mut selected = BTreeSet::from([99]);
        let mut anchor = Some(7);
        select_range(&mut selected, &[7, 2, 9, 1], &mut anchor, 9, true, true);
        assert_eq!(selected, BTreeSet::from([7, 2, 9, 99]));
        select_range(&mut selected, &[7, 2, 9, 1], &mut anchor, 2, false, true);
        assert_eq!(selected, BTreeSet::from([9, 99]));
        select_range(&mut selected, &[4, 5], &mut anchor, 5, true, true);
        assert_eq!(anchor, Some(5));
        assert_eq!(selected, BTreeSet::from([5, 9, 99]));
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(1);
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-invert-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-invert-shown").click();
        h.run();
        assert_eq!(h.state().state.selected, BTreeSet::from([0, 1]));
        h.get_by_label("sub-invert-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-invert-shown").click();
        h.run();
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
        h.state_mut().state.selection_anchor = Some(1);
        h.state_mut().state.selection_version = Some(0);
        h.run();
        assert_eq!(h.state().state.selection_anchor, None);
    }

    // T-KNW-044: scope requests freeze original filtered/selected indices without changing cue ticks.
    #[test]
    fn vocabulary_scope_actions_capture_hidden_selected_rows() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, true);
        h.state_mut().tabs = true;
        h.run();
        h.state_mut().state.selected.insert(1);
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-vocabulary-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-vocabulary-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-vocabulary-selected").scroll_to_me();
        h.run();
        h.get_by_label("sub-vocabulary-selected").click();
        h.run();
        assert!(h.state().out.iter().any(
            |c| matches!(c, Command::VocabularyScope { version: 1, indices } if indices == &[1])
        ));
        h.get_by_label("sub-vocabulary-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-vocabulary-shown").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::VocabularyScope { indices, .. } if indices == &[0]))
        );
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
    }

    // T-SUB-045: hidden text never enters accessible labels/painted galleys or lookup commands.
    #[test]
    fn listening_mask_reveals_explicitly_and_resets_on_source_change() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "秘密の猫"), (3000, "秘密の犬")]),
            0,
            false,
        );
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-mask-text").scroll_to_me();
        h.run();
        h.get_by_label("sub-mask-text").click();
        h.run();
        assert!(h.query_by_label("秘密の猫").is_none());
        assert!(
            h.state()
                .state
                .drawn
                .iter()
                .all(|r| !r.galley.text().contains("秘密"))
        );
        h.get_by_label("sub-reveal-line 1").scroll_to_me();
        h.run();
        h.get_by_label("sub-reveal-line 1").click();
        h.run();
        h.get_by_label("秘密の猫");
        assert!(h.query_by_label("秘密の犬").is_none());
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ListText { .. }))
        );
        h.get_by_label("sub-reveal-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-reveal-shown").click();
        h.run();
        h.get_by_label("秘密の犬");
        h.state_mut().state.selection_version = Some(0);
        h.run();
        assert!(h.query_by_label("秘密の猫").is_none());
        assert!(h.state().state.revealed.is_empty());
    }

    // T-SUB-044: duration predicates update cached rows and the export snapshot scope.
    #[test]
    fn duration_scope_matches_shown_indices_and_resets() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.selected.insert(0);
        h.state_mut().state.duration.max = Some(999);
        h.run();
        assert!(h.state().state.rows.cues.is_empty());
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-duration-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-duration-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-duration-reset").scroll_to_me();
        h.run();
        h.get_by_label("sub-duration-reset").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1]);
    }

    // T-SUB-040: speed scopes apply to visible rows and reset without affecting hidden ticks or playback.
    #[test]
    fn reading_speed_filters_rows_and_resets_explicitly() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "猫"), (3000, "犬犬"), (6000, "鳥鳥鳥鳥")]),
            0,
            false,
        );
        h.run();
        h.get_by_label("sub-select-line 1").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 1").click();
        h.run();
        h.state_mut().state.speed = ReadingSpeed {
            min: Some(2),
            max: Some(3),
        };
        h.run();
        assert_eq!(h.state().state.rows.cues, [1]);
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-speed-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-speed-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-speed-reset").scroll_to_me();
        h.run();
        h.get_by_label("sub-speed-reset").click();
        h.run();
        assert_eq!(h.state().state.rows.cues, [0, 1, 2]);
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ListTime(_)))
        );
    }

    // T-SUB-041: accessible metrics use the shown filter and hidden selection, and refresh with the query.
    #[test]
    fn scope_statistics_refresh_and_include_hidden_selected_lines() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫猫"), (3000, "犬犬")]), 0, false);
        h.run();
        h.get_by_label("sub-select-line 2").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-stats-title").scroll_to_me();
        h.run();
        h.get_by_label("sub-stats-title").click();
        h.run_steps(3);
        assert!(h.query_by_label("sub-stats-shown: 1 sub-stats-lines · 2 sub-stats-characters · sub-stats-dialogue: 0:01.000").is_some());
        assert!(h.query_by_label("sub-stats-selected: 1 sub-stats-lines · 2 sub-stats-characters · sub-stats-dialogue: 0:01.000").is_some());
        h.state_mut().state.query.clear();
        h.run();
        assert!(h.query_by_label("sub-stats-shown: 2 sub-stats-lines · 4 sub-stats-characters · sub-stats-dialogue: 0:02.000").is_some());
    }

    // T-SUB-042: bilingual SRT buttons capture their shown/selected scopes explicitly.
    #[test]
    fn bilingual_srt_action_uses_the_requested_scope() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫"), (3000, "犬")]), 0, false);
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-export").scroll_to_me();
        h.run();
        h.get_by_label("sub-copy-export").click();
        h.run_steps(3);
        h.get_by_label("sub-bilingual-srt-shown").scroll_to_me();
        h.run();
        h.get_by_label("sub-bilingual-srt-shown").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::SubtitleRowsExport { indices, format:crate::subtitle_export::ExportFormat::BilingualSrt, .. } if indices == &[0])));
    }

    // T-SUB-043: explicit navigation follows current filters and preserves hidden ticks.
    #[test]
    fn matching_navigation_uses_current_shown_scope() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(
            timeline(&[(1000, "猫"), (3000, "犬"), (5000, "鳥")]),
            2500,
            false,
        );
        h.run();
        h.get_by_label("sub-select-line 1").scroll_to_me();
        h.run();
        h.get_by_label("sub-select-line 1").click();
        h.run();
        h.state_mut().state.query = "犬".into();
        h.run();
        h.get_by_label("sub-study-tools").scroll_to_me();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-next-match").scroll_to_me();
        h.run();
        h.get_by_label("sub-next-match").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ListTime(1)))
        );
        assert_eq!(h.state().state.selected, BTreeSet::from([0]));
    }

    fn timeline(cues: &[(i64, &str)]) -> Timeline {
        let srt: String = cues
            .iter()
            .enumerate()
            .map(|(i, (start, text))| {
                let at = |ms: i64| {
                    format!(
                        "{:02}:{:02}:{:02},{:03}",
                        ms / 3_600_000,
                        ms / 60_000 % 60,
                        ms / 1000 % 60,
                        ms % 1000
                    )
                };
                format!(
                    "{}\n{} --> {}\n{text}\n\n",
                    i + 1,
                    at(*start),
                    at(start + 1000)
                )
            })
            .collect();
        Timeline::new(nikomiru_subs::parse(Format::Srt, &srt).cues)
    }

    /// A harness drawing the list over a click area like the video's.
    fn harness(timeline: Timeline, pos_ms: i64, focus: bool) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 720.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let video = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    if video.clicked() {
                        t.out.push(Command::VideoClicked);
                    }
                    let scene = Scene {
                        timeline: &t.timeline,
                        secondary: t.secondary.as_ref().map(|t| SecondaryScene {
                            cues: t.cues(),
                            delay: t.delay_ms,
                            version: 1,
                            complete: true,
                        }),
                        version: 1,
                        concordance: t.concordance.as_ref(),
                        pos_ms: t.pos_ms,
                        export_ready: !t.timeline.cues().is_empty(),
                        bilingual_ready: !t.timeline.cues().is_empty(),
                        export_label: "Export SRT".into(),
                        study: StudyLabels::translated(&|key, _| key.to_string()),
                        reading: None,
                        reading_cancel: "Cancel reading".into(),
                        title: "Subtitle list".into(),
                        score: t.score.clone(),
                        tabs: t.tabs.then(|| ("Lines".into(), "Words".into())),
                        filters: ["All", "New", "+1"].map(String::from),
                        marks_version: 0,
                        word_scope: None,
                        practice_status: None,
                        practice_pause_ms: 0,
                        practice_left: None,
                        words: None,
                        word_labels: Default::default(),
                        search_hint: "Search".into(),
                        close: "Close".into(),
                        marks: &|text: &str| {
                            let mined: Vec<Range<usize>> = text
                                .match_indices('猫')
                                .map(|(i, m)| i..i + m.len())
                                .collect();
                            // 鯨 is a word not known (KNW-013)
                            let unknown: Vec<Range<usize>> = text
                                .match_indices('鯨')
                                .map(|(i, m)| i..i + m.len())
                                .collect();
                            Some(crate::words::Marks {
                                plus_one: mined.len() == 1,
                                mined,
                                unknown,
                                ..Default::default()
                            })
                        },
                        text_size: t.sizes.0,
                        width: t.sizes.1,
                    };
                    if !draw(
                        ui.ctx(),
                        &scene,
                        &mut t.state,
                        &mut t.focus,
                        0.0,
                        &mut t.out,
                    ) {
                        t.closed = true;
                    }
                },
                Test {
                    timeline,
                    secondary: None,
                    concordance: None,
                    pos_ms,
                    state: State::default(),
                    focus,
                    closed: false,
                    out: Vec::new(),
                    sizes: (16.0, 420.0),
                    score: None,
                    tabs: false,
                },
            )
    }

    fn click(h: &mut Harness<'_, Test>, at: Pos2) -> Vec<Command> {
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        // the rows in view asked for their words are not the click's
        std::mem::take(&mut h.state_mut().out)
            .into_iter()
            .filter(|c| !matches!(c, Command::ListRows(_)))
            .collect()
    }

    fn drawn(h: &Harness<'_, Test>) -> Vec<(usize, bool)> {
        h.state()
            .state
            .drawn
            .iter()
            .map(|d| (d.cue, d.current))
            .collect()
    }

    // T-KNW-007: the score under the title, when there is one
    #[test]
    fn the_header_says_the_score() {
        let mut h = harness(timeline(&[(1000, "一つ目")]), 0, false);
        h.run();
        use egui_kittest::kittest::Queryable;
        assert!(h.query_by_label("Known: 50%").is_none());
        h.state_mut().score = Some("Known: 50%".into());
        h.run();
        h.get_by_label("Known: 50%");
    }

    // T-KNW-013: the lines with a new word, or with one, while the words
    // are coloured
    #[test]
    fn the_lines_are_filtered_by_their_new_words() {
        use egui_kittest::kittest::Queryable;
        let t = timeline(&[(1000, "鯨だ"), (3000, "猫だ"), (5000, "鳥だ")]);
        let mut h = harness(t, 0, false);
        h.run();
        assert!(
            h.query_by_label("New").is_none(),
            "no filters without colours"
        );
        h.state_mut().tabs = true;
        h.run();
        assert_eq!(drawn(&h).len(), 3);
        h.get_by_label("New").click();
        h.run();
        assert_eq!(drawn(&h), [(0, false)]);
        h.get_by_label("+1").click();
        h.run();
        assert_eq!(drawn(&h), [(1, false)]);
        h.get_by_label("All").click();
        h.run();
        assert_eq!(drawn(&h).len(), 3);
    }

    // T-SUB-015
    #[test]
    fn the_current_row_follows_playback_and_survives_the_search() {
        let t = timeline(&[
            (1000, "一つ目"),
            (3000, "二つ目 apple"),
            (5000, "三つ目 Apple"),
        ]);
        let mut h = harness(t, 3500, true);
        h.run();
        assert_eq!(drawn(&h), [(0, false), (1, true), (2, false)]);
        // in the gap after a cue the row stays on it
        h.state_mut().pos_ms = 4500;
        h.run();
        assert_eq!(drawn(&h), [(0, false), (1, true), (2, false)]);
        // the search field has the keyboard (Ctrl+F): typing filters, case aside
        h.event(egui::Event::Text("APPLE".into()));
        h.run();
        assert_eq!(drawn(&h), [(1, true), (2, false)]);
        // before the first hit nothing is current
        h.state_mut().pos_ms = 2000;
        h.run();
        assert_eq!(drawn(&h), [(1, false), (2, false)]);
    }

    // T-SUB-034: current complete text offers an accessible export action.
    #[test]
    fn complete_list_offers_srt_export() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(timeline(&[(1000, "猫")]), 0, false);
        h.run();
        h.get_by_label("Export SRT").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SubtitleExport))
        );
    }

    // T-KNW-026: filtering keeps original cue indices, timing and spelling.
    #[test]
    fn normalised_search_keeps_original_subtitle_targets() {
        use egui_kittest::kittest::Queryable;
        let t = timeline(&[(1000, "犬"), (3000, "ガッコウ ＡＢＣ"), (5000, "学校 abc")]);
        let mut h = harness(t, 3500, false);
        h.state_mut().state.query = "abc ｶﾞｯｺｳ".into();
        h.run_steps(2);
        assert_eq!(drawn(&h), [(1, true)]);
        h.get_by_label("ガッコウ ＡＢＣ");
        h.state_mut().state.query = "　 ".into();
        h.run_steps(2);
        assert_eq!(drawn(&h).len(), 3);
    }

    // T-SUB-023: bigger text, taller rows and a wider time column; a wider
    // list, but never past 60 % of the window
    #[test]
    fn the_list_takes_its_text_size_and_width() {
        let t = timeline(&[(1000, "一つ目"), (3000, "二つ目")]);
        let mut h = harness(t, 0, false);
        h.run();
        let first = |h: &Harness<'_, Test>| h.state().state.drawn[0].clone();
        let small = first(&h);
        h.state_mut().sizes = (24.0, 900.0);
        h.run();
        h.run();
        let big = first(&h);
        assert!(big.row.height() > small.row.height() * 1.3, "{big:?}");
        assert!(big.time.width() > small.time.width() * 1.4);
        assert!(big.text.min.x < small.text.min.x, "the list is wider");
        let window = h.ctx.content_rect();
        assert!(
            (big.row.width() - (0.6 * window.width() - 20.0)).abs() < 2.0,
            "{big:?}"
        );
    }

    #[test]
    fn escape_clears_the_search_then_closes_the_list() {
        let t = timeline(&[(1000, "一つ目"), (3000, "二つ目")]);
        let mut h = harness(t, 0, true);
        h.run();
        h.event(egui::Event::Text("二".into()));
        h.run();
        assert_eq!(drawn(&h), [(1, false)]);
        h.key_press(Key::Escape);
        h.run();
        assert!(!h.state().closed);
        assert_eq!(h.state().state.query, "");
        assert_eq!(drawn(&h), [(0, false), (1, false)]);
        // the field kept the keyboard: Esc again closes
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().closed);
    }

    #[test]
    fn a_click_on_the_time_seeks_and_on_the_text_looks_up_that_character() {
        let t = timeline(&[(1000, "食べました"), (3000, "二つ目")]);
        let mut h = harness(t, 0, false);
        h.run();
        let rows = h.state().state.drawn.clone();
        let out = click(&mut h, rows[1].time.center());
        assert!(matches!(out[..], [Command::ListTime(1)]), "{out:?}");

        // べ, the second character; the popup goes next to the row
        let first = &rows[0];
        let glyph = &first.galley.rows[0].glyphs[1];
        let at = first.text.min + vec2(glyph.pos.x + glyph.advance_width / 2.0, 5.0);
        let out = click(&mut h, at);
        let anchor = popup::Rect {
            x: f64::from(first.row.min.x),
            y: f64::from(first.row.min.y),
            w: f64::from(first.row.width()),
            h: f64::from(first.row.height()),
        };
        assert!(
            matches!(out[..], [Command::ListText { cue: 0, pos: 1, anchor: a }] if a == anchor),
            "{out:?}"
        );

        // right of the text: nothing, and the video does not take the click
        let out = click(&mut h, pos2(first.text.max.x + 20.0, first.text.center().y));
        assert!(out.is_empty(), "{out:?}");
        // the header neither
        let out = click(&mut h, pos2(first.row.center().x, 15.0));
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_long_list_draws_only_the_rows_in_view_and_scrolls_to_the_current_one() {
        let cues: Vec<(i64, String)> = (0..3000)
            .map(|i| (i64::from(i) * 2000, format!("台詞 {i}")))
            .collect();
        let cues: Vec<(i64, &str)> = cues.iter().map(|(t, s)| (*t, s.as_str())).collect();
        let mut h = harness(timeline(&cues), 2000 * 2500 + 500, false);
        h.run();
        let rows = drawn(&h);
        assert!(rows.len() < 60, "{} rows drawn", rows.len());
        assert!(rows.contains(&(2500, true)), "{rows:?}");
        let current = h.state().state.drawn.iter().find(|d| d.current).cloned();
        let row = current.map(|d| d.row).unwrap_or(Rect::NOTHING);
        assert!(row.min.y >= 0.0 && row.max.y <= 720.0, "{row:?}");
    }

    // T-LKP-061, T-KNW-003
    #[test]
    fn the_words_already_mined_are_underlined_and_the_rows_in_view_asked() {
        let mut h = harness(
            timeline(&[(1000, "猫が好き"), (3000, "犬"), (5000, "猫と猫")]),
            0,
            false,
        );
        h.run();
        let mined: Vec<usize> = h.state().state.drawn.iter().map(|d| d.mined).collect();
        assert_eq!(mined, [1, 0, 2]);
        // T-KNW-003 the line with one new word marked
        let plus_one: Vec<bool> = h.state().state.drawn.iter().map(|d| d.plus_one).collect();
        assert_eq!(plus_one, [true, false, false]);
        let asked: Vec<_> = h
            .state()
            .out
            .iter()
            .filter_map(|c| match c {
                Command::ListRows(cues) => Some(cues.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(asked, [vec![0, 1, 2]], "once, until the rows change");
    }
}
