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
            text_x: 52.0 * k + 32.0,
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

/// The rows on show: their cues and where each one starts.
#[derive(Debug, Default)]
struct Rows {
    /// The list version, the query, the filter and, with a filter, the
    /// words' revision the rows were picked for.
    picked: Option<(u64, String, LineFilter, u64)>,
    /// Changes whenever other rows are picked.
    generation: u64,
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
    select_shown: String,
    clear: String,
    export_selected: String,
    select_line: String,
    copy_shown: String,
    copy_selected: String,
    copied: String,
    copy_error: String,
}

impl StudyLabels {
    fn translated(tr: &super::popups::Translate<'_>) -> Self {
        Self {
            tools: tr("sub-study-tools", &[]),
            select_shown: tr("sub-select-shown", &[]),
            clear: tr("sub-selection-clear", &[]),
            export_selected: tr("sub-export-selected", &[]),
            select_line: tr("sub-select-line", &[]),
            copy_shown: tr("sub-copy-shown", &[]),
            copy_selected: tr("sub-copy-selected", &[]),
            copied: tr("sub-transcript-copied", &[]),
            copy_error: tr("sub-export-unavailable", &[]),
        }
    }
}

/// What the list keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    query: String,
    rows: Rows,
    selected: BTreeSet<usize>,
    selection_version: Option<u64>,
    copied: Option<bool>,
    /// The current row the list last scrolled to, with its rows' generation.
    followed: Option<(u64, usize)>,
    /// The search field had the keyboard in the last frame.
    search_focused: bool,
    /// The cues of the rows in view, last asked for their words.
    asked: Vec<usize>,
    /// The Words tab is shown (KNW-009).
    words_tab: bool,
    /// The lines shown (KNW-013).
    filter: LineFilter,
    /// What it keeps.
    words: super::word_list::State,
    /// The rows drawn in the last frame.
    #[cfg(test)]
    drawn: Vec<Drawn>,
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
    /// Changes whenever the cues change (`Session::sub_list_version`).
    pub version: u64,
    /// The playback position.
    pub pos_ms: i64,
    pub title: String,
    /// How much of the subtitle the learner understands, or how far its
    /// count is (KNW-007).
    pub score: Option<String>,
    pub reading: Option<(String, Option<f32>)>,
    pub reading_cancel: String,
    pub export_ready: bool,
    pub export_label: String,
    pub study: StudyLabels,
    /// The Lines and Words tabs, while the words are coloured (KNW-009);
    /// the words once all lines are split.
    pub tabs: Option<(String, String)>,
    /// The filters of the lines, in the order of `LineFilter::ALL`
    /// (KNW-013), and the words' revision, which changes their marks.
    pub filters: [String; 3],
    pub marks_version: u64,
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
        width: f32,
    ) {
        let timeline = scene.timeline;
        let revision = if filter == LineFilter::All {
            0
        } else {
            scene.marks_version
        };
        let fresh = self.picked.as_ref().is_none_or(|(v, q, f, r)| {
            *v != scene.version || q != query || *f != filter || *r != revision
        });
        if fresh {
            self.cues = if query.is_empty() {
                (0..timeline.cues().len()).collect()
            } else {
                let query = nikomiru_text::search::Query::new(query);
                timeline
                    .cues()
                    .iter()
                    .enumerate()
                    .filter(|(_, cue)| query.matches(&[&cue.plain_text]))
                    .map(|(index, _)| index)
                    .collect()
            };
            if filter != LineFilter::All {
                let cues = timeline.cues();
                self.cues
                    .retain(|&i| filter.keeps((scene.marks)(&cues[i].plain_text).as_ref()));
            }
            self.picked = Some((scene.version, query.to_string(), filter, revision));
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
    let scene = Scene {
        timeline: s.subs.timeline(),
        version: s.sub_list_version,
        pos_ms: (s.view.time_pos * 1000.0).round() as i64,
        title: tr("subtitle-list"),
        score: score_line(s),
        reading: s.graphic_read_status(),
        reading_cancel: tr("sub-reading-cancel"),
        export_ready: s.subtitle_export_ready(),
        export_label: tr("sub-export"),
        study: StudyLabels::translated(&|key, args| s.translator().tr(key, args)),
        tabs: coloured.then(|| (tr("subtitle-list-lines"), tr("subtitle-list-words"))),
        filters: [
            tr("subtitle-lines-all"),
            tr("subtitle-lines-new"),
            tr("subtitle-lines-plus-one"),
        ],
        marks_version: s.words_revision(),
        words: words.as_deref().map(Vec::as_slice),
        word_labels: super::word_list::Labels::translated(
            &|key, args| s.translator().tr(key, args),
            ticked,
        ),
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
        state.copied = None;
        state.selection_version = Some(scene.version);
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
                    open &= search(ui, state, search_focus, &scene.search_hint);
                    if scene.tabs.is_some() && state.words_tab {
                        if let Some(all) = scene.words {
                            let query = state.query.clone();
                            super::word_list::show(
                                ui,
                                all,
                                &query,
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

fn shown_indices(scene: &Scene<'_>, state: &State) -> Vec<usize> {
    let query = nikomiru_text::search::Query::new(&state.query);
    scene
        .timeline
        .cues()
        .iter()
        .enumerate()
        .filter(|(_, cue)| {
            query.matches(&[&cue.plain_text])
                && (scene.tabs.is_none()
                    || state.filter.keeps((scene.marks)(&cue.plain_text).as_ref()))
        })
        .map(|(i, _)| i)
        .collect()
}

fn study_tools(ui: &mut egui::Ui, scene: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    egui::CollapsingHeader::new(&scene.study.tools)
        .id_salt("subtitle-study-tools")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(&scene.study.select_shown).clicked() {
                    state.selected.extend(shown_indices(scene, state));
                }
                if ui
                    .add_enabled(
                        !state.selected.is_empty(),
                        egui::Button::new(&scene.study.clear),
                    )
                    .clicked()
                {
                    state.selected.clear();
                }
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
                    });
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
            state.rows.update(&ctx, scene, &state.query, filter, width);
            let rows = &state.rows;
            ui.set_height(rows.tops.last().copied().unwrap_or(0.0));
            let origin = ui.max_rect().min;
            let current = timeline.list_row(&rows.cues, scene.pos_ms);

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
                    if selected {
                        state.selected.insert(cue);
                    } else {
                        state.selected.remove(&cue);
                    }
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

                let plain = &timeline.cues()[cue].plain_text;
                let text: Arc<Galley> = painter.layout_job(text_job(plain, text_width, sizes.text));
                let at = rect.min + vec2(sizes.text_x, PAD);
                let text_rect = Rect::from_min_size(at, text.size());
                painter.galley(at, Arc::clone(&text), Color32::WHITE);
                let marks = (scene.marks)(plain).unwrap_or_default();
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
                clicked.widget_info(|| WidgetInfo::labeled(Role::Button, true, plain));
                if clicked.clicked()
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
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-export-selected").click();
        h.run();
        assert!(h.state().out.iter().any(
            |c| matches!(c, Command::SubtitleRowsExport { version: 1, indices } if indices == &[1])
        ));
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
        h.get_by_label("sub-select-line 2").click();
        h.run();
        h.state_mut().state.query = "猫".into();
        h.run();
        h.get_by_label("sub-study-tools").click();
        h.run_steps(3);
        h.get_by_label("sub-copy-shown").click();
        h.step();
        assert!(h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text.contains("猫") && !text.contains("犬"))));
        h.run();
        h.get_by_label("sub-copy-selected").click();
        h.step();
        assert!(h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text.contains("犬") && !text.contains("猫"))));
        assert_eq!(h.state().state.selected, BTreeSet::from([1]));
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
                        version: 1,
                        pos_ms: t.pos_ms,
                        export_ready: !t.timeline.cues().is_empty(),
                        export_label: "Export SRT".into(),
                        study: StudyLabels::translated(&|key, _| key.to_string()),
                        reading: None,
                        reading_cancel: "Cancel reading".into(),
                        title: "Subtitle list".into(),
                        score: t.score.clone(),
                        tabs: t.tabs.then(|| ("Lines".into(), "Words".into())),
                        filters: ["All", "New", "+1"].map(String::from),
                        marks_version: 0,
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
