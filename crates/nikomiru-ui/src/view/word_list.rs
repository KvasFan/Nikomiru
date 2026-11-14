// SPDX-License-Identifier: GPL-3.0-only

//! The words of the subtitle being watched (KNW-009), a tab of the subtitle
//! list: each word with how often it comes, what the learner knows of it
//! and whether it has a card, the most frequent first; the search and "only
//! words not known" narrow it; the words ticked get a status at once.

use std::collections::HashSet;

use egui::{Color32, RichText, Role, ScrollArea, WidgetInfo};
use nikomiru_library::words::word_key;

use super::{palette, theme};
use crate::ui::Command;
use crate::words::{ListedWord, Underline, WordStatus};

/// The height of a row.
const ROW: f32 = 26.0;
const KNOWN: Color32 = Color32::from_rgb(0x8f, 0xf0, 0xa4);
const IGNORED: Color32 = Color32::from_rgb(0x80, 0x80, 0x88);
const READING: Color32 = Color32::from_rgb(0x9a, 0xb8, 0xff);

/// The strings of the tab.
#[derive(Debug, Clone, Default)]
pub(crate) struct Labels {
    /// "Only words not known".
    pub new_only: String,
    /// "All shown": ticks them.
    pub all: String,
    /// "{n} ticked:", given the number.
    pub ticked: String,
    /// "A card was made of it".
    pub card: String,
    /// The four statuses, in the order of `STATUSES`.
    pub statuses: [String; 4],
}

/// The statuses offered, in order.
pub(crate) const STATUSES: [WordStatus; 4] = [
    WordStatus::Unknown,
    WordStatus::Learning,
    WordStatus::Known,
    WordStatus::Ignored,
];

/// What the tab keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Only the words not known are shown.
    pub new_only: bool,
    /// The words ticked, by headword and reading.
    pub ticked: HashSet<String>,
}

fn color(status: WordStatus) -> Color32 {
    match status {
        WordStatus::Unknown => palette::underline(Underline::Unknown),
        WordStatus::Learning => palette::underline(Underline::Learning),
        WordStatus::Known => KNOWN,
        WordStatus::Ignored => IGNORED,
    }
}

fn label(labels: &Labels, status: WordStatus) -> &str {
    let i = STATUSES.iter().position(|s| *s == status).unwrap_or(0);
    &labels.statuses[i]
}

/// Rows visible under the shared search and status filter.
pub(crate) fn filtered<'a>(
    all: &'a [ListedWord],
    query: &str,
    new_only: bool,
) -> Vec<&'a ListedWord> {
    let query = nikomiru_text::search::Query::new(query);
    all.iter()
        .filter(|w| !new_only || matches!(w.status, WordStatus::Unknown | WordStatus::Learning))
        .filter(|w| query.matches(&[&w.expression, &w.reading]))
        .collect()
}

