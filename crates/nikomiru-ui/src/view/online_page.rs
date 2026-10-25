// SPDX-License-Identifier: GPL-3.0-only

//! The Online subtitles page (MET-009, ADR 0008 §12): the search of the video
//! playing, its results with "Use", and the online settings.

use std::path::PathBuf;

use egui::{
    Align, Button, Color32, Frame, Layout, Margin, Rect, RichText, ScrollArea, Stroke, TextEdit,
    Ui, UiBuilder, vec2,
};
use nikomiru_core::config::{NetworkConfig, OnlineConfig};
use nikomiru_online::{Candidate, FileKind};

use super::dict_page::full_screen;
use super::popups::Translate;
use super::{View, theme};
use crate::online::OnlinePage;
use crate::session::Session;
use crate::ui::Command;

const BACKDROP: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 0xa6);
const CARD: Color32 = Color32::from_rgba_unmultiplied_const(0x20, 0x22, 0x27, 0xf0);
const CARD_BORDER: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x40);
const ROW: Color32 = Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x0c);
const DETAILS: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
const GOOD: Color32 = Color32::from_rgb(0x7c, 0xd3, 0x8a);
const BAD: Color32 = Color32::from_rgb(0xf0, 0xb3, 0x5a);

/// The results shown at most.
const ROWS: usize = 100;

/// What the page keeps between frames; dropped when it closes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The video whose search fills the fields.
    query_for: Option<PathBuf>,
    title: String,
    season: String,
    episode: String,
    /// The settings being edited.
    draft: Option<(OnlineConfig, NetworkConfig)>,
    /// The keys and the password are shown.
    show_keys: bool,
    /// The files loaded from the page when it opened: one more closes it.
    seen_loaded: Option<u64>,
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The page's state in the session.
    pub page: &'a OnlinePage,
    /// The online settings as saved.
    pub saved: (&'a OnlineConfig, &'a NetworkConfig),
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.online_page,
        saved: s.online_config(),
        tr: &tr,
    };
    if draw(ctx, &scene, &mut view.online, out) {
        view.close_online_page();
    }
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

fn number(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

/// Brings the page's fields up to date with the session; returns whether a
/// file was just loaded from the page, which closes it.
fn update(s: &Scene<'_>, state: &mut State) -> bool {
    if state.draft.is_none() {
        state.draft = Some((s.saved.0.clone(), s.saved.1.clone()));
    }
    if let Some(q) = &s.page.query
        && state.query_for.as_ref() != Some(&q.video)
    {
        state.query_for = Some(q.video.clone());
        state.title.clone_from(&q.title);
        state.season = q.season.map(|n| n.to_string()).unwrap_or_default();
        state.episode = q.episode.map(|n| n.to_string()).unwrap_or_default();
    }
    let seen = *state.seen_loaded.get_or_insert(s.page.loaded);
    s.page.loaded != seen
}

/// Draws the page; returns whether it closes.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> bool {
    let mut close = update(s, state);
    let page = s.page;
    let screen = ctx.content_rect();
    let width = 820.0_f32.min(screen.width() - 32.0).max(0.0);
    let height = 680.0_f32.min(screen.height() - 32.0).max(0.0);
    full_screen(ctx, "online-page", BACKDROP, |ui| {
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
                    ui.label(
                        RichText::new(tr(s, "online-page-title"))
                            .size(22.0)
                            .family(theme::bold()),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(tr(s, "online-close")).clicked() {
                            close = true;
                        }
                        if page.busy {
                            ui.spinner();
                        }
                    });
                });
                if let Some((text, ok)) = &page.status {
                    ui.add(
                        egui::Label::new(RichText::new(text).color(if *ok { GOOD } else { BAD }))
                            .wrap(),
                    );
                }
                ScrollArea::vertical()
                    .id_salt("online-page")
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, s, state, out));
            });
    });
    close
}

