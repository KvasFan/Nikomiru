// SPDX-License-Identifier: GPL-3.0-only

//! The setup assistant (INS-001…INS-006): the free dictionaries of the
//! catalogue, the audio libraries of Fushi's pack, and how cards are made,
//! as Fushi's welcome wizard asks. Everything it does goes through the
//! session; the page keeps nothing of its own.

use egui::{Area, Button, Frame, Id, Margin, Order, RichText, ScrollArea, Ui, vec2};
use nikomiru_core::config::AnkiConfig;

use super::{palette, theme};
use crate::mining::AnkiPage;
use crate::session::Session;
use crate::setup::{ItemState, SetupPage};
use crate::ui::Command;

/// The interface strings.
type Translate<'a> = dyn Fn(&str, &[(&str, String)]) -> String + 'a;

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The assistant's state.
    pub page: &'a SetupPage,
    /// The catalogue's dictionaries installed already.
    pub installed: std::collections::BTreeSet<String>,
    /// The Anki settings as saved.
    pub anki: &'a AnkiConfig,
    /// What AnkiConnect said last.
    pub anki_page: &'a AnkiPage,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.setup_page,
        installed: s.setup_installed(),
        anki: s.anki_config(),
        anki_page: &s.anki_page,
        tr: &tr,
    };
    draw(ctx, &scene, out);
}

/// A size as people read it.
pub(crate) fn size(bytes: u64) -> String {
    let mb = bytes as f64 / 1e6;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else {
        format!("{mb:.0} MB")
    }
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(text)
            .size(17.0)
            .family(theme::bold())
            .color(palette::current().details),
    );
}

fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).color(palette::current().details)).wrap());
}

/// Where an item is, in words; `installed` for a dictionary there already.
fn state_text(s: &Scene<'_>, id: &str, installed: bool) -> Option<(String, bool)> {
    let tr = |key: &str, args: &[(&str, String)]| (s.tr)(key, args);
    if installed {
        return Some((tr("setup-state-installed", &[]), true));
    }
    Some(match s.page.items.get(id)? {
        ItemState::Idle => return None,
        ItemState::Waiting => (tr("setup-state-waiting", &[]), true),
        ItemState::Downloading { done, total } => {
            let percent = if *total > 0 { done * 100 / total } else { 0 };
            (
                tr(
                    "setup-state-downloading",
                    &[("percent", percent.to_string()), ("size", size(*total))],
                ),
                true,
            )
        }
        ItemState::Importing => (tr("setup-state-importing", &[]), true),
        ItemState::Done => (tr("setup-state-done", &[]), true),
        ItemState::Failed(message) => (message.clone(), false),
    })
}

/// A row: its box, its name and details, where it is.
fn item(
    ui: &mut Ui,
    s: &Scene<'_>,
    id: &str,
    name: &str,
    details: &str,
    installed: bool,
    out: &mut Vec<Command>,
) {
    ui.horizontal(|ui| {
        let mut on = s.page.ticked.contains(id) && !installed;
        let enabled = !installed && !s.page.running;
        if ui
            .add_enabled(enabled, egui::Checkbox::new(&mut on, name))
            .changed()
        {
            out.push(Command::SetupTick {
                id: id.to_string(),
                on,
            });
        }
        ui.label(RichText::new(details).color(palette::current().details));
        if let Some((text, good)) = state_text(s, id, installed) {
            let colors = palette::current();
            ui.label(RichText::new(text).color(if good { colors.good } else { colors.bad }));
        }
    });
}

fn dictionaries(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let tr = |key: &str| (s.tr)(key, &[]);
    section(ui, &tr("setup-dictionaries"));
    note(ui, &tr("setup-dictionaries-hint"));
    for d in &s.page.catalog.dictionaries {
        let license = if d.license.is_empty() {
            tr("setup-license-unstated")
        } else {
            d.license.clone()
        };
        let details = format!("{} · {license} · {}", size(d.size_bytes), d.source);
        let installed = s.installed.contains(&d.id);
        item(ui, s, &d.id, &d.name, &details, installed, out);
    }
}

