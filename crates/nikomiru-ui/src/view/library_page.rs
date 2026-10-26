// SPDX-License-Identifier: GPL-3.0-only

//! The Library page (LIB-005): the first steps until they are done
//! (LIB-013), "Continue watching", the series with their covers and state, a
//! series' episodes with the edits of LIB-011 and LIB-012, and the imported
//! folders with their grouping (LIB-002).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{
    Align, Button, Color32, ComboBox, Frame, Layout, Margin, RichText, Role, ScrollArea, Sense,
    Stroke, TextEdit, TextureHandle, Ui, UiBuilder, WidgetInfo, vec2,
};
use nikomiru_library::home::Watched;
use nikomiru_library::{Grouping, VideoRecord};

use super::dict_page::full_screen;
use super::popups::Translate;
use super::{View, palette, theme};
use crate::catalog::{LibraryEdit, LibraryPage};
use crate::session::Session;
use crate::subtitles::clock;
use crate::ui::Command;

/// The width of a card of "Continue watching", and of a series.
const WIDE: f32 = 232.0;
const NARROW: f32 = 184.0;

/// What the page keeps between frames; dropped when it closes.
#[derive(Default)]
pub(crate) struct State {
    /// The covers as textures, by series.
    textures: HashMap<u64, TextureHandle>,
    /// The name being typed for the series open.
    rename: Option<(u64, String)>,
    /// The series to merge the open one into.
    merge_into: Option<u64>,
    /// The episodes ticked in the series open.
    selected: Vec<String>,
    /// Where the ticked episodes go.
    move_to: Option<u64>,
    /// The name of a series split off.
    split_name: String,
    /// The Statistics tab is shown (STA-005).
    statistics: bool,
    /// What it keeps.
    stats: super::stats_view::State,
    /// The view (home, or a series) whose first card or episode got the
    /// keyboard when it appeared (NFR-A11Y-01).
    focused_view: Option<Option<u64>>,
    /// This frame gives the keyboard to the first card or episode.
    focus_first: bool,
    /// A tab of the settings a first step asked for (LIB-013).
    pub(crate) open_settings: Option<super::settings::SettingsTab>,
}

impl State {
    /// Shows the Statistics tab, or the library.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub(crate) fn show_statistics(&mut self, on: bool) {
        self.statistics = on;
    }

    /// Forgets what was typed and ticked; the covers stay, they cost a
    /// decode each.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            textures: std::mem::take(&mut self.textures),
            ..Self::default()
        };
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("covers", &self.textures.len())
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

/// What the page shows.
pub(crate) struct Scene<'a> {
    /// The page's state in the session.
    pub page: &'a LibraryPage,
    /// Today's local day number, for the statistics.
    pub today: i64,
    /// The first steps and whether each is done (LIB-013), while shown.
    pub first_steps: Option<[bool; 3]>,
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.library_page,
        today: s.today(),
        first_steps: s.first_steps(),
        tr: &tr,
    };
    if draw(ctx, &scene, &mut view.library, out) {
        view.close_library_page();
    }
    if let Some(tab) = view.library.open_settings.take() {
        view.open_settings(tab, out);
    }
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(
        RichText::new(text)
            .size(17.0)
            .family(theme::bold())
            .color(palette::current().details),
    );
}

fn file_title(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `S1 E03`, `E03` or nothing.
fn episode_label(v: &VideoRecord) -> String {
    match (v.season, v.episode) {
        (Some(s), Some(e)) => format!("S{s} E{e:02}"),
        (None, Some(e)) => format!("E{e:02}"),
        _ => String::new(),
    }
}

/// A thin bar filled to `share` (0…1).
fn progress(ui: &mut Ui, width: f32, share: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 2.0, palette::current().track);
    let mut done = rect;
    done.set_width(rect.width() * share.clamp(0.0, 1.0));
    ui.painter().rect_filled(done, 2.0, color);
}

