// SPDX-License-Identifier: GPL-3.0-only

//! Vocabulary of a completed analysis, using shared word-status commands.

use super::{popups::Translate, word_list};
use crate::ui::Command;
use crate::words::ListedWord;

#[derive(Debug, Default)]
pub(crate) struct State {
    key: String,
    query: String,
    words: word_list::State,
}

/// Returns true when the user returns to the media list.
pub(crate) fn show(
    ui: &mut egui::Ui,
    key: String,
    title: &str,
    words: &[ListedWord],
    state: &mut State,
    tr: &Translate<'_>,
    out: &mut Vec<Command>,
) -> bool {
    if state.key != key {
        *state = State {
            key,
            ..Default::default()
        };
    }
    let mut close = false;
    ui.horizontal(|ui| {
        close = ui.button(tr("analysis-words-back", &[])).clicked();
        ui.heading(title);
    });
    let label = ui.label(tr("books-words-search", &[]));
    ui.add(egui::TextEdit::singleline(&mut state.query).desired_width(320.0))
        .labelled_by(label.id);
    let labels = word_list::Labels::translated(tr, state.words.ticked.len());

    ui.horizontal(|ui| {
        let shown = word_list::filtered(words, &state.query, &state.words);
        if ui
            .add_enabled(
                !shown.is_empty(),
                egui::Button::new(tr("analysis-export-shown", &[])),
            )
            .clicked()
        {
            out.push(Command::AnalysisWordsExport(
                shown.into_iter().cloned().collect(),
            ));
        }
        let selected: Vec<_> = words
            .iter()
            .filter(|w| {
                state
                    .words
                    .ticked
                    .contains(&nikomiru_library::words::word_key(
                        &w.expression,
                        &w.reading,
                    ))
            })
            .cloned()
            .collect();
        if ui
            .add_enabled(
                !selected.is_empty(),
                egui::Button::new(tr("analysis-export-selected", &[])),
            )
            .clicked()
        {
            out.push(Command::AnalysisWordsExport(selected));
        }
    });
    word_list::show(ui, words, &state.query, &labels, &mut state.words, out);
    close
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;
    use egui_kittest::{Harness, kittest::Queryable};

    struct Test {
        state: State,
        key: String,
        rows: Vec<ListedWord>,
        out: Vec<Command>,
    }

    // T-KNW-022: shown follows search/filter; selected includes hidden ticks.
    // T-KNW-020, T-KNW-021: a different analysis never inherits old selections.
    #[test]
    fn export_snapshots_follow_filters_selections_and_analysis_identity() {
        let row = |e: &str, r: &str, status| ListedWord {
            expression: e.into(),
            reading: r.into(),
            count: 2,
            status,
            card: false,
        };
        let mut h = Harness::builder()
            .with_size(egui::vec2(700.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    show(
                        ui,
                        t.key.clone(),
                        "Book",
                        &t.rows,
                        &mut t.state,
                        &|key, _| key.to_string(),
                        &mut t.out,
                    );
                },
                Test {
                    state: State::default(),
                    key: "a/1".into(),
                    rows: vec![
                        row("猫", "ねこ", WordStatus::Unknown),
                        row("犬", "いぬ", WordStatus::Known),
                    ],
                    out: vec![],
                },
            );
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().state.query = "いぬ".into();
        h.run();
        h.get_by_label("analysis-export-shown").click();
        h.run();
        assert!(
            matches!(&h.state().out[0], Command::AnalysisWordsExport(rows)
            if rows.len() == 1 && rows[0].expression == "犬")
        );
        h.get_by_label("analysis-export-selected").click();
        h.run();
        assert!(
            matches!(&h.state().out[1], Command::AnalysisWordsExport(rows)
            if rows.len() == 1 && rows[0].expression == "猫")
        );
        h.state_mut().state.query.clear();
        h.state_mut().state.words.new_only = true;
        h.run();
        h.get_by_label("analysis-export-shown").click();
        h.run();
        assert!(
            matches!(&h.state().out[2], Command::AnalysisWordsExport(rows)
            if rows.len() == 1 && rows[0].expression == "猫")
        );
        // T-KNW-027: the shown snapshot follows exact status/card filters.
        h.state_mut().rows[1].card = true;
        h.state_mut().state.words.new_only = false;
        h.state_mut().state.words.status = Some(WordStatus::Known);
        h.state_mut().state.words.card = word_list::CardFilter::With;
        h.run();
        h.get_by_label("analysis-export-shown").click();
        h.run();
        assert!(
            matches!(&h.state().out[3], Command::AnalysisWordsExport(rows)
            if rows.len() == 1 && rows[0].expression == "犬" && rows[0].card)
        );
        h.state_mut().key = "b/2".into();
        h.run();
        assert!(h.state().state.words.ticked.is_empty());
        assert!(h.state().state.query.is_empty());
    }
}
