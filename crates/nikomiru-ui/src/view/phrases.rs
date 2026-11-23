// SPDX-License-Identifier: GPL-3.0-only

use super::View;
use crate::{session::Session, ui::Command};
use nikomiru_library::words::WordStatus;

pub(super) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |key: &str| s.translator().tr(key, &[]);
    draw(
        ctx,
        &Scene {
            state: &s.phrases,
            writable: s.phrases_writable(),
            tr: &tr,
        },
        &mut view.phrases_open,
        &mut view.phrase_query,
        out,
    );
}
struct Scene<'a> {
    state: &'a crate::phrases::State,
    writable: bool,
    tr: &'a dyn Fn(&str) -> String,
}
fn draw(
    ctx: &egui::Context,
    scene: &Scene<'_>,
    open: &mut bool,
    query: &mut String,
    out: &mut Vec<Command>,
) {
    let tr = scene.tr;
    let state = scene.state;
    egui::Window::new(tr("phrases-title"))
        .id(egui::Id::unique("phrase-notebook"))
        .open(open)
        .default_width(680.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            if state.busy {
                ui.spinner();
            }
            if let Some(error) = &state.error {
                ui.colored_label(super::palette::current().bad, error);
            }
            if !scene.writable {
                ui.label(tr("phrases-readonly"));
            }
            ui.add_enabled_ui(!state.busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("batch-title")).clicked() {
                        out.push(Command::BatchOpen);
                    }
                    if ui.button(tr("bank-title")).clicked() {
                        out.push(Command::BankOpen);
                    }
                    if ui.button(tr("cloze-title")).clicked() {
                        out.push(Command::ClozeOpen);
                    }
                    if ui.button(tr("phrases-new")).clicked() {
                        out.push(Command::PhraseNew(false));
                    }
                    if ui.button(tr("phrases-capture")).clicked() {
                        out.push(Command::PhraseNew(true));
                    }
                });
                if let Some(original) = &state.editor {
                    let mut p = original.clone();
                    let label = ui.label(tr("phrases-text"));
                    ui.add(
                        egui::TextEdit::singleline(&mut p.text)
                            .id_salt("phrase-text")
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                    let label = ui.label(tr("phrases-definition"));
                    super::palette::add_boxy(
                        ui,
                        egui::TextEdit::multiline(&mut p.definition)
                            .id_salt("phrase-definition")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                    let label = ui.label(tr("phrases-context"));
                    super::palette::add_boxy(
                        ui,
                        egui::TextEdit::multiline(&mut p.context)
                            .id_salt("phrase-context")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                    let label = ui.label(tr("phrases-language"));
                    ui.add(egui::TextEdit::singleline(&mut p.language).id_salt("phrase-language"))
                        .labelled_by(label.id);
                    egui::ComboBox::from_label(tr("phrases-status"))
                        .selected_text(tr(&format!("word-status-{}", p.status.name())))
                        .show_ui(ui, |ui| {
                            for status in [
                                WordStatus::Unknown,
                                WordStatus::Learning,
                                WordStatus::Known,
                                WordStatus::Ignored,
                            ] {
                                ui.selectable_value(
                                    &mut p.status,
                                    status,
                                    tr(&format!("word-status-{}", status.name())),
                                );
                            }
                        });
                    if let Some(source) = &p.source {
                        ui.label(&source.title);
                    }
                    let valid = p.valid();
                    if &p != original {
                        out.push(Command::PhraseEdit(Some(p)));
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                valid && scene.writable,
                                egui::Button::new(tr("phrases-save")),
                            )
                            .clicked()
                        {
                            out.push(Command::PhraseSave);
                        }
                        if ui.button(tr("phrases-cancel")).clicked() {
                            out.push(Command::PhraseEdit(None));
                        }
                    });
                    if !valid {
                        ui.label(tr("phrases-invalid"));
                    }
                } else {
                    let label = ui.label(tr("phrases-search"));
                    ui.text_edit_singleline(query).labelled_by(label.id);
                    let rows = state
                        .items
                        .iter()
                        .filter(|p| crate::phrases::matches(p, query))
                        .collect::<Vec<_>>();
                    ui.label(format!("{} / {}", rows.len(), state.items.len()));
                    if ui
                        .add_enabled(!rows.is_empty(), egui::Button::new(tr("phrases-export")))
                        .clicked()
                    {
                        out.push(Command::PhraseExport(
                            rows.iter().map(|p| p.id.clone()).collect(),
                        ));
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("phrase-rows")
                        .show(ui, |ui| {
                            for p in rows {
                                ui.push_id(&p.id, |ui| {
                                    ui.separator();
                                    ui.label(egui::RichText::new(&p.text).strong());
                                    ui.label(&p.definition);
                                    ui.label(&p.context);
                                    ui.label(tr(&format!("word-status-{}", p.status.name())));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button(tr("cloze-preview")).clicked() {
                                            out.push(Command::ClozePreview(p.id.clone()));
                                        }
                                        if ui.button(tr("phrases-edit")).clicked() {
                                            out.push(Command::PhraseEdit(Some(p.clone())));
                                        }
                                        if ui
                                            .add_enabled(
                                                scene.writable,
                                                egui::Button::new(tr("phrases-delete")),
                                            )
                                            .clicked()
                                        {
                                            out.push(Command::PhraseDelete(p.id.clone()));
                                        }
                                        if p.source.is_some()
                                            && ui.button(tr("phrases-source")).clicked()
                                        {
                                            out.push(Command::PhraseSource(p.id.clone()));
                                        }
                                    });
                                });
                            }
                        });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    #[derive(Default)]
    struct Model {
        state: crate::phrases::State,
        open: bool,
        writable: bool,
        query: String,
        out: Vec<Command>,
    }
    // T-PHR-002: editing is a draft, saving is explicit, invalid/readonly drafts cannot be submitted.
    #[test]
    fn phrase_editor_requires_an_explicit_valid_writable_save() {
        let p = nikomiru_library::phrases::Phrase {
            id: "00000000-0000-4000-8000-000000000001".into(),
            language: "ja".into(),
            text: "猫が好き".into(),
            definition: String::new(),
            context: "猫が好きです。".into(),
            status: WordStatus::Unknown,
            source: None,
            created_ms: 1,
            changed_ms: 1,
        };
        let mut h = Harness::builder()
            .with_size(egui::vec2(900.0, 800.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(
                        ui.ctx(),
                        &Scene {
                            state: &m.state,
                            writable: m.writable,
                            tr: &str::to_owned,
                        },
                        &mut m.open,
                        &mut m.query,
                        &mut m.out,
                    );
                },
                Model {
                    state: crate::phrases::State {
                        editor: Some(p),
                        ..Default::default()
                    },
                    open: true,
                    writable: true,
                    ..Default::default()
                },
            );
        h.run();
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::PhraseSave))
        );
        h.get_by_label("phrases-save").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::PhraseSave))
        );
        h.state_mut().state.editor.as_mut().unwrap().text = "犬".into();
        h.run();
        assert!(
            h.get_by_label("phrases-save")
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().state.editor.as_mut().unwrap().text = "猫が好き".into();
        h.state_mut().writable = false;
        h.run();
        assert!(
            h.get_by_label("phrases-save")
                .accesskit_node()
                .is_disabled()
        );
        assert!(h.state().state.items.is_empty());
    }
}
