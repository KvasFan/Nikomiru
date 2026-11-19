// SPDX-License-Identifier: GPL-3.0-only

//! The Books page (BOK-004…BOK-006): the books added, and the reading of one,
//! paragraph by paragraph, each character a target, with its furigana above,
//! and of a manga, page by page, a selection read into text below it
//! (MNG-002…MNG-005). It sits under the popups, which take the same lookups.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Align2, Area, Button, Color32, FontId, Frame, Id, Layout, Margin, Order, RichText,
    ScrollArea, Sense, Ui, vec2,
};
use std::collections::HashMap;

use nikomiru_books::Paragraph;
use nikomiru_books::vertical::{Cell, Columns, Stance, lay_out};

use super::popups::Translate;
use super::{Dialog, View, palette, theme};
use crate::books::{BooksPage, OpenBook, percent};
use crate::popup;
use crate::session::Session;
use crate::ui::Command;
use crate::words::{Marks, Underline};

/// Actual visible underline segments, for the GUI harness.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DrawnMarks {
    pub unknown: usize,
    pub learning: usize,
}

impl DrawnMarks {
    fn record(&mut self, why: Underline) {
        match why {
            Underline::Unknown => self.unknown += 1,
            Underline::Learning => self.learning += 1,
            _ => {}
        }
    }
}

/// The size of the text, in points.
const TEXT_SIZE: f32 = 22.0;

thread_local! {
    /// The size of a book's text this frame, as a share of the usual one
    /// (RDR-005): set where the page is drawn, read by every measure of it.
    static TEXT_ZOOM: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// The size of the text, in points, at the zoom chosen (RDR-005).
fn text_size() -> f32 {
    TEXT_SIZE * TEXT_ZOOM.with(std::cell::Cell::get)
}

/// The most a manga's page is enlarged (RDR-005).
const MANGA_ZOOM: f32 = 4.0;
/// The widest column of text, in points.
const COLUMN: f32 = 860.0;
/// Behind the word looked up.
pub(super) const MARK: Color32 = Color32::from_rgba_premultiplied(40, 70, 120, 150);
/// Behind highlights and notes (RDR-002).
const ANNOTATED: Color32 = Color32::from_rgba_premultiplied(120, 96, 16, 110);

/// A character's cell in vertical writing, as a share of the text's size.
const CELL: f32 = 1.12;
/// A column's width, furigana included, as a share of the text's size.
const COLUMN_WIDTH: f32 = 1.9;
/// The smallest selection on a manga's page, in points.
const MIN_SELECTION: f32 = 8.0;

/// A chapter's pages are laid out for: book, chapter, width and height.
type PagesKey = (String, usize, u32, u32, u32);

/// What the page keeps between frames.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) drawn_marks: DrawnMarks,
    study_key: Option<(String, usize)>,
    words_open: bool,
    analysis: Option<String>,
    analysis_order: super::analysis_order::State,
    analysis_words: super::analysis_words::State,
    word_query: String,
    words: super::word_list::State,
    /// The chapter laid out: its book, its number, the rows and the columns
    /// of a page, and its cells.
    columns: Option<((String, usize, usize, usize), Columns)>,
    /// The first column of the page shown.
    column: usize,
    /// A page turn asked by Page Up/Down: forward or not (RDR-004).
    pub(crate) turn: Option<bool>,
    /// A swipe on the page (PORT-003), taken by the reader drawn.
    pub(crate) swipe: Option<crate::gestures::Gesture>,
    /// Opened from this shelf of the main page: closing the reader, or a
    /// dialog given up, goes back there.
    pub(crate) from_library: Option<super::library_page::Shelf>,
    /// The reader asked to close this frame.
    reader_closed: bool,
    /// Frames before an idle page counts as given up: the command that opens
    /// the book runs after the frame that asked for it.
    pub(crate) settle: u8,
    /// The chapter's horizontal pages: its book, number, width and height,
    /// and its paragraphs by page (RDR-004).
    pages: Option<(PagesKey, Vec<std::ops::Range<usize>>)>,
    /// An audiobook is to be chosen (ABK-001).
    audiobook: bool,
    /// A folder of pages is to be chosen (MNG-001).
    folder: bool,
    /// The address of a web page, as typed (WEB-001).
    address: String,
    pub(crate) paste_open: bool,
    paste_title: String,
    paste_text: String,
    paste_markdown: bool,
    /// The book those pictures are of, and the pictures drawn so far, by
    /// their key (BOK-010).
    book: String,
    textures: HashMap<String, Option<egui::TextureHandle>>,
    /// The corners of the selection on a manga's page, while it is drawn.
    selection: Option<(egui::Pos2, egui::Pos2)>,
    /// The text's size while Ctrl and the wheel change it, ahead of the
    /// setting; 0: as set (RDR-005).
    text_zoom: f32,
    /// How much a manga's page is enlarged (1 or less: whole), how far it
    /// is moved, and the book and page that was for (RDR-005).
    manga_zoom: f32,
    pan: egui::Vec2,
    zoomed_page: Option<(String, usize)>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("column", &self.column)
            .field("pictures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// Reader-only word status marks; native ruby is kept apart.
    pub marks: &'a dyn Fn(&str) -> Option<Marks>,
    /// The shared word-colour setting, for the chapter study controls.
    pub colours: bool,
    /// The books and the one open.
    pub page: &'a BooksPage,
    /// The book is drawn vertically (BOK-009).
    pub vertical: bool,
    /// A manga shows two pages (MNG-002).
    pub double: bool,
    /// Something with timed text plays: the book can read along (ABK-005).
    pub can_read_along: bool,
    /// The horizontal reading mode: `scroll`, `pages` or `sentence` (RDR-004).
    pub mode: &'a str,
    /// The open chapter's current highlights and notes (RDR-002).
    pub annotated: &'a [(usize, std::ops::Range<usize>)],
    /// The size of a book's text, as a share of the usual one (RDR-005).
    pub zoom: f32,
    /// A manga's text is written over its balloons (MNG-012).
    pub manga_text: bool,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let annotated = s.chapter_annotations();
    let scene = Scene {
        mode: s.book_mode(),
        annotated: &annotated,
        marks: &|text| s.book_marks(text),
        colours: s.word_colors_on(),
        page: &s.books_page,
        vertical: s.book_vertical(),
        double: s.book_double(),
        can_read_along: s.book_can_read_along(),
        zoom: s.book_zoom(),
        manga_text: s.manga_text(),
        tr: &tr,
    };
    let (close, add) = draw(ctx, &scene, &mut view.books, out);
    if add {
        view.dialog = Some(Dialog::Book);
    }
    if std::mem::take(&mut view.books.folder) {
        view.dialog = Some(Dialog::MangaFolder);
    }
    if std::mem::take(&mut view.books.audiobook) {
        view.dialog = Some(Dialog::Audiobook);
    }
    if close {
        view.close_books_page(out);
        view.books.from_library = None;
    }
    // back to the shelf the book was opened from
    view.books.settle = view.books.settle.saturating_sub(1);
    let idle = view.books.settle == 0
        && s.books_page.open.is_none()
        && !s.books_page.busy
        && s.books_page.status.is_none()
        && view.dialog.is_none()
        && !view.books.paste_open;
    if (std::mem::take(&mut view.books.reader_closed) || idle)
        && let Some(shelf) = view.books.from_library.take()
    {
        view.close_books_page(out);
        view.open_library_page(out);
        view.library.shelf = shelf;
    }
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

/// The rectangles of a range of bytes of `text` in `galley`, one a row.
pub(super) fn range_rects(
    galley: &egui::Galley,
    text: &str,
    range: std::ops::Range<usize>,
) -> Vec<egui::Rect> {
    let start = text[..range.start.min(text.len())].chars().count();
    let length = text.get(range).map_or(0, |part| part.chars().count());
    glyph_rows(galley, start..start + length)
}

/// The rects of the glyphs of characters `chars` (indices), one per row
/// they are on, from the glyphs themselves: a row's last character is as wide
/// as it is, not as the next row's start says, and the rect is the text's own
/// height, not the row's with the room for furigana.
pub(super) fn glyph_rows(galley: &egui::Galley, chars: std::ops::Range<usize>) -> Vec<egui::Rect> {
    let mut rows = Vec::new();
    let mut index = 0;
    for placed in &galley.rows {
        let mut rect: Option<egui::Rect> = None;
        for glyph in &placed.row.glyphs {
            if chars.contains(&index) {
                // from the baseline: the text's own box, whatever the row's height
                let top = glyph.pos.y - glyph.font_ascent;
                let cell = egui::Rect::from_min_size(
                    egui::pos2(glyph.pos.x, top),
                    vec2(glyph.advance_width, glyph.font_height),
                )
                .translate(placed.pos.to_vec2());
                rect = Some(rect.map_or(cell, |r| r.union(cell)));
            }
            index += 1;
        }
        if let Some(rect) = rect {
            rows.push(rect);
        }
        if placed.ends_with_newline {
            index += 1;
        }
        if index >= chars.end {
            break;
        }
    }
    rows
}

/// Furigana over their base: on each row the base is on, the share of the
/// reading that falls there, centred, and small enough not to run into the
/// next base when the reading is long.
fn draw_readings_over(
    ui: &Ui,
    galley: &egui::Galley,
    origin: egui::Pos2,
    item: &Paragraph,
    size: f32,
) {
    for ruby in &item.ruby {
        let rows = range_rects(galley, &item.text, ruby.range.clone());
        let reading: Vec<char> = ruby.text.chars().collect();
        if rows.is_empty() || reading.is_empty() {
            continue;
        }
        let total: f32 = rows.iter().map(|r| r.width()).sum::<f32>().max(1.0);
        let mut taken = 0;
        for (i, row) in rows.iter().enumerate() {
            let left = reading.len() - taken;
            let share = if i + 1 == rows.len() {
                left
            } else {
                ((row.width() / total * reading.len() as f32).round() as usize).min(left)
            };
            let part: String = reading[taken..taken + share].iter().collect();
            taken += share;
            if part.is_empty() {
                continue;
            }
            let count = part.chars().count() as f32;
            let font = (row.width() / count).clamp(size * 0.3, size * 0.5);
            ui.painter().text(
                origin + vec2(row.center().x, row.top() - size * 0.05),
                Align2::CENTER_BOTTOM,
                part,
                FontId::proportional(font),
                palette::current().details,
            );
        }
    }
}

/// A picture of the book, decoded the first time it shows and within the
/// limits of every picture (SRV-013); `None` if it cannot be.
fn texture(
    ctx: &egui::Context,
    state: &mut State,
    open: &OpenBook,
    key: &str,
) -> Option<egui::TextureHandle> {
    state
        .textures
        .entry(key.to_string())
        .or_insert_with(|| {
            let bytes = open.book.images.get(key)?;
            let (size, rgba) = if open.book.manga {
                let page = crate::popup_view::decode_page(bytes)?;
                (page.size, page.rgba)
            } else {
                let cover = crate::catalog::Cover::decode(bytes).ok()?;
                (cover.size, cover.rgba)
            };
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            Some(ctx.load_texture(key, image, Default::default()))
        })
        .clone()
}

/// Draws a picture inside `area`, whole and centred.
fn draw_picture(ui: &Ui, area: egui::Rect, texture: &egui::TextureHandle) {
    let size = texture.size_vec2();
    let scale = (area.width() / size.x).min(area.height() / size.y).min(1.0);
    let rect = egui::Rect::from_center_size(area.center(), size * scale);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
}

/// A paragraph: its offset in bytes, if clicked, and the rect it took.
#[allow(clippy::too_many_arguments)]
fn paragraph(
    ui: &mut Ui,
    item: &Paragraph,
    name: &str,
    marked: Option<std::ops::Range<usize>>,
    annotated: &[std::ops::Range<usize>],
    heard: bool,
    marks: Option<&Marks>,
    scale: f32,
) -> (Option<(usize, egui::Rect)>, egui::Rect, DrawnMarks) {
    let galley = paragraph_galley(ui, item, ui.available_width(), scale);
    let size = paragraph_size(item) * scale;
    let color = palette::current().text;
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    // the paragraph being heard (ABK-005)
    if heard {
        ui.painter().rect_filled(
            rect.expand(4.0),
            4.0,
            palette::current().accent.gamma_multiply(0.15),
        );
    }
    for range in annotated {
        for row in range_rects(&galley, &item.text, range.clone()) {
            ui.painter()
                .rect_filled(row.translate(rect.min.to_vec2()), 3.0, ANNOTATED);
        }
    }
    if let Some(range) = marked {
        for row in range_rects(&galley, &item.text, range) {
            ui.painter()
                .rect_filled(row.translate(rect.min.to_vec2()), 3.0, MARK);
        }
    }
    ui.painter().galley(rect.min, galley.clone(), color);
    let mut drawn = DrawnMarks::default();
    if let Some(marks) = marks {
        for (bytes, why) in marks.underlines() {
            for row in range_rects(&galley, &item.text, bytes.clone()) {
                let row = row.translate(rect.min.to_vec2());
                let line = egui::Rect::from_min_max(
                    egui::pos2(row.left(), row.bottom() - 2.0),
                    row.right_bottom(),
                );
                if ui.clip_rect().intersects(line) {
                    ui.painter().rect_filled(line, 0.0, palette::underline(why));
                    drawn.record(why);
                }
            }
        }
    }
    draw_readings_over(ui, &galley, rect.min, item, size);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let clicked = response.clicked().then(|| {
        let at = response.interact_pointer_pos().unwrap_or(rect.min) - rect.min;
        let cursor = galley.cursor_from_pos(at);
        let offset = crate::subtitles::char_to_byte(&item.text, cursor.index.0);
        (offset, rect)
    });
    (clicked, rect, drawn)
}

/// A paragraph's text size, before any mode's scale.
fn paragraph_size(item: &Paragraph) -> f32 {
    if item.heading {
        text_size() * 1.3
    } else {
        text_size()
    }
}

/// A paragraph laid out at `width`.
fn paragraph_galley(
    ui: &Ui,
    item: &Paragraph,
    width: f32,
    scale: f32,
) -> std::sync::Arc<egui::Galley> {
    let size = paragraph_size(item) * scale;
    let family = if item.heading {
        theme::bold()
    } else {
        egui::FontFamily::Proportional
    };
    let format = TextFormat {
        font_id: FontId::new(size, family),
        color: palette::current().text,
        // room for the furigana between the rows
        line_height: Some(size * 2.0),
        valign: egui::Align::BOTTOM,
        ..TextFormat::default()
    };
    let mut job = LayoutJob::single_section(item.text.clone(), format);
    job.wrap = TextWrapping {
        max_width: width,
        ..TextWrapping::default()
    };
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Where a cell goes on the page: its centre.
fn cell_centre(area: egui::Rect, first: usize, cell: &Cell) -> egui::Pos2 {
    let x = area.right() - ((cell.column - first) as f32 + 0.5) * text_size() * COLUMN_WIDTH;
    let y = area.top() + (cell.row as f32 + 0.5) * text_size() * CELL;
    egui::pos2(x, y)
}

fn draw_cell(ui: &Ui, area: egui::Rect, first: usize, cell: &Cell) {
    let centre = cell_centre(area, first, cell);
    draw_glyph(
        ui.painter(),
        centre,
        cell.ch,
        cell.stance,
        text_size(),
        palette::current().text,
    );
}

/// A character of `size` centred on `centre`, standing as vertical writing
/// wants it.
fn draw_glyph(
    painter: &egui::Painter,
    centre: egui::Pos2,
    ch: char,
    stance: Stance,
    size: f32,
    color: Color32,
) {
    let font = FontId::proportional(size);
    let text = ch.to_string();
    match stance {
        Stance::Upright => {
            painter.text(centre, Align2::CENTER_CENTER, text, font, color);
        }
        Stance::Corner => {
            let at = centre + vec2(size * 0.3, -size * 0.3);
            painter.text(at, Align2::CENTER_CENTER, text, font, color);
        }
        Stance::Turned => {
            let galley = painter.layout_no_wrap(text, font, color);
            let turned = galley.size();
            // a quarter turn around the cell's centre
            let at = centre + vec2(turned.y / 2.0, -turned.x / 2.0);
            painter.add(
                egui::epaint::TextShape::new(at, galley, color)
                    .with_angle(std::f32::consts::FRAC_PI_2),
            );
        }
    }
}

/// Behind a manga's text written over its balloon: the balloon's white.
const OVER_BALLOON: Color32 = Color32::from_rgb(250, 250, 250);
/// A manga's text written over its balloon, dark whatever the theme.
const BALLOON_INK: Color32 = Color32::from_rgb(24, 24, 24);

/// A character of a manga's text, where it is written over its balloon.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Glyph {
    /// Its byte in the region's text.
    offset: usize,
    ch: char,
    centre: egui::Pos2,
}

/// Where a region's characters are written over a page of `size` pixels
/// drawn at `drawn` (MNG-012): at one size for the whole balloon, what most
/// of its lines hold, as wide as they are (as tall, across) and as long as
/// their characters leave room for, so it is about the picture's; each line from its top (vertical)
/// or its left, its characters spread along it as the picture's are, at
/// most a fifth of a character apart.
fn region_glyphs(
    region: &nikomiru_books::mokuro::Region,
    size: (u32, u32),
    drawn: egui::Rect,
) -> (f32, Vec<Glyph>) {
    let sx = drawn.width() / size.0.max(1) as f32;
    let sy = drawn.height() / size.1.max(1) as f32;
    let lines: Vec<(usize, &str, egui::Rect, usize)> = region
        .lines
        .iter()
        .filter_map(|(range, [l, t, r, b])| {
            let text = region.text.get(range.clone())?;
            let count = text.chars().count();
            let at = egui::Rect::from_min_max(
                drawn.min + vec2(*l as f32 * sx, *t as f32 * sy),
                drawn.min + vec2(*r as f32 * sx, *b as f32 * sy),
            );
            (count > 0).then_some((range.start, text, at, count))
        })
        .collect();
    let along = |at: &egui::Rect| {
        if region.vertical {
            (at.width(), at.height())
        } else {
            (at.height(), at.width())
        }
    };
    let fits = |at: &egui::Rect, count: usize| {
        let (across, long) = along(at);
        across.min(long / count as f32)
    };
    // what each line holds; the balloon's size is what most hold, a line
    // with a character too many packing its own a little closer rather than
    // shrinking them all
    // lines of dots alone say nothing of the characters' size
    let dots = |text: &str| {
        text.chars()
            .all(|c| "．・…‥。、，.!?！？ー〜～".contains(c))
    };
    let text_lines: Vec<_> = lines.iter().filter(|(_, text, _, _)| !dots(text)).collect();
    let measured: Vec<_> = if text_lines.is_empty() {
        lines.iter().collect()
    } else {
        text_lines
    };
    let mut held: Vec<f32> = measured
        .iter()
        .map(|(_, _, at, count)| fits(at, *count))
        .collect();
    held.sort_by(f32::total_cmp);
    // the size most lines hold: a line's width is its characters' ink, the
    // size of the font that draws them that wide
    let size = match (held.first(), held.get(held.len() / 2)) {
        (Some(least), Some(most)) => least.max(*most),
        _ => 0.0,
    };
    if !size.is_finite() || size <= 0.0 {
        return (0.0, Vec::new());
    }
    let glyphs = lines
        .iter()
        .flat_map(|(start, text, at, count)| {
            let step = (along(at).1 / *count as f32).min(size * 1.2);
            text.char_indices().enumerate().map(move |(i, (byte, ch))| {
                let along = (i as f32 + 0.5) * step;
                let centre = if region.vertical {
                    egui::pos2(at.center().x, at.top() + along)
                } else {
                    egui::pos2(at.left() + along, at.center().y)
                };
                Glyph {
                    offset: start + byte,
                    ch,
                    centre,
                }
            })
        })
        .collect();
    (size, glyphs)
}

/// The readings of the paragraphs on the page, beside their bases.
fn draw_readings(
    ui: &Ui,
    area: egui::Rect,
    first: usize,
    cells: &[&Cell],
    paragraphs: &[Paragraph],
) {
    let shown = cells.iter().map(|c| c.paragraph);
    let (Some(low), Some(high)) = (shown.clone().min(), shown.max()) else {
        return;
    };
    for (index, item) in paragraphs.iter().enumerate().take(high + 1).skip(low) {
        for ruby in &item.ruby {
            let base: Vec<&&Cell> = cells
                .iter()
                .filter(|c| c.paragraph == index && ruby.range.contains(&c.offset))
                .collect();
            let (Some(top), Some(bottom)) = (base.first(), base.last()) else {
                continue;
            };
            let (top, bottom) = (
                cell_centre(area, first, top),
                cell_centre(area, first, bottom),
            );
            // a base that breaks across columns: over its start
            let height = if (top.x - bottom.x).abs() < 1.0 {
                bottom.y - top.y + text_size()
            } else {
                text_size()
            };
            let count = ruby.text.chars().count().max(1) as f32;
            let step = (height / count).min(text_size() * 0.6);
            let start = top.y - text_size() / 2.0 + (height - step * count) / 2.0;
            for (i, c) in ruby.text.chars().enumerate() {
                let at = egui::pos2(top.x + text_size() * 0.72, start + (i as f32 + 0.5) * step);
                ui.painter().text(
                    at,
                    Align2::CENTER_CENTER,
                    c,
                    FontId::proportional(text_size() * 0.5),
                    palette::current().details,
                );
            }
        }
    }
}

/// A key for the next page and one for the previous: the page turns to the
/// left, as the book does. The wheel counts only over the page, not over a
/// popup on it.
fn turns(ui: &Ui, over_page: bool) -> (bool, bool) {
    ui.input(|i| {
        // Ctrl and the wheel zoom instead (RDR-005)
        let wheel = i.events.iter().find_map(|e| match e {
            egui::Event::MouseWheel {
                delta, modifiers, ..
            } if over_page && !modifiers.command && !modifiers.ctrl => Some(delta.y),
            _ => None,
        });
        let wheel = wheel.unwrap_or(0.0);
        (
            i.key_pressed(egui::Key::ArrowLeft)
                || i.key_pressed(egui::Key::PageDown)
                || i.key_pressed(egui::Key::Space)
                || wheel < 0.0,
            i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageUp) || wheel > 0.0,
        )
    })
}

/// A selection as shares of the page; `None` if it is hardly one.
fn shares(selection: (egui::Pos2, egui::Pos2), page: egui::Rect) -> Option<[f32; 4]> {
    let area = egui::Rect::from_two_pos(selection.0, selection.1).intersect(page);
    (area.width() >= MIN_SELECTION && area.height() >= MIN_SELECTION).then(|| {
        [
            (area.left() - page.left()) / page.width(),
            (area.top() - page.top()) / page.height(),
            area.width() / page.width(),
            area.height() / page.height(),
        ]
    })
}

/// The line over a manga's page: the model to fetch, how it goes, or what to
/// do; whether the page shown was read ahead, or is being (MNG-011).
fn reading_line(ui: &mut Ui, s: &Scene<'_>, page: usize, out: &mut Vec<Command>) {
    let ocr = &s.page.ocr;
    if let Some(bytes) = ocr.fetched {
        let percent = crate::ocr::fetched_percent(bytes).to_string();
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label((s.tr)("manga-fetching", &[("percent", percent)]));
        });
    } else if !ocr.installed {
        let size = (nikomiru_ocr::model_bytes() >> 20).to_string();
        ui.add(egui::Label::new((s.tr)("manga-offer", &[("size", size)])).wrap());
        let details = palette::current().details;
        ui.add(egui::Label::new(RichText::new(tr(s, "manga-credit")).color(details)).wrap());
        if ui.button(tr(s, "manga-fetch")).clicked() {
            out.push(Command::OcrFetch);
        }
    } else if ocr.reading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr(s, "manga-reading"));
        });
    } else if ocr.found.get(&page).is_some_and(|f| !f.regions.is_empty()) {
        ui.label(RichText::new(tr(s, "manga-ready")).color(palette::current().details));
    } else if ocr.preparing.contains(&page) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr(s, "manga-preparing"));
        });
    } else {
        ui.label(RichText::new(tr(s, "manga-hint")).color(palette::current().details));
        if !ocr.pages {
            let size = (nikomiru_ocr::DETECTOR.bytes >> 20).to_string();
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new((s.tr)("manga-pages-offer", &[("size", size)]))
                            .color(palette::current().details),
                    )
                    .wrap(),
                );
                if ui.button(tr(s, "manga-pages-fetch")).clicked() {
                    out.push(Command::OcrFetch);
                }
            });
        }
    }
    if let Some((text, ok)) = &s.page.status {
        let shade = if *ok {
            palette::current().good
        } else {
            palette::current().bad
        };
        ui.add(egui::Label::new(RichText::new(text).color(shade)).wrap());
    }
}

