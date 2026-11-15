// SPDX-License-Identifier: GPL-3.0-only

//! Explicit finite recall controls; concealed answers never enter widget names (KNW-053).
use crate::{recall::Recall, ui::Command};
#[derive(Default)]
pub(super) struct Labels {
    pub title: String,
    pub reveal: String,
    pub skip: String,
    pub complete: String,
    pub no_reading: String,
    pub remaining: String,
}
impl Labels {
    pub fn translated(tr: &super::popups::Translate<'_>) -> Self {
        Self {
            title: tr("recall-title", &[]),
            reveal: tr("recall-reveal", &[]),
            skip: tr("recall-skip", &[]),
            complete: tr("recall-complete", &[]),
            no_reading: tr("recall-no-reading", &[]),
            remaining: tr("recall-remaining", &[]),
        }
    }
}
pub(super) fn show(ctx: &egui::Context, recall: &Recall, labels: &Labels, out: &mut Vec<Command>) {
    let mut open = true;
    egui::Window::new(format!("{} · {}", labels.title, recall.source))
        .id(egui::Id::unique("vocabulary-recall-window"))
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.7)
                .show(ui, |ui| {
                    ui.label(format!(
                        "{}: {} / {}",
                        labels.remaining,
                        recall.remaining(),
                        recall.order.len()
                    ));
                    if let Some(card) = recall.current() {
                        ui.label(format!("×{}", card.count));
                        if !card.reading.trim().is_empty() && card.reading != card.expression {
                            ui.heading(&card.reading);
                        } else {
                            ui.label(&labels.no_reading);
                        }
                        if recall.revealed {
                            ui.heading(&card.expression);
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(!recall.revealed, egui::Button::new(&labels.reveal))
                                .clicked()
                            {
                                out.push(Command::RecallReveal);
                            }
                            if ui.button(&labels.skip).clicked() {
                                out.push(Command::RecallSkip);
                            }
                        });
                    } else if recall.done() {
                        ui.heading(&labels.complete);
                    }
                });
        });
    if !open {
        out.push(Command::RecallClose);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::{ListedWord, WordStatus};
    use egui_kittest::{Harness, kittest::Queryable};
    struct Test {
        recall: Recall,
        out: Vec<Command>,
    }
    // T-KNW-053: the hidden headword is absent from accessible content; buttons only emit explicit session commands.
    #[test]
    fn recall_window_reveals_only_after_explicit_action() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let labels = Labels::translated(&|key, _| key.to_string());
                    show(ui.ctx(), &t.recall, &labels, &mut t.out);
                },
                Test {
                    recall: Recall::start(
                        "Study",
                        &[ListedWord {
                            expression: "猫".into(),
                            reading: "ねこ".into(),
                            count: 1,
                            status: WordStatus::Unknown,
                            card: false,
                        }],
                    )
                    .unwrap(),
                    out: Vec::new(),
                },
            );
        h.run();
        assert!(h.query_by_label("猫").is_none());
        h.get_by_label("ねこ");
        h.get_by_label("recall-reveal").click();
        h.run();
        assert!(matches!(h.state().out.last(), Some(Command::RecallReveal)));
        h.state_mut().recall.reveal();
        h.run();
        h.get_by_label("猫");
    }
}