/// Draws the page; returns whether it closes.
pub(crate) fn draw(
    ctx: &egui::Context,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) -> bool {
    for (id, cover) in &s.page.covers {
        if !state.textures.contains_key(id) {
            let image = egui::ColorImage::from_rgba_unmultiplied(cover.size, &cover.rgba);
            let texture = ctx.load_texture(format!("cover-{id}"), image, Default::default());
            state.textures.insert(*id, texture);
        }
    }
    let screen = ctx.content_rect();
    let mut close = false;
    full_screen(ctx, "library-page", palette::current().page, |ui| {
        let area = screen.shrink(24.0);
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(Layout::top_down(Align::Min)),
        );
        ui.visuals_mut().override_text_color = Some(palette::current().text);
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            // the library, or its statistics (STA-005)
            for (statistics, id) in [(false, "library-title"), (true, "stats-title")] {
                let text = RichText::new(tr(s, id)).size(24.0).family(theme::bold());
                if ui
                    .selectable_label(state.statistics == statistics, text)
                    .clicked()
                {
                    state.statistics = statistics;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(tr(s, "library-close")).clicked() {
                    close = true;
                }
                if state.statistics {
                    if ui.button(tr(s, "stats-export")).clicked() {
                        out.push(Command::StatsExport);
                    }
                } else if ui
                    .add_enabled(!s.page.busy, Button::new(tr(s, "library-add-folder")))
                    .clicked()
                {
                    out.push(Command::LibraryAddFolder);
                }
                if s.page.busy {
                    ui.spinner();
                }
            });
        });
        if let Some((text, ok)) = &s.page.status {
            ui.add(
                egui::Label::new(RichText::new(text).color(if *ok {
                    palette::current().good
                } else {
                    palette::current().bad
                }))
                .wrap(),
            );
        }
        ScrollArea::vertical()
            .id_salt("library-page")
            .auto_shrink([false, false])
            .show(&mut ui, |ui| match &s.page.series {
                _ if state.statistics => super::stats_view::draw(
                    ui,
                    s.page.stats.as_ref(),
                    s.today,
                    s.tr,
                    &mut state.stats,
                    out,
                ),
                Some((id, episodes)) => {
                    // the first episode takes the keyboard once listed
                    state.focus_first = episodes.is_some() && state.focused_view != Some(Some(*id));
                    if state.focus_first {
                        state.focused_view = Some(Some(*id));
                    }
                    series(ui, s, state, *id, episodes.as_deref(), out);
                }
                None => {
                    state.focus_first =
                        s.page.home.as_ref().is_some_and(|h| {
                            !h.continue_watching.is_empty() || !h.series.is_empty()
                        }) && state.focused_view != Some(None);
                    if state.focus_first {
                        state.focused_view = Some(None);
                    }
                    home(ui, s, state, out);
                }
            });
    });
    close
}

/// Cards `width` wide in rows that fit the page, one row under the other:
/// a frame cannot wrap in a wrapping layout, which does not know its size
/// before it is drawn.
fn rows<T>(ui: &mut Ui, width: f32, items: &[T], mut each: impl FnMut(&mut Ui, &T)) {
    const GAP: f32 = 12.0;
    // the card's own margins
    let outer = width + 16.0;
    let per_row = (((ui.available_width() + GAP) / (outer + GAP)).floor() as usize).max(1);
    ui.spacing_mut().item_spacing.y = GAP;
    for row in items.chunks(per_row) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for item in row {
                each(ui, item);
            }
        });
    }
}

/// A clickable card with a cover; whether it was clicked.
fn card(
    ui: &mut Ui,
    width: f32,
    texture: Option<&TextureHandle>,
    name: &str,
    focus: &mut bool,
    body: impl FnOnce(&mut Ui),
) -> bool {
    let inner = Frame::new()
        .fill(palette::current().row)
        .corner_radius(8)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            // a card is a column, also in a wrapped row
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                let size = vec2(width, width * 9.0 / 16.0);
                match texture {
                    Some(t) => {
                        ui.add(egui::Image::new(t).fit_to_exact_size(size).corner_radius(6));
                    }
                    None => {
                        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 6.0, palette::current().placeholder);
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "🎞",
                            egui::FontId::proportional(28.0),
                            palette::current().details,
                        );
                    }
                }
                body(ui);
            });
        });
    let response = ui.interact(
        inner.response.rect,
        egui::Id::unique(("library-card", name)),
        Sense::click(),
    );
    if std::mem::take(focus) {
        response.request_focus();
    }
    // the keyboard's place is always visible (NFR-A11Y-01)
    if response.has_focus() {
        ui.painter().rect_stroke(
            inner.response.rect,
            8.0,
            Stroke::new(2.0, palette::current().accent),
            egui::StrokeKind::Outside,
        );
    } else if response.hovered() {
        ui.painter().rect_stroke(
            inner.response.rect,
            8.0,
            Stroke::new(1.0, palette::current().row_hover),
            egui::StrokeKind::Inside,
        );
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    response.clicked()
}

