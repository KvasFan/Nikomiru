// SPDX-License-Identifier: GPL-3.0-only

//! The Statistics tab of the Library page (STA-005): the totals, a chart by
//! range, the calendar of the year, the series and the sessions, the CSV.

use std::collections::BTreeMap;

use egui::{Button, Color32, Frame, Margin, Rect, RichText, Sense, Ui, Vec2, vec2};
use nikomiru_library::stats::{DayTotal, date_of, day_number};

use super::popups::Translate;
use super::{palette, theme};
use crate::catalog::{LibraryEdit, Stats};
use crate::ui::Command;

/// The calendar's shade for a day of `minutes` of study: the accent,
/// fainter for a little.
fn heat(minutes: i64) -> Color32 {
    let p = palette::current();
    let share = match minutes {
        0 => return p.placeholder,
        1..15 => 0.3,
        15..30 => 0.5,
        30..60 => 0.75,
        _ => 1.0,
    };
    p.accent.gamma_multiply(share)
}

/// The ranges of the chart, in days.
pub(crate) const RANGES: [i64; 3] = [7, 30, 365];

/// What the tab keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The chart's range, an index of [`RANGES`].
    range: usize,
    history: Option<String>,
    query: String,
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
            .color(palette::current().details),
    );
}

/// Bars of `values`, the largest the full height, with a hover text each.
fn bars(ui: &mut Ui, values: &[u32], height: f32, hover: impl Fn(usize) -> String) {
    let most = values.iter().copied().max().unwrap_or(0).max(1);
    let width = ui.available_width().min(1000.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, palette::current().row);
    let step = rect.width() / values.len().max(1) as f32;
    for (i, n) in values.iter().enumerate() {
        let x = rect.left() + i as f32 * step;
        let h = (rect.height() - 8.0) * *n as f32 / most as f32;
        if *n > 0 {
            let bar = Rect::from_min_max(
                egui::pos2(x + step * 0.15, rect.bottom() - 4.0 - h),
                egui::pos2(x + step * 0.85, rect.bottom() - 4.0),
            );
            ui.painter()
                .rect_filled(bar, 1.0, palette::current().accent);
        }
    }
    if let Some(i) = response
        .hover_pos()
        .map(|p| ((p.x - rect.left()) / step) as usize)
        .filter(|i| *i < values.len())
    {
        response.on_hover_text(hover(i));
    }
}

/// The review's part of the tab (REV-008).
fn review_section(ui: &mut Ui, r: &nikomiru_library::review_log::ReviewStats, tr: &Translate<'_>) {
    use nikomiru_library::review_log::DAYS;
    section(ui, &tr("stats-review", &[]));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.label(
            RichText::new(tr(
                "stats-review-today",
                &[
                    ("count", r.today.0.to_string()),
                    ("kept", r.today.1.to_string()),
                ],
            ))
            .family(theme::bold()),
        );
        let retention = r.retention.map_or_else(
            || tr("stats-review-retention-none", &[]),
            |share| {
                tr(
                    "stats-review-retention",
                    &[("percent", format!("{:.0}", share * 100.0))],
                )
            },
        );
        ui.label(retention);
        ui.label(
            RichText::new(tr(
                "stats-review-cards",
                &[
                    ("new", r.new.to_string()),
                    ("learning", r.learning.to_string()),
                    ("young", r.young.to_string()),
                    ("mature", r.mature.to_string()),
                ],
            ))
            .color(palette::current().details),
        );
    });
    ui.label(RichText::new(tr("stats-review-days", &[])).color(palette::current().details));
    bars(ui, &r.days, 80.0, |i| {
        tr(
            "stats-review-day",
            &[
                ("ago", (DAYS as usize - 1 - i).to_string()),
                ("count", r.days[i].to_string()),
            ],
        )
    });
    ui.label(RichText::new(tr("stats-review-ahead", &[])).color(palette::current().details));
    bars(ui, &r.ahead, 60.0, |i| {
        tr(
            "stats-review-due",
            &[("in", i.to_string()), ("count", r.ahead[i].to_string())],
        )
    });
}

