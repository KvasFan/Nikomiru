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
    pub exact: String,
    pub presets: String,
    pub preset_save: String,
    pub preset_apply: String,
    pub preset_clear: String,
    pub preset_hint: String,
    pub preset_too_long: String,
    pub named: [String; 4],
    pub scripts: [String; 4],
    pub html_labels: [String; 3],
    pub coverage_labels: [String; 2],
    pub reading_labels: [String; 4],
    pub lengths: [String; 3],
    pub batch: [String; 5],
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
    pub source: String,
    pub frequency_shown: String,
    pub frequency_selected: String,
    pub lookup: String,
    pub examples: Option<String>,
    pub shown_words: String,
    pub selected_words: String,
    pub occurrences: String,
    pub min_count: String,
    pub max_count: String,
    pub reset_counts: String,
    pub order: String,
    pub orders: [String; 4],
    pub copy_shown: String,
    pub copy_selected: String,
    pub copied: String,
    pub copy_limit: String,
    /// The four statuses, in the order of `STATUSES`.
    pub statuses: [String; 4],
}

impl Labels {
    pub(crate) fn translated(tr: &super::popups::Translate<'_>, ticked: usize) -> Self {
        Self {
            new_only: tr("subtitle-words-new-only", &[]),
            html_labels: [
                "vocabulary-html-tools",
                "vocabulary-html-shown",
                "vocabulary-html-selected",
            ]
            .map(|k| tr(k, &[])),
            coverage_labels: ["vocabulary-coverage-target", "vocabulary-coverage-reset"]
                .map(|k| tr(k, &[])),
            reading_labels: [
                "vocabulary-reading-tools",
                "vocabulary-reading-any",
                "vocabulary-reading-distinct",
                "vocabulary-reading-same",
            ]
            .map(|k| tr(k, &[])),
            batch: [
                "vocabulary-batch-tools",
                "vocabulary-batch-next",
                "vocabulary-batch-clear",
                "vocabulary-batch-invert",
                "vocabulary-batch-size",
            ]
            .map(|k| tr(k, &[])),
            lengths: [
                "vocabulary-length-min",
                "vocabulary-length-max",
                "vocabulary-length-reset",
            ]
            .map(|k| tr(k, &[])),
            scripts: [
                "vocabulary-script-any",
                "vocabulary-script-kanji",
                "vocabulary-script-kana",
                "vocabulary-script-latin",
            ]
            .map(|k| tr(k, &[])),
            exact: tr("vocabulary-exact", &[]),
            presets: tr("vocabulary-presets", &[]),
            preset_save: tr("vocabulary-preset-save", &[]),
            preset_apply: tr("vocabulary-preset-apply", &[]),
            preset_clear: tr("vocabulary-preset-clear", &[]),
            preset_hint: tr("vocabulary-preset-hint", &[]),
            preset_too_long: tr("vocabulary-preset-too-long", &[]),
            named: [
                "vocabulary-named-presets",
                "vocabulary-preset-name",
                "vocabulary-preset-store",
                "vocabulary-preset-delete",
            ]
            .map(|k| tr(k, &[])),
            all: tr("subtitle-words-all", &[]),
            ticked: tr("subtitle-words-ticked", &[("count", ticked.to_string())]),
            card: tr("subtitle-words-card", &[]),
            statuses: STATUSES.map(|s| tr(&format!("word-status-{}", s.name()), &[])),
            copy_shown: tr("vocabulary-copy-shown", &[]),
            copy_selected: tr("vocabulary-copy-selected", &[]),
            copied: tr("vocabulary-copied", &[]),
            copy_limit: tr("analysis-export-too-large", &[]),
            source: String::new(),
            frequency_shown: tr("frequency-export-shown", &[]),
            frequency_selected: tr("frequency-export-selected", &[]),
            lookup: tr("vocabulary-lookup", &[]),
            examples: None,
            shown_words: tr("vocabulary-shown-words", &[]),
            selected_words: tr("vocabulary-selected-words", &[]),
            occurrences: tr("vocabulary-occurrences", &[]),
            min_count: tr("vocabulary-min-count", &[]),
            max_count: tr("vocabulary-max-count", &[]),
            reset_counts: tr("vocabulary-reset-counts", &[]),
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
    pub min_length: usize,
    pub max_length: Option<usize>,
    pub script: u8,
    pub coverage_target: Option<u8>,
    pub reading_filter: u8,
    pub exact: bool,
    preset_name: String,
    pub status: Option<WordStatus>,
    pub card: CardFilter,
    pub order: WordOrder,
    pub min_count: usize,
    pub max_count: Option<usize>,
    batch_size: Option<usize>,
    copied: Option<bool>,
    /// The words ticked, by headword and reading.
    pub ticked: HashSet<String>,
}

#[derive(Debug, Clone)]
struct Preset {
    coverage_target: Option<u8>,
    reading_filter: u8,
    min_length: usize,
    max_length: Option<usize>,
    script: u8,
    query: String,
    new_only: bool,
    exact: bool,
    status: Option<WordStatus>,
    card: CardFilter,
    order: WordOrder,
    min_count: usize,
    max_count: Option<usize>,
}
impl Preset {
    fn capture(query: &str, state: &State) -> Option<Self> {
        if query.len() > 4096 {
            return None;
        }
        Some(Self {
            query: query.to_string(),
            script: state.script,
            reading_filter: state.reading_filter,
            coverage_target: state.coverage_target,
            min_length: state.min_length,
            max_length: state.max_length,
            new_only: state.new_only,
            exact: state.exact,
            status: state.status,
            card: state.card,
            order: state.order,
            min_count: state.min_count,
            max_count: state.max_count,
        })
    }
    fn apply(&self, query: &mut String, state: &mut State) {
        query.clone_from(&self.query);
        state.script = self.script;
        state.reading_filter = self.reading_filter;
        state.coverage_target = self.coverage_target;
        state.min_length = self.min_length;
        state.max_length = self.max_length;
        state.new_only = self.new_only;
        state.exact = self.exact;
        state.status = self.status;
        state.card = self.card;
        state.order = self.order;
        state.min_count = self.min_count;
        state.max_count = self.max_count;
    }
}

pub(super) fn sync_presets(ctx: &egui::Context, presets: &[nikomiru_core::config::StudyPreset]) {
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::unique("vocabulary-named-presets"),
            presets.to_vec(),
        )
    });
}

