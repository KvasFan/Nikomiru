// SPDX-License-Identifier: GPL-3.0-only

//! Nikomiru's dictionary popup in egui, from the same view model as the Qt
//! popup (`PopupEntry.qml`, `GlossaryBlocks.qml`, `GlossaryRuns.qml`). Being
//! immediate mode, it is rebuilt on every frame; the StyledText paragraphs
//! become `LayoutJob`s once (a real app would do the same) and egui caches
//! their layout.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Id, Margin, Order, Rect, RichText, ScrollArea,
    Stroke, Ui, pos2, vec2,
};
use eframe::egui_glow;
use s5_common::popup::{
    Block, Cell, Content, Entry, Piece, Run, Section, Style, Tag, chip_color, color, marker,
    styled_text,
};
use s5_common::probe::FrameProbe;

const BASE: f32 = 15.0;
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
const LINK: Color32 = Color32::from_rgb(0x8a, 0xb4, 0xff);

thread_local! {
    /// StyledText paragraphs as layout jobs, by the address of their HTML
    /// (the content does not move while its popup is shown).
    static JOBS: RefCell<(u64, HashMap<usize, Arc<LayoutJob>>)> = RefCell::default();
}

fn rgba(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn margin(left: f32, right: f32, top: f32, bottom: f32) -> Margin {
    let m = |v: f32| v.round().clamp(0.0, 127.0) as i8;
    Margin {
        left: m(left),
        right: m(right),
        top: m(top),
        bottom: m(bottom),
    }
}

/// The popup with `content`, above the subtitles; `probe` is marked when the
/// frame with this generation is painted.
pub fn show(ctx: &egui::Context, content: &Content, probe: &Arc<FrameProbe>, generation: u64) {
    JOBS.with(|jobs| {
        let mut jobs = jobs.borrow_mut();
        if jobs.0 != generation {
            *jobs = (generation, HashMap::new());
        }
    });
    let screen = ctx.content_rect();
    let width = 560f32.min(screen.width() - 16.0);
    let max_height = screen.height() * 0.7;
    // a new area for every popup (no size remembered from the previous one),
    // shown at once: egui fades areas in over 200 ms by default
    egui::Area::new(Id::unique(("popup", generation)))
        .fade_in(false)
        .order(Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(screen.center().x, screen.height() * 0.72 - 8.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(0x1b, 0x1d, 0x22, 0xf2))
                .stroke(Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 0x40),
                ))
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ScrollArea::vertical()
                        .id_salt(generation)
                        .max_height(max_height - 20.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            for e in &content.entries {
                                entry(ui, e);
                                ui.add_space(8.0);
                            }
                        });
                    let probe = Arc::clone(probe);
                    ui.painter().add(egui::PaintCallback {
                        rect: Rect::from_min_size(ui.min_rect().min, vec2(1.0, 1.0)),
                        callback: Arc::new(egui_glow::CallbackFn::new(move |_, _| {
                            probe.mark(generation)
                        })),
                    });
                });
        });
}

fn chip(ui: &mut Ui, text: &str, fill: Color32, size: f32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(4)
        .inner_margin(Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(size).color(TEXT))
        });
}

fn tags(ui: &mut Ui, tags: &[Tag], size: f32) {
    if tags.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for t in tags {
            chip(ui, &t.name, rgba(chip_color(&t.category)), size);
        }
    });
}

