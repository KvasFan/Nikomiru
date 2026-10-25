// SPDX-License-Identifier: GPL-3.0-only

//! The Player settings page (PLY-016, PLY-018): the option set in use as
//! `mpv.conf` text and as controls in groups, both editing the same set, and
//! presets in `mpv.conf` format, loaded with a preview of every line, saved,
//! imported and exported.

use egui::{Button, ComboBox, Frame, Margin, RichText, Role, TextEdit, Ui, WidgetInfo, vec2};
use nikomiru_player::options::{Line, parse_mpv_conf};
use nikomiru_player::settings::{CONTROLS, Control, Group, Kind, OptionSet};

use super::popups::Translate;
use super::settings::{Leave, SettingsTab, page};
use super::{View, palette, theme};
use crate::player_settings::{LineStatus, PlayerPage};
use crate::session::Session;
use crate::ui::Command;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The set being edited.
    draft: Option<OptionSet>,
    /// The preset chosen in the list.
    chosen: String,
    /// The name to save the set under.
    name: String,
    /// The sets applied when the draft was taken.
    seen_applied: Option<u64>,
    /// The draft as `mpv.conf` text, as the user edits it.
    text: String,
    /// The draft the text was written from or read into: a draft changed by
    /// the controls writes the text again.
    text_of: Option<OptionSet>,
    /// The text has the keyboard: it is not written again under the cursor.
    text_focus: bool,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The page's state in the session.
    pub page: &'a PlayerPage,
    /// The option set as saved.
    pub saved: &'a OptionSet,
    /// The last preset loaded or saved.
    pub preset: &'a str,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let saved = s.player_set();
    let scene = Scene {
        page: &s.player_page,
        saved: &saved,
        preset: s.player_preset(),
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.player, out);
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

/// The draft follows the saved set when the page opens and after each apply.
fn update(s: &Scene<'_>, state: &mut State) {
    let applied = s.page.applied;
    if state.draft.is_none() || state.seen_applied != Some(applied) {
        state.draft = Some(s.saved.clone());
        state.seen_applied = Some(applied);
        if state.chosen.is_empty() {
            state.chosen = s.preset.to_string();
        }
        if state.name.is_empty() {
            state.name = s.preset.to_string();
        }
    }
}

/// Draws the page; returns what it asks for.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    update(s, state);
    let tr = |id: &str| tr(s, id);
    page(
        ctx,
        SettingsTab::Player,
        &tr,
        s.page.status.as_ref(),
        |buttons, ui| {
            if buttons.button(tr("player-apply")).clicked()
                && let Some(draft) = &state.draft
            {
                out.push(Command::PlayerApply(Box::new((draft.clone(), None))));
            }
            body(ui, s, state, out);
        },
    )
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    ui.spacing_mut().item_spacing.y = 8.0;
    presets(ui, s, state, out);
    if let Some(draft) = &state.draft
        && state.text_of.as_ref() != Some(draft)
        && !state.text_focus
    {
        state.text = draft.to_mpv_conf("");
        state.text_of = Some(draft.clone());
    }
    if let Some(preview) = &s.page.preview {
        Frame::new()
            .fill(palette::current().row)
            .corner_radius(6)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new((s.tr)(
                        "player-preview",
                        &[("source", preview.source.clone())],
                    ))
                    .family(theme::bold()),
                );
                for l in &preview.lines {
                    let (status, color) = match &l.status {
                        LineStatus::Control => {
                            (tr(s, "player-line-control"), palette::current().good)
                        }
                        LineStatus::Raw => (tr(s, "player-line-raw"), palette::current().good),
                        LineStatus::Ignored(why) => (why.clone(), palette::current().bad),
                        LineStatus::Invalid(why) => (why.clone(), palette::current().bad),
                    };
                    ui.label(
                        RichText::new(format!("{:>3}  {}  — {status}", l.line, l.text))
                            .family(egui::FontFamily::Monospace)
                            .color(color),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button(tr(s, "player-preview-apply")).clicked() {
                        out.push(Command::PlayerPreviewApply);
                    }
                    if ui.button(tr(s, "player-preview-cancel")).clicked() {
                        out.push(Command::PlayerPreviewCancel);
                    }
                });
            });
    }
    conf_text(ui, s, state);
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    for (group, title) in [
        (Group::Rendering, "player-rendering"),
        (Group::Audio, "player-audio"),
        (Group::Advanced, "player-advanced"),
    ] {
        ui.horizontal(|ui| {
            section(ui, &tr(s, title));
            // SET-006: this group back to the defaults
            if ui.small_button(tr(s, "player-reset")).clicked() {
                for c in CONTROLS.iter().filter(|c| c.group == group) {
                    match c.key {
                        "hwdec" => draft.values.insert("hwdec".into(), "auto-safe".into()),
                        "profile" => draft.values.insert("profile".into(), "default".into()),
                        key => draft.values.remove(key),
                    };
                }
            }
        });
        egui::Grid::new(title)
            .num_columns(2)
            .spacing(vec2(12.0, 6.0))
            .show(ui, |ui| {
                for c in CONTROLS.iter().filter(|c| c.group == group) {
                    control(ui, s, c, draft);
                    ui.end_row();
                }
            });
    }
    ui.add(
        egui::Label::new(
            RichText::new(tr(s, "player-apply-note")).color(palette::current().details),
        )
        .wrap(),
    );
}

