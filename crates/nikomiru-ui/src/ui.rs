// SPDX-License-Identifier: GPL-3.0-only

//! The interface as the app's event loop sees it: the session and its view,
//! both on the UI thread. What other threads produce (the player's events,
//! subtitle loads, lookups, imports, a second launch, signals, the test
//! harness) comes back as [`Inbound`] messages through the app's [`Waker`],
//! which also wakes the event loop (SPECIFICATION §5.3).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nikomiru_player::PlayerEvent;
use nikomiru_player::control::Stamped;

use crate::popup::{Rect, Reply};
use crate::session::{Services, Session, url_to_path};
use crate::shortcuts::{Action, Modifiers};
use crate::subtitles::{Loaded, Source};
use crate::view::{Dialog, FileDialog, View};

/// Work done on another thread, for the UI thread.
pub enum Inbound {
    /// A player event.
    Player(Stamped<PlayerEvent>),
    /// A subtitle load ended.
    Subtitles {
        /// The session's load generation.
        generation: u64,
        /// What was loaded.
        source: Source,
        /// The cues, or a message.
        result: Result<Loaded, String>,
    },
    /// A lookup ended.
    Lookup {
        /// The popup's generation.
        generation: u64,
        /// The reply, or a message.
        result: Result<Reply, String>,
    },
    /// Progress of a dictionary import.
    ImportProgress {
        /// The import's generation.
        generation: u64,
        /// How far it is.
        progress: nikomiru_dict::import::Progress,
    },
    /// A dictionary import ended.
    ImportDone {
        /// The import's generation.
        generation: u64,
        /// The dictionary's title, or a message.
        result: Result<String, String>,
    },
    /// Opens a file or `file://` URL (command line, second launch: APP-001).
    Open(String),
    /// Brings the window forward (second launch).
    Activate,
    /// The file the user chose in a file dialog.
    Chosen {
        /// The dialog that was open.
        dialog: Dialog,
        /// The file.
        path: PathBuf,
    },
    /// Saves the position and quits (a termination signal).
    Quit,
    /// A call of the GUI test harness, run on the UI thread.
    #[cfg(feature = "test-harness")]
    Harness(Box<dyn FnOnce(&mut Ui) + Send>),
}

impl std::fmt::Debug for Inbound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Inbound::Player(e) => return write!(f, "Player({:?})", e.value),
            Inbound::Subtitles { .. } => "Subtitles",
            Inbound::Lookup { .. } => "Lookup",
            Inbound::ImportProgress { .. } => "ImportProgress",
            Inbound::ImportDone { .. } => "ImportDone",
            Inbound::Open(_) => "Open",
            Inbound::Activate => "Activate",
            Inbound::Chosen { .. } => "Chosen",
            Inbound::Quit => "Quit",
            #[cfg(feature = "test-harness")]
            Inbound::Harness(_) => "Harness",
        };
        f.write_str(name)
    }
}

/// Hands [`Inbound`] messages to the UI thread and wakes it: the app's event
/// loop proxy. Messages sent after the loop ended are dropped.
pub trait Waker: Send + Sync + 'static {
    /// Queues `message` for the UI thread.
    fn wake(&self, message: Inbound);
}

/// A key of the window.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// The physical key, as a Linux evdev code (KEY-002).
    pub evdev: u32,
    /// The modifiers held.
    pub mods: Modifiers,
    /// Pressed or released.
    pub pressed: bool,
    /// An automatic repeat of a held key.
    pub repeat: bool,
}

/// What a click or a drag in the view asks for. The view only reads the
/// session while it draws; the commands run after the frame.
// the overlay, list, popup and dictionaries views send the rest (ROADMAP M3,
// work track 5)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum Command {
    Action(Action),
    SeekTo(f64),
    SetVolume(f64),
    SelectAudio(i64),
    MouseMoved,
    /// A click on the video, outside every other element.
    VideoClicked,
    SelectSubtitles(String),
    ForceEncoding(String),
    GraphemeClicked {
        cue: usize,
        offset: usize,
        anchor: Rect,
    },
    GraphemeHovered {
        cue: usize,
        offset: usize,
        x: f64,
        y: f64,
        shift: bool,
        anchor: Rect,
    },
    SubtitleHover(bool),
    ListText {
        cue: usize,
        pos: usize,
        anchor: Rect,
    },
    ListTime(usize),
    PopupLookup {
        level: usize,
        text: String,
        anchor: Rect,
    },
    PopupFocus(usize),
    PopupCloseAll,
    PopupHover(bool),
    PopupReplay,
    PopupGoto,
    PopupEntry(i64),
    DictOpen,
    DictChoose(PathBuf),
    DictConfirm(bool),
    DictCancel,
    DictMove(String, i32),
    DictSetEnabled(String, bool),
    DictDelete(String),
    DictRename(String, String),
    DictSetCollapse(String, String),
}

