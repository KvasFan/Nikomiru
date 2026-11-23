// SPDX-License-Identifier: GPL-3.0-only

//! The Library page (LIB-005): the first steps until they are done
//! (LIB-013), "Continue watching", the series with their covers and state, a
//! series' episodes with the edits of LIB-011 and LIB-012, and the imported
//! folders with their grouping (LIB-002).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{
    Align, Button, Color32, ComboBox, Frame, Layout, Margin, Rect, RichText, Role, ScrollArea,
    Sense, Stroke, TextEdit, TextureHandle, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use nikomiru_library::home::Watched;
use nikomiru_library::{Grouping, VideoRecord};

use super::dict_page::full_screen;
use super::icons::{self, Icon};
use super::popups::Translate;
use super::{View, palette, theme};
use crate::catalog::{LibraryEdit, LibraryPage};
use crate::session::Session;
use crate::subtitles::clock;
use crate::ui::Command;

/// The width of a card of "Continue watching", and of a series.
const NARROW: f32 = 200.0;
/// The width of a card of what is being watched (SET-010).
const HERO: f32 = 420.0;

/// What the page keeps between frames; dropped when it closes.
#[derive(Default)]
pub(crate) struct State {
    /// The covers as textures, by series.
    textures: HashMap<u64, TextureHandle>,
    analysis: Option<u64>,
    analysis_order: super::analysis_order::State,
    analysis_words: super::analysis_words::State,
    episode_words: super::analysis_words::State,
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
    /// The media server whose tab is shown (SRV-006).
    server: Option<String>,
    /// What is typed in its search.
    server_search: String,
    /// Its covers as textures, by item.
    server_textures: HashMap<String, TextureHandle>,
    /// The Statistics tab is shown (STA-005).
    statistics: bool,
    /// What it keeps.
    stats: super::stats_view::State,
    /// The Review tab is shown (REV-004).
    review: bool,
    /// What it keeps.
    review_state: super::review_view::State,
    /// The view (home, or a series) whose first card or episode got the
    /// keyboard when it appeared (NFR-A11Y-01).
    focused_view: Option<Option<u64>>,
    /// This frame gives the keyboard to the first card or episode.
    focus_first: bool,
    /// The Books page was asked for (BOK-004).
    pub(crate) open_books: bool,
    /// The Texthooker page was asked for (HOK-001).
    pub(crate) open_hooker: bool,
    /// A tab of the settings a first step asked for (LIB-013).
    pub(crate) open_settings: Option<super::settings::SettingsTab>,
    /// The shelf shown when neither the statistics, the review nor a media
    /// server is (VID-001, BOK-015).
    pub(crate) shelf: Shelf,
    /// The link being typed on the Video shelf, and its name (VID-002).
    link_url: String,
    link_title: String,
    /// The web page's address typed on the Web shelf (WEB-001).
    web_address: String,
    /// What the Books page is to do, asked from a shelf.
    pub(crate) books_action: Option<BooksAction>,
    /// The file dialog for a video was asked for.
    pub(crate) open_video: bool,
}

/// The shelves of the main page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Shelf {
    /// The imported folders: series and films (LIB-002).
    #[default]
    Collection,
    /// Videos opened on their own and links kept (VID-001, VID-002).
    Videos,
    /// Books, documents and pasted text (BOK-015).
    Books,
    /// Manga: CBZ, folders of pages, Mokuro volumes (MNG-001).
    Manga,
    /// Web pages read as books (WEB-001).
    Web,
}

impl Shelf {
    /// Every shelf, in the order of the tabs.
    pub(crate) const ALL: [Shelf; 5] = [
        Shelf::Collection,
        Shelf::Videos,
        Shelf::Books,
        Shelf::Manga,
        Shelf::Web,
    ];

    /// The shelf a book belongs on, from its path: an address is a web page,
    /// a CBZ, a Mokuro volume or a folder (no extension) a manga.
    pub(crate) fn of(path: &str) -> Shelf {
        if crate::ui::is_web_address(path) {
            return Shelf::Web;
        }
        let extension = Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        match extension.as_deref() {
            None | Some("cbz" | "zip" | "mokuro") => Shelf::Manga,
            _ => Shelf::Books,
        }
    }

    /// Its icon in the navigation rail (SET-010).
    fn icon(self) -> Icon {
        match self {
            Shelf::Collection => Icon::Home,
            Shelf::Videos => Icon::Video,
            Shelf::Books => Icon::Book,
            Shelf::Manga => Icon::Manga,
            Shelf::Web => Icon::Web,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Shelf::Collection => "tab-collection",
            Shelf::Videos => "tab-videos",
            Shelf::Books => "tab-books",
            Shelf::Manga => "tab-manga",
            Shelf::Web => "tab-web",
        }
    }

    fn is_reading(self) -> bool {
        matches!(self, Shelf::Books | Shelf::Manga | Shelf::Web)
    }
}

/// What a reading shelf asks of the Books page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BooksAction {
    /// The file dialog for a book or a manga.
    Add,
    /// The folder dialog for a manga of loose pages.
    Folder,
    /// The audiobook dialog.
    Audiobook,
    /// The editor of pasted text.
    Paste,
    /// Open this book.
    Open(String),
    /// Read this web page.
    Page(String),
}