/// A page of a manga, whole, with a selection drawn on it and the text read
/// from one under it (MNG-002…MNG-005).
fn manga_reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let Some(item) = open
        .book
        .chapters
        .get(open.chapter)
        .and_then(|chapter| chapter.paragraphs.first())
    else {
        return;
    };
    if let Some(imported) = open.book.manga_regions.get(&open.chapter) {
        ui.label(tr(
            s,
            if imported.regions.is_empty() {
                "mokuro-empty-page"
            } else {
                "mokuro-ready"
            },
        ));
    } else {
        reading_line(ui, s, open.chapter, out);
    }
    if !item.text.is_empty() {
        let marked = s.page.highlight.as_ref().map(|(_, range)| range.clone());
        let (clicked, _, _) = paragraph(ui, item, &item.text, marked, &[], false, None, 1.0);
        if let Some((offset, rect)) = clicked {
            out.push(Command::BookClicked {
                paragraph: 0,
                offset,
                anchor: popup::Rect {
                    x: f64::from(rect.min.x),
                    y: f64::from(rect.min.y),
                    w: f64::from(rect.width()),
                    h: f64::from(rect.height()),
                },
            });
        }
    }
    let room = ui.available_rect_before_wrap();
    let (rect, response) = ui.allocate_exact_size(room.size(), Sense::click_and_drag());
    let number = (open.chapter + 1).to_string();
    let name = (s.tr)("manga-page", &[("number", number)]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
    let key_of = |page: usize| {
        let item = open.book.chapters.get(page)?.paragraphs.first()?;
        item.image.clone()
    };
    let next = (s.double && open.chapter > 0)
        .then_some(open.chapter + 1)
        .and_then(key_of);
    let key = item.image.clone().unwrap_or_default();
    // pages are large: only those shown stay in memory
    state
        .textures
        .retain(|k, _| *k == key || Some(k) == next.as_ref());
    let Some(handle) = texture(ui.ctx(), state, open, &key) else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            tr(s, "manga-bad-page"),
            FontId::proportional(text_size()),
            palette::current().bad,
        );
        return;
    };
    // a wide picture is a spread already, and stays alone
    let second = next
        .and_then(|key| texture(ui.ctx(), state, open, &key))
        .filter(|other| is_upright(other) && is_upright(&handle));
    // where each page goes whole, then enlarged and moved (RDR-005)
    let fit = |handle: &egui::TextureHandle, area: egui::Rect, side: Align2| {
        let size = handle.size_vec2();
        let scale = (area.width() / size.x).min(area.height() / size.y);
        side.align_size_within_rect(size * scale, area)
    };
    let mut whole = match &second {
        // read from the right, and meeting in the middle
        Some(other) => {
            let (left, right) = rect.split_left_right_at_x(rect.center().x);
            vec![
                (
                    open.chapter,
                    fit(&handle, right, Align2::LEFT_CENTER),
                    &handle,
                ),
                (
                    open.chapter + 1,
                    fit(other, left, Align2::RIGHT_CENTER),
                    other,
                ),
            ]
        }
        None => vec![(
            open.chapter,
            fit(&handle, rect, Align2::CENTER_CENTER),
            &handle,
        )],
    };
    let zoomed = manga_zoom(ui, open, rect, &whole, &response, state);
    let painter = ui.painter_at(rect);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let mut pages = Vec::new();
    for (page, drawn, handle) in &mut whole {
        *drawn = zoomed.place(*drawn);
        painter.image(handle.id(), *drawn, uv, Color32::WHITE);
        pages.push((*page, *drawn));
    }
    let page_text = open
        .book
        .chapters
        .get(open.chapter)
        .and_then(|c| c.paragraphs.first())
        .map(|p| &p.text)
        .filter(|t| !t.is_empty());
    // first what covers the picture's text, then the text written over it,
    // so that no balloon's cover hides another's text (MNG-012)
    if s.manga_text {
        for (page, drawn) in &pages {
            let regions = open
                .book
                .manga_regions
                .get(page)
                .or_else(|| s.page.ocr.found.get(page));
            for region in regions
                .iter()
                .flat_map(|p| p.regions.iter().map(move |r| (p, r)))
            {
                let (imported, region) = region;
                let scale = vec2(
                    drawn.width() / imported.width.max(1) as f32,
                    drawn.height() / imported.height.max(1) as f32,
                );
                for [cl, ct, cr, cb] in &region.covers {
                    let cover = egui::Rect::from_min_max(
                        drawn.min + vec2(*cl as f32, *ct as f32) * scale,
                        drawn.min + vec2(*cr as f32, *cb as f32) * scale,
                    );
                    painter.rect_filled(cover.expand(0.5), 2.0, OVER_BALLOON);
                }
            }
        }
    }
    for (page, drawn) in &pages {
        // imported text, else what was found reading ahead (MNG-011)
        let regions = open
            .book
            .manga_regions
            .get(page)
            .or_else(|| s.page.ocr.found.get(page));
        if let Some(imported) = regions {
            for (index, region) in imported.regions.iter().enumerate() {
                let [l, t, r, b] = region.bounds;
                let at = egui::Rect::from_min_max(
                    drawn.min
                        + vec2(
                            l as f32 / imported.width as f32 * drawn.width(),
                            t as f32 / imported.height as f32 * drawn.height(),
                        ),
                    drawn.min
                        + vec2(
                            r as f32 / imported.width as f32 * drawn.width(),
                            b as f32 / imported.height as f32 * drawn.height(),
                        ),
                );
                // its text written over it, at about the size of the picture's
                // (MNG-012), the word looked up marked on it
                let glyphs = region_glyphs(region, (imported.width, imported.height), *drawn);
                // the covers are all down already: a neighbour's never hides it
                if s.manga_text && !glyphs.1.is_empty() {
                    let chosen = open.chapter == *page && page_text == Some(&region.text);
                    let marked = s
                        .page
                        .highlight
                        .as_ref()
                        .filter(|(paragraph, _)| chosen && *paragraph == 0)
                        .map(|(_, range)| range.clone());
                    let (size, cells) = &glyphs;
                    for glyph in cells {
                        if marked.as_ref().is_some_and(|r| r.contains(&glyph.offset)) {
                            let cell =
                                egui::Rect::from_center_size(glyph.centre, vec2(*size, *size));
                            painter.rect_filled(cell.expand(1.0), 2.0, MARK);
                        }
                        let stance = if region.vertical {
                            nikomiru_books::vertical::stance(glyph.ch)
                        } else {
                            Stance::Upright
                        };
                        draw_glyph(&painter, glyph.centre, glyph.ch, stance, *size, BALLOON_INK);
                    }
                }
                // enlarged, a balloon may be partly or wholly out of view
                let at = at.intersect(rect);
                if !at.is_positive() {
                    continue;
                }
                let hit = ui.interact(
                    at,
                    Id::unique(("mokuro-region", &open.uid, *page, index)),
                    Sense::click(),
                );
                let label = (s.tr)(
                    "mokuro-region",
                    &[
                        ("number", (index + 1).to_string()),
                        ("page", (page + 1).to_string()),
                    ],
                );
                hit.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, !s.page.ocr.reading, &label)
                });
                // the page's own text, clicked like a subtitle: no frame
                // over the drawing, the pointer shows it
                if hit.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if hit.clicked() && !s.page.ocr.reading {
                    let point = hit.interact_pointer_pos().unwrap_or(at.center());
                    let x = (point.x - drawn.min.x) / drawn.width() * imported.width as f32;
                    let y = (point.y - drawn.min.y) / drawn.height() * imported.height as f32;
                    // the character written under the pointer, else the
                    // share of the line of the picture's
                    let written = glyphs
                        .1
                        .iter()
                        .min_by(|a, b| {
                            a.centre
                                .distance_sq(point)
                                .total_cmp(&b.centre.distance_sq(point))
                        })
                        .filter(|_| s.manga_text)
                        .map(|glyph| glyph.offset);
                    match written.or_else(|| region.offset_at(x, y)) {
                        Some(offset) => out.push(Command::MokuroLookup {
                            page: *page,
                            region: index,
                            offset,
                            anchor: popup::Rect {
                                x: f64::from(point.x - 8.0),
                                y: f64::from(point.y - 8.0),
                                w: 16.0,
                                h: 16.0,
                            },
                        }),
                        None => out.push(Command::MokuroSelect {
                            page: *page,
                            region: index,
                        }),
                    }
                }
            }
        }
    }
    if s.page.ocr.installed && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let pointer = ui.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
    if open.book.manga_regions.is_empty()
        && response.drag_started_by(egui::PointerButton::Primary)
        && let (Some(start), Some(now)) = pointer
    {
        state.selection = Some((start, now));
    }
    if let (Some(selection), Some(now)) = (&mut state.selection, pointer.1) {
        selection.1 = now;
        let area = egui::Rect::from_two_pos(selection.0, selection.1);
        painter.rect_filled(area, 2.0, MARK);
    }
    if response.drag_stopped()
        && let Some(selection) = state.selection.take()
    {
        // the page it started on
        let under = pages.iter().find(|(_, drawn)| drawn.contains(selection.0));
        if let Some((page, drawn)) = under
            && let Some(area) = shares(selection, *drawn)
        {
            let boxed = egui::Rect::from_two_pos(selection.0, selection.1);
            out.push(Command::MangaRead {
                page: *page,
                area,
                anchor: popup::Rect {
                    x: f64::from(boxed.min.x),
                    y: f64::from(boxed.min.y),
                    w: f64::from(boxed.width()),
                    h: f64::from(boxed.height()),
                },
            });
        }
    }
    let step = pages.len();
    // enlarged, the wheel moves the page instead of turning it
    let (mut forward, mut back) = turns(ui, response.hovered() && !zoomed.on);
    // PORT-003: right to left, a swipe to the right goes on
    match state.swipe.take() {
        Some(crate::gestures::Gesture::SwipeRight) => forward = true,
        Some(crate::gestures::Gesture::SwipeLeft) => back = true,
        None => {}
    }
    if forward && open.chapter + step < open.book.chapters.len() {
        out.push(Command::BookChapter(open.chapter + step));
    } else if back && open.chapter > 0 {
        out.push(Command::BookChapter(open.chapter.saturating_sub(step)));
    }
}

