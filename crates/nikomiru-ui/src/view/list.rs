// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle list (SUB-015): every cue with its time, the current one
//! highlighted, search, a click on the time seeks and a click on the text looks
//! the word up.
//!
//! A film has a few thousand cues: their heights are measured once per list,
//! query and width, and only the rows in view are laid out and drawn.

use std::sync::Arc;

use egui::text::{CCursorRange, LayoutJob};
use egui::{
    Area, Color32, CursorIcon, FontId, Frame, Galley, Id, Key, Margin, Order, Pos2, Rect, RichText,
    Role, ScrollArea, Sense, TextEdit, Vec2, WidgetInfo, pos2, vec2,
};

use nikomiru_subs::timeline::Timeline;

use super::theme;
use crate::popup;
use crate::session::Session;
use crate::subtitles::clock;
use crate::ui::Command;

const BACKGROUND: Color32 = Color32::from_rgba_unmultiplied_const(0x14, 0x14, 0x18, 0xe6);
const CURRENT: Color32 = Color32::from_rgba_unmultiplied_const(0x4a, 0x90, 0xe2, 0x3a);
const TIME: Color32 = Color32::from_rgb(0x9a, 0xb8, 0xff);
/// The time column, and where the text starts in a row.
const TIME_WIDTH: f32 = 52.0;
const TEXT_X: f32 = TIME_WIDTH + 10.0;
/// Inside a row: 4 around the cells, 6 more right of the text.
const PAD: f32 = 4.0;
const RIGHT: f32 = 6.0;
const ROW_SPACING: f32 = 2.0;
const TEXT_SIZE: f32 = 16.0;
const TIME_SIZE: f32 = 14.0;

/// The rows on show: their cues and where each one starts.
#[derive(Debug, Default)]
struct Rows {
    /// The list version and the query the rows were picked for.
    picked: Option<(u64, String)>,
    /// Changes whenever other rows are picked.
    generation: u64,
    /// The width the heights were measured at.
    width: f32,
    /// The cues shown, in order.
    cues: Vec<usize>,
    /// The top of each row in the list, and the list's height last.
    tops: Vec<f32>,
}

/// What the list keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    query: String,
    rows: Rows,
    /// The current row the list last scrolled to, with its rows' generation.
    followed: Option<(u64, usize)>,
    /// The search field had the keyboard in the last frame.
    search_focused: bool,
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
}

/// What the list shows: the session's part of it.
pub(crate) struct Scene<'a> {
    pub timeline: &'a Timeline,
    /// Changes whenever the cues change (`Session::sub_list_version`).
    pub version: u64,
    /// The playback position.
    pub pos_ms: i64,
    pub title: String,
    pub search_hint: String,
    pub close: String,
}

fn text_job(text: &str, width: f32) -> LayoutJob {
    LayoutJob::simple(
        text.to_string(),
        FontId::proportional(TEXT_SIZE),
        Color32::WHITE,
        width,
    )
}

fn time_job(ms: i64) -> LayoutJob {
    LayoutJob::simple_singleline(clock(ms), FontId::monospace(TIME_SIZE), TIME)
}

impl Rows {
    /// Picks the rows again when the cues or the query change, and measures
    /// them again when the width changes too.
    fn update(&mut self, ctx: &egui::Context, scene: &Scene<'_>, query: &str, width: f32) {
        let timeline = scene.timeline;
        let fresh = self
            .picked
            .as_ref()
            .is_none_or(|(v, q)| *v != scene.version || q != query);
        if fresh {
            self.cues = if query.is_empty() {
                (0..timeline.cues().len()).collect()
            } else {
                timeline.search(query)
            };
            self.picked = Some((scene.version, query.to_string()));
            self.generation += 1;
        } else if self.width == width {
            return;
        }
        self.width = width;
        let text_width = (width - TEXT_X - RIGHT).max(TEXT_SIZE);
        let cues = timeline.cues();
        self.tops = ctx.fonts_mut(|fonts| {
            let time = fonts.layout_job(time_job(0)).size().y;
            let mut top = 0.0;
            let mut tops = Vec::with_capacity(self.cues.len() + 1);
            for &i in &self.cues {
                tops.push(top);
                let text = fonts
                    .layout_job(text_job(&cues[i].plain_text, text_width))
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
    let scene = Scene {
        timeline: s.subs.timeline(),
        version: s.sub_list_version,
        pos_ms: (s.view.time_pos * 1000.0).round() as i64,
        title: tr("subtitle-list"),
        search_hint: tr("subtitle-search"),
        close: tr("subtitle-list-close"),
    };
    draw(ctx, &scene, state, search_focus, bar_height, out)
}

pub(crate) fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    state: &mut State,
    search_focus: &mut bool,
    bar_height: f32,
    out: &mut Vec<Command>,
) -> bool {
    let screen = ctx.content_rect();
    let width = 420.0_f32.min(0.4 * screen.width());
    let height = (screen.height() - bar_height).max(0.0);
    let mut open = true;
    Area::new(Id::unique("subtitle-list"))
        .order(Order::Middle)
        .fixed_pos(pos2(screen.right() - width, screen.top()))
        .show(ctx, |ui| {
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

                    open &= search(ui, state, search_focus, &scene.search_hint);
                    rows(ui, scene, state, out);
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
            state.rows.update(&ctx, scene, &state.query, width);
            let rows = &state.rows;
            ui.set_height(rows.tops.last().copied().unwrap_or(0.0));
            let origin = ui.max_rect().min;
            let current = timeline.list_row(&rows.cues, scene.pos_ms);

            let text_width = (width - TEXT_X - RIGHT).max(TEXT_SIZE);
            let painter = ui.painter().clone();
            #[cfg(test)]
            state.drawn.clear();
            for k in rows.between(viewport.min.y, viewport.max.y) {
                let cue = rows.cues[k];
                let rect = rows.rect(k, origin);
                if current == Some(k) {
                    painter.rect_filled(rect, 4.0, CURRENT);
                }
                let id = Id::unique(("subtitle-list-row", cue));

                let start = timeline.start_pos(cue);
                let time: Arc<Galley> = painter.layout_job(time_job(start));
                let time_rect =
                    Rect::from_min_size(rect.min + vec2(PAD, PAD), vec2(TIME_WIDTH, time.size().y));
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
                let text: Arc<Galley> = painter.layout_job(text_job(plain, text_width));
                let at = rect.min + vec2(TEXT_X, PAD);
                let text_rect = Rect::from_min_size(at, text.size());
                painter.galley(at, Arc::clone(&text), Color32::WHITE);
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
                        title: "Subtitle list".into(),
                        search_hint: "Search".into(),
                        close: "Close".into(),
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
                },
            )
    }

    fn click(h: &mut Harness<'_, Test>, at: Pos2) -> Vec<Command> {
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    fn drawn(h: &Harness<'_, Test>) -> Vec<(usize, bool)> {
        h.state()
            .state
            .drawn
            .iter()
            .map(|d| (d.cue, d.current))
            .collect()
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
}
