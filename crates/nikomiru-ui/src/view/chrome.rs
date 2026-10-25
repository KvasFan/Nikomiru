// SPDX-License-Identifier: GPL-3.0-only

//! What sits over the video besides the controls: the drop hint (LIB-001), the
//! error box (PLY-012, ERR-001) and the toast.

use egui::{Align2, Area, Color32, CornerRadius, Frame, Id, Margin, Order, RichText, pos2};

use super::View;
use super::palette;
use crate::session::Session;
use crate::ui::Command;

/// How long a toast stays, and how long it fades in and out.
const TOAST_SECONDS: f64 = 1.5;
const FADE_SECONDS: f64 = 0.2;

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View) {
    let screen = ctx.content_rect();
    let error = s.view.error.as_deref().unwrap_or("");
    if !error.is_empty() {
        Area::new(Id::unique("error"))
            .order(Order::Middle)
            .interactable(false)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen.center())
            .show(ctx, |ui| {
                palette::on_video(ui);
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(0x3a, 0x1a, 0x1a, 0xcc))
                    .corner_radius(8)
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_max_width((screen.width() - 96.0).max(120.0));
                        ui.label(RichText::new(error).size(16.0).color(Color32::WHITE));
                    });
            });
    } else if !s.view.loaded && !view.library_page_open {
        Area::new(Id::unique("drop-hint"))
            .order(Order::Background)
            .interactable(false)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen.center())
            .show(ctx, |ui| {
                palette::on_video(ui);
                ui.label(
                    RichText::new(s.translator().tr("drop-hint", &[]))
                        .size(20.0)
                        .color(Color32::from_gray(0xbb)),
                );
            });
    }

    let now = ctx.input(|i| i.time);
    let Some((text, since)) = &view.toast else {
        return;
    };
    let age = now - since;
    if age > TOAST_SECONDS + FADE_SECONDS {
        view.toast = None;
        return;
    }
    let opacity = (age / FADE_SECONDS)
        .min((TOAST_SECONDS + FADE_SECONDS - age) / FADE_SECONDS)
        .clamp(0.0, 1.0) as f32;
    Area::new(Id::unique("toast"))
        .order(Order::Tooltip)
        .interactable(false)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(pos2(screen.center().x, screen.top() + 32.0))
        .show(ctx, |ui| {
            palette::on_video(ui);
            ui.multiply_opacity(opacity);
            Frame::new()
                .fill(Color32::from_black_alpha(0xcc))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(text.as_str())
                            .size(18.0)
                            .color(Color32::WHITE),
                    );
                });
        });
    ctx.request_repaint();
}

/// LIB-009: the next episode about to start, with "Play now" and "Cancel"
/// (Esc cancels too), over the bottom right of the video.
pub(crate) fn next_up(ctx: &egui::Context, s: &Session, bar_height: f32, out: &mut Vec<Command>) {
    let Some((title, seconds)) = s.next_up() else {
        return;
    };
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let screen = ctx.content_rect();
    Area::new(Id::unique("next-up"))
        .order(Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(
            screen.right() - 24.0,
            screen.bottom() - bar_height - 24.0,
        ))
        .show(ctx, |ui| {
            palette::on_video(ui);
            Frame::new()
                .fill(Color32::from_black_alpha(0xdd))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                    ui.label(
                        RichText::new(tr("next-up", &[("seconds", seconds.to_string())]))
                            .size(15.0)
                            .color(Color32::from_gray(0xbb)),
                    );
                    ui.label(RichText::new(title).size(18.0));
                    ui.horizontal(|ui| {
                        if ui.button(tr("next-up-play", &[])).clicked() {
                            out.push(Command::NextNow);
                        }
                        if ui.button(tr("next-up-cancel", &[])).clicked() {
                            out.push(Command::NextCancel);
                        }
                    });
                });
        });
    // the seconds go down by themselves
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}
