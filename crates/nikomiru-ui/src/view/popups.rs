// SPDX-License-Identifier: GPL-3.0-only

//! The dictionary popups (POP-001…POP-015): one panel per level of the stack,
//! placed next to the looked-up word, with the entries of the reply: headword
//! with furigana, tags, frequencies, pitch accents, each dictionary's
//! glossaries and the kanji cards.
//!
//! Glossaries come as the typed view model of `nikomiru_dict::structured`:
//! every string is laid out as literal text (egui reads no markup) and images
//! are pixels decoded on the lookup thread (NFR-SEC-01). A panel's height is
//! known only once its content is laid out: a pass that measures a height
//! other than the one the panel was placed with is discarded and run again,
//! so a popup never shows in the wrong place (POP-006).

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Area, Color32, ColorImage, CursorIcon, FontFamily, FontId, Frame, Galley, Id, Label,
    LayerId, Layout, Margin, Order, Pos2, Rect, Response, RichText, Role, ScrollArea, Sense, Shape,
    Stroke, StrokeKind, TextWrapMode, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2,
    WidgetInfo, pos2, vec2,
};
use nikomiru_dict::structured::{self, Block, Cell, Image, Pixels, Run, Target, Units};
use nikomiru_dict::style::{Length, ListMarker, Rgba, Style, VerticalAlign};

use super::theme;
use crate::popup::{self, Origin, Popup};
use crate::popup_view::{Accent, EntryView, KanjiView, PitchRow, Section};
use crate::session::Session;
use crate::ui::Command;

/// The size of glossary text; `em` lengths count in it.
const BASE: f32 = 15.0;
/// The preferred width; the height is the content's, up to 70% of the window.
const WIDTH: f32 = 560.0;
const MAX_HEIGHT: f32 = 0.7;
/// Between the panel's border and its content.
const PAD: f32 = 10.0;
/// From the window's edges (POP-006).
const EDGE: f64 = 8.0;

const PANEL: Color32 = Color32::from_rgba_unmultiplied_const(0x1b, 0x1d, 0x22, 0xf2);
const PANEL_BORDER: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x40);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
const WEAK: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
const LINK: Color32 = Color32::from_rgb(0x8a, 0xb4, 0xff);
const MARKER: Color32 = Color32::from_rgb(0xaa, 0xb2, 0xbf);
const READING: Color32 = Color32::from_rgb(0xc5, 0xcf, 0xf5);
const CURSOR: Color32 = Color32::from_rgb(0x6a, 0xa0, 0xff);
const SECTION: Color32 = Color32::from_rgb(0x26, 0x2a, 0x31);
const SECTION_TEXT: Color32 = Color32::from_rgb(0xc9, 0xd1, 0xdd);
const TRACE: Color32 = Color32::from_rgb(0x24, 0x46, 0x5a);
const FREQUENCY: Color32 = Color32::from_rgb(0x2f, 0x3a, 0x2a);
const PITCH: Color32 = Color32::from_rgb(0xf0, 0xb3, 0x5a);
const ONYOMI: Color32 = Color32::from_rgb(0x3a, 0x2a, 0x44);
const KUNYOMI: Color32 = Color32::from_rgb(0x2a, 0x3a, 0x44);
const CHIP: Color32 = Color32::from_rgb(0x3a, 0x3d, 0x44);
const CELL_BORDER: Color32 = Color32::from_rgb(0x55, 0x5b, 0x66);
const HEADER_CELL: Color32 = Color32::from_rgb(0x2c, 0x30, 0x38);
const BORDER: Color32 = Color32::from_rgba_unmultiplied_const(0x90, 0x9a, 0xa8, 0x80);

/// What a popup keeps between frames, by its generation: what the reader
/// changed in it and what its drawing needs.
#[derive(Default)]
struct Panel {
    /// The content's height in the last pass: the panel is placed with it.
    content_height: Option<f32>,
    /// Sections the reader opened or closed, by entry and section.
    sections: HashMap<(usize, usize), bool>,
    /// `details` the reader opened or closed.
    details: HashMap<Id, bool>,
    /// Collapsed images the reader opened.
    images: HashSet<Id>,
    /// Image textures, by the address of their pixels and their filtering.
    textures: HashMap<(usize, bool), TextureHandle>,
    /// The keyboard entry the panel last scrolled to (POP-014).
    entry: Option<usize>,
}

/// What the popups keep between frames.
#[derive(Default)]
pub(crate) struct State {
    panels: HashMap<u64, Panel>,
    /// The pointer was over a popup (POP-003): told to the session only when
    /// it changes.
    hovered: bool,
    /// The panels drawn, by level: for Alt+wheel (POP-014).
    pub rects: Vec<Rect>,
    #[cfg(test)]
    drawn: Vec<Drawn>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("panels", &self.panels.len())
            .field("hovered", &self.hovered)
            .finish_non_exhaustive()
    }
}

/// What the tests check of a drawn link or panel.
#[cfg(test)]
#[derive(Debug, Clone)]
enum Drawn {
    Panel { level: usize, rect: Rect },
    Link { text: String, rect: Rect },
}

/// Translates a message with its arguments.
pub(crate) type Translate<'a> = dyn Fn(&str, &[(&str, String)]) -> String + 'a;

/// The popups to draw and the strings they show.
pub(crate) struct Scene<'a> {
    pub stack: &'a [Popup],
    pub tr: &'a Translate<'a>,
}

/// Draws the popups; returns the levels drawn with their content.
pub(crate) fn show(
    ctx: &egui::Context,
    s: &Session,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Vec<usize> {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        stack: s.popups.stack(),
        tr: &tr,
    };
    draw(ctx, &scene, state, out)
}

/// The rectangle of a popup: its preferred size, placed next to its word.
fn place(p: &Popup, content: Option<f32>, screen: Rect) -> Rect {
    let (width, height) = (f64::from(screen.width()), f64::from(screen.height()));
    let preferred_height = content.map_or(height, |c| f64::from(c + 2.0 * PAD + 2.0));
    let size = (
        f64::from(WIDTH).min(width - 2.0 * EDGE),
        preferred_height.min(f64::from(MAX_HEIGHT) * height),
    );
    // the harness clicks with no rectangle: the bottom middle of the window
    let anchor = if p.anchor.w > 0.0 || p.anchor.h > 0.0 {
        p.anchor
    } else {
        popup::Rect {
            x: 0.25 * width,
            y: 0.72 * height,
            w: 0.5 * width,
            h: 0.12 * height,
        }
    };
    let r = popup::place(anchor, size, (width, height), EDGE);
    Rect::from_min_size(
        screen.min + vec2(r.x as f32, r.y as f32),
        vec2(r.w as f32, r.h as f32),
    )
}

