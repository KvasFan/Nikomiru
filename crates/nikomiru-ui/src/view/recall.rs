// SPDX-License-Identifier: GPL-3.0-only

//! Explicit finite recall controls; concealed answers never enter widget names (KNW-053).
use crate::{recall::Recall, ui::Command};
#[derive(Default)]
pub(super) struct Labels {
    pub file_labels: [String; 2],
    pub restart_labels: [String; 3],
    pub hints_ready: bool,
    pub hint_labels: [String; 2],
    pub lookup: String,
    pub grades: [String; 2],
    pub summary_labels: [String; 4],
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
            file_labels: ["recall-save", "recall-load"].map(|k| tr(k, &[])),
            restart_labels: [
                "recall-restart-original",
                "recall-restart-reverse",
                "recall-restart-shuffle",
            ]
            .map(|k| tr(k, &[])),
            hints_ready: false,
            hint_labels: ["recall-hints-capture", "recall-hints-reveal"].map(|k| tr(k, &[])),
            lookup: tr("vocabulary-lookup", &[]),
            grades: ["recall-again", "recall-good"].map(|k| tr(k, &[])),
            summary_labels: [
                "recall-good-total",
                "recall-again-total",
                "recall-skipped-total",
                "recall-attempts-total",
            ]
            .map(|k| tr(k, &[])),
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
        .id(egui::Id::unique(("vocabulary-recall-window", &recall.id)))
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.7)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(&labels.file_labels[0]).clicked() {
                            out.push(Command::RecallSave);
                        }
                        if ui.button(&labels.file_labels[1]).clicked() {
                            out.push(Command::RecallLoad);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (order, label) in labels.restart_labels.iter().enumerate() {
                            if ui.button(label).clicked() {
                                out.push(Command::RecallRestart(order as u8));
                            }
                        }
                    });
                    ui.label(format!(
                        "{}: {} / {}",
                        labels.remaining,
                        recall.remaining(),
                        recall.order.len()
                    ));
                    if ui
                        .add_enabled(
                            labels.hints_ready,
                            egui::Button::new(&labels.hint_labels[0]),
                        )
                        .clicked()
                    {
                        out.push(Command::RecallCaptureHints);
                    }
                    if let Some(card) = recall.current() {
                        if ui
                            .add_enabled(
                                !card.hints.is_empty() && !recall.hint_revealed,
                                egui::Button::new(&labels.hint_labels[1]),
                            )
                            .clicked()
                        {
                            out.push(Command::RecallRevealHint);
                        }
                        if recall.hint_revealed {
                            if let Some(source) = &recall.hint_source {
                                ui.label(source);
                            }
                            for hint in &card.hints {
                                ui.label(hint);
                            }
                        }

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
                            let lookup =
                                ui.add_enabled(recall.revealed, egui::Button::new(&labels.lookup));
                            if lookup.clicked() {
                                let r = lookup.rect;
                                out.push(Command::RecallLookup(crate::popup::Rect {
                                    x: f64::from(r.min.x),
                                    y: f64::from(r.min.y),
                                    w: f64::from(r.width()),
                                    h: f64::from(r.height()),
                                }));
                            }

                            for (good, label) in [false, true].into_iter().zip(&labels.grades) {
                                if ui
                                    .add_enabled(recall.revealed, egui::Button::new(label))
                                    .clicked()
                                {
                                    out.push(Command::RecallRate(good));
                                }
                            }

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
                        let stats = recall.summary();
                        for (label, n) in labels.summary_labels.iter().zip([
                            stats.good,
                            stats.again,
                            stats.skipped,
                            stats.attempts,
                        ]) {
                            ui.label(format!("{label}: {n}"));
                        }
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
    } // T-KNW-054: hidden answers keep self-rating controls disabled and completion exposes numeric summaries.
    #[test]
    fn rating_controls_require_reveal_and_show_completion_counts() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    show(
                        ui.ctx(),
                        &t.recall,
                        &Labels::translated(&|key, _| key.to_string()),
                        &mut t.out,
                    );
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
        h.get_by_label("recall-good").click();
        h.run();
        assert!(h.state().out.is_empty());
        h.state_mut().recall.reveal();
        h.run();
        h.get_by_label("recall-good").click();
        h.run();
        assert!(matches!(
            h.state().out.last(),
            Some(Command::RecallRate(true))
        ));
        h.state_mut().recall.rate(true);
        h.run();
        h.get_by_label("recall-good-total: 1");
        h.get_by_label("recall-attempts-total: 1");
    } // T-KNW-056: concealed headwords never enter disabled lookup labels or commands.
    #[test]
    fn recall_lookup_requires_reveal_and_retains_current_card() {
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    show(
                        ui.ctx(),
                        &t.recall,
                        &Labels::translated(&|key, _| key.to_string()),
                        &mut t.out,
                    );
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
        h.get_by_label("vocabulary-lookup").click();
        h.run();
        assert!(h.state().out.is_empty());
        assert!(h.query_by_label("猫").is_none());
        h.state_mut().recall.reveal();
        h.run();
        h.get_by_label("vocabulary-lookup").click();
        h.run();
        assert!(matches!(
            h.state().out.last(),
            Some(Command::RecallLookup(_))
        ));
        assert_eq!(h.state().recall.remaining(), 1);
    } // T-KNW-057: captured context appears only after its own reveal, without changing answer concealment.
    #[test]
    fn recall_sentence_hints_have_separate_explicit_reveal() {
        let mut round = Recall::start(
            "Study",
            &[ListedWord {
                expression: "猫".into(),
                reading: "ねこ".into(),
                count: 1,
                status: WordStatus::Unknown,
                card: false,
            }],
        )
        .unwrap();
        round.cards[0].hints = vec!["猫がいる".into()];
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    show(
                        ui.ctx(),
                        &t.recall,
                        &Labels::translated(&|key, _| key.to_string()),
                        &mut t.out,
                    )
                },
                Test {
                    recall: round,
                    out: Vec::new(),
                },
            );
        h.run();
        assert!(h.query_by_label("猫がいる").is_none());
        h.get_by_label("recall-hints-reveal").click();
        h.run();
        assert!(matches!(
            h.state().out.last(),
            Some(Command::RecallRevealHint)
        ));
        h.state_mut().recall.hint_revealed = true;
        h.run();
        h.get_by_label("猫がいる");
        assert!(h.query_by_label("猫").is_none());
        assert!(!h.state().recall.revealed);
    } // T-KNW-058: restart is an explicit command; a new round hides the old answer and preserves its captured data.
    #[test]
    fn recall_restart_controls_are_explicit() {
        let mut r = Recall::start(
            "Study",
            &[ListedWord {
                expression: "猫".into(),
                reading: "ねこ".into(),
                count: 1,
                status: WordStatus::Unknown,
                card: false,
            }],
        )
        .unwrap();
        r.reveal();
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    show(
                        ui.ctx(),
                        &t.recall,
                        &Labels::translated(&|key, _| key.to_string()),
                        &mut t.out,
                    )
                },
                Test {
                    recall: r,
                    out: Vec::new(),
                },
            );
        h.run();
        h.get_by_label("recall-restart-shuffle").click();
        h.run();
        assert!(matches!(
            h.state().out.last(),
            Some(Command::RecallRestart(2))
        ));
        h.state_mut()
            .recall
            .restart(crate::recall::Order::Shuffle, 1);
        h.run();
        assert!(h.query_by_label("猫").is_none());
    }
}
