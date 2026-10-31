// SPDX-License-Identifier: GPL-3.0-only

//! The Books page (BOK-004…BOK-006): the books added, and the reading of one,
//! paragraph by paragraph, each character a target, with its furigana above.
//! It sits under the popups, which take the same lookups.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Align2, Area, Button, Color32, FontId, Frame, Id, Layout, Margin, Order, RichText,
    ScrollArea, Sense, Ui, vec2,
};
use nikomiru_books::Paragraph;

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

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The books and the one open.
    pub page: &'a BooksPage,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.books_page,
        tr: &tr,
    };
    let (close, add) = draw(ctx, &scene, out);
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

/// A paragraph: its offset in bytes, if clicked, and the rect it took.
fn paragraph(
    ui: &mut Ui,
    item: &Paragraph,
    name: &str,
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

fn reader(ui: &mut Ui, s: &Scene<'_>, open: &OpenBook, out: &mut Vec<Command>) -> bool {
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
        });
    });
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
                    let name = format!("{index}: {}", item.text);
                    let (clicked, rect) = paragraph(ui, item, &name);
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
pub(crate) fn draw(ctx: &egui::Context, s: &Scene<'_>, out: &mut Vec<Command>) -> (bool, bool) {
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
                            if reader(ui, s, open, out) {
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
        out: Vec<Command>,
        add: bool,
    }

    fn harness<'a>(page: BooksPage) -> Harness<'a, Test> {
        Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        tr: &tr,
                    };
                    t.add |= draw(ui.ctx(), &scene, &mut t.out).1;
                },
                Test {
                    page,
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
            }],
        };
        OpenBook {
            uid: "book/a".into(),
            book: Book {
                title: "本".into(),
                author: String::new(),
                chapters: vec![chapter("読む本です"), chapter("次")],
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
