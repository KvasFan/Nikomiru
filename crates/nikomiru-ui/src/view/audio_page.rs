// SPDX-License-Identifier: GPL-3.0-only

//! The Word audio tab of the settings (AUD-001, AUD-005, AUD-006): the
//! sources, in the order they are asked, each on or off, with their URL
//! template; the volume; playing by itself when a popup opens.

use egui::{
    Align, Button, Frame, Layout, Margin, RichText, Role, Slider, TextEdit, Ui, WidgetInfo,
};
use nikomiru_core::config::{AudioSourceConfig, WordAudioConfig};

use super::popups::Translate;
use super::settings::{Leave, SettingsTab, page};
use super::{View, palette, theme};
use crate::session::Session;
use crate::ui::Command;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<WordAudioConfig>,
    /// The database chosen last in the file dialog, once put in its row.
    picked: u64,
    /// The sub-sources typed for each database row.
    priorities: Vec<String>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The settings as saved.
    pub saved: &'a WordAudioConfig,
    /// The last message, and whether it is good news.
    pub status: Option<&'a (String, bool)>,
    /// A database chosen in the file dialog: its number, its row, its path.
    pub picked: Option<&'a (u64, usize, String)>,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        saved: s.word_audio_config(),
        status: s.audio_status.as_ref(),
        picked: s.audio_database.as_ref(),
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.audio, out);
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

