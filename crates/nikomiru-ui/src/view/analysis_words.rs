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
    let labels = word_list::Labels {
        new_only: tr("subtitle-words-new-only", &[]),
        all: tr("subtitle-words-all", &[]),
        ticked: tr(
            "subtitle-words-ticked",
            &[("count", state.words.ticked.len().to_string())],
        ),
        card: tr("subtitle-words-card", &[]),
        statuses: word_list::STATUSES.map(|st| tr(&format!("word-status-{}", st.name()), &[])),
    };
    word_list::show(ui, words, &state.query, &labels, &mut state.words, out);
    close
}
