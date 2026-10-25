// SPDX-License-Identifier: GPL-3.0-only

//! The Statistics tab of the Library page (STA-005): the totals, a chart by
//! range, the calendar of the year, the series and the sessions, the CSV.

use std::collections::BTreeMap;

use egui::{Button, Color32, Frame, Margin, Rect, RichText, Sense, Ui, Vec2, vec2};
use nikomiru_library::stats::{DayTotal, date_of, day_number};

use super::popups::Translate;
use super::theme;
use crate::catalog::{LibraryEdit, Stats};
use crate::ui::Command;

const DETAILS: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
const ROW: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x0c);
const EMPTY: Color32 = Color32::from_rgb(0x26, 0x2b, 0x36);
const BAR: Color32 = Color32::from_rgb(0x6a, 0xa0, 0xff);
/// The calendar's shades, from a little to a lot of study.
const HEAT: [Color32; 4] = [
    Color32::from_rgb(0x2b, 0x45, 0x75),
    Color32::from_rgb(0x3d, 0x63, 0xa8),
    Color32::from_rgb(0x55, 0x86, 0xd8),
    Color32::from_rgb(0x8a, 0xb4, 0xff),
];

/// The ranges of the chart, in days.
pub(crate) const RANGES: [i64; 3] = [7, 30, 365];

/// What the tab keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The chart's range, an index of [`RANGES`].
    range: usize,
}

/// `1 h 20 min`, `23 min`, `< 1 min`.
pub(crate) fn duration(ms: i64, tr: &Translate<'_>) -> String {
    let minutes = ms / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => tr("stats-under-minute", &[]),
        (0, m) => tr("stats-minutes", &[("m", m.to_string())]),
        (h, m) => tr("stats-hours", &[("h", h.to_string()), ("m", m.to_string())]),
    }
}

fn total(days: &BTreeMap<i64, &DayTotal>, from: i64, to: i64) -> (i64, u64) {
    days.range(from..=to)
        .fold((0, 0), |(ms, c), (_, d)| (ms + d.duration_ms, c + d.chars))
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(text)
            .size(17.0)
            .family(theme::bold())
            .color(DETAILS),
    );
}

