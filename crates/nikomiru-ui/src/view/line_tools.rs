// SPDX-License-Identifier: GPL-3.0-only

//! The line tools' panel over the player (M121…M125), and the choice of a
//! part of the picture to read (OCR-001).

use egui::{Align2, Color32, Id, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, Ui, vec2};

use super::palette;
use crate::line_tools::{Answer, LineTools, Reading, Shadow};
use crate::session::Session;
use crate::ui::Command;

/// The interface's strings, with their arguments.
type Tr<'a> = &'a dyn Fn(&str, &[(&str, String)]) -> String;

/// What the panel shows.
pub(crate) struct Scene<'a> {
    pub tools: &'a LineTools,
    pub tr: Tr<'a>,
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

/// The panel, when it is open.
pub(crate) fn show(ctx: &egui::Context, s: &Session, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        tools: &s.line_tools,
        tr: &tr,
    };
    let mut open = true;
    egui::Window::new(tr("line-tools", &[]))
        .id(Id::unique("line-tools"))
        .open(&mut open)
        .anchor(Align2::RIGHT_TOP, vec2(-16.0, 16.0))
        .default_width(400.0)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| draw(ui, &scene, out));
    if !open {
        out.push(Command::LineTools(false));
    }
}

fn answer(ui: &mut Ui, s: &Scene<'_>, title: &str, answer: &Answer, id: &str) {
    match answer {
        Answer::None => {}
        Answer::Busy => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(s, "line-busy"));
            });
        }
        Answer::Done(text) => {
            ui.label(RichText::new(title).strong());
            ScrollArea::vertical()
                .id_salt(id)
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(text.as_str()).selectable(true));
                });
        }
        Answer::Failed(error) => {
            ui.colored_label(palette::current().bad, error);
        }
    }
}

/// The panel's contents.
pub(crate) fn draw(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let t = s.tools;
    if t.text.trim().is_empty() {
        ui.label(tr(s, "line-none"));
    } else if let Some((offset, rect)) = super::hooker_page::line(ui, &t.text, &t.text, None) {
        out.push(Command::LineClicked(
            offset,
            crate::popup::Rect {
                x: f64::from(rect.min.x),
                y: f64::from(rect.min.y),
                w: f64::from(rect.width()),
                h: f64::from(rect.height()),
            },
        ));
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let has_text = !t.text.trim().is_empty();
        if ui
            .add_enabled(
                has_text && t.translation != Answer::Busy,
                egui::Button::new(format!("{}  (Shift+T)", tr(s, "line-translate"))),
            )
            .clicked()
        {
            out.push(Command::LineTranslate);
        }
        if ui
            .add_enabled(
                has_text && t.explanation != Answer::Busy,
                egui::Button::new(format!("{}  (G)", tr(s, "line-explain"))),
            )
            .clicked()
        {
            out.push(Command::LineExplain);
        }
        let recording = t.shadow == Shadow::Recording;
        let shadow = if recording {
            tr(s, "line-shadow-stop")
        } else {
            format!("{}  (Y)", tr(s, "line-shadow"))
        };
        if ui
            .add_enabled(has_text || recording, egui::Button::new(shadow))
            .clicked()
        {
            out.push(Command::LineShadow);
        }
        if ui
            .add_enabled(
                t.reading == Reading::Idle,
                egui::Button::new(format!("{}  (O)", tr(s, "line-read"))),
            )
            .clicked()
        {
            out.push(Command::LineRead);
        }
    });
    if t.reading == Reading::Choosing {
        ui.horizontal(|ui| {
            ui.label(tr(s, "line-read-hint"));
            if ui.button(tr(s, "line-read-cancel")).clicked() {
                out.push(Command::LineReadCancel);
            }
        });
    }
    if t.reading == Reading::Busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr(s, "line-read-busy"));
        });
    }
    ui.separator();
    answer(
        ui,
        s,
        &tr(s, "line-translation"),
        &t.translation,
        "line-translation",
    );
    answer(
        ui,
        s,
        &tr(s, "line-explanation"),
        &t.explanation,
        "line-explanation",
    );
    match &t.shadow {
        Shadow::Idle => {}
        Shadow::Recording => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(s, "line-shadow-recording"));
            });
        }
        Shadow::Done(done) => {
            ui.label(RichText::new(tr(s, "line-shadow-result")).strong());
            match (&done.said, done.score) {
                (Some(said), Some(score)) => {
                    ui.label((s.tr)("line-shadow-score", &[("score", score.to_string())]));
                    ui.add(
                        egui::Label::new((s.tr)("line-shadow-said", &[("said", said.clone())]))
                            .selectable(true),
                    );
                }
                _ => {
                    ui.label(tr(s, "line-shadow-unscored"));
                }
            }
            if ui.button(tr(s, "line-shadow-play")).clicked() {
                out.push(Command::LinePlayRecording);
            }
        }
        Shadow::Failed(error) => {
            ui.colored_label(palette::current().bad, error);
        }
    }
}