fn named_presets(
    ui: &mut egui::Ui,
    query: &mut String,
    labels: &Labels,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    use nikomiru_core::config::StudyPreset;
    egui::CollapsingHeader::new(&labels.named[0])
        .id_salt("vocabulary-persistent-presets")
        .show(ui, |ui| {
            let saved = ui
                .ctx()
                .data(|d| {
                    d.get_temp::<Vec<StudyPreset>>(egui::Id::unique("vocabulary-named-presets"))
                })
                .unwrap_or_default();
            ui.add(
                egui::TextEdit::singleline(&mut state.preset_name)
                    .char_limit(64)
                    .hint_text(&labels.named[1]),
            )
            .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &labels.named[1]));
            let snapshot = StudyPreset {
                script: state.script,
                reading_filter: state.reading_filter,
                coverage_target: state.coverage_target,
                min_length: state.min_length,
                max_length: state.max_length,
                name: state.preset_name.trim().to_string(),
                query: query.clone(),
                new_only: state.new_only,
                exact: state.exact,
                status: state
                    .status
                    .and_then(|s| STATUSES.iter().position(|x| *x == s))
                    .map(|i| i as u8),
                card: match state.card {
                    CardFilter::Any => 0,
                    CardFilter::With => 1,
                    CardFilter::Without => 2,
                },
                order: match state.order {
                    WordOrder::Frequent => 0,
                    WordOrder::Rare => 1,
                    WordOrder::Expression => 2,
                    WordOrder::Reading => 3,
                },
                min_count: state.min_count,
                max_count: state.max_count,
            };
            let available = saved.len() < 16 || saved.iter().any(|p| p.name == snapshot.name);
            if ui
                .add_enabled(
                    snapshot.valid() && available,
                    egui::Button::new(&labels.named[2]),
                )
                .clicked()
            {
                out.push(Command::StudyPresetSave(snapshot));
            }
            for p in saved {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(&p.name).clicked() {
                        Preset {
                            query: p.query.clone(),
                            script: p.script,
                            reading_filter: p.reading_filter,
                            coverage_target: p.coverage_target,
                            min_length: p.min_length,
                            max_length: p.max_length,
                            new_only: p.new_only,
                            exact: p.exact,
                            status: p.status.and_then(|i| STATUSES.get(usize::from(i)).copied()),
                            card: [CardFilter::Any, CardFilter::With, CardFilter::Without]
                                [usize::from(p.card)],
                            order: [
                                WordOrder::Frequent,
                                WordOrder::Rare,
                                WordOrder::Expression,
                                WordOrder::Reading,
                            ][usize::from(p.order)],
                            min_count: p.min_count,
                            max_count: p.max_count,
                        }
                        .apply(query, state);
                        ui.ctx().request_repaint();
                    }
                    if ui
                        .button(format!("{}: {}", labels.named[3], p.name))
                        .clicked()
                    {
                        out.push(Command::StudyPresetDelete(p.name.clone()));
                    }
                });
            }
        });
}

fn presets(ui: &mut egui::Ui, query: &mut String, labels: &Labels, state: &mut State) {
    egui::CollapsingHeader::new(&labels.presets)
        .id_salt("vocabulary-session-presets")
        .show(ui, |ui| {
            let id = egui::Id::unique("vocabulary-shared-study-filters");
            let saved = ui.ctx().data(|d| d.get_temp::<Preset>(id));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(query.len() <= 4096, egui::Button::new(&labels.preset_save))
                    .clicked()
                    && let Some(preset) = Preset::capture(query, state)
                {
                    ui.ctx().data_mut(|d| d.insert_temp(id, preset));
                }
                if ui
                    .add_enabled(saved.is_some(), egui::Button::new(&labels.preset_apply))
                    .clicked()
                    && let Some(preset) = &saved
                {
                    preset.apply(query, state);
                    ui.ctx().request_repaint();
                }
                if ui
                    .add_enabled(saved.is_some(), egui::Button::new(&labels.preset_clear))
                    .clicked()
                {
                    ui.ctx().data_mut(|d| d.remove::<Preset>(id));
                }
            });
            ui.label(if query.len() <= 4096 {
                &labels.preset_hint
            } else {
                &labels.preset_too_long
            });
        });
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