/// The interface: the session and its view.
pub struct Ui {
    pub(crate) session: Session,
    pub(crate) view: View,
    waker: Arc<dyn Waker>,
    quit: bool,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ui")
            .field("session", &self.session)
            .finish()
    }
}

impl Ui {
    /// Builds the session from `services` and gives `ctx` the system fonts and
    /// the style.
    pub fn new(ctx: &egui::Context, services: Services, waker: Arc<dyn Waker>) -> Self {
        crate::view::theme::install(ctx);
        let mut ui = Self {
            session: Session::new(services),
            view: View::default(),
            waker,
            quit: false,
        };
        #[cfg(feature = "test-harness")]
        crate::harness::install(ui.waker.clone());
        ui.pump();
        ui
    }

    /// The video can be drawn: a file waiting for it opens now
    /// (SPECIFICATION §5.2.2 #3).
    pub fn renderer_ready(&mut self) {
        self.session.renderer_ready();
        self.pump();
    }

    /// The render context could not be created: no file can play.
    pub fn renderer_failed(&mut self, reason: &str) {
        self.session.view.error = Some(
            self.session
                .translator()
                .tr("error-video-output", &[("reason", reason.to_string())]),
        );
    }

    /// A message from another thread. Returns whether the window needs a redraw.
    pub fn handle(&mut self, message: Inbound) -> bool {
        let redraw = match message {
            Inbound::Player(event) => self.session.on_event(event),
            Inbound::Subtitles {
                generation,
                source,
                result,
            } => {
                self.session.subtitles_loaded(generation, source, result);
                true
            }
            Inbound::Lookup { generation, result } => {
                self.session.lookup_done(generation, result);
                true
            }
            Inbound::ImportProgress {
                generation,
                progress,
            } => {
                self.session.dict_progress(generation, progress);
                true
            }
            Inbound::ImportDone { generation, result } => {
                self.session.dict_finished(generation, result);
                true
            }
            Inbound::Open(target) => {
                self.session.open(&url_to_path(&target));
                true
            }
            Inbound::Activate => {
                self.view.activate = true;
                true
            }
            Inbound::Chosen { dialog, path } => {
                match dialog {
                    Dialog::Video => self.session.open(&path),
                    Dialog::Subtitles => self.session.open_subtitle_file(&path),
                    Dialog::Dictionary => self.session.dict_choose(path),
                }
                true
            }
            Inbound::Quit => {
                self.quit();
                true
            }
            #[cfg(feature = "test-harness")]
            Inbound::Harness(call) => {
                call(self);
                true
            }
        };
        self.pump();
        redraw
    }

    /// A key of the window (KEY-002). `text_focus`: a text field has the
    /// keyboard (KEY-004). Returns whether a shortcut took the key: then egui
    /// must not see it (it would move the focus with Tab or arrows, or press the
    /// focused button with Space).
    pub fn key(&mut self, key: Key, text_focus: bool) -> bool {
        let (effect, taken) =
            self.session
                .key(key.evdev, key.mods, key.pressed, key.repeat, text_focus);
        if key.pressed {
            self.view.effect(effect);
        }
        self.pump();
        taken
    }

    /// The window lost the keyboard: the release of a held key will not come.
    pub fn focus_lost(&mut self) {
        self.session.release_hold();
        self.pump();
    }

