// SPDX-License-Identifier: GPL-3.0-only

//! The view: draws the session every frame, bottom to top, and turns clicks
//! into [`Command`]s. It only reads the session; the commands run after the
//! frame, so nothing changes under the drawing.
//!
//! Layers, bottom to top: the video (drawn by mpv before egui paints), the
//! subtitles, the subtitle list, the control bar, the dictionary popups, the
//! dictionaries page, the toast.

mod anki_page;
mod audio_page;
mod backup_page;
mod bar;
mod books_page;
mod chrome;
mod dict_page;
mod hooker_page;
mod library_page;
mod link;
mod list;
mod mining_dialog;
mod online_page;
mod overlay;
pub(crate) mod palette;
mod player_page;
mod popups;
mod prefs_page;
mod servers_page;
mod services_page;
pub(crate) mod settings;
mod shortcuts_page;
mod stats_view;
pub(crate) mod theme;

use std::collections::HashSet;
use std::path::PathBuf;

use egui::{CursorIcon, Id, Sense, ViewportCommand};

use crate::session::{Session, UiEffect};
use crate::ui::Command;

/// The file dialogs of the interface, opened by the app through the XDG portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    /// A video to play (LIB-001).
    Video,
    /// A subtitle file for the open video (SUB-006).
    Subtitles,
    /// A Yomitan dictionary to import (DIC-001).
    Dictionary,
    /// The moved video of a link to a scene (ANK-052).
    Locate,
    /// An `mpv.conf` to load (PLY-016).
    MpvImport,
    /// Where to export the mpv options (PLY-016).
    MpvExport,
    /// A folder to import into the library (LIB-002).
    LibraryFolder,
    /// A database of the Local Audio Server (AUD-003).
    AudioDatabase,
    /// Where to export the study sessions (STA-005).
    StatsExport,
    /// A book to add (BOK-004).
    Book,
    /// Where to write a backup (BAK-001).
    BackupSave,
    /// A backup to restore (BAK-003).
    BackupOpen,
}

/// A file dialog to open: title and filters already translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDialog {
    /// Which dialog: the answer comes back with it.
    pub dialog: Dialog,
    /// The window title.
    pub title: String,
    /// Named filters, as glob patterns (`*.mkv`).
    pub filters: Vec<(String, Vec<&'static str>)>,
    /// A dialog that saves: the name it suggests.
    pub save_as: Option<String>,
    /// The folder it starts in, when not the portal's choice.
    pub folder: Option<PathBuf>,
    /// It chooses a folder, not a file.
    pub directory: bool,
}

pub(crate) const VIDEO_PATTERNS: [&str; 14] = [
    "*.mp4", "*.mkv", "*.avi", "*.mov", "*.webm", "*.m4v", "*.ts", "*.m2ts", "*.mts", "*.flv",
    "*.wmv", "*.mpg", "*.mpeg", "*.ogv",
];
const SUBTITLE_PATTERNS: [&str; 4] = ["*.srt", "*.ass", "*.ssa", "*.vtt"];

impl FileDialog {
    pub(crate) fn new(dialog: Dialog, s: &Session) -> Self {
        let tr = |id: &str| s.translator().tr(id, &[]);
        let all = (tr("file-filter-all"), vec!["*"]);
        let (title, filters) = match dialog {
            Dialog::Video => (
                tr("open-file"),
                vec![(tr("file-filter-video"), VIDEO_PATTERNS.to_vec()), all],
            ),
            Dialog::Subtitles => (
                tr("subtitle-open-file"),
                vec![
                    (tr("file-filter-subtitles"), SUBTITLE_PATTERNS.to_vec()),
                    all,
                ],
            ),
            Dialog::Dictionary => (
                tr("dict-import"),
                vec![(
                    tr("dict-import-filter"),
                    vec!["*.zip", "*.ifo", "*.dsl", "*.dsl.dz", "*.mdx"],
                )],
            ),
            Dialog::Locate => (
                tr("link-locate"),
                vec![(tr("file-filter-video"), VIDEO_PATTERNS.to_vec()), all],
            ),
            Dialog::MpvImport => (
                tr("player-import"),
                vec![(tr("player-filter-conf"), vec!["*.conf"]), all],
            ),
            Dialog::MpvExport => (
                tr("player-export"),
                vec![(tr("player-filter-conf"), vec!["*.conf"]), all],
            ),
            Dialog::LibraryFolder => (tr("library-add-folder"), Vec::new()),
            Dialog::AudioDatabase => (
                tr("audio-database-button"),
                vec![(tr("audio-database-filter"), vec!["*.db"]), all],
            ),
            Dialog::StatsExport => (
                tr("stats-export"),
                vec![(tr("stats-csv-filter"), vec!["*.csv"]), all],
            ),
            Dialog::Book => (
                tr("books-add"),
                vec![(tr("books-filter"), vec!["*.epub"]), all],
            ),
            Dialog::BackupSave => (
                tr("backup-make"),
                vec![(tr("backup-filter"), vec!["*.nikomiru-backup"]), all],
            ),
            Dialog::BackupOpen => (
                tr("backup-restore"),
                vec![(tr("backup-filter"), vec!["*.nikomiru-backup"]), all],
            ),
        };
        Self {
            dialog,
            title,
            filters,
            save_as: match dialog {
                Dialog::MpvExport => Some("mpv.conf".to_string()),
                Dialog::StatsExport => Some("nikomiru-sessions.csv".to_string()),
                Dialog::BackupSave => Some("nikomiru.nikomiru-backup".to_string()),
                _ => None,
            },
            folder: match dialog {
                Dialog::MpvImport => s.mpv_conf_folder(),
                _ => None,
            },
            directory: dialog == Dialog::LibraryFolder,
        }
    }
}

