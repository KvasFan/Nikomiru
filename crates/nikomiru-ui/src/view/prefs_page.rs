// SPDX-License-Identifier: GPL-3.0-only

//! The tabs of the settings that edit `config.toml` alone (SET-001): General,
//! Subtitles, Lookup, Shortcuts and Advanced, and About. Each edits a copy of
//! the settings; Save applies and writes the keys of its section (SET-002),
//! Reset puts them back to their defaults in the copy (SET-006).

use std::path::{Path, PathBuf};

use egui::{ComboBox, Grid, Response, RichText, Slider, TextEdit, Ui, vec2};
use nikomiru_core::Config;
use nikomiru_core::config::General;

use super::popups::Translate;
use super::settings::{self, Leave, SettingsTab};
use super::{View, palette, shortcuts_page, theme};
use crate::session::Session;
use crate::ui::Command;

/// The repository, shown in About.
const REPOSITORY: &str = "https://github.com/KvasFan/nikomiru";
/// The text of the license.
const LICENSE: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

/// What the tabs keep between frames; dropped when the settings close.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited, and the tab they are for.
    draft: Option<(SettingsTab, Config)>,
    /// The preferred languages as typed.
    languages: String,
    /// The Shortcuts tab.
    pub shortcuts: shortcuts_page::State,
}

/// What the tabs show.
pub(crate) struct Scene<'a> {
    pub tab: SettingsTab,
    /// The settings as saved.
    pub saved: &'a Config,
    /// The last message, and whether it is good news.
    pub status: Option<&'a (String, bool)>,
    /// `config.toml`.
    pub config_file: &'a Path,
    /// The logs' folder.
    pub logs_dir: &'a Path,
    /// `THIRD_PARTY_LICENSES.html`, where the package put it.
    pub licenses: Option<PathBuf>,
    /// Whether the session waits for a shortcut's key, and the chord it got.
    pub capture: (bool, Option<&'a str>),
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let Some(tab) = view.prefs_open else {
        return;
    };
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        tab,
        saved: s.config(),
        status: s.settings_status.as_ref(),
        config_file: s.config_file(),
        logs_dir: s.logs_dir(),
        licenses: third_party_licenses(),
        capture: s.capture(),
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.prefs, out);
    view.leave(leave, out);
}

/// Where the packages put `THIRD_PARTY_LICENSES.html`: the system's
/// licenses (Arch), the Flatpak's, or next to the program (a build).
fn third_party_licenses() -> Option<PathBuf> {
    const NAME: &str = "THIRD_PARTY_LICENSES.html";
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(NAME)));
    [
        Some(PathBuf::from("/usr/share/licenses/nikomiru").join(NAME)),
        Some(PathBuf::from("/app/share/licenses/nikomiru").join(NAME)),
        beside,
    ]
    .into_iter()
    .flatten()
    .find(|p| p.is_file())
}

/// Draws the tab; returns what it asks for.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    if state.draft.as_ref().is_none_or(|(tab, _)| *tab != s.tab) {
        state.draft = Some((s.tab, s.saved.clone()));
        state.languages.clear();
    }
    let tr = |id: &str| (s.tr)(id, &[]);
    let leave = settings::page(ctx, s.tab, &tr, s.status, |buttons, ui| {
        let Some((_, draft)) = state.draft.as_mut() else {
            return;
        };
        if let Some(section) = s.tab.section() {
            if buttons.button(tr("settings-save")).clicked() {
                out.push(Command::SettingsSave(section, Box::new(draft.clone())));
            }
            let reset = buttons
                .button(tr("settings-reset"))
                .on_hover_text(tr("settings-reset-hint"));
            if reset.clicked() {
                draft.reset(section);
                state.languages.clear();
                state.shortcuts.reset();
            }
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        match s.tab {
            SettingsTab::General => general(ui, s, draft),
            SettingsTab::Subtitles => subtitles(ui, s, draft, &mut state.languages),
            SettingsTab::Lookup => lookup(ui, s, draft),
            SettingsTab::Shortcuts => {
                let shortcuts = &mut state.shortcuts;
                shortcuts_page::body(ui, s.tr, &mut draft.shortcuts, shortcuts, s.capture, out);
            }
            SettingsTab::Advanced => advanced(ui, s, draft),
            SettingsTab::About => about(ui, s, out),
            _ => {}
        }
    });
    // a key awaited by a tab that closes would be lost
    if leave != Leave::Stay && state.shortcuts.capturing() {
        out.push(Command::ShortcutCapture(false));
    }
    leave
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

fn hint(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(palette::current().details)).wrap());
}

