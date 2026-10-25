// SPDX-License-Identifier: GPL-3.0-only

//! The video of a link to a scene cannot be found (ANK-052): a dialog offers
//! to locate it.

use egui::{Align, Color32, Frame, Layout, Margin, Rect, RichText, Stroke, UiBuilder, vec2};

use super::dict_page::full_screen;
use super::{palette, theme};
use crate::session::{MissingLink, Session};
use crate::ui::Command;

pub(super) fn show(
    ctx: &egui::Context,
    s: &Session,
    missing: &MissingLink,
    out: &mut Vec<Command>,
) {
    let tr = |id: &str| s.translator().tr(id, &[]);
    let screen = ctx.content_rect();
    full_screen(ctx, "link-missing", Color32::from_black_alpha(0x60), |ui| {
        let size = vec2(460.0_f32.min(screen.width() - 64.0).max(0.0), 0.0);
        let rect = Rect::from_center_size(screen.center(), size).expand2(vec2(0.0, 70.0));
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::top_down(Align::Center)),
        );
        Frame::new()
            .fill(palette::current().dialog)
            .stroke(Stroke::new(1.0, palette::current().card_border))
            .corner_radius(8)
            .inner_margin(Margin::same(16))
            .show(&mut ui, |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                ui.visuals_mut().override_text_color = Some(palette::current().text);
                ui.add(
                    egui::Label::new(RichText::new(tr("link-missing")).family(theme::bold()))
                        .wrap(),
                );
                let shown = missing
                    .path
                    .clone()
                    .unwrap_or_else(|| missing.link.uid.clone());
                ui.add(egui::Label::new(shown).wrap());
                ui.horizontal(|ui| {
                    if ui.button(tr("link-locate")).clicked() {
                        out.push(Command::LinkLocate);
                    }
                    if ui.button(tr("dict-cancel")).clicked() {
                        out.push(Command::LinkDismiss);
                    }
                });
            });
    });
}
