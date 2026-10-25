// SPDX-License-Identifier: GPL-3.0-only

//! The Shortcuts tab of the settings (KEY-001…KEY-004): the keys of every
//! action by context, a key removed with ×, one added by pressing it, an
//! action back to its default keys. A key another action of the context has
//! is refused there and then (KEY-003); the tab saves `[shortcuts]` with only
//! the actions whose keys differ from the defaults.

use std::collections::BTreeMap;

use egui::{Button, Grid, RichText, Role, Ui, WidgetInfo, vec2};

use super::palette;
use super::popups::Translate;
use crate::shortcuts::{Action, Bindings, Context, display, normalize};
use crate::ui::Command;

/// What the tab keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The action waiting for a key, and whether the session waits for it
    /// yet (it starts at the next frame).
    capturing: Option<(Action, bool)>,
    /// Why the last key was refused.
    refused: Option<String>,
}

impl State {
    /// The keys went back to their defaults.
    pub(crate) fn reset(&mut self) {
        self.refused = None;
    }

    /// Whether it waits for a key: the session must stop if the tab closes.
    pub(crate) fn capturing(&self) -> bool {
        self.capturing.is_some()
    }
}

/// The keys of `action`: its own in `overrides`, or its defaults.
fn keys(overrides: &BTreeMap<String, Vec<String>>, action: Action) -> Vec<String> {
    match overrides.get(action.name()) {
        Some(list) => list.iter().map(|c| normalize(c)).collect(),
        None => action
            .default_keys()
            .iter()
            .map(|c| (*c).to_string())
            .collect(),
    }
}

/// Gives `action` the keys `list`: an override, or none if they are its
/// defaults.
fn set(overrides: &mut BTreeMap<String, Vec<String>>, action: Action, list: Vec<String>) {
    if list
        .iter()
        .map(String::as_str)
        .eq(action.default_keys().iter().copied())
    {
        overrides.remove(action.name());
    } else {
        overrides.insert(action.name().to_string(), list);
    }
}

/// Adds `chord` to `action`, unless another action has it (KEY-003): then
/// the reason.
fn add(
    overrides: &mut BTreeMap<String, Vec<String>>,
    action: Action,
    chord: &str,
    tr: &Translate<'_>,
) -> Result<(), String> {
    let mut list = keys(overrides, action);
    if list.iter().any(|c| c == chord) {
        return Ok(());
    }
    list.push(chord.to_string());
    let mut candidate = overrides.clone();
    set(&mut candidate, action, list);
    match Bindings::with_overrides(&candidate) {
        Ok(_) => {
            *overrides = candidate;
            Ok(())
        }
        Err(conflicts) => {
            let others: Vec<String> = conflicts
                .iter()
                .filter(|c| c.chord == chord)
                .flat_map(|c| c.actions.iter())
                .filter(|a| **a != action)
                .map(|a| tr(&a.label_id(), &[]))
                .collect();
            Err(tr(
                "shortcuts-taken",
                &[("key", display(chord)), ("actions", others.join(", "))],
            ))
        }
    }
}

/// Draws the tab. `capture`: whether the session waits for a key, and the
/// chord it got.
pub(crate) fn body(
    ui: &mut Ui,
    tr: &Translate<'_>,
    overrides: &mut BTreeMap<String, Vec<String>>,
    state: &mut State,
    capture: (bool, Option<&str>),
    out: &mut Vec<Command>,
) {
    let t = |id: &str| tr(id, &[]);
    // the key pressed, once the session has it
    if let Some((action, started)) = state.capturing {
        let (waiting, got) = capture;
        if !started {
            if waiting {
                state.capturing = Some((action, true));
            }
        } else if !waiting {
            state.capturing = None;
            out.push(Command::ShortcutCapture(false));
            if let Some(chord) = got {
                state.refused = add(overrides, action, chord, tr).err();
            }
        }
    }
    let p = palette::current();
    ui.add(egui::Label::new(RichText::new(t("shortcuts-hint")).color(p.details)).wrap());
    if let Err(conflicts) = Bindings::with_overrides(overrides) {
        let keys: Vec<String> = conflicts.iter().map(|c| display(&c.chord)).collect();
        let text = tr("shortcuts-conflict", &[("keys", keys.join(", "))]);
        ui.label(RichText::new(text).color(p.bad));
    }
    if let Some(text) = &state.refused {
        ui.label(RichText::new(text).color(p.bad));
    }
    // one grid for every context: the columns line up
    Grid::new("shortcuts")
        .num_columns(3)
        .spacing(vec2(16.0, 6.0))
        .min_col_width(240.0)
        .show(ui, |ui| {
            for (context, title) in [
                (Context::Global, "shortcuts-global"),
                (Context::Player, "shortcuts-player"),
                (Context::Popup, "shortcuts-popup"),
            ] {
                ui.label(
                    RichText::new(t(title))
                        .size(15.0)
                        .family(super::theme::bold())
                        .color(p.details),
                );
                ui.end_row();
                for action in Action::ALL.into_iter().filter(|a| a.context() == context) {
                    row(ui, tr, overrides, state, action, out);
                }
            }
        });
}