/// Draws the tab.
pub(crate) fn draw(
    ui: &mut Ui,
    stats: Option<&Stats>,
    today: i64,
    tr: &Translate<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let Some(stats) = stats else {
        ui.spinner();
        return;
    };
    let days: BTreeMap<i64, &DayTotal> = stats
        .days
        .iter()
        .filter_map(|d| Some((day_number(&d.date)?, d)))
        .collect();
    let summary = |label: &str, (ms, chars): (i64, u64)| {
        tr(
            label,
            &[("time", duration(ms, tr)), ("chars", chars.to_string())],
        )
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.label(
            RichText::new(summary("stats-today", total(&days, today, today))).family(theme::bold()),
        );
        ui.label(summary("stats-week", total(&days, today - 6, today)));
        ui.label(summary("stats-all", total(&days, i64::MIN, i64::MAX)));
    });

    // the chart by range
    section(ui, &tr("stats-chart", &[]));
    ui.horizontal(|ui| {
        for (i, n) in RANGES.iter().enumerate() {
            let label = tr("stats-range", &[("n", n.to_string())]);
            if ui.selectable_label(state.range == i, label).clicked() {
                state.range = i;
            }
        }
    });
    let span = RANGES[state.range.min(RANGES.len() - 1)];
    let first = today - span + 1;
    let most = days
        .range(first..=today)
        .map(|(_, d)| d.duration_ms)
        .max()
        .unwrap_or(0)
        .max(60_000);
    let width = ui.available_width().min(1000.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 140.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, ROW);
    let step = rect.width() / span as f32;
    let mut hovered = None;
    for (i, day) in (first..=today).enumerate() {
        let ms = days.get(&day).map_or(0, |d| d.duration_ms);
        let h = (rect.height() - 8.0) * ms as f32 / most as f32;
        let x = rect.left() + i as f32 * step;
        let bar = Rect::from_min_max(
            egui::pos2(x + step * 0.15, rect.bottom() - 4.0 - h),
            egui::pos2(x + step * 0.85, rect.bottom() - 4.0),
        );
        if ms > 0 {
            ui.painter().rect_filled(bar, 1.0, BAR);
        }
        if response
            .hover_pos()
            .is_some_and(|p| p.x >= x && p.x < x + step)
        {
            hovered = Some(day);
        }
    }
    if let Some(day) = hovered {
        let d = days.get(&day);
        let text = tr(
            "stats-day",
            &[
                ("date", date_of(day)),
                ("time", duration(d.map_or(0, |d| d.duration_ms), tr)),
                ("chars", d.map_or(0, |d| d.chars).to_string()),
            ],
        );
        response.on_hover_text(text);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(date_of(first)).color(DETAILS).small());
        ui.add_space((width - 140.0).max(0.0));
        ui.label(RichText::new(date_of(today)).color(DETAILS).small());
    });

    // the calendar of the year: a column per week, Monday on top
    section(ui, &tr("stats-calendar", &[]));
    const CELL: f32 = 12.0;
    let weeks = 53;
    // 1970-01-01 was a Thursday: day 0 is weekday 3 (Monday 0)
    let weekday = |day: i64| (day + 3).rem_euclid(7);
    let start = today - weekday(today) - (weeks - 1) * 7;
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(weeks as f32 * (CELL + 2.0), 7.0 * (CELL + 2.0)),
        Sense::hover(),
    );
    let mut hovered = None;
    for day in start..=today {
        let col = (day - start) / 7;
        let row = weekday(day);
        let min = rect.min + vec2(col as f32 * (CELL + 2.0), row as f32 * (CELL + 2.0));
        let cell = Rect::from_min_size(min, Vec2::splat(CELL));
        let minutes = days.get(&day).map_or(0, |d| d.duration_ms / 60_000);
        let color = match minutes {
            0 => EMPTY,
            1..15 => HEAT[0],
            15..30 => HEAT[1],
            30..60 => HEAT[2],
            _ => HEAT[3],
        };
        ui.painter().rect_filled(cell, 2.0, color);
        if response.hover_pos().is_some_and(|p| cell.contains(p)) {
            hovered = Some(day);
        }
    }
    if let Some(day) = hovered {
        let d = days.get(&day);
        response.on_hover_text(tr(
            "stats-day",
            &[
                ("date", date_of(day)),
                ("time", duration(d.map_or(0, |d| d.duration_ms), tr)),
                ("chars", d.map_or(0, |d| d.chars).to_string()),
            ],
        ));
    }

    // by series
    if !stats.series.is_empty() {
        section(ui, &tr("stats-series", &[]));
        egui::Grid::new("stats-series")
            .num_columns(2)
            .spacing(vec2(24.0, 4.0))
            .min_col_width(320.0)
            .show(ui, |ui| {
                for (name, ms, chars) in stats.series.iter().take(30) {
                    ui.add(egui::Label::new(name).truncate());
                    ui.label(
                        RichText::new(tr(
                            "stats-time-chars",
                            &[("time", duration(*ms, tr)), ("chars", chars.to_string())],
                        ))
                        .color(DETAILS),
                    );
                    ui.end_row();
                }
            });
    }

    // the sessions
    section(ui, &tr("stats-sessions", &[]));
    if stats.sessions.is_empty() {
        ui.label(RichText::new(tr("stats-none", &[])).color(DETAILS));
    }
    for (id, s, name) in &stats.sessions {
        Frame::new()
            .fill(ROW)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} {:02}:00", s.date, s.hour)).family(theme::bold()),
                    );
                    ui.add(egui::Label::new(name).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let delete = ui.add(Button::new(tr("stats-delete", &[])).small());
                        let label = tr(
                            "stats-delete-session",
                            &[("date", s.date.clone()), ("name", name.clone())],
                        );
                        delete.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &label)
                        });
                        if delete.clicked() {
                            out.push(Command::LibraryEdit(LibraryEdit::DeleteSession(*id)));
                        }
                        ui.label(
                            RichText::new(tr(
                                "stats-session",
                                &[
                                    ("time", duration(s.duration_ms, tr)),
                                    ("chars", s.chars.to_string()),
                                    ("lookups", s.lookups.to_string()),
                                    ("cards", s.cards.to_string()),
                                ],
                            ))
                            .color(DETAILS),
                        );
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Id;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_library::stats::Segment;

    struct Test {
        stats: Stats,
        today: i64,
        state: State,
        out: Vec<Command>,
    }

    // T-STA-005
    #[test]
    fn the_tab_shows_totals_and_deletes_a_session() {
        let today = day_number("2026-10-25").unwrap();
        let day = |date: &str, ms, chars| DayTotal {
            date: date.into(),
            duration_ms: ms,
            chars,
            ..DayTotal::default()
        };
        let stats = Stats {
            days: vec![
                day("2026-10-20", 600_000, 100),
                day("2026-10-25", 1_500_000, 400),
            ],
            series: vec![("Show".into(), 2_100_000, 500)],
            sessions: vec![(
                7,
                Segment {
                    date: "2026-10-25".into(),
                    hour: 21,
                    duration_ms: 1_500_000,
                    chars: 400,
                    ..Segment::default()
                },
                "Show - 01".into(),
            )],
        };
        let mut h = Harness::builder()
            .with_size(vec2(1280.0, 1200.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (_, v) in args {
                            text.push(' ');
                            text.push_str(v);
                        }
                        text
                    };
                    draw(ui, Some(&t.stats), t.today, &tr, &mut t.state, &mut t.out);
                },
                Test {
                    stats,
                    today,
                    state: State::default(),
                    out: Vec::new(),
                },
            );
        h.run();
        // today, the last 7 days, all time
        h.get_by_label("stats-today stats-minutes 25 400");
        h.get_by_label("stats-week stats-minutes 35 500");
        h.get_by_label("stats-all stats-minutes 35 500");
        h.get_by_label("stats-range 30").click();
        h.run();
        assert_eq!(h.state().state.range, 1);
        h.get_by_label("stats-delete-session 2026-10-25 Show - 01")
            .click();
        h.run();
        assert!(matches!(
            h.state().out.as_slice(),
            [Command::LibraryEdit(LibraryEdit::DeleteSession(7))]
        ));
        assert_eq!(
            duration(5_400_000, &|id, a| format!("{id}{a:?}")),
            "stats-hours[(\"h\", \"1\"), (\"m\", \"30\")]"
        );
    }
}