/// A manga's view, enlarged: how its pages are moved.
struct Zoomed {
    on: bool,
    centre: egui::Pos2,
    zoom: f32,
    pan: egui::Vec2,
}

impl Zoomed {
    /// Where a page drawn whole at `whole` is drawn now.
    fn place(&self, whole: egui::Rect) -> egui::Rect {
        let at = |p: egui::Pos2| self.centre + (p - self.centre) * self.zoom + self.pan;
        egui::Rect::from_min_max(at(whole.min), at(whole.max))
    }
}

/// Ctrl and the wheel, or a pinch, over a manga's page enlarge it about the
/// pointer, up to [`MANGA_ZOOM`] times; enlarged, the wheel (with Shift,
/// across) and the middle button move it, and a new page shows its top
/// right, where it is read from (RDR-005).
fn manga_zoom(
    ui: &Ui,
    open: &OpenBook,
    rect: egui::Rect,
    whole: &[(usize, egui::Rect, &egui::TextureHandle)],
    response: &egui::Response,
    state: &mut State,
) -> Zoomed {
    let shown = Some((open.uid.clone(), open.chapter));
    let turned = state.zoomed_page != shown;
    state.zoomed_page = shown;
    let centre = rect.center();
    let mut zoom = state.manga_zoom.max(1.0);
    // over a balloon too, which lies on the page
    if response.contains_pointer() {
        let (delta, scroll, pointer) =
            ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
        if delta != 1.0 {
            let next = (zoom * delta).clamp(1.0, MANGA_ZOOM);
            // the point under the pointer stays there
            let p = pointer.unwrap_or(centre) - centre;
            state.pan = p - (p - state.pan) * (next / zoom);
            zoom = next;
        } else if zoom > 1.0 {
            state.pan += scroll;
        }
        if zoom > 1.0 && response.dragged_by(egui::PointerButton::Middle) {
            state.pan += response.drag_delta();
        }
    }
    if turned && zoom > 1.0 {
        state.pan = vec2(f32::NEG_INFINITY, f32::INFINITY);
    }
    state.manga_zoom = zoom;
    // kept on the view: never past the pages' edges
    let bounds = whole
        .iter()
        .map(|(_, r, _)| *r)
        .reduce(|a, b| a.union(b))
        .unwrap_or(rect);
    let base = (bounds.center() - centre) * zoom;
    let room = (bounds.size() * zoom - rect.size()).max(egui::Vec2::ZERO) / 2.0;
    state.pan = vec2(
        state.pan.x.clamp(-room.x - base.x, room.x - base.x),
        state.pan.y.clamp(-room.y - base.y, room.y - base.y),
    );
    if zoom <= 1.0 {
        state.pan = egui::Vec2::ZERO;
    }
    Zoomed {
        on: zoom > 1.0,
        centre,
        zoom,
        pan: state.pan,
    }
}

/// A picture taller than wide: one page.
fn is_upright(picture: &egui::TextureHandle) -> bool {
    let size = picture.size_vec2();
    size.y >= size.x
}

/// How many pages a turn goes: two in pairs, once past the cover.
fn page_step(s: &Scene<'_>, open: &OpenBook) -> usize {
    if open.book.manga && s.double && open.chapter > 0 {
        2
    } else {
        1
    }
}