    /// A file dropped on the window (LIB-001, DIC-001); only the first file of
    /// a drop counts. A zip is a dictionary; a subtitle file goes with the open
    /// video; anything else is a video.
    pub fn dropped(&mut self, path: &Path) {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("zip") => {
                // the page reads the list when it opens: `choose` needs the rows
                if !self.view.dict_page_open {
                    self.view.dict_page_open = true;
                    self.session.dict_refresh();
                }
                self.session.dict_choose(path.to_path_buf());
            }
            Some("srt" | "ass" | "ssa" | "vtt") if self.session.view.loaded => {
                self.session.open_subtitle_file(path);
            }
            _ => self.session.open(path),
        }
        self.pump();
    }

    /// Time-based work: the control bar, hover popups, the screensaver.
    /// Returns whether the window needs a redraw.
    pub fn tick(&mut self) -> bool {
        let before = (
            self.session.controls_visible(),
            self.session.popups.stack().len(),
        );
        self.session.tick();
        self.pump();
        before
            != (
                self.session.controls_visible(),
                self.session.popups.stack().len(),
            )
    }

    /// How soon [`Ui::tick`] must run again.
    pub fn tick_interval(&self) -> Duration {
        Duration::from_millis(250)
    }

    /// Draws a frame into `ui` (the whole window).
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let commands = self.view.frame(ui, &self.session);
        for command in commands {
            self.apply(command);
        }
        self.pump();
    }

    /// The frame just drawn is on screen (after the swap). The test harness's
    /// `popup-rendered` milestone is stamped here (T-PERF-06).
    pub fn presented(&mut self) {
        for level in self.view.take_presented_popups() {
            self.session.popup_rendered(level);
        }
        self.pump();
    }

    /// A file dialog to open, if the user asked for one: the app opens it (XDG
    /// portal) and sends the choice back as [`Inbound::Chosen`].
    pub fn take_file_dialog(&mut self) -> Option<FileDialog> {
        self.view
            .dialog
            .take()
            .map(|d| FileDialog::new(d, &self.session))
    }

    /// Saves the position; the app then leaves its event loop.
    pub fn quit(&mut self) {
        self.session.flush_position();
        self.quit = true;
        #[cfg(feature = "test-harness")]
        crate::harness::uninstall();
    }

    /// The app must leave its event loop.
    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    /// Runs one command, as a click of the view would.
    #[cfg_attr(not(feature = "test-harness"), allow(dead_code))]
    pub(crate) fn run(&mut self, command: Command) {
        self.apply(command);
        self.pump();
    }

    pub(crate) fn apply(&mut self, command: Command) {
        let s = &mut self.session;
        match command {
            Command::Action(action) => {
                let effect = s.action(action);
                self.view.effect(effect);
            }
            Command::SeekTo(seconds) => s.seek_to(seconds),
            Command::SetVolume(volume) => s.set_volume(volume),
            Command::SelectAudio(id) => s.select_audio(id),
            Command::MouseMoved => s.mouse_moved(),
            Command::VideoClicked => {
                if s.popups.is_open() {
                    s.close_all_popups();
                } else if s.view.loaded {
                    let effect = s.action(Action::PlayPause);
                    self.view.effect(effect);
                }
            }
            Command::SelectSubtitles(key) => s.select_subtitles(&key),
            Command::ForceEncoding(encoding) => s.force_encoding(&encoding),
            Command::GraphemeClicked {
                cue,
                offset,
                anchor,
            } => s.select_grapheme(cue, offset, anchor),
            Command::GraphemeHovered {
                cue,
                offset,
                x,
                y,
                shift,
                anchor,
            } => s.grapheme_hovered(cue, offset, x, y, shift, anchor),
            Command::SubtitleHover(inside) => s.subtitle_hover(inside),
            Command::ListText { cue, pos, anchor } => s.select_in_list(cue, pos, anchor),
            Command::ListTime(cue) => s.seek_to_cue(cue),
            Command::PopupLookup {
                level,
                text,
                anchor,
            } => s.popup_lookup(level, &text, anchor),
            Command::PopupFocus(level) => s.popup_focus(level),
            Command::PopupCloseAll => s.close_all_popups(),
            Command::PopupHover(inside) => s.popup_hover(inside),
            Command::PopupReplay => s.popup_replay(),
            Command::PopupGoto => s.popup_goto(),
            Command::PopupEntry(delta) => s.popup_entry(delta),
            Command::DictOpen => s.dict_refresh(),
            Command::DictChoose(zip) => s.dict_choose(zip),
            Command::DictConfirm(update) => s.dict_confirm(update),
            Command::DictCancel => s.dict_cancel(),
            Command::DictMove(id, delta) => s.dict_move(&id, delta),
            Command::DictSetEnabled(id, on) => s.dict_set_enabled(&id, on),
            Command::DictDelete(id) => s.dict_delete(&id),
            Command::DictRename(id, name) => s.dict_rename(&id, &name),
            Command::DictSetCollapse(id, state) => s.dict_set_collapse(&id, &state),
        }
    }

    /// Starts the jobs the session queued, with replies that come back as
    /// [`Inbound`] messages, and hands on its toasts and milestones.
    fn pump(&mut self) {
        for (generation, request) in self.session.take_subtitle_loads() {
            let waker = self.waker.clone();
            let source = request.source.clone();
            self.session.start_subtitle_load(
                request,
                Box::new(move |result| {
                    waker.wake(Inbound::Subtitles {
                        generation,
                        source,
                        result,
                    });
                }),
            );
        }
        for (generation, text) in self.session.take_lookups() {
            let waker = self.waker.clone();
            self.session.start_lookup(
                text,
                Box::new(move |result| waker.wake(Inbound::Lookup { generation, result })),
            );
        }
        for import in self.session.take_dict_imports() {
            let generation = import.generation;
            let (progress, done) = (self.waker.clone(), self.waker.clone());
            self.session.start_dict_import(
                import,
                Box::new(move |p| {
                    progress.wake(Inbound::ImportProgress {
                        generation,
                        progress: p,
                    });
                }),
                Box::new(move |result| done.wake(Inbound::ImportDone { generation, result })),
            );
        }
        if let Some(last) = self.session.toasts.drain(..).next_back() {
            self.view.toast(last);
        }
        let events = std::mem::take(&mut self.session.events);
        #[cfg(feature = "test-harness")]
        for event in events {
            crate::harness::push(event);
        }
        #[cfg(not(feature = "test-harness"))]
        drop(events);
    }
}