/// The option set in use as `mpv.conf` text (PLY-016): what the controls
/// set, then the options they do not show. Edited, it becomes the draft; the
/// lines that will not apply are named under it.
fn conf_text(ui: &mut Ui, s: &Scene<'_>, state: &mut State) {
    section(ui, &tr(s, "player-conf"));
    let label = ui.label(tr(s, "player-conf-hint"));
    let edit = ui
        .add(
            TextEdit::multiline(&mut state.text)
                .font(egui::TextStyle::Monospace)
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id);
    state.text_focus = edit.has_focus();
    if edit.changed() {
        let (set, _) = OptionSet::from_mpv_conf(&state.text);
        state.draft = Some(set.clone());
        state.text_of = Some(set);
    }
    for line in parse_mpv_conf(&state.text) {
        let text = match line {
            Line::Apply { .. } => continue,
            Line::Ignore { line, name, .. } => (s.tr)(
                "player-raw-ignored",
                &[("line", line.to_string()), ("name", name)],
            ),
            Line::Invalid { line, error } => (s.tr)(
                "player-raw-invalid",
                &[("line", line.to_string()), ("error", error)],
            ),
        };
        ui.label(RichText::new(text).color(palette::current().bad));
    }
}

/// The presets: load one, save the set as one, import and export.
fn presets(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    section(ui, &tr(s, "player-presets"));
    ui.horizontal(|ui| {
        let name = tr(s, "player-preset-list");
        let combo = ComboBox::from_id_salt("player-presets")
            .width(220.0)
            .selected_text(state.chosen.clone())
            .show_ui(ui, |ui| {
                for p in &s.page.presets {
                    ui.selectable_value(&mut state.chosen, p.clone(), p);
                }
            });
        combo
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &name));
        let can_load = s.page.presets.contains(&state.chosen);
        if ui
            .add_enabled(can_load, Button::new(tr(s, "player-load")))
            .clicked()
        {
            out.push(Command::PlayerLoadPreset(state.chosen.clone()));
        }
        if ui.button(tr(s, "player-import")).clicked() {
            out.push(Command::PlayerImport);
        }
    });
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "player-preset-name"));
        ui.add(TextEdit::singleline(&mut state.name).desired_width(220.0))
            .labelled_by(label.id);
        if let Some(draft) = &state.draft {
            if ui
                .add_enabled(
                    !state.name.trim().is_empty(),
                    Button::new(tr(s, "player-save-preset")),
                )
                .clicked()
            {
                out.push(Command::PlayerSavePreset(
                    state.name.trim().to_string(),
                    Box::new(draft.clone()),
                ));
            }
            if ui.button(tr(s, "player-export")).clicked() {
                out.push(Command::PlayerExport(Box::new(draft.clone())));
            }
        }
    });
}