pub(crate) fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Vec<usize> {
    let stack = scene.stack;
    state
        .panels
        .retain(|g, _| stack.iter().any(|p| p.generation == *g));
    state.rects.clear();
    #[cfg(test)]
    state.drawn.clear();
    let screen = ctx.content_rect();
    let (pointer, pressed) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.any_pressed()));
    let mut drawn = Vec::new();
    let mut over = false;
    // egui brings an area up when it appears: a nested popup comes over its
    // parent, a panel already shown keeps its place
    for (level, p) in stack.iter().enumerate() {
        let panel = state.panels.entry(p.generation).or_default();
        let rect = place(p, panel.content_height, screen);
        let layer = LayerId::new(Order::Foreground, Id::unique(("popup", p.generation)));
        let mut measured = None;
        Area::new(layer.id)
            .order(Order::Foreground)
            .fade_in(false)
            .constrain(false)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, PANEL_BORDER))
                    .corner_radius(8)
                    .inner_margin(Margin::same(PAD as i8))
                    .show(ui, |ui| {
                        // exactly the placed size: an area's content is otherwise
                        // held to the area's size in the last frame
                        ui.set_width((rect.width() - 2.0 * PAD - 2.0).max(0.0));
                        ui.set_height((rect.height() - 2.0 * PAD - 2.0).max(0.0));
                        let scroll = ScrollArea::vertical()
                            .id_salt(("popup-scroll", p.generation))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut d = Draw {
                                    level,
                                    panel,
                                    out,
                                    tr: scene.tr,
                                    #[cfg(test)]
                                    drawn: &mut state.drawn,
                                };
                                body(ui, &mut d, p, level);
                            });
                        measured = Some(scroll.content_size.y);
                    });
            });
        let panel = state.panels.entry(p.generation).or_default();
        if let Some(height) = measured
            && panel
                .content_height
                .is_none_or(|old| (old - height).abs() > 0.5)
        {
            panel.content_height = Some(height);
            ctx.request_discard("popup size");
        }
        state.rects.push(rect);
        #[cfg(test)]
        state.drawn.push(Drawn::Panel { level, rect });
        if p.reply.is_some() {
            drawn.push(level);
        }
        let inside = pointer.is_some_and(|pos| ctx.layer_id_at(pos) == Some(layer));
        over |= inside;
        // a press in a popup closes the ones above it (POP-012)
        if pressed && inside && level + 1 < stack.len() {
            out.push(Command::PopupFocus(level));
        }
    }
    if over != state.hovered {
        state.hovered = over;
        out.push(Command::PopupHover(over));
    }
    if ctx.will_discard() {
        drawn.clear();
    }
    drawn
}

/// The drawing of one panel.
struct Draw<'a> {
    level: usize,
    panel: &'a mut Panel,
    out: &'a mut Vec<Command>,
    tr: &'a Translate<'a>,
    #[cfg(test)]
    drawn: &'a mut Vec<Drawn>,
}

impl Draw<'_> {
    fn tr(&self, id: &str) -> String {
        (self.tr)(id, &[])
    }

    /// A click on a link: a nested lookup next to it, or the browser.
    fn link(&mut self, target: &Target, rect: Rect) {
        match target {
            Target::Lookup(text) => self.out.push(Command::PopupLookup {
                level: self.level,
                text: text.clone(),
                anchor: popup::Rect {
                    x: f64::from(rect.min.x),
                    y: f64::from(rect.min.y),
                    w: f64::from(rect.width()),
                    h: f64::from(rect.height()),
                },
            }),
            Target::External(url) => self.out.push(Command::OpenExternal(url.clone())),
        }
    }

    fn texture(
        &mut self,
        ctx: &egui::Context,
        pixels: &Arc<Pixels>,
        nearest: bool,
    ) -> TextureHandle {
        let key = (Arc::as_ptr(pixels) as usize, nearest);
        self.panel
            .textures
            .entry(key)
            .or_insert_with(|| {
                let image = ColorImage::from_rgba_unmultiplied(pixels.size, &pixels.rgba);
                let options = if nearest {
                    TextureOptions::NEAREST
                } else {
                    TextureOptions::LINEAR
                };
                ctx.load_texture("dictionary-image", image, options)
            })
            .clone()
    }
}

