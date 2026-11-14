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
    pub status_filter: String,
    pub all_statuses: String,
    pub card_filter: String,
    pub cards: [String; 3],
    pub order: String,
    pub orders: [String; 4],
    /// The four statuses, in the order of `STATUSES`.
    pub statuses: [String; 4],
}

impl Labels {
    pub(crate) fn translated(tr: &super::popups::Translate<'_>, ticked: usize) -> Self {
        Self {
            new_only: tr("subtitle-words-new-only", &[]),
            all: tr("subtitle-words-all", &[]),
            ticked: tr("subtitle-words-ticked", &[("count", ticked.to_string())]),
            card: tr("subtitle-words-card", &[]),
            statuses: STATUSES.map(|s| tr(&format!("word-status-{}", s.name()), &[])),
            order: tr("vocabulary-order", &[]),
            orders: [
                "vocabulary-frequency-desc",
                "vocabulary-frequency-asc",
                "vocabulary-expression",
                "vocabulary-reading",
            ]
            .map(|k| tr(k, &[])),
            status_filter: tr("vocabulary-status-filter", &[]),
            all_statuses: tr("vocabulary-all-statuses", &[]),
            card_filter: tr("vocabulary-card-filter", &[]),
            cards: [
                "vocabulary-any-card",
                "vocabulary-with-card",
                "vocabulary-without-card",
            ]
            .map(|k| tr(k, &[])),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardFilter {
    #[default]
    Any,
    With,
    Without,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordOrder {
    #[default]
    Frequent,
    Rare,
    Expression,
    Reading,
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
    pub status: Option<WordStatus>,
    pub card: CardFilter,
    pub order: WordOrder,
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
    state: &State,
) -> Vec<&'a ListedWord> {
    let query = nikomiru_text::search::Query::new(query);
    let mut rows: Vec<_> = all
        .iter()
        .filter(|w| {
            !state.new_only || matches!(w.status, WordStatus::Unknown | WordStatus::Learning)
        })
        .filter(|w| state.status.is_none_or(|status| status == w.status))
        .filter(|w| match state.card {
            CardFilter::Any => true,
            CardFilter::With => w.card,
            CardFilter::Without => !w.card,
        })
        .filter(|w| query.matches(&[&w.expression, &w.reading]))
        .collect();
    sort(&mut rows, state.order);
    rows
}

fn sort(rows: &mut [&ListedWord], order: WordOrder) {
    let lexical = |a: &ListedWord, b: &ListedWord| {
        a.expression
            .cmp(&b.expression)
            .then_with(|| a.reading.cmp(&b.reading))
    };
    fn reading(w: &ListedWord) -> &str {
        if w.reading.is_empty() {
            &w.expression
        } else {
            &w.reading
        }
    }
    rows.sort_by(|a, b| match order {
        WordOrder::Frequent => b.count.cmp(&a.count).then_with(|| lexical(a, b)),
        WordOrder::Rare => a.count.cmp(&b.count).then_with(|| lexical(a, b)),
        WordOrder::Expression => lexical(a, b),
        WordOrder::Reading => reading(a).cmp(reading(b)).then_with(|| lexical(a, b)),
    });
}

/// Selected identities, including hidden rows, in the user's chosen order.
pub(crate) fn selected<'a>(all: &'a [ListedWord], state: &State) -> Vec<&'a ListedWord> {
    let mut rows: Vec<_> = all
        .iter()
        .filter(|w| state.ticked.contains(&word_key(&w.expression, &w.reading)))
        .collect();
    sort(&mut rows, state.order);
    rows
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
    ui.horizontal_wrapped(|ui| {
        let orders = [
            WordOrder::Frequent,
            WordOrder::Rare,
            WordOrder::Expression,
            WordOrder::Reading,
        ];
        let index = orders
            .iter()
            .position(|order| order == &state.order)
            .unwrap_or(0);
        let response = egui::ComboBox::from_id_salt("vocabulary-order")
            .selected_text(&labels.orders[index])
            .show_ui(ui, |ui| {
                for (order, text) in orders.into_iter().zip(&labels.orders) {
                    ui.selectable_value(&mut state.order, order, text);
                }
            });
        response
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &labels.order));
        let response = egui::ComboBox::from_id_salt("vocabulary-status-filter")
            .selected_text(
                state
                    .status
                    .map_or(labels.all_statuses.as_str(), |s| label(labels, s)),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.status, None, &labels.all_statuses);
                for status in STATUSES {
                    ui.selectable_value(&mut state.status, Some(status), label(labels, status));
                }
            });
        response
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &labels.status_filter));
        let card_index = match state.card {
            CardFilter::Any => 0,
            CardFilter::With => 1,
            CardFilter::Without => 2,
        };
        let response = egui::ComboBox::from_id_salt("vocabulary-card-filter")
            .selected_text(&labels.cards[card_index])
            .show_ui(ui, |ui| {
                for (filter, label) in [CardFilter::Any, CardFilter::With, CardFilter::Without]
                    .into_iter()
                    .zip(&labels.cards)
                {
                    ui.selectable_value(&mut state.card, filter, label);
                }
            });
        response
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &labels.card_filter));
    });
    let shown = filtered(all, query, state);
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
                        order: "Order".into(),
                        orders: ["Frequent", "Rare", "Expression", "Reading"].map(String::from),
                        status_filter: "Status filter".into(),
                        all_statuses: "All statuses".into(),
                        card_filter: "Card filter".into(),
                        cards: ["Any card", "With card", "Without card"].map(String::from),
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

    // T-KNW-028: ordering is deterministic and preserves hidden selected identities.
    #[test]
    fn frequency_expression_and_reading_orders_combine_with_filters() {
        let rows = vec![
            word("b", "a", 2, WordStatus::Unknown),
            word("a", "z", 5, WordStatus::Known),
            word("c", "", 2, WordStatus::Learning),
        ];
        let mut state = State::default();
        let names = |rows: Vec<&ListedWord>| {
            rows.into_iter()
                .map(|w| w.expression.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(filtered(&rows, "", &state)), ["a", "b", "c"]);
        state.order = WordOrder::Rare;
        assert_eq!(names(filtered(&rows, "", &state)), ["b", "c", "a"]);
        state.order = WordOrder::Expression;
        assert_eq!(names(filtered(&rows, "", &state)), ["a", "b", "c"]);
        state.order = WordOrder::Reading;
        assert_eq!(names(filtered(&rows, "", &state)), ["b", "c", "a"]);
        state
            .ticked
            .extend([word_key("b", "a"), word_key("a", "z")]);
        state.status = Some(WordStatus::Known);
        assert_eq!(names(filtered(&rows, "", &state)), ["a"]);
        assert_eq!(names(selected(&rows, &state)), ["b", "a"]);
        assert_eq!(
            names(filtered(&rows, "missing", &state)),
            Vec::<String>::new()
        );
    }

    // T-KNW-027: exact filters intersect; hidden ticks remain explicit bulk targets.
    #[test]
    fn status_and_card_filters_keep_hidden_selections() {
        let mut h = harness();
        h.state_mut().words[0].card = true;
        h.run();
        h.get_by_label("Status filter").click();
        h.run();
        h.get_by_label("known").click();
        h.run_steps(2);
        assert!(h.query_by_label("猫 ねこ").is_none());
        assert!(h.query_all_by_label("が").count() > 0);
        h.get_by_label("Card filter").click();
        h.run();
        h.get_by_label("With card").click();
        h.run_steps(2);
        assert_eq!(h.query_all_by_label("が").count(), 0);
        h.get_by_label("Status filter").click();
        h.run();
        h.get_by_label("All statuses").click();
        h.run_steps(2);
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.get_by_label("Card filter").click();
        h.run();
        h.get_by_label("Without card").click();
        h.run_steps(2);
        assert!(h.query_by_label("猫 ねこ").is_none());
        h.get_by_label("known").click();
        h.run();
        assert!(
            matches!(h.state().out.as_slice(), [Command::WordsStatus(rows, WordStatus::Known)]
            if rows == &[("猫".into(), "ねこ".into())])
        );
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
