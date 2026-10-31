// SPDX-License-Identifier: GPL-3.0-only

//! The Backup tab of the settings (BAK-001…BAK-004, BAK-009): a backup to a
//! file, its restore, and the WebDAV folder.

use egui::{Button, RichText, TextEdit, Ui};
use nikomiru_core::config::SyncConfig;

use super::popups::Translate;
use super::settings::{Leave, SettingsTab, page};
use super::{View, palette, theme};
use crate::session::Session;
use crate::sync::BackupPage;
use crate::ui::Command;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<SyncConfig>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The settings as saved.
    pub saved: &'a SyncConfig,
    /// The state of the page.
    pub page: &'a BackupPage,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        saved: s.sync_config(),
        page: &s.backup_page,
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.backup, out);
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
    if state.draft.is_none() {
        state.draft = Some(s.saved.clone());
    }
    let tr_id = |id: &str| tr(s, id);
    page(
        ctx,
        SettingsTab::Backup,
        &tr_id,
        s.page.status.as_ref(),
        |buttons, ui| {
            if buttons.button(tr(s, "online-save")).clicked()
                && let Some(draft) = &state.draft
            {
                out.push(Command::SyncSave(Box::new(draft.clone())));
            }
            body(ui, s, state, out);
        },
    )
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    ui.spacing_mut().item_spacing.y = 8.0;
    let busy = s.page.busy;
    section(ui, &tr(s, "backup-file"));
    hint(ui, tr(s, "backup-file-hint"));
    let mut options = s.page.options;
    let dictionaries = ui.checkbox(&mut options.dictionaries, tr(s, "backup-dictionaries"));
    let secrets = ui.checkbox(&mut options.secrets, tr(s, "backup-secrets"));
    if dictionaries.changed() || secrets.changed() {
        out.push(Command::BackupOptions(options));
    }
    if options.secrets {
        hint(ui, tr(s, "backup-secrets-warning"));
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, Button::new(tr(s, "backup-make")))
            .clicked()
        {
            out.push(Command::BackupMake);
        }
        if ui
            .add_enabled(!busy, Button::new(tr(s, "backup-restore")))
            .clicked()
        {
            out.push(Command::BackupChoose);
        }
        if busy {
            ui.spinner();
        }
    });
    if let Some((_, manifest)) = &s.page.chosen {
        let args = [
            ("version", manifest.version.clone()),
            ("date", crate::sync::date_of(manifest.created_at)),
        ];
        hint(ui, (s.tr)("backup-restore-confirm", &args));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, Button::new(tr(s, "backup-restore-yes")))
                .clicked()
            {
                out.push(Command::BackupRestore);
            }
            if ui.button(tr(s, "servers-cancel")).clicked() {
                out.push(Command::BackupCancel);
            }
        });
    }
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    section(ui, &tr(s, "sync-title"));
    hint(ui, tr(s, "sync-hint"));
    egui::Grid::new("sync-login").num_columns(2).show(ui, |ui| {
        let label = ui.label(tr(s, "servers-address"));
        ui.text_edit_singleline(&mut draft.url)
            .labelled_by(label.id);
        ui.end_row();
        let label = ui.label(tr(s, "servers-user"));
        ui.text_edit_singleline(&mut draft.user)
            .labelled_by(label.id);
        ui.end_row();
        let label = ui.label(tr(s, "servers-password"));
        ui.add(TextEdit::singleline(&mut draft.password).password(true))
            .labelled_by(label.id);
        ui.end_row();
    });
    ui.checkbox(&mut draft.allow_unencrypted, tr(s, "servers-unencrypted"));
    if draft.allow_unencrypted {
        hint(ui, tr(s, "servers-unencrypted-warning"));
    }
    ui.checkbox(&mut draft.automatic, tr(s, "sync-automatic"));
    let ready = !busy && !s.saved.url.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(ready, Button::new(tr(s, "servers-check")))
            .clicked()
        {
            out.push(Command::SyncCheck);
        }
        if ui
            .add_enabled(ready, Button::new(tr(s, "sync-now")))
            .clicked()
        {
            out.push(Command::SyncNow);
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
        saved: SyncConfig,
        page: BackupPage,
        state: State,
        out: Vec<Command>,
    }

    fn harness<'a>(saved: SyncConfig) -> Harness<'a, Test> {
        Harness::builder()
            .with_size(vec2(1000.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        saved: &t.saved,
                        page: &t.page,
                        tr: &tr,
                    };
                    draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    saved,
                    page: BackupPage::default(),
                    state: State::default(),
                    out: Vec::new(),
                },
            )
    }

    // T-BAK-004: nothing to sync with until it is set up
    #[test]
    fn without_a_folder_there_is_no_sync() {
        let mut h = harness(SyncConfig::default());
        h.run();
        h.get_by_label("sync-now").click();
        h.run();
        assert!(h.state().out.is_empty());
    }

    // T-BAK-001, T-BAK-003
    #[test]
    fn the_buttons_ask_for_a_file() {
        let mut h = harness(SyncConfig::default());
        h.run();
        h.get_by_label("backup-make").click();
        h.run();
        assert!(matches!(h.state().out.as_slice(), [Command::BackupMake]));
        h.state_mut().out.clear();
        h.get_by_label("backup-restore").click();
        h.run();
        assert!(matches!(h.state().out.as_slice(), [Command::BackupChoose]));
    }

    // T-BAK-002: keys go only when ticked, and the page says what that means
    #[test]
    fn the_secrets_are_a_choice() {
        let mut h = harness(SyncConfig::default());
        h.run();
        h.get_by_label("backup-secrets").click();
        h.run();
        assert!(matches!(
            h.state().out.as_slice(),
            [Command::BackupOptions(o)] if o.secrets && !o.dictionaries
        ));
    }

    // T-BAK-004
    #[test]
    fn the_settings_are_saved() {
        let saved = SyncConfig {
            url: "http://nas/dav".into(),
            ..SyncConfig::default()
        };
        let mut h = harness(saved);
        h.run();
        h.get_by_label("sync-automatic").click();
        h.run();
        h.get_by_label("online-save").click();
        h.run();
        assert!(matches!(
            h.state().out.as_slice(),
            [Command::SyncSave(c)] if c.automatic && c.url == "http://nas/dav"
        ));
    }
}