fn rgba(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The colour of a tag chip by its category.
fn chip_color(category: &str) -> Color32 {
    match category {
        "popular" => Color32::from_rgb(0x5a, 0x3b, 0x1f),
        "partOfSpeech" => Color32::from_rgb(0x2a, 0x3f, 0x5f),
        "frequent" => Color32::from_rgb(0x3d, 0x5a, 0x2a),
        "archaism" | "dialect" => Color32::from_rgb(0x4a, 0x3a, 0x5a),
        _ => CHIP,
    }
}

/// A length in points, for text of `em` points, within sane bounds.
fn points(length: Option<Length>, em: f32) -> f32 {
    length.map_or(0.0, |l| l.px(em).clamp(0.0, 120.0))
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

fn bold(on: bool) -> FontFamily {
    if on {
        theme::bold()
    } else {
        FontFamily::Proportional
    }
}

/// The text format of a run: its size, colour, weight and decorations;
/// links are blue and underlined.
fn text_format(s: &Style, link: bool) -> TextFormat {
    let mut size = BASE * s.font_scale.unwrap_or(1.0);
    let valign = match s.vertical_align {
        Some(VerticalAlign::Super) => {
            size *= 0.75;
            Align::TOP
        }
        Some(VerticalAlign::Sub) => {
            size *= 0.75;
            Align::BOTTOM
        }
        None => Align::BOTTOM,
    };
    let color = if link {
        LINK
    } else {
        s.color.map_or(TEXT, rgba)
    };
    let line = |on: bool| {
        if on {
            Stroke::new(1.0, color)
        } else {
            Stroke::NONE
        }
    };
    TextFormat {
        font_id: FontId::new(size, bold(s.bold == Some(true))),
        color,
        italics: s.italic == Some(true),
        underline: line(link || s.underline == Some(true)),
        strikethrough: line(s.strike == Some(true)),
        valign,
        ..TextFormat::default()
    }
}

fn run_job(text: &str, s: &Style, link: bool) -> LayoutJob {
    LayoutJob::single_section(text.to_string(), text_format(s, link))
}

/// The character under `pos` (relative to the galley).
fn char_at(galley: &Galley, pos: Vec2) -> Option<usize> {
    let mut start = 0;
    for row in &galley.rows {
        if (row.pos.y..row.pos.y + row.size.y).contains(&pos.y) {
            let x = pos.x - row.pos.x;
            return row
                .glyphs
                .iter()
                .position(|g| (g.pos.x..g.max_x()).contains(&x))
                .map(|k| start + k);
        }
        start += row.glyphs.len() + usize::from(row.ends_with_newline);
    }
    None
}

/// The rectangle around characters `chars` of a galley.
fn chars_rect(galley: &Galley, chars: &Range<usize>) -> Rect {
    let mut rect = Rect::NOTHING;
    let mut start = 0;
    for row in &galley.rows {
        for (k, g) in row.glyphs.iter().enumerate() {
            if chars.contains(&(start + k)) {
                rect = rect.union(Rect::from_min_max(
                    row.pos + vec2(g.pos.x, 0.0),
                    row.pos + vec2(g.max_x(), row.size.y),
                ));
            }
        }
        start += row.glyphs.len() + usize::from(row.ends_with_newline);
    }
    rect
}

/// A small coloured label; `tip` is shown on hover.
fn chip(ui: &mut Ui, text: &str, fill: Color32, size: f32, tip: &str) -> Response {
    let response = Frame::new()
        .fill(fill)
        .corner_radius(4)
        .inner_margin(Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.add(
                Label::new(RichText::new(text).size(size).color(TEXT))
                    .wrap_mode(TextWrapMode::Extend)
                    .selectable(false),
            );
        })
        .response;
    if tip.is_empty() {
        response
    } else {
        response.on_hover_text(tip)
    }
}

/// A chip that does something when clicked.
fn chip_button(ui: &mut Ui, text: &str) -> bool {
    let response = chip(ui, text, CHIP, 13.0, "")
        .interact(Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let name = text.to_string();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
    response.clicked()
}

/// Text with its reading centred above it; an empty reading keeps its line,
/// so text and ruby in a flow stand on one base line.
fn ruby(
    ui: &mut Ui,
    base: LayoutJob,
    reading: &str,
    size: f32,
    color: Color32,
    sense: Sense,
) -> Response {
    let base = ui.ctx().fonts_mut(|f| f.layout_job(base));
    let reading = if reading.is_empty() { " " } else { reading };
    let top = ui
        .ctx()
        .fonts_mut(|f| f.layout_no_wrap(reading.to_string(), FontId::proportional(size), color));
    let width = base.size().x.max(top.size().x);
    let (rect, response) = ui.allocate_exact_size(vec2(width, top.size().y + base.size().y), sense);
    let role = if sense.senses_click() {
        Role::Link
    } else {
        Role::Label
    };
    let text = base.text().to_string();
    response.widget_info(|| WidgetInfo::labeled(role, true, &text));
    let painter = ui.painter();
    painter.galley(
        pos2(rect.center().x - top.size().x / 2.0, rect.top()),
        top.clone(),
        color,
    );
    painter.galley(
        pos2(
            rect.center().x - base.size().x / 2.0,
            rect.top() + top.size().y,
        ),
        base,
        TEXT,
    );
    response
}

// ---------------------------------------------------------------------------
// The panel

fn body(ui: &mut Ui, d: &mut Draw<'_>, p: &Popup, level: usize) {
    ui.spacing_mut().item_spacing.y = 14.0;
    ui.visuals_mut().override_text_color = Some(TEXT);
    let Some(reply) = &p.reply else {
        ui.label(RichText::new(d.tr("popup-loading")).size(BASE).color(WEAK));
        return;
    };
    let content = &reply.content;
    if reply.empty {
        ui.label(
            RichText::new(d.tr("popup-no-results"))
                .size(BASE)
                .color(WEAK),
        );
    }
    for k in &content.kanji {
        kanji(ui, d, k);
    }
    // replay the line or go to it (POP-015)
    if level == 0 && matches!(p.origin, Origin::Subtitle { .. }) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if chip_button(ui, &format!("↻ {}", d.tr("popup-replay"))) {
                d.out.push(Command::PopupReplay);
            }
            if chip_button(ui, &format!("⇥ {}", d.tr("popup-goto"))) {
                d.out.push(Command::PopupGoto);
            }
        });
    }
    let follow = d.panel.entry.replace(p.entry).is_some_and(|e| e != p.entry);
    for (k, e) in content.entries.iter().enumerate() {
        let inner = ui.horizontal_top(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| entry(ui, d, e, k));
        });
        let rect = inner.response.rect;
        // the entry the keyboard is on (POP-014)
        if k == p.entry && content.entries.len() > 1 {
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                1.5,
                CURSOR,
            );
            if follow {
                ui.scroll_to_rect(rect, Some(Align::TOP));
            }
        }
    }
}

fn entry(ui: &mut Ui, d: &mut Draw<'_>, e: &EntryView, k: usize) {
    ui.spacing_mut().item_spacing.y = 6.0;
    // the headword, a reading over each part (LKP-050)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for segment in &e.furigana {
            let base = LayoutJob::single_section(
                segment.t.clone(),
                TextFormat::simple(FontId::proportional(26.0), Color32::WHITE),
            );
            ruby(ui, base, &segment.r, 12.0, READING, Sense::hover());
        }
    });
    if !e.trace.is_empty() || !e.term_tags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for t in &e.trace {
                chip(ui, &t.name, TRACE, 12.0, &t.description);
            }
            for t in &e.term_tags {
                chip(ui, &t.name, chip_color(&t.category), 12.0, &t.notes);
            }
        });
    }
    if !e.frequencies.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for f in &e.frequencies {
                chip(
                    ui,
                    &format!("{}: {}", f.dictionary, f.values.join(", ")),
                    FREQUENCY,
                    12.0,
                    "",
                );
            }
        });
    }
    for row in &e.pitches {
        pitch_row(ui, d, row, &e.morae);
    }
    for (n, section) in e.sections.iter().enumerate() {
        dictionary_section(ui, d, section, (k, n));
    }
}

/// The accents of one row: the morae with a line over the high ones and a
/// tick where the pitch falls (LKP-041, LKP-042).
fn pitch_row(ui: &mut Ui, d: &Draw<'_>, row: &PitchRow, morae: &[String]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
        let label = match row.dictionaries.as_slice() {
            [one] => one.clone(),
            many => format!("{} · {}", d.tr("popup-pitch-accent"), many.len()),
        };
        chip(ui, &label, CHIP, 12.0, &row.dictionaries.join(", "));
        for a in &row.accents {
            accent(ui, a, morae);
        }
        for text in row.patterns.iter().chain(&row.transcriptions) {
            ui.label(RichText::new(text).size(14.0).color(TEXT));
        }
    });
}

