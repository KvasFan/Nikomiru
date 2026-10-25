// SPDX-License-Identifier: GPL-3.0-only

//! The Subtitle services tab of the settings (MET-011): which services search
//! for subtitles and in which order, their keys and the OpenSubtitles login
//! (MET-005), the automatic subtitles (MET-007) and the proxy (SET-007).

use egui::{Align, Button, Frame, Layout, Margin, RichText, Role, TextEdit, Ui, WidgetInfo, vec2};
use nikomiru_core::config::{NetworkConfig, OnlineConfig};
use nikomiru_online::ProviderId;

use super::popups::Translate;
use super::settings::{Leave, SettingsTab, page};
use super::{View, palette, theme};
use crate::session::Session;
use crate::ui::Command;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<(OnlineConfig, NetworkConfig)>,
    /// Every service in the order shown, and whether it is used.
    order: Vec<(ProviderId, bool)>,
    /// The keys and the password are shown.
    show_keys: bool,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The online settings as saved.
    pub saved: (&'a OnlineConfig, &'a NetworkConfig),
    /// The last message, and whether it is good news.
    pub status: Option<&'a (String, bool)>,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        saved: s.online_config(),
        status: s.services_status.as_ref(),
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.services, out);
    view.leave(leave, out);
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(text)
            .size(15.0)
            .family(theme::bold())
            .color(palette::current().details),
    );
}

/// The services in the order shown: those used, in their order, then the
/// others.
fn order_of(online: &OnlineConfig) -> Vec<(ProviderId, bool)> {
    let used = ProviderId::from_keys(&online.providers);
    let mut order: Vec<(ProviderId, bool)> = used.iter().map(|p| (*p, true)).collect();
    order.extend(
        ProviderId::ALL
            .into_iter()
            .filter(|p| !used.contains(p))
            .map(|p| (p, false)),
    );
    order
}

/// Draws the page; returns what it asks for.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    if state.draft.is_none() {
        state.draft = Some((s.saved.0.clone(), s.saved.1.clone()));
        state.order = order_of(s.saved.0);
    }
    let tr = |id: &str| tr(s, id);
    page(ctx, SettingsTab::Services, &tr, s.status, |buttons, ui| {
        if buttons.button(tr("online-save")).clicked() {
            save(state, out);
        }
        body(ui, s, state);
    })
}

