// SPDX-License-Identifier: GPL-3.0-only

//! The Media servers tab of the settings (SRV-001…SRV-003, SRV-005, SRV-006):
//! the servers added, with their names and libraries, and the logins.

use egui::{Button, Frame, Margin, RichText, TextEdit, Ui};
use nikomiru_core::config::ServerConfig;
use nikomiru_online::servers::Library;

use super::popups::Translate;
use super::settings::{Leave, SettingsTab, page};
use super::{View, palette, theme};
use crate::session::Session;
use crate::ui::Command;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    url: String,
    user: String,
    password: String,
    allow_unencrypted: bool,
    /// The names being edited, by server.
    names: std::collections::HashMap<String, String>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The servers added.
    pub servers: &'a [ServerConfig],
    /// The server whose libraries were asked for, and them.
    pub checked: Option<(&'a str, &'a [Library])>,
    /// A login is going on.
    pub logging_in: bool,
    /// The last message, and whether it is good news.
    pub status: Option<&'a (String, bool)>,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let page = &s.servers_page;
    let scene = Scene {
        servers: s.server_list(),
        checked: page
            .server
            .as_deref()
            .filter(|_| !page.libraries.is_empty())
            .map(|id| (id, page.libraries.as_slice())),
        logging_in: page.logging_in,
        status: page.status.as_ref(),
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.servers, out);
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

fn hint(ui: &mut Ui, text: String) {
    ui.add(egui::Label::new(RichText::new(text).color(palette::current().details)).wrap());
}

pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    let tr_id = |id: &str| tr(s, id);
    page(ctx, SettingsTab::Servers, &tr_id, s.status, |_, ui| {
        body(ui, s, state, out);
    })
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    ui.spacing_mut().item_spacing.y = 8.0;
    section(ui, &tr(s, "servers-added"));
    if s.servers.is_empty() {
        hint(ui, tr(s, "servers-none"));
    }
    for server in s.servers {
        row(ui, s, state, server, out);
    }
    section(ui, &tr(s, "servers-add-jellyfin"));
    egui::Grid::new("server-login")
        .num_columns(2)
        .show(ui, |ui| {
            let label = ui.label(tr(s, "servers-address"));
            ui.add(TextEdit::singleline(&mut state.url).hint_text(tr(s, "servers-address-hint")))
                .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(tr(s, "servers-user"));
            ui.text_edit_singleline(&mut state.user)
                .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(tr(s, "servers-password"));
            ui.add(TextEdit::singleline(&mut state.password).password(true))
                .labelled_by(label.id);
            ui.end_row();
        });
    ui.checkbox(&mut state.allow_unencrypted, tr(s, "servers-unencrypted"));
    if state.allow_unencrypted {
        hint(ui, tr(s, "servers-unencrypted-warning"));
    }
    let ready = !s.logging_in && !state.url.trim().is_empty() && !state.user.trim().is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(ready, Button::new(tr(s, "servers-log-in")))
            .clicked()
        {
            out.push(Command::ServerJellyfin {
                url: state.url.trim().to_string(),
                user: state.user.trim().to_string(),
                // kept nowhere after this (SRV-002)
                password: std::mem::take(&mut state.password),
                allow_unencrypted: state.allow_unencrypted,
            });
        }
    });
    section(ui, &tr(s, "servers-add-plex"));
    hint(ui, tr(s, "servers-plex-hint"));
    ui.horizontal(|ui| {
        if s.logging_in {
            ui.spinner();
            if ui.button(tr(s, "servers-cancel")).clicked() {
                out.push(Command::ServerCancel);
            }
        } else if ui.button(tr(s, "servers-sign-in-plex")).clicked() {
            out.push(Command::ServerPlex);
        }
    });
}

fn row(
    ui: &mut Ui,
    s: &Scene<'_>,
    state: &mut State,
    server: &ServerConfig,
    out: &mut Vec<Command>,
) {
    Frame::new()
        .fill(palette::current().row)
        .corner_radius(6)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let name = state
                    .names
                    .entry(server.id.clone())
                    .or_insert_with(|| server.name.clone());
                let label = ui.label(tr(s, "servers-name"));
                let edit = ui
                    .add(TextEdit::singleline(name).desired_width(200.0))
                    .labelled_by(label.id);
                if edit.lost_focus() && *name != server.name && !name.trim().is_empty() {
                    out.push(Command::ServerRename(
                        server.id.clone(),
                        name.trim().to_string(),
                    ));
                }
                ui.label(RichText::new(&server.kind).color(palette::current().details));
                if ui.button(tr(s, "servers-check")).clicked() {
                    out.push(Command::ServerCheck(server.id.clone()));
                }
                if ui.button(tr(s, "servers-remove")).clicked() {
                    out.push(Command::ServerRemove(server.id.clone()));
                }
            });
            ui.add(
                egui::Label::new(
                    RichText::new(nikomiru_core::config::without_userinfo(&server.url))
                        .color(palette::current().details),
                )
                .truncate(),
            );
            if let Some((_, libraries)) = s.checked.filter(|(id, _)| *id == server.id) {
                for library in libraries {
                    let mut shown =
                        server.libraries.is_empty() || server.libraries.contains(&library.id);
                    if ui.checkbox(&mut shown, &library.title).changed() {
                        let list: Vec<String> = libraries
                            .iter()
                            .filter(|l| {
                                if l.id == library.id {
                                    shown
                                } else {
                                    server.libraries.is_empty() || server.libraries.contains(&l.id)
                                }
                            })
                            .map(|l| l.id.clone())
                            .collect();
                        out.push(Command::ServerShowLibraries(server.id.clone(), list));
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use egui::{Id, Sense, vec2};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    struct Test {
        servers: Vec<ServerConfig>,
        state: State,
        out: Vec<Command>,
    }

    fn harness<'a>(servers: Vec<ServerConfig>, state: State) -> Harness<'a, Test> {
        Harness::builder()
            .with_size(vec2(1000.0, 800.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        servers: &t.servers,
                        checked: None,
                        logging_in: false,
                        status: None,
                        tr: &tr,
                    };
                    draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    servers,
                    state,
                    out: Vec::new(),
                },
            )
    }

    // T-SRV-001, T-SRV-002: the password goes with the login, and stays nowhere
    #[test]
    fn a_jellyfin_is_added_from_the_page() {
        let state = State {
            url: "http://nas:8096".into(),
            user: "kana".into(),
            password: "secret".into(),
            ..State::default()
        };
        let mut h = harness(Vec::new(), state);
        h.get_by_label("servers-log-in").click();
        h.run();
        assert!(matches!(
            h.state().out.as_slice(),
            [Command::ServerJellyfin { password, .. }] if password == "secret"
        ));
        assert!(h.state().state.password.is_empty());
    }

    // T-SRV-001: with none, only the way to add one
    #[test]
    fn none_by_default() {
        let h = harness(Vec::new(), State::default());
        let _ = h.get_by_label("servers-none");
    }

    // T-SRV-001
    #[test]
    fn a_server_is_removed() {
        let server = ServerConfig {
            id: "a".into(),
            kind: "jellyfin".into(),
            name: "Home".into(),
            url: "http://nas".into(),
            ..ServerConfig::default()
        };
        let mut h = harness(vec![server], State::default());
        h.get_by_label("servers-remove").click();
        h.run();
        assert!(matches!(h.state().out.as_slice(), [Command::ServerRemove(id)] if id == "a"));
    }
}