fn accent(ui: &mut Ui, a: &Accent, morae: &[String]) {
    let galleys: Vec<Arc<Galley>> = ui.ctx().fonts_mut(|f| {
        morae
            .iter()
            .map(|m| f.layout_no_wrap(m.clone(), FontId::proportional(BASE), TEXT))
            .collect()
    });
    let tail = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(
            format!(" [{}] {}", a.position, a.category),
            FontId::proportional(12.0),
            WEAK,
        )
    });
    let text_height = galleys.iter().map(|g| g.size().y).fold(BASE, f32::max);
    let width: f32 = galleys.iter().map(|g| g.size().x).sum::<f32>() + tail.size().x;
    let (rect, _) = ui.allocate_exact_size(vec2(width, text_height + 3.0), Sense::hover());
    let painter = ui.painter();
    let line = Stroke::new(1.5, PITCH);
    let high: Vec<bool> = a.high_low.chars().map(|c| c == 'H').collect();
    let top = rect.top() + 1.0;
    let mut x = rect.left();
    for (i, g) in galleys.iter().enumerate() {
        let w = g.size().x;
        painter.galley(pos2(x, rect.top() + 3.0), Arc::clone(g), TEXT);
        if high.get(i) == Some(&true) {
            painter.line_segment([pos2(x, top), pos2(x + w, top)], line);
            if high.get(i + 1) == Some(&false) {
                painter.line_segment([pos2(x + w, top), pos2(x + w, top + 6.0)], line);
            }
        }
        x += w;
    }
    painter.galley(pos2(x, rect.bottom() - tail.size().y), tail, WEAK);
}

/// A dictionary's glossaries under a header that opens and closes them
/// (POP-008, DIC-034).
fn dictionary_section(ui: &mut Ui, d: &mut Draw<'_>, s: &Section, key: (usize, usize)) {
    let open = d.panel.sections.get(&key).copied().unwrap_or(s.expanded);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let header = Frame::new()
            .fill(SECTION)
            .corner_radius(4)
            .inner_margin(Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    Label::new(
                        RichText::new(format!("{} {}", if open { "▾" } else { "▸" }, s.dictionary))
                            .size(13.0)
                            .family(theme::bold())
                            .color(SECTION_TEXT),
                    )
                    .selectable(false),
                );
            })
            .response
            .interact(Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let name = s.dictionary.clone();
        header.widget_info(|| WidgetInfo::selected(Role::Button, true, open, &name));
        if header.clicked() {
            d.panel.sections.insert(key, !open);
        }
        if !open {
            return;
        }
        let id = Id::unique(("popup-section", d.level, key));
        for (i, g) in s.glossaries.iter().enumerate() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if !g.tags.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for t in &g.tags {
                            chip(ui, &t.name, chip_color(&t.category), 11.0, &t.notes);
                        }
                    });
                }
                blocks(ui, d, &g.blocks, id.with(i));
            });
        }
    });
}

fn kanji(ui: &mut Ui, d: &Draw<'_>, k: &KanjiView) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.vertical(|ui| {
            ui.label(RichText::new(&k.character).size(56.0).color(Color32::WHITE));
            ui.label(RichText::new(&k.dictionary).size(12.0).color(WEAK));
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for r in &k.onyomi {
                    chip(ui, r, ONYOMI, 13.0, "");
                }
                for r in &k.kunyomi {
                    chip(ui, r, KUNYOMI, 13.0, "");
                }
            });
            if !k.meanings.is_empty() {
                ui.add(
                    Label::new(RichText::new(k.meanings.join(", ")).size(BASE).color(TEXT)).wrap(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                if let Some(n) = &k.strokes {
                    chip(
                        ui,
                        &(d.tr)("kanji-strokes", &[("count", n.clone())]),
                        CHIP,
                        12.0,
                        "",
                    );
                }
                if let Some(r) = &k.radical {
                    chip(
                        ui,
                        &(d.tr)("kanji-radical", &[("radical", r.clone())]),
                        CHIP,
                        12.0,
                        "",
                    );
                }
                for (name, value) in &k.stats {
                    chip(ui, &format!("{name}: {value}"), CHIP, 12.0, "");
                }
                for f in &k.frequencies {
                    chip(
                        ui,
                        &format!("{}: {}", f.dictionary, f.values.join(", ")),
                        FREQUENCY,
                        12.0,
                        "",
                    );
                }
            });
        });
    });
}

// ---------------------------------------------------------------------------
// Glossaries

fn blocks(ui: &mut Ui, d: &mut Draw<'_>, blocks_: &[Block], id: Id) {
    ui.spacing_mut().item_spacing.y = 3.0;
    for (i, b) in blocks_.iter().enumerate() {
        let id = id.with(i);
        match b {
            Block::Flow { runs } => flow(ui, d, runs, id),
            Block::Box { s, blocks: inner } => boxed(ui, d, s, inner, id),
            Block::List {
                ordered,
                s,
                items,
                markers,
            } => list(ui, d, *ordered, s, items, markers, id),
            Block::Table { columns, rows, .. } => table(ui, d, *columns, rows, id),
            Block::Details {
                open,
                summary,
                blocks: inner,
            } => details(ui, d, *open, summary, inner, id),
            Block::Pairs { items } => {
                for (term, tags) in items {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        ui.label(RichText::new(term).size(BASE).color(TEXT));
                        for t in tags {
                            chip(ui, t, CHIP, 12.0, "");
                        }
                    });
                }
            }
        }
    }
}

fn flow(ui: &mut Ui, d: &mut Draw<'_>, runs: &[Run], id: Id) {
    if structured::plain(runs) {
        paragraph(ui, d, runs);
    } else {
        mixed(ui, d, runs, id);
    }
}

