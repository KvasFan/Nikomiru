// SPDX-License-Identifier: GPL-3.0-only

use crate::{sentence_bank::State, session::Session, ui::Command};
#[derive(Clone, Copy)]
struct Controls {
    writable: bool,
    book: bool,
    cues: bool,
}
pub(super) fn show(
    ui: &mut egui::Ui,
    s: &Session,
    expression: &mut String,
    reading: &mut String,
    out: &mut Vec<Command>,
) {
    let tr = |key: &str| s.translator().tr(key, &[]);
    draw(
        ui,
        &s.bank,
        Controls {
            writable: s.phrases_writable(),
            book: s.books_page.open.as_ref().is_some_and(|o| !o.book.manga),
            cues: !s.subs.incomplete && !s.subs.timeline().cues().is_empty(),
        },
        expression,
        reading,
        &tr,
        out,
    );
}
fn draw(
    ui: &mut egui::Ui,
    state: &State,
    c: Controls,
    expression: &mut String,
    reading: &mut String,
    tr: &dyn Fn(&str) -> String,
    out: &mut Vec<Command>,
) {
    egui::ScrollArea::vertical()
        .id_salt("sentence-bank-page")
        .show(ui, |ui| {
            ui.heading(tr("bank-title"));
            if ui.button(tr("bank-close")).clicked() {
                out.push(Command::BankClose);
            }
            ui.label(tr("bank-instructions"));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        c.writable && c.book && !state.indexing,
                        egui::Button::new(tr("bank-index-book")),
                    )
                    .clicked()
                {
                    out.push(Command::BankIndex(true));
                }
                if ui
                    .add_enabled(
                        c.writable && c.cues && !state.indexing,
                        egui::Button::new(tr("bank-index-track")),
                    )
                    .clicked()
                {
                    out.push(Command::BankIndex(false));
                }
                if ui
                    .add_enabled(
                        c.writable && !state.indexing,
                        egui::Button::new(tr("bank-import")),
                    )
                    .clicked()
                {
                    out.push(Command::BankImport);
                }
                if state.indexing {
                    ui.spinner();
                    ui.label(tr("bank-indexing"));
                }
                if state.cancel.is_some() && ui.button(tr("bank-cancel")).clicked() {
                    out.push(Command::BankCancel);
                }
            });
            if !c.writable {
                ui.label(tr("phrases-readonly"));
            }
            if let Some(error) = &state.error {
                ui.colored_label(super::palette::current().bad, error);
            }
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(tr("bank-expression"));
                ui.text_edit_singleline(expression).labelled_by(label.id);
                let label = ui.label(tr("bank-reading"));
                ui.text_edit_singleline(reading).labelled_by(label.id);
                if ui
                    .add_enabled(
                        !expression.trim().is_empty(),
                        egui::Button::new(tr("bank-search")),
                    )
                    .clicked()
                {
                    out.push(Command::BankQuery(expression.clone(), reading.clone()));
                }
                if state.querying {
                    ui.spinner();
                }
            });
            ui.label(format!(
                "{} · {} / {}",
                tr("bank-snapshot"),
                state.expression,
                state.reading
            ));
            ui.label(format!("{} / 50", state.examples.len()));
            if ui
                .add_enabled(
                    !state.examples.is_empty(),
                    egui::Button::new(tr("bank-export")),
                )
                .clicked()
            {
                out.push(Command::BankExport);
            }
            for (index, example) in state.examples.iter().enumerate() {
                ui.push_id(("example", index), |ui| {
                    ui.separator();
                    let mut text = egui::text::LayoutJob::default();
                    let base = egui::TextFormat {
                        font_id: egui::TextStyle::Body.resolve(ui.style()),
                        color: ui.visuals().text_color(),
                        ..Default::default()
                    };
                    let mut at = 0;
                    for &(start, end) in &example.ranges {
                        let (start, end) = (start as usize, end as usize);
                        if let (Some(before), Some(word)) = (
                            example.sentence.text.get(at..start),
                            example.sentence.text.get(start..end),
                        ) {
                            text.append(before, 0.0, base.clone());
                            text.append(
                                word,
                                0.0,
                                egui::TextFormat {
                                    underline: egui::Stroke::new(1.5, ui.visuals().text_color()),
                                    ..Default::default()
                                },
                            );
                            at = end;
                        }
                    }
                    if let Some(rest) = example.sentence.text.get(at..) {
                        text.append(rest, 0.0, base.clone());
                    }
                    ui.label(text);
                    ui.label(&example.title);
                    if let Some(source) = &example.sentence.source {
                        ui.label(&source.title);
                        if ui.button(tr("phrases-source")).clicked() {
                            out.push(Command::BankSource(Box::new(example.clone())));
                        }
                    }
                });
            }
            ui.separator();
            ui.heading(tr("bank-sources"));
            for batch in &state.batches {
                ui.push_id(&batch.key, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{} ({})", batch.title, batch.count));
                        if ui
                            .add_enabled(
                                c.writable && !state.indexing,
                                egui::Button::new(tr("bank-delete")),
                            )
                            .clicked()
                        {
                            out.push(Command::BankDelete(batch.key.clone()));
                        }
                    });
                });
            }
        });
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    // T-SEN-002: named exact-query controls, available source indexing and read-only mutation controls.
    #[test]
    fn sentence_browser_requires_explicit_exact_queries_and_respects_readonly() {
        struct Model {
            state: State,
            expression: String,
            reading: String,
            out: Vec<Command>,
            writable: bool,
        }
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, m: &mut Model| {
                    draw(
                        ui,
                        &m.state,
                        Controls {
                            writable: m.writable,
                            book: true,
                            cues: true,
                        },
                        &mut m.expression,
                        &mut m.reading,
                        &str::to_owned,
                        &mut m.out,
                    )
                },
                Model {
                    state: State::default(),
                    expression: "食べる".into(),
                    reading: "たべる".into(),
                    out: Vec::new(),
                    writable: true,
                },
            );
        h.run();
        assert!(h.state().out.is_empty());
        h.get_by_label("bank-search").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c,Command::BankQuery(e,r) if e=="食べる" && r=="たべる"))
        );
        h.state_mut().writable = false;
        h.run();
        assert!(
            h.get_by_label("bank-index-book")
                .accesskit_node()
                .is_disabled()
        );
        assert!(h.get_by_label("bank-import").accesskit_node().is_disabled());
    }
}
