// SPDX-License-Identifier: GPL-3.0-only

//! View-only selection from already analysed material (KNW-024).

use super::popups::Translate;
use crate::words::Score;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Order {
    #[default]
    Original,
    Most,
    Least,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    order: Order,
    only_scored: bool,
    minimum: u8,
}

impl State {
    pub(crate) fn controls(&mut self, ui: &mut egui::Ui, tr: &Translate<'_>) {
        ui.horizontal_wrapped(|ui| {
            let label = |order| {
                tr(
                    match order {
                        Order::Original => "analysis-order-original",
                        Order::Most => "analysis-order-most",
                        Order::Least => "analysis-order-least",
                    },
                    &[],
                )
            };
            let response = egui::ComboBox::from_id_salt("analysis-order")
                .selected_text(label(self.order))
                .show_ui(ui, |ui| {
                    for order in [Order::Original, Order::Most, Order::Least] {
                        ui.selectable_value(&mut self.order, order, label(order));
                    }
                });
            response.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tr("analysis-order", &[]))
            });
            ui.checkbox(&mut self.only_scored, tr("analysis-only-scored", &[]));
            ui.add_enabled(
                self.only_scored,
                egui::Slider::new(&mut self.minimum, 0..=100)
                    .text(tr("analysis-minimum-known", &[])),
            );
        });
    }

    /// Original indices of the visible rows, never changing persisted order.
    pub(crate) fn indices(
        &self,
        count: usize,
        score: impl Fn(usize) -> Option<Score>,
    ) -> Vec<usize> {
        let scores: Vec<_> = (0..count)
            .map(|i| score(i).filter(|s| s.words > 0))
            .collect();
        let mut indices: Vec<_> = (0..count)
            .filter(|&i| {
                !self.only_scored
                    || scores[i].is_some_and(|s| {
                        s.known_percent().unwrap_or(0) >= usize::from(self.minimum)
                    })
            })
            .collect();
        if self.order != Order::Original {
            indices.sort_by(|&a, &b| match (scores[a], scores[b]) {
                (Some(a), Some(b)) => {
                    let order = ((a.known as u128) * (b.words as u128))
                        .cmp(&((b.known as u128) * (a.words as u128)));
                    if self.order == Order::Most {
                        order.reverse()
                    } else {
                        order
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });
        }
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-024: compare ratios before rounding; stable ties and unscored rows.
    #[test]
    fn order_and_threshold_use_completed_ratios_without_touching_sources() {
        let score = |known, words| {
            Some(Score {
                known,
                words,
                ..Default::default()
            })
        };
        let mut rows = [
            None,
            score(1, 3),
            score(100, 300),
            score(334, 1000),
            score(0, 0),
            score(1, 1),
        ];
        let mut state = State {
            order: Order::Most,
            ..Default::default()
        };
        assert_eq!(state.indices(rows.len(), |i| rows[i]), [5, 3, 1, 2, 0, 4]);
        state.order = Order::Least;
        assert_eq!(state.indices(rows.len(), |i| rows[i]), [1, 2, 3, 5, 0, 4]);
        state.only_scored = true;
        state.minimum = 34;
        assert_eq!(state.indices(rows.len(), |i| rows[i]), [5]);
        rows[1] = score(3, 3);
        assert_eq!(state.indices(rows.len(), |i| rows[i]), [1, 5]);
        state.minimum = 0;
        state.order = Order::Original;
        assert_eq!(state.indices(rows.len(), |i| rows[i]), [1, 2, 3, 5]);
    }
}
