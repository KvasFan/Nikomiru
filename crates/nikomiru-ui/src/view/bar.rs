// SPDX-License-Identifier: GPL-3.0-only

//! The control bar (PLY-001, PLY-003, PLY-004): progress, play/pause, time,
//! volume, speed, audio track, the subtitle menu, the dictionaries page, open
//! and fullscreen. It fades with the controls; its buttons never take the
//! keyboard, so Space stays play/pause.

use egui::{
    Align2, Area, Button, Color32, ComboBox, Frame, Id, Margin, Order, RichText, Role, Slider,
    WidgetInfo, pos2,
};

use super::{Dialog, View};
use crate::session::Session;
use crate::shortcuts::Action;
use crate::subtitles::clock;
use crate::ui::Command;

/// The subtitle encodings that can be forced (SUB-005); "" is automatic.
const ENCODINGS: [&str; 7] = ["", "UTF-8", "CP932", "EUC-JP", "GB18030", "BIG5", "CP1252"];

/// What the bar keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The bar's height, for the subtitles above it.
    pub height: f32,
    /// The position while the progress slider is dragged: the slider does not
    /// follow mpv meanwhile.
    seek: Option<f64>,
    /// The volume while its slider is dragged.
    volume: Option<f64>,
}

fn named(response: &egui::Response, enabled: bool, name: &str) {
    let name = name.to_string();
    response.widget_info(move || WidgetInfo::labeled(Role::Button, enabled, &name));
}

/// A button with a glyph, a tooltip and an accessible name (NFR-A11Y-01).
fn icon(ui: &mut egui::Ui, enabled: bool, glyph: &str, name: &str) -> bool {
    let response = ui
        .add_enabled(
            enabled,
            Button::new(RichText::new(glyph).size(16.0)).min_size(egui::vec2(34.0, 30.0)),
        )
        .on_hover_text(name);
    named(&response, enabled, name);
    response.clicked()
}

fn track_label(s: &Session, t: &nikomiru_player::player::Track) -> String {
    let mut label = t
        .lang
        .clone()
        .unwrap_or_else(|| s.translator().tr("track-unknown-language", &[]));
    if let Some(title) = &t.title {
        label.push_str(" — ");
        label.push_str(title);
    }
    if let Some(n) = t.channels {
        label.push_str(&format!(" ({n} ch)"));
    }
    label
}

