// SPDX-License-Identifier: GPL-3.0-only

//! The Anki settings page (ANK-006, ANK-011, ANK-020, SPECIFICATION §9): the
//! AnkiConnect address and key with a connection test, "Create Lapis deck",
//! the deck and note type, the template of each field, duplicates, tags and
//! media. The settings being edited live here; Save sends them to the session,
//! which writes `config.toml` and gives them to the `anki` worker.

use std::collections::BTreeMap;

use egui::{Button, ComboBox, DragValue, RichText, Role, TextEdit, Ui, WidgetInfo, vec2};
use nikomiru_core::config::{AnkiConfig, ReviewConfig};

use super::popups::Translate;
use super::settings::{self, Leave, SettingsTab};
use super::{View, palette, theme};
use crate::mining::AnkiPage;
use crate::session::Session;
use crate::ui::Command;

/// The placeholders of ANK-021, for the picker of each field.
pub(crate) const PLACEHOLDERS: [&str; 20] = [
    "expression",
    "reading",
    "furigana-plain",
    "glossary",
    "glossary-first",
    "sentence",
    "cue-sentence",
    "sentence-audio",
    "card-image",
    "audio",
    "frequencies",
    "frequency-harmonic-rank",
    "pitch-accent-positions",
    "pitch-accent-categories",
    "phonetic-transcriptions",
    "tags",
    "document-title",
    "source-link",
    "clip-timestamp",
    "single-glossary-",
];

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The settings being edited.
    draft: Option<AnkiConfig>,
    /// `[review]` being edited (REV-003).
    review: Option<ReviewConfig>,
    /// The tags as typed.
    tags: String,
    /// The API key is shown.
    show_key: bool,
    /// The "Create Lapis deck" already applied.
    seen_lapis: u64,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The page's state in the session.
    pub page: &'a AnkiPage,
    /// The settings as saved.
    pub saved: &'a AnkiConfig,
    /// `[review]` as saved.
    pub saved_review: &'a ReviewConfig,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.anki_page,
        saved: s.anki_config(),
        saved_review: &s.config().review,
        tr: &tr,
    };
    let leave = draw(ctx, &scene, &mut view.anki, out);
    view.leave(leave, out);
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

fn section(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add_space(6.0);
    ui.label(
        RichText::new(text)
            .size(15.0)
            .family(theme::bold())
            .color(palette::current().details),
    )
}

/// A combo box over `options`, named for screen readers.
fn choice(ui: &mut Ui, id: &str, name: &str, value: &mut String, options: &[(String, String)]) {
    let shown = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or_else(|| value.clone(), |(_, label)| label.clone());
    let combo = ComboBox::from_id_salt(id)
        .width(260.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, label) in options {
                ui.selectable_value(value, v.clone(), label);
            }
        });
    let name = name.to_string();
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &name));
}

/// Brings the settings being edited up to date with the session: a Lapis
/// deck just made is chosen, a note type without templates gets the Lapis
/// defaults.
fn update(s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let page = s.page;
    if state.draft.is_none() {
        // the page opens on the saved settings; a Lapis deck made before
        // is in them already
        state.draft = Some(s.saved.clone());
        state.review = Some(s.saved_review.clone());
        state.tags = s.saved.tags.join(" ");
        state.seen_lapis = page.lapis_created;
    }
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    if page.lapis_created != state.seen_lapis {
        state.seen_lapis = page.lapis_created;
        if let Some((deck, note_type)) = &page.lapis {
            draft.deck = deck.clone();
            draft.note_type = note_type.clone();
        }
        out.push(Command::AnkiTest(draft.clone()));
    }
    if let Some(lists) = &page.lists
        && lists.model == draft.note_type
        && !lists.fields.is_empty()
        && draft
            .field_mapping
            .get(&draft.note_type)
            .is_none_or(BTreeMap::is_empty)
    {
        let defaults =
            nikomiru_anki::lapis::with_defaults(&draft.note_type, &lists.fields, &BTreeMap::new());
        draft
            .field_mapping
            .insert(draft.note_type.clone(), defaults);
    }
}