/// Draws the page; returns what it asks for.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    if state.draft.is_none() {
        state.draft = Some(s.saved.clone());
        state.priorities = s
            .saved
            .sources
            .iter()
            .map(|x| x.priority.join(", "))
            .collect();
    }
    // AUD-003: the database chosen goes in its row
    if let Some((n, row, path)) = s.picked
        && *n != state.picked
    {
        state.picked = *n;
        if let Some(source) = state.draft.as_mut().and_then(|d| d.sources.get_mut(*row)) {
            source.url.clone_from(path);
        }
    }
    let tr = |id: &str| tr(s, id);
    page(ctx, SettingsTab::Audio, &tr, s.status, |buttons, ui| {
        if buttons.button(tr("audio-save")).clicked()
            && let Some(draft) = &state.draft
        {
            let mut config = draft.clone();
            for (source, typed) in config.sources.iter_mut().zip(&state.priorities) {
                source.priority = typed
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            config.sources.retain(|x| !x.url.trim().is_empty());
            for source in &mut config.sources {
                source.name = source.name.trim().to_string();
                source.url = source.url.trim().to_string();
                if source.database().is_none() {
                    source.priority.clear();
                }
            }
            out.push(Command::WordAudioSave(Box::new(config)));
        }
        body(ui, s, state, out);
    })
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    state.priorities.resize(draft.sources.len(), String::new());
    ui.spacing_mut().item_spacing.y = 8.0;
    section(ui, &tr(s, "audio-sources"));
    ui.add(
        egui::Label::new(
            RichText::new(tr(s, "audio-sources-hint")).color(palette::current().details),
        )
        .wrap(),
    );
    let count = draft.sources.len();
    let mut swap = None;
    let mut remove = None;
    for (i, source) in draft.sources.iter_mut().enumerate() {
        Frame::new()
            .fill(palette::current().row)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let name = (s.tr)("audio-source-n", &[("n", (i + 1).to_string())]);
                    let on = ui.checkbox(&mut source.enabled, "");
                    on.widget_info(|| {
                        WidgetInfo::selected(Role::CheckBox, true, source.enabled, &name)
                    });
                    let label = ui.label(tr(s, "audio-name"));
                    ui.add(TextEdit::singleline(&mut source.name).desired_width(160.0))
                        .labelled_by(label.id);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button = |ui: &mut Ui, text: &str, id: &str, enabled: bool| {
                            let label = (s.tr)(id, &[("n", (i + 1).to_string())]);
                            let b = ui.add_enabled(enabled, Button::new(text).small());
                            b.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &label));
                            b.clicked()
                        };
                        if button(ui, "✕", "audio-remove", true) {
                            remove = Some(i);
                        }
                        if button(ui, "▼", "audio-down", i + 1 < count) {
                            swap = Some((i, i + 1));
                        }
                        if button(ui, "▲", "audio-up", i > 0) {
                            swap = Some((i - 1, i));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let label = ui.label(tr(s, "audio-url"));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // AUD-003: a database of the Local Audio Server
                        let name = (s.tr)("audio-database", &[("n", (i + 1).to_string())]);
                        let pick = ui.button(tr(s, "audio-database-button"));
                        pick.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
                        if pick.clicked() {
                            out.push(Command::AudioChooseDatabase(i));
                        }
                        ui.add(
                            TextEdit::singleline(&mut source.url)
                                .hint_text("http://localhost:5050/?term={term}&reading={reading}")
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    });
                });
                if source.database().is_some() {
                    ui.horizontal(|ui| {
                        let label = ui.label(tr(s, "audio-priority"));
                        ui.add(
                            TextEdit::singleline(&mut state.priorities[i])
                                .hint_text("nhk16, forvo, jpod")
                                .desired_width(320.0),
                        )
                        .labelled_by(label.id);
                    });
                }
            });
    }
    if let Some((a, b)) = swap {
        draft.sources.swap(a, b);
        state.priorities.swap(a, b);
    }
    if let Some(i) = remove {
        draft.sources.remove(i);
        state.priorities.remove(i);
    }
    if ui.button(tr(s, "audio-add")).clicked() {
        let n = draft.sources.len() + 1;
        draft.sources.push(AudioSourceConfig {
            name: (s.tr)("audio-source-n", &[("n", n.to_string())]),
            url: String::new(),
            enabled: true,
            priority: Vec::new(),
        });
    }
    section(ui, &tr(s, "audio-playing"));
    ui.checkbox(&mut draft.autoplay, tr(s, "audio-autoplay"));
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "audio-volume"));
        ui.add(Slider::new(&mut draft.volume, 0..=100).suffix(" %"))
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
        saved: WordAudioConfig,
        state: State,
        out: Vec<Command>,
        picked: Option<(u64, usize, String)>,
    }

    fn harness(saved: WordAudioConfig) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, args: &[(&str, String)]| {
                        let mut text = id.to_string();
                        for (_, v) in args {
                            text.push(' ');
                            text.push_str(v);
                        }
                        text
                    };
                    let scene = Scene {
                        saved: &t.saved,
                        status: None,
                        picked: t.picked.as_ref(),
                        tr: &tr,
                    };
                    let _ = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    saved,
                    state: State::default(),
                    out: Vec::new(),
                    picked: None,
                },
            )
    }

    fn saved(h: &mut Harness<'_, Test>) -> WordAudioConfig {
        h.get_by_label("audio-save").click();
        h.run();
        match std::mem::take(&mut h.state_mut().out).as_slice() {
            [Command::WordAudioSave(c)] => (**c).clone(),
            other => panic!("{other:?}"),
        }
    }

    // T-AUD-001, T-AUD-007
    #[test]
    fn sources_are_added_ordered_turned_off_and_saved() {
        let mut h = harness(WordAudioConfig::default());
        h.run();
        // none built in
        assert!(saved(&mut h).sources.is_empty());
        for url in [
            "http://localhost:5050/?t={term}",
            "https://audio.example.org/{reading}",
        ] {
            h.get_by_label("audio-add").click();
            h.run();
            let field = h.query_all_by_label("audio-url").last().unwrap();
            field.focus();
            h.run();
            h.query_all_by_label("audio-url")
                .last()
                .unwrap()
                .type_text(url);
            h.run();
        }
        h.get_by_label("audio-up 2").click();
        h.run();
        h.get_by_label("audio-source-n 1").click();
        h.run();
        // an empty source is not saved
        h.get_by_label("audio-add").click();
        h.run();
        let config = saved(&mut h);
        let sources: Vec<(&str, bool)> = config
            .sources
            .iter()
            .map(|s| (s.url.as_str(), s.enabled))
            .collect();
        assert_eq!(
            sources,
            [
                ("https://audio.example.org/{reading}", false),
                ("http://localhost:5050/?t={term}", true)
            ]
        );
        assert!(config.autoplay);
    }

    // T-AUD-003: a database chosen for a row, its sub-sources in order
    #[test]
    fn a_database_is_chosen_with_its_sub_sources() {
        let mut h = harness(WordAudioConfig::default());
        h.run();
        h.get_by_label("audio-add").click();
        h.run();
        h.get_by_label("audio-database 1").click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(out.as_slice(), [Command::AudioChooseDatabase(0)]),
            "{out:?}"
        );
        // the file dialog answers
        h.state_mut().picked = Some((1, 0, "/data/android.db".into()));
        h.run();
        h.run();
        h.get_by_label("audio-priority").focus();
        h.run();
        h.get_by_label("audio-priority").type_text("forvo, nhk16");
        h.run();
        let config = saved(&mut h);
        assert_eq!(config.sources[0].url, "/data/android.db");
        assert_eq!(config.sources[0].priority, ["forvo", "nhk16"]);
        assert_eq!(config.sources[0].database(), Some("/data/android.db"));
    }
}