fn entry(ui: &mut Ui, e: &Entry) {
    // headword with furigana
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for f in &e.furigana {
            ruby(
                ui,
                &f.t,
                if f.r.is_empty() { " " } else { &f.r },
                26.0,
                12.0,
                Color32::WHITE,
                Color32::from_rgb(0xc5, 0xcf, 0xf5),
                false,
            );
        }
    });
    if !e.trace.is_empty() || !e.term_tags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for t in &e.trace {
                chip(ui, &t.name, Color32::from_rgb(0x24, 0x46, 0x5a), 12.0);
            }
            for t in &e.term_tags {
                chip(ui, &t.name, rgba(chip_color(&t.category)), 12.0);
            }
        });
    }
    if !e.frequencies.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for f in &e.frequencies {
                let values: Vec<String> = f.values.iter().map(|v| v.to_string()).collect();
                chip(
                    ui,
                    &format!("{}: {}", f.dictionary, values.join(", ")),
                    Color32::from_rgb(0x2f, 0x3a, 0x2a),
                    12.0,
                );
            }
        });
    }
    for s in &e.blocks {
        section(ui, s);
    }
}

fn section(ui: &mut Ui, s: &Section) {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x26, 0x2a, 0x31))
        .corner_radius(4)
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let arrow = if s.expanded { "▾" } else { "▸" };
            ui.label(
                RichText::new(format!("{arrow} {}", s.dictionary))
                    .size(13.0)
                    .family(bold_family())
                    .color(Color32::from_rgb(0xc9, 0xd1, 0xdd)),
            );
        });
    if !s.expanded {
        return;
    }
    for g in &s.glossaries {
        tags(ui, &g.tags, 11.0);
        blocks(ui, &g.blocks);
    }
}

fn blocks(ui: &mut Ui, blocks_: &[Block]) {
    ui.spacing_mut().item_spacing.y = 3.0;
    for b in blocks_ {
        match b {
            Block::Text { html, .. } => text(ui, html),
            Block::Flow { runs } => flow(ui, runs),
            Block::Box { s, blocks: inner } => boxed(ui, s, inner),
            Block::List {
                ordered,
                s,
                items,
                markers,
            } => {
                let left = s.margin.px(BASE)[3];
                for (i, item) in items.iter().enumerate() {
                    let m = marker(
                        markers.get(i).and_then(|m| m.as_deref()),
                        s.list_marker.as_deref(),
                        *ordered,
                        i,
                    );
                    ui.horizontal_top(|ui| {
                        ui.add_space(left);
                        if !m.is_empty() {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(BASE.max(m.chars().count() as f32 * BASE * 0.6), BASE * 1.3),
                                egui::Sense::hover(),
                            );
                            ui.painter().text(
                                rect.left_top(),
                                Align2::LEFT_TOP,
                                m,
                                FontId::proportional(BASE),
                                Color32::from_rgb(0xaa, 0xb2, 0xbf),
                            );
                        }
                        ui.vertical(|ui| blocks(ui, item));
                    });
                }
            }
            Block::Table { columns, rows, .. } => table(ui, *columns, rows),
            Block::Details {
                open,
                summary,
                blocks: inner,
            } => {
                let title: String = summary
                    .iter()
                    .filter_map(|r| match r {
                        Run::Text { t, .. } | Run::Link { t, .. } | Run::Ruby { t, .. } => {
                            Some(t.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                egui::CollapsingHeader::new(title)
                    .id_salt(inner.as_ptr() as usize)
                    .default_open(*open)
                    .show(ui, |ui| blocks(ui, inner));
            }
            Block::Pairs { items } => {
                for (term, item_tags) in items {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(term).size(BASE).color(TEXT));
                        for t in item_tags {
                            chip(ui, t, rgba(chip_color("")), 12.0);
                        }
                    });
                }
            }
        }
    }
}