/// Plain runs as one text: they wrap together, and a click finds its link by
/// the character under the pointer.
fn paragraph(ui: &mut Ui, d: &mut Draw<'_>, runs: &[Run]) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let mut links: Vec<(Range<usize>, &Target)> = Vec::new();
    let mut chars = 0;
    let mut space = 0.0;
    for r in runs {
        let (text, s, target) = match r {
            Run::Text { t, s } => (t, s, None),
            Run::Link { t, target, s } => (t, s, Some(target)),
            Run::Br => {
                job.append("\n", 0.0, text_format(&Style::default(), false));
                chars += 1;
                space = 0.0;
                continue;
            }
            Run::Ruby { .. } | Run::Image { .. } => continue,
        };
        let em = BASE * s.font_scale.unwrap_or(1.0);
        let lead = space + points(s.margin.left, em) + points(s.padding.left, em);
        job.append(text, lead, text_format(s, target.is_some()));
        let n = text.chars().count();
        if let Some(target) = target {
            links.push((chars..chars + n, target));
        }
        chars += n;
        space = points(s.padding.right, em) + points(s.margin.right, em);
    }
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let sense = if links.is_empty() {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(galley.size(), sense);
    let text = galley.text().to_string();
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
    let link_at = |pos: Pos2| {
        let c = char_at(&galley, pos - rect.min)?;
        links.iter().find(|(range, _)| range.contains(&c))
    };
    if response.hover_pos().and_then(link_at).is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.clicked()
        && let Some((range, target)) = response.interact_pointer_pos().and_then(link_at)
    {
        let at = chars_rect(&galley, range).translate(rect.min.to_vec2());
        d.link(target, at);
    }
    #[cfg(test)]
    for (range, _) in &links {
        let text: String = galley
            .text()
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        d.drawn.push(Drawn::Link {
            text,
            rect: chars_rect(&galley, range).translate(rect.min.to_vec2()),
        });
    }
    ui.painter().galley(rect.min, galley, TEXT);
}

/// Runs with ruby, chips or images: one item each, wrapping like words. In a
/// flow with ruby every item has a reading line, so all stand on one base
/// line.
fn mixed(ui: &mut Ui, d: &mut Draw<'_>, runs: &[Run], id: Id) {
    let ruby_flow = runs.iter().any(|r| matches!(r, Run::Ruby { .. }));
    let reading = BASE * 0.55;
    let layout = Layout::left_to_right(if ruby_flow { Align::Min } else { Align::Center })
        .with_main_wrap(true);
    ui.with_layout(layout, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        for (i, r) in runs.iter().enumerate() {
            match r {
                Run::Br => ui.end_row(),
                Run::Text { t, s } if structured::chip(s) => {
                    if ruby_flow {
                        ui.vertical(|ui| {
                            ui.add_space(reading * 1.2);
                            chip_run(ui, t, s);
                        });
                    } else {
                        chip_run(ui, t, s);
                    }
                }
                Run::Text { t, s } if ruby_flow => {
                    ruby(ui, run_job(t, s, false), "", reading, TEXT, Sense::hover());
                }
                Run::Text { t, s } => {
                    let em = BASE * s.font_scale.unwrap_or(1.0);
                    ui.add_space(points(s.margin.left, em) + points(s.padding.left, em));
                    ui.add(Label::new(run_job(t, s, false)).selectable(false));
                    ui.add_space(points(s.padding.right, em) + points(s.margin.right, em));
                }
                Run::Ruby { t, rt, s } => {
                    let color = s.color.map_or(TEXT, rgba);
                    ruby(ui, run_job(t, s, false), rt, reading, color, Sense::hover());
                }
                Run::Link { t, target, s } => {
                    let response = if ruby_flow {
                        ruby(ui, run_job(t, s, true), "", reading, LINK, Sense::click())
                    } else {
                        ui.add(
                            Label::new(run_job(t, s, true))
                                .sense(Sense::click())
                                .selectable(false),
                        )
                    }
                    .on_hover_cursor(CursorIcon::PointingHand);
                    #[cfg(test)]
                    d.drawn.push(Drawn::Link {
                        text: t.clone(),
                        rect: response.rect,
                    });
                    if response.clicked() {
                        d.link(target, response.rect);
                    }
                }
                Run::Image { image } => image_run(ui, d, image, id.with(i)),
            }
        }
    });
}

/// A run with a background or a border.
fn chip_run(ui: &mut Ui, text: &str, s: &Style) {
    let em = BASE * s.font_scale.unwrap_or(1.0);
    let border = s
        .border_color
        .map(|c| {
            Stroke::new(
                s.border_width.map_or(1.0, |w| w.px(em).clamp(0.0, 4.0)),
                rgba(c),
            )
        })
        .unwrap_or(Stroke::NONE);
    ui.add_space(points(s.margin.left, em));
    Frame::new()
        .fill(s.background.map_or(Color32::TRANSPARENT, rgba))
        .stroke(border)
        .corner_radius(points(s.border_radius, em).round().min(255.0) as u8)
        .inner_margin(margin(
            points(s.padding.left, em),
            points(s.padding.right, em),
            points(s.padding.top, em),
            points(s.padding.bottom, em),
        ))
        .show(ui, |ui| {
            ui.add(
                Label::new(run_job(text, s, false))
                    .wrap_mode(TextWrapMode::Extend)
                    .selectable(false),
            );
        });
    ui.add_space(points(s.margin.right, em));
}

/// An image at its size in the flow (POP-010), or its replacement text: when
/// missing, or collapsed until clicked.
fn image_run(ui: &mut Ui, d: &mut Draw<'_>, image: &Image, id: Id) {
    let opened = !image.collapsed || d.panel.images.contains(&id);
    let Some(pixels) = image.pixels.as_ref().filter(|_| opened) else {
        let name = [&image.alt, &image.title]
            .into_iter()
            .find(|t| !t.is_empty())
            .map_or("image", |t| t.as_str());
        let clickable = image.pixels.is_some();
        let response = ui.add(
            Label::new(
                RichText::new(format!("[{name}]"))
                    .size(BASE * 0.85)
                    .color(if clickable { LINK } else { WEAK }),
            )
            .sense(if clickable {
                Sense::click()
            } else {
                Sense::hover()
            })
            .selectable(false),
        );
        if clickable && response.clicked() {
            d.panel.images.insert(id);
        }
        if !image.title.is_empty() {
            response.on_hover_text(&image.title);
        }
        return;
    };
    let texture = d.texture(ui.ctx(), pixels, image.pixelated);
    let size = image_size(image, pixels.size, ui.available_width());
    let response = ui.add(egui::Image::new((texture.id(), size)));
    if !image.title.is_empty() {
        response.on_hover_text(&image.title);
    }
}

/// The size an image is drawn at: its own sizes when set (in pixels or in
/// ems), the natural one otherwise, never wider than `room`; the aspect is
/// kept unless both sizes are set.
fn image_size(image: &Image, natural: [usize; 2], room: f32) -> Vec2 {
    let unit = match image.units {
        Units::Em => BASE,
        Units::Px => 1.0,
    };
    let (nw, nh) = (natural[0].max(1) as f32, natural[1].max(1) as f32);
    let aspect = nh / nw;
    let (w, h) = match (image.width, image.height) {
        (Some(w), Some(h)) => (w * unit, h * unit),
        (Some(w), None) => (w * unit, w * unit * aspect),
        (None, Some(h)) => (h * unit / aspect, h * unit),
        (None, None) => (nw, nh),
    };
    if w > room && room > 0.0 {
        vec2(room, room * h / w)
    } else {
        vec2(w, h)
    }
}