/// Draws the page; returns what it asks for.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> Leave {
    update(s, state, out);
    let page = s.page;
    let tr = |id: &str| tr(s, id);
    settings::page(
        ctx,
        SettingsTab::Anki,
        &tr,
        page.status.as_ref(),
        |buttons, ui| {
            let save = buttons.add_enabled(!page.busy, palette::suggested(tr("anki-save")));
            if save.clicked()
                && let Some(draft) = state.draft.as_mut()
            {
                draft.tags = state.tags.split_whitespace().map(str::to_string).collect();
                if let Some(review) = &state.review {
                    out.push(Command::ReviewSave(review.clone()));
                }
                out.push(Command::AnkiSave(draft.clone()));
            }
            if page.busy {
                buttons.spinner();
            }
            body(ui, s, state, out);
        },
    )
}

/// The cards kept for later (ANK-044): each with Remove, and Send to Anki.
fn later(ui: &mut Ui, s: &Scene<'_>, chosen: bool, out: &mut Vec<Command>) {
    let page = s.page;
    let colors = palette::current();
    if chosen {
        ui.add(
            egui::Label::new(
                RichText::new(tr(s, "anki-destination-later-hint")).color(colors.details),
            )
            .wrap(),
        );
    }
    let count = page.later.len().to_string();
    section(ui, &(s.tr)("anki-later", &[("count", count)]));
    if page.later.is_empty() {
        ui.label(RichText::new(tr(s, "anki-later-none")).color(colors.details));
    }
    for card in &page.later {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!page.later_busy, Button::new(tr(s, "anki-later-remove")))
                .clicked()
            {
                out.push(Command::LaterForget(card.id.clone()));
            }
            let text = if card.damaged {
                tr(s, "anki-later-damaged")
            } else if card.reading.is_empty() || card.reading == card.expression {
                format!("{} · {}", card.expression, card.sentence)
            } else {
                format!(
                    "{}【{}】 · {}",
                    card.expression, card.reading, card.sentence
                )
            };
            ui.add(egui::Label::new(text).truncate());
        });
        if let Some(problem) = &card.problem {
            ui.label(RichText::new(problem).color(colors.bad).size(12.0));
        }
    }
    ui.horizontal(|ui| {
        let any = page.later.iter().any(|c| !c.damaged);
        if ui
            .add_enabled(
                any && !page.later_busy,
                Button::new(tr(s, "anki-later-send")),
            )
            .clicked()
        {
            out.push(Command::LaterSend);
        }
        if page.later_busy {
            ui.spinner();
        }
    });
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let page = s.page;
    let Some(draft) = state.draft.as_mut() else {
        return;
    };
    ui.spacing_mut().item_spacing.y = 8.0;

    // REV-001
    ui.horizontal(|ui| {
        let name = tr(s, "anki-destination");
        ui.label(&name);
        choice(
            ui,
            "anki-destination",
            &name,
            &mut draft.destination,
            &[
                ("anki".into(), tr(s, "anki-destination-anki")),
                ("anki-later".into(), tr(s, "anki-destination-later")),
                ("nikomiru".into(), tr(s, "anki-destination-nikomiru")),
            ],
        );
    });
    if draft.to_review() {
        ui.add(
            egui::Label::new(
                RichText::new(tr(s, "anki-destination-hint")).color(palette::current().details),
            )
            .wrap(),
        );
        if let Some(review) = state.review.as_mut() {
            section(ui, &tr(s, "review-settings"));
            egui::Grid::new("review-settings")
                .num_columns(2)
                .spacing(vec2(12.0, 8.0))
                .show(ui, |ui| {
                    let label = ui.label(tr(s, "review-retention"));
                    let range = ReviewConfig::RETENTION;
                    ui.add(
                        egui::Slider::new(&mut review.retention, range)
                            .step_by(0.01)
                            .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                    let label = ui.label(tr(s, "review-new-per-day"));
                    ui.add(DragValue::new(&mut review.new_per_day).range(0..=9999))
                        .labelled_by(label.id);
                    ui.end_row();
                });
        }
    }

    // ANK-044
    if draft.later() || !page.later.is_empty() {
        later(ui, s, draft.later(), out);
    }

    section(ui, &tr(s, "anki-connection"));
    egui::Grid::new("anki-connection")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            let label = ui.label(tr(s, "anki-url"));
            ui.add(TextEdit::singleline(&mut draft.url).desired_width(320.0))
                .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(tr(s, "anki-key"));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut draft.api_key)
                        .password(!state.show_key)
                        .desired_width(240.0),
                )
                .labelled_by(label.id);
                ui.checkbox(&mut state.show_key, tr(s, "anki-show-key"));
            });
            ui.end_row();
        });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!page.busy, Button::new(tr(s, "anki-test")))
            .clicked()
        {
            out.push(Command::AnkiTest(draft.clone()));
        }
        if ui
            .add_enabled(!page.busy, Button::new(tr(s, "anki-create-lapis")))
            .clicked()
        {
            out.push(Command::AnkiCreateLapis(draft.clone()));
        }
    });

    section(ui, &tr(s, "anki-note"));
    let list = |items: Option<&Vec<String>>, current: &str| -> Vec<(String, String)> {
        let mut items: Vec<String> = items.cloned().unwrap_or_default();
        if !items.iter().any(|i| i == current) {
            items.insert(0, current.to_string());
        }
        items.into_iter().map(|i| (i.clone(), i)).collect()
    };
    let lists = page.lists.as_ref();
    egui::Grid::new("anki-note")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            ui.label(tr(s, "anki-deck"));
            let decks = list(lists.map(|l| &l.decks), &draft.deck);
            choice(
                ui,
                "anki-deck",
                &tr(s, "anki-deck"),
                &mut draft.deck,
                &decks,
            );
            ui.end_row();
            ui.label(tr(s, "anki-note-type"));
            let before = draft.note_type.clone();
            let models = list(lists.map(|l| &l.models), &draft.note_type);
            choice(
                ui,
                "anki-note-type",
                &tr(s, "anki-note-type"),
                &mut draft.note_type,
                &models,
            );
            if draft.note_type != before {
                // its fields
                out.push(Command::AnkiTest(draft.clone()));
            }
            ui.end_row();
        });

    section(ui, &tr(s, "anki-fields"));
    let fields: Vec<String> = lists
        .filter(|l| l.model == draft.note_type)
        .map(|l| l.fields.clone())
        .unwrap_or_default();
    if fields.is_empty() {
        ui.add(
            egui::Label::new(
                RichText::new(tr(s, "anki-fields-hint")).color(palette::current().details),
            )
            .wrap(),
        );
    } else {
        let note_type = draft.note_type.clone();
        let mapping = draft.field_mapping.entry(note_type.clone()).or_default();
        egui::Grid::new("anki-fields")
            .num_columns(3)
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                for field in &fields {
                    let label = ui.label(field);
                    let template = mapping.entry(field.clone()).or_default();
                    // a grid cell would shrink it to its content
                    ui.add_sized(vec2(320.0, 22.0), TextEdit::singleline(template))
                        .labelled_by(label.id);
                    ui.menu_button("{…}", |ui| {
                        for p in PLACEHOLDERS {
                            if ui.button(format!("{{{p}}}")).clicked() {
                                template.push_str(&format!("{{{p}}}"));
                                ui.close();
                            }
                        }
                    })
                    .response
                    .on_hover_text(tr(s, "anki-insert"));
                    ui.end_row();
                }
            });
        mapping.retain(|_, template| !template.trim().is_empty());
        if nikomiru_anki::lapis::is_lapis(&note_type, &fields)
            && ui.button(tr(s, "anki-lapis-defaults")).clicked()
        {
            *mapping = nikomiru_anki::lapis::with_defaults(&note_type, &fields, &BTreeMap::new());
        }
    }

    section(ui, &tr(s, "anki-duplicates"));
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut draft.duplicate_scope,
            "deck".to_string(),
            tr(s, "anki-scope-deck"),
        );
        ui.radio_value(
            &mut draft.duplicate_scope,
            "collection".to_string(),
            tr(s, "anki-scope-collection"),
        );
    });

    let label = section(ui, &tr(s, "anki-tags"));
    ui.add(TextEdit::singleline(&mut state.tags).desired_width(320.0))
        .labelled_by(label.id);

    section(ui, &tr(s, "anki-media"));
    egui::Grid::new("anki-media")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            let max = nikomiru_core::config::AnkiConfig::MAX_PAD_MS;
            let label = ui.label(tr(s, "anki-pad-head"));
            ui.add(
                DragValue::new(&mut draft.audio_pad_head_ms)
                    .range(0..=max)
                    .speed(10),
            )
            .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(tr(s, "anki-pad-tail"));
            ui.add(
                DragValue::new(&mut draft.audio_pad_tail_ms)
                    .range(0..=max)
                    .speed(10),
            )
            .labelled_by(label.id);
            ui.end_row();
            ui.label(tr(s, "anki-audio-format"));
            let formats: Vec<(String, String)> = ["aac-m4a", "opus-ogg", "mp3"]
                .iter()
                .map(|f| ((*f).to_string(), (*f).to_string()))
                .collect();
            choice(
                ui,
                "anki-audio-format",
                &tr(s, "anki-audio-format"),
                &mut draft.audio_format,
                &formats,
            );
            ui.end_row();
            ui.label(tr(s, "anki-audio-quality"));
            let mut level = draft.audio_quality_level.to_string();
            let levels: Vec<(String, String)> = (0..3)
                .map(|n| (n.to_string(), tr(s, &format!("anki-audio-q{n}"))))
                .collect();
            choice(
                ui,
                "anki-audio-quality",
                &tr(s, "anki-audio-quality"),
                &mut level,
                &levels,
            );
            draft.audio_quality_level = level.parse().unwrap_or(0);
            ui.end_row();
            ui.label(tr(s, "anki-image-format"));
            let formats: Vec<(String, String)> = [
                ("jpeg", "JPEG".to_string()),
                ("webp", "WebP".to_string()),
                ("webp-animated", tr(s, "anki-image-webp-animated")),
                ("avif-animated", tr(s, "anki-image-avif-animated")),
                ("webm", tr(s, "anki-image-webm")),
            ]
            .into_iter()
            .map(|(f, label)| (f.to_string(), label))
            .collect();
            choice(
                ui,
                "anki-image-format",
                &tr(s, "anki-image-format"),
                &mut draft.image_format,
                &formats,
            );
            ui.end_row();
            ui.label(tr(s, "anki-image-quality"));
            let mut level = draft.image_quality_level.to_string();
            // an animated picture has its own sizes and frame rates (ANK-042)
            let animated = draft.image_format.ends_with("-animated");
            let levels: Vec<(String, String)> = (0..4)
                .map(|n| {
                    let key = if animated {
                        format!("anki-image-a{n}")
                    } else {
                        format!("anki-image-q{n}")
                    };
                    (n.to_string(), tr(s, &key))
                })
                .collect();
            choice(
                ui,
                "anki-image-quality",
                &tr(s, "anki-image-quality"),
                &mut level,
                &levels,
            );
            draft.image_quality_level = level.parse().unwrap_or(1);
            ui.end_row();
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mining::AnkiLists;
    use egui::{Id, Sense};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        page: AnkiPage,
        saved: AnkiConfig,
        state: State,
        out: Vec<Command>,
    }

    fn harness(page: AnkiPage) -> Harness<'static, Test> {
        with_saved(page, AnkiConfig::default())
    }

    fn with_saved(page: AnkiPage, saved: AnkiConfig) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let review = ReviewConfig::default();
                    let scene = Scene {
                        page: &t.page,
                        saved: &t.saved,
                        saved_review: &review,
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

    fn lapis_lists() -> AnkiLists {
        AnkiLists {
            version: 6,
            decks: vec!["Default".into(), "Lapis".into()],
            models: vec!["Basic".into(), "Lapis".into()],
            deck: "Lapis".into(),
            model: "Lapis".into(),
            fields: nikomiru_anki::lapis::FIELDS
                .iter()
                .map(|f| (*f).to_string())
                .collect(),
        }
    }

    // T-ANK-044: the cards kept, Remove and Send to Anki
    #[test]
    fn the_cards_kept_for_later_can_be_removed_and_sent() {
        use crate::mining::LaterCard;
        let page = AnkiPage {
            later: vec![
                LaterCard {
                    id: "1".into(),
                    expression: "食べる".into(),
                    reading: "たべる".into(),
                    sentence: "ご飯を食べた".into(),
                    problem: Some("Anki has this card already".into()),
                    damaged: false,
                },
                LaterCard {
                    id: "2".into(),
                    damaged: true,
                    ..LaterCard::default()
                },
            ],
            ..AnkiPage::default()
        };
        let mut h = harness(page);
        h.run();
        assert!(
            h.query_by_label_contains("食べる【たべる】 · ご飯を食べた")
                .is_some()
        );
        assert!(h.query_by_label("Anki has this card already").is_some());
        assert!(h.query_by_label("anki-later-damaged").is_some());
        let removes: Vec<_> = h.get_all_by_label("anki-later-remove").collect();
        assert_eq!(removes.len(), 2);
        removes[1].click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(out.as_slice(), [Command::LaterForget(id)] if id == "2"),
            "{out:?}"
        );
        let out = click(&mut h, "anki-later-send");
        assert!(matches!(out.as_slice(), [Command::LaterSend]), "{out:?}");
        // only damaged ones: nothing to send; none: the section is gone
        h.state_mut().page.later.remove(0);
        h.run();
        assert!(click(&mut h, "anki-later-send").is_empty());
        h.state_mut().page.later.clear();
        h.run();
        assert!(h.query_by_label("anki-later-send").is_none());
    }

    // T-ANK-006, T-ANK-012
    #[test]
    fn the_fields_of_lapis_get_the_defaults_and_save_sends_them() {
        let page = AnkiPage {
            lists: Some(lapis_lists()),
            status: Some(("anki-connected".into(), true)),
            ..AnkiPage::default()
        };
        let mut h = harness(page);
        h.run();
        // the 22 fields, the defaults in them
        for field in ["Expression", "MiscInfo", "Hint"] {
            assert!(h.query_by_label(field).is_some(), "{field}");
        }
        let draft = h.state().state.draft.clone().unwrap();
        assert_eq!(draft.field_mapping["Lapis"]["Expression"], "{expression}");
        assert_eq!(draft.field_mapping["Lapis"].len(), 15);
        let out = click(&mut h, "anki-save");
        match &out[..] {
            [Command::ReviewSave(_), Command::AnkiSave(saved)] => {
                assert_eq!(saved.field_mapping["Lapis"]["Sentence"], "{sentence}");
                assert_eq!(saved.url, "http://127.0.0.1:8765");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            &click(&mut h, "anki-test")[..],
            [Command::AnkiTest(_)]
        ));
        assert!(matches!(
            &click(&mut h, "anki-create-lapis")[..],
            [Command::AnkiCreateLapis(_)]
        ));
    }

    // T-ANK-011
    #[test]
    fn reopening_the_page_does_not_choose_lapis_again_and_tags_can_be_cleared() {
        let page = AnkiPage {
            lapis_created: 3,
            lapis: Some(("Lapis".into(), "Lapis".into())),
            ..AnkiPage::default()
        };
        let saved = AnkiConfig {
            deck: "Mining".into(),
            tags: vec!["mine".into()],
            ..AnkiConfig::default()
        };
        let mut h = with_saved(page, saved);
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(out.is_empty(), "{out:?}");
        let draft = h.state().state.draft.clone().unwrap();
        assert_eq!(draft.deck, "Mining");
        assert_eq!(h.state().state.tags, "mine");
        // emptied, the field stays empty
        h.state_mut().state.tags.clear();
        h.run();
        assert_eq!(h.state().state.tags, "");
        match &click(&mut h, "anki-save")[..] {
            [Command::ReviewSave(_), Command::AnkiSave(saved)] => assert!(saved.tags.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    // T-ANK-011
    #[test]
    fn a_lapis_deck_just_made_is_chosen() {
        let mut h = harness(AnkiPage::default());
        h.state_mut().saved.deck = "Mining".into();
        h.state_mut().saved.note_type = "Basic".into();
        h.run();
        assert!(
            h.query_by_label("anki-fields-hint").is_some(),
            "no fields yet"
        );
        h.state_mut().page.lapis_created = 1;
        h.state_mut().page.lapis = Some(("Lapis".into(), "Lapis".into()));
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            out.iter().any(
                |c| matches!(c, Command::AnkiTest(c) if c.note_type == "Lapis" && c.deck == "Lapis")
            ),
            "{out:?}"
        );
    }
}