/// The first steps (LIB-013): import a dictionary, add a folder, make a
/// card, each with what does it; hidden for good with "Hide".
fn first_steps(
    ui: &mut Ui,
    s: &Scene<'_>,
    steps: [bool; 3],
    state: &mut State,
    out: &mut Vec<Command>,
) {
    use super::settings::SettingsTab;
    Frame::new()
        .fill(palette::current().row_hover)
        .corner_radius(8.0)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr(s, "first-steps-title"))
                        .size(17.0)
                        .family(theme::bold()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr(s, "first-steps-hide")).clicked() {
                        out.push(Command::HideFirstSteps);
                    }
                });
            });
            let rows = [
                ("first-steps-dictionary", "first-steps-dictionary-button"),
                ("first-steps-folder", "library-add-folder"),
                ("first-steps-card", "first-steps-card-button"),
            ];
            for (n, ((text, button), done)) in rows.into_iter().zip(steps).enumerate() {
                ui.horizontal(|ui| {
                    let mark = if done {
                        "✓".to_string()
                    } else {
                        format!("{}.", n + 1)
                    };
                    ui.label(RichText::new(mark).family(theme::bold()));
                    let line = RichText::new(tr(s, text));
                    ui.label(if done {
                        line.color(palette::current().details)
                    } else {
                        line
                    });
                    if !done && ui.button(tr(s, button)).clicked() {
                        match n {
                            0 => state.open_settings = Some(SettingsTab::Dictionaries),
                            1 => out.push(Command::LibraryAddFolder),
                            _ => state.open_settings = Some(SettingsTab::Anki),
                        }
                    }
                });
            }
        });
    ui.add_space(4.0);
}

fn home(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let Some(home) = &s.page.home else {
        ui.spinner();
        return;
    };
    if let Some(steps) = s.first_steps {
        first_steps(ui, s, steps, state, out);
    }
    if !home.continue_watching.is_empty() {
        section(ui, &tr(s, "library-continue"));
        rows(ui, WIDE, &home.continue_watching, |ui, (uid, v)| {
            let series = v.series.and_then(|id| home.series_title(id));
            let title = series.map_or_else(|| file_title(&v.path), str::to_string);
            let left = v
                .duration_ms
                .map(|d| d.saturating_sub(v.last_position_ms) as i64);
            let left = left.map(|left| (s.tr)("library-left", &[("time", clock(left))]));
            let detail = [Some(episode_label(v)), left]
                .into_iter()
                .flatten()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let texture = v.series.and_then(|id| state.textures.get(&id));
            let name = format!("{title} {}", episode_label(v));
            let clicked = card(ui, WIDE, texture, &name, &mut state.focus_first, |ui| {
                ui.add(egui::Label::new(RichText::new(&title).family(theme::bold())).truncate());
                ui.label(RichText::new(detail).color(palette::current().details));
                let share = v
                    .duration_ms
                    .map_or(0.0, |d| v.last_position_ms as f32 / d.max(1) as f32);
                progress(ui, WIDE, share, palette::current().accent);
            });
            if clicked {
                tracing::debug!("continue {uid}");
                out.push(Command::LibraryPlay(PathBuf::from(&v.path)));
            }
        });
    }
    section(ui, &tr(s, "library-series"));
    if home.series.is_empty() {
        ui.label(RichText::new(tr(s, "library-empty")).color(palette::current().details));
    }
    rows(ui, NARROW, &home.series, |ui, summary| {
        let clicked = card(
            ui,
            NARROW,
            state.textures.get(&summary.id),
            &summary.record.title,
            &mut state.focus_first,
            |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&summary.record.title).family(theme::bold()))
                        .truncate(),
                );
                let count = (s.tr)(
                    "library-watched",
                    &[
                        ("watched", summary.watched.to_string()),
                        ("total", summary.episodes.to_string()),
                    ],
                );
                ui.label(RichText::new(count).color(palette::current().details));
                let share = summary.watched as f32 / summary.episodes.max(1) as f32;
                progress(ui, NARROW, share, palette::current().good);
            },
        );
        if clicked {
            out.push(Command::LibrarySeries(Some(summary.id)));
        }
    });
    if !home.folders.is_empty() {
        section(ui, &tr(s, "library-folders"));
        for (root, folder) in &home.folders {
            ui.horizontal(|ui| {
                ui.label(root);
                let mut grouping = folder.grouping;
                let name = |g: Grouping| match g {
                    Grouping::BySeries => tr(s, "library-by-series"),
                    Grouping::ByFolder => tr(s, "library-by-folder"),
                };
                let label = tr(s, "library-grouping");
                let combo = ComboBox::from_id_salt(("grouping", root))
                    .selected_text(name(grouping))
                    .show_ui(ui, |ui| {
                        for g in [Grouping::BySeries, Grouping::ByFolder] {
                            ui.selectable_value(&mut grouping, g, name(g));
                        }
                    });
                combo
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
                if grouping != folder.grouping {
                    out.push(Command::LibraryScan(PathBuf::from(root), grouping));
                }
                if ui
                    .add_enabled(!s.page.busy, Button::new(tr(s, "library-rescan")))
                    .clicked()
                {
                    out.push(Command::LibraryScan(PathBuf::from(root), folder.grouping));
                }
            });
        }
    }
}