// Presence filters allow mixed-script headwords and keep original spelling.
fn script_keeps(script: u8, text: &str) -> bool {
    script == 0 || text.chars().any(|c| match script {
        1 => matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x323af),
        2 => matches!(c as u32, 0x3040..=0x30ff | 0x31f0..=0x31ff | 0xff66..=0xff9f | 0x1aff0..=0x1afff | 0x1b000..=0x1b16f),
        3 => c.is_alphabetic() && matches!(c as u32, 0x41..=0x5a | 0x61..=0x7a | 0xc0..=0x24f | 0x1e00..=0x1eff | 0xff21..=0xff3a | 0xff41..=0xff5a),
        _ => false,
    })
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
        .filter(|w| {
            let n = w.expression.chars().filter(|c| !c.is_whitespace()).count();
            n >= state.min_length && state.max_length.is_none_or(|max| n <= max)
        })
        .filter(|w| {
            let distinct = !w.reading.trim().is_empty() && w.reading.trim() != w.expression.trim();
            match state.reading_filter {
                1 => distinct,
                2 => !distinct,
                _ => true,
            }
        })
        .filter(|w| script_keeps(state.script, &w.expression))
        .filter(|w| w.count >= state.min_count && state.max_count.is_none_or(|max| w.count <= max))
        .filter(|w| {
            if state.exact {
                query.matches_exact(&[&w.expression, &w.reading])
            } else {
                query.matches(&[&w.expression, &w.reading])
            }
        })
        .collect();
    if let Some(percent) = state.coverage_target {
        retain_coverage(&mut rows, percent);
    }
    sort(&mut rows, state.order);
    rows
}