/// A box: margin, border, background and padding around its blocks. Borders
/// on some sides only are drawn side by side; with a left one the content
/// moves right of it.
fn boxed(ui: &mut Ui, d: &mut Draw<'_>, s: &Style, inner: &[Block], id: Id) {
    let em = BASE;
    let sides = s.border_sides.unwrap_or([true; 4]);
    let partial = s.border_sides.is_some() && !sides.iter().all(|&b| b);
    let bordered = s.border_color.is_some() || s.border_width.is_some() || partial;
    let width = s.border_width.map_or(if partial { 3.0 } else { 1.0 }, |w| {
        w.px(em).clamp(0.0, 8.0)
    });
    let color = s.border_color.map_or(BORDER, rgba);
    let indent = if partial && sides[3] {
        width + 4.0
    } else {
        0.0
    };
    let (ml, mr, mt, mb) = (
        points(s.margin.left, em),
        points(s.margin.right, em),
        points(s.margin.top, em),
        points(s.margin.bottom, em),
    );
    let frame = Frame::new()
        .outer_margin(margin(ml, mr, mt, mb))
        .inner_margin(margin(
            points(s.padding.left, em) + indent,
            points(s.padding.right, em),
            points(s.padding.top, em),
            points(s.padding.bottom, em),
        ))
        .fill(s.background.map_or(Color32::TRANSPARENT, rgba))
        .corner_radius(if partial {
            0
        } else {
            points(s.border_radius, em).round().min(255.0) as u8
        })
        .stroke(if bordered && !partial {
            Stroke::new(width, color)
        } else {
            Stroke::NONE
        });
    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        blocks(ui, d, inner, id);
    });
    if bordered && partial {
        let outer = response.response.rect;
        let r = Rect::from_min_max(outer.min + vec2(ml, mt), outer.max - vec2(mr, mb));
        let edges = [
            Rect::from_min_max(r.left_top(), pos2(r.right(), r.top() + width)),
            Rect::from_min_max(pos2(r.right() - width, r.top()), r.right_bottom()),
            Rect::from_min_max(pos2(r.left(), r.bottom() - width), r.right_bottom()),
            Rect::from_min_max(r.left_top(), pos2(r.left() + width, r.bottom())),
        ];
        for (edge, on) in edges.into_iter().zip(sides) {
            if on {
                ui.painter().rect_filled(edge, 0.0, color);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn list(
    ui: &mut Ui,
    d: &mut Draw<'_>,
    ordered: bool,
    s: &Style,
    items: &[Vec<Block>],
    markers: &[Option<ListMarker>],
    id: Id,
) {
    let indent = points(s.margin.left, BASE);
    let default = if ordered {
        ListMarker::Decimal
    } else {
        ListMarker::Disc
    };
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (i, item) in items.iter().enumerate() {
            let marker = markers
                .get(i)
                .and_then(Option::as_ref)
                .or(s.list_marker.as_ref())
                .unwrap_or(&default)
                .label(i);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(indent);
                if !marker.is_empty() {
                    let galley = ui.ctx().fonts_mut(|f| {
                        f.layout_no_wrap(marker, FontId::proportional(BASE), MARKER)
                    });
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(galley.size().x.max(BASE), galley.size().y),
                        Sense::hover(),
                    );
                    ui.painter().galley(rect.min, galley, MARKER);
                }
                ui.vertical(|ui| blocks(ui, d, item, id.with(i)));
            });
        }
    });
}

/// `details`: a summary line that opens and closes the body.
fn details(ui: &mut Ui, d: &mut Draw<'_>, open: bool, summary: &[Run], inner: &[Block], id: Id) {
    let is_open = d.panel.details.get(&id).copied().unwrap_or(open);
    let header = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(
                Label::new(
                    RichText::new(if is_open { "▾" } else { "▸" })
                        .size(BASE)
                        .color(MARKER),
                )
                .selectable(false),
            );
            flow(ui, d, summary, id.with("summary"));
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if header.clicked() {
        d.panel.details.insert(id, !is_open);
    }
    if is_open {
        ui.horizontal_top(|ui| {
            ui.add_space(16.0);
            ui.vertical(|ui| blocks(ui, d, inner, id.with("body")));
        });
    }
}