/// A chapter in columns from the right, a page at a time (BOK-009).
fn vertical_reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let Some(chapter) = open.book.chapters.get(open.chapter) else {
        return;
    };
    let room = ui.available_rect_before_wrap();
    let rows = ((room.height() / (text_size() * CELL)) as usize).max(2);
    let per_page = ((room.width() / (text_size() * COLUMN_WIDTH)) as usize).max(1);
    let key = (open.uid.clone(), open.chapter, rows, per_page);
    if state.columns.as_ref().map(|(k, _)| k) != Some(&key) {
        state.columns = Some((key, lay_out(&chapter.paragraphs, rows, per_page)));
        state.column = usize::MAX;
    }
    let Some((key, columns)) = state.columns.take() else {
        return;
    };
    let kept = Some((key, ()));
    if open.jump || state.column == usize::MAX {
        state.column = columns.column_of(open.paragraph) / per_page * per_page;
        out.push(Command::BookAt(open.paragraph));
    }
    let width = per_page as f32 * text_size() * COLUMN_WIDTH;
    let area = egui::Rect::from_min_size(
        egui::pos2(room.center().x - width / 2.0, room.top()),
        vec2(width, room.height()),
    );
    let text: String = columns
        .cells
        .iter()
        .filter(|c| (state.column..state.column + per_page).contains(&c.column))
        .map(|c| c.ch)
        .collect();
    let (_, response) = ui.allocate_exact_size(room.size(), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &text));
    let cells: Vec<&Cell> = columns
        .cells
        .iter()
        .filter(|c| (state.column..state.column + per_page).contains(&c.column))
        .collect();
    // the paragraph being heard (ABK-005)
    if let Some((chapter, heard)) = s.page.along.as_ref().and_then(|a| a.heard)
        && chapter == open.chapter
    {
        for cell in cells.iter().filter(|c| c.paragraph == heard) {
            let box_size = vec2(text_size() * 1.1, text_size() * CELL);
            let rect =
                egui::Rect::from_center_size(cell_centre(area, state.column, cell), box_size);
            ui.painter()
                .rect_filled(rect, 0.0, palette::current().accent.gamma_multiply(0.15));
        }
    }
    if let Some((paragraph, range)) = &s.page.highlight {
        for cell in cells
            .iter()
            .filter(|c| c.paragraph == *paragraph && range.contains(&c.offset))
        {
            let box_size = vec2(text_size() * 1.1, text_size() * CELL);
            let rect =
                egui::Rect::from_center_size(cell_centre(area, state.column, cell), box_size);
            ui.painter().rect_filled(rect, 2.0, MARK);
        }
    }
    for cell in &cells {
        draw_cell(ui, area, state.column, cell);
    }
    let shown: std::collections::BTreeSet<_> = cells.iter().map(|c| c.paragraph).collect();
    if s.colours {
        for &index in &shown {
            if let Some(item) = chapter.paragraphs.get(index)
                && let Some(marks) = (s.marks)(&item.text)
            {
                // A paragraph may span many pages. Search its disjoint status
                // ranges for each visible cell instead of scanning the page
                // for every word in that whole paragraph.
                let mut ranges: Vec<_> = marks.underlines().collect();
                ranges.sort_unstable_by_key(|(bytes, _)| bytes.start);
                for cell in cells.iter().filter(|c| c.paragraph == index) {
                    let after = ranges.partition_point(|(bytes, _)| bytes.start <= cell.offset);
                    if let Some((bytes, why)) = after.checked_sub(1).and_then(|i| ranges.get(i))
                        && bytes.contains(&cell.offset)
                    {
                        let centre = cell_centre(area, state.column, cell);
                        // Native ruby is to the right; leave it room.
                        let line = egui::Rect::from_center_size(
                            centre - vec2(text_size() * 0.55, 0.0),
                            vec2(2.0, text_size() * CELL),
                        );
                        if ui.clip_rect().intersects(line) {
                            ui.painter()
                                .rect_filled(line, 0.0, palette::underline(*why));
                            state.drawn_marks.record(*why);
                        }
                    }
                }
            }
        }
        out.push(Command::BookRows(
            open.uid.clone(),
            open.chapter,
            shown.into_iter().collect(),
        ));
    }
    let picture = columns
        .pictures
        .iter()
        .find(|(_, column)| *column == state.column)
        .and_then(|(paragraph, _)| chapter.paragraphs.get(*paragraph)?.image.clone());
    if let Some(handle) = picture.and_then(|key| texture(ui.ctx(), state, open, &key)) {
        draw_picture(ui, room, &handle);
    }
    draw_readings(ui, area, state.column, &cells, &chapter.paragraphs);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let near = cells.iter().min_by_key(|c| {
            let d = cell_centre(area, state.column, c) - at;
            (d.length_sq() * 16.0) as u64
        });
        if let Some(cell) = near {
            let centre = cell_centre(area, state.column, cell);
            out.push(Command::BookClicked {
                paragraph: cell.paragraph,
                offset: cell.offset,
                anchor: popup::Rect {
                    x: f64::from(centre.x - text_size() / 2.0),
                    y: f64::from(centre.y - text_size() / 2.0),
                    w: f64::from(text_size()),
                    h: f64::from(text_size()),
                },
            });
        }
    }
    let (mut forward, mut back) = turns(ui, response.hovered());
    // PORT-003: right to left, a swipe to the right goes on
    match state.swipe.take() {
        Some(crate::gestures::Gesture::SwipeRight) => forward = true,
        Some(crate::gestures::Gesture::SwipeLeft) => back = true,
        None => {}
    }
    let total = columns.columns;
    if forward {
        if state.column + per_page < total {
            state.column += per_page;
            out.push(Command::BookAt(columns.paragraph_at(state.column)));
        } else if open.chapter + 1 < open.book.chapters.len() {
            out.push(Command::BookChapter(open.chapter + 1));
        }
    } else if back {
        if state.column > 0 {
            state.column = state.column.saturating_sub(per_page);
            out.push(Command::BookAt(columns.paragraph_at(state.column)));
        } else if open.chapter > 0 {
            out.push(Command::BookChapter(open.chapter - 1));
        }
    }
    if let Some((key, _)) = kept {
        state.columns = Some((key, columns));
    }
}

fn reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    state: &mut State,
    out: &mut Vec<Command>,
) -> bool {
    let key = (open.uid.clone(), open.chapter);
    if state.study_key.as_ref() != Some(&key) {
        state.study_key = Some(key);
        state.words_open = false;
        state.word_query.clear();
        state.words = super::word_list::State::default();
    }
    if state.book != open.uid {
        // the same name may be another picture in another book
        state.book.clone_from(&open.uid);
        state.textures.clear();
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr(s, "phrases-title")).clicked() {
            out.push(Command::PhrasesOpen);
        }
        if ui.button(tr(s, "search-title")).clicked() {
            out.push(Command::ReaderToolsOpen(crate::reader_tools::Tab::Search));
        }
        if !open.book.manga {
            use nikomiru_library::annotations::Kind;
            if ui.button(tr(s, "annotations-title")).clicked() {
                out.push(Command::ReaderToolsOpen(
                    crate::reader_tools::Tab::Annotations,
                ));
            }
            for kind in [Kind::Bookmark, Kind::Highlight, Kind::Note] {
                if ui
                    .button(tr(s, super::reader_tools::new_label(kind)))
                    .clicked()
                {
                    out.push(Command::AnnotationNew(kind));
                }
            }
            if !s.vertical {
                ui.separator();
                ui.label(tr(s, "books-mode"));
                for (mode, label) in [
                    ("scroll", "books-mode-scroll"),
                    ("pages", "books-mode-pages"),
                    ("sentence", "books-mode-sentence"),
                ] {
                    let chosen = s.mode == mode;
                    let button = ui.selectable_label(chosen, tr(s, label));
                    if button.clicked() && !chosen {
                        out.push(Command::BookMode(mode.into()));
                    }
                }
            }
        }
    });
    let manga = open.book.manga;
    let (next, previous) = if manga {
        ("books-next-page", "books-previous-page")
    } else {
        ("books-next", "books-previous")
    };
    let step = page_step(s, open);
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button(tr(s, "books-back")).clicked() {
            close = true;
        }
        // the page turns first, at the right: the title is cut before them
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let last = open.book.chapters.len().saturating_sub(1);
            if ui
                .add_enabled(open.chapter < last, Button::new(tr(s, next)))
                .clicked()
            {
                out.push(Command::BookChapter((open.chapter + step).min(last)));
            }
            let number = format!("{} / {}", open.chapter + 1, open.book.chapters.len());
            ui.label(RichText::new(number).color(palette::current().details));
            if ui
                .add_enabled(open.chapter > 0, Button::new(tr(s, previous)))
                .clicked()
            {
                out.push(Command::BookChapter(open.chapter.saturating_sub(step)));
            }
            if manga
                && ui
                    .selectable_label(s.double, tr(s, "manga-double"))
                    .clicked()
            {
                out.push(Command::MangaDouble(!s.double));
            }
            if manga
                && ui
                    .selectable_label(s.manga_text, tr(s, "manga-text"))
                    .clicked()
            {
                out.push(Command::MangaText(!s.manga_text));
            }
            if !manga
                && ui
                    .selectable_label(s.vertical, tr(s, "books-vertical"))
                    .clicked()
            {
                out.push(Command::BookWriting(!s.vertical));
            }
            // ABK-005
            if !manga {
                let along = s.page.along.is_some();
                if along && ui.button(tr(s, "books-listen")).clicked() {
                    out.push(Command::BookListen);
                }
                let toggle = ui
                    .add_enabled(
                        along || s.can_read_along,
                        egui::Button::selectable(along, tr(s, "books-along")),
                    )
                    .on_disabled_hover_text(tr(s, "books-along-hint"));
                if toggle.clicked() {
                    out.push(Command::BookAlong(!along));
                }
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&open.book.title).family(theme::bold()))
                        .truncate(),
                );
            });
        });
    });
    if manga {
        manga_reader(ui, s, open, state, out);
        return close;
    }
    if s.colours {
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(!state.words_open, tr(s, "books-read"))
                .clicked()
            {
                state.words_open = false;
            }
            if ui
                .selectable_label(state.words_open, tr(s, "books-words"))
                .clicked()
            {
                state.words_open = true;
            }
            let study = &s.page.study;
            let percent = |p: Option<usize>| p.map_or_else(|| "–".into(), |n| n.to_string());
            let score = match study.score {
                Some(Ok(score)) => Some((s.tr)(
                    "books-chapter-score",
                    &[
                        ("known", percent(score.known_percent())),
                        ("clear", percent(score.clear_percent())),
                    ],
                )),
                Some(Err(done)) => Some((s.tr)(
                    "books-chapter-counting",
                    &[
                        ("done", done.to_string()),
                        ("of", study.paragraphs.to_string()),
                    ],
                )),
                None if study.limited => Some(tr(s, "books-chapter-too-long")),
                None => None,
            };
            if let Some(score) = score {
                ui.label(score);
            }
        });
        if state.words_open {
            let label = ui.label(tr(s, "books-words-search"));
            ui.add(egui::TextEdit::singleline(&mut state.word_query).desired_width(320.0))
                .labelled_by(label.id);
            if let Some(words) = &s.page.study.words {
                let mut labels =
                    super::word_list::Labels::translated(s.tr, state.words.ticked.len());
                labels.source = open.book.title.clone();
                super::word_list::show(
                    ui,
                    words,
                    &mut state.word_query,
                    &labels,
                    &mut state.words,
                    out,
                );
            }
            return close;
        }
    } else {
        state.words_open = false;
    }
    if s.vertical {
        vertical_reader(ui, s, open, state, out);
        return close;
    }
    let Some(chapter) = open.book.chapters.get(open.chapter) else {
        return close;
    };
    match s.mode {
        "pages" => paged_reader(ui, s, open, chapter, state, out),
        "sentence" => sentence_reader(ui, s, open, chapter, state, out),
        _ => {
            // a scrolled chapter has no pages to turn
            state.swipe = None;
            scroll_reader(ui, s, open, chapter, state, out);
        }
    }
    close
}

/// The paragraph's marks: the word looked up (or the text an annotation or
/// a search led to) and its current highlights and notes.
fn paragraph_marks(
    s: &Scene<'_>,
    index: usize,
) -> (Option<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let marked = s
        .page
        .highlight
        .as_ref()
        .or(s.page.found.as_ref())
        .filter(|(at, _)| *at == index)
        .map(|(_, range)| range.clone());
    let annotated = s
        .annotated
        .iter()
        .filter(|(at, _)| *at == index)
        .map(|(_, range)| range.clone())
        .collect();
    (marked, annotated)
}

/// Paragraphs `range` of the chapter, one under the other: those shown, and
/// the first whose bottom is under the top of the view.
fn draw_paragraphs(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    chapter: &nikomiru_books::Chapter,
    range: std::ops::Range<usize>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> (Vec<usize>, Option<usize>) {
    let mut shown = Vec::new();
    let width = ui.available_width().min(COLUMN);
    let mut reached = None;
    ui.vertical_centered(|ui| {
        ui.set_max_width(width);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = text_size() * 0.6;
            let top = ui.clip_rect().top();
            for (index, item) in chapter.paragraphs.iter().enumerate() {
                if !range.contains(&index) {
                    continue;
                }
                if let Some(key) = &item.image {
                    if let Some(handle) = texture(ui.ctx(), state, open, key) {
                        let size = handle.size_vec2();
                        let scale = (ui.available_width() / size.x).min(1.0);
                        let (rect, _) = ui.allocate_exact_size(size * scale, Sense::hover());
                        draw_picture(ui, rect, &handle);
                    }
                    continue;
                }
                let name = format!("{index}: {}", item.text);
                let (marked, annotated) = paragraph_marks(s, index);
                let heard =
                    s.page.along.as_ref().and_then(|a| a.heard) == Some((open.chapter, index));
                let marks = (s.marks)(&item.text);
                let (clicked, rect, drawn) = paragraph(
                    ui,
                    item,
                    &name,
                    marked,
                    &annotated,
                    heard,
                    marks.as_ref(),
                    1.0,
                );
                state.drawn_marks.unknown += drawn.unknown;
                state.drawn_marks.learning += drawn.learning;
                if s.colours && ui.clip_rect().intersects(rect) {
                    shown.push(index);
                }
                if open.jump && index == open.paragraph && s.mode == "scroll" {
                    ui.scroll_to_rect(rect, Some(Align::Min));
                }
                if reached.is_none() && rect.bottom() > top {
                    reached = Some(index);
                }
                if let Some((offset, rect)) = clicked {
                    out.push(clicked_at(index, offset, rect));
                }
            }
        });
    });
    (shown, reached)
}

fn clicked_at(paragraph: usize, offset: usize, rect: egui::Rect) -> Command {
    Command::BookClicked {
        paragraph,
        offset,
        anchor: popup::Rect {
            x: f64::from(rect.min.x),
            y: f64::from(rect.min.y),
            w: f64::from(rect.width()),
            h: f64::from(rect.height()),
        },
    }
}

/// The whole chapter in one scrolling column (BOK-006).
fn scroll_reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    chapter: &nikomiru_books::Chapter,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let mut area = ScrollArea::vertical()
        .id_salt(("book", open.chapter))
        .auto_shrink([false, false]);
    if open.jump && open.paragraph == 0 {
        area = area.vertical_scroll_offset(0.0);
    }
    let (shown, reached) = area
        .show(ui, |ui| {
            draw_paragraphs(ui, s, open, chapter, 0..usize::MAX, state, out)
        })
        .inner;
    if let Some(index) = reached
        && (open.jump || index != open.paragraph)
    {
        out.push(Command::BookAt(if open.jump {
            open.paragraph
        } else {
            index
        }));
    }
    if s.colours {
        out.push(Command::BookRows(open.uid.clone(), open.chapter, shown));
    }
}

