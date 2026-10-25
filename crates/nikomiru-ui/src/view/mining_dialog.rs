// SPDX-License-Identifier: GPL-3.0-only

//! The "Advanced mining" dialog (ANK-040, ANK-041), opened by Shift+click on
//! an entry's card button or Ctrl+Shift+Enter: the sentence with the lines
//! before and after it added or taken away, the clip's start and end with a
//! preview, the picture (the frame of the click, the frame at the start of
//! the sentence, none) and the sentence audio, then the card.

use egui::{
    Align, Button, DragValue, Frame, Layout, Margin, Rect, RichText, Stroke, Ui, UiBuilder, vec2,
};

use super::dict_page::full_screen;
use super::popups::Translate;
use super::{palette, theme};
use crate::mining::{AdvancedCard, AdvancedEdit, FrameKind};
use crate::session::Session;
use crate::ui::Command;

pub(crate) fn show(ctx: &egui::Context, s: &Session, card: &AdvancedCard, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    draw(ctx, card, &tr, out);
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text)
            .size(15.0)
            .family(theme::bold())
            .color(palette::current().details),
    );
}

/// Seconds with two decimals, for a position in ms.
fn seconds(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// Draws the dialog.
pub(crate) fn draw(
    ctx: &egui::Context,
    card: &AdvancedCard,
    tr: &Translate<'_>,
    out: &mut Vec<Command>,
) {
    let t = |id: &str| tr(id, &[]);
    let p = palette::current();
    let screen = ctx.content_rect();
    let size = vec2(
        640.0_f32.min(screen.width() - 32.0).max(0.0),
        560.0_f32.min(screen.height() - 32.0).max(0.0),
    );
    let mut edit = |e: AdvancedEdit| out.push(Command::Advanced(e));
    full_screen(ctx, "advanced-card", p.backdrop, |ui| {
        let rect = Rect::from_center_size(screen.center(), size);
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min)),
        );
        Frame::new()
            .fill(p.dialog)
            .stroke(Stroke::new(1.0, p.card_border))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(&mut ui, |ui| {
                ui.set_width((size.x - 34.0).max(0.0));
                ui.visuals_mut().override_text_color = Some(p.text);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(
                    RichText::new(tr(
                        "mining-advanced-title",
                        &[("word", card.expression.clone())],
                    ))
                    .size(20.0)
                    .family(theme::bold()),
                );

                // ANK-041: the sentence, the lines around it
                section(ui, &t("mining-sentence"));
                if card.sentence.is_empty() {
                    ui.label(RichText::new(t("mining-no-sentence")).color(p.details));
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("advanced-sentence")
                        .max_height(110.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&card.sentence).size(17.0)).wrap(),
                            );
                        });
                }
                if card.reference.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        let line = |ui: &mut Ui, enabled: bool, id: &str| {
                            ui.add_enabled(enabled, Button::new(t(id))).clicked()
                        };
                        let r = card.reference;
                        if line(ui, card.can_previous, "mining-add-previous") {
                            edit(AdvancedEdit::Previous(true));
                        }
                        if line(ui, Some(card.first) < r, "mining-drop-previous") {
                            edit(AdvancedEdit::Previous(false));
                        }
                        if line(ui, card.can_next, "mining-add-next") {
                            edit(AdvancedEdit::Next(true));
                        }
                        if line(ui, r.is_some_and(|r| card.last > r), "mining-drop-next") {
                            edit(AdvancedEdit::Next(false));
                        }
                    });
                }

                // the clip
                section(ui, &t("mining-audio-section"));
                let mut audio = card.audio;
                let has_clip = card.clip.is_some();
                if ui
                    .add_enabled(has_clip, egui::Checkbox::new(&mut audio, t("mining-audio")))
                    .changed()
                {
                    edit(AdvancedEdit::Audio(audio));
                }
                if let Some((start, end)) = card.clip {
                    ui.horizontal(|ui| {
                        let label = ui.label(t("mining-start"));
                        let mut v = seconds(start);
                        let r = ui.add(
                            DragValue::new(&mut v)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .suffix(" s"),
                        );
                        let r = r.labelled_by(label.id);
                        if r.changed() {
                            edit(AdvancedEdit::Start((v * 1000.0).round() as i64));
                        }
                        ui.add_space(12.0);
                        let label = ui.label(t("mining-end"));
                        let mut v = seconds(end);
                        let r = ui.add(
                            DragValue::new(&mut v)
                                .speed(0.01)
                                .fixed_decimals(2)
                                .suffix(" s"),
                        );
                        let r = r.labelled_by(label.id);
                        if r.changed() {
                            edit(AdvancedEdit::End((v * 1000.0).round() as i64));
                        }
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("{:.2} s", seconds(end - start)))
                                .color(p.details),
                        );
                        ui.add_space(12.0);
                        if ui.button(t("mining-preview")).clicked() {
                            edit(AdvancedEdit::Preview);
                        }
                    });
                }

                // the picture
                section(ui, &t("mining-picture"));
                for (kind, id) in [
                    (FrameKind::Click, "mining-picture-click"),
                    (FrameKind::SentenceStart, "mining-picture-start"),
                    (FrameKind::None, "mining-picture-none"),
                ] {
                    let enabled = kind != FrameKind::SentenceStart || card.reference.is_some();
                    if ui
                        .add_enabled(enabled, egui::RadioButton::new(card.frame == kind, t(id)))
                        .clicked()
                    {
                        edit(AdvancedEdit::Frame(kind));
                    }
                }

                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new(t("mining-create")).family(theme::bold()))
                        .clicked()
                    {
                        edit(AdvancedEdit::Create);
                    }
                    if ui.button(t("mining-cancel")).clicked() {
                        edit(AdvancedEdit::Cancel);
                    }
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Id;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        card: AdvancedCard,
        out: Vec<Command>,
    }

    // T-ANK-040, T-ANK-041
    #[test]
    fn the_dialog_sends_what_is_chosen() {
        let card = AdvancedCard {
            level: 0,
            entry: 1,
            expression: "食べる".into(),
            reference: Some(4),
            first: 4,
            last: 4,
            sentence: "食べました".into(),
            offset: Some(0),
            clip: Some((1_880, 2_700)),
            frame: FrameKind::Click,
            audio: true,
            can_previous: true,
            can_next: false,
        };
        let mut h = Harness::builder()
            .with_size(vec2(1000.0, 800.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), egui::Sense::click());
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (_, v) in args {
                            text.push(' ');
                            text.push_str(v);
                        }
                        text
                    };
                    draw(ui.ctx(), &t.card, &tr, &mut t.out);
                },
                Test {
                    card,
                    out: Vec::new(),
                },
            );
        h.run();
        h.get_by_label("mining-advanced-title 食べる");
        h.get_by_label("mining-add-previous").click();
        h.run();
        // nothing after the sentence, nothing to take away yet
        h.get_by_label("mining-add-next").click();
        h.get_by_label("mining-drop-previous").click();
        h.run();
        h.get_by_label("mining-picture-start").click();
        h.get_by_label("mining-audio").click();
        h.run();
        h.get_by_label("mining-preview").click();
        h.get_by_label("mining-create").click();
        h.run();
        let edits: Vec<AdvancedEdit> = h
            .state()
            .out
            .iter()
            .filter_map(|c| match c {
                Command::Advanced(e) => Some(*e),
                _ => None,
            })
            .collect();
        assert_eq!(
            edits,
            [
                AdvancedEdit::Previous(true),
                AdvancedEdit::Frame(FrameKind::SentenceStart),
                AdvancedEdit::Audio(false),
                AdvancedEdit::Preview,
                AdvancedEdit::Create,
            ]
        );
    }
}
