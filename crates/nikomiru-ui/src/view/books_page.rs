// SPDX-License-Identifier: GPL-3.0-only

//! The Books page (BOK-004…BOK-006): the books added, and the reading of one,
//! paragraph by paragraph, each character a target, with its furigana above.
//! It sits under the popups, which take the same lookups.

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

/// The size of the text, in points.
const TEXT_SIZE: f32 = 22.0;
/// The widest column of text, in points.
const COLUMN: f32 = 860.0;
/// Behind the word looked up.
pub(super) const MARK: Color32 = Color32::from_rgba_premultiplied(40, 70, 120, 150);

/// A character's cell in vertical writing, as a share of the text's size.
const CELL: f32 = 1.12;
/// A column's width, furigana included, as a share of the text's size.
const COLUMN_WIDTH: f32 = 1.9;

/// What the page keeps between frames.
#[derive(Default)]
pub(crate) struct State {
    /// The chapter laid out: its book, its number, the rows and the columns
    /// of a page, and its cells.
    columns: Option<((String, usize, usize, usize), Columns)>,
    /// The first column of the page shown.
    column: usize,
    /// The pictures drawn so far, by their key (BOK-010).
    textures: HashMap<String, Option<egui::TextureHandle>>,
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
    /// The books and the one open.
    pub page: &'a BooksPage,
    /// The book is drawn vertically (BOK-009).
    pub vertical: bool,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.books_page,
        vertical: s.book_vertical(),
        tr: &tr,
    };
    let (close, add) = draw(ctx, &scene, &mut view.books, out);
    if add {
        view.dialog = Some(Dialog::Book);
    }
    if close {
        view.close_books_page(out);
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
    let mut rows: Vec<egui::Rect> = Vec::new();
    for at in start..start + length {
        let left = galley.pos_from_cursor(egui::text::CCursor::new(at));
        let right = galley.pos_from_cursor(egui::text::CCursor::new(at + 1));
        let width = if (left.top() - right.top()).abs() < 1.0 {
            right.left() - left.left()
        } else {
            left.height()
        };
        let cell = egui::Rect::from_min_size(left.left_top(), vec2(width, left.height()));
        match rows.last_mut() {
            Some(row) if (row.top() - cell.top()).abs() < 1.0 => *row = row.union(cell),
            _ => rows.push(cell),
        }
    }
    rows
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
            let cover = crate::catalog::Cover::decode(bytes).ok()?;
            let image = egui::ColorImage::from_rgba_unmultiplied(cover.size, &cover.rgba);
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
fn paragraph(
    ui: &mut Ui,
    item: &Paragraph,
    name: &str,
    marked: Option<std::ops::Range<usize>>,
) -> (Option<(usize, egui::Rect)>, egui::Rect) {
    let size = if item.heading {
        TEXT_SIZE * 1.3
    } else {
        TEXT_SIZE
    };
    let color = palette::current().text;
    let family = if item.heading {
        theme::bold()
    } else {
        egui::FontFamily::Proportional
    };
    let format = TextFormat {
        font_id: FontId::new(size, family),
        color,
        // room for the furigana between the rows
        line_height: Some(size * 2.0),
        valign: egui::Align::BOTTOM,
        ..TextFormat::default()
    };
    let mut job = LayoutJob::single_section(item.text.clone(), format);
    job.wrap = TextWrapping {
        max_width: ui.available_width(),
        ..TextWrapping::default()
    };
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    if let Some(range) = marked {
        for row in range_rects(&galley, &item.text, range) {
            ui.painter()
                .rect_filled(row.translate(rect.min.to_vec2()), 3.0, MARK);
        }
    }
    ui.painter().galley(rect.min, galley.clone(), color);
    for ruby in &item.ruby {
        let chars = item.text[..ruby.range.start].chars().count();
        let length = item.text[ruby.range.clone()].chars().count();
        let first = galley.pos_from_cursor(egui::text::CCursor::new(chars));
        let last = galley.pos_from_cursor(egui::text::CCursor::new(chars + length));
        if (first.top() - last.top()).abs() > 1.0 {
            // it breaks across rows: over its start
            ui.painter().text(
                rect.min + first.left_top().to_vec2(),
                Align2::LEFT_BOTTOM,
                &ruby.text,
                FontId::proportional(size * 0.5),
                palette::current().details,
            );
            continue;
        }
        let x = (first.left() + last.left()) / 2.0;
        let y = first.bottom() - size * 1.05;
        ui.painter().text(
            rect.min + vec2(x, y),
            Align2::CENTER_BOTTOM,
            &ruby.text,
            FontId::proportional(size * 0.5),
            palette::current().details,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let clicked = response.clicked().then(|| {
        let at = response.interact_pointer_pos().unwrap_or(rect.min) - rect.min;
        let cursor = galley.cursor_from_pos(at);
        let offset = crate::subtitles::char_to_byte(&item.text, cursor.index.0);
        (offset, rect)
    });
    (clicked, rect)
}

/// Where a cell goes on the page: its centre.
fn cell_centre(area: egui::Rect, first: usize, cell: &Cell) -> egui::Pos2 {
    let x = area.right() - ((cell.column - first) as f32 + 0.5) * TEXT_SIZE * COLUMN_WIDTH;
    let y = area.top() + (cell.row as f32 + 0.5) * TEXT_SIZE * CELL;
    egui::pos2(x, y)
}

fn draw_cell(ui: &Ui, area: egui::Rect, first: usize, cell: &Cell) {
    let color = palette::current().text;
    let font = FontId::proportional(TEXT_SIZE);
    let centre = cell_centre(area, first, cell);
    let text = cell.ch.to_string();
    match cell.stance {
        Stance::Upright => {
            ui.painter()
                .text(centre, Align2::CENTER_CENTER, text, font, color);
        }
        Stance::Corner => {
            let at = centre + vec2(TEXT_SIZE * 0.3, -TEXT_SIZE * 0.3);
            ui.painter()
                .text(at, Align2::CENTER_CENTER, text, font, color);
        }
        Stance::Turned => {
            let galley = ui.fonts_mut(|f| f.layout_no_wrap(text, font, color));
            let size = galley.size();
            // a quarter turn around the cell's centre
            let at = centre + vec2(size.y / 2.0, -size.x / 2.0);
            ui.painter().add(
                egui::epaint::TextShape::new(at, galley, color)
                    .with_angle(std::f32::consts::FRAC_PI_2),
            );
        }
    }
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
                bottom.y - top.y + TEXT_SIZE
            } else {
                TEXT_SIZE
            };
            let count = ruby.text.chars().count().max(1) as f32;
            let step = (height / count).min(TEXT_SIZE * 0.6);
            let start = top.y - TEXT_SIZE / 2.0 + (height - step * count) / 2.0;
            for (i, c) in ruby.text.chars().enumerate() {
                let at = egui::pos2(top.x + TEXT_SIZE * 0.72, start + (i as f32 + 0.5) * step);
                ui.painter().text(
                    at,
                    Align2::CENTER_CENTER,
                    c,
                    FontId::proportional(TEXT_SIZE * 0.5),
                    palette::current().details,
                );
            }
        }
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
    let rows = ((room.height() / (TEXT_SIZE * CELL)) as usize).max(2);
    let per_page = ((room.width() / (TEXT_SIZE * COLUMN_WIDTH)) as usize).max(1);
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
    let width = per_page as f32 * TEXT_SIZE * COLUMN_WIDTH;
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
    if let Some((paragraph, range)) = &s.page.highlight {
        for cell in cells
            .iter()
            .filter(|c| c.paragraph == *paragraph && range.contains(&c.offset))
        {
            let box_size = vec2(TEXT_SIZE * 1.1, TEXT_SIZE * CELL);
            let rect =
                egui::Rect::from_center_size(cell_centre(area, state.column, cell), box_size);
            ui.painter().rect_filled(rect, 2.0, MARK);
        }
    }
    for cell in &cells {
        draw_cell(ui, area, state.column, cell);
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
                    x: f64::from(centre.x - TEXT_SIZE / 2.0),
                    y: f64::from(centre.y - TEXT_SIZE / 2.0),
                    w: f64::from(TEXT_SIZE),
                    h: f64::from(TEXT_SIZE),
                },
            });
        }
    }
    // the page turns to the left, as the book does
    let (forward, back) = ui.input(|i| {
        let wheel = i.events.iter().find_map(|e| match e {
            egui::Event::MouseWheel { delta, .. } => Some(delta.y),
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
    });
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
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button(tr(s, "books-back")).clicked() {
            close = true;
        }
        ui.add(egui::Label::new(RichText::new(&open.book.title).family(theme::bold())).truncate());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let last = open.book.chapters.len().saturating_sub(1);
            if ui
                .add_enabled(open.chapter < last, Button::new(tr(s, "books-next")))
                .clicked()
            {
                out.push(Command::BookChapter(open.chapter + 1));
            }
            let number = format!("{} / {}", open.chapter + 1, open.book.chapters.len());
            ui.label(RichText::new(number).color(palette::current().details));
            if ui
                .add_enabled(open.chapter > 0, Button::new(tr(s, "books-previous")))
                .clicked()
            {
                out.push(Command::BookChapter(open.chapter - 1));
            }
            if ui
                .selectable_label(s.vertical, tr(s, "books-vertical"))
                .clicked()
            {
                out.push(Command::BookWriting(!s.vertical));
            }
        });
    });
    if s.vertical {
        vertical_reader(ui, s, open, state, out);
        return close;
    }
    let Some(chapter) = open.book.chapters.get(open.chapter) else {
        return close;
    };
    let mut area = ScrollArea::vertical()
        .id_salt(("book", open.chapter))
        .auto_shrink([false, false]);
    if open.jump && open.paragraph == 0 {
        area = area.vertical_scroll_offset(0.0);
    }
    area.show(ui, |ui| {
        let width = ui.available_width().min(COLUMN);
        ui.vertical_centered(|ui| {
            ui.set_max_width(width);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = TEXT_SIZE * 0.6;
                let top = ui.clip_rect().top();
                let mut reached = None;
                for (index, item) in chapter.paragraphs.iter().enumerate() {
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
                    let marked = s
                        .page
                        .highlight
                        .as_ref()
                        .filter(|(at, _)| *at == index)
                        .map(|(_, range)| range.clone());
                    let (clicked, rect) = paragraph(ui, item, &name, marked);
                    if open.jump && index == open.paragraph {
                        ui.scroll_to_rect(rect, Some(Align::Min));
                    }
                    if reached.is_none() && rect.bottom() > top {
                        reached = Some(index);
                    }
                    if let Some((offset, rect)) = clicked {
                        out.push(Command::BookClicked {
                            paragraph: index,
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
                if let Some(index) = reached
                    && (open.jump || index != open.paragraph)
                {
                    out.push(Command::BookAt(if open.jump {
                        open.paragraph
                    } else {
                        index
                    }));
                }
            });
        });
    });
    close
}

fn list(ui: &mut Ui, s: &Scene<'_>, add: &mut bool, close: &mut bool, out: &mut Vec<Command>) {
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
            if ui
                .add_enabled(!s.page.busy, Button::new(tr(s, "books-add")))
                .clicked()
            {
                *add = true;
            }
        });
    });
    if let Some((text, ok)) = &s.page.status {
        let shade: Color32 = if *ok {
            palette::current().good
        } else {
            palette::current().bad
        };
        ui.add(egui::Label::new(RichText::new(text).color(shade)).wrap());
    }
    if s.page.list.is_empty() {
        ui.label(RichText::new(tr(s, "books-none")).color(palette::current().details));
    }
    ScrollArea::vertical()
        .id_salt("books")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (uid, record) in &s.page.list {
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

/// Draws the page; whether it closes, and whether a file is to be chosen.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> (bool, bool) {
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
                            if reader(ui, s, open, state, out) {
                                out.push(Command::BookClose);
                            }
                        }
                        None => list(ui, s, &mut add, &mut close, out),
                    }
                });
        });
    (close, add)
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_books::{Book, Chapter, Ruby};

    use super::*;

    struct Test {
        page: BooksPage,
        vertical: bool,
        state: State,
        out: Vec<Command>,
        add: bool,
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
                        page: &t.page,
                        vertical: t.vertical,
                        tr: &tr,
                    };
                    t.add |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out).1;
                },
                Test {
                    page,
                    vertical,
                    state: State::default(),
                    out: Vec::new(),
                    add: false,
                },
            )
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
                title: "本".into(),
                author: String::new(),
                right_to_left: false,
                chapters: vec![chapter("読む本です"), chapter("次")],
                images: Default::default(),
            },
            chapter: 0,
            paragraph: 0,
            jump: false,
            started: (0, 0),
        }
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
}