/// The episodes, the year and the format of a series as found online.
fn online_details(
    s: &Scene<'_>,
    episodes: Option<u32>,
    year: Option<u32>,
    format: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(n) = episodes {
        parts.push((s.tr)(
            "library-online-episodes",
            &[("count", n.to_string())],
        ));
    }
    parts.extend(year.map(|y| y.to_string()));
    parts.extend(format.map(str::to_string));
    parts.join(" · ")
}

/// MET-001, MET-002: what AniList or Jikan said of the series, or the way
/// to ask them; the results to choose from when none has exactly its title.
fn online(ui: &mut Ui, s: &Scene<'_>, id: u64, out: &mut Vec<Command>) {
    let page = s.page;
    let p = palette::current();
    let identify = page.identify.as_ref().filter(|i| i.series == id);
    let info = page
        .series_info
        .as_ref()
        .filter(|_| page.series.as_ref().is_some_and(|(open, _)| *open == id));
    match info {
        Some(info) => {
            let names: Vec<&str> = std::iter::once(info.romaji.as_str())
                .chain(info.native.as_deref())
                .chain(info.english.as_deref())
                .collect();
            ui.label(RichText::new(names.join(" · ")).color(p.details));
            let details = online_details(s, info.episodes, info.year, info.format.as_deref());
            ui.horizontal_wrapped(|ui| {
                if !details.is_empty() {
                    ui.label(RichText::new(details).color(p.details));
                }
                if let Some(url) = &info.page_url {
                    let source = nikomiru_online::metadata::Source::from_key(&info.source);
                    let link = ui.link(tr(s, "library-online-page") + " " + source.name());
                    if link.clicked() {
                        out.push(Command::OpenExternal(url.clone()));
                    }
                }
                if ui
                    .add_enabled(
                        identify.is_none(),
                        Button::new(tr(s, "library-identify-again")),
                    )
                    .clicked()
                {
                    out.push(Command::LibraryIdentify(id));
                }
                if ui.button(tr(s, "library-forget")).clicked() {
                    out.push(Command::LibraryForget(id));
                }
            });
        }
        None if identify.is_none() => {
            let button = ui
                .button(tr(s, "library-identify"))
                .on_hover_text(tr(s, "library-identify-hint"));
            if button.clicked() {
                out.push(Command::LibraryIdentify(id));
            }
        }
        None => {}
    }
    let Some(identify) = identify else {
        return;
    };
    if identify.busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr(s, "library-identifying"));
        });
        return;
    }
    let source = identify.source.map_or("", |src| src.name()).to_string();
    ui.label(
        RichText::new((s.tr)("library-identify-choose", &[("source", source)])).color(p.details),
    );
    for (i, choice) in identify.choices.iter().enumerate() {
        ui.horizontal(|ui| {
            let label = (s.tr)("library-identify-this", &[("title", choice.romaji.clone())]);
            let this = ui.button(tr(s, "library-identify-pick"));
            this.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
            if this.clicked() {
                out.push(Command::LibraryIdentifyChoose(Some(i)));
            }
            ui.label(RichText::new(&choice.romaji).family(theme::bold()));
            let mut details: Vec<String> = choice.native.iter().cloned().collect();
            let rest = online_details(s, choice.episodes, choice.year, choice.format.as_deref());
            if !rest.is_empty() {
                details.push(rest);
            }
            ui.label(RichText::new(details.join(" · ")).color(p.details));
        });
    }
    if ui.button(tr(s, "library-identify-none-of-these")).clicked() {
        out.push(Command::LibraryIdentifyChoose(None));
    }
}