/// One control: its option's name and its editor; an empty value or
/// "default" leaves the option unset.
fn control(ui: &mut Ui, s: &Scene<'_>, c: &Control, set: &mut OptionSet) {
    let label = ui.label(c.key);
    let default = tr(s, "player-default");
    let always_set = matches!(c.key, "hwdec" | "profile");
    match c.kind {
        Kind::Choice(values) => {
            let mut current = set.values.get(c.key).cloned().unwrap_or_default();
            let shown = if current.is_empty() {
                default.clone()
            } else {
                current.clone()
            };
            let combo = ComboBox::from_id_salt(c.key)
                .width(220.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if !always_set {
                        ui.selectable_value(&mut current, String::new(), &default);
                    }
                    for v in values {
                        ui.selectable_value(&mut current, (*v).to_string(), *v);
                    }
                });
            combo
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, c.key));
            store(set, c.key, current);
        }
        Kind::Flag => {
            let mut current = set.values.get(c.key).cloned().unwrap_or_default();
            let shown = if current.is_empty() {
                default.clone()
            } else {
                current.clone()
            };
            let combo = ComboBox::from_id_salt(c.key)
                .width(120.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut current, String::new(), &default);
                    ui.selectable_value(&mut current, "yes".into(), "yes");
                    ui.selectable_value(&mut current, "no".into(), "no");
                });
            combo
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, c.key));
            store(set, c.key, current);
        }
        Kind::Integer(..) | Kind::Text => {
            let mut current = set.values.get(c.key).cloned().unwrap_or_default();
            ui.add(
                TextEdit::singleline(&mut current)
                    .hint_text(&default)
                    .desired_width(120.0),
            )
            .labelled_by(label.id);
            store(set, c.key, current.trim().to_string());
        }
    }
}