impl State {
    /// Shows the Statistics tab, or the library.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub(crate) fn show_statistics(&mut self, on: bool) {
        self.statistics = on;
        self.review = false;
    }

    /// Shows the list of the review's cards, or its study (REV-009).
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub(crate) fn show_review_cards(&mut self, on: bool) {
        self.review_state.browse = on;
    }

    /// Shows the Review tab, or the library.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub(crate) fn show_review(&mut self, on: bool) {
        self.review = on;
        self.statistics = false;
    }

    /// Forgets what was typed and ticked; the covers stay, they cost a
    /// decode each.
    pub(crate) fn reset(&mut self) {
        *self = Self {
            textures: std::mem::take(&mut self.textures),
            shelf: self.shelf,
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
    /// The media servers added, each a tab (SRV-006).
    pub servers: &'a [nikomiru_core::config::ServerConfig],
    /// What is shown of one.
    pub server_page: &'a crate::servers::ServersPage,
    /// The tab of the settings shown last (SET-008).
    pub settings_tab: super::settings::SettingsTab,
    /// Now, for the review's buttons.
    pub now: super::review_view::Now,
    /// `[review]` of the settings (REV-003).
    pub review_settings: nikomiru_library::review::Settings,
    /// Cards go to the review (REV-001): its tab is shown even without any.
    pub to_review: bool,
    /// A condensed-audio export is running.
    pub exporting: bool,
    /// Word-status colours enable explicit series scores (KNW-017).
    pub colours: bool,
    /// An AniDB account is in the settings (MET-006).
    pub anidb: bool,
    /// The score of an episode, by uid, while the words are coloured
    /// (KNW-008).
    pub episode_score: &'a dyn Fn(&str) -> Option<crate::scores::EpisodeScore>,
    /// The books, manga and web pages added (BOK-015), and their page's state.
    pub books: &'a crate::books::BooksPage,
    /// The links kept (VID-002).
    pub links: &'a [nikomiru_core::config::SavedLink],
    /// The interface strings.
    pub tr: &'a Translate<'a>,
}

pub(crate) fn show(ctx: &egui::Context, s: &Session, view: &mut View, out: &mut Vec<Command>) {
    let tr = |id: &str, args: &[(&str, String)]| s.translator().tr(id, args);
    let scene = Scene {
        page: &s.library_page,
        today: s.today(),
        first_steps: s.first_steps(),
        servers: s.server_list(),
        server_page: &s.servers_page,
        settings_tab: view.settings_tab,
        now: {
            let (ms, offset_s) = s.review_now();
            super::review_view::Now { ms, offset_s }
        },
        review_settings: crate::review::settings(&s.config().review),
        to_review: s.config().anki.to_review(),
        exporting: s.audio_export.running.is_some(),
        colours: s.config().subtitles.word_colors,
        anidb: s.anidb_ready(),
        episode_score: &|uid: &str| s.episode_score(uid),
        books: &s.books_page,
        links: &s.config().videos.links,
        tr: &tr,
    };
    if draw(ctx, &scene, &mut view.library, out) {
        view.close_library_page();
    }
    if std::mem::take(&mut view.library.open_video) {
        view.dialog = Some(super::Dialog::Video);
    }
    if let Some(action) = view.library.books_action.take() {
        let shelf = view.library.shelf;
        // the reader shows on the Books page, and comes back here (BOK-015)
        view.open_books_page(out);
        view.books.from_library = Some(shelf);
        view.books.settle = 3;
        match action {
            BooksAction::Add => view.dialog = Some(super::Dialog::Book),
            BooksAction::Folder => view.dialog = Some(super::Dialog::MangaFolder),
            BooksAction::Audiobook => view.dialog = Some(super::Dialog::Audiobook),
            BooksAction::Paste => view.books.paste_open = true,
            BooksAction::Open(uid) => out.push(Command::BookChoose(uid)),
            BooksAction::Page(url) => out.push(Command::BookPage(url)),
        }
    }
    if let Some(tab) = view.library.open_settings.take() {
        view.open_settings(tab, out);
    }
}

fn tr(s: &Scene<'_>, id: &str) -> String {
    (s.tr)(id, &[])
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(text)
            .size(18.0)
            .family(theme::bold())
            .color(palette::current().text),
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
        .rect_filled(rect, palette::PILL, palette::current().track);
    let mut done = rect;
    done.set_width(rect.width() * share.clamp(0.0, 1.0));
    if done.width() > 0.0 {
        done.set_width(done.width().max(4.0));
        ui.painter().rect_filled(done, palette::PILL, color);
    }
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
    for (id, cover) in &s.server_page.covers {
        if !state.server_textures.contains_key(id) {
            let image = egui::ColorImage::from_rgba_unmultiplied(cover.size, &cover.rgba);
            let texture = ctx.load_texture(format!("server-cover-{id}"), image, Default::default());
            state.server_textures.insert(id.clone(), texture);
        }
    }
    let screen = ctx.content_rect();
    let mut close = false;
    // a wide window has the navigation rail at its left (SET-010)
    let with_rail = screen.width() >= RAIL_MIN_WIDTH;
    full_screen(ctx, "library-page", palette::current().page, |ui| {
        let mut area = screen.shrink(if with_rail { 12.0 } else { 24.0 });
        if with_rail {
            // expanded, the names beside the icons, on a window wide enough;
            // compact, the names under them, on a narrower one
            let compact = screen.width() < RAIL_EXPANDED_MIN_WIDTH;
            let width = if compact {
                RAIL_COMPACT_WIDTH
            } else {
                RAIL_WIDTH
            };
            let rail = Rect::from_min_size(area.min, vec2(width, area.height()));
            let mut rail_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(rail)
                    .layout(Layout::top_down(Align::Min)),
            );
            navigation_rail(&mut rail_ui, rail, compact, s, state, out);
            area.min.x = rail.right() + 24.0;
            area.min.y += 12.0;
            area.max.x -= 12.0;
        }
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(Layout::top_down(Align::Min)),
        );
        ui.visuals_mut().override_text_color = Some(palette::current().text);
        ui.spacing_mut().item_spacing.y = 10.0;
        header(&mut ui, s, state, with_rail, &mut close, out);
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
        if state.shelf.is_reading()
            && let Some((text, ok)) = &s.books.status
        {
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
                _ if state.server.is_some() => server_tab(ui, s, state, out),
                _ if state.review => super::review_view::draw(
                    ui,
                    &s.page.review,
                    &s.now,
                    &s.review_settings,
                    s.tr,
                    &mut state.review_state,
                    out,
                ),
                _ if state.statistics => super::stats_view::draw(
                    ui,
                    s.page.stats.as_ref(),
                    s.today,
                    s.tr,
                    &mut state.stats,
                    out,
                ),
                _ if state.shelf == Shelf::Videos => videos(ui, s, state, out),
                _ if state.shelf.is_reading() => reading_shelf(ui, s, state, out),
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

/// A tab of the page: a pill, on the accent's tonal fill when chosen;
/// whether it was clicked.
fn tab(ui: &mut Ui, chosen: bool, text: &str) -> bool {
    let text = RichText::new(text).size(15.0);
    let text = if chosen {
        text.family(theme::bold())
    } else {
        text
    };
    let button = Button::new(text)
        .selected(chosen)
        .corner_radius(palette::PILL)
        .min_size(vec2(0.0, 34.0));
    ui.add(button).clicked()
}

/// From this width on the library has its navigation rail (SET-010).
const RAIL_MIN_WIDTH: f32 = 900.0;
/// From this width on the rail has the names beside the icons.
const RAIL_EXPANDED_MIN_WIDTH: f32 = 1400.0;
/// The rail's width, expanded and compact.
const RAIL_WIDTH: f32 = 220.0;
const RAIL_COMPACT_WIDTH: f32 = 88.0;

/// Where the page's tabs lead: a shelf, the statistics, the review, a
/// media server.
#[derive(Debug, Clone, PartialEq)]
enum Destination {
    Shelf(Shelf),
    Statistics,
    Review,
    Server(String),
}

/// The tabs of the page, in their order: each with its name, its icon and
/// whether it is the one shown.
fn destinations(s: &Scene<'_>, state: &State) -> Vec<(Destination, String, Icon, bool)> {
    // the review's tab once it is used
    let due = s
        .page
        .review
        .review
        .as_ref()
        .map_or(0, |r| r.queue.cards.len());
    let with_review = s.to_review
        || s.page
            .review
            .review
            .as_ref()
            .is_some_and(|r| r.queue.total > 0);
    let shelf_shown = state.server.is_none() && !state.statistics && !state.review;
    let mut all: Vec<_> = Shelf::ALL
        .into_iter()
        .map(|shelf| {
            (
                Destination::Shelf(shelf),
                tr(s, shelf.label()),
                shelf.icon(),
                shelf_shown && state.shelf == shelf,
            )
        })
        .collect();
    all.push((
        Destination::Statistics,
        tr(s, "stats-title"),
        Icon::Chart,
        state.statistics && state.server.is_none(),
    ));
    if with_review {
        let title = if due > 0 {
            (s.tr)("review-title-due", &[("count", due.to_string())])
        } else {
            tr(s, "review-title")
        };
        all.push((
            Destination::Review,
            title,
            Icon::Review,
            state.review && state.server.is_none(),
        ));
    }
    for server in s.servers {
        all.push((
            Destination::Server(server.id.clone()),
            server.name.clone(),
            Icon::Server,
            state.server.as_ref() == Some(&server.id),
        ));
    }
    all
}

/// Shows `to`, chosen from the tabs; `chosen` when it was already shown.
fn go(to: &Destination, chosen: bool, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    match to {
        Destination::Shelf(shelf) => {
            let shelf = *shelf;
            state.shelf = shelf;
            state.statistics = false;
            state.review = false;
            state.server = None;
            if shelf.is_reading() && !chosen {
                out.push(Command::BooksOpen);
            }
            // a series open stays open on its own shelf only
            if shelf != Shelf::Collection && s.page.series.is_some() {
                out.push(Command::LibrarySeries(None));
            }
        }
        Destination::Statistics => {
            // read again: the review may have answered since (REV-008)
            if !state.statistics {
                out.push(Command::StatsLoad);
            }
            state.statistics = true;
            state.review = false;
            state.server = None;
        }
        Destination::Review => {
            if !state.review {
                out.push(Command::Review(crate::review::ReviewJob::Load));
            }
            state.review = true;
            state.statistics = false;
            state.server = None;
        }
        Destination::Server(id) => {
            if !chosen {
                state.server = Some(id.clone());
                state.statistics = false;
                state.review = false;
                state.server_search.clear();
                out.push(Command::ServerShow(id.clone()));
            }
        }
    }
}

/// The navigation rail (SET-010): the name, the shelves, the statistics,
/// the review and the media servers; the tools and the settings at its foot.
fn navigation_rail(
    ui: &mut Ui,
    rect: Rect,
    compact: bool,
    s: &Scene<'_>,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    let p = palette::current();
    ui.painter().rect_filled(rect, 24.0, p.rail);
    let inner = rect.shrink2(vec2(if compact { 6.0 } else { 10.0 }, 16.0));
    let mut ui = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    ui.spacing_mut().item_spacing.y = 2.0;
    let item = |ui: &mut Ui, icon: Icon, name: &str, chosen: bool| {
        if compact {
            icons::rail_item(ui, icon, name, chosen)
        } else {
            icons::nav_item(ui, icon, name, chosen)
        }
    };
    if !compact {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(
                RichText::new(tr(s, "app-title"))
                    .size(22.0)
                    .family(theme::bold())
                    .color(p.accent_text),
            );
        });
    }
    ui.add_space(14.0);
    let all = destinations(s, state);
    let mut chosen_now = None;
    ScrollArea::vertical()
        .id_salt("library-rail")
        .max_height((inner.height() - if compact { 190.0 } else { 160.0 }).max(48.0))
        .show(&mut ui, |ui| {
            for (i, (to, name, icon, chosen)) in all.iter().enumerate() {
                // the shelves, then what is not a shelf
                if i > 0 && matches!(to, Destination::Statistics) {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                if item(ui, *icon, name, *chosen) {
                    chosen_now = Some((to.clone(), *chosen));
                }
            }
        });
    if let Some((to, chosen)) = chosen_now {
        go(&to, chosen, s, state, out);
    }
    // the foot: the tools and the settings
    let foot_height = if compact { 130.0 } else { 100.0 };
    let foot = Rect::from_min_max(pos2(inner.left(), inner.bottom() - foot_height), inner.max);
    let mut foot_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(foot)
            .layout(Layout::bottom_up(Align::Min)),
    );
    foot_ui.spacing_mut().item_spacing.y = 2.0;
    // SET-008
    if item(
        &mut foot_ui,
        Icon::Settings,
        &tr(s, "settings-title"),
        false,
    ) {
        state.open_settings = Some(s.settings_tab);
    }
    let tools = item(&mut foot_ui, Icon::Tools, &tr(s, "library-tools"), false);
    let popup_id = egui::Id::unique("library-rail-tools");
    if tools {
        egui::Popup::toggle_id(foot_ui.ctx(), popup_id);
    }
    let anchor = foot_ui.min_rect();
    egui::Popup::new(popup_id, foot_ui.ctx().clone(), anchor, foot_ui.layer_id())
        .open_memory(None)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| tools_menu(ui, s, state, out));
}

