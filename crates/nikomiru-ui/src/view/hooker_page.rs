// SPDX-License-Identifier: GPL-3.0-only

//! The Texthooker page (HOK-001…HOK-004): the lines of a game, clickable as
//! subtitles. It sits under the popups, which take the same lookups.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align, Area, Button, Frame, Id, Layout, Margin, Order, RichText, ScrollArea, Sense, TextEdit,
    Ui, vec2,
};
use nikomiru_core::config::HookerConfig;

use super::popups::Translate;
use super::{View, palette, theme};
use crate::hooker::HookerPage;
use crate::popup;
use crate::session::Session;
use crate::ui::Command;

/// The size of the lines, in points.
const LINE_SIZE: f32 = 26.0;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<HookerConfig>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The settings as saved.
    pub saved: &'a HookerConfig,
    /// The lines and the state of the connection.
    pub page: &'a HookerPage,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        saved: s.hooker_config(),
        page: &s.hooker_page,
        tr: &tr,
    };
    if draw(ctx, &scene, &mut view.hooker, out) {
        view.close_hooker_page(out);
    }
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

/// One line, each character a target; the byte offset of the click.
fn line(ui: &mut Ui, text: &str, name: &str) -> Option<(usize, egui::Rect)> {
    let format = TextFormat {
        font_id: egui::FontId::proportional(LINE_SIZE),
        color: palette::current().text,
        ..TextFormat::default()
    };
    let mut job = LayoutJob::single_section(text.to_string(), format);
    job.wrap = TextWrapping {
        max_width: ui.available_width(),
        ..TextWrapping::default()
    };
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    ui.painter()
        .galley(rect.min, galley.clone(), palette::current().text);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if !response.clicked() {
        return None;
    }
    let at = response.interact_pointer_pos()? - rect.min;
    let cursor = galley.cursor_from_pos(at);
    let offset = crate::subtitles::char_to_byte(text, cursor.index.0);
    Some((offset, rect))
}

fn header(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr(s, "hooker-title"))
                .size(24.0)
                .family(theme::bold()),
        );
        let (text, good) = if s.page.connected {
            (tr(s, "hooker-connected"), true)
        } else {
            (tr(s, "hooker-waiting"), false)
        };
        let shade = if good {
            palette::current().good
        } else {
            palette::current().details
        };
        ui.label(RichText::new(text).color(shade));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr(s, "library-close")).clicked() {
                close = true;
            }
            if ui.button(tr(s, "hooker-clear")).clicked() {
                out.push(Command::HookerClear);
            }
        });
    });
    let Some(draft) = state.draft.as_mut() else {
        return close;
    };
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "servers-address"));
        ui.add(TextEdit::singleline(&mut draft.url).desired_width(240.0))
            .labelled_by(label.id);
        ui.checkbox(&mut draft.clipboard, tr(s, "hooker-clipboard"));
        let changed = draft != s.saved;
        if ui
            .add_enabled(changed, Button::new(tr(s, "online-save")))
            .clicked()
        {
            out.push(Command::HookerSave(Box::new(draft.clone())));
        }
    });
    ui.add(
        egui::Label::new(RichText::new(tr(s, "hooker-hint")).color(palette::current().details))
            .wrap(),
    );
    if let Some((text, ok)) = &s.page.status {
        let shade = if *ok {
            palette::current().good
        } else {
            palette::current().bad
        };
        ui.add(egui::Label::new(RichText::new(text).color(shade)).wrap());
    }
    close
}

/// Draws the page; returns whether it closes.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> bool {
    if state.draft.is_none() {
        state.draft = Some(s.saved.clone());
    }
    // Ctrl+V adds what is in the clipboard (HOK-003)
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Paste(text) = event {
                out.push(Command::HookerPaste(text.clone()));
            }
        }
    });
    let screen = ctx.content_rect();
    let mut close = false;
    // under the popups, which are in the foreground (HOK-004)
    Area::new(Id::unique("hooker-page"))
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
                    close = header(ui, s, state, out);
                    ScrollArea::vertical()
                        .id_salt("hooker-lines")
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for (index, text) in s.page.lines.iter().enumerate() {
                                let name = format!("{index}: {text}");
                                if let Some((offset, rect)) = line(ui, text, &name) {
                                    out.push(Command::HookerClicked {
                                        line: index,
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
                        });
                });
        });
    close
}

#[cfg(test)]
mod tests {
    use egui::{Pos2, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    struct Test {
        page: HookerPage,
        saved: HookerConfig,
        state: State,
        out: Vec<Command>,
        closed: bool,
    }

    fn harness<'a>(lines: &[&str]) -> Harness<'a, Test> {
        let page = HookerPage {
            lines: lines.iter().map(ToString::to_string).collect(),
            ..HookerPage::default()
        };
        Harness::builder()
            .with_size(vec2(1000.0, 700.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        saved: &t.saved,
                        page: &t.page,
                        tr: &tr,
                    };
                    t.closed |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    page,
                    saved: HookerConfig::default(),
                    state: State::default(),
                    out: Vec::new(),
                    closed: false,
                },
            )
    }

    // T-HOK-004: a click gives the line and the byte of the character
    #[test]
    fn a_click_on_a_line_gives_its_character() {
        let mut h = harness(&["今日は", "ゲームです"]);
        h.run();
        let rect = h.get_by_label("1: ゲームです").rect();
        let at = Pos2::new(rect.min.x + LINE_SIZE * 1.5, rect.center().y);
        h.hover_at(at);
        h.drag_at(at);
        h.drop_at(at);
        h.run();
        let clicked = h.state().out.iter().find_map(|c| match c {
            Command::HookerClicked { line, offset, .. } => Some((*line, *offset)),
            _ => None,
        });
        let (line, offset) = clicked.expect("a click");
        assert_eq!(line, 1);
        assert_eq!(offset % 3, 0, "on a character");
    }

    // T-HOK-001
    #[test]
    fn the_settings_are_saved_when_changed() {
        let mut h = harness(&[]);
        h.run();
        h.get_by_label("hooker-clipboard").click();
        h.run();
        h.get_by_label("online-save").click();
        h.run();
        assert!(matches!(
            h.state().out.as_slice(),
            [Command::HookerSave(c)] if c.clipboard
        ));
    }

    #[test]
    fn close_closes() {
        let mut h = harness(&[]);
        h.run();
        h.get_by_label("library-close").click();
        h.run();
        assert!(h.state().closed);
    }
}
