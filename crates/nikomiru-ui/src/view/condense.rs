// SPDX-License-Identifier: GPL-3.0-only

//! Episode/series audio export menus (AUD-010).

use super::popups::Translate;
use crate::{condense::Target, ui::Command};
use egui::Ui;

pub(crate) fn menu(
    ui: &mut Ui,
    tr: &Translate<'_>,
    target: Target,
    running: bool,
    out: &mut Vec<Command>,
) {
    use nikomiru_media::condense::Format;
    ui.menu_button(tr("condense-export", &[]), |ui| {
        if running {
            ui.label(tr("condense-running", &[]));
            if ui.button(tr("condense-cancel", &[])).clicked() {
                out.push(Command::CondenseCancel);
                ui.close();
            }
        } else {
            for (format, name) in [(Format::Opus, "Opus (.opus)"), (Format::Mp3, "MP3 (.mp3)")] {
                if ui.button(name).clicked() {
                    out.push(Command::Condense(target.clone(), format));
                    ui.close();
                }
            }
            ui.label(tr("condense-hint", &[]));
        }
    });
}