/// What the view keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct View {
    /// Bring the window forward (second launch, APP-001).
    pub activate: bool,
    /// The dictionaries page is shown.
    pub dict_page_open: bool,
    /// The Anki settings page is shown.
    pub anki_page_open: bool,
    /// What the Anki settings page keeps.
    pub anki: anki_page::State,
    /// The Online subtitles page is shown (MET-009).
    pub online_page_open: bool,
    /// The Player settings page is shown (PLY-016, PLY-018).
    pub player_page_open: bool,
    /// What it keeps.
    pub player: player_page::State,
    /// What it keeps.
    pub online: online_page::State,
    /// The Subtitle services tab of the settings is shown (MET-011).
    pub services_page_open: bool,
    /// The Word audio tab of the settings is shown (AUD-001).
    pub audio_page_open: bool,
    /// What it keeps.
    pub audio: audio_page::State,
    /// What it keeps.
    pub services: services_page::State,
    pub servers_page_open: bool,
    pub servers: servers_page::State,
    pub backup_page_open: bool,
    pub hooker_page_open: bool,
    pub books_page_open: bool,
    /// Esc, or a file opened: the Texthooker page closes at the next frame.
    hooker_closing: bool,
    /// Esc, or a file opened: the book, then the Books page, close.
    books_closing: bool,
    pub hooker: hooker_page::State,
    /// The part of the window to read after this frame (PLY-015), in pixels.
    pub capture: Option<(egui::Pos2, egui::Vec2)>,
    pub backup: backup_page::State,
    /// The tab of the settings shown last: ⚙ opens it again.
    pub settings_tab: settings::SettingsTab,
    /// Ctrl+, asked for the settings: they open at the next frame.
    settings_requested: bool,
    /// The General, Subtitles, Lookup, Shortcuts, Advanced or About tab is
    /// shown.
    pub prefs_open: Option<settings::SettingsTab>,
    /// What it keeps.
    pub prefs: prefs_page::State,
    /// How the interface looks, as of the last frame (SET-003, SET-005).
    look: Option<crate::session::Look>,
    /// egui's zoom, as of the last frame (SET-005).
    pub zoom: f32,
    /// The Library page is shown (LIB-005).
    pub library_page_open: bool,
    /// The files opened as of the last frame.
    opened: u64,
    /// Esc over the bare video: the Library page opens at the next frame.
    library_asked: bool,
    /// What it keeps.
    pub library: library_page::State,
    /// The subtitle list is shown (SUB-015).
    pub sub_list_open: bool,
    /// Give the keyboard to the list's search field (Ctrl+F).
    pub sub_search_focus: bool,
    /// The window is fullscreen, as of the last frame.
    pub fullscreen: bool,
    fullscreen_request: Option<bool>,
    /// A file dialog the app must open.
    pub dialog: Option<Dialog>,
    toast_pending: Option<String>,
    toast: Option<(String, f64)>,
    /// The last toast shown, for the test harness: never cleared.
    pub last_toast: String,
    title: String,
    /// Levels of the popups drawn with their content in the last frame and not
    /// reported yet.
    presented: Vec<usize>,
    /// Popup generations already reported as drawn.
    reported: HashSet<u64>,
    pub bar: bar::State,
    pub overlay: overlay::State,
    pub list: list::State,
    pub popups: popups::State,
    pub dict: dict_page::State,
}