/// Dragging over the picture chooses the part to read; Back (Escape) gives up.
pub(crate) fn choose(
    ctx: &egui::Context,
    s: &Session,
    from: &mut Option<Pos2>,
    out: &mut Vec<Command>,
) {
    let screen = ctx.content_rect();
    let fit = super::overlay::Fit::of(&s.config().player.fit);
    let video = super::overlay::video_rect(screen, s.view.video_size, fit).intersect(screen);
    egui::Area::new(Id::unique("line-read-choose"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let response = ui.interact(screen, Id::unique("line-read-area"), Sense::drag());
            let name = s.translator().tr("line-read-hint", &[]);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, &name));
            let painter = ui.painter_at(screen);
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(90));
            painter.text(
                screen.center_top() + vec2(0.0, 24.0),
                Align2::CENTER_TOP,
                &name,
                egui::FontId::proportional(18.0),
                Color32::WHITE,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            if response.drag_started() {
                *from = response.interact_pointer_pos();
            }
            let now = response
                .interact_pointer_pos()
                .or(ui.ctx().pointer_latest_pos());
            if let (Some(a), Some(b)) = (*from, now) {
                let rect = Rect::from_two_pos(a, b);
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(2.0, palette::current().accent),
                    egui::StrokeKind::Outside,
                );
            }
            if response.drag_stopped()
                && let (Some(a), Some(b)) = (from.take(), now)
            {
                let video = [video.min.x, video.min.y, video.width(), video.height()];
                if let Some(area) = crate::line_tools::area(video, (a.x, a.y), (b.x, b.y)) {
                    out.push(Command::LineReadArea(area));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_tools::Shadowed;
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(tools: LineTools) -> Harness<'static, Vec<Command>> {
        let tr = |id: &str, args: &[(&str, String)]| {
            let mut text = id.to_string();
            for (k, v) in args {
                text.push_str(&format!(" {k}={v}"));
            }
            text
        };
        Harness::builder().allow_missing_glyphs().build_ui_state(
            move |ui, out: &mut Vec<Command>| {
                let scene = Scene {
                    tools: &tools,
                    tr: &tr,
                };
                draw(ui, &scene, out);
            },
            Vec::new(),
        )
    }

    // T-TRN-002, T-AIX-002, T-SHD-002: the panel offers the tools on its line
    // and shows their answers.
    #[test]
    fn the_panel_asks_and_shows_answers() {
        let mut tools = LineTools::default();
        tools.set_line(
            "行ってきます".into(),
            String::new(),
            String::new(),
            Some((0, 1000)),
        );
        tools.translation = Answer::Done("I'm off".into());
        tools.explanation = Answer::Failed("no key".into());
        tools.shadow = Shadow::Done(Shadowed {
            recording: "/tmp/r.wav".into(),
            said: Some("いってきます".into()),
            score: Some(83),
        });
        let mut h = harness(tools);
        h.run();
        assert!(h.query_by_label("I'm off").is_some());
        assert!(h.query_by_label("no key").is_some());
        assert!(h.query_by_label("line-shadow-score score=83").is_some());
        for (label, command) in [
            ("line-translate  (Shift+T)", "LineTranslate"),
            ("line-explain  (G)", "LineExplain"),
            ("line-shadow  (Y)", "LineShadow"),
            ("line-read  (O)", "LineRead"),
            ("line-shadow-play", "LinePlayRecording"),
        ] {
            h.get_by_label(label).click();
            h.run();
            let out = std::mem::take(h.state_mut());
            assert!(format!("{out:?}").contains(command), "{label}: {out:?}");
        }
        // a click on the line is a lookup at that character
        h.get_by_label("行ってきます").click();
        h.run();
        assert!(
            matches!(h.state()[..], [Command::LineClicked(..)]),
            "{:?}",
            h.state()
        );
    }

    // T-TRN-002: without a line, nothing can be asked; a recording can be stopped.
    #[test]
    fn without_a_line_only_reading_is_offered() {
        let mut tools = LineTools {
            reading: Reading::Busy,
            ..LineTools::default()
        };
        tools.shadow = Shadow::Recording;
        let mut h = harness(tools);
        h.run_steps(3);
        assert!(h.query_by_label("line-none").is_some());
        assert!(h.query_by_label("line-read-busy").is_some());
        h.get_by_label("line-translate  (Shift+T)").click();
        h.get_by_label("line-read  (O)").click();
        h.run_steps(3);
        assert!(h.state().is_empty(), "{:?}", h.state());
        h.get_by_label("line-shadow-stop").click();
        h.run_steps(3);
        assert!(matches!(h.state()[..], [Command::LineShadow]));
    }
}