/// The words' part of the tab (KNW-010): how many by status, and the
/// words known over the last days.
fn words_section(ui: &mut Ui, w: &nikomiru_library::words::WordStats, tr: &Translate<'_>) {
    use nikomiru_library::words::WORD_DAYS;
    section(ui, &tr("stats-words", &[]));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        ui.label(
            RichText::new(tr(
                "stats-words-set",
                &[
                    ("known", w.known.to_string()),
                    ("learning", w.learning.to_string()),
                    ("ignored", w.ignored.to_string()),
                ],
            ))
            .family(theme::bold()),
        );
        if w.anki_known + w.anki_learning > 0 {
            ui.label(
                RichText::new(tr(
                    "stats-words-anki",
                    &[
                        ("known", w.anki_known.to_string()),
                        ("learning", w.anki_learning.to_string()),
                    ],
                ))
                .color(palette::current().details),
            );
        }
    });
    ui.label(RichText::new(tr("stats-words-days", &[])).color(palette::current().details));
    bars(ui, &w.known_by_day, 80.0, |i| {
        tr(
            "stats-words-day",
            &[
                ("ago", (WORD_DAYS as usize - 1 - i).to_string()),
                ("count", w.known_by_day[i].to_string()),
            ],
        )
    });
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
    if let Some(uid) = state.history.clone() {
        if let Some((_, title, total)) = stats.encounters.iter().find(|(u, _, _)| u == &uid)
            && let Some(rows) = stats.encounter_words.get(&uid)
        {
            if ui.button(tr("analysis-words-back", &[])).clicked() {
                state.history = None;
            }
            ui.heading(title);
            ui.label(tr(
                "encounter-history-coverage",
                &[
                    ("shown", rows.len().to_string()),
                    ("total", total.to_string()),
                ],
            ));
            let label = ui.label(tr("books-words-search", &[]));
            ui.add(egui::TextEdit::singleline(&mut state.query))
                .labelled_by(label.id);
            if ui.button(tr("encounter-history-clear", &[])).clicked() {
                out.push(Command::LibraryEdit(LibraryEdit::ClearEncounters(
                    uid.clone(),
                )));
                state.history = None;
            }
            let query = state.query.trim();
            let shown: Vec<_> = rows
                .iter()
                .filter(|(e, r, _)| query.is_empty() || e.contains(query) || r.contains(query))
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("encounter-words")
                .show_rows(ui, 26.0, shown.len(), |ui, range| {
                    for (expression, reading, at) in &shown[range] {
                        ui.label(format!("{expression} · {reading} · {at}"));
                    }
                });
            return;
        }
        state.history = None;
    }
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

    if !stats.encounters.is_empty() {
        section(ui, &tr("stats-encounters", &[]));
        ui.label(
            RichText::new(tr("stats-encounters-detail", &[])).color(palette::current().details),
        );
        for (uid, title, count) in &stats.encounters {
            ui.push_id(uid, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr(
                        "stats-encounters-episode",
                        &[("title", title.clone()), ("count", count.to_string())],
                    ));
                    if ui.button(tr("encounter-history-open", &[])).clicked() {
                        state.history = Some(uid.clone());
                        state.query.clear();
                    }
                });
            });
        }
    }

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
    ui.painter().rect_filled(rect, 4.0, palette::current().row);
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
            ui.painter()
                .rect_filled(bar, 1.0, palette::current().accent);
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
        ui.label(
            RichText::new(date_of(first))
                .color(palette::current().details)
                .small(),
        );
        ui.add_space((width - 140.0).max(0.0));
        ui.label(
            RichText::new(date_of(today))
                .color(palette::current().details)
                .small(),
        );
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
        let color = heat(minutes);
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

    if let Some(review) = &stats.review {
        review_section(ui, review, tr);
    }
    if let Some(words) = &stats.words {
        words_section(ui, words, tr);
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
                        .color(palette::current().details),
                    );
                    ui.end_row();
                }
            });
    }

    // the sessions
    section(ui, &tr("stats-sessions", &[]));
    if stats.sessions.is_empty() {
        ui.label(RichText::new(tr("stats-none", &[])).color(palette::current().details));
    }
    for (id, s, name) in &stats.sessions {
        Frame::new()
            .fill(palette::current().row)
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
                            .color(palette::current().details),
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
    // T-KNW-018: the historical new-word count has its own episode label.
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
            encounters: vec![("video/show-01".into(), "Show - 01".into(), 7)],
            encounter_words: BTreeMap::from([(
                "video/show-01".into(),
                vec![
                    ("猫".into(), "ねこ".into(), 42),
                    ("犬".into(), "いぬ".into(), 30),
                ],
            )]),
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
            review: Some(nikomiru_library::review_log::ReviewStats {
                today: (12, 10),
                days: vec![1; 30],
                retention: Some(0.874),
                new: 3,
                learning: 1,
                young: 20,
                mature: 5,
                ahead: vec![4, 2, 0, 0, 1, 0, 0],
            }),
            words: Some(nikomiru_library::words::WordStats {
                known: 120,
                learning: 30,
                ignored: 4,
                anki_known: 800,
                anki_learning: 50,
                known_by_day: vec![100; 30],
            }),
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
        // T-REV-008 the review's part
        h.get_by_label("stats-review-today 12 10");
        h.get_by_label("stats-review-retention 87");
        h.get_by_label("stats-review-cards 3 1 20 5");
        // T-KNW-010 the words' part
        h.get_by_label("stats-encounters-episode Show - 01 7");
        h.get_by_label("stats-words-set 120 30 4");
        h.get_by_label("stats-words-anki 800 50");
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
        // T-KNW-023: inspect historical words, search a reading and clear only this uid.
        h.get_by_label("encounter-history-open").click();
        h.run_steps(2);
        h.get_by_label("猫 · ねこ · 42");
        h.state_mut().state.query = "いぬ".into();
        h.run();
        assert!(h.query_by_label("猫 · ねこ · 42").is_none());
        h.get_by_label("犬 · いぬ · 30");
        h.get_by_label("encounter-history-clear").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c,
            Command::LibraryEdit(LibraryEdit::ClearEncounters(uid)) if uid == "video/show-01")));
        assert_eq!(
            duration(5_400_000, &|id, a| format!("{id}{a:?}")),
            "stats-hours[(\"h\", \"1\"), (\"m\", \"30\")]"
        );
    }
}