/// The tools: the texthooker, the phrases, the recall, the podcasts, the
/// annotations, the search, the books page.
fn tools_menu(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    if ui.button(tr(s, "hooker-title")).clicked() {
        state.open_hooker = true;
        ui.close();
    }
    if ui.button(tr(s, "phrases-title")).clicked() {
        out.push(Command::PhrasesOpen);
        ui.close();
    }
    if ui.button(tr(s, "recall-history")).clicked() {
        out.push(Command::RecallHistoryOpen);
        ui.close();
    }
    if ui.button(tr(s, "recall-load")).clicked() {
        out.push(Command::RecallLoad);
        ui.close();
    }
    if ui.button(tr(s, "podcasts-title")).clicked() {
        out.push(Command::PodcastsOpen);
        ui.close();
    }
    if ui.button(tr(s, "annotations-title")).clicked() {
        out.push(Command::ReaderToolsOpen(
            crate::reader_tools::Tab::Annotations,
        ));
        ui.close();
    }
    if ui.button(tr(s, "search-title")).clicked() {
        out.push(Command::ReaderToolsOpen(crate::reader_tools::Tab::Search));
        ui.close();
    }
    ui.separator();
    if ui.button(tr(s, "library-books-page")).clicked() {
        state.open_books = true;
        ui.close();
    }
}

/// The title of what is shown, and the way back; without the rail also the
/// name, the tools, the settings and the tabs, these on rows of their own so
/// that nothing overlaps however narrow the window; then what the tab shown
/// can add.
fn header(
    ui: &mut Ui,
    s: &Scene<'_>,
    state: &mut State,
    with_rail: bool,
    close: &mut bool,
    out: &mut Vec<Command>,
) {
    let all = destinations(s, state);
    let shown = all
        .iter()
        .find(|(.., chosen)| *chosen)
        .map(|(_, name, ..)| name.clone());
    ui.horizontal(|ui| {
        let title = match (&shown, with_rail) {
            (Some(name), true) => name.clone(),
            _ => tr(s, "app-title"),
        };
        ui.label(
            RichText::new(title)
                .size(28.0)
                .family(theme::bold())
                .color(if with_rail {
                    palette::current().text
                } else {
                    palette::current().accent_text
                }),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(tr(s, "library-close")).clicked() {
                *close = true;
            }
            if !with_rail {
                // SET-008
                let settings = icons::button(ui, Icon::Settings, &tr(s, "settings-title"));
                if settings.clicked() {
                    state.open_settings = Some(s.settings_tab);
                }
                ui.menu_button(tr(s, "library-tools"), |ui| tools_menu(ui, s, state, out));
            }
            if s.page.busy || (state.shelf.is_reading() && s.books.busy) {
                ui.spinner();
            }
        });
    });
    if !with_rail {
        let mut chosen_now = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (to, name, _, chosen) in &all {
                if matches!(to, Destination::Statistics) {
                    ui.separator();
                }
                if tab(ui, *chosen, name) {
                    chosen_now = Some((to.clone(), *chosen));
                }
            }
        });
        if let Some((to, chosen)) = chosen_now {
            go(&to, chosen, s, state, out);
        }
    }
    // what the tab shown can add
    ui.horizontal_wrapped(|ui| {
        if state.server.is_some() || state.review {
        } else if state.statistics {
            if ui.button(tr(s, "stats-export")).clicked() {
                out.push(Command::StatsExport);
            }
        } else {
            match state.shelf {
                Shelf::Collection => {
                    if ui
                        .add_enabled(
                            !s.page.busy,
                            palette::suggested(tr(s, "library-add-folder")),
                        )
                        .clicked()
                    {
                        out.push(Command::LibraryAddFolder);
                    }
                }
                Shelf::Videos => {
                    if ui
                        .add(palette::suggested(tr(s, "videos-open-file")))
                        .clicked()
                    {
                        state.open_video = true;
                    }
                    if ui.button(tr(s, "webvideo-open")).clicked() {
                        out.push(Command::WebOpenDialog);
                    }
                }
                Shelf::Books => {
                    let free = !s.books.busy;
                    if ui
                        .add_enabled(free, palette::suggested(tr(s, "books-add")))
                        .clicked()
                    {
                        state.books_action = Some(BooksAction::Add);
                    }
                    if ui
                        .add_enabled(free, Button::new(tr(s, "books-paste")))
                        .clicked()
                    {
                        state.books_action = Some(BooksAction::Paste);
                    }
                    if ui.button(tr(s, "books-audiobook")).clicked() {
                        state.books_action = Some(BooksAction::Audiobook);
                    }
                }
                Shelf::Manga => {
                    let free = !s.books.busy;
                    if ui
                        .add_enabled(free, palette::suggested(tr(s, "manga-add")))
                        .clicked()
                    {
                        state.books_action = Some(BooksAction::Add);
                    }
                    if ui
                        .add_enabled(free, Button::new(tr(s, "books-folder")))
                        .clicked()
                    {
                        state.books_action = Some(BooksAction::Folder);
                    }
                }
                Shelf::Web => {
                    let label = ui.label(tr(s, "books-web"));
                    let field = ui
                        .add(
                            TextEdit::singleline(&mut state.web_address)
                                .hint_text(tr(s, "address-hint"))
                                .desired_width(ui.available_width().clamp(160.0, 420.0)),
                        )
                        .labelled_by(label.id);
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let address = state.web_address.trim().to_string();
                    let open = ui.add_enabled(
                        !s.books.busy && !address.is_empty(),
                        palette::suggested(tr(s, "books-web-open")),
                    );
                    if (open.clicked() || enter) && !address.is_empty() {
                        state.books_action = Some(BooksAction::Page(address));
                    }
                }
            }
        }
    });
}

/// The Video shelf (VID-001, VID-002): the links kept, then the videos
/// opened on their own, the last first.
fn videos(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    section(ui, &tr(s, "videos-links"));
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tr(s, "videos-link-url"));
        ui.add(
            TextEdit::singleline(&mut state.link_url)
                .hint_text(tr(s, "address-hint"))
                .desired_width(ui.available_width().clamp(160.0, 360.0)),
        )
        .labelled_by(label.id);
        let label = ui.label(tr(s, "videos-link-title"));
        ui.add(TextEdit::singleline(&mut state.link_title).desired_width(180.0))
            .labelled_by(label.id);
        let url = state.link_url.trim().to_string();
        if ui
            .add_enabled(
                crate::ui::is_web_address(&url),
                Button::new(tr(s, "videos-link-keep")),
            )
            .clicked()
        {
            out.push(Command::LinkKeep(url, state.link_title.trim().to_string()));
            state.link_url.clear();
            state.link_title.clear();
        }
    });
    if s.links.is_empty() {
        ui.label(RichText::new(tr(s, "videos-links-none")).color(palette::current().details));
    }
    for link in s.links {
        let title = if link.title.is_empty() {
            link.url.as_str()
        } else {
            link.title.as_str()
        };
        list_row(
            ui,
            |ui| {
                let remove = (s.tr)("videos-link-remove", &[("title", title.to_string())]);
                if cross(ui, &remove) {
                    out.push(Command::LinkRemove(link.url.clone()));
                }
                if ui.add(palette::suggested(tr(s, "videos-play"))).clicked() {
                    out.push(Command::WebOpen(link.url.clone()));
                }
            },
            |ui| {
                ui.add(egui::Label::new(RichText::new(title).family(theme::bold())).truncate());
                if !link.title.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&link.url).color(palette::current().details),
                        )
                        .truncate(),
                    );
                }
            },
        );
    }
    section(ui, &tr(s, "videos-history"));
    let Some(home) = &s.page.home else {
        ui.spinner();
        return;
    };
    if home.history.is_empty() {
        ui.label(RichText::new(tr(s, "videos-history-none")).color(palette::current().details));
    }
    for (uid, v) in &home.history {
        let title = home.server_titles.get(uid).cloned().unwrap_or_else(|| {
            if crate::ui::is_web_address(&v.path) {
                v.path.clone()
            } else {
                file_title(&v.path)
            }
        });
        list_row(
            ui,
            |ui| {
                let forget = (s.tr)("videos-forget", &[("title", title.clone())]);
                if cross(ui, &forget) {
                    out.push(Command::LibraryEdit(LibraryEdit::Remove(vec![uid.clone()])));
                }
                if crate::ui::is_web_address(&v.path)
                    && !s.links.iter().any(|l| l.url == v.path)
                    && ui.button(tr(s, "videos-link-keep")).clicked()
                {
                    out.push(Command::LinkKeep(v.path.clone(), String::new()));
                }
                if ui.add(palette::suggested(tr(s, "videos-play"))).clicked() {
                    out.push(Command::LibraryPlay(PathBuf::from(&v.path)));
                }
            },
            |ui| {
                ui.add(egui::Label::new(RichText::new(&title).family(theme::bold())).truncate());
                let watched = Watched::of(v) == Watched::Watched;
                let detail = if watched {
                    tr(s, "videos-watched")
                } else {
                    v.duration_ms
                        .map(|d| {
                            (s.tr)(
                                "library-left",
                                &[("time", clock(d.saturating_sub(v.last_position_ms) as i64))],
                            )
                        })
                        .unwrap_or_default()
                };
                if !detail.is_empty() {
                    ui.label(RichText::new(detail).color(palette::current().details));
                }
                let share = if watched {
                    1.0
                } else {
                    v.duration_ms
                        .map_or(0.0, |d| v.last_position_ms as f32 / d.max(1) as f32)
                };
                progress(
                    ui,
                    ui.available_width().min(220.0),
                    share,
                    palette::current().accent,
                );
            },
        );
    }
}