/// Whole paragraphs grouped into pages of `height` (RDR-004): a paragraph
/// taller than a page has a page of its own.
pub(crate) fn pages(heights: &[f32], height: f32, gap: f32) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (index, h) in heights.iter().enumerate() {
        let need = if index == start { *h } else { used + gap + h };
        if index > start && need > height {
            out.push(start..index);
            start = index;
            used = *h;
        } else {
            used = need;
        }
    }
    if start < heights.len() || out.is_empty() {
        out.push(start..heights.len());
    }
    out
}

/// The navigation row of a page or a sentence: previous, where, next.
fn turn_row(
    ui: &mut Ui,
    s: &Scene<'_>,
    labels: (&str, &str),
    place: String,
    can: (bool, bool),
) -> (bool, bool) {
    let mut turned = (false, false);
    ui.horizontal(|ui| {
        turned.0 = ui
            .add_enabled(can.0, Button::new(tr(s, labels.0)))
            .clicked();
        ui.label(RichText::new(place).color(palette::current().details));
        turned.1 = ui
            .add_enabled(can.1, Button::new(tr(s, labels.1)))
            .clicked();
    });
    turned
}

/// The chapter a page at a time (RDR-004).
fn paged_reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    chapter: &nikomiru_books::Chapter,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let room = ui.available_rect_before_wrap();
    let width = room.width().min(COLUMN);
    let height = (room.height() - text_size() * 3.0).max(text_size() * 4.0);
    let zoom = (TEXT_ZOOM.with(std::cell::Cell::get) * 100.0).round() as u32;
    let key = (
        open.uid.clone(),
        open.chapter,
        width as u32,
        height as u32,
        zoom,
    );
    if state.pages.as_ref().map(|(k, _)| k) != Some(&key) {
        let heights: Vec<f32> = chapter
            .paragraphs
            .iter()
            .map(|item| match &item.image {
                Some(key) => texture(ui.ctx(), state, open, key).map_or(0.0, |h| {
                    let size = h.size_vec2();
                    size.y * (width / size.x).min(1.0)
                }),
                None => paragraph_galley(ui, item, width, 1.0).size().y,
            })
            .collect();
        state.pages = Some((key, pages(&heights, height, text_size() * 0.6)));
    }
    let Some((_, groups)) = state.pages.as_ref() else {
        return;
    };
    let groups = groups.clone();
    let page = groups
        .iter()
        .position(|g| g.contains(&open.paragraph))
        .unwrap_or(groups.len().saturating_sub(1));
    let last_chapter = open.book.chapters.len().saturating_sub(1);
    let place = (s.tr)(
        "books-page-of",
        &[
            ("page", (page + 1).to_string()),
            ("pages", groups.len().to_string()),
        ],
    );
    let can = (
        page > 0 || open.chapter > 0,
        page + 1 < groups.len() || open.chapter < last_chapter,
    );
    let (mut back, mut forward) = turn_row(
        ui,
        s,
        ("books-previous-page", "books-next-page"),
        place,
        can,
    );
    // Page Up/Down, through the shortcut layer; a swipe (PORT-003)
    match state.turn.take() {
        Some(true) => forward |= can.1,
        Some(false) => back |= can.0,
        None => {}
    }
    match state.swipe.take() {
        Some(crate::gestures::Gesture::SwipeLeft) => forward |= can.1,
        Some(crate::gestures::Gesture::SwipeRight) => back |= can.0,
        None => {}
    }
    if forward {
        match groups.get(page + 1) {
            Some(next) => out.push(Command::BookTurn(next.start)),
            None => out.push(Command::BookChapter(open.chapter + 1)),
        }
    } else if back {
        match page.checked_sub(1).and_then(|p| groups.get(p)) {
            Some(previous) => out.push(Command::BookTurn(previous.start)),
            None => out.push(Command::BookChapter(open.chapter.saturating_sub(1))),
        }
    }
    if open.jump {
        out.push(Command::BookAt(open.paragraph));
    }
    let shown_range = groups.get(page).cloned().unwrap_or(0..0);
    let (shown, _) = ScrollArea::vertical()
        .id_salt(("book-page", open.chapter, page))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            draw_paragraphs(ui, s, open, chapter, shown_range, state, out)
        })
        .inner;
    if s.colours {
        out.push(Command::BookRows(open.uid.clone(), open.chapter, shown));
    }
}

/// One sentence at a time, larger, in its original paragraph (RDR-004).
fn sentence_reader(
    ui: &mut Ui,
    s: &Scene<'_>,
    open: &OpenBook,
    chapter: &nikomiru_books::Chapter,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let index = open
        .paragraph
        .min(chapter.paragraphs.len().saturating_sub(1));
    let item = chapter.paragraphs.get(index);
    let sentences = item
        .filter(|p| p.image.is_none())
        .map(|p| nikomiru_text::sentence::sentences(&p.text))
        .unwrap_or_default();
    let at = open.sentence.min(sentences.len().saturating_sub(1));
    let last_chapter = open.book.chapters.len().saturating_sub(1);
    let more_here = chapter
        .paragraphs
        .iter()
        .skip(index + 1)
        .any(|p| p.image.is_none() && !p.text.trim().is_empty());
    let place = (s.tr)(
        "books-sentence-of",
        &[
            ("sentence", (at + 1).to_string()),
            ("sentences", sentences.len().max(1).to_string()),
            ("paragraph", (index + 1).to_string()),
        ],
    );
    let can = (
        at > 0 || index > 0 || open.chapter > 0,
        at + 1 < sentences.len() || more_here || open.chapter < last_chapter,
    );
    let (mut back, mut forward) = turn_row(
        ui,
        s,
        ("books-previous-sentence", "books-next-sentence"),
        place,
        can,
    );
    match state.swipe.take() {
        Some(crate::gestures::Gesture::SwipeLeft) => forward |= can.1,
        Some(crate::gestures::Gesture::SwipeRight) => back |= can.0,
        None => {}
    }
    if forward {
        out.push(Command::BookSentence(true));
    } else if back {
        out.push(Command::BookSentence(false));
    }
    if open.jump {
        out.push(Command::BookAt(index));
    }
    let Some(item) = item else {
        return;
    };
    if item.image.is_some() {
        draw_paragraphs(ui, s, open, chapter, index..index + 1, state, out);
        return;
    }
    let Some(range) = sentences.get(at).cloned() else {
        return;
    };
    // the sentence as a paragraph of its own, its marks moved with it
    let shift = |r: &std::ops::Range<usize>| -> Option<std::ops::Range<usize>> {
        let start = r.start.max(range.start);
        let end = r.end.min(range.end);
        (start < end).then(|| start - range.start..end - range.start)
    };
    let part = Paragraph {
        text: item.text[range.clone()].to_string(),
        ruby: item
            .ruby
            .iter()
            .filter(|r| r.range.start >= range.start && r.range.end <= range.end)
            .map(|r| nikomiru_books::Ruby {
                range: r.range.start - range.start..r.range.end - range.start,
                text: r.text.clone(),
            })
            .collect(),
        heading: item.heading,
        image: None,
    };
    let (marked, annotated) = paragraph_marks(s, index);
    let marked = marked.as_ref().and_then(shift);
    let annotated: Vec<_> = annotated.iter().filter_map(shift).collect();
    let heard = s.page.along.as_ref().and_then(|a| a.heard) == Some((open.chapter, index));
    let marks = (s.marks)(&part.text);
    let name = format!("{index}: {}", part.text);
    ScrollArea::vertical()
        .id_salt(("book-sentence", open.chapter, index, at))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(text_size() * 2.0);
            let width = ui.available_width().min(COLUMN);
            ui.vertical_centered(|ui| {
                ui.set_max_width(width);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    let (clicked, _, drawn) = paragraph(
                        ui,
                        &part,
                        &name,
                        marked,
                        &annotated,
                        heard,
                        marks.as_ref(),
                        1.4,
                    );
                    state.drawn_marks.unknown += drawn.unknown;
                    state.drawn_marks.learning += drawn.learning;
                    if let Some((offset, rect)) = clicked {
                        out.push(clicked_at(index, range.start + offset, rect));
                    }
                });
            });
        });
    if s.colours {
        out.push(Command::BookRows(
            open.uid.clone(),
            open.chapter,
            vec![index],
        ));
    }
}

fn whole_book_score(
    ui: &mut Ui,
    s: &Scene<'_>,
    uid: &str,
    title: &str,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    use crate::book_scores::Status;
    let status = s.page.scores.get(uid);
    match status {
        Some(Status::Counting) => {
            ui.spinner();
            ui.label(tr(s, "books-score-counting"));
        }
        Some(Status::Score(score)) => {
            let percent =
                |value: Option<usize>| value.map_or_else(|| "–".into(), |n| n.to_string());
            let args = [
                ("known", percent(score.known_percent())),
                ("clear", percent(score.clear_percent())),
            ];
            ui.label((s.tr)("books-score-summary", &args))
                .on_hover_text((s.tr)("books-score-detail", &args));
        }
        Some(Status::NoText) => {
            ui.label(tr(s, "books-score-no-text"));
        }
        Some(Status::TooLarge) => {
            ui.label(tr(s, "books-score-too-long"));
        }
        Some(Status::Failed(error)) => {
            ui.label(tr(s, "books-score-failed")).on_hover_text(error);
        }
        None => {}
    }
    if matches!(status, Some(Status::Counting)) && ui.button(tr(s, "analysis-cancel")).clicked() {
        out.push(Command::BookAnalysisCancel(uid.into()));
    }
    if s.page.scores.words(uid).is_some() && ui.button(tr(s, "analysis-words")).clicked() {
        state.analysis = Some(uid.into());
    }
    let button = ui.add_enabled(
        !s.page.scores.busy(),
        Button::new(tr(
            s,
            if status.is_some() {
                "books-score-refresh"
            } else {
                "books-score-analyse"
            },
        )),
    );
    button.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            button.enabled(),
            (s.tr)("books-score-analyse-title", &[("title", title.into())]),
        )
    });
    if button
        .on_disabled_hover_text(tr(s, "books-score-wait"))
        .clicked()
    {
        out.push(Command::BookAnalyse(uid.into()));
    }
}

fn list(
    ui: &mut Ui,
    s: &Scene<'_>,
    state: &mut State,
    add: &mut bool,
    close: &mut bool,
    out: &mut Vec<Command>,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr(s, "books-title"))
                .size(24.0)
                .family(theme::bold()),
        );
        if s.page.busy {
            ui.spinner();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr(s, "library-close")).clicked() {
                *close = true;
            }
        });
    });
    // the actions on a row of their own, wrapped: never over the title
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!s.page.busy, palette::suggested(tr(s, "books-add")))
            .clicked()
        {
            *add = true;
        }
        if ui.button(tr(s, "books-folder")).clicked() {
            state.folder = true;
        }
        if ui.button(tr(s, "books-audiobook")).clicked() {
            state.audiobook = true;
        }
        if ui.button(tr(s, "podcasts-title")).clicked() {
            out.push(Command::PodcastsOpen);
        }
        if ui.button(tr(s, "annotations-title")).clicked() {
            out.push(Command::ReaderToolsOpen(
                crate::reader_tools::Tab::Annotations,
            ));
        }
        if ui.button(tr(s, "search-title")).clicked() {
            out.push(Command::ReaderToolsOpen(crate::reader_tools::Tab::Search));
        }
    });
    if let Some(uid) = state.analysis.clone() {
        if s.colours
            && let Some((serial, words)) = s.page.scores.words(&uid)
            && let Some((_, book)) = s.page.list.iter().find(|(u, _)| u == &uid)
        {
            if super::analysis_words::show(
                ui,
                format!("{uid}/{serial}"),
                &book.title,
                words,
                &mut state.analysis_words,
                s.tr,
                out,
            ) {
                state.analysis = None;
            }
            return;
        }
        state.analysis = None;
        state.analysis_words = Default::default();
    }
    if s.colours {
        state.analysis_order.controls(ui, s.tr);
        let ready = s.page.list.iter().any(|(uid, _)| {
            matches!(
                s.page.scores.get(uid),
                Some(crate::book_scores::Status::Score(_))
            )
        });
        if ui
            .add_enabled(ready, Button::new(tr(s, "study-report-export")))
            .clicked()
        {
            out.push(Command::StudyReport(crate::study_report::Scope::Books));
        }
    }
    if let Some((text, ok)) = &s.page.status {
        let shade: Color32 = if *ok {
            palette::current().good
        } else {
            palette::current().bad
        };
        ui.add(egui::Label::new(RichText::new(text).color(shade)).wrap());
    }
    if ui
        .add_enabled(!s.page.busy, Button::new(tr(s, "books-paste")))
        .clicked()
    {
        state.paste_open = true;
    }
    if ui.button(tr(s, "phrases-title")).clicked() {
        out.push(Command::PhrasesOpen);
    }
    // WEB-001: a web page, read as a book
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "books-web"));
        let field = ui
            .add(egui::TextEdit::singleline(&mut state.address).desired_width(420.0))
            .labelled_by(label.id);
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let open = ui.add_enabled(
            !s.page.busy && !state.address.trim().is_empty(),
            palette::suggested(tr(s, "books-web-open")),
        );
        if (open.clicked() || enter) && !state.address.trim().is_empty() {
            out.push(Command::BookPage(state.address.trim().to_string()));
        }
    });
    if s.page.list.is_empty() {
        ui.label(RichText::new(tr(s, "books-none")).color(palette::current().details));
    }
    ScrollArea::vertical()
        .id_salt("books")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let indices = if s.colours {
                state.analysis_order.indices(s.page.list.len(), |i| {
                    match s.page.scores.get(&s.page.list[i].0) {
                        Some(crate::book_scores::Status::Score(score)) => Some(*score),
                        _ => None,
                    }
                })
            } else {
                (0..s.page.list.len()).collect()
            };
            for i in indices {
                let (uid, record) = &s.page.list[i];
                ui.horizontal(|ui| {
                    let name = if record.author.is_empty() {
                        record.title.clone()
                    } else {
                        format!("{} — {}", record.title, record.author)
                    };
                    if ui.button(name).clicked() {
                        out.push(Command::BookChoose(uid.clone()));
                    }
                    ui.label(
                        RichText::new(format!("{} %", percent(record)))
                            .color(palette::current().details),
                    );
                    if s.colours {
                        ui.push_id(("book-score", uid), |ui| {
                            whole_book_score(ui, s, uid, &record.title, state, out)
                        });
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = (s.tr)("books-remove", &[("title", record.title.clone())]);
                        if ui.button(label).clicked() {
                            out.push(Command::BookRemove(uid.clone()));
                        }
                    });
                });
            }
        });
}