fn body(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let page = s.page;
    ui.spacing_mut().item_spacing.y = 8.0;
    section(ui, &tr(s, "online-search-section"));
    let mut search = false;
    egui::Grid::new("online-search")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            let label = ui.label(tr(s, "online-title"));
            let title = ui
                .add(TextEdit::singleline(&mut state.title).desired_width(420.0))
                .labelled_by(label.id);
            search |= title.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.end_row();
            let label = ui.label(tr(s, "online-season"));
            ui.add(TextEdit::singleline(&mut state.season).desired_width(60.0))
                .labelled_by(label.id);
            ui.end_row();
            let label = ui.label(tr(s, "online-episode"));
            ui.add(TextEdit::singleline(&mut state.episode).desired_width(60.0))
                .labelled_by(label.id);
            ui.end_row();
        });
    let can_search = !page.busy && !state.title.trim().is_empty();
    search |= ui
        .add_enabled(can_search, Button::new(tr(s, "online-search")))
        .clicked();
    if search && can_search {
        out.push(Command::OnlineSearch {
            title: state.title.trim().to_string(),
            season: number(&state.season),
            episode: number(&state.episode),
        });
    }
    if let Some(results) = &page.results {
        for failure in &results.failed {
            ui.add(egui::Label::new(RichText::new(failure).color(BAD)).wrap());
        }
        for (i, c) in results.candidates.iter().take(ROWS).enumerate() {
            row(ui, s, i, c, page.busy, out);
        }
    }
    settings(ui, s, state, out);
}

/// One result: provider and file, the details, "Use".
fn row(
    ui: &mut Ui,
    s: &Scene<'_>,
    index: usize,
    c: &Candidate,
    busy: bool,
    out: &mut Vec<Command>,
) {
    Frame::new()
        .fill(ROW)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let usable = c.kind != FileKind::Unusable;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let use_it = ui.add_enabled(usable && !busy, Button::new(tr(s, "online-use")));
                    if use_it.clicked() {
                        out.push(Command::OnlineUse(index));
                    }
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&c.file_name).family(theme::bold()))
                                .wrap(),
                        );
                        let mut details = vec![c.provider.name().to_string()];
                        if !c.source.is_empty() && c.source != c.file_name {
                            details.push(c.source.clone());
                        }
                        if let Some(l) = &c.language {
                            details.push(l.clone());
                        }
                        if let Some(e) = c.episode {
                            details.push((s.tr)("online-episode-n", &[("n", e.to_string())]));
                        }
                        match c.kind {
                            FileKind::Zip => details.push(tr(s, "online-archive")),
                            FileKind::Unusable => details.push(tr(s, "online-unusable")),
                            FileKind::Subtitle => {}
                        }
                        if c.machine_translated {
                            details.push(tr(s, "online-machine"));
                        }
                        ui.add(
                            egui::Label::new(RichText::new(details.join(" · ")).color(DETAILS))
                                .wrap(),
                        );
                    });
                });
            });
        });
}