// Compute ceil(total * percent / 100) without overflowing even wide sums.
fn retain_coverage(rows: &mut Vec<&ListedWord>, percent: u8) {
    sort(rows, WordOrder::Frequent);
    let total: u128 = rows.iter().map(|w| w.count as u128).sum();
    if total == 0 {
        rows.clear();
        return;
    }
    let percent = u128::from(percent.clamp(1, 100));
    let threshold = total / 100 * percent + (total % 100 * percent).div_ceil(100);
    let mut sum = 0u128;
    let mut cutoff = None;
    let mut keep = 0;
    for word in rows.iter() {
        if cutoff.is_some_and(|count| word.count < count) {
            break;
        }
        sum += word.count as u128;
        keep += 1;
        if cutoff.is_none() && sum >= threshold {
            cutoff = Some(word.count);
        }
    }
    rows.truncate(keep);
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

#[derive(Debug, PartialEq, Eq)]
struct Coverage {
    all_words: usize,
    shown_words: usize,
    selected_words: usize,
    all_tokens: u128,
    shown_tokens: u128,
    selected_tokens: u128,
    percent: Option<u128>,
}

fn coverage(all: &[ListedWord], shown: &[&ListedWord], selected: &[&ListedWord]) -> Coverage {
    let sum = |rows: &[&ListedWord]| {
        rows.iter()
            .fold(0u128, |n, w| n.saturating_add(w.count as u128))
    };
    let all_tokens = all
        .iter()
        .fold(0u128, |n, w| n.saturating_add(w.count as u128));
    let shown_tokens = sum(shown);
    Coverage {
        all_words: all.len(),
        shown_words: shown.len(),
        selected_words: selected.len(),
        all_tokens,
        shown_tokens,
        selected_tokens: sum(selected),
        percent: shown_tokens
            .checked_mul(100)
            .and_then(|n| n.checked_div(all_tokens)),
    }
}

fn next_batch(shown: &[&ListedWord], state: &mut State) {
    let count = state.batch_size.unwrap_or(20).clamp(1, 1000);
    let fresh: Vec<_> = shown
        .iter()
        .map(|w| word_key(&w.expression, &w.reading))
        .filter(|key| !state.ticked.contains(key))
        .take(count)
        .collect();
    state.ticked.extend(fresh);
}

/// Draws the words of `all` that the query and the filter let through.
pub(crate) fn show(
    ui: &mut egui::Ui,
    all: &[ListedWord],
    query: &mut String,
    labels: &Labels,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    presets(ui, query, labels, state);
    named_presets(ui, query, labels, state, out);
    egui::CollapsingHeader::new(&labels.reading_labels[0])
        .id_salt("vocabulary-reading-filters")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut enabled = state.coverage_target.is_some();
                if ui
                    .checkbox(&mut enabled, &labels.coverage_labels[0])
                    .changed()
                {
                    state.coverage_target = enabled.then_some(90);
                }
                if let Some(percent) = &mut state.coverage_target {
                    ui.add(egui::DragValue::new(percent).range(1..=100).suffix("%"));
                }
                if ui.button(&labels.coverage_labels[1]).clicked() {
                    state.coverage_target = None;
                }
            });

            for (i, label) in labels.reading_labels[1..].iter().enumerate() {
                ui.selectable_value(&mut state.reading_filter, i as u8, label);
            }
        });
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
    ui.horizontal_wrapped(|ui| {
        let min = ui.label(&labels.min_count);
        ui.add(
            egui::DragValue::new(&mut state.min_count)
                .range(0..=usize::MAX)
                .speed(1.0),
        )
        .labelled_by(min.id);
        let mut limited = state.max_count.is_some();
        if ui.checkbox(&mut limited, &labels.max_count).changed() {
            state.max_count = limited.then(|| all.iter().map(|w| w.count).max().unwrap_or(0));
        }
        if let Some(max) = &mut state.max_count {
            ui.add(egui::DragValue::new(max).range(0..=usize::MAX).speed(1.0))
                .on_hover_text(&labels.max_count);
        }
        if ui
            .add_enabled(
                state.min_count != 0 || state.max_count.is_some(),
                egui::Button::new(&labels.reset_counts),
            )
            .clicked()
        {
            state.min_count = 0;
            state.max_count = None;
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (i, label) in labels.scripts.iter().enumerate() {
            ui.selectable_value(&mut state.script, i as u8, label);
        }
    });
    egui::CollapsingHeader::new(&labels.lengths[0])
        .id_salt("vocabulary-length-tools")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(&labels.lengths[0]);
                ui.add(egui::DragValue::new(&mut state.min_length).range(0..=4096))
                    .labelled_by(label.id);
                let mut bounded = state.max_length.is_some();
                if ui.checkbox(&mut bounded, &labels.lengths[1]).changed() {
                    state.max_length = bounded.then_some(8);
                }
                if let Some(max) = &mut state.max_length {
                    ui.add(egui::DragValue::new(max).range(0..=4096))
                        .on_hover_text(&labels.lengths[1]);
                }
                if ui.button(&labels.lengths[2]).clicked() {
                    state.min_length = 0;
                    state.max_length = None;
                }
            });
        });
    let shown = filtered(all, query, state);
    let key = |w: &ListedWord| word_key(&w.expression, &w.reading);
    egui::CollapsingHeader::new(&labels.html_labels[0])
        .id_salt("vocabulary-html-sheet")
        .show(ui, |ui| {
            for (label, rows) in [
                (&labels.html_labels[1], shown.clone()),
                (&labels.html_labels[2], selected(all, state)),
            ] {
                if ui
                    .add_enabled(!rows.is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    out.push(Command::VocabularyHtml {
                        source: labels.source.clone(),
                        rows: rows.into_iter().cloned().collect(),
                    });
                }
            }
        });

    egui::CollapsingHeader::new(&labels.batch[0])
        .id_salt("vocabulary-study-batches")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(&labels.batch[4]);
                ui.add(egui::DragValue::new(state.batch_size.get_or_insert(20)).range(1..=1000))
                    .labelled_by(label.id);
                if ui
                    .add_enabled(
                        shown.iter().any(|w| !state.ticked.contains(&key(w))),
                        egui::Button::new(&labels.batch[1]),
                    )
                    .clicked()
                {
                    next_batch(&shown, state);
                }
                if ui
                    .add_enabled(
                        shown.iter().any(|w| state.ticked.contains(&key(w))),
                        egui::Button::new(&labels.batch[2]),
                    )
                    .clicked()
                {
                    for w in &shown {
                        state.ticked.remove(&key(w));
                    }
                }
                if ui
                    .add_enabled(!shown.is_empty(), egui::Button::new(&labels.batch[3]))
                    .clicked()
                {
                    for w in &shown {
                        let id = key(w);
                        if !state.ticked.remove(&id) {
                            state.ticked.insert(id);
                        }
                    }
                }
            });
        });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.new_only, &labels.new_only);
        ui.checkbox(&mut state.exact, &labels.exact);
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
    ui.horizontal_wrapped(|ui| {
        let mut copy = None;
        if ui
            .add_enabled(!shown.is_empty(), egui::Button::new(&labels.copy_shown))
            .clicked()
        {
            copy = Some(shown.clone());
        }
        let selected = selected(all, state);
        if ui
            .add_enabled(
                !selected.is_empty(),
                egui::Button::new(&labels.copy_selected),
            )
            .clicked()
        {
            copy = Some(selected);
        }
        if let Some(rows) = copy {
            let rows: Vec<_> = rows.into_iter().cloned().collect();
            match crate::word_export::tsv(&rows) {
                Ok(text) => {
                    ui.ctx().copy_text(text);
                    state.copied = Some(true);
                }
                Err(_) => state.copied = Some(false),
            }
        }
        if let Some(ok) = state.copied {
            ui.label(if ok {
                &labels.copied
            } else {
                &labels.copy_limit
            });
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
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !shown.is_empty(),
                egui::Button::new(&labels.frequency_shown),
            )
            .clicked()
        {
            out.push(Command::FrequencyExport {
                source: labels.source.clone(),
                rows: shown.iter().map(|w| (*w).clone()).collect(),
            });
        }
        let rows = selected(all, state);
        if ui
            .add_enabled(
                !rows.is_empty(),
                egui::Button::new(&labels.frequency_selected),
            )
            .clicked()
        {
            out.push(Command::FrequencyExport {
                source: labels.source.clone(),
                rows: rows.into_iter().cloned().collect(),
            });
        }
    });
    let counts = coverage(all, &shown, &selected(all, state));
    let percent = counts
        .percent
        .map_or_else(|| "–".to_string(), |p| format!("{p}%"));
    ui.label(format!(
        "{}: {} / {} · {}: {} / {} ({percent})",
        labels.shown_words,
        counts.shown_words,
        counts.all_words,
        labels.occurrences,
        counts.shown_tokens,
        counts.all_tokens
    ));
    ui.label(format!(
        "{}: {} · {}: {}",
        labels.selected_words, counts.selected_words, labels.occurrences, counts.selected_tokens
    ));
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
                    let lookup = ui.small_button("⌕").on_hover_text(&labels.lookup);
                    lookup.widget_info(|| {
                        WidgetInfo::labeled(
                            Role::Button,
                            true,
                            format!("{}: {} {}", labels.lookup, w.expression, w.reading),
                        )
                    });
                    if lookup.clicked() {
                        let r = lookup.rect;
                        out.push(Command::VocabularyLookup {
                            text: w.expression.clone(),
                            anchor: crate::popup::Rect {
                                x: f64::from(r.min.x),
                                y: f64::from(r.min.y),
                                w: f64::from(r.width()),
                                h: f64::from(r.height()),
                            },
                        });
                    }
                    if let Some(label) = &labels.examples {
                        let examples = ui.small_button("≡").on_hover_text(label);
                        examples.widget_info(|| {
                            WidgetInfo::labeled(
                                Role::Button,
                                true,
                                format!("{label}: {} {}", w.expression, w.reading),
                            )
                        });
                        if examples.clicked() {
                            out.push(Command::VocabularyExamples {
                                expression: w.expression.clone(),
                                reading: w.reading.clone(),
                            });
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
                        html_labels: ["HTML vocabulary", "HTML shown", "HTML selected"]
                            .map(String::from),
                        coverage_labels: ["Coverage target", "Reset coverage"].map(String::from),
                        reading_labels: [
                            "Reading filters",
                            "Any reading",
                            "Distinct reading",
                            "No distinct reading",
                        ]
                        .map(String::from),
                        batch: [
                            "Study batches",
                            "Select next batch",
                            "Clear shown ticks",
                            "Invert shown ticks",
                            "Batch size",
                        ]
                        .map(String::from),
                        lengths: ["Minimum length", "Maximum length", "Reset length"]
                            .map(String::from),
                        scripts: ["Any script", "Kanji", "Kana", "Latin"].map(String::from),
                        exact: "Exact match".into(),
                        presets: "Session presets".into(),
                        preset_save: "Remember filters".into(),
                        preset_apply: "Apply filters".into(),
                        preset_clear: "Forget filters".into(),
                        preset_hint: "Session only".into(),
                        preset_too_long: "Query too long".into(),
                        named: [
                            "Named presets",
                            "Preset name",
                            "Store preset",
                            "Delete preset",
                        ]
                        .map(String::from),
                        all: "All".into(),
                        ticked: "Ticked:".into(),
                        card: "Card".into(),
                        statuses: ["new", "learning", "known", "ignore"].map(String::from),
                        copy_shown: "Copy shown".into(),
                        copy_selected: "Copy selected".into(),
                        copied: "Copied".into(),
                        copy_limit: "Too large".into(),
                        source: "Test source".into(),
                        frequency_shown: "Export frequency shown".into(),
                        frequency_selected: "Export frequency selected".into(),
                        lookup: "Lookup".into(),
                        examples: Some("Examples".into()),
                        shown_words: "Shown words".into(),
                        selected_words: "Selected words".into(),
                        occurrences: "Occurrences".into(),
                        min_count: "Min occurrences".into(),
                        max_count: "Max occurrences".into(),
                        reset_counts: "Reset occurrences".into(),
                        order: "Order".into(),
                        orders: ["Frequent", "Rare", "Expression", "Reading"].map(String::from),
                        status_filter: "Status filter".into(),
                        all_statuses: "All statuses".into(),
                        card_filter: "Card filter".into(),
                        cards: ["Any card", "With card", "Without card"].map(String::from),
                    };
                    show(
                        ui,
                        &t.words,
                        &mut t.query,
                        &labels,
                        &mut t.state,
                        &mut t.out,
                    );
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

    // T-KNW-029: clipboard is explicit and scoped; hidden selections retain identity.
    #[test]
    fn copy_actions_emit_only_the_requested_rows_without_changing_selection() {
        let mut h = harness();
        h.run();
        h.get_by_label("Copy selected").click();
        h.step();
        assert!(
            !h.output()
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
        );
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().query = "たべ".into();
        h.run();
        h.get_by_label("Copy shown").click();
        h.step();
        let copied = h
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .expect("explicit clipboard output");
        assert!(copied.contains("食べる\tたべる\tlearning\t2\tfalse"));
        assert!(!copied.contains("猫"));
        h.run();
        h.get_by_label("Copy selected").click();
        h.step();
        let copied = h
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .expect("selected clipboard output");
        assert!(copied.contains("猫\tねこ\tunknown\t5\tfalse"));
        assert!(!copied.contains("食べる"));
        assert_eq!(h.state().state.ticked.len(), 1);
        assert!(h.state().out.is_empty());
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

    // T-KNW-040: a preset crosses view state, includes all filters, and never restores source ticks or emits commands.
    #[test]
    fn session_presets_apply_across_views_and_preserve_target_ticks() {
        let mut h = harness();
        h.run();
        h.state_mut().query = "たべる".into();
        h.state_mut().state.exact = true;
        h.state_mut().state.min_count = 2;
        h.state_mut().state.max_count = Some(3);
        h.state_mut().state.status = Some(WordStatus::Learning);
        h.state_mut().state.order = WordOrder::Reading;
        h.run();
        h.get_by_label("食べる たべる").click();
        h.run();
        h.get_by_label("Session presets").click();
        h.run_steps(3);
        h.get_by_label("Remember filters").click();
        h.run();
        h.state_mut().state = State::default();
        h.state_mut().query.clear();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.get_by_label("Apply filters").click();
        h.run();
        assert_eq!(h.state().query, "たべる");
        assert!(h.state().state.exact);
        assert_eq!(
            (h.state().state.min_count, h.state().state.max_count),
            (2, Some(3))
        );
        assert_eq!(h.state().state.status, Some(WordStatus::Learning));
        assert_eq!(h.state().state.order, WordOrder::Reading);
        assert_eq!(
            h.state().state.ticked,
            HashSet::from([word_key("猫", "ねこ")])
        );
        h.get_by_label("Forget filters").click();
        h.run();
        assert!(
            h.ctx
                .data(|d| d.get_temp::<Preset>(egui::Id::unique("vocabulary-shared-study-filters")))
                .is_none()
        );
        assert!(h.state().out.is_empty());
    }

    // T-KNW-040: the session snapshot cannot retain an oversized query or selected identities.
    #[test]
    fn preset_capture_is_bounded_and_does_not_replace_selection() {
        let mut state = State::default();
        state.ticked.insert("source tick".into());
        assert!(Preset::capture(&"x".repeat(4097), &state).is_none());
        let preset = Preset::capture("猫", &state).unwrap();
        let mut target = State::default();
        target.ticked.insert("target tick".into());
        let mut query = String::new();
        preset.apply(&mut query, &mut target);
        assert_eq!(query, "猫");
        assert_eq!(target.ticked, HashSet::from(["target tick".to_string()]));
    }

    // T-KNW-039: the accessible exact toggle affects shown rows while retaining hidden selections.
    #[test]
    fn exact_mode_intersects_filters_and_preserves_hidden_ticks() {
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().query = "たべ".into();
        h.run();
        assert!(h.query_by_label("食べる たべる").is_some());
        h.get_by_label("Exact match").click();
        h.run();
        assert!(filtered(&h.state().words, "たべ", &h.state().state).is_empty());
        h.state_mut().query = "ﾀﾍﾞﾙ".into();
        h.run();
        assert!(h.query_by_label("食べる たべる").is_some());
        assert!(h.state().state.ticked.contains(&word_key("猫", "ねこ")));
        assert!(h.state().out.is_empty());
    }

    // T-KNW-038: dictionary export captures the source title and explicit shown/hidden-selected scope.
    #[test]
    fn frequency_export_preserves_the_requested_scope() {
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().query = "たべ".into();
        h.run();
        h.get_by_label("Export frequency shown").click();
        h.run();
        assert!(
            matches!(&h.state().out[0], Command::FrequencyExport { source, rows } if source == "Test source" && rows.len() == 1 && rows[0].expression == "食べる")
        );
        h.get_by_label("Export frequency selected").click();
        h.run();
        assert!(
            matches!(&h.state().out[1], Command::FrequencyExport { rows, .. } if rows.len() == 1 && rows[0].expression == "猫")
        );
    }

    // T-KNW-037: example actions keep the exact headword and reading, without implicit playback.
    #[test]
    fn examples_dispatch_exact_word_identity() {
        let mut h = harness();
        h.run();
        h.get_by_label("Examples: 食べる たべる").click();
        h.run();
        assert!(
            matches!(h.state().out.as_slice(), [Command::VocabularyExamples { expression, reading }] if expression == "食べる" && reading == "たべる")
        );
    }

    // T-KNW-036: dictionary actions carry an expression/anchor, with no implicit status or media command.
    #[test]
    fn study_lookup_preserves_ticks_and_dispatches_only_the_requested_word() {
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.get_by_label("Lookup: 食べる たべる").click();
        h.run();
        assert!(
            matches!(h.state().out.as_slice(), [Command::VocabularyLookup { text, .. }] if text == "食べる")
        );
        assert!(h.state().state.ticked.contains(&word_key("猫", "ねこ")));
    }

    // T-KNW-035: token coverage differs from distinct counts; hidden ticks and wide sums remain accurate.
    #[test]
    fn vocabulary_coverage_uses_tokens_and_handles_empty_and_large_counts() {
        let rows = vec![
            word("a", "", usize::MAX, WordStatus::Unknown),
            word("b", "", usize::MAX, WordStatus::Known),
        ];
        let c = coverage(&rows, &[&rows[0]], &[&rows[1]]);
        assert_eq!(c.all_tokens, 2 * usize::MAX as u128);
        assert_eq!(c.percent, Some(50));
        assert_eq!(c.shown_words, 1);
        assert_eq!(coverage(&[], &[], &[]).percent, None);
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().query = "たべ".into();
        h.run();
        assert!(
            h.query_by_label("Shown words: 1 / 3 · Occurrences: 2 / 11 (18%)")
                .is_some()
        );
        assert!(
            h.query_by_label("Selected words: 1 · Occurrences: 5")
                .is_some()
        );
    }

    // T-KNW-034: range intersections affect shown actions while hidden selected identities survive.
    #[test]
    fn occurrence_ranges_filter_rows_and_reset_without_changing_selections() {
        let mut h = harness();
        h.run();
        h.get_by_label("猫 ねこ").click();
        h.run();
        h.state_mut().state.min_count = 2;
        h.state_mut().state.max_count = Some(3);
        h.run();
        assert!(h.query_by_label("猫 ねこ").is_none());
        assert!(h.query_by_label("食べる たべる").is_some());
        assert_eq!(
            selected(&h.state().words, &h.state().state)[0].expression,
            "猫"
        );
        h.state_mut().state.min_count = 4;
        h.run();
        assert!(filtered(&h.state().words, "", &h.state().state).is_empty());
        h.get_by_label("Reset occurrences").click();
        h.run();
        assert_eq!(h.state().state.min_count, 0);
        assert!(h.state().state.max_count.is_none());
        assert!(h.query_by_label("猫 ねこ").is_some());
        assert!(h.state().out.is_empty());
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
    // T-KNW-041: saved presets apply across view state without changing ticks.
    #[test]
    fn named_preset_applies_and_deletes_explicitly() {
        let mut h = harness();
        sync_presets(
            &h.ctx,
            &[nikomiru_core::config::StudyPreset {
                name: "Cats".into(),
                query: "猫".into(),
                exact: true,
                ..Default::default()
            }],
        );
        h.state_mut().state.ticked.insert(word_key("が", "が"));
        h.run();
        h.get_by_label("Named presets").click();
        h.run();
        h.get_by_label("Cats").click();
        h.run();
        assert_eq!(h.state().query, "猫");
        assert!(h.state().state.exact);
        assert!(h.state().state.ticked.contains(&word_key("が", "が")));
        h.get_by_label("Delete preset: Cats").click();
        h.run();
        assert!(
            matches!(h.state().out.last(), Some(Command::StudyPresetDelete(name)) if name == "Cats")
        );
    }
    // T-KNW-042: mixed scripts, extended Han, half-width kana and accessible composition.
    #[test]
    fn script_filters_preserve_spelling_ticks_and_presets() {
        assert!(script_keeps(1, "𠀀"));
        assert!(script_keeps(2, "ﾈｺ"));
        assert!(script_keeps(3, "ÉＡ"));
        assert!(!script_keeps(3, "123猫"));
        assert!(script_keeps(1, "食べる") && script_keeps(2, "食べる"));
        let mut h = harness();
        h.run();
        h.state_mut().state.ticked.insert(word_key("猫", "ねこ"));
        h.get_by_label("Kana").click();
        h.run();
        assert!(h.query_by_label("猫 ねこ").is_none());
        h.get_by_label("食べる たべる");
        let preset = Preset::capture("", &h.state().state).unwrap();
        let mut state = State::default();
        preset.apply(&mut String::new(), &mut state);
        assert_eq!(state.script, 2);
        assert!(h.state().state.ticked.contains(&word_key("猫", "ねこ")));
    }
    // T-KNW-043: inclusive Unicode scalar bounds, whitespace and reversed ranges.
    #[test]
    fn length_filters_count_unicode_scalars_and_round_trip_presets() {
        let words = vec![
            word("𠀀 A", "", 1, WordStatus::Unknown),
            word("猫", "ねこ", 2, WordStatus::Known),
        ];
        let mut state = State {
            min_length: 2,
            max_length: Some(2),
            ..Default::default()
        };
        assert_eq!(filtered(&words, "", &state)[0].expression, "𠀀 A");
        let preset = Preset::capture("", &state).unwrap();
        state.min_length = 0;
        state.max_length = None;
        preset.apply(&mut String::new(), &mut state);
        assert_eq!((state.min_length, state.max_length), (2, Some(2)));
        state.max_length = Some(1);
        assert!(filtered(&words, "", &state).is_empty());
        let mut h = harness();
        h.state_mut().state.min_length = 2;
        h.run();
        assert!(h.query_by_label("猫 ねこ").is_none());
        h.get_by_label("Minimum length").click();
        h.run();
        h.get_by_label("Reset length").click();
        h.run();
        h.get_by_label("猫 ねこ");
    }
    // T-KNW-045: bounded additive batches follow current order and retain hidden ticks.
    #[test]
    fn vocabulary_batches_continue_without_reselecting_or_clearing_hidden_rows() {
        let mut h = harness();
        h.state_mut().state.batch_size = Some(1);
        h.state_mut().state.order = WordOrder::Rare;
        h.state_mut().state.ticked.insert(word_key("hidden", ""));
        h.run();
        h.get_by_label("Study batches").click();
        h.run();
        h.get_by_label("Select next batch").click();
        h.run();
        assert!(
            h.state()
                .state
                .ticked
                .contains(&word_key("食べる", "たべる"))
        );
        h.get_by_label("Select next batch").click();
        h.run();
        assert!(h.state().state.ticked.contains(&word_key("が", "が")));
        h.state_mut().query = "猫".into();
        h.run();
        h.get_by_label("Invert shown ticks").click();
        h.run();
        assert_eq!(h.state().state.ticked.len(), 4);
        h.get_by_label("Clear shown ticks").click();
        h.run();
        assert_eq!(h.state().state.ticked.len(), 3);
        let all = (0..1100)
            .map(|i| word(&i.to_string(), "", 1, WordStatus::Unknown))
            .collect::<Vec<_>>();
        let rows = all.iter().collect::<Vec<_>>();
        let mut state = State {
            batch_size: Some(usize::MAX),
            ..Default::default()
        };
        next_batch(&rows, &mut state);
        assert_eq!(state.ticked.len(), 1000);
        next_batch(&rows, &mut state);
        assert_eq!(state.ticked.len(), 1100);
        let mut state = State::default();
        next_batch(&rows, &mut state);
        assert_eq!(state.ticked.len(), 20);
    } // T-KNW-046: whitespace, absent/identical readings, presets and hidden selections.
    #[test]
    fn reading_filters_compose_and_restore_without_changing_ticks() {
        let all = vec![
            word("猫", " ねこ ", 2, WordStatus::Unknown),
            word("犬", "  ", 1, WordStatus::Unknown),
            word("かな", "かな", 3, WordStatus::Known),
        ];
        let mut state = State {
            reading_filter: 1,
            ..Default::default()
        };
        assert_eq!(filtered(&all, "", &state).len(), 1);
        state.reading_filter = 2;
        assert_eq!(filtered(&all, "", &state).len(), 2);
        let preset = Preset::capture("", &state).unwrap();
        state.reading_filter = 0;
        preset.apply(&mut String::new(), &mut state);
        assert_eq!(state.reading_filter, 2);
        let mut h = harness();
        h.run();
        h.state_mut().state.ticked.insert(word_key("猫", "ねこ"));
        h.get_by_label("Reading filters").click();
        h.run();
        h.get_by_label("No distinct reading").click();
        h.run();
        assert!(h.query_by_label("猫 ねこ").is_none());
        assert!(h.query_all_by_label("が").count() > 0);
        assert!(h.state().state.ticked.contains(&word_key("猫", "ねこ")));
    } // T-KNW-047: exact thresholds, tied boundary words, zero totals, wide counts and display order.
    #[test]
    fn coverage_target_includes_boundary_ties_before_display_order() {
        let all = vec![
            word("Z", "", 6, WordStatus::Unknown),
            word("A", "", 2, WordStatus::Unknown),
            word("B", "", 2, WordStatus::Known),
            word("C", "", 0, WordStatus::Unknown),
        ];
        let mut state = State {
            coverage_target: Some(60),
            order: WordOrder::Expression,
            ..Default::default()
        };
        assert_eq!(
            filtered(&all, "", &state)
                .iter()
                .map(|w| w.expression.as_str())
                .collect::<Vec<_>>(),
            ["Z"]
        );
        state.coverage_target = Some(70);
        assert_eq!(filtered(&all, "", &state).len(), 3);
        state.coverage_target = Some(100);
        assert_eq!(filtered(&all, "", &state).len(), 3);
        state.status = Some(WordStatus::Known);
        assert_eq!(filtered(&all, "", &state).len(), 1);
        state.status = None;
        let preset = Preset::capture("", &state).unwrap();
        state.coverage_target = None;
        preset.apply(&mut String::new(), &mut state);
        assert_eq!(state.coverage_target, Some(100));
        let wide = vec![
            word("A", "", usize::MAX, WordStatus::Unknown),
            word("B", "", usize::MAX, WordStatus::Unknown),
        ];
        state.coverage_target = Some(1);
        assert_eq!(filtered(&wide, "", &state).len(), 2);
        assert!(filtered(&[word("zero", "", 0, WordStatus::Known)], "", &state).is_empty());
        let mut h = harness();
        h.state_mut().state.coverage_target = Some(1);
        h.run();
        h.get_by_label("Reading filters").click();
        h.run();
        h.get_by_label("Reset coverage").click();
        h.run();
        assert_eq!(h.state().state.coverage_target, None);
    } // T-KNW-048: selected HTML captures hidden identities independently of shown filters.
    #[test]
    fn vocabulary_html_actions_capture_frozen_shown_and_selected_rows() {
        let mut h = harness();
        h.state_mut().state.ticked.insert(word_key("猫", "ねこ"));
        h.state_mut().query = "が".into();
        h.run();
        h.get_by_label("HTML vocabulary").click();
        h.run();
        h.get_by_label("HTML selected").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c,Command::VocabularyHtml { rows, .. } if rows.len()==1 && rows[0].expression == "猫")));
        h.get_by_label("HTML shown").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c,Command::VocabularyHtml { rows, .. } if rows.len()==1 && rows[0].expression == "が")));
    }
}