/// A row of a form: the label, then the control, named by the label for
/// the screen readers.
fn row(ui: &mut Ui, label: &str, control: impl FnOnce(&mut Ui) -> Response) {
    let label = ui.label(label);
    control(ui).labelled_by(label.id);
    ui.end_row();
}

/// A choice among `options` (the value written, its name).
fn choice(ui: &mut Ui, id: &str, value: &mut String, options: &[(&str, String)]) -> Response {
    let shown = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or_else(|| value.clone(), |(_, name)| name.clone());
    let mut combo = ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(240.0)
        .show_ui(ui, |ui| {
            for (v, name) in options {
                ui.selectable_value(value, (*v).to_string(), name);
            }
        });
    if combo.inner.is_some() {
        combo.response.mark_changed();
    }
    combo.response
}

fn form(ui: &mut Ui, id: &str, rows: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing(vec2(16.0, 10.0))
        .min_col_width(200.0)
        .show(ui, rows);
}

fn general(ui: &mut Ui, s: &Scene<'_>, draft: &mut Config) {
    let tr = |id: &str| (s.tr)(id, &[]);
    section(ui, &tr("settings-interface"));
    form(ui, "settings-general", |ui| {
        let g = &mut draft.general;
        row(ui, &tr("settings-language"), |ui| {
            choice(
                ui,
                "settings-language",
                &mut g.ui_language,
                &[
                    ("system", tr("settings-language-system")),
                    ("en", "English".into()),
                    ("it", "Italiano".into()),
                ],
            )
        });
        row(ui, &tr("settings-theme"), |ui| {
            choice(
                ui,
                "settings-theme",
                &mut g.theme,
                &[
                    ("system", tr("settings-theme-system")),
                    ("dark", tr("settings-theme-dark")),
                    ("light", tr("settings-theme-light")),
                ],
            )
        });
        row(ui, &tr("settings-accent"), |ui| {
            let mut rgb = g.accent_rgb().unwrap_or([0x6a, 0xa0, 0xff]);
            let response = ui.color_edit_button_srgb(&mut rgb);
            if response.changed() {
                g.accent = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            }
            response
        });
        row(ui, &tr("settings-scale"), |ui| {
            ui.add(
                Slider::new(&mut g.ui_scale, General::SCALES)
                    .step_by(5.0)
                    .suffix(" %"),
            )
        });
    });
    section(ui, &tr("settings-playback"));
    form(ui, "settings-playback", |ui| {
        row(ui, &tr("settings-seek"), |ui| {
            ui.add(Slider::new(&mut draft.player.seek_seconds, 1..=30).suffix(" s"))
        });
    });
    ui.checkbox(&mut draft.player.auto_next, tr("settings-auto-next"));
}