/// Sends the settings, the services used in their order.
fn save(state: &State, out: &mut Vec<Command>) {
    if let Some((mut online, network)) = state.draft.clone() {
        online.providers = state
            .order
            .iter()
            .filter(|(_, used)| *used)
            .map(|(p, _)| p.key().to_string())
            .collect();
        out.push(Command::OnlineSave(Box::new((online, network))));
    }
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State) {
    ui.spacing_mut().item_spacing.y = 8.0;
    section(ui, &tr(s, "services-list"));
    ui.add(
        egui::Label::new(
            RichText::new(tr(s, "services-list-hint")).color(palette::current().details),
        )
        .wrap(),
    );
    let show = state.show_keys;
    let Some((online, network)) = state.draft.as_mut() else {
        return;
    };
    let count = state.order.len();
    let mut swap = None;
    for (i, (id, used)) in state.order.iter_mut().enumerate() {
        Frame::new()
            .fill(palette::current().row)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.checkbox(used, RichText::new(id.name()).family(theme::bold()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let arrow = |ui: &mut Ui, text: &str, label: &str, enabled: bool| {
                            let name = (s.tr)(label, &[("name", id.name().to_string())]);
                            let b = ui.add_enabled(enabled, Button::new(text).small());
                            b.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &name));
                            b.clicked()
                        };
                        if arrow(ui, "▼", "services-down", i + 1 < count) {
                            swap = Some((i, i + 1));
                        }
                        if arrow(ui, "▲", "services-up", i > 0) {
                            swap = Some((i - 1, i));
                        }
                    });
                });
                let about = match id {
                    ProviderId::Jimaku => "services-jimaku-about",
                    ProviderId::OpenSubtitles => "services-os-about",
                };
                ui.add(
                    egui::Label::new(RichText::new(tr(s, about)).color(palette::current().details))
                        .wrap(),
                );
                egui::Grid::new(("service-keys", id.key()))
                    .num_columns(2)
                    .spacing(vec2(12.0, 6.0))
                    .show(ui, |ui| {
                        let field = |ui: &mut Ui, name: &str, value: &mut String, hide| {
                            let label = ui.label(tr(s, name));
                            ui.add(
                                TextEdit::singleline(value)
                                    .password(hide)
                                    .desired_width(320.0),
                            )
                            .labelled_by(label.id);
                            ui.end_row();
                        };
                        match id {
                            ProviderId::Jimaku => {
                                field(ui, "online-jimaku-key", &mut online.jimaku_api_key, !show);
                            }
                            ProviderId::OpenSubtitles => {
                                field(
                                    ui,
                                    "online-os-key",
                                    &mut online.opensubtitles_api_key,
                                    !show,
                                );
                                field(
                                    ui,
                                    "online-os-user",
                                    &mut online.opensubtitles_username,
                                    false,
                                );
                                field(
                                    ui,
                                    "online-os-password",
                                    &mut online.opensubtitles_password,
                                    !show,
                                );
                            }
                        }
                    });
            });
    }
    if let Some((a, b)) = swap {
        state.order.swap(a, b);
    }
    ui.checkbox(&mut state.show_keys, tr(s, "online-show-keys"));
    ui.add(
        egui::Label::new(
            RichText::new(tr(s, "online-keys-hint")).color(palette::current().details),
        )
        .wrap(),
    );
    section(ui, &tr(s, "services-auto-section"));
    ui.checkbox(&mut online.auto_subtitles, tr(s, "online-auto"));
    section(ui, &tr(s, "services-network"));
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "online-proxy"));
        ui.add(TextEdit::singleline(&mut network.proxy).desired_width(320.0))
            .labelled_by(label.id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Id, Sense};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        saved: (OnlineConfig, NetworkConfig),
        state: State,
        out: Vec<Command>,
        leave: Option<Leave>,
    }

    fn harness(online: OnlineConfig) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 1000.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    // the arguments shown, to tell the rows apart
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (_, v) in args {
                            text.push(' ');
                            text.push_str(v);
                        }
                        text
                    };
                    let scene = Scene {
                        saved: (&t.saved.0, &t.saved.1),
                        status: None,
                        tr: &tr,
                    };
                    match draw(ui.ctx(), &scene, &mut t.state, &mut t.out) {
                        Leave::Stay => {}
                        other => t.leave = Some(other),
                    }
                },
                Test {
                    saved: (online, NetworkConfig::default()),
                    state: State::default(),
                    out: Vec::new(),
                    leave: None,
                },
            )
    }

    fn click(h: &mut Harness<'_, Test>, label: &str) -> Vec<Command> {
        h.get_by_label(label).click();
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    fn saved(out: &[Command]) -> OnlineConfig {
        match out {
            [Command::OnlineSave(settings)] => settings.0.clone(),
            other => panic!("{other:?}"),
        }
    }

    // T-MET-011, T-SET-008
    #[test]
    fn the_services_are_chosen_ordered_and_saved() {
        let mut h = harness(OnlineConfig::default());
        h.run();
        // both used, Jimaku first, as by default
        let out = click(&mut h, "online-save");
        assert_eq!(saved(&out).providers, ["jimaku", "opensubtitles"]);
        // OpenSubtitles first, then Jimaku left out
        h.get_by_label("services-up OpenSubtitles").click();
        h.run();
        h.get_by_label("Jimaku").click();
        h.run();
        h.get_by_label("online-auto").click();
        h.run();
        let out = click(&mut h, "online-save");
        let online = saved(&out);
        assert_eq!(online.providers, ["opensubtitles"]);
        assert!(online.auto_subtitles);
        // the first cannot go up
        h.get_by_label("services-up OpenSubtitles").click();
        h.run();
        assert_eq!(
            h.state().state.order,
            [
                (ProviderId::OpenSubtitles, true),
                (ProviderId::Jimaku, false)
            ]
        );
        // the keys of each service are on its row
        h.get_by_label("online-jimaku-key");
        h.get_by_label("online-os-password");
        // another tab
        h.get_by_label("settings-player").click();
        h.run();
        assert_eq!(h.state().leave, Some(Leave::To(SettingsTab::Player)));
    }

    // T-MET-011: a service left out stays below the others
    #[test]
    fn a_service_left_out_is_listed_last() {
        let online = OnlineConfig {
            providers: vec!["opensubtitles".into()],
            ..OnlineConfig::default()
        };
        assert_eq!(
            order_of(&online),
            [
                (ProviderId::OpenSubtitles, true),
                (ProviderId::Jimaku, false)
            ]
        );
        let none = OnlineConfig {
            providers: Vec::new(),
            ..OnlineConfig::default()
        };
        assert!(order_of(&none).iter().all(|(_, used)| !used));
    }
}
