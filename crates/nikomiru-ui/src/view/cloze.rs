// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    cloze::{Mode, ResultKind},
    session::Session,
    ui::Command,
};
use nikomiru_library::review::Grade;

pub(super) fn show(ui: &mut egui::Ui, s: &Session, query: &mut String, out: &mut Vec<Command>) {
    let tr = |key: &str| s.translator().tr(key, &[]);
    draw(ui, &s.clozes, s.phrases_writable(), query, &tr, out);
}
fn draw(
    ui: &mut egui::Ui,
    state: &crate::cloze::State,
    writable: bool,
    query: &mut String,
    tr: &dyn Fn(&str) -> String,
    out: &mut Vec<Command>,
) {
    egui::ScrollArea::vertical()
        .id_salt("cloze-page")
        .show(ui, |ui| {
            ui.heading(tr("cloze-title"));
            if ui.button(tr("cloze-close")).clicked() {
                out.push(Command::ClozeClose);
            }
            if state.busy {
                ui.spinner();
            }
            if let Some(error) = &state.error {
                ui.colored_label(super::palette::current().bad, error);
            }
            if !writable {
                ui.label(tr("phrases-readonly"));
            }
            ui.add_enabled_ui(!state.busy, |ui| {
                if let Some(round) = &state.round {
                    ui.label(format!(
                        "{} / {}",
                        round.index.min(round.rows.len()),
                        round.rows.len()
                    ));
                    ui.label(tr(if round.scheduled {
                        "cloze-scheduled"
                    } else {
                        "cloze-finite"
                    }));
                    if let Some(row) = round.current() {
                        if let Some(prompt) = row.prompt() {
                            ui.label(egui::RichText::new(prompt).size(24.0));
                        }
                        if let Some(correct) = round.feedback {
                            ui.label(tr(if correct {
                                "cloze-correct"
                            } else {
                                "cloze-incorrect"
                            }));
                            ui.label(&row.phrase.text);
                            ui.label(&row.phrase.definition);
                            if round.scheduled {
                                ui.horizontal_wrapped(|ui| {
                                    for (grade, key) in [
                                        (Grade::Again, "review-again"),
                                        (Grade::Hard, "review-hard"),
                                        (Grade::Good, "review-good"),
                                        (Grade::Easy, "review-easy"),
                                    ] {
                                        if ui.button(tr(key)).clicked() {
                                            out.push(Command::ClozeNext(grade, false));
                                        }
                                    }
                                });
                            } else if ui.button(tr("cloze-next")).clicked() {
                                out.push(Command::ClozeNext(
                                    if correct { Grade::Good } else { Grade::Again },
                                    false,
                                ));
                            }
                        } else {
                            match round.mode {
                                Mode::Typed => {
                                    let mut input = round.input.clone();
                                    let label = ui.label(tr("cloze-answer"));
                                    let field = ui
                                        .add(
                                            egui::TextEdit::singleline(&mut input)
                                                .id_salt("cloze-answer-input")
                                                .char_limit(4096),
                                        )
                                        .labelled_by(label.id);
                                    if input != round.input {
                                        out.push(Command::ClozeInput(input.clone()));
                                    }
                                    let enter = field.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if (ui
                                        .add_enabled(
                                            !input.trim().is_empty(),
                                            egui::Button::new(tr("cloze-submit")),
                                        )
                                        .clicked()
                                        || enter)
                                        && !input.trim().is_empty()
                                    {
                                        out.push(Command::ClozeSubmit);
                                    }
                                }
                                Mode::Choice => {
                                    for option in round.options() {
                                        if ui.button(&option).clicked() {
                                            out.push(Command::ClozeInput(option));
                                            out.push(Command::ClozeSubmit);
                                        }
                                    }
                                }
                            }
                        }
                        if ui.button(tr("cloze-skip")).clicked() {
                            out.push(Command::ClozeNext(Grade::Again, true));
                        }
                    } else {
                        ui.heading(tr("cloze-complete"));
                        for (kind, key) in [
                            (ResultKind::Correct, "cloze-correct"),
                            (ResultKind::Incorrect, "cloze-incorrect"),
                            (ResultKind::Skipped, "cloze-skipped"),
                        ] {
                            ui.label(format!(
                                "{}: {}",
                                tr(key),
                                round.results.iter().filter(|r| **r == kind).count()
                            ));
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(round.undo.is_some(), egui::Button::new(tr("cloze-undo")))
                            .clicked()
                        {
                            out.push(Command::ClozeUndo);
                        }
                        if ui.button(tr("cloze-cancel")).clicked() {
                            out.push(Command::ClozeCancelRound);
                        }
                    });
                } else if let Some(p) = &state.preview {
                    ui.heading(tr("cloze-preview"));
                    let prompt = nikomiru_text::cloze::mask(&p.context, &p.text);
                    if let Some(prompt) = &prompt {
                        ui.label(prompt);
                    }
                    ui.label(&p.text);
                    ui.label(&p.definition);
                    if ui
                        .add_enabled(
                            writable && p.valid() && prompt.is_some(),
                            egui::Button::new(tr("cloze-create")),
                        )
                        .clicked()
                    {
                        out.push(Command::ClozeCreate);
                    }
                    if ui.button(tr("cloze-cancel")).clicked() {
                        out.push(Command::ClozeCancelRound);
                    }
                    if prompt.is_none() {
                        ui.label(tr("cloze-unavailable"));
                    }
                } else {
                    ui.label(tr("cloze-instructions"));
                    ui.label(tr("cloze-rules"));
                    let label = ui.label(tr("phrases-search"));
                    ui.text_edit_singleline(query).labelled_by(label.id);
                    let matching = state
                        .items
                        .iter()
                        .filter(|r| crate::phrases::matches(&r.phrase, query))
                        .collect::<Vec<_>>();
                    ui.label(format!(
                        "{} / {}",
                        matching.len().min(crate::cloze::ROUND_LIMIT),
                        matching.len()
                    ));
                    let shown = matching
                        .into_iter()
                        .take(crate::cloze::ROUND_LIMIT)
                        .collect::<Vec<_>>();
                    let ids = shown
                        .iter()
                        .map(|r| r.phrase.id.clone())
                        .collect::<Vec<_>>();
                    ui.horizontal_wrapped(|ui| {
                        for (mode, key) in [
                            (Mode::Typed, "cloze-start-typed"),
                            (Mode::Choice, "cloze-start-choice"),
                        ] {
                            let available = !shown.is_empty()
                                && (mode == Mode::Typed
                                    || shown.iter().all(|r| {
                                        state.choice_languages.contains(&r.phrase.language)
                                    }));
                            if ui
                                .add_enabled(available, egui::Button::new(tr(key)))
                                .clicked()
                            {
                                out.push(Command::ClozeStart {
                                    ids: ids.clone(),
                                    mode,
                                    scheduled: false,
                                });
                            }
                        }
                        for (mode, key) in [
                            (Mode::Typed, "cloze-start-due"),
                            (Mode::Choice, "cloze-start-due-choice"),
                        ] {
                            let available = writable
                                && state.items.iter().any(|r| r.memory.is_some())
                                && (mode == Mode::Typed
                                    || shown.iter().all(|r| {
                                        state.choice_languages.contains(&r.phrase.language)
                                    }));
                            if ui
                                .add_enabled(available, egui::Button::new(tr(key)))
                                .clicked()
                            {
                                out.push(Command::ClozeStart {
                                    ids: Vec::new(),
                                    mode,
                                    scheduled: true,
                                });
                            }
                        }
                    });
                    for row in shown {
                        ui.push_id(&row.phrase.id, |ui| {
                            ui.separator();
                            ui.label(&row.phrase.context);
                            ui.label(&row.phrase.text);
                            ui.horizontal_wrapped(|ui| {
                                if row.memory.is_none()
                                    && ui
                                        .add_enabled(writable, egui::Button::new(tr("cloze-enrol")))
                                        .clicked()
                                {
                                    out.push(Command::ClozeEnrol(row.phrase.id.clone()));
                                }
                                if row.memory.is_some() {
                                    ui.label(tr("cloze-scheduled"));
                                }
                                if ui
                                    .add_enabled(writable, egui::Button::new(tr("cloze-delete")))
                                    .clicked()
                                {
                                    out.push(Command::ClozeDelete(row.phrase.id.clone()));
                                }
                            });
                        });
                    }
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    // T-CLO-001: an unanswered typed prompt has no answer/definition labels; submission is explicit.
    #[test]
    fn typed_cloze_conceals_answers_until_an_explicit_submission() {
        let row = nikomiru_library::cloze::Exercise {
            phrase: nikomiru_library::phrases::Phrase {
                id: "00000000-0000-4000-8000-000000000001".into(),
                language: "ja".into(),
                text: "猫".into(),
                definition: "cat meaning".into(),
                context: "猫です。猫！".into(),
                status: crate::words::WordStatus::Unknown,
                source: None,
                created_ms: 1,
                changed_ms: 1,
            },
            memory: None,
        };
        struct Model {
            state: crate::cloze::State,
            query: String,
            out: Vec<Command>,
        }
        let mut h = Harness::builder()
            .with_size(egui::vec2(900.0, 700.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(ui, &m.state, true, &mut m.query, &str::to_owned, &mut m.out)
                },
                Model {
                    state: crate::cloze::State {
                        open: true,
                        round: crate::cloze::Round::new(
                            vec![row.clone()],
                            &[row],
                            Mode::Typed,
                            false,
                        ),
                        ..Default::default()
                    },
                    query: String::new(),
                    out: Vec::new(),
                },
            );
        h.run();
        assert!(h.query_by_label("猫").is_none());
        assert!(h.query_by_label("cat meaning").is_none());
        assert!(h.query_by_label("[…]です。[…]！").is_some());
        h.state_mut().state.round.as_mut().unwrap().input = "猫".into();
        h.run();
        assert!(h.query_by_label("cat meaning").is_none());
        h.get_by_label("cloze-submit").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::ClozeSubmit))
        );
        h.state_mut().state.round.as_mut().unwrap().submit();
        h.run();
        assert!(h.query_by_label("猫").is_some());
        assert!(h.query_by_label("cat meaning").is_some());
    }
}