fn subtitles(ui: &mut Ui, s: &Scene<'_>, draft: &mut Config, languages: &mut String) {
    let tr = |id: &str| (s.tr)(id, &[]);
    let modes = [
        ("none", tr("subtitle-secondary-shown")),
        ("blur", tr("subtitle-secondary-blur")),
        ("hidden", tr("subtitle-secondary-hidden")),
    ];
    let subs = &mut draft.subtitles;
    form(ui, "settings-subtitles", |ui| {
        row(ui, &tr("settings-sub-size"), |ui| {
            ui.add(Slider::new(&mut subs.font_size, 8.0..=200.0).step_by(1.0))
        });
        row(ui, &tr("settings-sub-languages"), |ui| {
            if languages.is_empty() {
                *languages = subs.preferred_languages.join(", ");
            }
            let response = ui.add(TextEdit::singleline(languages).desired_width(240.0));
            if response.changed() {
                subs.preferred_languages = languages
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            response
        });
        row(ui, &tr("settings-sub-obscure"), |ui| {
            choice(ui, "settings-sub-obscure", &mut subs.obscure, &modes)
        });
        row(ui, &tr("settings-sub-secondary-obscure"), |ui| {
            choice(
                ui,
                "settings-sub-secondary-obscure",
                &mut subs.secondary_obscure,
                &modes,
            )
        });
        row(ui, &tr("settings-sub-secondary-position"), |ui| {
            choice(
                ui,
                "settings-sub-secondary-position",
                &mut subs.secondary_position,
                &[
                    ("top", tr("settings-position-top")),
                    ("bottom", tr("settings-position-bottom")),
                ],
            )
        });
    });
    hint(ui, &tr("settings-sub-hint"));
    ui.checkbox(&mut subs.respect_ass_styles, tr("settings-sub-ass"));
    // the words of the subtitles (LKP-051, LKP-061)
    ui.checkbox(&mut subs.furigana, tr("settings-sub-furigana"));
    ui.checkbox(&mut subs.mark_mined, tr("settings-sub-mark-mined"));
    hint(ui, &tr("settings-sub-words-hint"));
}

fn lookup(ui: &mut Ui, s: &Scene<'_>, draft: &mut Config) {
    let tr = |id: &str| (s.tr)(id, &[]);
    form(ui, "settings-lookup", |ui| {
        row(ui, &tr("settings-lookup-results"), |ui| {
            ui.add(Slider::new(&mut draft.lookup.max_results, 1..=50))
        });
        let (widths, heights) = (
            nikomiru_core::config::PopupConfig::WIDTHS,
            nikomiru_core::config::PopupConfig::HEIGHTS,
        );
        row(ui, &tr("settings-popup-width"), |ui| {
            ui.add(
                Slider::new(&mut draft.popup.max_width, widths)
                    .step_by(10.0)
                    .logarithmic(true)
                    .suffix(" pt"),
            )
        });
        row(ui, &tr("settings-popup-height"), |ui| {
            ui.add(
                Slider::new(&mut draft.popup.max_height, heights)
                    .step_by(10.0)
                    .logarithmic(true)
                    .suffix(" pt"),
            )
        });
    });
    ui.checkbox(
        &mut draft.lookup.hover_without_modifier,
        tr("settings-lookup-hover"),
    );
    ui.checkbox(
        &mut draft.lookup.resume_on_leave,
        tr("settings-lookup-resume"),
    );
}

/// A path the user can select and copy.
fn path_row(ui: &mut Ui, label: &str, path: &Path, copy: &str) {
    let label = ui.label(label);
    ui.horizontal(|ui| {
        let text = path.display().to_string();
        ui.add(egui::Label::new(RichText::new(&text).monospace()).selectable(true))
            .labelled_by(label.id);
        if ui.small_button(copy).clicked() {
            ui.ctx().copy_text(text);
        }
    });
    ui.end_row();
}

fn advanced(ui: &mut Ui, s: &Scene<'_>, draft: &mut Config) {
    let tr = |id: &str| (s.tr)(id, &[]);
    form(ui, "settings-advanced", |ui| {
        row(ui, &tr("settings-ffmpeg"), |ui| {
            ui.add(
                TextEdit::singleline(&mut draft.paths.ffmpeg)
                    .hint_text(tr("settings-ffmpeg-hint"))
                    .desired_width(360.0),
            )
        });
    });
    hint(ui, &tr("settings-next-start"));
    section(ui, &tr("settings-folders"));
    form(ui, "settings-folders", |ui| {
        let copy = tr("settings-copy");
        path_row(ui, &tr("settings-config-file"), s.config_file, &copy);
        path_row(ui, &tr("settings-logs"), s.logs_dir, &copy);
    });
    hint(ui, &tr("settings-mpv-hint"));
}

fn about(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let tr = |id: &str| (s.tr)(id, &[]);
    ui.label(
        RichText::new(format!("Nikomiru {}", env!("CARGO_PKG_VERSION")))
            .size(22.0)
            .family(theme::bold()),
    );
    hint(ui, &tr("about-tagline"));
    ui.horizontal(|ui| {
        ui.label(tr("about-license"));
        if ui.link("GPL-3.0-only").clicked() {
            out.push(Command::OpenExternal(LICENSE.into()));
        }
    });
    if ui.link(REPOSITORY).clicked() {
        out.push(Command::OpenExternal(REPOSITORY.into()));
    }
    section(ui, &tr("about-credits"));
    hint(ui, &tr("about-derived"));
    section(ui, &tr("about-third-party"));
    match &s.licenses {
        Some(path) => {
            form(ui, "about-licenses", |ui| {
                path_row(
                    ui,
                    &tr("about-third-party-file"),
                    path,
                    &tr("settings-copy"),
                );
            });
        }
        None => hint(ui, &tr("about-third-party-missing")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Id;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_core::config::Section;

    struct Test {
        tab: SettingsTab,
        saved: Config,
        state: State,
        out: Vec<Command>,
        leave: Leave,
    }

    fn harness(tab: SettingsTab, saved: Config) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), egui::Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        tab: t.tab,
                        saved: &t.saved,
                        status: None,
                        config_file: Path::new("/home/u/.config/nikomiru/config.toml"),
                        logs_dir: Path::new("/home/u/.local/state/nikomiru/logs"),
                        licenses: None,
                        capture: (false, None),
                        tr: &tr,
                    };
                    let leave = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                    if leave != Leave::Stay {
                        t.leave = leave;
                    }
                },
                Test {
                    tab,
                    saved,
                    state: State::default(),
                    out: Vec::new(),
                    leave: Leave::Stay,
                },
            )
    }

    // T-SET-006: Reset puts the tab's keys back in the draft, Save sends them
    #[test]
    fn the_general_tab_resets_and_saves() {
        let mut saved = Config::default();
        saved.general.theme = "light".into();
        saved.general.ui_scale = 150;
        saved.player.seek_seconds = 10;
        saved.subtitles.font_size = 50.0;
        let mut h = harness(SettingsTab::General, saved);
        h.run();
        h.get_by_label("settings-reset").click();
        h.run();
        h.get_by_label("settings-save").click();
        h.run();
        let [Command::SettingsSave(Section::General, draft)] = &h.state().out[..] else {
            panic!("{:?}", h.state().out);
        };
        assert_eq!(draft.general.theme, "system");
        assert_eq!(
            (draft.general.ui_scale, draft.player.seek_seconds),
            (100, 3)
        );
        // another tab's keys are not the General tab's to reset
        assert_eq!(draft.subtitles.font_size, 50.0);
        h.get_by_label("settings-close").click();
        h.run();
        assert_eq!(h.state().leave, Leave::Close);
        // the tabs lead to one another
        h.get_by_label("settings-shortcuts").click();
        h.run();
        assert_eq!(h.state().leave, Leave::To(SettingsTab::Shortcuts));
    }

    #[test]
    fn about_shows_the_version_and_the_licenses() {
        let mut h = harness(SettingsTab::About, Config::default());
        h.run();
        h.get_by_label(&format!("Nikomiru {}", env!("CARGO_PKG_VERSION")));
        h.get_by_label("about-third-party-missing");
        h.get_by_label("GPL-3.0-only").click();
        h.run();
        assert!(matches!(&h.state().out[..], [Command::OpenExternal(url)] if url == LICENSE));
        // nothing to save or reset
        assert!(h.query_by_label("settings-save").is_none());
    }
}
