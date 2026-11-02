// SPDX-License-Identifier: GPL-3.0-only

//! The Review tab of the Library page (REV-004, REV-005): the card's word
//! and sentence, then its answer, and the four grades with when each brings
//! the card back.

use egui::{Align, Button, Key, Layout, Modifiers, RichText, TextureHandle, Ui, vec2};
use nikomiru_library::review::{Grade, Settings, answer};

use super::popups::Translate;
use super::{palette, theme};
use crate::review::{ReviewJob, ReviewPage, sentence_parts, span};
use crate::ui::Command;

/// What the tab keeps between frames.
#[derive(Default)]
pub(crate) struct State {
    /// The picture of the card shown, by its id and date.
    picture: Option<((u64, i64), TextureHandle)>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("picture", &self.picture.as_ref().map(|(k, _)| k))
            .finish()
    }
}

/// Now, for the intervals on the buttons.
pub(crate) struct Now {
    /// Unix ms.
    pub ms: i64,
    /// The local offset, seconds.
    pub offset_s: i32,
}

const COLUMN: f32 = 760.0;

fn grade_label(g: Grade) -> &'static str {
    match g {
        Grade::Again => "review-again",
        Grade::Hard => "review-hard",
        Grade::Good => "review-good",
        Grade::Easy => "review-easy",
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn draw(
    ui: &mut Ui,
    page: &ReviewPage,
    now: &Now,
    settings: &Settings,
    tr: &Translate<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let t = |id: &str| tr(id, &[]);
    let Some(review) = &page.review else {
        if let Some(error) = &page.error {
            ui.label(RichText::new(error).color(palette::current().bad));
        }
        ui.spinner();
        return;
    };
    let q = &review.queue;
    let typing = ui.ctx().text_edit_focused();
    ui.horizontal(|ui| {
        let counts = [
            (q.new, "review-count-new", palette::current().accent),
            (q.learning, "review-count-learning", palette::current().bad),
            (q.review, "review-count-due", palette::current().good),
        ];
        for (n, id, color) in counts {
            ui.label(RichText::new(tr(id, &[("count", n.to_string())])).color(color));
        }
        if page.busy {
            ui.spinner();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(t("review-reload")).clicked() {
                out.push(Command::Review(ReviewJob::Load));
            }
            let undo = ui.add_enabled(
                page.undo.is_some() && !page.busy,
                Button::new(t("review-undo")),
            );
            if undo.clicked() {
                out.push(Command::ReviewUndo);
            }
        });
    });
    if let Some(error) = &page.error {
        ui.label(RichText::new(error).color(palette::current().bad));
    }
    if !typing
        && page.undo.is_some()
        && !page.busy
        && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z))
    {
        out.push(Command::ReviewUndo);
    }
    let Some((id, card)) = q.cards.first() else {
        state.picture = None;
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(t("review-done"))
                    .size(22.0)
                    .family(theme::bold()),
            );
            if let Some(due) = q.next_due_ms {
                let (unit, n) = span(now.ms, due);
                let when = tr(unit, &[("n", n)]);
                ui.label(tr("review-next", &[("when", when)]));
            }
            ui.label(
                RichText::new(tr("review-total", &[("count", q.total.to_string())]))
                    .color(palette::current().details),
            );
            if q.total == 0 {
                ui.add(
                    egui::Label::new(
                        RichText::new(t("review-empty-hint")).color(palette::current().details),
                    )
                    .wrap(),
                );
            }
        });
        return;
    };
    let key = (*id, card.created_at);
    if state.picture.as_ref().map(|(k, _)| *k) != Some(key) {
        state.picture = review.picture.as_ref().map(|cover| {
            let image = egui::ColorImage::from_rgba_unmultiplied(cover.size, &cover.rgba);
            let texture =
                ui.ctx()
                    .load_texture(format!("review-{}", id), image, Default::default());
            (key, texture)
        });
    }
    let width = ui.available_width().min(COLUMN);
    let margin = ((ui.available_width() - width) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(margin);
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.vertical_centered(|ui| {
                if page.revealed && !card.reading.is_empty() && card.reading != card.expression {
                    ui.label(
                        RichText::new(&card.reading)
                            .size(20.0)
                            .color(palette::current().details),
                    );
                }
                ui.label(
                    RichText::new(&card.expression)
                        .size(44.0)
                        .family(theme::bold()),
                );
                if !card.sentence.is_empty() {
                    let (before, word, after) = sentence_parts(&card.sentence, card.word);
                    let mut job = egui::text::LayoutJob::default();
                    let font = egui::FontId::proportional(22.0);
                    let plain = egui::TextFormat::simple(font.clone(), palette::current().text);
                    let marked = egui::TextFormat {
                        font_id: egui::FontId::new(22.0, theme::bold()),
                        color: palette::current().accent,
                        ..Default::default()
                    };
                    job.append(before, 0.0, plain.clone());
                    job.append(word, 0.0, marked);
                    job.append(after, 0.0, plain);
                    job.wrap.max_width = width;
                    job.halign = Align::Center;
                    ui.label(job);
                }
            });
            if !page.revealed {
                ui.vertical_centered(|ui| {
                    let show = ui
                        .add(Button::new(t("review-show")).min_size(vec2(200.0, 36.0)))
                        .on_hover_text(t("review-show-keys"));
                    if show.clicked() {
                        out.push(Command::ReviewReveal);
                    }
                });
                if !typing
                    && ui.input_mut(|i| {
                        i.consume_key(Modifiers::NONE, Key::Space)
                            || i.consume_key(Modifiers::NONE, Key::Enter)
                    })
                {
                    out.push(Command::ReviewReveal);
                }
                return;
            }
            // the grades first, where the eye is: the rest may need a scroll
            let gap = ui.spacing().item_spacing.x;
            let button_width = (width - 3.0 * gap) / 4.0;
            ui.horizontal(|ui| {
                for (n, grade) in Grade::ALL.into_iter().enumerate() {
                    let due = answer(&card.memory, grade, now.ms, now.offset_s, settings).due_ms;
                    let (unit, count) = span(now.ms, due);
                    let text = format!("{}\n{}", t(grade_label(grade)), tr(unit, &[("n", count)]));
                    let pressed = ui
                        .add_enabled(
                            !page.busy,
                            Button::new(RichText::new(text).size(16.0))
                                .min_size(vec2(button_width, 52.0)),
                        )
                        .on_hover_text(format!("{}", n + 1))
                        .clicked();
                    if pressed {
                        out.push(Command::Review(ReviewJob::Answer(*id, grade)));
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if review.audio.is_some()
                    && ui
                        .button(t("review-sentence-audio"))
                        .on_hover_text("R")
                        .clicked()
                {
                    out.push(Command::ReviewAudio);
                }
                if ui.button(t("review-word-audio")).clicked() {
                    out.push(Command::ReviewWord);
                }
                if !card.video_uid.is_empty() && ui.button(t("review-scene")).clicked() {
                    out.push(Command::ReviewScene);
                }
                let delete = if page.confirm_delete == Some(*id) {
                    RichText::new(t("review-delete-confirm")).color(palette::current().bad)
                } else {
                    RichText::new(t("review-delete"))
                };
                if ui.add_enabled(!page.busy, Button::new(delete)).clicked() {
                    out.push(Command::ReviewDelete);
                }
                if !card.title.is_empty() {
                    ui.label(
                        RichText::new(&card.title)
                            .size(13.0)
                            .color(palette::current().details),
                    );
                }
            });
            ui.separator();
            for (name, text) in &card.glossaries {
                ui.label(
                    RichText::new(name)
                        .size(13.0)
                        .family(theme::bold())
                        .color(palette::current().details),
                );
                ui.add(egui::Label::new(RichText::new(text).size(17.0)).wrap());
            }
            if let Some((_, texture)) = &state.picture {
                let size = texture.size_vec2();
                let scale = (width / size.x).min(300.0 / size.y).min(1.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((texture.id(), size * scale)).corner_radius(6.0));
                });
            }
            if typing || page.busy {
                return;
            }
            let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4];
            for (key, grade) in keys.into_iter().zip(Grade::ALL) {
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    out.push(Command::Review(ReviewJob::Answer(*id, grade)));
                }
            }
            if review.audio.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::R)) {
                out.push(Command::ReviewAudio);
            }
        });
    });
}