/// The online settings (MET-005, MET-007, SET-007).
fn settings(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let show = state.show_keys;
    let Some((online, network)) = state.draft.as_mut() else {
        return;
    };
    section(ui, &tr(s, "online-settings"));
    ui.checkbox(&mut online.auto_subtitles, tr(s, "online-auto"));
    egui::Grid::new("online-settings")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            let secret = |ui: &mut Ui, name: &str, value: &mut String, hide: bool| {
                let label = ui.label(name);
                ui.add(
                    TextEdit::singleline(value)
                        .password(hide)
                        .desired_width(320.0),
                )
                .labelled_by(label.id);
                ui.end_row();
            };
            secret(
                ui,
                &tr(s, "online-jimaku-key"),
                &mut online.jimaku_api_key,
                !show,
            );
            secret(
                ui,
                &tr(s, "online-os-key"),
                &mut online.opensubtitles_api_key,
                !show,
            );
            secret(
                ui,
                &tr(s, "online-os-user"),
                &mut online.opensubtitles_username,
                false,
            );
            secret(
                ui,
                &tr(s, "online-os-password"),
                &mut online.opensubtitles_password,
                !show,
            );
            secret(ui, &tr(s, "online-proxy"), &mut network.proxy, false);
        });
    ui.checkbox(&mut state.show_keys, tr(s, "online-show-keys"));
    ui.add(egui::Label::new(RichText::new(tr(s, "online-keys-hint")).color(DETAILS)).wrap());
    if ui
        .add_enabled(!s.page.busy, Button::new(tr(s, "online-save")))
        .clicked()
        && let Some(draft) = state.draft.clone()
    {
        out.push(Command::OnlineSave(Box::new(draft)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::online::{OnlineQuery, OnlineResults};
    use egui::{Id, Sense};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_online::{Locator, ProviderId};

    struct Test {
        page: OnlinePage,
        saved: (OnlineConfig, NetworkConfig),
        state: State,
        out: Vec<Command>,
        closed: bool,
    }

    fn harness(page: OnlinePage) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 900.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        saved: (&t.saved.0, &t.saved.1),
                        tr: &tr,
                    };
                    t.closed |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    page,
                    saved: (OnlineConfig::default(), NetworkConfig::default()),
                    state: State::default(),
                    out: Vec::new(),
                    closed: false,
                },
            )
    }

    fn click(h: &mut Harness<'_, Test>, label: &str) -> Vec<Command> {
        h.get_by_label(label).click();
        h.run();
        std::mem::take(&mut h.state_mut().out)
    }

    fn candidate(name: &str, kind: FileKind) -> Candidate {
        Candidate {
            provider: ProviderId::Jimaku,
            locator: Locator::Url(String::new()),
            file_name: name.into(),
            source: "Show".into(),
            language: Some("ja".into()),
            episode: Some(1),
            kind,
            machine_translated: false,
            size: None,
            order: 0,
        }
    }

    // T-MET-009
    #[test]
    fn the_page_searches_uses_a_result_and_saves_the_settings() {
        let page = OnlinePage {
            query: Some(OnlineQuery {
                video: PathBuf::from("/v/Show - 01.mkv"),
                title: "Show".into(),
                season: None,
                episode: Some(1),
            }),
            results: Some(OnlineResults {
                candidates: vec![
                    candidate("Show 01.srt", FileKind::Subtitle),
                    candidate("Show S1.rar", FileKind::Unusable),
                ],
                failed: vec!["OpenSubtitles refused the key".into()],
            }),
            ..OnlinePage::default()
        };
        let mut h = harness(page);
        h.run();
        // the fields come from the file name
        assert_eq!(h.state().state.title, "Show");
        assert_eq!(h.state().state.episode, "1");
        let out = click(&mut h, "online-search");
        assert!(
            matches!(out.as_slice(), [Command::OnlineSearch { title, episode: Some(1), .. }] if title == "Show"),
            "{out:?}"
        );
        h.get_by_label("OpenSubtitles refused the key");
        let uses: Vec<_> = h.query_all_by_label("online-use").collect();
        assert_eq!(uses.len(), 2);
        uses[0].click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(matches!(out.as_slice(), [Command::OnlineUse(0)]), "{out:?}");
        // the RAR cannot be used
        let uses: Vec<_> = h.query_all_by_label("online-use").collect();
        uses[1].click();
        h.run();
        assert!(h.state().out.is_empty());
        // the settings
        h.get_by_label("online-auto").click();
        h.run();
        let out = click(&mut h, "online-save");
        match out.as_slice() {
            [Command::OnlineSave(settings)] => assert!(settings.0.auto_subtitles),
            other => panic!("{other:?}"),
        }
        // a file loaded from the page closes it
        assert!(!h.state().closed);
        h.state_mut().page.loaded += 1;
        h.run();
        assert!(h.state().closed);
    }
}