impl View {
    /// What the session asks of the window (from a key or a button).
    pub fn effect(&mut self, effect: UiEffect) {
        match effect {
            UiEffect::None => {}
            UiEffect::ToggleFullscreen => self.fullscreen_request = Some(!self.fullscreen),
            UiEffect::Back => {
                // popups were closed by the session already (POP-012)
                if self.online_page_open {
                    self.close_online_page();
                } else if self.hooker_page_open {
                    self.hooker_closing = true;
                } else if self.books_page_open {
                    self.books_closing = true;
                } else if self.library_page_open {
                    self.close_library_page();
                } else if self.services_page_open
                    || self.servers_page_open
                    || self.backup_page_open
                    || self.audio_page_open
                    || self.dict_page_open
                    || self.prefs_open.is_some()
                {
                    self.close_settings();
                } else if self.player_page_open {
                    self.close_player_page();
                } else if self.anki_page_open {
                    self.close_anki_page();
                } else if self.fullscreen {
                    self.fullscreen_request = Some(false);
                } else {
                    // nothing over the video: back to the library (LIB-005)
                    self.library_asked = true;
                }
            }
            UiEffect::OpenDialog => self.dialog = Some(Dialog::Video),
            UiEffect::ToggleSubList => self.sub_list_open = !self.sub_list_open,
            UiEffect::OpenSettings => {
                self.settings_requested = true;
            }
            UiEffect::SubSearch => {
                self.sub_list_open = true;
                self.sub_search_focus = true;
            }
        }
    }

    /// Shows a toast (the last of a batch), even with the same text as before.
    pub fn toast(&mut self, text: String) {
        self.last_toast.clone_from(&text);
        self.toast_pending = Some(text);
    }

    pub fn open_dict_page(&mut self, out: &mut Vec<Command>) {
        if !self.dict_page_open {
            self.dict_page_open = true;
            out.push(Command::DictOpen);
        }
    }

    pub fn open_anki_page(&mut self, out: &mut Vec<Command>) {
        if !self.anki_page_open {
            self.anki_page_open = true;
            out.push(Command::AnkiOpen);
        }
    }

    pub fn close_anki_page(&mut self) {
        self.anki_page_open = false;
        self.anki = anki_page::State::default();
    }

    pub fn open_online_page(&mut self, out: &mut Vec<Command>) {
        if !self.online_page_open {
            self.online_page_open = true;
            out.push(Command::OnlineOpen);
        }
    }

    pub fn close_online_page(&mut self) {
        self.online_page_open = false;
        self.online = online_page::State::default();
    }

    pub fn open_player_page(&mut self, out: &mut Vec<Command>) {
        if !self.player_page_open {
            self.player_page_open = true;
            out.push(Command::PlayerOpen);
        }
    }

    pub fn close_player_page(&mut self) {
        self.player_page_open = false;
        self.player = player_page::State::default();
    }

    /// Opens the settings at `tab` (SET-008), in place of any other page
    /// over the video.
    pub fn open_settings(&mut self, tab: settings::SettingsTab, out: &mut Vec<Command>) {
        use settings::SettingsTab;
        self.close_settings();
        self.close_online_page();
        self.settings_tab = tab;
        match tab {
            SettingsTab::Player => self.open_player_page(out),
            SettingsTab::Services => {
                self.services_page_open = true;
                out.push(Command::ServicesOpen);
            }
            SettingsTab::Servers => {
                self.servers_page_open = true;
                out.push(Command::ServersOpen);
            }
            SettingsTab::Backup => {
                self.backup_page_open = true;
                out.push(Command::BackupOpen);
            }
            SettingsTab::Dictionaries => self.open_dict_page(out),
            SettingsTab::Audio => {
                self.audio_page_open = true;
                out.push(Command::AudioOpen);
            }
            SettingsTab::Anki => self.open_anki_page(out),
            SettingsTab::General
            | SettingsTab::Subtitles
            | SettingsTab::Lookup
            | SettingsTab::Shortcuts
            | SettingsTab::Advanced
            | SettingsTab::About => {
                self.prefs_open = Some(tab);
                out.push(Command::SettingsOpen);
            }
        }
    }