/// An action: its name, its keys with × and +, ↺ if they are not the
/// defaults.
fn row(
    ui: &mut Ui,
    tr: &Translate<'_>,
    overrides: &mut BTreeMap<String, Vec<String>>,
    state: &mut State,
    action: Action,
    out: &mut Vec<Command>,
) {
    let p = palette::current();
    let name = tr(&action.label_id(), &[]);
    ui.label(&name);
    let list = keys(overrides, action);
    ui.horizontal(|ui| {
        for (i, chord) in list.iter().enumerate() {
            ui.label(RichText::new(display(chord)).strong());
            let label = tr(
                "shortcuts-remove",
                &[("key", display(chord)), ("action", name.clone())],
            );
            let remove = ui.add(Button::new("×").small()).on_hover_text(&label);
            remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
            if remove.clicked() {
                let mut rest = list.clone();
                rest.remove(i);
                set(overrides, action, rest);
                state.refused = None;
            }
            ui.add_space(6.0);
        }
        if state.capturing.is_some_and(|(a, _)| a == action) {
            ui.label(RichText::new(tr("shortcuts-press", &[])).color(p.accent_text));
        } else {
            let label = tr("shortcuts-add", &[("action", name.clone())]);
            let plus = ui.add(Button::new("+").small()).on_hover_text(&label);
            plus.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
            if plus.clicked() && state.capturing.is_none() {
                state.capturing = Some((action, false));
                state.refused = None;
                out.push(Command::ShortcutCapture(true));
            }
        }
    });
    if overrides.contains_key(action.name()) {
        let label = tr("shortcuts-default", &[("action", name.clone())]);
        let reset = ui.add(Button::new("↺").small()).on_hover_text(&label);
        reset.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
        if reset.clicked() {
            overrides.remove(action.name());
            state.refused = None;
        }
    } else {
        ui.label("");
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Id;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        overrides: BTreeMap<String, Vec<String>>,
        state: State,
        capture: (bool, Option<String>),
        out: Vec<Command>,
    }

    fn harness() -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1000.0, 1600.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), egui::Sense::click());
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (_, v) in args {
                            text.push(' ');
                            text.push_str(v);
                        }
                        text
                    };
                    let capture = (t.capture.0, t.capture.1.as_deref());
                    body(ui, &tr, &mut t.overrides, &mut t.state, capture, &mut t.out);
                },
                Test {
                    overrides: BTreeMap::new(),
                    state: State::default(),
                    capture: (false, None),
                    out: Vec::new(),
                },
            )
    }

    // T-KEY-001, T-KEY-003
    #[test]
    fn a_key_is_added_refused_removed_and_reset() {
        let mut h = harness();
        h.run();
        h.get_by_label("shortcuts-add action-mute").click();
        h.run();
        assert!(matches!(
            h.state().out[..],
            [Command::ShortcutCapture(true)]
        ));
        // the session waits, then gets K
        h.state_mut().capture = (true, None);
        h.run();
        h.state_mut().capture = (false, Some("KeyK".into()));
        h.run();
        assert_eq!(h.state().overrides["mute"], ["KeyM", "KeyK"]);
        assert!(matches!(
            h.state().out[..],
            [
                Command::ShortcutCapture(true),
                Command::ShortcutCapture(false)
            ]
        ));
        // Space is play/pause's: refused, and said
        h.get_by_label("shortcuts-add action-mute").click();
        h.run();
        h.state_mut().capture = (true, None);
        h.run();
        h.state_mut().capture = (false, Some("Space".into()));
        h.run();
        assert_eq!(h.state().overrides["mute"], ["KeyM", "KeyK"]);
        h.get_by_label("shortcuts-taken Space action-play-pause");
        // M removed, then the defaults back
        h.get_by_label("shortcuts-remove M action-mute").click();
        h.run();
        assert_eq!(h.state().overrides["mute"], ["KeyK"]);
        h.get_by_label("shortcuts-default action-mute").click();
        h.run();
        assert!(h.state().overrides.is_empty());
        // Esc gave up: nothing changes
        h.get_by_label("shortcuts-add action-mute").click();
        h.run();
        h.state_mut().capture = (true, None);
        h.run();
        h.state_mut().capture = (false, None);
        h.run();
        assert!(h.state().overrides.is_empty());
        assert!(!h.state().state.capturing());
    }
}