/// Draws the words of `all` that the query and the filter let through.
pub(crate) fn show(
    ui: &mut egui::Ui,
    all: &[ListedWord],
    query: &str,
    labels: &Labels,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let shown = filtered(all, query, state.new_only);
    let key = |w: &ListedWord| word_key(&w.expression, &w.reading);
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.new_only, &labels.new_only);
        let mut every = !shown.is_empty() && shown.iter().all(|w| state.ticked.contains(&key(w)));
        if ui.checkbox(&mut every, &labels.all).changed() {
            for w in &shown {
                if every {
                    state.ticked.insert(key(w));
                } else {
                    state.ticked.remove(&key(w));
                }
            }
        }
    });
    // the words ticked, whether shown or not, get the status chosen
    if !state.ticked.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(&labels.ticked);
            for status in STATUSES {
                let name = label(labels, status);
                if ui
                    .button(RichText::new(name).color(color(status)))
                    .clicked()
                {
                    let words = all
                        .iter()
                        .filter(|w| state.ticked.contains(&key(w)))
                        .map(|w| (w.expression.clone(), w.reading.clone()))
                        .collect();
                    out.push(Command::WordsStatus(words, status));
                    state.ticked.clear();
                }
            }
        });
    }
    ScrollArea::vertical()
        .id_salt("subtitle-list-words")
        .auto_shrink([false, false])
        .show_rows(ui, ROW, shown.len(), |ui, range| {
            for w in &shown[range] {
                ui.horizontal(|ui| {
                    ui.set_height(ROW);
                    let k = key(w);
                    let mut ticked = state.ticked.contains(&k);
                    let name = if w.reading.is_empty() || w.reading == w.expression {
                        w.expression.clone()
                    } else {
                        format!("{} {}", w.expression, w.reading)
                    };
                    let check = ui.checkbox(&mut ticked, "");
                    check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, ticked, &name));
                    if check.changed() {
                        if ticked {
                            state.ticked.insert(k);
                        } else {
                            state.ticked.remove(&k);
                        }
                    }
                    ui.label(RichText::new("●").color(color(w.status)))
                        .on_hover_text(label(labels, w.status));
                    ui.label(RichText::new(&w.expression).family(theme::bold()));
                    if !w.reading.is_empty() && w.reading != w.expression {
                        ui.label(RichText::new(&w.reading).color(READING));
                    }
                    if w.card {
                        ui.label(RichText::new("◆").color(READING))
                            .on_hover_text(&labels.card);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("×{}", w.count)).color(READING));
                    });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use egui::vec2;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    struct Test {
        words: Vec<ListedWord>,
        query: String,
        state: State,
        out: Vec<Command>,
    }

    fn word(expression: &str, reading: &str, count: usize, status: WordStatus) -> ListedWord {
        ListedWord {
            expression: expression.into(),
            reading: reading.into(),
            count,
            status,
            card: false,
        }
    }

    fn harness() -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(420.0, 600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let labels = Labels {
                        new_only: "Only new".into(),
                        all: "All".into(),
                        ticked: "Ticked:".into(),
                        card: "Card".into(),
                        statuses: ["new", "learning", "known", "ignore"].map(String::from),
                    };
                    show(ui, &t.words, &t.query, &labels, &mut t.state, &mut t.out);
                },
                Test {
                    words: vec![
                        word("猫", "ねこ", 5, WordStatus::Unknown),
                        word("が", "が", 4, WordStatus::Known),
                        word("食べる", "たべる", 2, WordStatus::Learning),
                    ],
                    query: String::new(),
                    state: State::default(),
                    out: Vec::new(),
                },
            )
    }

    // T-KNW-026: matching folds readings and combines terms without changing keys.
    #[test]
    fn vocabulary_search_accepts_kana_width_and_terms_across_fields() {
        let mut h = harness();
        h.state_mut().query = "猫 ﾈｺ".into();
        h.run();
        h.get_by_label("猫 ねこ");
        assert!(h.query_by_label("食べる たべる").is_none());
        h.state_mut().query = "タベル".into();
        h.run();
        h.get_by_label("食べる たべる");
        assert!(h.query_by_label("猫 ねこ").is_none());
    }

    // T-KNW-009: the words listed, narrowed, ticked and marked at once
    #[test]
    fn the_words_ticked_get_a_status() {
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ");
        assert!(h.query_all_by_label("が").count() >= 1);
        h.get_by_label("×5");
        assert!(h.query_by_label("known").is_none(), "nothing ticked");
        // only the words not known
        h.get_by_label("Only new").click();
        h.run();
        assert_eq!(h.query_all_by_label("が").count(), 0);
        // all of those shown ticked, then marked known
        h.get_by_label("All").click();
        h.run();
        h.get_by_label("known").click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        let [Command::WordsStatus(words, WordStatus::Known)] = out.as_slice() else {
            panic!("{out:?}")
        };
        let words: Vec<&str> = words.iter().map(|(e, _)| e.as_str()).collect();
        assert_eq!(words, ["猫", "食べる"]);
        assert!(h.state().state.ticked.is_empty());
        // the search narrows too
        h.get_by_label("Only new").click();
        h.state_mut().query = "たべ".into();
        h.run();
        assert!(h.query_by_label("猫 ねこ").is_none());
        h.get_by_label("食べる たべる").click();
        h.run();
        h.get_by_label("ignore").click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(out.as_slice(), [Command::WordsStatus(w, WordStatus::Ignored)] if w.len() == 1),
            "{out:?}"
        );
    }
}
