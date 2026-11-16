// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    batch_mining::{Stage, State},
    session::Session,
    ui::Command,
};
use nikomiru_library::batches::Status;
fn status(s: &Status, tr: &dyn Fn(&str) -> String) -> String {
    tr(match s {
        Status::Planned => "batch-planned",
        Status::Sending => "batch-sending",
        Status::Created(_) => "batch-created",
        Status::Kept => "batch-kept",
        Status::Duplicate => "batch-duplicate",
        Status::Failed {
            uncertain: true, ..
        } => "batch-unresolved",
        Status::Failed {
            uncertain: false, ..
        } => "batch-failed",
        Status::Cancelled => "batch-cancelled",
    })
}
pub(super) fn show(ui: &mut egui::Ui, s: &Session, detail: &mut usize, out: &mut Vec<Command>) {
    draw(
        ui,
        &s.batch,
        s.phrases_writable(),
        detail,
        &|k| s.translator().tr(k, &[]),
        out,
    );
}
fn draw(
    ui: &mut egui::Ui,
    state: &State,
    writable: bool,
    detail: &mut usize,
    tr: &dyn Fn(&str) -> String,
    out: &mut Vec<Command>,
) {
    egui::ScrollArea::vertical()
        .id_salt("batch-mining-page")
        .show(ui, |ui| {
            ui.heading(tr("batch-title"));
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("batch-close")).clicked() {
                    out.push(Command::BatchClose);
                }
                if ui
                    .add_enabled(!state.stage.busy(), egui::Button::new(tr("batch-new")))
                    .clicked()
                {
                    out.push(Command::BatchNew);
                }
                if state.stage.busy() {
                    ui.spinner();
                }
                if state.stage.busy() && ui.button(tr("batch-stop")).clicked() {
                    out.push(Command::BatchCancel);
                }
            });
            ui.label(tr("batch-instructions"));
            if !writable {
                ui.label(tr("phrases-readonly"));
            }
            if let Some(error) = &state.error {
                ui.colored_label(super::palette::current().bad, error);
            }
            ui.label(format!("{} / 20", state.selection.entries.len()));
            ui.label(format!(
                "{} / {}",
                state.cursor.min(state.run.len()),
                state.run.len()
            ));
            let editable = !state.stage.busy() && state.active.is_none();
            for (index, entry) in state.selection.entries.iter().enumerate() {
                ui.push_id(&entry.operation, |ui| {
                    ui.separator();
                    if let Some(candidate) = entry.candidate() {
                        ui.horizontal_wrapped(|ui| {
                            let mut enabled = entry.enabled;
                            if ui
                                .add_enabled(
                                    editable,
                                    egui::Checkbox::new(
                                        &mut enabled,
                                        format!(
                                            "{} · {}",
                                            candidate.request.expression, candidate.request.reading
                                        ),
                                    ),
                                )
                                .changed()
                            {
                                out.push(Command::BatchEdit {
                                    index,
                                    context: None,
                                    enabled: Some(enabled),
                                    audio: None,
                                    image: None,
                                });
                            }
                            ui.label(status(&entry.status, tr));
                            if let Status::Created(id) = entry.status {
                                ui.label(id.to_string());
                            }
                            if ui
                                .selectable_label(*detail == index, tr("batch-details"))
                                .clicked()
                            {
                                *detail = index;
                            }
                            if !state.stage.busy()
                                && matches!(
                                    entry.status,
                                    Status::Failed {
                                        uncertain: false,
                                        ..
                                    } | Status::Cancelled
                                )
                                && ui.button(tr("batch-retry")).clicked()
                            {
                                out.push(Command::BatchRetry(index));
                            }
                        });
                        let mut selected = entry.selected;
                        ui.add_enabled_ui(editable, |ui| {
                            egui::ComboBox::from_label(tr("batch-context"))
                                .selected_text(format!(
                                    "{} · {}",
                                    entry.selected + 1,
                                    candidate.request.title
                                ))
                                .show_ui(ui, |ui| {
                                    for (i, c) in entry.contexts.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut selected,
                                            i,
                                            format!(
                                                "{} · {} · {}",
                                                i + 1,
                                                c.request.title,
                                                c.request.sentence
                                            ),
                                        );
                                    }
                                });
                            if selected != entry.selected {
                                out.push(Command::BatchEdit {
                                    index,
                                    context: Some(selected),
                                    enabled: None,
                                    audio: None,
                                    image: None,
                                });
                            }
                            let mut audio = entry.audio;
                            let mut image = entry.image;
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(
                                        candidate.request.audio && candidate.request.clip.is_some(),
                                        egui::Checkbox::new(&mut audio, tr("batch-audio")),
                                    )
                                    .changed()
                                {
                                    out.push(Command::BatchEdit {
                                        index,
                                        context: None,
                                        enabled: None,
                                        audio: Some(audio),
                                        image: None,
                                    });
                                }
                                if ui
                                    .add_enabled(
                                        candidate.request.picture.is_some()
                                            || !matches!(
                                                candidate.request.frame,
                                                crate::mining::FrameChoice::None
                                            ),
                                        egui::Checkbox::new(&mut image, tr("batch-image")),
                                    )
                                    .changed()
                                {
                                    out.push(Command::BatchEdit {
                                        index,
                                        context: None,
                                        enabled: None,
                                        audio: None,
                                        image: Some(image),
                                    });
                                }
                            });
                        });
                        ui.label(&candidate.request.sentence);
                        if let Some((start, end)) = candidate.request.clip {
                            ui.label(format!("{start}–{end} ms"));
                        }
                        if let Status::Failed { message, .. } = &entry.status {
                            ui.colored_label(super::palette::current().bad, message);
                        }
                        if *detail == index
                            && let Some(p) = &entry.preview
                        {
                            ui.label(format!("{} · {} · {}", p.destination, p.deck, p.note_type));
                            ui.label(p.tags.join(" "));
                            ui.label(tr("batch-media-planned"));
                            for (name, value) in &p.fields {
                                ui.collapsing(name, |ui| {
                                    let mut text = value.clone();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut text)
                                            .desired_rows(4)
                                            .desired_width(f32::INFINITY)
                                            .interactive(false),
                                    )
                                    .labelled_by(ui.label(name).id);
                                });
                            }
                        }
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        editable && state.selection.entries.iter().any(|e| e.enabled),
                        egui::Button::new(tr("batch-preview")),
                    )
                    .clicked()
                {
                    out.push(Command::BatchPreview);
                }
                if ui
                    .add_enabled(
                        writable
                            && state.stage == Stage::Ready
                            && state.active.is_none()
                            && state.selection.entries.iter().any(|e| e.enabled)
                            && state
                                .selection
                                .entries
                                .iter()
                                .filter(|e| e.enabled)
                                .all(|e| e.preview.is_some()),
                        egui::Button::new(tr("batch-start")),
                    )
                    .clicked()
                {
                    out.push(Command::BatchStart);
                }
                if ui
                    .add_enabled(
                        writable && !state.stage.busy() && state.active.is_some(),
                        egui::Button::new(tr("batch-continue")),
                    )
                    .clicked()
                {
                    out.push(Command::BatchContinue);
                }
            });
            ui.separator();
            ui.heading(tr("batch-history"));
            for journal in &state.journals {
                ui.collapsing(format!("{} · {}", journal.created_ms, journal.id), |ui| {
                    for item in &journal.items {
                        ui.push_id(&item.operation, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "{} · {} · {}",
                                    item.expression,
                                    item.reading,
                                    status(&item.status, tr)
                                ));
                                if item.status.unresolved()
                                    && ui
                                        .add_enabled(
                                            writable && !state.stage.busy(),
                                            egui::Button::new(tr("batch-reconcile")),
                                        )
                                        .clicked()
                                {
                                    out.push(Command::BatchReconcile(
                                        journal.id.clone(),
                                        item.operation.clone(),
                                    ));
                                }
                            });
                        });
                    }
                    if ui
                        .add_enabled(
                            writable && !state.stage.busy() && !journal.unresolved(),
                            egui::Button::new(tr("batch-delete-journal")),
                        )
                        .clicked()
                    {
                        out.push(Command::BatchDeleteJournal(journal.id.clone()));
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        batch_mining::{Candidate, Preview, Ticket},
        mining::{CardAction, FrameChoice, MineRequest},
    };
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    // T-BMG-001, T-BMG-002: explicit preview and Start are separate named controls; no preview cannot authorize Start.
    #[test]
    fn batch_controls_require_a_complete_preview_and_explicit_start() {
        let request = MineRequest {
            query: "猫".into(),
            entry: 0,
            expression: "猫".into(),
            reading: "ねこ".into(),
            matched: "猫".into(),
            sentence: "猫です。".into(),
            offset: Some(0),
            video: Default::default(),
            audio_input: Default::default(),
            audio_stream: None,
            clip: None,
            position_ms: 0,
            uid: None,
            title: "Owned".into(),
            action: CardAction::Add,
            frame: FrameChoice::None,
            picture: None,
            audio: false,
        };
        struct Model {
            state: State,
            out: Vec<Command>,
            detail: usize,
        }
        let mut state = State::default();
        assert!(state.selection.add(Candidate {
            request: request.clone(),
            source_uid: None
        }));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(
                        ui,
                        &m.state,
                        true,
                        &mut m.detail,
                        &str::to_owned,
                        &mut m.out,
                    )
                },
                Model {
                    state,
                    out: Vec::new(),
                    detail: 0,
                },
            );
        h.run();
        assert!(h.get_by_label("batch-start").accesskit_node().is_disabled());
        h.get_by_label("batch-preview").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BatchPreview))
        );
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BatchStart))
        );
        let operation = h.state().state.selection.entries[0].operation.clone();
        h.state_mut().state.selection.entries[0].preview = Some(Preview {
            ticket: Ticket {
                operation,
                request,
                configuration: String::new(),
                dictionaries: String::new(),
                source: String::new(),
                request_hash: String::new(),
                schema: Vec::new(),
            },
            destination: "anki".into(),
            deck: "Lapis".into(),
            note_type: "Lapis".into(),
            tags: Vec::new(),
            fields: vec![("Expression".into(), "猫".into())],
            audio: false,
            image: false,
        });
        h.state_mut().state.stage = Stage::Ready;
        h.run();
        h.get_by_label("batch-start").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::BatchStart))
        );
    }
}