fn store(set: &mut OptionSet, key: &str, value: String) {
    if value.is_empty() {
        set.values.remove(key);
    } else {
        set.values.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_settings::{Preview, PreviewLine};
    use egui::{Id, Sense};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        page: PlayerPage,
        saved: OptionSet,
        state: State,
        out: Vec<Command>,
    }

    fn harness(page: PlayerPage, saved: OptionSet) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 1400.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        saved: &t.saved,
                        preset: "",
                        tr: &tr,
                    };
                    let _ = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    page,
                    saved,
                    state: State::default(),
                    out: Vec::new(),
                },
            )
    }

    fn click(h: &mut Harness<'_, Test>, label: &str) -> Vec<Command> {
        h.get_by_label(label).click();
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    // T-PLY-018
    #[test]
    fn the_controls_edit_the_set_and_apply_sends_it() {
        // as the settings give it: `ao` set the way the controls are, a raw line
        let mut cfg = nikomiru_core::config::PlayerConfig::default();
        cfg.mpv_options.insert("deband".into(), "yes".into());
        cfg.mpv_options.insert("ao".into(), "null".into());
        cfg.raw_mpv_options = "sharpen=0.5".into();
        let saved = OptionSet::from_config(&cfg);
        let mut h = harness(PlayerPage::default(), saved);
        h.run();
        // the raw options and the other options are shown
        assert_eq!(h.state().state.draft.as_ref().unwrap().raw, "sharpen=0.5");
        // the Rendering group back to the defaults: deband goes
        let resets: Vec<_> = h.query_all_by_label("player-reset").collect();
        resets[0].click();
        h.run();
        let out = click(&mut h, "player-apply");
        match out.as_slice() {
            [Command::PlayerApply(settings)] => {
                let (set, preset) = settings.as_ref();
                assert!(!set.values.contains_key("deband"), "{set:?}");
                assert_eq!(set.values["profile"], "default");
                assert_eq!(set.values["ao"], "null", "options of no control stay");
                assert_eq!(preset, &None);
            }
            other => panic!("{other:?}"),
        }
        let out = click(&mut h, "player-export");
        assert!(matches!(out.as_slice(), [Command::PlayerExport(_)]));
        let out = click(&mut h, "player-import");
        assert!(matches!(out.as_slice(), [Command::PlayerImport]));
    }

    // T-PLY-016: the set in use as mpv.conf text, edited both ways
    #[test]
    fn the_set_in_use_is_shown_and_edited_as_mpv_conf() {
        let mut cfg = nikomiru_core::config::PlayerConfig::default();
        cfg.mpv_options.insert("deband".into(), "yes".into());
        cfg.raw_mpv_options = "sharpen=0.5".into();
        let mut h = harness(PlayerPage::default(), OptionSet::from_config(&cfg));
        h.run();
        let text = |h: &Harness<'_, Test>| h.get_by_label("player-conf-hint").value();
        // preloaded: only what differs from the defaults
        assert_eq!(text(&h).as_deref(), Some("deband=yes\nsharpen=0.5\n"));
        // a control changed: the text follows
        let resets: Vec<_> = h.query_all_by_label("player-reset").collect();
        resets[0].click();
        h.run();
        h.run();
        assert_eq!(text(&h).as_deref(), Some("sharpen=0.5\n"));
        // the text typed: the controls follow, the text stays as typed
        h.get_by_label("player-conf-hint").focus();
        h.run();
        h.get_by_label("player-conf-hint")
            .type_text("brightness=7\n# mine\nscale=\n");
        h.run();
        h.run();
        let typed = text(&h).unwrap_or_default();
        assert!(
            typed.contains("# mine") && typed.contains("scale="),
            "{typed}"
        );
        assert_eq!(h.get_by_label("brightness").value().as_deref(), Some("7"));
        let out = click(&mut h, "player-apply");
        match out.as_slice() {
            [Command::PlayerApply(settings)] => {
                let (set, _) = settings.as_ref();
                assert_eq!(set.values["brightness"], "7");
                assert!(!set.values.contains_key("deband"), "{set:?}");
                assert_eq!(set.raw, "sharpen=0.5", "{set:?}");
            }
            other => panic!("{other:?}"),
        }
        // the text left: written again from the set, the empty value gone
        h.run();
        assert_eq!(text(&h).as_deref(), Some("brightness=7\nsharpen=0.5\n"));
    }

    // T-PLY-016
    #[test]
    fn a_preview_lists_every_line_and_waits_for_apply() {
        let (set, _) = OptionSet::from_mpv_conf("scale=lanczos\n");
        let page = PlayerPage {
            presets: vec!["anime".into()],
            preview: Some(Preview {
                source: "mpv.conf".into(),
                preset: None,
                set,
                lines: vec![
                    PreviewLine {
                        line: 1,
                        text: "scale=lanczos".into(),
                        status: LineStatus::Control,
                    },
                    PreviewLine {
                        line: 2,
                        text: "script".into(),
                        status: LineStatus::Ignored("runs code".into()),
                    },
                ],
            }),
            ..PlayerPage::default()
        };
        let mut h = harness(page, OptionSet::default());
        h.run();
        h.get_by_label("  2  script  — runs code");
        let out = click(&mut h, "player-preview-apply");
        assert!(matches!(out.as_slice(), [Command::PlayerPreviewApply]));
        let out = click(&mut h, "player-preview-cancel");
        assert!(matches!(out.as_slice(), [Command::PlayerPreviewCancel]));
        // a preset is chosen, then loaded
        h.state_mut().state.chosen = "anime".into();
        h.run();
        let out = click(&mut h, "player-load");
        assert!(matches!(out.as_slice(), [Command::PlayerLoadPreset(n)] if n == "anime"));
        h.state_mut().state.name = "mine".into();
        h.run();
        let out = click(&mut h, "player-save-preset");
        assert!(matches!(out.as_slice(), [Command::PlayerSavePreset(n, _)] if n == "mine"));
    }
}