fn series(
    ui: &mut Ui,
    s: &Scene<'_>,
    state: &mut State,
    id: u64,
    episodes: Option<&[(String, VideoRecord)]>,
    out: &mut Vec<Command>,
) {
    let home = s.page.home.as_ref();
    let title = home
        .and_then(|h| h.series_title(id))
        .unwrap_or_default()
        .to_string();
    if ui.button(tr(s, "library-back")).clicked() {
        state.reset();
        out.push(Command::LibrarySeries(None));
        return;
    }
    ui.label(RichText::new(&title).size(22.0).family(theme::bold()));
    online(ui, s, id, out);
    let others: Vec<(u64, String)> = home
        .map(|h| {
            h.series
                .iter()
                .filter(|x| x.id != id)
                .map(|x| (x.id, x.record.title.clone()))
                .collect()
        })
        .unwrap_or_default();
    // LIB-011: rename, merge; LIB-012: delete
    ui.horizontal(|ui| {
        let name = &mut state.rename.get_or_insert_with(|| (id, title.clone())).1;
        let label = ui.label(tr(s, "library-name"));
        ui.add(TextEdit::singleline(name).desired_width(260.0))
            .labelled_by(label.id);
        if ui
            .add_enabled(
                !name.trim().is_empty() && *name != title,
                Button::new(tr(s, "library-rename")),
            )
            .clicked()
        {
            out.push(Command::LibraryEdit(LibraryEdit::Rename(
                id,
                name.trim().to_string(),
            )));
        }
    });
    if !others.is_empty() {
        ui.horizontal(|ui| {
            target(ui, s, "library-merge-into", &others, &mut state.merge_into);
            if ui
                .add_enabled(
                    state.merge_into.is_some(),
                    Button::new(tr(s, "library-merge")),
                )
                .clicked()
                && let Some(into) = state.merge_into
            {
                out.push(Command::LibraryEdit(LibraryEdit::Merge { from: id, into }));
                out.push(Command::LibrarySeries(Some(into)));
            }
        });
    }
    if ui.button(tr(s, "library-delete")).clicked() {
        out.push(Command::LibraryEdit(LibraryEdit::Delete(id)));
        out.push(Command::LibrarySeries(None));
    }
    let Some(episodes) = episodes else {
        ui.spinner();
        return;
    };
    section(
        ui,
        &(s.tr)("library-episodes", &[("count", episodes.len().to_string())]),
    );
    for (uid, v) in episodes {
        Frame::new()
            .fill(palette::current().row)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut ticked = state.selected.contains(uid);
                    let name = format!("{} {}", episode_label(v), file_title(&v.path));
                    let check = ui.checkbox(&mut ticked, "");
                    check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, ticked, &name));
                    if check.changed() {
                        if ticked {
                            state.selected.push(uid.clone());
                        } else {
                            state.selected.retain(|u| u != uid);
                        }
                    }
                    let (glyph, color) = match Watched::of(v) {
                        Watched::New => ("○", palette::current().details),
                        Watched::Started => ("◐", palette::current().accent_text),
                        Watched::Watched => ("●", palette::current().good),
                    };
                    ui.label(RichText::new(glyph).color(color));
                    ui.label(RichText::new(episode_label(v)).family(theme::bold()));
                    ui.add(egui::Label::new(file_title(&v.path)).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let play = ui.button(tr(s, "library-play"));
                        if std::mem::take(&mut state.focus_first) {
                            play.request_focus();
                        }
                        play.widget_info(|| {
                            WidgetInfo::labeled(
                                Role::Button,
                                true,
                                format!("{} {name}", tr(s, "library-play")),
                            )
                        });
                        if play.clicked() {
                            out.push(Command::LibraryPlay(PathBuf::from(&v.path)));
                        }
                        if let Some(d) = v.duration_ms.filter(|_| v.last_position_ms > 0) {
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    clock(v.last_position_ms as i64),
                                    clock(d as i64)
                                ))
                                .color(palette::current().details),
                            );
                        }
                    });
                });
            });
    }
    if state.selected.is_empty() {
        return;
    }
    // LIB-011: move, split; LIB-012: remove
    let uids = state.selected.clone();
    ui.horizontal(|ui| {
        if !others.is_empty() {
            target(ui, s, "library-move-to", &others, &mut state.move_to);
            if ui
                .add_enabled(state.move_to.is_some(), Button::new(tr(s, "library-move")))
                .clicked()
                && let Some(to) = state.move_to
            {
                out.push(Command::LibraryEdit(LibraryEdit::Move {
                    uids: uids.clone(),
                    to,
                }));
                state.selected.clear();
            }
        }
    });
    ui.horizontal(|ui| {
        let label = ui.label(tr(s, "library-split-name"));
        ui.add(TextEdit::singleline(&mut state.split_name).desired_width(260.0))
            .labelled_by(label.id);
        if ui
            .add_enabled(
                !state.split_name.trim().is_empty(),
                Button::new(tr(s, "library-split")),
            )
            .clicked()
        {
            out.push(Command::LibraryEdit(LibraryEdit::Split {
                uids: uids.clone(),
                title: state.split_name.trim().to_string(),
            }));
            state.selected.clear();
            state.split_name.clear();
        }
    });
    if ui.button(tr(s, "library-remove")).clicked() {
        out.push(Command::LibraryEdit(LibraryEdit::Remove(uids)));
        state.selected.clear();
    }
}