/// A small ✕ named `name` for screen readers; whether it was clicked.
fn cross(ui: &mut Ui, name: &str) -> bool {
    let response = ui.small_button("✕").on_hover_text(name);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    response.clicked()
}

/// A row of a list, the width of the page: `right`'s buttons first, at the
/// right, then `left` in what is left, so that its text is cut before them.
fn list_row(ui: &mut Ui, right: impl FnOnce(&mut Ui), left: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(palette::current().row)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // a row's own height: centring in the list's would push the
            // buttons far below their text
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    right(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.vertical(left);
                    });
                });
            });
        });
}

/// A row of controls that wraps, its labels centred on its fields.
fn toolbar(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    ui.horizontal_wrapped(body);
}

/// The Books, Manga or Web shelf: what was added there, the last read first.
fn reading_shelf(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    let shelf = state.shelf;
    let items: Vec<_> = s
        .books
        .list
        .iter()
        .filter(|(_, b)| Shelf::of(&b.path) == shelf)
        .collect();
    if items.is_empty() {
        let none = match shelf {
            Shelf::Manga => "manga-none",
            Shelf::Web => "web-none",
            _ => "books-none",
        };
        ui.label(RichText::new(tr(s, none)).color(palette::current().details));
        return;
    }
    for (uid, book) in items {
        list_row(
            ui,
            |ui| {
                let remove = (s.tr)("books-remove", &[("title", book.title.clone())]);
                if cross(ui, &remove) {
                    out.push(Command::BookRemove(uid.clone()));
                }
                let percent = crate::books::percent(book);
                let open = if percent > 0 {
                    (s.tr)("books-continue", &[("percent", percent.to_string())])
                } else {
                    tr(s, "books-open")
                };
                if ui.add(palette::suggested(open)).clicked() {
                    state.books_action = Some(BooksAction::Open(uid.clone()));
                }
            },
            |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&book.title).family(theme::bold())).truncate(),
                );
                let detail = if shelf == Shelf::Web {
                    book.path.clone()
                } else {
                    book.author.clone()
                };
                if !detail.is_empty() {
                    ui.add(
                        egui::Label::new(RichText::new(detail).color(palette::current().details))
                            .truncate(),
                    );
                }
                let percent = crate::books::percent(book);
                progress(
                    ui,
                    ui.available_width().min(220.0),
                    percent as f32 / 100.0,
                    palette::current().good,
                );
            },
        );
    }
}

/// A media server's tab (SRV-006): its libraries, then what they hold.
fn server_tab(ui: &mut Ui, s: &Scene<'_>, state: &mut State, out: &mut Vec<Command>) {
    use nikomiru_online::servers::ParentKind;

    use crate::servers::Place;
    let page = s.server_page;
    if let Some(error) = &page.error {
        ui.add(egui::Label::new(RichText::new(error).color(palette::current().bad)).wrap());
    }
    ui.horizontal_wrapped(|ui| {
        for library in &page.libraries {
            let chosen = page.path.first().is_some_and(|p| p.parent == library.id);
            if ui.selectable_label(chosen, &library.title).clicked() {
                out.push(Command::ServerGo(vec![Place {
                    parent: library.id.clone(),
                    title: library.title.clone(),
                    kind: ParentKind::Library,
                }]));
            }
        }
        if page.busy {
            ui.spinner();
        }
    });
    if page.path.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        if page.path.len() > 1 && ui.button(tr(s, "library-back")).clicked() {
            out.push(Command::ServerGo(page.path[..page.path.len() - 1].to_vec()));
        }
        if let Some(place) = page.path.last() {
            ui.label(RichText::new(&place.title).family(theme::bold()));
        }
        let search = ui.add(
            egui::TextEdit::singleline(&mut state.server_search)
                .hint_text(tr(s, "servers-search"))
                .desired_width(220.0),
        );
        if search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            out.push(Command::ServerSearch(
                state.server_search.trim().to_string(),
            ));
        }
    });
    let Some(listing) = &page.listing else {
        return;
    };
    if listing.items.is_empty() && !page.busy {
        ui.label(RichText::new(tr(s, "servers-empty")).color(palette::current().details));
    }
    rows(ui, 200.0, &listing.items, |ui, item| {
        if let Some(cover) = &item.cover
            && !page.asked.contains(&item.id)
        {
            out.push(Command::ServerCover(item.id.clone(), cover.clone()));
        }
        let texture = state.server_textures.get(&item.id);
        let number = match (item.kind.playable(), item.episode) {
            (true, Some(n)) => format!("{n}. "),
            _ => String::new(),
        };
        let name = format!("{number}{}", item.title);
        let mut unused = false;
        let clicked = card(ui, 200.0, texture, &name, &mut unused, |ui| {
            ui.add(egui::Label::new(RichText::new(&name).family(theme::bold())).truncate());
            let mark = if item.played {
                tr(s, "servers-watched")
            } else if item.position_ms > 0 {
                tr(s, "servers-started")
            } else {
                String::new()
            };
            ui.label(RichText::new(mark).color(palette::current().details));
        });
        if clicked {
            if item.kind.playable() {
                out.push(Command::ServerPlay(item.id.clone()));
            } else {
                let mut path = page.path.clone();
                path.push(Place {
                    parent: item.id.clone(),
                    title: item.title.clone(),
                    kind: ParentKind::Item,
                });
                out.push(Command::ServerGo(path));
            }
        }
    });
    if listing.items.len() < listing.total
        && ui
            .add_enabled(!page.busy, Button::new(tr(s, "servers-more")))
            .clicked()
    {
        out.push(Command::ServerMore);
    }
}

/// Cards `width` wide in rows that fit the page, one row under the other:
/// a frame cannot wrap in a wrapping layout, which does not know its size
/// before it is drawn.
fn rows<T>(ui: &mut Ui, width: f32, items: &[T], mut each: impl FnMut(&mut Ui, &T)) {
    const GAP: f32 = 16.0;
    let per_row = (((ui.available_width() + GAP) / (width + GAP)).floor() as usize).max(1);
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

/// The corners of a cover (SET-010).
const COVER_RADIUS: f32 = 16.0;

/// A cover in `rect`: the picture, or a placeholder with a film.
fn cover(ui: &Ui, rect: Rect, texture: Option<&TextureHandle>) {
    let p = palette::current();
    match texture {
        Some(t) => {
            egui::Image::new(t)
                .corner_radius(COVER_RADIUS)
                .paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, COVER_RADIUS, p.placeholder);
            let side = (rect.height() * 0.3).clamp(20.0, 48.0);
            icons::paint(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(side, side)),
                Icon::Video,
                p.details,
            );
        }
    }
}

/// The card's response: focus asked for, the ring of the keyboard's place
/// (NFR-A11Y-01) around the cover, the cover lighter under the pointer.
fn card_response(ui: &mut Ui, whole: Rect, cover: Rect, name: &str, focus: &mut bool) -> bool {
    let response = ui.interact(
        whole,
        egui::Id::unique(("library-card", name)),
        Sense::click(),
    );
    if std::mem::take(focus) {
        response.request_focus();
    }
    let p = palette::current();
    if response.has_focus() {
        ui.painter().rect_stroke(
            cover.expand(3.0),
            COVER_RADIUS + 3.0,
            Stroke::new(2.0, p.accent_text),
            egui::StrokeKind::Outside,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(cover, COVER_RADIUS, p.row_hover);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name));
    response.clicked()
}

/// A clickable card: the cover, `body` under it; whether it was clicked.
fn card(
    ui: &mut Ui,
    width: f32,
    texture: Option<&TextureHandle>,
    name: &str,
    focus: &mut bool,
    body: impl FnOnce(&mut Ui),
) -> bool {
    let inner = ui.with_layout(Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 4.0;
        let (rect, _) = ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), Sense::hover());
        cover(ui, rect, texture);
        ui.add_space(2.0);
        body(ui);
        rect
    });
    card_response(ui, inner.response.rect, inner.inner, name, focus)
}

/// A large card of what is being watched (SET-010): the title and `detail`
/// over the cover's foot, on a shade that deepens downwards, and the share
/// seen as a wavy line; whether it was clicked.
fn hero_card(
    ui: &mut Ui,
    width: f32,
    texture: Option<&TextureHandle>,
    (title, detail): (&str, &str),
    share: f32,
    name: &str,
    focus: &mut bool,
) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), Sense::hover());
    cover(ui, rect, texture);
    // the shade: from a third of the way down it deepens to the cover's
    // foot, which keeps the cover's lower corners
    let darkest = Color32::from_black_alpha(0xd0);
    let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - COVER_RADIUS), rect.max);
    let fade = Rect::from_min_max(
        pos2(rect.left(), rect.top() + rect.height() * 0.35),
        pos2(rect.right(), foot.top()),
    );
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(fade.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(fade.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(fade.left_bottom(), darkest);
    mesh.colored_vertex(fade.right_bottom(), darkest);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
    let radius = COVER_RADIUS as u8;
    ui.painter().rect_filled(
        foot,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        },
        darkest,
    );
    let margin = 18.0;
    let white = Color32::WHITE;
    let line_y = rect.bottom() - margin;
    let detail_galley = ui.painter().layout(
        detail.to_string(),
        egui::FontId::proportional(14.0),
        Color32::from_gray(0xe0),
        rect.width() - 2.0 * margin,
    );
    let title_galley = ui.painter().layout(
        title.to_string(),
        egui::FontId::new(22.0, theme::bold()),
        white,
        rect.width() - 2.0 * margin,
    );
    let title_y = line_y - 14.0 - title_galley.size().y;
    let detail_y = title_y - detail_galley.size().y - 2.0;
    ui.painter()
        .galley(pos2(rect.left() + margin, detail_y), detail_galley, white);
    ui.painter()
        .galley(pos2(rect.left() + margin, title_y), title_galley, white);
    let p = palette::current();
    wavy_progress(
        ui,
        Rect::from_min_max(
            pos2(rect.left() + margin, line_y - 4.0),
            pos2(rect.right() - margin, line_y + 4.0),
        ),
        share,
        palette::primary_colors(p.accent, true).0,
        Color32::from_white_alpha(0x50),
    );
    card_response(ui, rect, rect, name, focus)
}