/// A StyledText paragraph: one label.
fn text(ui: &mut Ui, html: &str) {
    let key = html.as_ptr() as usize;
    let job = JOBS.with(|jobs| {
        Arc::clone(jobs.borrow_mut().1.entry(key).or_insert_with(|| {
            let mut job = LayoutJob::default();
            for piece in styled_text(html) {
                match piece {
                    Piece::Break => job.append(
                        "\n",
                        0.0,
                        TextFormat::simple(FontId::proportional(BASE), TEXT),
                    ),
                    Piece::Span {
                        text,
                        color: c,
                        scale,
                        bold,
                        italic,
                        underline,
                        strike,
                        link,
                    } => {
                        let colour = if link.is_some() {
                            LINK
                        } else {
                            c.map_or(TEXT, rgba)
                        };
                        let family = if bold {
                            bold_family()
                        } else {
                            FontFamily::Proportional
                        };
                        job.append(
                            &text,
                            0.0,
                            TextFormat {
                                font_id: FontId::new(BASE * scale, family),
                                color: colour,
                                italics: italic,
                                underline: if underline || link.is_some() {
                                    Stroke::new(1.0, colour)
                                } else {
                                    Stroke::NONE
                                },
                                strikethrough: if strike {
                                    Stroke::new(1.0, colour)
                                } else {
                                    Stroke::NONE
                                },
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            Arc::new(job)
        }))
    });
    ui.add(egui::Label::new(job).wrap());
}

fn rich(t: &str, s: &Style) -> RichText {
    let mut r = RichText::new(t)
        .size(BASE * s.font_scale.unwrap_or(1.0))
        .color(s.color.as_deref().and_then(color).map_or(TEXT, rgba));
    if s.bold == Some(true) {
        r = r.family(bold_family());
    }
    if s.italic == Some(true) {
        r = r.italics();
    }
    if s.underline == Some(true) {
        r = r.underline();
    }
    if s.strike == Some(true) {
        r = r.strikethrough();
    }
    r
}

/// Base text with its reading centred above it.
#[allow(clippy::too_many_arguments)]
fn ruby(
    ui: &mut Ui,
    t: &str,
    rt: &str,
    size: f32,
    rt_size: f32,
    c: Color32,
    rt_c: Color32,
    bold: bool,
) {
    let family = if bold {
        bold_family()
    } else {
        FontFamily::Proportional
    };
    let base = ui
        .painter()
        .layout_no_wrap(t.to_string(), FontId::new(size, family), c);
    let top = ui
        .painter()
        .layout_no_wrap(rt.to_string(), FontId::proportional(rt_size), rt_c);
    let w = base.size().x.max(top.size().x);
    let (rect, _) =
        ui.allocate_exact_size(vec2(w, base.size().y + top.size().y), egui::Sense::hover());
    let p = ui.painter();
    p.galley(
        pos2(rect.center().x - top.size().x / 2.0, rect.top()),
        top,
        rt_c,
    );
    p.galley(
        pos2(
            rect.center().x - base.size().x / 2.0,
            rect.top() + rect.height() - base.size().y,
        ),
        base,
        c,
    );
}

/// Inline runs: characters and words wrap; ruby stands over its base. In a
/// flow with ruby every piece of text keeps an empty reading line above it,
/// so all of them sit on the same base line (as `rubyLift` in QML).
fn flow(ui: &mut Ui, runs: &[Run]) {
    let has_ruby = runs.iter().any(|r| matches!(r, Run::Ruby { .. }));
    let rt_size = BASE * 0.55;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for r in runs {
            match r {
                Run::Text { t, s } if s.background.is_some() || s.border_color.is_some() => {
                    let [pt, pr, pb, pl] = s.padding.px(BASE);
                    let [_, mr, _, ml] = s.margin.px(BASE);
                    ui.add_space(ml);
                    egui::Frame::new()
                        .fill(
                            s.background
                                .as_deref()
                                .and_then(color)
                                .map_or(Color32::TRANSPARENT, rgba),
                        )
                        .corner_radius(s.border_radius.map_or(0.0, |l| l.px(BASE)).round() as u8)
                        .inner_margin(margin(pl, pr, pt, pb))
                        .show(ui, |ui| ui.label(rich(t, s)));
                    ui.add_space(mr);
                }
                Run::Text { t, s } if has_ruby => {
                    let c = s.color.as_deref().and_then(color).map_or(TEXT, rgba);
                    ruby(
                        ui,
                        t,
                        " ",
                        BASE * s.font_scale.unwrap_or(1.0),
                        rt_size,
                        c,
                        c,
                        s.bold == Some(true),
                    );
                }
                Run::Text { t, s } => {
                    ui.label(rich(t, s));
                }
                Run::Ruby { t, rt, s } => {
                    let c = s.color.as_deref().and_then(color).map_or(TEXT, rgba);
                    ruby(
                        ui,
                        t,
                        rt,
                        BASE * s.font_scale.unwrap_or(1.0),
                        rt_size,
                        c,
                        c,
                        s.bold == Some(true),
                    );
                }
                Run::Link { t, s, .. } => {
                    let _ = ui.link(rich(t, s).color(LINK));
                }
                Run::Image { .. } => {
                    ui.label(RichText::new("[image]").color(Color32::from_rgb(0x9a, 0xa0, 0xaa)));
                }
                Run::Br => ui.end_row(),
            }
        }
    });
}

fn boxed(ui: &mut Ui, s: &Style, inner: &[Block]) {
    let [mt, mr, mb, ml] = s.margin.px(BASE);
    let [pt, pr, pb, pl] = s.padding.px(BASE);
    let left_only = s
        .border_sides
        .is_some_and(|b| b[3] && !(b[0] && b[1] && b[2]));
    let line = s
        .border_width
        .map_or(if left_only { 3.0 } else { 1.0 }, |w| w.px(BASE).max(1.0));
    let border = s.border_color.as_deref().and_then(color).map(rgba);
    let frame = egui::Frame::new()
        .outer_margin(margin(ml, mr, mt, mb))
        .inner_margin(margin(
            pl + if left_only { line + 4.0 } else { 0.0 },
            pr,
            pt,
            pb,
        ))
        .fill(
            s.background
                .as_deref()
                .and_then(color)
                .map_or(Color32::TRANSPARENT, rgba),
        )
        .corner_radius(if left_only {
            0
        } else {
            s.border_radius.map_or(0.0, |l| l.px(BASE)).round() as u8
        })
        .stroke(match border {
            Some(c) if !left_only => Stroke::new(line, c),
            _ => Stroke::NONE,
        });
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        blocks(ui, inner);
    });
    if left_only {
        let r = response.response.rect;
        let bar = Rect::from_min_max(
            pos2(r.left() + ml, r.top() + mt),
            pos2(r.left() + ml + line, r.bottom() - mb),
        );
        ui.painter().rect_filled(
            bar,
            0.0,
            border.unwrap_or(Color32::from_rgba_unmultiplied(0x90, 0x9a, 0xa8, 0x80)),
        );
    }
}

/// A table: cells in a grid (spans are not supported by egui's grid).
fn table(ui: &mut Ui, columns: u32, rows: &[Vec<Cell>]) {
    egui::ScrollArea::horizontal()
        .id_salt(rows.as_ptr() as usize)
        .show(ui, |ui| {
            egui::Grid::new(rows.as_ptr() as usize)
                .num_columns(columns.max(1) as usize)
                .spacing(vec2(0.0, 0.0))
                .show(ui, |ui| {
                    for row in rows {
                        for cell in row {
                            let fill = cell
                                .s
                                .background
                                .as_deref()
                                .and_then(color)
                                .map(rgba)
                                .unwrap_or(if cell.header {
                                    Color32::from_rgb(0x2c, 0x30, 0x38)
                                } else {
                                    Color32::TRANSPARENT
                                });
                            egui::Frame::new()
                                .fill(fill)
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x55, 0x5b, 0x66)))
                                .inner_margin(Margin::symmetric(6, 4))
                                .show(ui, |ui| {
                                    ui.set_max_width(
                                        (cell.natural_em * BASE + 14.0).clamp(32.0, 280.0),
                                    );
                                    blocks(ui, &cell.blocks);
                                });
                        }
                        ui.end_row();
                    }
                });
        });
}
