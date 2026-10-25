// SPDX-License-Identifier: GPL-3.0-only

//! The view: draws the session every frame, bottom to top, and turns clicks
//! into [`Command`]s. It only reads the session; the commands run after the
//! frame, so nothing changes under the drawing.
//!
//! Layers, bottom to top: the video (drawn by mpv before egui paints), the
//! subtitles, the subtitle list, the control bar, the dictionary popups, the
//! dictionaries page, the toast.

mod anki_page;
mod bar;
mod chrome;
mod dict_page;
mod link;
mod list;
mod online_page;
mod overlay;
mod player_page;
mod popups;
pub(crate) mod theme;

use std::collections::HashSet;

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
                vec![(tr("dict-import-filter"), vec!["*.zip"])],
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
        };
        Self {
            dialog,
            title,
            filters,
            save_as: (dialog == Dialog::MpvExport).then(|| "mpv.conf".to_string()),
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
                } else if self.player_page_open {
                    self.close_player_page();
                } else if self.anki_page_open {
                    self.close_anki_page();
                } else if self.dict_page_open {
                    self.close_dict_page();
                } else if self.fullscreen {
                    self.fullscreen_request = Some(false);
                }
            }
            UiEffect::OpenDialog => self.dialog = Some(Dialog::Video),
            UiEffect::ToggleSubList => self.sub_list_open = !self.sub_list_open,
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
        let ctx = ui.ctx().clone();
        let mut out = Vec::new();
        self.window(&ctx, s);
        if ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            out.push(Command::MouseMoved);
        }
        let screen = ctx.content_rect();
        let controls = s.controls_visible();

        // the video: a click plays or pauses (or closes the popups), a double
        // click toggles fullscreen (PLY-001)
        let video = ui.interact(screen, Id::unique("video"), Sense::click());
        if video.double_clicked() {
            self.fullscreen_request = Some(!self.fullscreen);
        } else if video.clicked() {
            out.push(Command::VideoClicked);
        }

        let bar_height = if controls { self.bar.height } else { 0.0 };
        overlay::show(&ctx, s, &mut self.overlay, bar_height, &mut out);
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
        {
            self.popups.rects.clear();
            Vec::new()
        } else {
            popups::show(&ctx, s, &mut self.popups, &mut out)
        };
        self.mark_presented(s, &drawn);
        if self.dict_page_open {
            dict_page::show(&ctx, s, self, &mut out);
        }
        if self.anki_page_open {
            anki_page::show(&ctx, s, self, &mut out);
        }
        if self.player_page_open {
            player_page::show(&ctx, s, self, &mut out);
        }
        if self.online_page_open {
            online_page::show(&ctx, s, self, &mut out);
        }
        if let Some(missing) = &s.missing_link {
            link::show(&ctx, s, missing, &mut out);
        }
        chrome::show(&ctx, s, self);

        // the cursor hides with the controls, over the video (PLY-004)
        if !controls && !s.popups.is_open() && video.hovered() {
            ctx.set_cursor_icon(CursorIcon::None);
        }
        out
    }

    /// Fullscreen, title and activation, through egui's viewport commands.
    fn window(&mut self, ctx: &egui::Context, s: &Session) {
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