/// Material 3 Expressive's progress line in `rect`: the share done as a
/// wave, a gap, then the rest as a flat track ending in a dot.
fn wavy_progress(ui: &Ui, rect: Rect, share: f32, done: Color32, rest: Color32) {
    let share = share.clamp(0.0, 1.0);
    let y = rect.center().y;
    let split = rect.left() + rect.width() * share;
    let stroke = 3.0;
    if split > rect.left() + 1.0 {
        let wavelength = 18.0;
        let amplitude = (rect.height() / 2.0 - stroke / 2.0).max(1.0);
        let steps = ((split - rect.left()) / 2.0).ceil().max(2.0) as usize;
        let points: Vec<_> = (0..=steps)
            .map(|i| {
                let x = rect.left() + (split - rect.left()) * i as f32 / steps as f32;
                let phase = (x - rect.left()) / wavelength * std::f32::consts::TAU;
                pos2(x, y + amplitude * phase.sin())
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, Stroke::new(stroke, done)));
    }
    let gap = 6.0;
    if split + gap < rect.right() {
        ui.painter().line_segment(
            [pos2(split + gap, y), pos2(rect.right(), y)],
            Stroke::new(stroke, rest),
        );
    }
    ui.painter()
        .circle_filled(pos2(rect.right(), y), stroke / 2.0 + 0.5, done);
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
    // a banner on the accent's tonal fill, as Fushi's (SET-010)
    Frame::new()
        .fill(palette::current().selection)
        .corner_radius(20.0)
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                icons::paint(ui.painter(), rect, Icon::Info, palette::current().text);
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
                // ANK-055: on Android the cards go to AnkiDroid
                (
                    if cfg!(target_os = "android") {
                        "first-steps-card-android"
                    } else {
                        "first-steps-card"
                    },
                    "first-steps-card-button",
                ),
            ];
            // a narrow window (a phone upright) wraps the steps and their
            // buttons instead of cutting them (PLY-024)
            // INS-001: all of it at once
            if !steps[0] || !steps[2] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr(s, "setup-first-steps"));
                    if ui.add(palette::suggested(tr(s, "setup-open"))).clicked() {
                        out.push(Command::SetupOpen);
                    }
                });
            }
            for (n, ((text, button), done)) in rows.into_iter().zip(steps).enumerate() {
                ui.horizontal_wrapped(|ui| {
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
    if let Some(id) = state.analysis {
        if s.colours
            && let Some((serial, words)) = s.page.scores.words(id)
            && let Some(series) = home.series.iter().find(|s| s.id == id)
            && let Some(crate::series_scores::Status::Ready(summary)) = s.page.scores.get(id)
        {
            ui.label((s.tr)(
                "library-score-coverage",
                &[
                    ("scored", summary.scored.to_string()),
                    ("total", summary.total.to_string()),
                    ("failed", summary.failed.to_string()),
                ],
            ));
            if super::analysis_words::show(
                ui,
                format!("series/{id}/{serial}"),
                &series.record.title,
                words,
                &mut state.analysis_words,
                s.tr,
                out,
            ) {
                state.analysis = None;
            }
            return;
        }
        state.analysis = None;
        state.analysis_words = Default::default();
    }
    if let Some(steps) = s.first_steps {
        first_steps(ui, s, steps, state, out);
    }
    // the collection's own: a video opened on its own is on the Video shelf
    let continuing: Vec<(String, VideoRecord)> = home
        .continue_watching
        .iter()
        .filter(|(_, v)| v.series.is_some())
        .cloned()
        .collect();
    if !continuing.is_empty() {
        section(ui, &tr(s, "library-continue"));
        rows(ui, HERO, &continuing, |ui, (uid, v)| {
            let series = v.series.and_then(|id| home.series_title(id));
            let title = home
                .server_titles
                .get(uid)
                .map(String::as_str)
                .or(series)
                .map_or_else(|| file_title(&v.path), str::to_string);
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
            let share = v
                .duration_ms
                .map_or(0.0, |d| v.last_position_ms as f32 / d.max(1) as f32);
            let clicked = hero_card(
                ui,
                HERO,
                texture,
                (&title, &detail),
                share,
                &name,
                &mut state.focus_first,
            );
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
    if s.colours {
        state.analysis_order.controls(ui, s.tr);
        let ready = home.series.iter().any(|series| {
            matches!(
                s.page.scores.get(series.id),
                Some(crate::series_scores::Status::Ready(_))
            )
        });
        if ui
            .add_enabled(ready, Button::new(tr(s, "study-report-export")))
            .clicked()
        {
            out.push(Command::StudyReport(crate::study_report::Scope::Series));
        }
    }
    let indices = if s.colours {
        state.analysis_order.indices(home.series.len(), |i| {
            match s.page.scores.get(home.series[i].id) {
                Some(crate::series_scores::Status::Ready(summary)) => Some(summary.score),
                _ => None,
            }
        })
    } else {
        (0..home.series.len()).collect()
    };
    let shown: Vec<_> = indices.into_iter().map(|i| &home.series[i]).collect();
    rows(ui, NARROW, &shown, |ui, summary| {
        ui.vertical(|ui| {
            let clicked = card(
                ui,
                NARROW,
                state.textures.get(&summary.id),
                &summary.record.title,
                &mut state.focus_first,
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(&summary.record.title).family(theme::bold()),
                        )
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
            if s.colours {
                ui.set_width(NARROW);
                ui.push_id(("series-score", summary.id), |ui| {
                    series_score(ui, s, summary.id, &summary.record.title, state, out);
                });
            }
        });
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

fn series_score(
    ui: &mut Ui,
    s: &Scene<'_>,
    id: u64,
    title: &str,
    state: &mut State,
    out: &mut Vec<Command>,
) {
    use crate::series_scores::Status;
    let status = s.page.scores.get(id);
    match status {
        Some(Status::Loading) => {
            ui.label(tr(s, "library-score-loading"));
        }
        Some(Status::Counting { done, total }) => {
            ui.label((s.tr)(
                "library-score-counting",
                &[("done", done.to_string()), ("total", total.to_string())],
            ));
        }
        Some(Status::Ready(summary)) => {
            let percent =
                |value: Option<usize>| value.map_or_else(|| "–".into(), |n| n.to_string());
            let args = [
                ("known", percent(summary.score.known_percent())),
                ("clear", percent(summary.score.clear_percent())),
                ("scored", summary.scored.to_string()),
                ("total", summary.total.to_string()),
                ("failed", summary.failed.to_string()),
            ];
            ui.label((s.tr)("library-score-summary", &args))
                .on_hover_text((s.tr)("library-score-detail", &args));
            ui.label((s.tr)("library-score-coverage", &args));
        }
        Some(Status::Unavailable { total, failed }) => {
            ui.label(tr(s, "library-score-unavailable"))
                .on_hover_text((s.tr)(
                    "library-score-coverage",
                    &[
                        ("scored", "0".into()),
                        ("total", total.to_string()),
                        ("failed", failed.to_string()),
                    ],
                ));
        }
        Some(Status::TooLarge) => {
            ui.label(tr(s, "library-score-too-large"));
        }
        Some(Status::Failed(error)) => {
            ui.label(tr(s, "library-score-unavailable"))
                .on_hover_text(error);
        }
        None => {}
    }
    if matches!(status, Some(Status::Loading | Status::Counting { .. }))
        && ui.button(tr(s, "analysis-cancel")).clicked()
    {
        out.push(Command::SeriesAnalysisCancel(id));
    }
    if s.page.scores.words(id).is_some() && ui.button(tr(s, "analysis-words")).clicked() {
        state.analysis = Some(id);
    }
    let button = ui.add_enabled(
        !s.page.busy && !s.page.scores.busy(),
        Button::new(tr(
            s,
            if status.is_some() {
                "library-score-refresh"
            } else {
                "library-score-analyse"
            },
        )),
    );
    button.widget_info(|| {
        WidgetInfo::labeled(
            Role::Button,
            button.enabled(),
            (s.tr)("library-score-analyse-title", &[("title", title.into())]),
        )
    });
    if button.clicked() {
        out.push(Command::SeriesAnalyse(id));
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

/// MET-006: identify the series from one of its files through AniDB; enabled
/// once an AniDB account is in Settings → Subtitle services.
fn anidb_button(ui: &mut Ui, s: &Scene<'_>, id: u64, free: bool, out: &mut Vec<Command>) {
    let hint = if s.anidb {
        "library-identify-file-hint"
    } else {
        "library-identify-file-setup"
    };
    let button = ui
        .add_enabled(free && s.anidb, Button::new(tr(s, "library-identify-file")))
        .on_hover_text(tr(s, hint))
        .on_disabled_hover_text(tr(s, hint));
    if button.clicked() {
        out.push(Command::LibraryIdentifyFile(id));
    }
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
                anidb_button(ui, s, id, identify.is_none(), out);
                if ui.button(tr(s, "library-forget")).clicked() {
                    out.push(Command::LibraryForget(id));
                }
            });
        }
        None if identify.is_none() => {
            ui.horizontal_wrapped(|ui| {
                let button = ui
                    .button(tr(s, "library-identify"))
                    .on_hover_text(tr(s, "library-identify-hint"));
                if button.clicked() {
                    out.push(Command::LibraryIdentify(id));
                }
                anidb_button(ui, s, id, true, out);
            });
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
    if s.colours
        && let Some(study) = &s.page.episode_words
    {
        if super::analysis_words::show(
            ui,
            format!("episode/{}/{}", study.uid, study.epoch),
            &study.title,
            &study.words,
            &mut state.episode_words,
            s.tr,
            out,
        ) {
            out.push(Command::EpisodeWords(None));
        }
        return;
    }
    let home = s.page.home.as_ref();
    let title = home
        .and_then(|h| h.series_title(id))
        .unwrap_or_default()
        .to_string();
    let mut back = false;
    ui.horizontal(|ui| {
        back = ui.button(tr(s, "library-back")).clicked();
        ui.add(egui::Label::new(RichText::new(&title).size(22.0).family(theme::bold())).truncate());
    });
    if back {
        state.reset();
        out.push(Command::LibrarySeries(None));
        return;
    }
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
    // LIB-011: rename, merge; LIB-012: delete; all on one row, wrapped
    toolbar(ui, |ui| {
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
        if !others.is_empty() {
            ui.separator();
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
        }
        ui.separator();
        if ui.button(tr(s, "library-delete")).clicked() {
            out.push(Command::LibraryEdit(LibraryEdit::Delete(id)));
            out.push(Command::LibrarySeries(None));
        }
    });
    let Some(episodes) = episodes else {
        ui.spinner();
        return;
    };
    super::condense::menu(
        ui,
        s.tr,
        crate::condense::Target::Series(id),
        s.exporting,
        out,
    );
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
                    // the buttons first, at the right: the name takes what is
                    // left and is cut there, never under them
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
                        super::condense::menu(
                            ui,
                            s.tr,
                            crate::condense::Target::Episode(uid.clone()),
                            s.exporting,
                            out,
                        );
                        episode_score(ui, s, uid);
                        if s.colours
                            && matches!(
                                (s.episode_score)(uid),
                                Some(crate::scores::EpisodeScore::Score(_))
                            )
                        {
                            let button = ui.button(tr(s, "analysis-words"));
                            button.widget_info(|| {
                                WidgetInfo::labeled(
                                    Role::Button,
                                    true,
                                    (s.tr)(
                                        "episode-words-title",
                                        &[("title", file_title(&v.path))],
                                    ),
                                )
                            });
                            if button.clicked() {
                                out.push(Command::EpisodeWords(Some(uid.clone())));
                            }
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
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(egui::Label::new(file_title(&v.path)).truncate());
                        });
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
    // LIB-014
    ui.horizontal_wrapped(|ui| {
        for (key, watched) in [
            ("library-mark-watched", true),
            ("library-mark-unwatched", false),
        ] {
            if ui.button(tr(s, key)).clicked() {
                out.push(Command::LibraryEdit(LibraryEdit::Watched {
                    uids: uids.clone(),
                    watched,
                }));
                state.selected.clear();
            }
        }
    });
    if ui.button(tr(s, "library-remove")).clicked() {
        out.push(Command::LibraryEdit(LibraryEdit::Remove(uids)));
        state.selected.clear();
    }
}

/// The score of an episode, short, with the whole of it on hover
/// (KNW-008): the words known and the lines with no new word.
fn episode_score(ui: &mut Ui, s: &Scene<'_>, uid: &str) {
    let color = palette::current().details;
    match (s.episode_score)(uid) {
        Some(crate::scores::EpisodeScore::Counting) => {
            ui.label(RichText::new("…").color(color))
                .on_hover_text(tr(s, "library-score-counting"));
        }
        Some(crate::scores::EpisodeScore::Score(score)) => {
            let percent = |p: Option<usize>| p.map_or_else(|| "–".into(), |p| p.to_string());
            let (known, clear) = (
                percent(score.known_percent()),
                percent(score.clear_percent()),
            );
            let full = (s.tr)(
                "subtitle-score",
                &[("known", known.clone()), ("clear", clear.clone())],
            );
            ui.label(RichText::new(format!("{known}% · {clear}%")).color(color))
                .on_hover_text(full);
        }
        None => {}
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
    use egui_kittest::kittest::{NodeT, Queryable};
    use nikomiru_library::home::SeriesSummary;
    use nikomiru_library::{FolderRecord, SeriesRecord};
    use nikomiru_online::servers::Item;

    struct Test {
        page: LibraryPage,
        first_steps: Option<[bool; 3]>,
        servers: Vec<nikomiru_core::config::ServerConfig>,
        server_page: crate::servers::ServersPage,
        state: State,
        out: Vec<Command>,
        closed: bool,
        /// The episodes' scores (KNW-008).
        scores: HashMap<String, crate::scores::EpisodeScore>,
        colours: bool,
        anidb: bool,
        books: crate::books::BooksPage,
        links: Vec<nikomiru_core::config::SavedLink>,
    }

    fn harness(page: LibraryPage) -> Harness<'static, Test> {
        harness_sized(page, vec2(1280.0, 1000.0))
    }

    fn harness_sized(page: LibraryPage, size: egui::Vec2) -> Harness<'static, Test> {
        Harness::builder()
            .with_size(size)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, t: &mut Test| {
                    let _ = ui.interact(ui.max_rect(), Id::unique("video"), Sense::click());
                    let tr = |id: &str, _: &[(&str, String)]| id.to_string();
                    let scene = Scene {
                        page: &t.page,
                        today: 0,
                        first_steps: t.first_steps,
                        servers: &t.servers,
                        server_page: &t.server_page,
                        settings_tab: crate::view::settings::SettingsTab::Anki,
                        now: crate::view::review_view::Now {
                            ms: 1_760_000_000_000,
                            offset_s: 0,
                        },
                        review_settings: nikomiru_library::review::Settings::default(),
                        to_review: false,
                        exporting: false,
                        colours: t.colours,
                        anidb: t.anidb,
                        episode_score: &|uid: &str| t.scores.get(uid).copied(),
                        books: &t.books,
                        links: &t.links,
                        tr: &tr,
                    };
                    t.closed |= draw(ui.ctx(), &scene, &mut t.state, &mut t.out);
                },
                Test {
                    page,
                    first_steps: None,
                    servers: Vec::new(),
                    server_page: crate::servers::ServersPage::default(),
                    state: State::default(),
                    out: Vec::new(),
                    closed: false,
                    scores: HashMap::new(),
                    colours: false,
                    anidb: false,
                    books: crate::books::BooksPage::default(),
                    links: Vec::new(),
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
            server_titles: Default::default(),
            history: vec![
                (
                    "web/a".into(),
                    VideoRecord {
                        path: "https://example.com/watch?v=a".into(),
                        duration_ms: Some(600_000),
                        last_position_ms: 60_000,
                        last_played_at: Some(5),
                        ..VideoRecord::default()
                    },
                ),
                (
                    "video/clip".into(),
                    VideoRecord {
                        path: "/tmp/clip.mkv".into(),
                        last_played_at: Some(4),
                        ..VideoRecord::default()
                    },
                ),
            ],
        }
    }

    // T-VID-001, T-VID-002, T-BOK-015: the shelves are tabs; videos opened on
    // their own and links kept are on the Video shelf, books, manga and web
    // pages each on their own, and nothing of a shelf shows on another
    #[test]
    fn the_shelves_sort_what_was_added() {
        let page = LibraryPage {
            home: Some(home()),
            ..LibraryPage::default()
        };
        let mut h = harness(page);
        let book = |path: &str, title: &str| {
            (
                format!("uid:{title}"),
                nikomiru_library::books::BookRecord {
                    path: path.into(),
                    title: title.into(),
                    chars: 100,
                    read_chars: 50,
                    ..Default::default()
                },
            )
        };
        h.state_mut().books.list = vec![
            book("/b/novel.epub", "Novel"),
            book("/b/volume.cbz", "Volume"),
            book("/b/pages", "Pages"),
            book("https://example.com/story", "Story"),
        ];
        h.state_mut().links = vec![nikomiru_core::config::SavedLink {
            url: "https://example.com/watch?v=b".into(),
            title: "Later".into(),
            added_at: 1,
        }];
        h.run();
        // the collection: series, no single video
        assert!(h.query_all_by_label("Alpha").next().is_some());
        assert!(h.query_all_by_label("clip").next().is_none());
        let out = click(&mut h, "tab-videos");
        assert!(out.is_empty(), "{out:?}");
        assert!(h.query_all_by_label("clip").next().is_some());
        assert!(
            h.query_all_by_label("https://example.com/watch?v=a")
                .next()
                .is_some()
        );
        assert!(h.query_all_by_label("Later").next().is_some());
        assert!(h.query_all_by_label("Alpha").next().is_none());
        // the first Play is the saved link's
        h.query_all_by_label("videos-play").next().unwrap().click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(&out[..], [Command::WebOpen(u)] if u == "https://example.com/watch?v=b"),
            "{out:?}"
        );
        // a web video of the history can be kept as a link, then forgotten
        // the form's own button is first, and off without an address
        h.query_all_by_label("videos-link-keep")
            .last()
            .unwrap()
            .click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(&out[..], [Command::LinkKeep(u, _)] if u.contains("v=a")),
            "{out:?}"
        );
        h.query_all_by_label("videos-forget")
            .next()
            .unwrap()
            .click();
        h.run();
        assert!(matches!(
            &h.state().out[..],
            [Command::LibraryEdit(LibraryEdit::Remove(u))] if u == &["web/a".to_string()]
        ));
        h.state_mut().out.clear();
        // reading shelves: each its own kind, opened through the Books page
        let out = click(&mut h, "tab-manga");
        assert!(matches!(&out[..], [Command::BooksOpen]), "{out:?}");
        assert!(h.query_all_by_label("Volume").next().is_some());
        assert!(h.query_all_by_label("Pages").next().is_some());
        assert!(h.query_all_by_label("Novel").next().is_none());
        h.get_by_label("manga-add").click();
        h.run();
        assert_eq!(h.state().state.books_action, Some(BooksAction::Add));
        h.state_mut().state.books_action = None;
        click(&mut h, "tab-books");
        assert!(
            h.query_all_by_label("Novel").next().is_some()
                && h.query_all_by_label("Volume").next().is_none()
        );
        click(&mut h, "books-continue");
        assert_eq!(
            h.state().state.books_action,
            Some(BooksAction::Open("uid:Novel".into()))
        );
        click(&mut h, "tab-web");
        assert!(
            h.query_all_by_label("Story").next().is_some()
                && h.query_all_by_label("Novel").next().is_none()
        );
        // the tools are in one menu
        click(&mut h, "library-tools");
        let out = click(&mut h, "phrases-title");
        assert!(matches!(&out[..], [Command::PhrasesOpen]), "{out:?}");
    }

    // T-KNW-031: episode vocabulary never triggers playback and reuses shared controls.
    #[test]
    fn episode_words_action_and_browser_keep_playback_untouched() {
        let page = LibraryPage {
            home: Some(home()),
            series: Some((
                1,
                Some(vec![("ep/a".into(), video("/v/Alpha - 01.mkv", 1, 1, 0))]),
            )),
            ..Default::default()
        };
        let mut h = harness(page);
        h.state_mut().colours = true;
        h.state_mut().scores.insert(
            "ep/a".into(),
            crate::scores::EpisodeScore::Score(crate::words::Score {
                words: 2,
                ..Default::default()
            }),
        );
        h.run();
        h.get_by_label("episode-words-title").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::EpisodeWords(Some(uid)) if uid == "ep/a"))
        );
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::LibraryPlay(_)))
        );
        h.state_mut().page.episode_words = Some(crate::catalog::EpisodeVocabulary {
            uid: "ep/a".into(),
            title: "Alpha - 01".into(),
            epoch: 1,
            revision: 0,
            words: std::sync::Arc::new(vec![crate::words::ListedWord {
                expression: "猫".into(),
                reading: "ねこ".into(),
                count: 2,
                status: crate::words::WordStatus::Unknown,
                card: false,
            }]),
        });
        h.run_steps(2);
        h.get_by_label("×2");
        h.get_by_label("subtitle-words-all").click();
        h.run();
        h.get_by_label("word-status-known").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c, Command::WordsStatus(rows, crate::words::WordStatus::Known) if rows.len() == 1)));
        h.get_by_label("analysis-words-back").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::EpisodeWords(None)))
        );
    }

    // T-KNW-021: vocabulary actions keep library navigation and playback untouched.
    #[test]
    fn series_vocabulary_shows_frequency_and_bulk_status_controls() {
        let mut page = LibraryPage {
            home: Some(home()),
            ..Default::default()
        };
        let words = crate::words::Words::default();
        page.scores.update(true, &words, &[], "", &[1]);
        let request = page.scores.request(1).unwrap();
        page.scores
            .loaded(request, Ok(vec![("ep/a".into(), VideoRecord::default())]));
        let (_, uid, _) = page.scores.next_episode().unwrap();
        page.scores.done(
            request,
            &uid,
            Ok(Some(crate::words::Counted::from_lines([vec![
                (
                    "猫".into(),
                    "ねこ".into()
                );
                3
            ]]))),
        );
        page.scores.update(true, &words, &[], "", &[1]);
        let mut h = harness(page);
        h.state_mut().colours = true;
        h.run();
        // T-KNW-024: only completed/scorable series remain, no automatic work.
        h.get_by_label("analysis-order");
        assert!(h.query_all_by_label("Beta").count() > 0);
        h.get_by_label("analysis-only-scored").click();
        h.run_steps(2);
        assert_eq!(h.query_all_by_label("Beta").count(), 0);
        assert!(h.state().out.is_empty());
        // T-KNW-033: export all completed series summaries without opening them.
        h.get_by_label("study-report-export").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::StudyReport(crate::study_report::Scope::Series)))
        );
        h.state_mut().out.clear();

        h.get_by_label("analysis-words").click();
        h.run_steps(2);
        h.get_by_label("×3");
        h.get_by_label("subtitle-words-all").click();
        h.run();
        h.get_by_label("word-status-learning").click();
        h.run();
        assert!(h.state().out.iter().any(|c| matches!(c,
            Command::WordsStatus(words, crate::words::WordStatus::Learning) if words.len() == 1)));
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::LibrarySeries(_) | Command::LibraryPlay(_)))
        );
        h.state_mut().page.scores.clear();
        h.run_steps(2);
        assert!(h.query_by_label("analysis-words-back").is_none());
    }

    // T-KNW-017: the explicit card action is separate from opening the
    // series and is hidden/disabled with colours off/another analysis.
    #[test]
    fn a_series_card_requests_analysis_without_opening_it() {
        let mut page = LibraryPage {
            home: Some(home()),
            ..Default::default()
        };
        page.home.as_mut().unwrap().series.truncate(1);
        let mut h = harness(page);
        h.run();
        assert!(h.query_by_label("library-score-analyse-title").is_none());
        h.state_mut().colours = true;
        h.run();
        h.state_mut().out.clear();
        // T-KNW-033: no completed result means the report action cannot emit a command.
        h.get_by_label("study-report-export").click();
        h.run();
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::StudyReport(_)))
        );
        h.get_by_label("library-score-analyse-title").click();
        h.run();
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SeriesAnalyse(1)))
        );
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::LibrarySeries(_)))
        );
        h.state_mut().out.clear();
        h.state_mut().page.scores.request(1).unwrap();
        h.run_steps(2);
        h.get_by_label("library-score-analyse-title").click();
        h.run_steps(2);
        assert!(
            !h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SeriesAnalyse(_)))
        );
        // T-KNW-019: accessible action is available during active analysis.
        h.get_by_label("analysis-cancel").click();
        h.run_steps(2);
        assert!(
            h.state()
                .out
                .iter()
                .any(|c| matches!(c, Command::SeriesAnalysisCancel(1)))
        );
    }

    fn server_item(id: &str, kind: nikomiru_online::servers::ItemKind) -> Item {
        Item {
            id: id.into(),
            kind,
            title: format!("Title {id}"),
            series: None,
            parent: None,
            season: None,
            episode: None,
            cover: None,
            played: false,
            position_ms: 0,
            duration_ms: 0,
            last_played_s: None,
            anilist_id: None,
        }
    }

    fn review_page(revealed: bool) -> crate::review::ReviewPage {
        use nikomiru_library::review::{Memory, Queue, ReviewCard};
        let card = ReviewCard {
            expression: "食べる".into(),
            reading: "たべる".into(),
            sentence: "パンを食べる".into(),
            word: Some((9, 18)),
            glossaries: vec![("JMdict".into(), "• to eat".into())],
            audio: None,
            picture: None,
            video_uid: "video/av_basic".into(),
            title: "av_basic".into(),
            start_ms: 0,
            end_ms: 1000,
            created_at: 1,
            memory: Memory::default(),
        };
        crate::review::ReviewPage {
            review: Some(crate::review::Review {
                queue: Queue {
                    cards: vec![(7, card)],
                    new: 1,
                    total: 1,
                    ..Queue::default()
                },
                picture: None,
                audio: Some("/r/a.m4a".into()),
                list: vec![
                    crate::review::CardRow {
                        id: 7,
                        expression: "食べる".into(),
                        reading: "たべる".into(),
                        sentence: "パンを食べる".into(),
                        memory: Memory::default(),
                    },
                    crate::review::CardRow {
                        id: 8,
                        expression: "飲む".into(),
                        reading: "のむ".into(),
                        sentence: "水を飲む".into(),
                        memory: Memory {
                            phase: nikomiru_library::review::Phase::Review,
                            due_ms: 1_760_000_000_000 + 3 * 86_400_000,
                            reps: 4,
                            lapses: 1,
                            ..Memory::default()
                        },
                    },
                ],
            }),
            revealed,
            ..crate::review::ReviewPage::default()
        }
    }

    // T-REV-004: the tab shows the word, then its answer and the four grades
    #[test]
    fn the_review_tab_asks_then_grades() {
        use crate::review::ReviewJob;
        use nikomiru_library::review::Grade;
        let mut h = harness(LibraryPage {
            home: Some(home()),
            review: review_page(false),
            ..LibraryPage::default()
        });
        h.run();
        // T-REV-008 the statistics read again when shown
        // (a spinner on the tab while it reads: a step, not a run)
        h.get_by_label("stats-title").click();
        h.step();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(matches!(&out[..], [Command::StatsLoad]), "{out:?}");
        let out = click(&mut h, "review-title-due");
        assert!(
            matches!(&out[..], [Command::Review(ReviewJob::Load)]),
            "{out:?}"
        );
        assert!(h.query_by_label("食べる").is_some());
        assert!(h.query_by_label("review-again").is_none(), "no answer yet");
        assert!(matches!(
            &click(&mut h, "review-show")[..],
            [Command::ReviewReveal]
        ));
        h.state_mut().page.review = review_page(true);
        h.run();
        assert!(
            h.query_by_label("たべる").is_some(),
            "the reading over the word"
        );
        assert!(h.query_by_label("• to eat").is_some());
        // a new card: Again in a minute, Good in ten
        assert!(h.query_by_label("review-again\nreview-minutes").is_some());
        let out = click(&mut h, "review-good\nreview-minutes");
        assert!(
            matches!(
                &out[..],
                [Command::Review(ReviewJob::Answer(7, Grade::Good))]
            ),
            "{out:?}"
        );
        h.key_press(egui::Key::Num4);
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(
            matches!(
                &out[..],
                [Command::Review(ReviewJob::Answer(7, Grade::Easy))]
            ),
            "{out:?}"
        );
        assert!(matches!(
            &click(&mut h, "review-sentence-audio")[..],
            [Command::ReviewAudio]
        ));
        assert!(matches!(
            &click(&mut h, "review-scene")[..],
            [Command::ReviewScene]
        ));
        assert!(matches!(
            &click(&mut h, "review-delete")[..],
            [Command::ReviewDelete]
        ));
    }

    // T-REV-009: the list of the cards, searched; Start over and Delete each
    // asked twice
    #[test]
    fn the_cards_are_listed_searched_and_started_over() {
        use crate::review::ReviewJob;
        let mut h = harness(LibraryPage {
            home: Some(home()),
            review: review_page(false),
            ..LibraryPage::default()
        });
        h.run();
        click(&mut h, "review-title-due");
        click(&mut h, "review-cards");
        assert!(h.state().state.review_state.browse);
        assert!(h.query_by_label("水を飲む").is_some());
        assert!(
            h.query_by_label("review-phase-review · review-days · review-answers")
                .is_some()
        );
        // a new card cannot start over
        click(&mut h, "review-reset 食べる");
        assert!(
            h.query_by_label("review-reset-confirm 食べる").is_none(),
            "a click on it does nothing"
        );
        assert!(click(&mut h, "review-reset 飲む").is_empty(), "asked once");
        let out = click(&mut h, "review-reset-confirm 飲む");
        assert!(
            matches!(&out[..], [Command::Review(ReviewJob::Reset(8))]),
            "{out:?}"
        );
        assert!(click(&mut h, "review-delete 食べる").is_empty());
        let out = click(&mut h, "review-delete-confirm 食べる");
        assert!(
            matches!(&out[..], [Command::Review(ReviewJob::Delete(7))]),
            "{out:?}"
        );
        h.state_mut().state.review_state.search = "飲".into();
        h.run();
        assert!(h.query_by_label("水を飲む").is_some());
        assert!(h.query_by_label("パンを食べる").is_none());
        click(&mut h, "review-study");
        assert!(!h.state().state.review_state.browse);
    }

    // T-SET-010: the rail on a wide window, compact or expanded; the tabs on
    // a narrow one; each leads where the tabs led
    #[test]
    fn the_rail_or_the_tabs_lead_to_the_shelves() {
        let page = || LibraryPage {
            home: Some(home()),
            ..LibraryPage::default()
        };
        // expanded: the app's name heads the rail, the page has the shelf's
        let mut h = harness_sized(page(), vec2(1500.0, 900.0));
        h.run();
        assert!(h.query_by_label("app-title").is_some());
        assert_eq!(
            h.get_all_by_label("tab-collection").count(),
            2,
            "the rail and the title"
        );
        click(&mut h, "tab-videos");
        assert_eq!(h.state().state.shelf, Shelf::Videos);
        // compact: no name of the app, the same items
        let mut h = harness_sized(page(), vec2(1000.0, 900.0));
        h.run();
        assert!(h.query_by_label("app-title").is_none());
        click(&mut h, "tab-videos");
        assert_eq!(h.state().state.shelf, Shelf::Videos);
        click(&mut h, "library-tools");
        let out = click(&mut h, "phrases-title");
        assert!(matches!(&out[..], [Command::PhrasesOpen]), "{out:?}");
        // narrow: the tabs above the page, under the app's name
        let mut h = harness_sized(page(), vec2(700.0, 900.0));
        h.run();
        assert!(h.query_by_label("app-title").is_some());
        click(&mut h, "tab-videos");
        assert_eq!(h.state().state.shelf, Shelf::Videos);
    }

    // T-SET-008: the settings, from the library too
    #[test]
    fn the_settings_open_from_the_library() {
        let mut h = harness(LibraryPage {
            home: Some(home()),
            ..LibraryPage::default()
        });
        h.run();
        click(&mut h, "settings-title");
        assert_eq!(
            h.state_mut().state.open_settings.take(),
            Some(crate::view::settings::SettingsTab::Anki),
            "the tab shown last"
        );
    }

    // T-SRV-006: a server's tab, from its libraries to a video
    #[test]
    fn a_server_is_browsed_from_its_tab() {
        use nikomiru_online::servers::{ItemKind, Library, ParentKind};

        use crate::servers::{Listing, Place};
        let mut h = harness(LibraryPage {
            home: Some(home()),
            ..LibraryPage::default()
        });
        h.state_mut().servers = vec![nikomiru_core::config::ServerConfig {
            id: "a".into(),
            name: "Home".into(),
            ..Default::default()
        }];
        h.run();
        let out = click(&mut h, "Home");
        assert!(
            matches!(out.as_slice(), [Command::ServerShow(id)] if id == "a"),
            "{out:?}"
        );
        h.state_mut().server_page.libraries = vec![Library {
            id: "lib".into(),
            title: "Anime".into(),
        }];
        h.run();
        let out = click(&mut h, "Anime");
        assert!(matches!(
            out.as_slice(),
            [Command::ServerGo(path)] if path.len() == 1 && path[0].parent == "lib"
        ));
        h.state_mut().server_page.path = vec![Place {
            parent: "lib".into(),
            title: "Anime".into(),
            kind: ParentKind::Library,
        }];
        h.state_mut().server_page.listing = Some(Listing {
            items: vec![
                server_item("s", ItemKind::Series),
                server_item("e", ItemKind::Episode),
            ],
            total: 3,
        });
        h.run();
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Title s")
            .click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(matches!(
            out.as_slice(),
            [Command::ServerGo(path)] if path.len() == 2 && path[1].parent == "s"
        ));
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Title e")
            .click();
        h.run();
        let out = std::mem::take(&mut h.state_mut().out);
        assert!(matches!(out.as_slice(), [Command::ServerPlay(id)] if id == "e"));
        // 2 of 3: there is more
        let out = click(&mut h, "servers-more");
        assert!(matches!(out.as_slice(), [Command::ServerMore]));
    }

    // T-MET-001, T-MET-006: a series asked online, the result chosen, forgotten
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
        // T-MET-006: by file only once an AniDB account is set
        assert!(
            h.get_by_label("library-identify-file")
                .accesskit_node()
                .is_disabled()
        );
        h.state_mut().anidb = true;
        h.run();
        assert!(matches!(
            click(&mut h, "library-identify-file")[..],
            [Command::LibraryIdentifyFile(1)]
        ));
        h.state_mut().anidb = false;
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
    // T-KNW-008: each episode's score, or that it is being counted
    #[test]
    fn the_episodes_show_their_scores() {
        use crate::scores::EpisodeScore;
        let episodes = vec![
            ("a".to_string(), video("/v/Alpha - 01.mkv", 1, 1, 0)),
            ("b".to_string(), video("/v/Alpha - 02.mkv", 1, 2, 0)),
            ("c".to_string(), video("/v/Alpha - 03.mkv", 1, 3, 0)),
        ];
        let mut h = harness(LibraryPage {
            home: Some(home()),
            series: Some((1, Some(episodes))),
            ..LibraryPage::default()
        });
        let score = nikomiru_library::words::Score {
            words: 10,
            known: 8,
            lines: 4,
            clear: 1,
        };
        h.state_mut().scores = HashMap::from([
            ("a".to_string(), EpisodeScore::Score(score)),
            ("b".to_string(), EpisodeScore::Counting),
        ]);
        h.run();
        h.get_by_label("80% · 25%");
        h.get_by_label("…");
        assert_eq!(h.query_all_by_label_contains("%").count(), 1, "c has none");
    }

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
        // T-LIB-014: ticked episodes marked watched, then new again
        for (key, watched) in [
            ("library-mark-watched", true),
            ("library-mark-unwatched", false),
        ] {
            h.get_by_label("E02 Alpha - 02").click();
            h.run();
            let out = click(&mut h, key);
            assert!(
                matches!(out.as_slice(), [Command::LibraryEdit(LibraryEdit::Watched { uids, watched: w })]
                    if uids == &["video/Alpha - 02"] && *w == watched),
                "{out:?}"
            );
        }
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