    /// Whether a tab of the settings is shown.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub fn settings_open(&self) -> bool {
        self.player_page_open
            || self.anki_page_open
            || self.dict_page_open
            || self.services_page_open
            || self.servers_page_open
            || self.backup_page_open
            || self.audio_page_open
            || self.prefs_open.is_some()
    }

    /// Closes the settings, whichever tab is shown.
    pub fn close_settings(&mut self) {
        self.close_player_page();
        self.close_anki_page();
        self.close_dict_page();
        self.prefs_open = None;
        self.prefs = prefs_page::State::default();
        self.services_page_open = false;
        self.services = services_page::State::default();
        self.servers_page_open = false;
        self.servers = servers_page::State::default();
        self.backup_page_open = false;
        self.backup = backup_page::State::default();
        self.audio_page_open = false;
        self.audio = audio_page::State::default();
    }

    /// What a page asked for after its frame.
    pub(crate) fn leave(&mut self, leave: settings::Leave, out: &mut Vec<Command>) {
        match leave {
            settings::Leave::Stay => {}
            settings::Leave::Close => self.close_settings(),
            settings::Leave::To(tab) => self.open_settings(tab, out),
        }
    }

    pub fn open_library_page(&mut self, out: &mut Vec<Command>) {
        if !self.library_page_open {
            self.library_page_open = true;
            out.push(Command::LibraryOpen);
        }
    }

    /// The Books page opens (BOK-004), in place of the Library page.
    pub fn open_books_page(&mut self, out: &mut Vec<Command>) {
        self.close_library_page();
        self.books_page_open = true;
        out.push(Command::BooksOpen);
    }

    pub fn close_books_page(&mut self, out: &mut Vec<Command>) {
        self.books_page_open = false;
        out.push(Command::BookClose);
    }

    /// The Texthooker page opens (HOK-001), in place of the Library page.
    pub fn open_hooker_page(&mut self, out: &mut Vec<Command>) {
        self.close_library_page();
        self.hooker_page_open = true;
        out.push(Command::HookerOpen);
    }

    pub fn close_hooker_page(&mut self, out: &mut Vec<Command>) {
        self.hooker_page_open = false;
        self.hooker = hooker_page::State::default();
        out.push(Command::HookerClose);
    }

    pub fn close_library_page(&mut self) {
        self.library_page_open = false;
        self.library.reset();
    }

    pub fn close_dict_page(&mut self) {
        self.dict_page_open = false;
        self.dict = dict_page::State::default();
    }

    /// The popups drawn since the last call, by level, once per popup.
    pub fn take_presented_popups(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.presented)
    }

    /// Draws the window.
    pub fn frame(&mut self, ui: &mut egui::Ui, s: &Session) -> Vec<Command> {
        // a file opened, from the page or anywhere else: the video shows
        if s.opened() != self.opened {
            self.opened = s.opened();
            self.close_library_page();
            self.hooker_closing = self.hooker_page_open;
            self.books_closing = self.books_page_open;
        }
        let ctx = ui.ctx().clone();
        let mut out = Vec::new();
        if std::mem::take(&mut self.hooker_closing) {
            self.close_hooker_page(&mut out);
        }
        if std::mem::take(&mut self.books_closing) {
            if s.books_page.open.is_some() {
                out.push(Command::BookClose);
            } else {
                self.close_books_page(&mut out);
            }
        }
        if std::mem::take(&mut self.library_asked) {
            self.open_library_page(&mut out);
        }
        if std::mem::take(&mut self.settings_requested) {
            self.open_settings(self.settings_tab, &mut out);
        }
        self.window(&ctx, s);
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            out.push(Command::MouseMoved);
        }
        let screen = ctx.content_rect();
        let controls = s.controls_visible();

        // the video: a click plays or pauses (or closes the popups), a double
        // click toggles fullscreen (PLY-001)
        let video = ui.interact(screen, Id::unique("video"), Sense::click());
        let name = s.translator().tr("video-area", &[]);
        video.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Video, true, &name));
        if video.double_clicked() {
            self.fullscreen_request = Some(!self.fullscreen);
        } else if video.clicked() {
            out.push(Command::VideoClicked);
        }

        let bar_height = if controls { self.bar.height } else { 0.0 };
        overlay::show(&ctx, s, &mut self.overlay, bar_height, &mut out);
        // PLY-015: a frame of the video and its subtitles, and nothing else
        if s.capture_pending() {
            let area = overlay::video_rect(screen, s.view.video_size);
            let ppp = ctx.pixels_per_point();
            self.capture = Some((area.min * ppp, area.size() * ppp));
            return out;
        }
        if self.sub_list_open {
            self.sub_list_open = list::show(
                &ctx,
                s,
                &mut self.list,
                &mut self.sub_search_focus,
                bar_height,
                &mut out,
            );
        }
        // the bar fades in and out over 150 ms (PLY-004)
        let opacity = ctx.animate_bool_with_time(Id::unique("bar-fade"), controls, 0.15);
        if opacity > 0.0 {
            bar::show(&ctx, s, self, opacity, &mut out);
        }
        // the settings pages cover the popups: they wait under them
        let drawn = if self.dict_page_open
            || self.anki_page_open
            || self.online_page_open
            || self.player_page_open
            || self.services_page_open
            || self.servers_page_open
            || self.backup_page_open
            || self.audio_page_open
            || self.prefs_open.is_some()
            || self.library_page_open
            || s.advanced_card().is_some()
        {
            self.popups.rects.clear();
            Vec::new()
        } else {
            popups::show(&ctx, s, &mut self.popups, &mut out)
        };
        self.mark_presented(s, &drawn);
        if self.library_page_open {
            library_page::show(&ctx, s, self, &mut out);
        }
        if std::mem::take(&mut self.library.open_hooker) {
            self.open_hooker_page(&mut out);
        }
        if std::mem::take(&mut self.library.open_books) {
            self.open_books_page(&mut out);
        }
        if self.dict_page_open {
            dict_page::show(&ctx, s, self, &mut out);
        }
        if self.anki_page_open {
            anki_page::show(&ctx, s, self, &mut out);
        }
        if self.player_page_open {
            player_page::show(&ctx, s, self, &mut out);
        }
        if self.services_page_open {
            services_page::show(&ctx, s, self, &mut out);
        }
        if self.servers_page_open {
            servers_page::show(&ctx, s, self, &mut out);
        }
        if self.backup_page_open {
            backup_page::show(&ctx, s, self, &mut out);
        }
        if self.hooker_page_open {
            hooker_page::show(&ctx, s, self, &mut out);
        }
        if self.books_page_open {
            books_page::show(&ctx, s, self, &mut out);
        }
        if self.audio_page_open {
            audio_page::show(&ctx, s, self, &mut out);
        }
        if self.prefs_open.is_some() {
            prefs_page::show(&ctx, s, self, &mut out);
        }
        if self.online_page_open {
            online_page::show(&ctx, s, self, &mut out);
        }
        if let Some(card) = s.advanced_card() {
            mining_dialog::show(&ctx, s, card, &mut out);
        }
        if let Some(missing) = &s.missing_link {
            link::show(&ctx, s, missing, &mut out);
        }
        chrome::show(&ctx, s, self);
        chrome::next_up(&ctx, s, bar_height, &mut out);

        // the cursor hides with the controls, over the video (PLY-004)
        if !controls && !s.popups.is_open() && video.hovered() {
            ctx.set_cursor_icon(CursorIcon::None);
        }
        out
    }

    /// Fullscreen, title and activation, through egui's viewport commands.
    fn window(&mut self, ctx: &egui::Context, s: &Session) {
        // the theme, the accent and the scale, applied when they change
        let look = s.look();
        if self.look != Some(look) {
            self.look = Some(look);
            let [r, g, b] = look.accent;
            palette::apply(ctx, look.dark, egui::Color32::from_rgb(r, g, b));
            ctx.set_zoom_factor(f32::from(look.scale) / 100.0);
        }
        self.zoom = ctx.zoom_factor();
        self.fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if let Some(on) = self.fullscreen_request.take() {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(on));
        }
        let app = s.translator().tr("app-title", &[]);
        let title = if s.view.loaded && !s.view.title.is_empty() {
            format!("{} — {app}", s.view.title)
        } else {
            app
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if std::mem::take(&mut self.activate) {
            // focus works on X11; Wayland lets the compositor only flag the window
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            ctx.send_viewport_cmd(ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
        if let Some(text) = self.toast_pending.take() {
            self.toast = Some((text, ctx.input(|i| i.time)));
        }
    }

    /// Records the popups drawn with their content, once per popup, for the
    /// harness's `popup-rendered` (stamped after the swap).
    fn mark_presented(&mut self, s: &Session, drawn: &[usize]) {
        let stack = s.popups.stack();
        for &level in drawn {
            if let Some(p) = stack.get(level)
                && p.reply.is_some()
                && self.reported.insert(p.generation)
            {
                self.presented.push(level);
            }
        }
        self.reported
            .retain(|g| stack.iter().any(|p| p.generation == *g));
    }
}
