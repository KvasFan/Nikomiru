// SPDX-License-Identifier: GPL-3.0-only

//! Graphic subtitle reading controls (SUB-033), shared by menu and list.

use crate::ui::Command;

pub(crate) fn show(
    ui: &mut egui::Ui,
    text: &str,
    fraction: Option<f32>,
    cancel: &str,
    out: &mut Vec<Command>,
) {
    if let Some(fraction) = fraction {
        ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).text(text));
    } else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(text);
        });
    }
    if ui.button(cancel).clicked() {
        out.push(Command::GraphicReadCancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    // T-SUB-033: preparation/progress and an accessible explicit cancellation command.
    #[test]
    fn reading_progress_offers_cancellation() {
        let mut h = Harness::builder().allow_missing_glyphs().build_ui(|ui| {
            show(
                ui,
                "16 / 64 pictures",
                Some(0.25),
                "Cancel reading",
                &mut Vec::new(),
            );
        });
        h.run_steps(2);
        h.get_by_label("16 / 64 pictures");
        let mut h = Harness::builder().allow_missing_glyphs().build_ui_state(
            |ui, out: &mut Vec<Command>| {
                show(ui, "Preparing", None, "Cancel reading", out);
            },
            Vec::new(),
        );
        h.run_steps(2);
        h.get_by_label("Preparing");
        h.get_by_label("Cancel reading").click();
        h.run_steps(2);
        assert!(matches!(h.state().as_slice(), [Command::GraphicReadCancel]));
    }
}