fn audio(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let tr = |key: &str| (s.tr)(key, &[]);
    section(ui, &tr("setup-audio"));
    note(ui, &tr("setup-audio-hint"));
    for a in &s.page.catalog.audio {
        let name = tr(match a.kind.as_str() {
            "english" => "setup-audio-english",
            _ => "setup-audio-japanese",
        });
        let details = format!("{} · {}", size(a.size_bytes), a.name);
        item(ui, s, &a.id, &name, &details, false, out);
    }
}

fn buttons(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let tr = |key: &str| (s.tr)(key, &[]);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let plan = s.page.plan(&s.installed);
        let any = !(plan.dictionaries.is_empty() && plan.audio.is_empty());
        if ui
            .add_enabled(
                any && !s.page.running,
                palette::suggested(tr("setup-install")),
            )
            .clicked()
        {
            out.push(Command::SetupInstall);
        }
        if s.page.running {
            if ui.button(tr("setup-stop")).clicked() {
                out.push(Command::SetupStop);
            }
            ui.spinner();
        } else if s.page.busy() {
            ui.spinner();
        }
    });
}

/// How cards are made (INS-005), as Fushi's wizard asks.
fn cards(ui: &mut Ui, s: &Scene<'_>, out: &mut Vec<Command>) {
    let tr = |key: &str| (s.tr)(key, &[]);
    section(ui, &tr("setup-cards"));
    let review = s.anki.to_review();
    ui.horizontal(|ui| {
        if ui.radio(!review, tr("setup-cards-anki")).clicked() && review {
            let mut config = s.anki.clone();
            config.destination = "anki".into();
            out.push(Command::AnkiSave(config.clone()));
            out.push(Command::AnkiTest(config));
        }
        if ui.radio(review, tr("setup-cards-review")).clicked() && !review {
            let mut config = s.anki.clone();
            config.destination = "nikomiru".into();
            out.push(Command::AnkiSave(config));
        }
    });
    if review {
        note(ui, &tr("anki-destination-hint"));
        return;
    }
    let page = s.anki_page;
    let colors = palette::current();
    match &page.lists {
        Some(lists) => {
            let has_lapis = lists.models.iter().any(|m| m == "Lapis");
            let using_lapis = s.anki.note_type == "Lapis";
            ui.label(
                RichText::new((s.tr)(
                    "setup-anki-answers",
                    &[("version", lists.version.to_string())],
                ))
                .color(colors.good),
            );
            if using_lapis && has_lapis {
                ui.label(RichText::new(tr("setup-anki-lapis-in-use")).color(colors.good));
            } else if has_lapis {
                if ui
                    .add(palette::suggested(tr("setup-anki-use-lapis")))
                    .clicked()
                {
                    let mut config = s.anki.clone();
                    config.note_type = "Lapis".into();
                    if lists.decks.iter().any(|d| d == "Lapis") {
                        config.deck = "Lapis".into();
                    }
                    out.push(Command::AnkiSave(config));
                }
            } else if ui
                .add_enabled(!page.busy, palette::suggested(tr("anki-create-lapis")))
                .clicked()
            {
                out.push(Command::AnkiCreateLapis(s.anki.clone()));
            }
        }
        None if page.busy => {
            ui.spinner();
        }
        None => {
            ui.label(RichText::new(tr("setup-anki-away")).color(colors.bad));
            note(ui, &tr("setup-anki-steps"));
            ui.horizontal(|ui| {
                if ui.button(tr("setup-anki-retry")).clicked() {
                    out.push(Command::AnkiTest(s.anki.clone()));
                }
                if !s.anki.later() && ui.button(tr("setup-anki-later")).clicked() {
                    let mut config = s.anki.clone();
                    config.destination = "anki-later".into();
                    out.push(Command::AnkiSave(config));
                }
            });
        }
    }
    if let Some((text, good)) = &page.status
        && page.lists.is_none()
        && !page.busy
    {
        let color = if *good { colors.good } else { colors.details };
        ui.label(RichText::new(text).color(color).size(12.0));
    }
}