fn paste_editor(ctx: &egui::Context, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    if !state.paste_open {
        return;
    }
    let mut open = true;
    let mut submitted = false;
    egui::Window::new(tr(s, "books-paste"))
        .id(Id::unique("paste-book-editor"))
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            let label = ui.label(tr(s, "books-paste-title"));
            ui.add(egui::TextEdit::singleline(&mut state.paste_title).char_limit(256))
                .labelled_by(label.id);
            ui.checkbox(&mut state.paste_markdown, tr(s, "books-paste-markdown"));
            ui.label(tr(s, "books-paste-hint"));
            let label = ui.label(tr(s, "books-paste-content"));
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.45)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut state.paste_text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(12)
                            .char_limit(nikomiru_books::text::MAX_BYTES),
                    )
                    .labelled_by(label.id);
                });
            let ready = !s.page.busy
                && !state.paste_title.trim().is_empty()
                && !state.paste_title.chars().any(char::is_control)
                && !state.paste_text.trim().is_empty()
                && state.paste_text.len() <= nikomiru_books::text::MAX_BYTES;
            if ui
                .add_enabled(ready, Button::new(tr(s, "books-paste-read")))
                .clicked()
            {
                out.push(Command::BookPaste(crate::books::Paste {
                    title: state.paste_title.trim().into(),
                    text: state.paste_text.clone(),
                    format: if state.paste_markdown {
                        nikomiru_books::text::Format::Markdown
                    } else {
                        nikomiru_books::text::Format::Plain
                    },
                }));
                submitted = true;
            }
        });
    state.paste_open = open && !submitted;
}

/// Draws the page; whether it closes, and whether a file is to be chosen.
/// Ctrl and the wheel, or a pinch, over a book's text make it larger or
/// smaller; the page draws at that size at once, and the setting follows
/// (RDR-005). A manga's pages are enlarged instead, in its reader.
fn text_zoom(ui: &Ui, s: &Scene<'_>, open: &OpenBook, state: &mut State, out: &mut Vec<Command>) {
    let mut zoom = if state.text_zoom > 0.0 {
        state.text_zoom
    } else {
        s.zoom
    };
    let over = ui.rect_contains_pointer(ui.max_rect());
    let delta = ui.input(|i| i.zoom_delta());
    if over && !open.book.manga && delta != 1.0 {
        let range = crate::books::TEXT_ZOOM;
        zoom = (zoom * delta).clamp(*range.start(), *range.end());
        state.text_zoom = zoom;
        if crate::books::text_zoom(Some(zoom)) != s.zoom {
            out.push(Command::BookZoom(zoom));
        }
    } else if state.text_zoom > 0.0 && crate::books::text_zoom(Some(state.text_zoom)) == s.zoom {
        // the setting has caught up
        state.text_zoom = 0.0;
        zoom = s.zoom;
    }
    TEXT_ZOOM.with(|z| z.set(zoom));
}

pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> (bool, bool) {
    state.drawn_marks = DrawnMarks::default();
    let screen = ctx.content_rect();
    let (mut close, mut add) = (false, false);
    // under the popups, which are in the foreground (BOK-006)
    Area::new(Id::unique("books-page"))
        .order(Order::Middle)
        .fade_in(false)
        .constrain(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            Frame::new()
                .fill(palette::current().page)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_min_size(screen.size() - vec2(48.0, 48.0));
                    ui.set_max_size(screen.size() - vec2(48.0, 48.0));
                    ui.visuals_mut().override_text_color = Some(palette::current().text);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    match &s.page.open {
                        Some(open) => {
                            text_zoom(ui, s, open, state, out);
                            if reader(ui, s, open, state, out) {
                                out.push(Command::BookClose);
                                state.reader_closed = true;
                            }
                        }
                        None => {
                            state.study_key = None;
                            list(ui, s, state, &mut add, &mut close, out);
                        }
                    }
                });
        });
    paste_editor(ctx, s, state, out);
    (close, add)
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_books::{Book, Chapter, Ruby};

    use super::*;

    // T-BOK-002, T-WEB-004: a base that ends its row keeps its furigana on
    // that row, over its own glyphs: not over the row above, and not
    // stretched to the next row's start
    #[test]
    fn a_base_at_the_end_of_a_row_keeps_its_reading_there() {
        let mut h = Harness::builder().allow_missing_glyphs().build_ui(|ui| {
            let item = Paragraph {
                text: "あいうえお漢字かきくけこ".into(),
                ruby: vec![Ruby {
                    range: 15..21,
                    text: "かんじ".into(),
                }],
                ..Paragraph::default()
            };
            // seven characters fit on the first row: the base ends it
            let one = glyph_rows(&paragraph_galley(ui, &item, 10_000.0, 1.0), 0..1)[0].width();
            let galley = paragraph_galley(ui, &item, one * 7.2, 1.0);
            assert!(galley.rows.len() >= 2, "{}", galley.rows.len());
            let rows = range_rects(&galley, &item.text, 15..21);
            assert_eq!(rows.len(), 1, "{rows:?}");
            let base = rows[0];
            let first = glyph_rows(&galley, 0..1)[0];
            // on the first row, the text's own height, not the row's
            assert!((base.top() - first.top()).abs() < 1.0, "{base:?} {first:?}");
            assert!(base.height() < TEXT_SIZE * 1.6, "{base:?}");
            // two glyphs wide, not one glyph and a whole row height
            assert!(base.width() < one * 2.4, "{base:?}");
            let next = glyph_rows(&galley, 7..8)[0];
            assert!(next.top() > base.bottom(), "{next:?}");
        });
        h.run();
    }

    struct Test {
        page: BooksPage,
        marks: Marks,
        colours: bool,
        vertical: bool,
        double: bool,
        state: State,
        out: Vec<Command>,
        add: bool,
        /// Something with timed text plays (ABK-005).
        can_read_along: bool,
        mode: &'static str,
        annotated: Vec<(usize, std::ops::Range<usize>)>,
        zoom: f32,
        manga_text: bool,
    }

    fn harness<'a>(page: BooksPage) -> Harness<'a, Test> {
        vertical_harness(page, false)
    }

    fn vertical_harness<'a>(page: BooksPage, vertical: bool) -> Harness<'a, Test> {
        Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        marks: &|_| Some(t.marks.clone()),
                        colours: t.colours,
                        page: &t.page,
                        vertical: t.vertical,
                        double: t.double,
                        can_read_along: t.can_read_along,
                        mode: t.mode,
                        annotated: &t.annotated,
                        zoom: t.zoom,
                        manga_text: t.manga_text,
                        tr: &tr,
                    };
                    t.add |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out).1;
                },
                Test {
                    page,
                    marks: Marks::default(),
                    colours: false,
                    vertical,
                    double: false,
                    state: State::default(),
                    out: Vec::new(),
                    add: false,
                    can_read_along: false,
                    mode: "scroll",
                    annotated: Vec::new(),
                    zoom: 1.0,
                    manga_text: true,
                },
            )
    }

    // T-KNW-016: the explicit list action never opens the reader; hidden
    // when colours are off, and disabled while another analysis is active.
    #[test]
    fn whole_book_analysis_is_requested_from_the_list() {
        let mut h = harness(BooksPage {
            list: vec![(
                "book/a".into(),
                nikomiru_library::books::BookRecord {
                    path: "a.epub".into(),
                    title: "Book".into(),
                    ..Default::default()
                },
            )],
            ..Default::default()
        });
        h.run();
        assert!(h.query_by_label("books-score-analyse-title").is_none());
        h.state_mut().colours = true;
        h.run();
        // T-KNW-033: no completed result means the report action cannot emit a command.
        h.get_by_label("study-report-export").click();
        h.run();
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::StudyReport(_)))
        );
        h.get_by_label("books-score-analyse-title").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookAnalyse(uid) if uid == "book/a"))
        );
        assert!(h.state().page.open.is_none());
        h.state_mut().out.clear();
        h.state_mut()
            .page
            .scores
            .request("book/a".into(), "a.epub".into(), 0);
        h.run_steps(2);
        h.get_by_label("books-score-analyse-title").click();
        h.run_steps(2);
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookAnalyse(_)))
        );
        // T-KNW-019: accessible action is available during active analysis.
        h.get_by_label("analysis-cancel").click();
        h.run_steps(2);
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookAnalysisCancel(uid) if uid == "book/a"))
        );
    }

    // T-KNW-020: whole-book vocabulary is reached without opening the reader.
    #[test]
    fn whole_book_words_use_the_shared_bulk_status_controls() {
        let mut page = BooksPage {
            list: vec![(
                "book/a".into(),
                nikomiru_library::books::BookRecord {
                    path: "a.epub".into(),
                    title: "Book".into(),
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        page.scores.request("book/a".into(), "a.epub".into(), 0);
        let request = page.scores.take().pop().unwrap();
        page.scores.done(
            &request,
            Ok(crate::book_scores::Outcome::Counted(
                crate::words::Counted::from_lines([
                    vec![("猫".into(), "ねこ".into()); 2],
                    vec![("読む".into(), "よむ".into())],
                ]),
            )),
        );
        page.scores
            .update(true, &page.list, &crate::words::Words::default());
        page.list.push((
            "book/b".into(),
            nikomiru_library::books::BookRecord {
                title: "Other book".into(),
                path: "b.epub".into(),
                ..Default::default()
            },
        ));
        let mut h = harness(page);
        h.state_mut().colours = true;
        h.run();
        // T-KNW-024: filtering retained scores never starts an analysis.
        h.get_by_label("analysis-order");
        h.get_by_label("Other book");
        h.get_by_label("analysis-only-scored").click();
        h.run_steps(2);
        assert!(h.query_by_label("Other book").is_none());
        assert!(h.state().out.is_empty());
        // T-KNW-033: the completed book view exposes a scoped report action.
        h.get_by_label("study-report-export").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::StudyReport(crate::study_report::Scope::Books)))
        );
        h.state_mut().out.clear();

        h.state_mut().colours = false;
        h.run();
        h.get_by_label("Other book");
        h.state_mut().colours = true;
        h.run();
        h.get_by_label("analysis-words").click();
        h.run_steps(2);
        h.get_by_label("×2");
        h.get_by_label("subtitle-words-all").click();
        h.run();
        h.get_by_label("word-status-known").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c,
            Command::WordsStatus(words, crate::words::WordStatus::Known) if words.len() == 2)));
        assert!(h.state().page.open.is_none());
        h.state_mut()
            .page
            .scores
            .request("book/a".into(), "a.epub".into(), 0);
        h.run_steps(2);
        assert!(h.query_by_label("analysis-words-back").is_none());
        assert!(h.state().state.analysis.is_none());
    }

    fn open() -> OpenBook {
        let chapter = |text: &str| Chapter {
            title: None,
            paragraphs: vec![Paragraph {
                text: text.into(),
                ruby: vec![Ruby {
                    range: 0..3,
                    text: "よ".into(),
                }],
                heading: false,
                image: None,
            }],
        };
        OpenBook {
            uid: "book/a".into(),
            book: Book {
                identity: None,
                manga_regions: Default::default(),
                title: "本".into(),
                author: String::new(),
                right_to_left: false,
                manga: false,
                chapters: vec![chapter("読む本です"), chapter("次")],
                images: Default::default(),
            },
            chapter: 0,
            paragraph: 0,
            jump: false,
            started: (0, 0),
            sentence: 0,
        }
    }

    // T-BOK-006: a click gives the paragraph and the byte of the character
    // T-KNW-002: both writing directions draw status marks beside native ruby.
    #[test]
    fn status_marks_follow_the_writing_direction() {
        for vertical in [false, true] {
            let mut h = vertical_harness(
                BooksPage {
                    open: Some(open()),
                    ..BooksPage::default()
                },
                vertical,
            );
            h.state_mut().colours = true;
            h.state_mut().marks = Marks {
                unknown: vec![0..3, 6..9],
                learning: vec![3..6, 9..12],
                ..Marks::default()
            };
            h.run();
            assert_eq!(h.state().state.drawn_marks.unknown, 2);
            assert_eq!(h.state().state.drawn_marks.learning, 2);
            assert!(
                h.state()
                    .out
                    .iter()
                    .any(|c| matches!(c, Command::BookRows(_, 0, rows) if rows.contains(&0)))
            );
            h.state_mut().colours = false;
            h.state_mut().marks = Marks::default();
            h.run();
            assert_eq!(h.state().state.drawn_marks.unknown, 0);
        }
    }

    // T-KNW-002: a word crossing a row/column boundary keeps all its segments.
    #[test]
    fn status_marks_wrap_with_the_text() {
        for vertical in [false, true] {
            let mut open = open();
            open.book.chapters[0].paragraphs[0].text = "読む本です".repeat(100);
            let mut h = vertical_harness(
                BooksPage {
                    open: Some(open),
                    ..BooksPage::default()
                },
                vertical,
            );
            h.state_mut().colours = true;
            h.state_mut().marks = Marks {
                unknown: std::iter::once(0..300).collect(),
                learning: std::iter::once(300..600).collect(),
                ..Marks::default()
            };
            h.run();
            assert!(h.state().state.drawn_marks.unknown > 1);
            assert!(h.state().state.drawn_marks.learning > 1);
        }
    }

    // T-KNW-015: the reader uses the shared bulk-status widget and resets
    // selection/search when moving to another chapter.
    #[test]
    fn chapter_words_can_be_marked_together_and_reset_on_navigation() {
        use crate::words::{ListedWord, WordStatus};
        let mut page = BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        };
        page.study.words = Some(std::sync::Arc::new(vec![ListedWord {
            expression: "読む".into(),
            reading: "よむ".into(),
            count: 2,
            status: WordStatus::Unknown,
            card: false,
        }]));
        let mut h = harness(page);
        h.state_mut().colours = true;
        h.run();
        h.get_by_label("books-words").click();
        h.run();
        h.get_by_label("subtitle-words-all").click();
        h.run();
        h.get_by_label("word-status-known").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::WordsStatus(words, WordStatus::Known) if words == &vec![("読む".into(), "よむ".into())])));
        assert_eq!(h.state().page.open.as_ref().unwrap().paragraph, 0);
        h.state_mut().state.word_query = "読む".into();
        h.state_mut()
            .state
            .words
            .ticked
            .insert("old selection".into());
        h.state_mut().page.open.as_mut().unwrap().chapter = 1;
        h.run();
        assert!(!h.state().state.words_open);
        assert!(h.state().state.word_query.is_empty());
        assert!(h.state().state.words.ticked.is_empty());
    }

    // T-BOK-006: a click gives the paragraph and the byte of the character
    #[test]
    fn a_click_gives_its_character() {
        let mut h = harness(BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        });
        h.run();
        let rect = h.get_by_label("0: 読む本です").rect();
        let at = Pos2::new(rect.min.x + TEXT_SIZE * 2.5, rect.center().y);
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        let clicked = h.state().out.iter().find_map(|c| match c {
            Command::BookClicked {
                paragraph, offset, ..
            } => Some((*paragraph, *offset)),
            _ => None,
        });
        let (paragraph, offset) = clicked.expect("a click");
        assert_eq!(paragraph, 0);
        assert_eq!(offset % 3, 0, "on a character");
    }

    // T-RDR-004: pages group whole paragraphs; a tall one stands alone.
    #[test]
    fn paragraphs_group_into_pages() {
        assert_eq!(pages(&[10.0, 10.0, 10.0], 25.0, 2.0), vec![0..2, 2..3]);
        assert_eq!(
            pages(&[10.0, 50.0, 10.0], 25.0, 2.0),
            vec![0..1, 1..2, 2..3]
        );
        assert_eq!(pages(&[], 25.0, 2.0), vec![0..0]);
        assert_eq!(pages(&[5.0; 4], 100.0, 1.0), vec![0..4]);
    }

    fn long_chapter() -> OpenBook {
        let mut book = open();
        book.book.chapters[0].paragraphs = (0..40)
            .map(|i| Paragraph {
                text: format!("第{i}段落。二つ目の文。"),
                ..Paragraph::default()
            })
            .collect();
        book
    }

    // T-RDR-004: pages turn explicitly, by button and key, to whole paragraphs.
    #[test]
    fn pages_turn_explicitly() {
        let mut h = harness(BooksPage {
            open: Some(long_chapter()),
            ..BooksPage::default()
        });
        h.state_mut().mode = "pages";
        h.run();
        assert!(h.query_by_label("0: 第0段落。二つ目の文。").is_some());
        assert!(
            h.query_by_label("39: 第39段落。二つ目の文。").is_none(),
            "a page, not the chapter"
        );
        h.get_by_label("books-next-page").click();
        h.run();
        let next = h.state().out.iter().find_map(|c| match c {
            Command::BookTurn(p) => Some(*p),
            _ => None,
        });
        assert!(next.is_some_and(|p| p > 0), "{next:?}");
        h.state_mut().out.clear();
        h.state_mut().state.turn = Some(true);
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookTurn(_)))
        );
        assert_eq!(h.state().state.turn, None);
        assert!(h.query_by_label("books-page-of").is_some());
    }

    // T-PORT-004: a swipe turns pages left to right, and does nothing to a scroll.
    #[test]
    fn swipes_turn_pages() {
        let mut h = harness(BooksPage {
            open: Some(long_chapter()),
            ..BooksPage::default()
        });
        h.state_mut().mode = "pages";
        h.run();
        h.state_mut().state.swipe = Some(crate::gestures::Gesture::SwipeLeft);
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookTurn(p) if *p > 0))
        );
        assert!(h.state().state.swipe.is_none());
        h.state_mut().out.clear();
        h.state_mut().mode = "scroll";
        h.state_mut().state.swipe = Some(crate::gestures::Gesture::SwipeLeft);
        h.run();
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookTurn(_)))
        );
        assert!(
            h.state().state.swipe.is_none(),
            "dropped, not kept for later"
        );
    }

    // T-RDR-004: sentence focus shows one sentence; clicks keep original offsets.
    #[test]
    fn sentence_focus_keeps_original_offsets() {
        let mut book = long_chapter();
        book.paragraph = 2;
        book.sentence = 1;
        let mut h = harness(BooksPage {
            open: Some(book),
            ..BooksPage::default()
        });
        h.state_mut().mode = "sentence";
        h.run();
        let rect = h.get_by_label("2: 二つ目の文。").rect();
        let at = Pos2::new(rect.min.x + 4.0, rect.center().y);
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        let clicked = h.state().out.iter().find_map(|c| match c {
            Command::BookClicked {
                paragraph, offset, ..
            } => Some((*paragraph, *offset)),
            _ => None,
        });
        // "第2段落。" is 13 bytes: the second sentence starts there
        assert_eq!(clicked, Some((2, 13)));
        h.get_by_label("books-next-sentence").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookSentence(true)))
        );
        h.get_by_label("books-previous-sentence").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookSentence(false)))
        );
    }

    // T-RDR-001, T-RDR-004: the reader offers annotations and modes by name.
    #[test]
    fn the_reader_offers_annotations_and_modes() {
        let mut h = harness(BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("annotation-new-highlight").click();
        h.run();
        h.get_by_label("books-mode-pages").click();
        h.run();
        let out = &h.state().out;
        assert!(out.iter().any(|c| matches!(
            c,
            Command::AnnotationNew(nikomiru_library::annotations::Kind::Highlight)
        )));
        assert!(
            out.iter()
                .any(|c| matches!(c, Command::BookMode(m) if m == "pages"))
        );
    }

    // T-BOK-009: the first column is on the right, and a click finds its
    // character
    #[test]
    fn vertical_text_is_clicked_where_it_is() {
        let mut h = vertical_harness(
            BooksPage {
                open: Some(open()),
                ..BooksPage::default()
            },
            true,
        );
        h.run();
        let rect = h.get_by_label("読む本です").rect();
        // the second character of the rightmost column
        let width = rect.width();
        let columns = (width / (TEXT_SIZE * COLUMN_WIDTH)).floor();
        let right = rect.center().x + columns * TEXT_SIZE * COLUMN_WIDTH / 2.0;
        let at = Pos2::new(
            right - TEXT_SIZE * COLUMN_WIDTH / 2.0,
            rect.min.y + TEXT_SIZE * CELL * 1.5,
        );
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        let clicked = h.state().out.iter().find_map(|c| match c {
            Command::BookClicked {
                paragraph, offset, ..
            } => Some((*paragraph, *offset)),
            _ => None,
        });
        assert_eq!(clicked, Some((0, 3)), "む, the second character");
    }

    // T-BOK-009
    #[test]
    fn writing_is_a_choice() {
        let mut h = harness(BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("books-vertical").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookWriting(true)))
        );
    }

    // T-BOK-005
    #[test]
    fn chapters_go_one_by_one() {
        let mut h = harness(BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("books-next").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(1)))
        );
    }

    // T-ABK-005: Read along only with something playing, then Listen from
    // here; the paragraph heard marked
    #[test]
    fn read_along_is_asked_and_listens() {
        let mut h = harness(BooksPage {
            open: Some(open()),
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("books-along").click();
        h.run();
        assert!(h.state().out.is_empty(), "nothing plays");
        assert!(h.query_by_label("books-listen").is_none());
        h.state_mut().can_read_along = true;
        h.run();
        h.get_by_label("books-along").click();
        h.run();
        assert!(matches!(h.state().out[..], [Command::BookAlong(true)]));
        h.state_mut().out.clear();
        h.state_mut().page.along = Some(crate::books::ReadAlong {
            book: "uid".into(),
            places: vec![Some((0, 0))],
            heard: Some((0, 0)),
        });
        h.run();
        h.get_by_label("books-listen").click();
        h.run();
        h.get_by_label("books-along").click();
        h.run();
        assert!(matches!(
            h.state().out[..],
            [Command::BookListen, Command::BookAlong(false)]
        ));
    }

    fn manga() -> OpenBook {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(100, 200, image::Rgb([200, 200, 200]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let page = |key: &str| Chapter {
            title: None,
            paragraphs: vec![Paragraph {
                image: Some(key.into()),
                ..Paragraph::default()
            }],
        };
        let mut book = Book {
            title: "漫画".into(),
            manga: true,
            chapters: vec![page("p1.png"), page("p2.png")],
            ..Book::default()
        };
        book.images.insert("p1.png".into(), bytes.clone());
        book.images.insert("p2.png".into(), bytes);
        OpenBook {
            uid: "book/m".into(),
            book,
            chapter: 0,
            paragraph: 0,
            jump: false,
            started: (0, 0),
            sentence: 0,
        }
    }

    // T-MNG-003: a drag on the page gives the part of it, in shares
    #[test]
    fn a_selection_is_a_part_of_the_page() {
        let mut page = BooksPage {
            open: Some(manga()),
            ..BooksPage::default()
        };
        page.ocr.installed = true;
        let mut h = harness(page);
        h.run();
        let rect = h.get_by_label("manga-page").rect();
        let (top, height) = (rect.center().y - rect.height() / 2.0, rect.height());
        let width = height / 2.0;
        let left = rect.center().x - width / 2.0;
        let at = |x: f32, y: f32| Pos2::new(left + x * width, top + y * height);
        h.hover_at(at(0.25, 0.25));
        h.drag_at(at(0.25, 0.25));
        h.run();
        h.hover_at(at(0.5, 0.4));
        h.run();
        h.hover_at(at(0.75, 0.5));
        h.run();
        h.drop_at(at(0.75, 0.5));
        h.run();
        let area = h.state().out.iter().find_map(|c| match c {
            Command::MangaRead { page: 0, area, .. } => Some(*area),
            _ => None,
        });
        let area = area.expect("a selection");
        for (got, wanted) in area.iter().zip([0.25, 0.25, 0.5, 0.25]) {
            assert!((got - wanted).abs() < 0.02, "{area:?}");
        }
    }

    /// Three upright pages, or the last one wide.
    fn pairs(wide_last: bool) -> OpenBook {
        let mut open = manga();
        let mut bytes = Vec::new();
        let (w, h) = if wide_last { (200, 100) } else { (100, 200) };
        image::RgbImage::new(w, h)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        open.book.images.insert("p3.png".into(), bytes);
        open.book.chapters.push(Chapter {
            title: None,
            paragraphs: vec![Paragraph {
                image: Some("p3.png".into()),
                ..Paragraph::default()
            }],
        });
        open
    }

    // T-MNG-002: past the cover, the second page is on the left, a selection
    // reads the page it started on, and a turn goes two pages
    #[test]
    fn two_pages_read_from_the_right() {
        let mut open = pairs(false);
        open.chapter = 1;
        let mut h = harness(BooksPage {
            open: Some(open),
            ..BooksPage::default()
        });
        h.state_mut().double = true;
        h.state_mut().page.ocr.installed = true;
        h.run();
        assert_eq!(h.state().state.textures.len(), 2);
        let rect = h.get_by_label("manga-page").rect();
        let y = rect.center().y;
        let left = rect.center().x - 60.0;
        h.hover_at(Pos2::new(left, y));
        h.drag_at(Pos2::new(left, y));
        h.run();
        h.hover_at(Pos2::new(left + 30.0, y + 30.0));
        h.run();
        h.drop_at(Pos2::new(left + 30.0, y + 30.0));
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::MangaRead { page: 2, .. })),
            "{:?}",
            h.state().out
        );
        h.get_by_label("books-previous-page").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(0)))
        );
    }

    // T-MNG-002: the cover and a wide picture stay alone
    #[test]
    fn a_cover_and_a_spread_stay_alone() {
        let mut h = harness(BooksPage {
            open: Some(pairs(false)),
            ..BooksPage::default()
        });
        h.state_mut().double = true;
        h.run();
        assert_eq!(h.state().state.textures.len(), 1, "the cover");
        let mut open = pairs(true);
        open.chapter = 1;
        h.state_mut().page.open = Some(open);
        h.run();
        h.key_press(egui::Key::ArrowLeft);
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(2))),
            "one page: {:?}",
            h.state().out
        );
    }

    // T-MNG-005: the text read is clicked like a book's
    #[test]
    fn the_text_of_a_page_is_clicked() {
        let mut open = manga();
        open.book.chapters[0].paragraphs[0].text = "読む本です".into();
        let mut h = harness(BooksPage {
            open: Some(open),
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("読む本です").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookClicked { paragraph: 0, .. }))
        );
    }

    // T-MNG-004: the model comes only when asked for, and the page says so
    #[test]
    fn the_model_is_asked_for() {
        let mut h = harness(BooksPage {
            open: Some(manga()),
            ..BooksPage::default()
        });
        h.run();
        let _ = h.get_by_label("manga-credit");
        assert!(h.state().out.is_empty(), "{:?}", h.state().out);
        h.get_by_label("manga-fetch").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::OcrFetch)));
    }

    // T-MNG-002: the next page is the next one, and the writing is not a choice
    #[test]
    fn pages_turn() {
        let mut h = harness(BooksPage {
            open: Some(manga()),
            ..BooksPage::default()
        });
        h.run();
        assert!(h.query_by_label("books-vertical").is_none());
        h.get_by_label("books-next-page").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(1)))
        );
    }

    // T-BOK-010: a picture of the same name in another book is another picture
    #[test]
    fn pictures_are_of_their_book() {
        let mut other = manga();
        other.uid = "book/n".into();
        let mut h = harness(BooksPage {
            open: Some(manga()),
            ..BooksPage::default()
        });
        h.run();
        let first = h.state().state.textures["p1.png"].clone().unwrap().id();
        h.state_mut().page.open = Some(other);
        h.run();
        let second = h.state().state.textures["p1.png"].clone().unwrap().id();
        assert_ne!(first, second);
    }

    // T-BOK-004
    #[test]
    fn the_list_adds_and_opens() {
        let record = nikomiru_library::books::BookRecord {
            title: "本".into(),
            chars: 100,
            read_chars: 50,
            ..Default::default()
        };
        let mut h = harness(BooksPage {
            list: vec![("book/a".into(), record)],
            ..BooksPage::default()
        });
        h.run();
        h.get_by_label("books-add").click();
        h.run();
        assert!(h.state().add);
        h.get_by_label("本").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChoose(u) if u == "book/a"))
        );
        let _ = h.get_by_label("50 %");
    }
    // T-BOK-013: an explicit editor submission is required, empty input is disabled and the draft survives submission.
    #[test]
    fn pasted_text_editor_emits_an_explicit_bounded_document() {
        let mut h = harness(BooksPage::default());
        h.run();
        h.get_by_label("books-paste").click();
        h.run();
        h.get_by_label("books-paste-read").click();
        h.run();
        assert!(h.state().out.is_empty());
        {
            let state = &mut h.state_mut().state;
            state.paste_title = "読書".into();
            state.paste_text = "# 猫\n\n本文".into();
            state.paste_markdown = true;
        }
        h.run();
        h.get_by_label("books-paste-read").click();
        h.run();
        assert!(h.state().out.iter().any(|c|matches!(c,Command::BookPaste(p) if p.title=="読書"&&p.text=="# 猫\n\n本文"&&p.format==nikomiru_books::text::Format::Markdown)));
        assert!(!h.state().state.paste_open);
        assert_eq!(h.state().state.paste_text, "# 猫\n\n本文");
    }
    // T-MNG-008, T-MNG-010: imported text has an accessible target without
    // model installation or an OCR offer; a click looks the word under it up.
    #[test]
    fn imported_manga_region_looks_up_the_word_clicked() {
        let mut open = manga();
        open.book.manga_regions.insert(
            0,
            nikomiru_books::mokuro::Page {
                width: 100,
                height: 200,
                regions: vec![nikomiru_books::mokuro::Region {
                    bounds: [10, 10, 80, 100],
                    text: "猫です".into(),
                    vertical: true,
                    lines: vec![(0..9, [10, 10, 80, 100])],
                    covers: vec![[10, 10, 80, 100]],
                }],
            },
        );
        let mut h = harness(BooksPage {
            open: Some(open),
            ..Default::default()
        });
        h.run();
        assert!(h.query_by_label("manga-fetch").is_none());
        h.get_by_label("mokuro-region").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::MokuroLookup { page: 0, region: 0, offset, .. } if *offset < 9)),
            "{:?}",
            h.state().out
        );
    }

    // T-MNG-011: a page being read ahead says so; read, its blocks are
    // clicked like imported ones, and a box can still be dragged; without
    // the detector, it is offered alone
    #[test]
    fn a_page_read_ahead_is_clicked() {
        let mut page = BooksPage {
            open: Some(manga()),
            ..Default::default()
        };
        page.ocr.installed = true;
        page.ocr.pages = true;
        page.ocr.preparing.insert(0);
        let mut h = harness(page);
        h.run_steps(3);
        assert!(h.query_by_label("manga-preparing").is_some());
        assert!(h.query_by_label("mokuro-region").is_none());
        h.state_mut().page.ocr.preparing.clear();
        h.state_mut().page.ocr.found.insert(
            0,
            nikomiru_books::mokuro::Page {
                width: 100,
                height: 200,
                regions: vec![nikomiru_books::mokuro::Region::guessed(
                    [10, 10, 80, 100],
                    "猫です".into(),
                    true,
                )],
            },
        );
        h.run();
        assert!(h.query_by_label("manga-ready").is_some());
        assert!(h.query_by_label("manga-pages-fetch").is_none());
        h.get_by_label("mokuro-region").click();
        h.run();
        assert!(
            h.state().out.iter().any(|c| matches!(
                c,
                Command::MokuroLookup {
                    page: 0,
                    region: 0,
                    ..
                }
            )),
            "{:?}",
            h.state().out
        );
        h.state_mut().out.clear();
        h.state_mut().page.ocr.found.clear();
        h.state_mut().page.ocr.pages = false;
        h.run();
        h.get_by_label("manga-pages-fetch").click();
        h.run();
        assert!(matches!(h.state().out[..], [Command::OcrFetch]));
    }

    /// The wheel turned one line up (`up`) or down over `at`, with Ctrl or not.
    fn wheel(h: &Harness<'_, Test>, at: Pos2, up: bool, ctrl: bool) {
        h.hover_at(at);
        let modifiers = if ctrl {
            egui::Modifiers::COMMAND
        } else {
            egui::Modifiers::NONE
        };
        let event = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, if up { 1.0 } else { -1.0 }),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers,
        };
        h.event_modifiers(event, modifiers);
    }

    // T-RDR-005: Ctrl and the wheel make a book's text larger or smaller,
    // at once, and never turn its page
    #[test]
    fn ctrl_and_the_wheel_size_the_text() {
        let mut h = vertical_harness(
            BooksPage {
                open: Some(open()),
                ..BooksPage::default()
            },
            true,
        );
        h.run();
        let middle = Pos2::new(500.0, 400.0);
        wheel(&h, middle, true, true);
        h.run();
        let larger = h.state().out.iter().find_map(|c| match c {
            Command::BookZoom(z) => Some(*z),
            _ => None,
        });
        assert!(larger.is_some_and(|z| z > 1.0), "{:?}", h.state().out);
        // laid out again at the new size, at the same place
        assert!(
            h.state()
                .out
                .iter()
                .all(|c| matches!(c, Command::BookZoom(_) | Command::BookAt(0))),
            "{:?}",
            h.state().out
        );
        assert!(text_size() > TEXT_SIZE, "drawn larger at once");
        // the setting caught up, then smaller
        h.state_mut().zoom = crate::books::text_zoom(larger);
        h.state_mut().out.clear();
        for _ in 0..30 {
            wheel(&h, middle, false, true);
            h.run();
        }
        let last = h.state().out.iter().rev().find_map(|c| match c {
            Command::BookZoom(z) => Some(*z),
            _ => None,
        });
        assert_eq!(last.map(|z| crate::books::text_zoom(Some(z))), Some(0.6));
        // the wheel alone still turns
        h.state_mut().out.clear();
        wheel(&h, middle, false, false);
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(1))),
            "{:?}",
            h.state().out
        );
    }

    // T-RDR-005: Ctrl and the wheel enlarge a manga's page about the pointer;
    // enlarged, the wheel moves it instead of turning, and its balloons are
    // still clicked; back to whole, the wheel turns again
    #[test]
    fn ctrl_and_the_wheel_enlarge_a_manga_page() {
        let mut page = BooksPage {
            open: Some(manga()),
            ..Default::default()
        };
        page.ocr.installed = true;
        page.ocr.pages = true;
        page.ocr.found.insert(
            0,
            nikomiru_books::mokuro::Page {
                width: 100,
                height: 200,
                regions: vec![nikomiru_books::mokuro::Region::guessed(
                    [10, 10, 80, 100],
                    "猫です".into(),
                    true,
                )],
            },
        );
        let mut h = harness(page);
        h.run_steps(3);
        let before = h.get_by_label("mokuro-region").rect();
        let at = before.center();
        wheel(&h, at, true, true);
        h.run();
        wheel(&h, at, true, true);
        h.run();
        assert!(h.state().state.manga_zoom > 1.0);
        let after = h.get_by_label("mokuro-region").rect();
        assert!(
            after.width() > before.width() * 1.05,
            "{before:?} {after:?}"
        );
        assert!(after.contains(at), "about the pointer: {after:?} {at:?}");
        assert!(text_size() == TEXT_SIZE, "a manga's text keeps its size");
        wheel(&h, at, false, false);
        h.run();
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(_))),
            "{:?}",
            h.state().out
        );
        h.get_by_label("mokuro-region").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::MokuroLookup { page: 0, .. })),
            "{:?}",
            h.state().out
        );
        h.state_mut().out.clear();
        for _ in 0..30 {
            wheel(&h, at, false, true);
            h.run();
        }
        assert_eq!(h.state().state.manga_zoom, 1.0);
        assert_eq!(h.state().state.pan, egui::Vec2::ZERO);
        wheel(&h, at, false, false);
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BookChapter(1))),
            "{:?}",
            h.state().out
        );
    }

    // T-MNG-012: a balloon's text written in its lines, columns from the
    // right, at one size each line holds, about the picture's
    #[test]
    fn a_balloon_is_written_where_it_is() {
        // two columns of 4 in a block 100×200, the page drawn at half size
        let region = nikomiru_books::mokuro::Region::guessed(
            [0, 0, 100, 200],
            "今日は晴れです！".into(),
            true,
        );
        let drawn = egui::Rect::from_min_size(Pos2::new(10.0, 20.0), vec2(200.0, 400.0));
        let (size, glyphs) = region_glyphs(&region, (400, 800), drawn);
        // each column 25 wide and 100 tall for 4 (25 each)
        assert_eq!(size, 25.0);
        assert_eq!(glyphs.len(), 8);
        assert_eq!(glyphs[0].ch, '今');
        assert_eq!(glyphs[0].centre, Pos2::new(10.0 + 37.5, 20.0 + 12.5));
        assert_eq!(glyphs[4].ch, 'れ');
        assert_eq!(glyphs[4].offset, 12);
        assert_eq!(glyphs[4].centre, Pos2::new(10.0 + 12.5, 20.0 + 12.5));
        // a row, left to right, as tall as it, a fifth apart at most
        let wide = nikomiru_books::mokuro::Region::guessed([0, 0, 400, 40], "ここ".into(), false);
        let (size, glyphs) = region_glyphs(&wide, (400, 800), drawn);
        assert_eq!(size, 20.0);
        assert_eq!(glyphs[1].centre, Pos2::new(10.0 + 36.0, 20.0 + 10.0));
        let empty = nikomiru_books::mokuro::Region::guessed([0, 0, 10, 10], String::new(), true);
        assert!(region_glyphs(&empty, (400, 800), drawn).1.is_empty());
    }

    // T-MNG-012: a click looks up the character written under it; the text
    // over the balloons is switched off and on from the reader's line
    #[test]
    fn a_written_character_is_clicked() {
        let mut open = manga();
        open.book.manga_regions.insert(
            0,
            nikomiru_books::mokuro::Page {
                width: 100,
                height: 200,
                regions: vec![nikomiru_books::mokuro::Region::guessed(
                    [10, 10, 90, 190],
                    "猫です".into(),
                    true,
                )],
            },
        );
        let mut h = harness(BooksPage {
            open: Some(open),
            ..Default::default()
        });
        h.run();
        let region = h.get_by_label("mokuro-region").rect();
        // the region holds one column of 3: its last character near the bottom
        let (_, glyphs) = region_glyphs(
            &nikomiru_books::mokuro::Region::guessed([10, 10, 90, 190], "猫です".into(), true),
            (100, 200),
            // the page as drawn: the region's rect scaled back out
            {
                let sx = region.width() / 80.0;
                let sy = region.height() / 180.0;
                egui::Rect::from_min_size(
                    region.min - vec2(10.0 * sx, 10.0 * sy),
                    vec2(100.0 * sx, 200.0 * sy),
                )
            },
        );
        let at = glyphs[1].centre;
        h.hover_at(at);
        h.run();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        }
        assert!(
            h.state().out.iter().any(|c| matches!(
                c,
                Command::MokuroLookup {
                    page: 0,
                    region: 0,
                    offset: 3,
                    ..
                }
            )),
            "{:?}",
            h.state().out
        );
        h.state_mut().out.clear();
        h.get_by_label("manga-text").click();
        h.run();
        assert!(matches!(h.state().out[..], [Command::MangaText(false)]));
    }
}