/// The label of an entry of the subtitle menu (`Session::subtitle_menu`).
fn source_label(source: &serde_json::Value) -> String {
    let text = |k: &str| source.get(k).and_then(|v| v.as_str()).unwrap_or("");
    if text("kind") == "file" {
        return text("name").to_string();
    }
    let codec = text("codec");
    let parts: Vec<String> = [
        text("name").to_string(),
        text("lang").to_string(),
        text("title").to_string(),
        if codec.is_empty() {
            String::new()
        } else {
            format!("({codec})")
        },
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();
    let label = parts.join(" · ");
    if text("kind") == "bitmap" {
        format!("🖼 {label}")
    } else {
        label
    }
}

fn subtitle_menu(ui: &mut egui::Ui, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str| s.translator().tr(id, &[]);
    let current = s.subs.source.as_ref().map(|x| x.key()).unwrap_or_default();
    if ui
        .selectable_label(current.is_empty(), tr("subtitles-off"))
        .clicked()
    {
        out.push(Command::SelectSubtitles("off".into()));
        ui.close();
    }
    if let serde_json::Value::Array(sources) = s.subtitle_menu() {
        for source in &sources {
            let key = source.get("key").and_then(|k| k.as_str()).unwrap_or("");
            if ui
                .selectable_label(key == current, source_label(source))
                .clicked()
            {
                out.push(Command::SelectSubtitles(key.to_string()));
                ui.close();
            }
        }
    }
    ui.separator();
    if ui.button(tr("subtitle-open-file")).clicked() {
        view.dialog = Some(Dialog::Subtitles);
        ui.close();
    }
    // MET-009
    if ui.button(tr("subtitle-search-online")).clicked() {
        view.open_online_page(out);
        ui.close();
    }
    // SUB-018
    if ui
        .add_enabled(!current.is_empty(), egui::Button::new(tr("subtitle-align")))
        .clicked()
    {
        out.push(Command::AlignSubtitles);
        ui.close();
    }
    if ui
        .selectable_label(s.subs.disabled, format!("{}  (V)", tr("subtitles-disable")))
        .clicked()
    {
        out.push(Command::Action(Action::SubToggle));
        ui.close();
    }
    let detected = s
        .subs
        .encoding
        .as_deref()
        .map(|e| s.encoding_label(e))
        .unwrap_or_default();
    let title = if detected.is_empty() {
        tr("subtitle-encoding")
    } else {
        format!("{}: {detected}", tr("subtitle-encoding"))
    };
    let forced = s.subs.forced_encoding.clone().unwrap_or_default();
    ui.menu_button(title, |ui| {
        for e in ENCODINGS {
            let label = if e.is_empty() {
                if forced.is_empty() && !detected.is_empty() {
                    format!("{} — {detected}", tr("encoding-auto"))
                } else {
                    tr("encoding-auto")
                }
            } else {
                s.encoding_label(e)
            };
            if ui.selectable_label(forced == e, label).clicked() {
                out.push(Command::ForceEncoding(e.to_string()));
                ui.close();
            }
        }
    });
    if ui.button(tr("subtitle-list")).clicked() {
        view.sub_list_open = !view.sub_list_open;
        ui.close();
    }
}

pub(crate) fn show(
    ctx: &egui::Context,
    s: &Session,
    view: &mut View,
    opacity: f32,
    out: &mut Vec<Command>,
) {
    let screen = ctx.content_rect();
    let tr = |id: &str| s.translator().tr(id, &[]);
    let v = &s.view;
    let response = Area::new(Id::unique("bar"))
        .order(Order::Middle)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(pos2(screen.left(), screen.bottom()))
        .show(ctx, |ui| {
            ui.multiply_opacity(opacity);
            Frame::new()
                .fill(Color32::from_rgba_unmultiplied(0x14, 0x14, 0x18, 0xd9))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(screen.width() - 20.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.visuals_mut().override_text_color = Some(Color32::WHITE);

                    // progress: does not fight mpv while dragged
                    let duration = v.duration.max(0.001);
                    let mut position = view.bar.seek.unwrap_or(v.time_pos);
                    ui.spacing_mut().slider_width = ui.available_width();
                    let slider = ui.add_enabled(
                        v.loaded,
                        Slider::new(&mut position, 0.0..=duration).show_value(false),
                    );
                    slider.widget_info(|| {
                        WidgetInfo::labeled(Role::Slider, v.loaded, tr("position"))
                    });
                    if slider.changed() {
                        view.bar.seek = Some(position);
                        out.push(Command::SeekTo(position));
                    }
                    if !slider.dragged() {
                        view.bar.seek = None;
                    }

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let (glyph, name) = if v.paused {
                            ("▶", tr("play"))
                        } else {
                            ("⏸", tr("pause"))
                        };
                        if icon(ui, v.loaded, glyph, &name) {
                            out.push(Command::Action(Action::PlayPause));
                        }
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                clock((v.time_pos * 1000.0) as i64),
                                clock((v.duration * 1000.0) as i64)
                            ))
                            .monospace(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (glyph, name) = if view.fullscreen {
                                ("🗗", tr("exit-fullscreen"))
                            } else {
                                ("⛶", tr("fullscreen"))
                            };
                            if icon(ui, true, glyph, &name) {
                                view.effect(crate::session::UiEffect::ToggleFullscreen);
                            }
                            if icon(ui, true, "📂", &tr("open-file")) {
                                view.dialog = Some(Dialog::Video);
                            }
                            if icon(ui, true, "📚", &tr("dict-page-title")) {
                                view.open_dict_page(out);
                            }
                            if icon(ui, true, "Anki", &tr("anki-open")) {
                                view.open_anki_page(out);
                            }
                            // PLY-016, PLY-018
                            if icon(ui, true, "⚙", &tr("player-page-title")) {
                                view.open_player_page(out);
                            }
                            let menu = ui.menu_button(RichText::new("字").size(16.0), |ui| {
                                subtitle_menu(ui, s, view, out);
                            });
                            menu.response.clone().on_hover_text(tr("subtitles"));
                            named(&menu.response, true, &tr("subtitles"));
                            let audio: Vec<_> = v.audio_tracks.iter().collect();
                            if audio.len() > 1 {
                                let selected = audio
                                    .iter()
                                    .find(|t| Some(t.id) == v.aid)
                                    .map(|t| track_label(s, t))
                                    .unwrap_or_default();
                                let combo = ComboBox::from_id_salt("audio")
                                    .width(200.0)
                                    .selected_text(selected)
                                    .show_ui(ui, |ui| {
                                        for t in &audio {
                                            if ui
                                                .selectable_label(
                                                    Some(t.id) == v.aid,
                                                    track_label(s, t),
                                                )
                                                .clicked()
                                            {
                                                out.push(Command::SelectAudio(t.id));
                                            }
                                        }
                                    });
                                named(&combo.response, true, &tr("audio-track"));
                            }
                            if icon(ui, true, "+", &tr("speed-up")) {
                                out.push(Command::Action(Action::SpeedUp));
                            }
                            ui.label(RichText::new(format!("{:.2}×", v.speed)).monospace());
                            if icon(ui, true, "−", &tr("speed-down")) {
                                out.push(Command::Action(Action::SpeedDown));
                            }
                            let mut volume = view.bar.volume.unwrap_or(v.volume);
                            ui.spacing_mut().slider_width = 120.0;
                            let slider = ui.add(
                                Slider::new(&mut volume, 0.0..=150.0)
                                    .step_by(5.0)
                                    .show_value(false),
                            );
                            slider.widget_info(|| {
                                WidgetInfo::labeled(Role::Slider, true, tr("volume"))
                            });
                            if slider.changed() {
                                view.bar.volume = Some(volume);
                                out.push(Command::SetVolume(volume));
                            }
                            if !slider.dragged() {
                                view.bar.volume = None;
                            }
                            let (glyph, name) = if v.muted {
                                ("🔇", tr("unmute"))
                            } else {
                                ("🔊", tr("mute"))
                            };
                            if icon(ui, true, glyph, &name) {
                                out.push(Command::Action(Action::Mute));
                            }
                        });
                    });
                });
        });
    view.bar.height = response.response.rect.height();
}