/// A series to choose, among `others`.
fn target(
    ui: &mut Ui,
    s: &Scene<'_>,
    label: &str,
    others: &[(u64, String)],
    chosen: &mut Option<u64>,
) {
    let name = tr(s, label);
    ui.label(&name);
    let shown = chosen
        .and_then(|c| others.iter().find(|(id, _)| *id == c))
        .map(|(_, t)| t.clone())
        .unwrap_or_default();
    let combo = ComboBox::from_id_salt(label)
        .width(220.0)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (id, title) in others {
                ui.selectable_value(chosen, Some(*id), title);
            }
        });
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Home;
    use egui::Id;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use nikomiru_library::home::SeriesSummary;
    use nikomiru_library::{FolderRecord, SeriesRecord};

    struct Test {
        page: LibraryPage,
        first_steps: Option<[bool; 3]>,
        state: State,
        out: Vec<Command>,
        closed: bool,
    }

    fn harness(page: LibraryPage) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(vec2(1280.0, 1000.0))
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        today: 0,
                        first_steps: t.first_steps,
                        tr: &tr,
                    };
                    t.closed |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    page,
                    first_steps: None,
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

    fn video(path: &str, series: u64, episode: u32, position: u64) -> VideoRecord {
        VideoRecord {
            path: path.into(),
            series: Some(series),
            episode: Some(episode),
            duration_ms: Some(1_440_000),
            last_position_ms: position,
            last_played_at: (position > 0).then_some(1),
            ..VideoRecord::default()
        }
    }

    fn home() -> Home {
        let summary = |id, title: &str, episodes, watched| SeriesSummary {
            id,
            record: SeriesRecord {
                title: title.into(),
                ..SeriesRecord::default()
            },
            episodes,
            watched,
            started: 0,
            last_played_at: None,
        };
        Home {
            continue_watching: vec![(
                "video/Alpha - 02".into(),
                video("/v/Alpha - 02.mkv", 1, 2, 600_000),
            )],
            series: vec![summary(1, "Alpha", 3, 1), summary(2, "Beta", 1, 0)],
            folders: vec![("/v".into(), FolderRecord::default())],
        }
    }

    // T-MET-001: a series asked online, the result chosen, forgotten
    #[test]
    fn a_series_is_identified_from_its_page() {
        use crate::catalog::Identify;
        use nikomiru_library::series_info::SeriesInfoRecord;
        use nikomiru_online::metadata::{SeriesInfo, Source};
        let episodes = vec![(
            "video/Alpha - 01".into(),
            video("/v/Alpha - 01.mkv", 1, 1, 0),
        )];
        let mut h = harness(LibraryPage {
            home: Some(home()),
            series: Some((1, Some(episodes.clone()))),
            ..LibraryPage::default()
        });
        h.run();
        assert!(matches!(
            click(&mut h, "library-identify")[..],
            [Command::LibraryIdentify(1)]
        ));
        // two results, neither exactly "Alpha"
        let choice = |romaji: &str| SeriesInfo {
            romaji: romaji.into(),
            ..SeriesInfo::default()
        };
        h.state_mut().page.identify = Some(Identify {
            series: 1,
            source: Some(Source::AniList),
            choices: vec![choice("Alpha Season 1"), choice("Alpha Movie")],
            busy: false,
        });
        h.run();
        h.get_by_label("library-identify-choose");
        h.get_all_by_label("library-identify-this")
            .next()
            .unwrap()
            .click();
        h.run();
        assert!(matches!(
            std::mem::take(&mut h.state_mut().out)[..],
            [Command::LibraryIdentifyChoose(Some(0))]
        ));
        // identified: what was found, and the way to forget it
        h.state_mut().page.identify = None;
        h.state_mut().page.series_info = Some(SeriesInfoRecord {
            source: "anilist".into(),
            anilist_id: Some(7),
            romaji: "Alpha Season 1".into(),
            native: Some("アルファ".into()),
            episodes: Some(12),
            page_url: Some("https://anilist.co/anime/7".into()),
            ..SeriesInfoRecord::default()
        });
        h.run();
        h.get_by_label("Alpha Season 1 · アルファ");
        assert!(matches!(
            click(&mut h, "library-forget")[..],
            [Command::LibraryForget(1)]
        ));
        assert!(matches!(
            &click(&mut h, "library-online-page AniList")[..],
            [Command::OpenExternal(url)] if url == "https://anilist.co/anime/7"
        ));
    }

    // T-LIB-005
    #[test]
    fn the_home_page_continues_opens_series_and_scans() {
        let mut h = harness(LibraryPage {
            home: Some(home()),
            ..LibraryPage::default()
        });
        h.run();
        let card = |h: &mut Harness<'_, Test>, label: &str| {
            h.get_by_role_and_label(Role::Button, label).click();
            h.run();
            std::mem::take(&mut h.state_mut().out)
        };
        let out = card(&mut h, "Alpha E02");
        assert!(
            matches!(out.as_slice(), [Command::LibraryPlay(p)] if p == Path::new("/v/Alpha - 02.mkv")),
            "{out:?}"
        );
        let out = card(&mut h, "Beta");
        assert!(
            matches!(out.as_slice(), [Command::LibrarySeries(Some(2))]),
            "{out:?}"
        );
        let out = click(&mut h, "library-rescan");
        assert!(
            matches!(out.as_slice(), [Command::LibraryScan(p, Grouping::BySeries)] if p == Path::new("/v")),
            "{out:?}"
        );
        let out = click(&mut h, "library-add-folder");
        assert!(matches!(out.as_slice(), [Command::LibraryAddFolder]));
        click(&mut h, "library-close");
        assert!(h.state().closed);
    }

    // T-LIB-005: many series wrap into rows that fit the window
    #[test]
    fn the_series_wrap_into_rows() {
        let mut home = home();
        home.series = (0..30)
            .map(|i| SeriesSummary {
                id: i,
                record: SeriesRecord {
                    title: format!("A rather long series title number {i}"),
                    ..SeriesRecord::default()
                },
                episodes: 12,
                watched: 0,
                started: 0,
                last_played_at: None,
            })
            .collect();
        let mut h = harness(LibraryPage {
            home: Some(home),
            ..LibraryPage::default()
        });
        h.run();
        let first = h
            .get_by_role_and_label(Role::Button, "A rather long series title number 0")
            .rect();
        let sixth = h
            .get_by_role_and_label(Role::Button, "A rather long series title number 5")
            .rect();
        assert!(sixth.right() <= 1280.0, "{sixth:?}");
        assert!(sixth.top() > first.bottom(), "{first:?} {sixth:?}");
    }

    // T-LIB-011, T-LIB-012
    #[test]
    fn a_series_is_edited_and_its_episodes_moved() {
        let episodes = vec![
            (
                "video/Alpha - 01".to_string(),
                video("/v/Alpha - 01.mkv", 1, 1, 0),
            ),
            (
                "video/Alpha - 02".to_string(),
                video("/v/Alpha - 02.mkv", 1, 2, 600_000),
            ),
        ];
        let mut h = harness(LibraryPage {
            home: Some(home()),
            series: Some((1, Some(episodes))),
            ..LibraryPage::default()
        });
        h.run();
        let out = click(&mut h, "library-play E01 Alpha - 01");
        assert!(
            matches!(out.as_slice(), [Command::LibraryPlay(_)]),
            "{out:?}"
        );
        // rename
        h.get_by_label("library-name").focus();
        h.get_by_label("library-name").type_text(" (TV)");
        h.run();
        let out = click(&mut h, "library-rename");
        assert!(
            matches!(out.as_slice(), [Command::LibraryEdit(LibraryEdit::Rename(1, t))] if t == "Alpha (TV)"),
            "{out:?}"
        );
        // tick an episode: it is split off, then another removed
        h.get_by_label("E01 Alpha - 01").click();
        h.run();
        h.get_by_label("library-split-name").focus();
        h.get_by_label("library-split-name").type_text("Pilot");
        h.run();
        let out = click(&mut h, "library-split");
        assert!(
            matches!(out.as_slice(), [Command::LibraryEdit(LibraryEdit::Split { uids, title })]
                if uids == &["video/Alpha - 01"] && title == "Pilot"),
            "{out:?}"
        );
        h.get_by_label("E02 Alpha - 02").click();
        h.run();
        let out = click(&mut h, "library-remove");
        assert!(
            matches!(out.as_slice(), [Command::LibraryEdit(LibraryEdit::Remove(u))] if u == &["video/Alpha - 02"]),
            "{out:?}"
        );
        let out = click(&mut h, "library-delete");
        assert!(matches!(
            out.as_slice(),
            [
                Command::LibraryEdit(LibraryEdit::Delete(1)),
                Command::LibrarySeries(None)
            ]
        ));
        let out = click(&mut h, "library-back");
        assert!(matches!(out.as_slice(), [Command::LibrarySeries(None)]));
    }

    // T-LIB-013
    #[test]
    fn the_first_steps_lead_to_what_does_them_and_hide() {
        let page = LibraryPage {
            home: Some(Home::default()),
            ..LibraryPage::default()
        };
        let mut h = harness(page);
        h.state_mut().first_steps = Some([false, true, false]);
        h.run();
        h.get_by_label("first-steps-title");
        // the folder is there: no second "Add folder…"
        assert_eq!(h.query_all_by_label("library-add-folder").count(), 1);
        click(&mut h, "first-steps-dictionary-button");
        assert_eq!(
            h.state_mut().state.open_settings.take(),
            Some(crate::view::settings::SettingsTab::Dictionaries)
        );
        click(&mut h, "first-steps-card-button");
        assert_eq!(
            h.state_mut().state.open_settings.take(),
            Some(crate::view::settings::SettingsTab::Anki)
        );
        let out = click(&mut h, "first-steps-hide");
        assert!(matches!(out.as_slice(), [Command::HideFirstSteps]));
        h.state_mut().first_steps = Some([true, false, true]);
        h.run();
        assert_eq!(h.query_all_by_label("library-add-folder").count(), 2);
        h.state_mut().first_steps = None;
        h.run();
        assert!(h.query_by_label("first-steps-title").is_none());
    }
}
