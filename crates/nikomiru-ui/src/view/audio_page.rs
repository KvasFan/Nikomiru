// SPDX-License-Identifier: GPL-3.0-only

//! The Word audio tab of the settings (AUD-001, AUD-005, AUD-006): the
//! sources, in the order they are asked, each on or off, with their URL
//! template; the volume; playing by itself when a popup opens.

use egui::{
    Align, Button, Color32, Frame, Layout, Margin, Rect, RichText, Role, ScrollArea, Slider,
    Stroke, TextEdit, Ui, UiBuilder, WidgetInfo, vec2,
};
use nikomiru_core::config::{AudioSourceConfig, WordAudioConfig};

use super::dict_page::full_screen;
use super::popups::Translate;
use super::settings::{Leave, SettingsTab, tabs};
use super::{View, theme};
use crate::session::Session;
use crate::ui::Command;

const BACKDROP: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 0xa6);
const CARD: Color32 = Color32::from_rgba_unmultiplied_const(0x20, 0x22, 0x27, 0xf0);
const CARD_BORDER: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x40);
const ROW: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x0c);
const DETAILS: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
const GOOD: Color32 = Color32::from_rgb(0x7c, 0xd3, 0x8a);
const BAD: Color32 = Color32::from_rgb(0xf0, 0xb3, 0x5a);

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<WordAudioConfig>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The settings as saved.
    pub saved: &'a WordAudioConfig,
    /// The last message, and whether it is good news.
    pub status: Option<&'a (String, bool)>,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        saved: s.word_audio_config(),
        status: s.audio_status.as_ref(),
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
            .color(DETAILS),
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
    }
    let screen = ctx.content_rect();
    let width = 820.0_f32.min(screen.width() - 32.0).max(0.0);
    let height = 680.0_f32.min(screen.height() - 32.0).max(0.0);
    let mut leave = Leave::Stay;
    full_screen(ctx, "audio-page", BACKDROP, |ui| {
        let card = Rect::from_center_size(screen.center(), vec2(width, height));
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(card)
                .layout(Layout::top_down(Align::Min)),
        );
        Frame::new()
            .fill(CARD)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .corner_radius(10)
            .inner_margin(Margin::same(16))
            .show(&mut ui, |ui| {
                ui.set_width((width - 34.0).max(0.0));
                ui.set_height((height - 34.0).max(0.0));
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.horizontal(|ui| {
                    if let Some(tab) = tabs(ui, SettingsTab::Audio, &|id| tr(s, id)) {
                        leave = Leave::To(tab);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr(s, "audio-close")).clicked() {
                            leave = Leave::Close;
                        }
                        if ui.button(tr(s, "audio-save")).clicked()
                            && let Some(draft) = &state.draft
                        {
                            let mut config = draft.clone();
                            config.sources.retain(|x| !x.url.trim().is_empty());
                            for source in &mut config.sources {
                                source.name = source.name.trim().to_string();
                                source.url = source.url.trim().to_string();
                            }
                            out.push(Command::WordAudioSave(Box::new(config)));
                        }
                    });
                });
                if let Some((text, ok)) = s.status {
                    ui.add(
                        egui::Label::new(RichText::new(text).color(if *ok { GOOD } else { BAD }))
                            .wrap(),
                    );
                }
                ScrollArea::vertical()
                    .id_salt("audio-page")
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, s, state));
            });
    });
    leave
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State) {
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    ui.spacing_mut().item_spacing.y = 8.0;
    section(ui, &tr(s, "audio-sources"));
    ui.add(egui::Label::new(RichText::new(tr(s, "audio-sources-hint")).color(DETAILS)).wrap());
    let count = draft.sources.len();
    let mut swap = None;
    let mut remove = None;
    for (i, source) in draft.sources.iter_mut().enumerate() {
        Frame::new()
            .fill(ROW)
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
                    ui.add(
                        TextEdit::singleline(&mut source.url)
                            .hint_text("http://localhost:5050/?term={term}&reading={reading}")
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                });
            });
    }
    if let Some((a, b)) = swap {
        draft.sources.swap(a, b);
    }
    if let Some(i) = remove {
        draft.sources.remove(i);
    }
    if ui.button(tr(s, "audio-add")).clicked() {
        let n = draft.sources.len() + 1;
        draft.sources.push(AudioSourceConfig {
            name: (s.tr)("audio-source-n", &[("n", n.to_string())]),
            url: String::new(),
            enabled: true,
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
    }

    fn harness(saved: WordAudioConfig) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 1000.0))
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
                        tr: &tr,
                    };
                    let _ = draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    saved,
                    state: State::default(),
                    out: Vec::new(),
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
}