/// Draws the page.
pub(crate) fn draw(ctx: &egui::Context, s: &Scene<'_>, out: &mut Vec<Command>) {
    let screen = ctx.content_rect();
    Area::new(Id::unique("setup-page"))
        .order(Order::Middle)
        .fade_in(false)
        .constrain(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            Frame::new()
                .fill(palette::current().page)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_min_size(screen.size() - vec2(48.0, 48.0));
                    ui.set_max_size(screen.size() - vec2(48.0, 48.0));
                    ui.visuals_mut().override_text_color = Some(palette::current().text);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        ui.heading((s.tr)("setup-title", &[]));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(Button::new((s.tr)("setup-close", &[]))).clicked() {
                                out.push(Command::SetupClose);
                            }
                        });
                    });
                    note(ui, &(s.tr)("setup-intro", &[]));
                    ScrollArea::vertical()
                        .id_salt("setup-scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_max_width(820.0);
                            dictionaries(ui, s, out);
                            audio(ui, s, out);
                            buttons(ui, s, out);
                            cards(ui, s, out);
                        });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    struct Test {
        page: SetupPage,
        installed: std::collections::BTreeSet<String>,
        anki: AnkiConfig,
        anki_page: AnkiPage,
        out: Vec<Command>,
    }

    fn harness(test: Test) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1100.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        installed: t.installed.clone(),
                        anki: &t.anki,
                        anki_page: &t.anki_page,
                        tr: &tr,
                    };
                    draw(ui.ctx(), &scene, &mut t.out);
                },
                test,
            )
    }

    fn test() -> Test {
        Test {
            page: SetupPage::default(),
            installed: ["jitendex".to_string()].into(),
            anki: AnkiConfig::default(),
            anki_page: AnkiPage::default(),
            out: Vec::new(),
        }
    }

    fn click(h: &mut Harness<'_, Test>, label: &str) -> Vec<Command> {
        h.get_by_label(label).click();
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    // T-INS-001, T-INS-002: the items, an installed one marked; Install
    #[test]
    fn the_items_and_install() {
        let mut h = harness(test());
        h.run();
        assert!(h.query_by_label("setup-state-installed").is_some());
        for name in [
            "KANJIDIC",
            "JMnedict",
            "setup-audio-japanese",
            "setup-audio-english",
        ] {
            assert!(h.query_by_label(name).is_some(), "{name}");
        }
        let out = click(&mut h, "JMnedict");
        assert!(
            matches!(out.as_slice(), [Command::SetupTick { id, on: true }] if id == "jmnedict"),
            "{out:?}"
        );
        let out = click(&mut h, "setup-install");
        assert!(matches!(out.as_slice(), [Command::SetupInstall]), "{out:?}");
        assert!(matches!(
            click(&mut h, "setup-close").as_slice(),
            [Command::SetupClose]
        ));
    }

    // T-INS-005: Lapis found and chosen in one click; made when missing;
    // Anki away: try again or "Anki, later"; the review chosen
    #[test]
    fn the_cards_as_fushi_asks() {
        let lists = |models: &[&str]| crate::mining::AnkiLists {
            version: 6,
            decks: vec!["Default".into(), "Lapis".into()],
            models: models.iter().map(|m| (*m).to_string()).collect(),
            deck: "Default".into(),
            model: "Basic".into(),
            fields: Vec::new(),
        };
        let mut t = test();
        t.anki.note_type = "Basic".into();
        t.anki_page.lists = Some(lists(&["Basic", "Lapis"]));
        let mut h = harness(t);
        h.run();
        let out = click(&mut h, "setup-anki-use-lapis");
        assert!(
            matches!(out.as_slice(), [Command::AnkiSave(c)] if c.note_type == "Lapis" && c.deck == "Lapis"),
            "{out:?}"
        );
        h.state_mut().anki_page.lists = Some(lists(&["Basic"]));
        h.run();
        let out = click(&mut h, "anki-create-lapis");
        assert!(
            matches!(out.as_slice(), [Command::AnkiCreateLapis(_)]),
            "{out:?}"
        );
        h.state_mut().anki_page.lists = None;
        h.run();
        assert!(matches!(
            click(&mut h, "setup-anki-retry").as_slice(),
            [Command::AnkiTest(_)]
        ));
        let out = click(&mut h, "setup-anki-later");
        assert!(
            matches!(out.as_slice(), [Command::AnkiSave(c)] if c.destination == "anki-later"),
            "{out:?}"
        );
        let out = click(&mut h, "setup-cards-review");
        assert!(
            matches!(out.as_slice(), [Command::AnkiSave(c)] if c.destination == "nikomiru"),
            "{out:?}"
        );
    }
}