/// A table on a grid of its own: egui's grid has no spans. Columns are as
/// wide as their cells want (within bounds), rows as tall as their tallest
/// cell; a row-spanning cell stretches the last row it covers. Wider than
/// the panel, it scrolls sideways.
fn table(ui: &mut Ui, d: &mut Draw<'_>, columns: u32, rows: &[Vec<Cell>], id: Id) {
    const CELL_PAD: Vec2 = vec2(6.0, 4.0);
    let columns = columns.max(1) as usize;
    let wants = |c: &Cell| (c.natural_em * BASE + 2.0 * CELL_PAD.x + 2.0).clamp(32.0, 280.0);
    let mut widths = vec![0.0_f32; columns];
    for c in rows.iter().flatten().filter(|c| c.colspan == 1) {
        if let Some(w) = widths.get_mut(c.col as usize) {
            *w = w.max(wants(c));
        }
    }
    for c in rows.iter().flatten().filter(|c| c.colspan > 1) {
        let span = c.col as usize..(c.col + c.colspan) as usize;
        let Some(slots) = widths.get_mut(span) else {
            continue;
        };
        let have: f32 = slots.iter().sum();
        let more = (wants(c) - have).max(0.0) / slots.len() as f32;
        slots.iter_mut().for_each(|w| *w += more);
    }
    for w in &mut widths {
        if *w == 0.0 {
            *w = 32.0;
        }
    }
    let mut xs = vec![0.0_f32];
    for w in &widths {
        xs.push(xs.last().copied().unwrap_or(0.0) + w);
    }
    let total_width = xs.last().copied().unwrap_or(0.0);

    ScrollArea::horizontal()
        .id_salt(id.with("table"))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let origin = ui.cursor().min;
            let painter = ui.painter().clone();
            let mut ys = vec![0.0_f32];
            // cells still open below: their last row and the height they need
            let mut spanning: Vec<(usize, usize, f32)> = Vec::new();
            let mut cells: Vec<(&Cell, usize, egui::layers::ShapeIdx)> = Vec::new();
            for (r, row) in rows.iter().enumerate() {
                let top = ys[r];
                let mut height = BASE + 2.0 * CELL_PAD.y;
                for (k, c) in row.iter().enumerate() {
                    let (col, end) = (c.col as usize, ((c.col + c.colspan) as usize).min(columns));
                    let (Some(&x0), Some(&x1)) = (xs.get(col), xs.get(end)) else {
                        continue;
                    };
                    let background = painter.add(Shape::Noop);
                    let rect = Rect::from_min_size(
                        origin + vec2(x0, top) + CELL_PAD,
                        vec2((x1 - x0 - 2.0 * CELL_PAD.x).max(1.0), f32::INFINITY),
                    );
                    let mut child = ui.new_child(
                        UiBuilder::new()
                            .max_rect(rect)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    blocks(&mut child, d, &c.blocks, id.with((r, k)));
                    let needed = child.min_rect().height() + 2.0 * CELL_PAD.y;
                    if c.rowspan > 1 {
                        spanning.push((r + c.rowspan as usize - 1, r, needed));
                    } else {
                        height = height.max(needed);
                    }
                    cells.push((c, r, background));
                }
                let mut bottom = top + height;
                for &(last, first, needed) in &spanning {
                    if last == r {
                        bottom = bottom.max(ys[first] + needed);
                    }
                }
                ys.push(bottom);
            }
            for (c, r, background) in cells {
                let (col, end) = (c.col as usize, ((c.col + c.colspan) as usize).min(columns));
                let last = (r + c.rowspan.max(1) as usize).min(rows.len());
                let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) =
                    (xs.get(col), xs.get(end), ys.get(r), ys.get(last))
                else {
                    continue;
                };
                let rect = Rect::from_min_max(origin + vec2(x0, y0), origin + vec2(x1, y1));
                let fill = c.s.background.map(rgba).unwrap_or(if c.header {
                    HEADER_CELL
                } else {
                    Color32::TRANSPARENT
                });
                painter.set(
                    background,
                    Shape::Vec(vec![
                        Shape::rect_filled(rect, 0.0, fill),
                        Shape::rect_stroke(
                            rect,
                            0.0,
                            Stroke::new(1.0, CELL_BORDER),
                            StrokeKind::Inside,
                        ),
                    ]),
                );
            }
            let total_height = ys.last().copied().unwrap_or(0.0);
            ui.allocate_rect(
                Rect::from_min_size(origin, vec2(total_width, total_height)),
                Sense::hover(),
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::popup::Reply;
    use crate::popup_view::{Content, Glossary};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_dict::css::Sheet;
    use serde_json::json;

    struct Test {
        stack: Vec<Popup>,
        state: State,
        out: Vec<Command>,
        drawn: Vec<usize>,
    }

    fn word() -> popup::Rect {
        popup::Rect {
            x: 600.0,
            y: 600.0,
            w: 80.0,
            h: 40.0,
        }
    }

    fn entry(expression: &str, glossary: serde_json::Value) -> EntryView {
        EntryView {
            expression: expression.into(),
            reading: String::new(),
            matched: expression.into(),
            furigana: vec![crate::popup_view::Segment {
                t: expression.into(),
                r: String::new(),
            }],
            morae: Vec::new(),
            trace: Vec::new(),
            term_tags: Vec::new(),
            frequencies: Vec::new(),
            pitches: Vec::new(),
            sections: vec![Section {
                dictionary: "JM".into(),
                expanded: true,
                glossaries: vec![Glossary {
                    tags: Vec::new(),
                    blocks: structured::render(&glossary, &Sheet::default()),
                }],
            }],
        }
    }

    fn popup(generation: u64, entries: Option<Vec<EntryView>>) -> Popup {
        Popup {
            query: "q".into(),
            origin: Origin::Subtitle { cue: 0, start: 0 },
            anchor: word(),
            generation,
            reply: entries.map(|entries| Reply {
                entries: entries.len(),
                empty: entries.is_empty(),
                best_length: 1,
                content: Arc::new(Content {
                    entries,
                    kanji: Vec::new(),
                    best_length: 1,
                }),
            }),
            entry: 0,
        }
    }

    fn harness(stack: Vec<Popup>) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 720.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let video = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    if video.clicked() {
                        t.out.push(Command::VideoClicked);
                    }
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        stack: &t.stack,
                        tr: &tr,
                    };
                    t.drawn = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    stack,
                    state: State::default(),
                    out: Vec::new(),
                    drawn: Vec::new(),
                },
            )
    }

    /// A click; kittest takes the pointer away after it, so hovering is left
    /// out.
    fn click(h: &mut Harness<'_, Test>, at: Pos2) -> Vec<Command> {
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        let mut out = std::mem::take(&mut h.state_mut().out);
        out.retain(|c| !matches!(c, Command::PopupHover(_)));
        out
    }

    fn panel(h: &Harness<'_, Test>, level: usize) -> Rect {
        h.state()
            .state
            .drawn
            .iter()
            .find_map(|d| match d {
                Drawn::Panel { level: l, rect } if *l == level => Some(*rect),
                _ => None,
            })
            .unwrap_or(Rect::NOTHING)
    }

    fn link(h: &Harness<'_, Test>, text: &str) -> Rect {
        h.state()
            .state
            .drawn
            .iter()
            .find_map(|d| match d {
                Drawn::Link { text: t, rect } if t == text => Some(*rect),
                _ => None,
            })
            .unwrap_or(Rect::NOTHING)
    }

    // T-POP-006
    #[test]
    fn a_popup_sits_above_its_word_and_is_drawn_once_it_has_content() {
        let mut h = harness(vec![popup(1, None)]);
        h.run();
        assert!(h.state().drawn.is_empty(), "still looking up");
        h.state_mut().stack = vec![popup(1, Some(vec![entry("食べる", json!(["to eat"]))]))];
        h.run();
        assert_eq!(h.state().drawn, [0]);
        let rect = panel(&h, 0);
        let layer = LayerId::new(Order::Foreground, Id::unique(("popup", 1_u64)));
        let shown = h
            .ctx
            .memory(|m| m.area_rect(layer.id))
            .unwrap_or(Rect::NOTHING);
        assert!(
            (shown.min - rect.min).length() < 0.5 && (shown.max - rect.max).length() < 0.5,
            "drawn where placed, as tall as placed: {shown:?} {rect:?}"
        );
        assert!(rect.max.y <= 600.0 - 4.0 + 0.5, "above the word: {rect:?}");
        assert!(rect.height() < 200.0, "as tall as its content: {rect:?}");
        assert!(
            (rect.center().x - 640.0).abs() < 1.0,
            "centred on the word: {rect:?}"
        );
        // no room above: below the word
        let mut low = popup(2, Some(vec![entry("食べる", json!(["to eat"]))]));
        low.anchor.y = 20.0;
        h.state_mut().stack = vec![low];
        h.run();
        assert!(panel(&h, 0).min.y >= 60.0 + 4.0 - 0.5, "{:?}", panel(&h, 0));
    }

    // T-POP-011, T-POP-012
    #[test]
    fn links_open_a_nested_lookup_next_to_them_or_the_browser() {
        let g = json!([{"type": "structured-content", "content": [
            "see ",
            {"tag": "a", "href": "?query=%E6%9D%A5%E3%82%8B", "content": "来る"},
            " or ",
            {"tag": "a", "href": "https://example.org/x", "content": "site"}
        ]}]);
        let mut h = harness(vec![popup(1, Some(vec![entry("行く", g)]))]);
        h.run();
        let at = link(&h, "来る");
        assert!(at.is_positive(), "{at:?}");
        let out = click(&mut h, at.center());
        let anchor = popup::Rect {
            x: f64::from(at.min.x),
            y: f64::from(at.min.y),
            w: f64::from(at.width()),
            h: f64::from(at.height()),
        };
        assert!(
            matches!(&out[..], [Command::PopupLookup { level: 0, text, anchor: a }] if text == "来る" && *a == anchor),
            "{out:?}"
        );
        let site = link(&h, "site");
        let out = click(&mut h, site.center());
        assert!(
            matches!(&out[..], [Command::OpenExternal(url)] if url == "https://example.org/x"),
            "{out:?}"
        );
        // the text around the links does nothing, and never reaches the video
        let panel = panel(&h, 0);
        let out = click(&mut h, pos2(panel.right() - 20.0, panel.bottom() - 15.0));
        assert!(out.is_empty(), "{out:?}");
    }

    // T-POP-012, T-POP-003
    #[test]
    fn a_press_in_a_parent_closes_its_children_and_hovering_is_told_once() {
        let mut child = popup(2, Some(vec![entry("来る", json!(["to come"]))]));
        child.origin = Origin::Glossary;
        child.anchor = popup::Rect {
            x: 600.0,
            y: 300.0,
            w: 40.0,
            h: 20.0,
        };
        let root = popup(1, Some(vec![entry("行く", json!(["to go"]))]));
        let mut h = harness(vec![root, child]);
        h.run();
        let (parent, top) = (panel(&h, 0), panel(&h, 1));
        assert!(!parent.intersects(top), "{parent:?} {top:?}");
        h.hover_at(parent.center());
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(matches!(&out[..], [Command::PopupHover(true)]), "{out:?}");
        h.hover_at(parent.center() + vec2(5.0, 0.0));
        h.run();
        assert!(h.state().out.is_empty(), "told once: {:?}", h.state().out);
        let out = click(&mut h, parent.center());
        assert!(
            out.iter().any(|c| matches!(c, Command::PopupFocus(0))),
            "{out:?}"
        );
        assert!(!out.iter().any(|c| matches!(c, Command::VideoClicked)));
        // outside every popup the click is the video's (which closes them)
        let out = click(&mut h, pos2(20.0, 20.0));
        assert!(matches!(&out[..], [Command::VideoClicked]), "{out:?}");
    }

    // T-POP-008
    #[test]
    fn a_dictionary_header_folds_its_glossaries() {
        let g = json!([{"type": "structured-content", "content": {"tag": "a", "href": "?query=x", "content": "linked"}}]);
        let mut h = harness(vec![popup(1, Some(vec![entry("行く", g)]))]);
        h.run();
        let before = panel(&h, 0);
        assert!(link(&h, "linked").is_positive());
        // the header is the first thing under the headword
        let header = h.get_by_label("▾ JM").rect();
        click(&mut h, header.center());
        assert_eq!(link(&h, "linked"), Rect::NOTHING, "folded");
        assert!(h.get_by_label("▸ JM").rect().is_positive());
        let after = panel(&h, 0);
        assert!(after.height() < before.height(), "{before:?} {after:?}");
        assert!(
            (after.max.y - before.max.y).abs() < 0.5,
            "still right above the word"
        );
    }

    // T-POP-009: every kind of block draws
    #[test]
    fn tables_lists_details_ruby_and_chips_draw() {
        let g = json!([{"type": "structured-content", "content": [
            {"tag": "ruby", "content": ["漢字", {"tag": "rt", "content": "かんじ"}]},
            "の",
            {"tag": "span", "style": {"backgroundColor": "#565656", "padding": "0.1em 0.3em", "borderRadius": "0.3em"}, "content": "n"},
            {"tag": "table", "content": [
                {"tag": "tr", "content": [{"tag": "th", "rowSpan": 2, "content": "a"}, {"tag": "td", "colSpan": 2, "content": "b"}]},
                {"tag": "tr", "content": [{"tag": "td", "content": "c"}, {"tag": "td", "content": "d"}]}
            ]},
            {"tag": "ol", "style": {"listStyleType": "lower-roman"}, "content": [{"tag": "li", "content": "one"}, {"tag": "li", "content": "two"}]},
            {"tag": "details", "content": [{"tag": "summary", "content": "more"}, "hidden"]},
            {"tag": "div", "style": {"borderStyle": "none none none solid", "borderColor": "green", "paddingLeft": "0.5em"}, "content": "quote"},
            {"tag": "img", "path": "img/a.png", "alt": "pic", "width": 2, "height": 1, "sizeUnits": "em"}
        ]}]);
        let mut h = harness(vec![popup(1, Some(vec![entry("漢字", g)]))]);
        h.run();
        assert_eq!(h.state().drawn, [0]);
        for label in ["の", "one", "two", "quote", "▸", "[pic]"] {
            assert!(h.query_by_label(label).is_some(), "{label}");
        }
        let more = h.get_by_label("▸").rect();
        click(&mut h, more.center());
        assert!(h.query_by_label("▾").is_some(), "details open");
    }

    #[test]
    fn images_keep_their_aspect_inside_the_flow() {
        let image = |width, height, units| Image {
            path: "a.png".into(),
            width,
            height,
            units,
            pixelated: false,
            collapsed: false,
            title: String::new(),
            alt: String::new(),
            pixels: None,
        };
        assert_eq!(
            image_size(&image(None, None, Units::Px), [40, 20], 500.0),
            vec2(40.0, 20.0)
        );
        assert_eq!(
            image_size(&image(Some(2.0), None, Units::Em), [40, 20], 500.0),
            vec2(30.0, 15.0)
        );
        assert_eq!(
            image_size(&image(None, Some(10.0), Units::Px), [40, 20], 500.0),
            vec2(20.0, 10.0)
        );
        assert_eq!(
            image_size(&image(None, None, Units::Px), [1000, 500], 400.0),
            vec2(400.0, 200.0),
            "never wider than the flow"
        );
    }
}
