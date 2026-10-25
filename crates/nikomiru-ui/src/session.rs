// SPDX-License-Identifier: GPL-3.0-only

//! The playback session behind the player page, apart from its drawing: it opens files,
//! filters and reduces player events into a [`ViewState`], saves the position
//! (LIB-006) and turns shortcut actions into player commands.

use std::path::{Path, PathBuf};

use nikomiru_core::{Clock, Config};
use nikomiru_library::progress::ProgressSaver;
use nikomiru_library::{Library, VideoRecord, Writer};
use nikomiru_player::control::{self, HoldToSpeed, LoadToken, Stamped};
use nikomiru_player::player::{EndFile, Track};
use nikomiru_player::{LoadRequest, PlayerEvent, PlayerHandle, options};

use crate::controls::ControlsVisibility;
pub use crate::dictionaries::DictionaryService;
use crate::dictionaries::{DictionaryPage, Effect};
use crate::hover::Hover;
use crate::i18n::Translator;
use crate::popup::{Command, Origin, Popups, Rect, Reply};
use crate::screensaver::ScreensaverPolicy;
use crate::shortcuts::{Action, Bindings, Context, Modifiers};
use crate::subtitles::{Loaded, Obscure, Request, Source, Subtitles};
use nikomiru_subs::select::{self, Choice, EmbeddedTrack};
use nikomiru_subs::timeline::Jump;

/// What the view must do after an action: the window's business, not the
/// session's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEffect {
    /// Nothing for the UI.
    None,
    /// Toggle fullscreen.
    ToggleFullscreen,
    /// Back: leave fullscreen (later: close popups, go to the library).
    Back,
    /// Show the file dialog.
    OpenDialog,
    /// Show or hide the subtitle list (SUB-015).
    ToggleSubList,
    /// Open the subtitle list with the search field focused.
    SubSearch,
}

/// Milestones of a load, for the GUI test harness (TEST §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// The file is open.
    FileLoaded,
    /// The first position after the load: the video is moving.
    FirstFrame,
    /// The file could not be played: the message id of the error.
    Error(&'static str),
    /// A cue became visible (T-PERF-07): its index and the position.
    CueShown {
        /// Index of the cue.
        index: usize,
        /// Position in milliseconds when it appeared.
        time_pos_ms: i64,
    },
    /// A dictionary import finished well.
    DictionaryImported,
    /// A screenshot was written (PLY-011).
    ScreenshotSaved(PathBuf),
    /// A card was made or overwritten in Anki: its note id.
    CardCreated(u64),
    /// A popup was drawn with its content (T-PERF-06).
    PopupRendered {
        /// Its level (0 = from the subtitles).
        level: usize,
        /// The query.
        query: String,
        /// Headwords shown, in order.
        headwords: Vec<String>,
    },
}

/// State shown by the player page.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// A file is loaded.
    pub loaded: bool,
    /// File name, for the title.
    pub title: String,
    /// Position in seconds.
    pub time_pos: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Paused.
    pub paused: bool,
    /// Volume in percent.
    pub volume: f64,
    /// Muted.
    pub muted: bool,
    /// Speed.
    pub speed: f64,
    /// Audio tracks.
    pub audio_tracks: Vec<Track>,
    /// Selected audio track.
    pub aid: Option<i64>,
    /// Number of chapters.
    pub chapters: usize,
    /// Error to show, already translated (PLY-012, ERR-001).
    pub error: Option<String>,
    /// Display size of the video (anamorphic pixels applied), 0 if unknown.
    pub video_size: (i64, i64),
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            loaded: false,
            title: String::new(),
            time_pos: 0.0,
            duration: 0.0,
            paused: true,
            volume: 100.0,
            muted: false,
            speed: 1.0,
            audio_tracks: Vec::new(),
            aid: None,
            chapters: 0,
            error: None,
            video_size: (0, 0),
        }
    }
}

/// Keeps the screen on (PLY-013); the application talks to the desktop.
pub trait IdleInhibitor: Send {
    /// Inhibits the screensaver, or releases the inhibition.
    fn set(&self, inhibit: bool);
}

/// Loads subtitles away from the UI thread: decoding (SUB-005) and the
/// extraction of embedded tracks (SUB-007). `done` may run on any thread.
pub trait SubtitleSource: Send {
    /// Loads `request` and calls `done` with the result or a translated error.
    fn load(&self, request: Request, done: Box<dyn FnOnce(Result<Loaded, String>) + Send>);

    /// Aligns cues with the audio (SUB-018); a new load cancels it.
    fn align(
        &self,
        request: crate::subtitles::AlignRequest,
        done: Box<dyn FnOnce(Result<nikomiru_subs::align::AlignResult, String>) + Send>,
    );
}

/// Everything the session needs from the application.
pub struct Services {
    /// The player.
    pub player: PlayerHandle,
    /// The configuration.
    pub config: Config,
    /// The library, if it could be opened.
    pub library: Option<std::sync::Arc<Library>>,
    /// The interface strings.
    pub translator: Translator,
    /// Time source.
    pub clock: Box<dyn Clock>,
    /// Startup problems, already translated (invalid settings, read-only
    /// library…): shown until the first file opens (ERR-001).
    pub notices: Vec<String>,
    /// The screensaver inhibition.
    pub inhibitor: Box<dyn IdleInhibitor>,
    /// The subtitle loader.
    pub subtitle_source: Box<dyn SubtitleSource>,
    /// The dictionaries.
    pub dictionaries: Box<dyn DictionaryService>,
    /// Where screenshots go (PLY-011).
    pub screenshots_dir: PathBuf,
    /// Mining to Anki (ADR 0007).
    pub anki: Box<dyn crate::mining::AnkiService>,
    /// Online subtitles (ADR 0008).
    pub online: Box<dyn crate::online::OnlineService>,
    /// Where downloaded subtitles are kept (MET-008).
    pub subtitles_dir: PathBuf,
    /// Where the mpv presets are kept (PLY-016).
    pub mpv_presets_dir: PathBuf,
    /// `config.toml`, where the settings pages save.
    pub config_file: PathBuf,
    /// Where the media of a card are made (`$XDG_CACHE_HOME/nikomiru/mining`):
    /// the frame of the click is saved there.
    pub mining_dir: PathBuf,
    /// The settings came from `config.toml` (or there is none yet): saving may
    /// write it. When it could not be read, the settings in memory are the
    /// defaults, and writing them would lose the user's file.
    pub config_writable: bool,
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services")
            .field("player", &self.player)
            .finish()
    }
}

struct Current {
    uid: Option<String>,
    path: PathBuf,
    /// The OpenSubtitles hash: the folder of its downloaded subtitles
    /// (MET-008).
    hash: Option<nikomiru_online::VideoHash>,
}

/// The settings page whose settings are being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SavingPage {
    #[default]
    Anki,
    Online,
    Player,
}

/// A link to a scene whose video cannot be found (ANK-052).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingLink {
    /// The link.
    pub link: nikomiru_anki::link::SourceLink,
    /// Where the library had the video, if it knows it.
    pub path: Option<String>,
}

/// The clip of a link being opened (ANK-052), and the subtitle load whose
/// cues it goes to: the automatic choice for its video, once queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LinkClip {
    start_ms: i64,
    end_ms: i64,
    load: Option<u64>,
}

/// The playback session.
pub struct Session {
    services: Services,
    bindings: Bindings,
    token: LoadToken,
    saver: ProgressSaver,
    hold: HoldToSpeed,
    controls: ControlsVisibility,
    screensaver: ScreensaverPolicy,
    inhibited: bool,
    renderer_ready: bool,
    pending: Option<PathBuf>,
    current: Option<Current>,
    /// What the page shows.
    pub view: ViewState,
    /// Toasts waiting to be shown (translated).
    pub toasts: Vec<String>,
    /// Load milestones not yet taken by the UI layer.
    pub events: Vec<UiEvent>,
    first_frame: bool,
    /// The subtitles of the current file.
    pub subs: Subtitles,
    /// The subtitle sources of the current file, for the menu.
    sub_sources: Vec<Source>,
    /// Languages of embedded tracks, for the menu labels.
    sub_tracks: Vec<Track>,
    sub_generation: u64,
    sub_loads: Vec<(u64, Request)>,
    sub_auto_done: bool,
    /// Changes whenever the cue list changes, so the UI rebuilds it.
    pub sub_list_version: u64,
    /// The dictionary popups.
    pub popups: Popups,
    lookups: Vec<(u64, String)>,
    screenshots: Vec<String>,
    anki_jobs: Vec<crate::mining::AnkiJob>,
    /// The last card made since Nikomiru started (ANK-030).
    last_card: Option<crate::mining::LastCard>,
    /// A link whose video cannot be found: the view offers to locate it
    /// (ANK-052).
    pub missing_link: Option<MissingLink>,
    /// A settings page covers the popups (the view says so): they wait under
    /// it, and their keys with them (POP-014).
    pub popups_covered: bool,
    /// The clip of the link being opened: once its subtitles are there,
    /// playback goes to its first cue (ANK-052).
    link_clip: Option<LinkClip>,
    /// Where the next file opens, instead of where it was left.
    start_at: Option<f64>,
    /// The Anki settings page (ANK-006, ANK-011).
    pub anki_page: crate::mining::AnkiPage,
    /// The settings to write, taken by the UI layer.
    config_save: Option<Config>,
    /// A write of the settings is running: the next one waits for it.
    config_saving: bool,
    /// The settings "Create Lapis deck" was asked with.
    lapis_request: Option<nikomiru_core::config::AnkiConfig>,
    /// The page's message stays when the settings it saved are written.
    keep_status: bool,
    /// The page that asked for the write of the settings.
    saving_page: SavingPage,
    /// Online jobs, started by the UI layer (ADR 0008).
    online_jobs: Vec<crate::online::OnlineJob>,
    /// The Online subtitles page (MET-009).
    pub online_page: crate::online::OnlinePage,
    /// Videos opened so far: an online answer for another one is dropped.
    online_generation: u64,
    /// The subtitle load current when automatic subtitles were asked for:
    /// any later choice of the user wins over them (MET-007).
    auto_online: Option<u64>,
    /// The Player settings page (PLY-016, PLY-018).
    pub player_page: crate::player_settings::PlayerPage,
    /// Alignments with the audio to start: the subtitle load they are for,
    /// whether they were asked automatically, the request (SUB-018).
    align_jobs: Vec<(u64, bool, crate::subtitles::AlignRequest)>,
    /// The subtitle load that downloaded subtitles are aligned after.
    align_after: Option<u64>,
    /// Presets and exports to write, taken by the UI layer.
    file_writes: Vec<crate::player_settings::FileWrite>,
    /// The answers of the options set now, for the UI layer to wait on.
    option_replies: Vec<(String, nikomiru_player::CommandReply)>,
    /// The set to export, until the file is chosen.
    pending_export: Option<nikomiru_player::settings::OptionSet>,
    /// The dictionaries page.
    pub dict_page: DictionaryPage,
    dict_imports: Vec<DictImport>,
    /// Library writes, off the UI thread (LIB-006 saves every second).
    writer: Option<Writer>,
    /// Lookup on hover (POP-003).
    hover: Hover,
    /// The pointer is over a popup.
    popup_hovered: bool,
    /// A cue is being replayed from the popup: mpv stops at its end (SUB-013).
    replaying: bool,
}

/// An import to start, taken by the UI layer.
#[derive(Debug)]
pub struct DictImport {
    /// The zip.
    pub zip: PathBuf,
    /// The dictionary it updates.
    pub replace: Option<String>,
    /// Matches its progress and its end.
    pub generation: u64,
    /// Stops it.
    pub cancel: nikomiru_core::Cancel,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("view", &self.view).finish()
    }
}

/// `file:///path%20x` or a plain path → path.
pub fn url_to_path(url: &str) -> PathBuf {
    let Some(rest) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    // bytes, not `&str` slices: a `%` before a multibyte character must not panic
    let hex = |b: u8| {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high << 4 | low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(out))
}

impl Drop for Session {
    /// Every way out (window closed, quit, signal) saves the position (LIB-006).
    fn drop(&mut self) {
        self.flush_position();
        if self.inhibited {
            self.services.inhibitor.set(false);
        }
    }
}

impl Session {
    /// A new session.
    pub fn new(services: Services) -> Self {
        let bindings = Bindings::with_overrides(&services.config.shortcuts)
            .unwrap_or_else(|_| Bindings::defaults());
        let view = ViewState {
            error: (!services.notices.is_empty()).then(|| services.notices.join("\n")),
            ..ViewState::default()
        };
        let services_obscure = services.config.subtitles.obscure.clone();
        let popup_size = (
            services.config.popup.max_width,
            services.config.popup.max_height,
        );
        let writer = services
            .library
            .as_ref()
            .filter(|l| !l.read_only())
            .map(|l| Writer::start(std::sync::Arc::clone(l)));
        Self {
            services,
            bindings,
            token: LoadToken::default(),
            saver: ProgressSaver::default(),
            hold: HoldToSpeed::default(),
            controls: ControlsVisibility::default(),
            screensaver: ScreensaverPolicy::default(),
            inhibited: false,
            renderer_ready: false,
            pending: None,
            current: None,
            view,
            toasts: Vec::new(),
            events: Vec::new(),
            first_frame: false,
            subs: Subtitles::with_mode(Obscure::from_config(&services_obscure)),
            sub_sources: Vec::new(),
            sub_tracks: Vec::new(),
            sub_generation: 0,
            sub_loads: Vec::new(),
            sub_auto_done: false,
            sub_list_version: 0,
            popups: Popups::new(popup_size),
            lookups: Vec::new(),
            screenshots: Vec::new(),
            anki_jobs: Vec::new(),
            last_card: None,
            missing_link: None,
            popups_covered: false,
            link_clip: None,
            start_at: None,
            anki_page: crate::mining::AnkiPage::default(),
            config_save: None,
            config_saving: false,
            lapis_request: None,
            keep_status: false,
            saving_page: SavingPage::default(),
            online_jobs: Vec::new(),
            online_page: crate::online::OnlinePage::default(),
            online_generation: 0,
            auto_online: None,
            player_page: crate::player_settings::PlayerPage::default(),
            align_jobs: Vec::new(),
            align_after: None,
            file_writes: Vec::new(),
            option_replies: Vec::new(),
            pending_export: None,
            dict_page: DictionaryPage::default(),
            dict_imports: Vec::new(),
            writer,
            hover: Hover::default(),
            popup_hovered: false,
            replaying: false,
        }
    }

    fn tr(&self, id: &str, args: &[(&str, String)]) -> String {
        self.services.translator.tr(id, args)
    }

    /// The player, for the GUI test harness's `mpv_get`.
    #[cfg(feature = "test-harness")]
    pub fn player(&self) -> &nikomiru_player::PlayerHandle {
        &self.services.player
    }

    /// The interface strings.
    pub fn translator(&self) -> &Translator {
        &self.services.translator
    }

    /// The video item has a render context: a pending file can be loaded now.
    pub fn renderer_ready(&mut self) {
        self.renderer_ready = true;
        if let Some(path) = self.pending.take() {
            self.open(&path);
        }
    }

    /// Opens a file (LIB-001). Waits for the renderer if needed (§5.2.2).
    pub fn open(&mut self, path: &Path) {
        if !self.renderer_ready {
            self.pending = Some(path.to_path_buf());
            return;
        }
        self.flush_position();
        self.end_replay();
        if self.start_at.is_none() {
            // an ordinary open: no link's cue to go to
            self.link_clip = None;
        }
        // the reads below must see the positions still queued (the same file
        // reopened at once)
        if let Some(w) = &self.writer {
            w.wait();
        }
        self.view.error = None;
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let meta = std::fs::metadata(&path).ok();
        let library = self.services.library.as_ref().filter(|l| !l.read_only());
        let mut record = VideoRecord::default();
        let uid = library.and_then(|lib| {
            let uid = lib
                .uid_for_path(&path)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()?;
            record = lib
                .video(&uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()
                .flatten()
                .unwrap_or_else(|| VideoRecord {
                    path: path.to_string_lossy().into_owned(),
                    ..VideoRecord::default()
                });
            if let Some(m) = &meta {
                record.size = m.len();
                record.mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64);
            }
            Some(uid)
        });
        if let (Some(uid), Some(w)) = (&uid, &self.writer) {
            let (uid, record) = (uid.clone(), record.clone());
            w.run(move |lib| {
                if let Err(e) = lib.put_video(&uid, &record) {
                    tracing::warn!("library: {e}");
                }
            });
        }
        let (options, report) = options::load_options(&self.services.config.player);
        // Names only: a raw option's value may hold a token (PRV-002).
        for line in report {
            match line {
                options::Line::Apply { .. } => {}
                options::Line::Ignore { line, name, reason } => {
                    tracing::warn!("mpv option {name} (line {line}) ignored: {reason:?}");
                }
                options::Line::Invalid { line, error } => {
                    tracing::warn!("mpv options, line {line}: {error}");
                }
            }
        }
        let request = LoadRequest {
            start: self.start_at.take().or_else(|| {
                (record.last_position_ms > 0).then(|| record.last_position_ms as f64 / 1000.0)
            }),
            options,
            aid: record.aid,
            volume: record.volume,
            speed: record.speed,
        };
        self.saver.reset();
        self.first_frame = false;
        self.reset_subtitles();
        self.view.loaded = false;
        self.view.title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        tracing::info!(
            "opening {} (resume at {:?} s)",
            path.display(),
            request.start
        );
        // the options mpv rejects are logged by the player's command thread
        match self.services.player.load(&path.to_string_lossy(), request) {
            Ok(token) => self.token = token,
            Err(e) => {
                tracing::error!("load failed: {e}");
                self.view.error = Some(self.services.translator.error(&e));
            }
        }
        // the downloaded subtitles of this video, and online answers for it only
        let hash = nikomiru_online::hash::video_hash(&path)
            .inspect_err(|e| tracing::warn!("{}: {e}", path.display()))
            .ok();
        self.online_generation += 1;
        self.auto_online = None;
        self.online_page.busy = false;
        self.current = Some(Current { uid, path, hash });
    }

    /// A player event; returns whether the view changed.
    pub fn on_event(&mut self, event: Stamped<PlayerEvent>) -> bool {
        let Some(event) = event.current(self.token) else {
            return false;
        };
        let now = self.services.clock.monotonic();
        match event {
            PlayerEvent::FileLoaded => {
                tracing::info!("file loaded");
                self.events.push(UiEvent::FileLoaded);
                self.view.loaded = true;
                self.saver.ready();
            }
            PlayerEvent::TimePos(t) => {
                if self.view.loaded && !self.first_frame {
                    self.first_frame = true;
                    self.events.push(UiEvent::FirstFrame);
                }
                self.view.time_pos = t;
                let pos_ms = (t * 1000.0).round() as i64;
                for index in self.subs.update(pos_ms) {
                    self.events.push(UiEvent::CueShown {
                        index,
                        time_pos_ms: pos_ms,
                    });
                }
                if let Some(ms) = self.saver.position((t * 1000.0) as u64, now) {
                    self.save_position(ms);
                }
            }
            PlayerEvent::Duration(d) => self.view.duration = d,
            PlayerEvent::Pause(p) => {
                self.view.paused = p;
                self.controls.set_playing(!p, now);
            }
            PlayerEvent::Speed(s) => self.view.speed = s,
            PlayerEvent::Volume(v) => self.view.volume = v,
            PlayerEvent::Mute(m) => self.view.muted = m,
            PlayerEvent::Tracks(t) => {
                self.sub_tracks = t.iter().filter(|t| t.kind == "sub").cloned().collect();
                self.view.audio_tracks = t.into_iter().filter(|t| t.kind == "audio").collect();
                self.discover_subtitles();
            }
            PlayerEvent::VideoSize(w, h) => self.view.video_size = (w, h),
            PlayerEvent::Aid(a) => self.view.aid = a,
            PlayerEvent::Chapters(c) => self.view.chapters = c.len(),
            PlayerEvent::EndFile(EndFile::Error(e)) => {
                tracing::warn!("playback error: {e}");
                let file = self.view.title.clone();
                self.events
                    .push(UiEvent::Error(nikomiru_core::Localized::message_id(&e)));
                let reason = self.services.translator.error(&e);
                self.view.loaded = false;
                self.view.error =
                    Some(self.tr("error-file", &[("file", file), ("reason", reason)]));
            }
            _ => return false,
        }
        self.update_screensaver();
        true
    }

    /// Periodic update (from the UI's timer): the screensaver policy, and a
    /// hover popup the pointer left (POP-003).
    pub fn tick(&mut self) {
        self.update_screensaver();
        let now = self.services.clock.monotonic();
        if self
            .hover
            .expired(now, self.services.config.lookup.resume_on_leave)
        {
            self.close_all_popups();
        }
    }

    /// The end set by a replay from the popup no longer applies: the user
    /// took control, a popup closed or another file opened.
    fn end_replay(&mut self) {
        if std::mem::take(&mut self.replaying) {
            let _ = self.services.player.stop_at(None);
        }
    }

    /// "Replay sentence" (POP-015, SUB-013): the popup's cue from its start,
    /// pausing at its end.
    pub fn popup_replay(&mut self) {
        let Some(cue) = self.popups.root_cue() else {
            return;
        };
        let t = self.subs.timeline();
        let Some(c) = t.cues().get(cue) else {
            return;
        };
        let (start, end) = (t.start_pos(cue), c.end_ms + t.delay_ms);
        let p = &self.services.player;
        let _ = p.stop_at(Some(end as f64 / 1000.0));
        let _ = p.seek_absolute(start as f64 / 1000.0);
        let _ = p.set_pause(false);
        self.replaying = true;
    }

    /// "Go to sentence" (POP-015): the popups close and playback goes to the
    /// start of the cue.
    pub fn popup_goto(&mut self) {
        let Some(cue) = self
            .popups
            .root_cue()
            .filter(|&c| c < self.subs.timeline().cues().len())
        else {
            return;
        };
        let start = self.subs.timeline().start_pos(cue);
        self.close_all_popups();
        let _ = self.services.player.seek_absolute(start as f64 / 1000.0);
    }

    /// Tab, Shift+Tab, Alt+wheel in the popup (POP-014).
    pub fn popup_entry(&mut self, delta: i64) {
        self.popups.move_entry(delta);
    }

    /// Opens a `nikomiru://source` link (ANK-050…ANK-052): its video, paused at
    /// the start of the clip; a video the library cannot find is offered to
    /// be located.
    pub fn open_link(&mut self, url: &str) {
        let link = match nikomiru_anki::link::SourceLink::parse(url) {
            Ok(link) => link,
            Err(e) => {
                tracing::info!("{e}");
                let message = self.services.translator.error(&e);
                self.toasts.push(message);
                return;
            }
        };
        let record = self
            .services
            .library
            .as_ref()
            .and_then(|lib| lib.video(&link.uid).ok().flatten());
        match record.map(|r| PathBuf::from(r.path)) {
            Some(path) if path.is_file() => self.open_at_link(&path, &link),
            other => {
                self.missing_link = Some(MissingLink {
                    link,
                    path: other.map(|p| p.to_string_lossy().into_owned()),
                });
            }
        }
    }

    fn open_at_link(&mut self, path: &Path, link: &nikomiru_anki::link::SourceLink) {
        self.close_all_popups();
        self.missing_link = None;
        let _ = self.services.player.set_pause(true);
        self.start_at = Some(f64::from(link.start_ms) / 1000.0);
        self.link_clip = Some(LinkClip {
            start_ms: i64::from(link.start_ms),
            end_ms: i64::from(link.end_ms),
            load: None,
        });
        self.open(path);
    }

    /// With the subtitles of the link's video there, playback goes to the
    /// first cue that overlaps the clip, still paused (ANK-052).
    fn go_to_link_cue(&mut self, link: LinkClip) {
        let (start, end) = (link.start_ms, link.end_ms);
        let t = self.subs.timeline();
        let cue = (0..t.cues().len()).find(|&i| {
            let c = &t.cues()[i];
            c.start_ms + t.delay_ms < end && c.end_ms + t.delay_ms > start
        });
        if let Some(i) = cue {
            let at = t.start_pos(i);
            let _ = self.services.player.seek_absolute(at as f64 / 1000.0);
        }
    }

    /// "Locate the file" was chosen with `path` (ANK-052): the library keeps
    /// the new place, and the link opens.
    pub fn located(&mut self, path: &Path) {
        let Some(missing) = self.missing_link.take() else {
            return;
        };
        if let Some(lib) = self.services.library.as_ref().filter(|l| !l.read_only()) {
            let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let mut record = lib
                .video(&missing.link.uid)
                .ok()
                .flatten()
                .unwrap_or_default();
            record.path = path.to_string_lossy().into_owned();
            if let Err(e) = lib.put_video(&missing.link.uid, &record) {
                tracing::warn!("library: {e}");
            }
        }
        self.open_at_link(path, &missing.link);
    }

    /// The offer to locate a video was declined.
    pub fn dismiss_missing_link(&mut self) {
        self.missing_link = None;
    }

    /// The Anki settings as saved.
    pub fn anki_config(&self) -> &nikomiru_core::config::AnkiConfig {
        &self.services.config.anki
    }

    /// The Anki settings page opened: what Anki has is read at once.
    pub fn anki_open(&mut self) {
        self.anki_page.status = None;
        let config = self.services.config.anki.clone();
        self.anki_test(config);
    }

    /// "Test the connection" with the settings being edited (ANK-006).
    pub fn anki_test(&mut self, config: nikomiru_core::config::AnkiConfig) {
        self.anki_page.busy = true;
        self.anki_jobs.push(crate::mining::AnkiJob::Refresh(config));
    }

    /// "Create Lapis deck" with the settings being edited (ANK-011).
    pub fn anki_create_lapis(&mut self, config: nikomiru_core::config::AnkiConfig) {
        self.anki_page.busy = true;
        self.lapis_request = Some(config.clone());
        self.anki_jobs
            .push(crate::mining::AnkiJob::CreateLapis(config));
    }

    /// Saves the Anki settings: they apply at once, to the session and the
    /// worker (SET-002), and go to `config.toml` unless it could not be read,
    /// which writing the defaults over would lose.
    pub fn anki_save(&mut self, config: nikomiru_core::config::AnkiConfig) {
        self.services.config.anki = config.clone();
        self.anki_jobs
            .push(crate::mining::AnkiJob::Configure(config));
        self.keep_status = false;
        self.saving_page = SavingPage::Anki;
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.anki_page.status = Some((text, false));
        }
    }

    /// What Anki has, or why it could not be read; a deck or a note type of
    /// the settings that Anki does not have is reported (ANK-006).
    pub fn anki_lists(&mut self, result: Result<crate::mining::AnkiLists, String>) {
        self.anki_page.busy = false;
        match result {
            Ok(lists) => {
                let missing = if !lists.decks.contains(&lists.deck) {
                    Some(self.tr("anki-deck-missing", &[("name", lists.deck.clone())]))
                } else if !lists.models.contains(&lists.model) {
                    Some(self.tr("anki-note-type-missing", &[("name", lists.model.clone())]))
                } else {
                    None
                };
                let status = match missing {
                    Some(text) => (text, false),
                    None => (
                        self.tr(
                            "anki-connected",
                            &[
                                ("version", lists.version.to_string()),
                                ("decks", lists.decks.len().to_string()),
                                ("models", lists.models.len().to_string()),
                            ],
                        ),
                        true,
                    ),
                };
                if !self.keep_status {
                    self.anki_page.status = Some(status);
                }
                self.anki_page.lists = Some(lists);
            }
            Err(message) => self.anki_page.status = Some((message, false)),
        }
    }

    /// "Create Lapis deck" ended (ANK-011): the settings it was asked with
    /// choose the Lapis note type and deck, with the default mapping, and are
    /// saved.
    pub fn lapis_done(&mut self, result: Result<crate::mining::LapisMade, String>) {
        self.anki_page.busy = false;
        let request = self.lapis_request.take();
        match result {
            Ok(made) => {
                let key = if made.created {
                    "anki-lapis-created"
                } else {
                    "anki-lapis-exists"
                };
                let text = self.tr(key, &[]);
                let mut config = request.unwrap_or_else(|| self.services.config.anki.clone());
                config.deck = made.deck.clone();
                config.note_type = made.note_type.clone();
                let fields: Vec<String> = nikomiru_anki::lapis::FIELDS
                    .iter()
                    .map(|f| (*f).to_string())
                    .collect();
                let mapping = config
                    .field_mapping
                    .entry(made.note_type.clone())
                    .or_default();
                if mapping.is_empty() {
                    *mapping = nikomiru_anki::lapis::with_defaults(
                        &made.note_type,
                        &fields,
                        &std::collections::BTreeMap::new(),
                    );
                }
                self.anki_page.lapis = Some((made.deck, made.note_type));
                self.anki_page.lapis_created += 1;
                self.anki_save(config);
                // a save that cannot be written keeps its own message
                if self.services.config_writable {
                    self.anki_page.status = Some((text, true));
                    self.keep_status = true;
                }
            }
            Err(message) => self.anki_page.status = Some((message, false)),
        }
    }

    /// The settings to write, taken by the UI layer with their file: one
    /// write at a time, the latest settings when it ends.
    pub fn take_config_save(&mut self) -> Option<(Config, PathBuf)> {
        if self.config_saving {
            return None;
        }
        let config = self.config_save.take()?;
        self.config_saving = true;
        Some((config, self.services.config_file.clone()))
    }

    /// The settings were written, or not.
    pub fn config_saved(&mut self, result: Result<(), String>) {
        self.config_saving = false;
        if self.config_save.is_some() {
            // newer settings wait: their write tells how it went
            return;
        }
        let status = match result {
            Ok(()) if std::mem::take(&mut self.keep_status) => return,
            Ok(()) => (self.tr("settings-saved", &[]), true),
            Err(message) => (message, false),
        };
        match self.saving_page {
            SavingPage::Anki => self.anki_page.status = Some(status),
            SavingPage::Online => self.online_page.status = Some(status),
            SavingPage::Player => self.player_page.status = Some(status),
        }
    }

    /// The Anki jobs to start, taken by the UI layer.
    pub fn take_anki_jobs(&mut self) -> Vec<crate::mining::AnkiJob> {
        std::mem::take(&mut self.anki_jobs)
    }

    /// The Anki service.
    pub fn anki(&self) -> &dyn crate::mining::AnkiService {
        self.services.anki.as_ref()
    }

    /// Asks for automatic subtitles for `video` (MET-007), when they are on
    /// and a provider has a key.
    fn ask_online_subtitles(&mut self, video: &Path) {
        let online = &self.services.config.online;
        let keys = !online.jimaku_api_key.trim().is_empty()
            || !online.opensubtitles_api_key.trim().is_empty();
        if !online.auto_subtitles || !keys {
            return;
        }
        self.auto_online = Some(self.sub_generation);
        self.online_jobs.push(crate::online::OnlineJob::Automatic {
            generation: self.online_generation,
            query: crate::online::OnlineQuery::of_video(video.to_path_buf()),
        });
    }

    /// The Online subtitles page opens (MET-009): the search of the video
    /// playing, filled from its file name.
    pub fn online_open(&mut self) {
        let video = self.current.as_ref().map(|c| c.path.clone());
        let other = match (&self.online_page.query, &video) {
            (Some(q), Some(v)) => &q.video != v,
            _ => true,
        };
        if other {
            self.online_page.query = video.map(crate::online::OnlineQuery::of_video);
            self.online_page.results = None;
        }
        self.online_page.status = None;
    }

    /// A search from the page.
    pub fn online_search(&mut self, title: String, season: Option<u32>, episode: Option<u32>) {
        let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
            self.online_page.status = Some((self.tr("online-no-video", &[]), false));
            return;
        };
        let query = crate::online::OnlineQuery {
            video,
            title,
            season,
            episode,
        };
        self.online_page.query = Some(query.clone());
        self.online_page.results = None;
        self.online_page.status = None;
        self.online_page.busy = true;
        self.online_jobs.push(crate::online::OnlineJob::Search {
            generation: self.online_generation,
            query,
        });
    }

    /// "Use" on the result `index` of the page.
    pub fn online_use(&mut self, index: usize) {
        let page = &self.online_page;
        let (Some(query), Some(candidate)) = (
            page.query.clone(),
            page.results
                .as_ref()
                .and_then(|r| r.candidates.get(index))
                .cloned(),
        ) else {
            return;
        };
        let text = self.tr(
            "online-downloading",
            &[("name", candidate.file_name.clone())],
        );
        self.online_page.status = Some((text, true));
        self.online_page.busy = true;
        self.online_jobs.push(crate::online::OnlineJob::Download {
            generation: self.online_generation,
            query,
            candidate: Box::new(candidate),
        });
    }

    /// The answer of a search.
    pub fn online_found(
        &mut self,
        generation: u64,
        result: Result<crate::online::OnlineResults, String>,
    ) {
        if generation != self.online_generation {
            return;
        }
        self.online_page.busy = false;
        match result {
            Ok(results) => {
                if results.candidates.is_empty() && results.failed.is_empty() {
                    let text = self.tr("online-nothing-found", &[]);
                    self.online_page.status = Some((text, false));
                }
                self.online_page.results = Some(results);
            }
            Err(message) => self.online_page.status = Some((message, false)),
        }
    }

    /// A file downloaded from the page (`automatic` false) or for automatic
    /// subtitles: kept for its video, loaded if that video is still open and,
    /// for automatic subtitles, the user did not choose others meanwhile.
    pub fn online_downloaded(
        &mut self,
        generation: u64,
        automatic: bool,
        result: Result<Option<crate::online::Downloaded>, String>,
    ) {
        if generation != self.online_generation {
            // the file is kept for its video: the next opening finds it
            return;
        }
        if automatic && self.auto_online.take() != Some(self.sub_generation) {
            tracing::info!("automatic subtitles: the user chose others meanwhile");
            return;
        }
        if !automatic {
            self.online_page.busy = false;
        }
        match result {
            Ok(Some(d)) => {
                let source = Source::File(d.path.clone());
                if !self.sub_sources.contains(&source) {
                    self.sub_sources.push(source.clone());
                }
                self.use_subtitles(source);
                // a downloaded file may be timed for another release (SUB-018)
                self.align_after = Some(self.sub_generation);
                let text = self.tr(
                    "online-loaded",
                    &[
                        ("name", d.file_name.clone()),
                        ("provider", d.provider.name().to_string()),
                    ],
                );
                if !automatic {
                    self.online_page.status = Some((text.clone(), true));
                    self.online_page.loaded += 1;
                }
                self.toasts.push(text);
            }
            Ok(None) => {
                tracing::info!("automatic subtitles: nothing fits");
                self.toasts.push(self.tr("online-auto-none", &[]));
            }
            Err(message) if automatic => self.toasts.push(message),
            Err(message) => self.online_page.status = Some((message, false)),
        }
    }

    /// Save on the Online subtitles page: the settings written to
    /// `config.toml` (MET-005) and given to the `online` worker.
    pub fn online_save(
        &mut self,
        online: nikomiru_core::config::OnlineConfig,
        network: nikomiru_core::config::NetworkConfig,
    ) {
        self.services.config.online = online.clone();
        self.services.config.network = network.clone();
        self.online_jobs
            .push(crate::online::OnlineJob::Configure(Box::new((
                online, network,
            ))));
        self.saving_page = SavingPage::Online;
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.online_page.status = Some((text, false));
        }
    }

    /// The Player settings page opens: the saved presets.
    pub fn player_open(&mut self) {
        self.player_page.presets = self.presets();
        self.player_page.preview = None;
        self.player_page.status = None;
    }

    fn presets(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.services.mpv_presets_dir)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        name.strip_suffix(".conf").map(str::to_string)
                    })
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// The option set as saved (PLY-016).
    pub fn player_set(&self) -> nikomiru_player::settings::OptionSet {
        nikomiru_player::settings::OptionSet::from_config(&self.services.config.player)
    }

    /// The last preset loaded or saved.
    pub fn player_preset(&self) -> &str {
        &self.services.config.player.mpv_preset
    }

    fn apply_option_now(&mut self, key: &str, value: &str) {
        let option = nikomiru_player::options::MpvOption {
            key: key.to_string(),
            value: value.to_string(),
        };
        match self.services.player.apply_option(&option) {
            Ok(reply) => self.option_replies.push((key.to_string(), reply)),
            Err(e) => tracing::warn!("option {key}: {e}"),
        }
    }

    /// Applies `set` (PLY-016, PLY-018): written to `config.toml` and set in
    /// mpv now (SET-002), an option the user unset going back to mpv's own
    /// value; the options mpv takes only later are named.
    pub fn player_apply(
        &mut self,
        set: nikomiru_player::settings::OptionSet,
        preset: Option<String>,
    ) {
        use nikomiru_player::options::{Line, parse_mpv_conf};
        use nikomiru_player::settings::{OptionSet, control_of};
        let old = OptionSet::from_config(&self.services.config.player);
        let mut player = self.services.config.player.clone();
        set.apply_to(&mut player);
        if let Some(name) = preset {
            player.mpv_preset = name;
        }
        self.services.config.player = player;
        let mut later: Vec<String> = Vec::new();
        for (key, value) in &set.values {
            if old.values.get(key) == Some(value) {
                continue;
            }
            if control_of(key).is_some_and(|c| c.next_file) {
                later.push(key.clone());
            } else {
                self.apply_option_now(key, value);
            }
        }
        let unset: Vec<String> = old
            .values
            .keys()
            .filter(|k| !set.values.contains_key(*k))
            .cloned()
            .collect();
        for key in unset {
            match self.services.player.option_default(&key) {
                Some(default) => self.apply_option_now(&key, &default),
                None => later.push(key),
            }
        }
        if set.raw != old.raw {
            let applied = |raw: &str| -> Vec<(String, String)> {
                parse_mpv_conf(raw)
                    .into_iter()
                    .filter_map(|l| match l {
                        Line::Apply { option, .. } => Some((option.key, option.value)),
                        _ => None,
                    })
                    .collect()
            };
            let new = applied(&set.raw);
            for (key, value) in &new {
                self.apply_option_now(key, value);
            }
            // a raw option removed keeps its value until the next start
            later.extend(
                applied(&old.raw)
                    .into_iter()
                    .map(|(k, _)| k)
                    .filter(|k| !new.iter().any(|(n, _)| n == k)),
            );
        }
        self.player_page.applied += 1;
        self.player_page.preview = None;
        self.saving_page = SavingPage::Player;
        self.keep_status = !later.is_empty();
        if !later.is_empty() {
            later.sort();
            later.dedup();
            let text = self.tr("player-later", &[("names", later.join(", "))]);
            self.player_page.status = Some((text, true));
        }
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.player_page.status = Some((text, false));
        }
    }

    /// The options mpv refused when they were set now.
    pub fn options_applied(&mut self, refused: Vec<(String, String)>) {
        if refused.is_empty() {
            return;
        }
        let names: Vec<String> = refused.iter().map(|(k, e)| format!("{k} ({e})")).collect();
        let text = self.tr("player-refused", &[("names", names.join(", "))]);
        self.player_page.status = Some((text, false));
    }

    /// The answers to wait on, taken by the UI layer.
    pub fn take_option_replies(&mut self) -> Vec<(String, nikomiru_player::CommandReply)> {
        std::mem::take(&mut self.option_replies)
    }

    fn ignored_reason(&self, why: &nikomiru_player::options::Ignored) -> String {
        use nikomiru_player::options::Ignored;
        let id = match why {
            Ignored::Fixed => "player-ignored-fixed",
            Ignored::Blocked(_) => "player-ignored-blocked",
            Ignored::NoEffect(_) => "player-ignored-no-effect",
            Ignored::Section => "player-ignored-section",
        };
        self.tr(id, &[])
    }

    /// The preview of `text` from `source`.
    fn preview(&mut self, source: String, preset: Option<String>, text: &str) {
        let reason = |why: &nikomiru_player::options::Ignored| self.ignored_reason(why);
        let preview = crate::player_settings::Preview::of(source, preset, text, &reason);
        self.player_page.preview = Some(preview);
        self.player_page.status = None;
    }

    /// Reads a preset or an `mpv.conf`, at most 1 MiB.
    fn read_conf(&self, path: &Path) -> Result<String, String> {
        use std::io::Read;
        let mut text = String::new();
        std::fs::File::open(path)
            .and_then(|f| f.take(1 << 20).read_to_string(&mut text))
            .map(|_| text)
            .map_err(|e| {
                self.tr(
                    "player-read-error",
                    &[
                        ("path", path.display().to_string()),
                        ("reason", e.to_string()),
                    ],
                )
            })
    }

    /// A saved preset, shown in the preview (PLY-016).
    pub fn player_load_preset(&mut self, name: &str) {
        let Some(file) = nikomiru_player::settings::preset_file_name(name) else {
            return;
        };
        match self.read_conf(&self.services.mpv_presets_dir.join(file)) {
            Ok(text) => self.preview(name.to_string(), Some(name.to_string()), &text),
            Err(message) => self.player_page.status = Some((message, false)),
        }
    }

    /// An `mpv.conf` chosen in the file dialog, shown in the preview.
    pub fn player_import(&mut self, path: &Path) {
        let source = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.read_conf(path) {
            Ok(text) => self.preview(source, None, &text),
            Err(message) => self.player_page.status = Some((message, false)),
        }
    }

    /// The preview applied.
    pub fn player_preview_apply(&mut self) {
        if let Some(p) = self.player_page.preview.take() {
            self.player_apply(p.set, p.preset);
        }
    }

    /// The preview dropped.
    pub fn player_preview_cancel(&mut self) {
        self.player_page.preview = None;
    }

    /// The header of a saved preset: the version and the date.
    fn preset_header(&self) -> String {
        let days = self
            .services
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() / 86_400);
        let (y, m, d) = civil_date(days as i64);
        format!(
            "Nikomiru {} mpv options
{y:04}-{m:02}-{d:02}",
            env!("CARGO_PKG_VERSION")
        )
    }

    /// Saves `set` as the preset `name` (PLY-016).
    pub fn player_save_preset(&mut self, name: &str, set: nikomiru_player::settings::OptionSet) {
        let Some(file) = nikomiru_player::settings::preset_file_name(name) else {
            let text = self.tr("player-bad-name", &[]);
            self.player_page.status = Some((text, false));
            return;
        };
        self.file_writes.push(crate::player_settings::FileWrite {
            path: self.services.mpv_presets_dir.join(file),
            text: set.to_mpv_conf(&self.preset_header()),
            mode: 0o600,
            preset: Some(name.trim().to_string()),
        });
    }

    /// The set to export, until the file dialog answers.
    pub fn player_export(&mut self, set: nikomiru_player::settings::OptionSet) {
        self.pending_export = Some(set);
    }

    /// The file chosen for the export.
    pub fn player_export_to(&mut self, path: &Path) {
        if let Some(set) = self.pending_export.take() {
            self.file_writes.push(crate::player_settings::FileWrite {
                path: path.to_path_buf(),
                text: set.to_mpv_conf(&self.preset_header()),
                mode: 0o644,
                preset: None,
            });
        }
    }

    /// The files to write, taken by the UI layer.
    pub fn take_file_writes(&mut self) -> Vec<crate::player_settings::FileWrite> {
        std::mem::take(&mut self.file_writes)
    }

    /// A preset or an export was written, or not.
    pub fn file_written(
        &mut self,
        path: PathBuf,
        preset: Option<String>,
        result: Result<(), String>,
    ) {
        let status = match (&result, &preset) {
            (Ok(()), Some(name)) => (
                self.tr("player-preset-saved", &[("name", name.clone())]),
                true,
            ),
            (Ok(()), None) => (
                self.tr("player-exported", &[("path", path.display().to_string())]),
                true,
            ),
            (Err(message), _) => (message.clone(), false),
        };
        if result.is_ok() && preset.is_some() {
            self.player_page.presets = self.presets();
        }
        self.player_page.status = Some(status);
    }

    /// The online jobs to start, taken by the UI layer.
    pub fn take_online_jobs(&mut self) -> Vec<crate::online::OnlineJob> {
        std::mem::take(&mut self.online_jobs)
    }

    /// The online service.
    pub fn online(&self) -> &dyn crate::online::OnlineService {
        self.services.online.as_ref()
    }

    /// The online settings as saved.
    pub fn online_config(
        &self,
    ) -> (
        &nikomiru_core::config::OnlineConfig,
        &nikomiru_core::config::NetworkConfig,
    ) {
        (&self.services.config.online, &self.services.config.network)
    }

    /// Marks the last card's entries `✓↩` (ANK-030).
    fn apply_last_card(&mut self) {
        use crate::mining::CardState;
        let last = self.last_card.clone();
        for p in self.popups.stack_mut() {
            let Some(reply) = &p.reply else { continue };
            for (k, e) in reply.content.entries.iter().enumerate() {
                let Some(state) = p.cards.get_mut(k) else {
                    continue;
                };
                match &last {
                    Some(l) if l.expression == e.expression && l.reading == e.reading => {
                        if *state != CardState::Busy {
                            *state = CardState::Last(l.note_id);
                        }
                    }
                    _ if matches!(state, CardState::Last(_)) => *state = CardState::Exists,
                    _ => {}
                }
            }
        }
    }

    /// The answer of a check (ANK-030).
    pub fn card_states(&mut self, generation: u64, states: Option<Vec<bool>>) {
        use crate::mining::CardState;
        let Some(p) = self
            .popups
            .stack_mut()
            .iter_mut()
            .find(|p| p.generation == generation)
        else {
            return;
        };
        if let Some(states) = states {
            for (card, exists) in p.cards.iter_mut().zip(states) {
                if matches!(
                    card,
                    CardState::Unknown | CardState::CanAdd | CardState::Exists
                ) {
                    *card = if exists {
                        CardState::Exists
                    } else {
                        CardState::CanAdd
                    };
                }
            }
        }
        self.apply_last_card();
    }

    /// A click on an entry's card button, or Ctrl+Enter (ANK-030, POP-014).
    pub fn card_button(&mut self, level: usize, entry: usize) {
        use crate::mining::{CardAction, CardState};
        let Some(p) = self.popups.stack_mut().get_mut(level) else {
            return;
        };
        let state = p.cards.get(entry).copied().unwrap_or_default();
        match state {
            CardState::Busy => {}
            s if s.has_menu() => {
                p.menu = if p.menu == Some(entry) {
                    None
                } else {
                    Some(entry)
                };
            }
            _ => self.start_mining(level, entry, CardAction::Add),
        }
    }

    /// An item of an entry's card menu (ANK-030, ANK-037, ANK-038).
    pub fn card_menu(&mut self, level: usize, entry: usize, item: crate::mining::MenuItem) {
        use crate::mining::{AnkiJob, BrowseTarget, CardAction, CardState, MenuItem};
        let Some(p) = self.popups.stack_mut().get_mut(level) else {
            return;
        };
        p.menu = None;
        let state = p.cards.get(entry).copied().unwrap_or_default();
        let Some(e) = p.reply.as_ref().and_then(|r| r.content.entries.get(entry)) else {
            return;
        };
        let (expression, reading) = (e.expression.clone(), e.reading.clone());
        match (item, state) {
            (MenuItem::Overwrite, CardState::Last(id)) => {
                self.start_mining(level, entry, CardAction::Overwrite(id));
            }
            (MenuItem::AddAnyway, _) => self.start_mining(level, entry, CardAction::AddAnyway),
            (MenuItem::OpenInAnki, CardState::Last(id)) => {
                self.anki_jobs.push(AnkiJob::Browse(BrowseTarget::Note(id)));
            }
            (MenuItem::OpenInAnki, _) => {
                self.anki_jobs.push(AnkiJob::Browse(BrowseTarget::Word {
                    expression,
                    reading,
                }));
            }
            (MenuItem::Overwrite, _) => {}
        }
    }

    /// The reference cue of a card (ANK-031): the one the lookup started from,
    /// the current one, the one found by position.
    fn reference_cue(&self) -> Option<usize> {
        let t = self.subs.timeline();
        let pos = (self.view.time_pos * 1000.0) as i64;
        self.popups
            .root_cue()
            .filter(|&c| c < t.cues().len())
            .or_else(|| t.active(pos).first().copied())
            .or_else(|| t.near(pos))
    }

    /// Builds the request for a card from entry `entry` of popup `level`.
    fn start_mining(&mut self, level: usize, entry: usize, action: crate::mining::CardAction) {
        use crate::mining::{AnkiJob, CardState, MineRequest};
        let Some(current) = &self.current else {
            return;
        };
        let Some(p) = self.popups.stack().get(level) else {
            return;
        };
        let Some(e) = p.reply.as_ref().and_then(|r| r.content.entries.get(entry)) else {
            return;
        };
        let anki = &self.services.config.anki;
        let t = self.subs.timeline();
        let cue = self.reference_cue();
        let sentence = cue
            .and_then(|c| t.cues().get(c))
            .map(|c| c.plain_text.clone())
            .unwrap_or_default();
        // the word's place, if this popup's lookup started in that cue
        let offset = match (level, self.popups.stack().first().map(|r| &r.origin)) {
            (0, Some(Origin::Subtitle { cue: c, start })) if Some(*c) == cue => Some(*start),
            _ => None,
        };
        // a cue of no length gives no audio
        let clip = cue
            .map(|c| {
                let (start, end) = t.clip(
                    c,
                    i64::from(anki.audio_pad_head_ms),
                    i64::from(anki.audio_pad_tail_ms),
                );
                (start, end.min(start + nikomiru_media::clip::MAX_AUDIO_MS))
            })
            .filter(|(start, end)| end > start);
        let (audio_input, audio_stream) = self.audio_source(&current.path);
        let request = MineRequest {
            query: p.query.clone(),
            entry,
            expression: e.expression.clone(),
            reading: e.reading.clone(),
            matched: e.matched.clone(),
            sentence,
            offset,
            video: current.path.clone(),
            audio_input,
            audio_stream,
            clip,
            position_ms: (self.view.time_pos * 1000.0) as i64,
            uid: current.uid.clone(),
            title: current
                .path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            action,
        };
        let generation = p.generation;
        if let Some(card) = self
            .popups
            .stack_mut()
            .get_mut(level)
            .and_then(|p| p.cards.get_mut(entry))
        {
            *card = CardState::Busy;
        }
        let frame = self.ask_frame();
        self.anki_jobs.push(AnkiJob::Mine {
            generation,
            entry,
            request: Box::new(request),
            frame,
        });
    }

    /// The active audio track: the file it is in (an external track's own
    /// file) and its ff-index there; `None` without one.
    fn audio_source(&self, video: &Path) -> (PathBuf, Option<i64>) {
        let selected = self
            .view
            .audio_tracks
            .iter()
            .find(|tr| Some(tr.id) == self.view.aid);
        match selected {
            Some(tr) if tr.external => match &tr.external_filename {
                Some(file) => (PathBuf::from(file), tr.ff_index),
                None => (video.to_path_buf(), None),
            },
            Some(tr) => (video.to_path_buf(), tr.ff_index),
            None => (video.to_path_buf(), None),
        }
    }

    /// Aligns the subtitles shown with the audio (SUB-018): asked from the
    /// menu, or `automatic`ally after downloaded subtitles load.
    pub fn align_subtitles(&mut self, automatic: bool) {
        let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
            return;
        };
        let cues: Vec<(i64, i64)> = self
            .subs
            .timeline()
            .cues()
            .iter()
            .map(|c| (c.start_ms, c.end_ms))
            .collect();
        if cues.is_empty() {
            if !automatic {
                self.toasts.push(self.tr("toast-sub-align-none", &[]));
            }
            return;
        }
        let (input, stream) = self.audio_source(&video);
        if !automatic {
            self.toasts.push(self.tr("toast-sub-aligning", &[]));
        }
        self.align_jobs.push((
            self.sub_generation,
            automatic,
            crate::subtitles::AlignRequest {
                input,
                stream,
                cues,
            },
        ));
    }

    /// The alignments to start, taken by the UI layer.
    pub fn take_align_jobs(&mut self) -> Vec<(u64, bool, crate::subtitles::AlignRequest)> {
        std::mem::take(&mut self.align_jobs)
    }

    /// Starts an alignment on the loader.
    pub fn start_align(
        &self,
        request: crate::subtitles::AlignRequest,
        done: Box<dyn FnOnce(Result<nikomiru_subs::align::AlignResult, String>) + Send>,
    ) {
        self.services.subtitle_source.align(request, done);
    }

    /// The answer of an alignment: the offset applied when it is sure, for
    /// the subtitles it was asked for (SUB-018).
    pub fn aligned(
        &mut self,
        generation: u64,
        automatic: bool,
        result: Result<nikomiru_subs::align::AlignResult, String>,
    ) {
        use nikomiru_subs::align::AlignResult;
        if generation != self.sub_generation {
            return;
        }
        let percent = |c: f64| format!("{:.0}", (c * 100.0).max(0.0));
        match result {
            Ok(AlignResult::Aligned(a)) => {
                tracing::info!("aligned with the audio: {a:?}");
                self.subs.timeline_mut().delay_ms = a.offset_ms;
                self.subs.update((self.view.time_pos * 1000.0) as i64);
                self.sub_list_version += 1;
                self.toasts.push(self.tr(
                    "toast-sub-aligned",
                    &[
                        ("value", format!("{:+}", a.offset_ms)),
                        ("confidence", percent(a.confidence)),
                    ],
                ));
            }
            Ok(AlignResult::Unsure(a)) => {
                tracing::info!("not aligned, unsure: {a:?}");
                if !automatic {
                    self.toasts.push(self.tr(
                        "toast-sub-align-unsure",
                        &[
                            ("value", format!("{:+}", a.offset_ms)),
                            ("confidence", percent(a.confidence)),
                        ],
                    ));
                }
            }
            Ok(AlignResult::NoData) => {
                if !automatic {
                    self.toasts.push(self.tr("toast-sub-align-none", &[]));
                }
            }
            Err(message) => {
                tracing::warn!("alignment: {message}");
                if !automatic {
                    self.toasts.push(message);
                }
            }
        }
    }

    /// Asks the player for the frame on screen now, for a card (ANK-034): it
    /// is saved in order with the player's other commands, before a resume.
    fn ask_frame(&self) -> Option<crate::mining::PendingFrame> {
        use std::sync::mpsc::RecvTimeoutError;
        let dir = &self.services.mining_dir;
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("{}: {e}", dir.display());
            return None;
        }
        let path = dir.join(format!("{}.png", nikomiru_anki::link::new_source_id()));
        let reply = match self.services.player.screenshot(&path) {
            Ok(reply) => reply,
            Err(e) => {
                tracing::warn!("card frame: {e}");
                return None;
            }
        };
        let late = path.clone();
        Some(crate::mining::PendingFrame::new(
            path,
            Box::new(move |timeout| match reply.recv_timeout(timeout) {
                Ok(result) => result.map_err(|e| e.to_string()),
                Err(RecvTimeoutError::Timeout) => {
                    // the file may still come: it goes when it does
                    let spawned =
                        std::thread::Builder::new()
                            .name("late-frame".into())
                            .spawn(move || {
                                let _ = reply.recv();
                                let _ = std::fs::remove_file(&late);
                            });
                    if let Err(e) = spawned {
                        tracing::warn!("late frame: {e}");
                    }
                    Err("the player did not save the frame in time".into())
                }
                Err(RecvTimeoutError::Disconnected) => Err("the player stopped".into()),
            }),
        ))
    }

    /// A card was made, or not (ANK-030…ANK-039): `headword` is the entry's
    /// expression and reading, the last card even if its popup has closed.
    pub fn mined(
        &mut self,
        generation: u64,
        entry: usize,
        headword: (String, String),
        result: Result<crate::mining::Mined, crate::mining::MineFailure>,
    ) {
        use crate::mining::{CardState, LastCard};
        match result {
            Ok(mined) => {
                let (expression, reading) = headword;
                self.last_card = Some(LastCard {
                    note_id: mined.note_id,
                    expression,
                    reading,
                });
                if let Some(card) = self
                    .popups
                    .stack_mut()
                    .iter_mut()
                    .find(|p| p.generation == generation)
                    .and_then(|p| p.cards.get_mut(entry))
                {
                    *card = CardState::Last(mined.note_id);
                }
                self.apply_last_card();
                let key = if mined.overwritten {
                    "toast-card-overwritten"
                } else {
                    "toast-card-added"
                };
                let message = self.tr(key, &[("deck", mined.deck.clone())]);
                self.toasts.push(message);
                self.events.push(UiEvent::CardCreated(mined.note_id));
            }
            Err(failure) => {
                if let Some(card) = self
                    .popups
                    .stack_mut()
                    .iter_mut()
                    .find(|p| p.generation == generation)
                    .and_then(|p| p.cards.get_mut(entry))
                {
                    *card = if failure.duplicate {
                        CardState::Exists
                    } else {
                        CardState::Unknown
                    };
                }
                self.apply_last_card();
                self.toasts.push(failure.message);
            }
        }
    }

    /// Anki's browser opened, or not (ANK-038).
    pub fn browsed(&mut self, result: Result<(), String>) {
        if let Err(message) = result {
            self.toasts.push(message);
        }
    }

    fn update_screensaver(&mut self) {
        let now = self.services.clock.monotonic();
        let playing = self.view.loaded && !self.view.paused;
        self.screensaver.set_playing(playing, now);
        let want = self.screensaver.inhibit(now);
        if want != self.inhibited {
            self.services.inhibitor.set(want);
            self.inhibited = want;
        }
    }

    fn save_position(&self, ms: u64) {
        let (Some(w), Some(Current { uid: Some(uid), .. })) =
            (self.writer.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let at = self
            .services
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let uid = uid.clone();
        w.run(move |lib| {
            if let Err(e) = lib.save_position(&uid, ms, at) {
                tracing::warn!("saving the position: {e}");
            }
        });
    }

    /// Exit or file change: write the last position and the per-file settings.
    pub fn flush_position(&mut self) {
        if let Some(ms) = self.saver.flush() {
            self.save_position(ms);
        }
        let (Some(w), Some(Current { uid: Some(uid), .. })) =
            (self.writer.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let (uid, aid, volume, speed, duration) = (
            uid.clone(),
            self.view.aid,
            self.view.volume,
            self.view.speed,
            self.view.duration,
        );
        w.run(move |lib| {
            if let Ok(Some(mut rec)) = lib.video(&uid) {
                rec.aid = aid;
                rec.volume = Some(volume);
                rec.speed = Some(speed);
                if duration > 0.0 {
                    rec.duration_ms = Some((duration * 1000.0) as u64);
                }
                let _ = lib.put_video(&uid, &rec);
            }
        });
    }

    /// The mouse moved over the window (PLY-004).
    pub fn mouse_moved(&mut self) {
        self.controls.mouse_moved(self.services.clock.monotonic());
    }

    /// Whether the control bar and the cursor are visible now (PLY-004).
    pub fn controls_visible(&self) -> bool {
        !self.view.loaded || self.controls.visible(self.services.clock.monotonic())
    }

    /// A key event: the physical key as an evdev code (KEY-002) and the
    /// modifiers. `text_focus`: a text field has the keyboard (KEY-004). Returns
    /// the effect and whether a binding took the key.
    pub fn key(
        &mut self,
        evdev: u32,
        mods: Modifiers,
        pressed: bool,
        auto_repeat: bool,
        text_focus: bool,
    ) -> (UiEffect, bool) {
        let Some(chord) = crate::shortcuts::chord(evdev, mods) else {
            if pressed {
                tracing::debug!("key: evdev code {evdev} has no name");
            }
            return (UiEffect::None, false);
        };
        // a popup's keys first, then the player's (KEY-003)
        let action = if self.popups.is_open() && !self.popups_covered {
            self.bindings
                .action(Context::Popup, &chord, text_focus)
                .or_else(|| self.bindings.action(Context::Player, &chord, text_focus))
        } else {
            self.bindings.action(Context::Player, &chord, text_focus)
        };
        if pressed && !auto_repeat {
            tracing::debug!("key {chord} → {}", action.map_or("nothing", Action::name));
        }
        let Some(action) = action else {
            return (UiEffect::None, false);
        };
        if !pressed {
            if action == Action::HoldSpeed {
                self.release_hold();
            }
            return (UiEffect::None, true);
        }
        if action == Action::HoldSpeed {
            if let Some(speed) = self.hold.key_down(self.view.speed, auto_repeat) {
                let _ = self.services.player.set_speed(speed);
            }
            return (UiEffect::None, true);
        }
        if auto_repeat && !action.repeats() {
            return (UiEffect::None, true);
        }
        (self.action(action), true)
    }

    /// Ends hold-to-speed (PLY-002): on the key's release, or when the window
    /// loses the keyboard and the release will never arrive.
    pub fn release_hold(&mut self) {
        if let Some(speed) = self.hold.key_up() {
            let _ = self.services.player.set_speed(speed);
        }
    }

    /// Runs an action (from a key or a button).
    pub fn action(&mut self, action: Action) -> UiEffect {
        let p = &self.services.player;
        let step = control::seek_step(self.services.config.player.seek_seconds);
        let result = match action {
            Action::PlayPause => {
                self.popups.playback_changed_by_user();
                self.end_replay();
                self.services.player.toggle_pause()
            }
            Action::PopupNext | Action::PopupPrev => {
                self.popup_entry(if action == Action::PopupNext { 1 } else { -1 });
                Ok(())
            }
            Action::CreateCard => {
                let top = self.popups.stack().len().checked_sub(1);
                if let Some((level, entry)) =
                    top.and_then(|l| self.popups.stack().get(l).map(|p| (l, p.entry)))
                {
                    self.card_button(level, entry);
                }
                Ok(())
            }
            Action::ReplayCue if self.popups.is_open() && !self.popups_covered => {
                self.popup_replay();
                Ok(())
            }
            Action::SeekBack => p.seek_relative(-step),
            Action::SeekForward => p.seek_relative(step),
            Action::VolumeUp | Action::VolumeDown => {
                let dir = if action == Action::VolumeUp { 1 } else { -1 };
                let v = control::step_volume(self.view.volume, dir);
                self.toasts
                    .push(self.tr("toast-volume", &[("value", format!("{v:.0}"))]));
                p.set_volume(v)
            }
            Action::Mute => {
                let key = if self.view.muted {
                    "toast-unmuted"
                } else {
                    "toast-muted"
                };
                self.toasts.push(self.tr(key, &[]));
                p.set_mute(!self.view.muted)
            }
            Action::SpeedUp | Action::SpeedDown | Action::SpeedReset => {
                let s = match action {
                    Action::SpeedUp => control::step_speed(self.view.speed, 1),
                    Action::SpeedDown => control::step_speed(self.view.speed, -1),
                    _ => 1.0,
                };
                self.toasts
                    .push(self.tr("toast-speed", &[("value", format!("{s:.2}"))]));
                p.set_speed(s)
            }
            Action::FramePrev => p.frame_step(false),
            Action::FrameNext => p.frame_step(true),
            Action::Screenshot => {
                if let Some(current) = &self.current {
                    let name = crate::screenshot::file_name(&current.path, self.view.time_pos);
                    self.screenshots.push(name);
                }
                Ok(())
            }
            Action::ChapterPrev => p.chapter(-1),
            Action::ChapterNext => p.chapter(1),
            Action::SubPrev | Action::SubNext => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                let secs = i64::from(self.services.config.player.seek_seconds.max(1));
                let t = self.subs.timeline();
                let jump = if action == Action::SubPrev {
                    t.previous(pos, true, secs)
                } else {
                    t.next(pos, secs)
                };
                match jump {
                    Jump::To(ms) => p.seek_absolute(ms as f64 / 1000.0),
                    Jump::Seek(s) => p.seek_relative(s as f64),
                }
            }
            Action::ReplayCue | Action::ReplayPrevCue => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                let t = self.subs.timeline();
                let target = if action == Action::ReplayCue {
                    t.replay(pos).map(|(start, _)| start)
                } else {
                    match t.previous(pos, false, 0) {
                        Jump::To(ms) => Some(ms),
                        Jump::Seek(_) => None,
                    }
                };
                match target {
                    Some(ms) => p.seek_absolute(ms as f64 / 1000.0),
                    None => Ok(()),
                }
            }
            Action::SubDelayMinus | Action::SubDelayPlus => {
                let steps = if action == Action::SubDelayPlus {
                    1
                } else {
                    -1
                };
                let d = self.subs.timeline_mut().nudge(steps);
                self.after_delay_change(d);
                Ok(())
            }
            Action::SubAlignPrev | Action::SubAlignNext => {
                let pos = (self.view.time_pos * 1000.0) as i64;
                if let Some(d) = self
                    .subs
                    .timeline_mut()
                    .align(pos, action == Action::SubAlignNext)
                {
                    self.after_delay_change(d);
                }
                Ok(())
            }
            Action::SubBlur | Action::SubHide => {
                let wanted = if action == Action::SubBlur {
                    Obscure::Blur
                } else {
                    Obscure::Hidden
                };
                self.subs.mode = if self.subs.mode == wanted {
                    Obscure::None
                } else {
                    wanted
                };
                let key = match self.subs.mode {
                    Obscure::None => "toast-sub-visible",
                    Obscure::Blur => "toast-sub-blurred",
                    Obscure::Hidden => "toast-sub-hidden",
                };
                self.toasts.push(self.tr(key, &[]));
                Ok(())
            }
            Action::SubToggle => {
                self.subs.disabled = !self.subs.disabled;
                let key = if self.subs.disabled {
                    "toast-sub-off"
                } else {
                    "toast-sub-on"
                };
                self.toasts.push(self.tr(key, &[]));
                Ok(())
            }
            Action::SubList => return UiEffect::ToggleSubList,
            Action::SubSearch => return UiEffect::SubSearch,
            Action::Fullscreen => return UiEffect::ToggleFullscreen,
            Action::Back if self.popups.is_open() && !self.popups_covered => {
                self.close_popup();
                return UiEffect::None;
            }
            Action::Back => return UiEffect::Back,
            Action::OpenFile => return UiEffect::OpenDialog,
            Action::HoldSpeed => Ok(()),
        };
        if let Err(e) = result {
            // a command mpv rejects is not fatal (e.g. a chapter past the last)
            tracing::warn!("player command {} failed: {e}", action.name());
        }
        UiEffect::None
    }

    /// Absolute seek from the progress bar.
    pub fn seek_to(&self, seconds: f64) {
        let _ = self.services.player.seek_absolute(seconds);
    }

    /// Volume from the slider.
    pub fn set_volume(&self, volume: f64) {
        let _ = self.services.player.set_volume(volume);
    }

    /// Audio track from the menu.
    pub fn select_audio(&self, id: i64) {
        let _ = self.services.player.set_aid((id > 0).then_some(id));
    }

    /// Current file, if any.
    pub fn current_path(&self) -> Option<&Path> {
        self.current.as_ref().map(|c| c.path.as_path())
    }
}

/// Subtitles (SUB-005…SUB-016).
impl Session {
    /// The cues were replaced or removed: the list starts over, and the popups
    /// close, since they looked up words of the cues that were (and replay or
    /// go to them).
    fn cues_changed(&mut self) {
        self.sub_list_version += 1;
        if self.popups.is_open() {
            self.close_all_popups();
        }
    }

    fn reset_subtitles(&mut self) {
        self.subs.clear(None);
        self.sub_sources.clear();
        self.sub_tracks.clear();
        self.sub_generation += 1;
        self.sub_loads.clear();
        self.sub_auto_done = false;
        self.cues_changed();
    }

    /// The subtitle sources of the current file: external files next to it
    /// (SUB-006) and the embedded tracks; the first time, the automatic choice
    /// (SUB-008).
    fn discover_subtitles(&mut self) {
        let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
            return;
        };
        let entries: Vec<PathBuf> = video
            .parent()
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        // and the subtitles downloaded for it (MET-008)
        let downloaded = self
            .current
            .as_ref()
            .and_then(|c| c.hash)
            .map(|h| {
                nikomiru_online::store::list(&nikomiru_online::store::folder(
                    &self.services.subtitles_dir,
                    &h,
                ))
            })
            .unwrap_or_default();
        let mut externals = select::external_files(&video, &entries);
        externals.extend(select::downloaded_files(&downloaded));
        let mut sources: Vec<Source> = externals
            .iter()
            .map(|e| Source::File(e.path.clone()))
            .collect();
        // a file the user opened stays in the menu
        if let Some(Source::File(p)) = &self.subs.source
            && !sources.contains(&Source::File(p.clone()))
        {
            sources.push(Source::File(p.clone()));
        }
        let mut embedded = Vec::new();
        for t in &self.sub_tracks {
            if t.external {
                continue;
            }
            let codec = t.codec.clone().unwrap_or_default();
            let text = nikomiru_media::extract::is_text_codec(&codec);
            sources.push(match (text, t.ff_index) {
                (true, Some(ff_index)) => Source::Embedded {
                    id: t.id,
                    ff_index,
                    codec,
                },
                _ => Source::Bitmap(t.id),
            });
            embedded.push(EmbeddedTrack {
                id: t.id,
                language: t.lang.clone(),
                title: t.title.clone(),
                text,
            });
        }
        self.sub_sources = sources;
        if self.sub_auto_done || !self.view.loaded {
            return;
        }
        self.sub_auto_done = true;
        let preferred = self.services.config.subtitles.preferred_languages.clone();
        let choice = preferred
            .iter()
            .find_map(|lang| select::choose(None, &externals, &embedded, lang));
        let source = match choice {
            Some(Choice::External(p)) => Some(Source::File(p)),
            Some(Choice::Embedded(id)) => self
                .sub_sources
                .iter()
                .find(
                    |s| matches!(s, Source::Embedded { id: i, .. } | Source::Bitmap(i) if *i == id),
                )
                .cloned(),
            None => None,
        };
        // no text subtitles in the preferred language: online ones (MET-007)
        let online = matches!(source, None | Some(Source::Bitmap(_)));
        if let Some(source) = source {
            self.use_subtitles(source);
        }
        if online {
            self.ask_online_subtitles(&video);
        }
        // a link's cue comes with this load or not at all (ANK-052)
        let load = self.sub_loads.last().map(|(generation, _)| *generation);
        self.link_clip = self
            .link_clip
            .take()
            .zip(load)
            .map(|(link, load)| LinkClip {
                load: Some(load),
                ..link
            });
    }

    /// Shows `source`: mpv draws graphic tracks; text ones are loaded in the
    /// background and drawn by the overlay.
    fn use_subtitles(&mut self, source: Source) {
        self.sub_generation += 1;
        self.sub_loads.clear();
        match &source {
            Source::Bitmap(id) => {
                let _ = self.services.player.show_bitmap_subtitles(Some(*id));
                self.subs.clear(Some(source));
                self.cues_changed();
            }
            Source::File(_) | Source::Embedded { .. } => {
                let _ = self.services.player.show_bitmap_subtitles(None);
                let Some(video) = self.current.as_ref().map(|c| c.path.clone()) else {
                    return;
                };
                self.sub_loads.push((
                    self.sub_generation,
                    Request {
                        video,
                        source,
                        encoding: self.subs.forced_encoding.clone(),
                        duration_ms: (self.view.duration * 1000.0) as i64,
                    },
                ));
            }
        }
    }

    /// Loads waiting to be started by the UI layer, with their generation.
    pub fn take_subtitle_loads(&mut self) -> Vec<(u64, Request)> {
        std::mem::take(&mut self.sub_loads)
    }

    /// Starts `request` on the loader; `done` gets the result.
    pub fn start_subtitle_load(
        &self,
        request: Request,
        done: Box<dyn FnOnce(Result<Loaded, String>) + Send>,
    ) {
        self.services.subtitle_source.load(request, done);
    }

    /// A load finished. A result for an older request is dropped.
    pub fn subtitles_loaded(
        &mut self,
        generation: u64,
        source: Source,
        result: Result<Loaded, String>,
    ) {
        if generation != self.sub_generation {
            return;
        }
        // any other load ends the wait of a link: the user chose another
        let link = self.link_clip.take().filter(|l| l.load == Some(generation));
        match result {
            Ok(loaded) => {
                let name = source_name(&source);
                let encoding = loaded.encoding.as_deref().map(|e| self.encoding_label(e));
                self.subs.set_track(source, loaded);
                self.subs.update((self.view.time_pos * 1000.0) as i64);
                self.cues_changed();
                if let Some(link) = link {
                    self.go_to_link_cue(link);
                }
                if self.align_after.take() == Some(generation) {
                    self.align_subtitles(true);
                }
                let text = match encoding {
                    Some(e) => self.tr(
                        "toast-sub-loaded-encoding",
                        &[("name", name), ("encoding", e)],
                    ),
                    None => self.tr("toast-sub-loaded", &[("name", name)]),
                };
                self.toasts.push(text);
            }
            Err(reason) => {
                tracing::warn!("subtitles: {reason}");
                self.subs.clear(None);
                self.cues_changed();
                self.toasts
                    .push(self.tr("error-subtitles", &[("reason", reason)]));
            }
        }
    }

    /// Menu choice: a source key, or `"off"`.
    pub fn select_subtitles(&mut self, key: &str) {
        if key == "off" {
            self.sub_generation += 1;
            let _ = self.services.player.show_bitmap_subtitles(None);
            self.subs.clear(None);
            self.cues_changed();
            return;
        }
        if let Some(source) = self.sub_sources.iter().find(|s| s.key() == key).cloned() {
            self.use_subtitles(source);
        }
    }

    /// A subtitle file chosen by the user.
    pub fn open_subtitle_file(&mut self, path: &Path) {
        let source = Source::File(path.to_path_buf());
        if !self.sub_sources.contains(&source) {
            self.sub_sources.push(source.clone());
        }
        self.use_subtitles(source);
    }

    /// Forces an encoding (`""` = automatic) and reloads the file (SUB-005).
    pub fn force_encoding(&mut self, encoding: &str) {
        self.subs.forced_encoding = (!encoding.is_empty()).then(|| encoding.to_string());
        if let Some(source @ Source::File(_)) = self.subs.source.clone() {
            self.use_subtitles(source);
        }
    }

    /// The subtitle menu: `{key, kind, name, lang, title, codec}`.
    pub fn subtitle_menu(&self) -> serde_json::Value {
        let items = self.sub_sources.iter().map(|s| {
            let track = |id: i64| self.sub_tracks.iter().find(|t| t.id == id);
            match s {
                Source::File(p) => serde_json::json!({
                    "key": s.key(), "kind": "file", "name": source_name(s),
                    "path": p.display().to_string(),
                }),
                Source::Embedded { id, codec, .. } => serde_json::json!({
                    "key": s.key(), "kind": "text", "name": format!("#{id}"),
                    "lang": track(*id).and_then(|t| t.lang.clone()),
                    "title": track(*id).and_then(|t| t.title.clone()),
                    "codec": codec,
                }),
                Source::Bitmap(id) => serde_json::json!({
                    "key": s.key(), "kind": "bitmap", "name": format!("#{id}"),
                    "lang": track(*id).and_then(|t| t.lang.clone()),
                    "title": track(*id).and_then(|t| t.title.clone()),
                    "codec": track(*id).and_then(|t| t.codec.clone()),
                }),
            }
        });
        serde_json::Value::Array(items.collect())
    }

    /// A readable name for an encoding ("Japanese — Shift_JIS (CP932)"), or
    /// the encoding itself when there is no translation for it.
    pub fn encoding_label(&self, encoding: &str) -> String {
        let id = format!("encoding-{}", encoding.to_ascii_lowercase());
        self.services
            .translator
            .try_tr(&id, &[])
            .unwrap_or_else(|_| encoding.to_string())
    }

    fn after_delay_change(&mut self, delay_ms: i64) {
        self.subs.update((self.view.time_pos * 1000.0) as i64);
        self.sub_list_version += 1;
        self.toasts
            .push(self.tr("toast-sub-delay", &[("value", format!("{delay_ms:+}"))]));
    }

    /// A click on a grapheme of the overlay or of the list (POP-001): the
    /// text from there (LKP-001…LKP-003) is looked up in a popup next to
    /// `anchor`; playback pauses if it was going.
    pub fn select_grapheme(&mut self, cue: usize, offset: usize, anchor: Rect) {
        // a click: the popup does not close when the pointer leaves
        self.hover.reset();
        self.look_up_grapheme(cue, offset, anchor);
    }

    /// The pointer moved over a grapheme (POP-003): with Shift held, or
    /// always if so configured, a movement of 8 px looks it up.
    #[allow(clippy::too_many_arguments)]
    pub fn grapheme_hovered(
        &mut self,
        cue: usize,
        offset: usize,
        x: f64,
        y: f64,
        shift: bool,
        anchor: Rect,
    ) {
        let armed = shift || self.services.config.lookup.hover_without_modifier;
        if self.hover.moved(x, y, armed, (cue, offset)) {
            self.look_up_grapheme(cue, offset, anchor);
        }
    }

    /// The pointer entered or left the subtitles.
    pub fn subtitle_hover(&mut self, inside: bool) {
        self.subs.hovered = inside;
        let now = self.services.clock.monotonic();
        self.controls.set_over_subtitles(inside, now);
        self.hover.inside(inside || self.popup_hovered, now);
    }

    /// The pointer entered or left a popup.
    pub fn popup_hover(&mut self, inside: bool) {
        self.popup_hovered = inside;
        let now = self.services.clock.monotonic();
        self.hover.inside(inside || self.subs.hovered, now);
    }

    fn look_up_grapheme(&mut self, cue: usize, offset: usize, anchor: Rect) {
        let Some(text) = self.subs.cue(cue).map(|c| c.plain_text.clone()) else {
            return;
        };
        if offset >= text.len() {
            return;
        }
        let range = nikomiru_text::scan::query(&text, offset, nikomiru_text::scan::Context::Cue);
        let query = nikomiru_text::scan::normalize(&text[range.clone()]);
        // a space or a delimiter: nothing to look up, and nothing changes
        if query.is_empty() {
            return;
        }
        self.subs.selection = Some((cue, offset));
        self.subs.highlight = None;
        // where the normalized query starts in the cue
        let start = range.start + text[range].find(query.as_str()).unwrap_or(0);
        let playing = self.view.loaded && !self.view.paused;
        let commands =
            self.popups
                .open_from_subtitle(playing, query, Origin::Subtitle { cue, start }, anchor);
        self.run_popup_commands(commands);
    }

    /// A click in the text of a list row, on the character at index `pos`.
    pub fn select_in_list(&mut self, cue: usize, pos: usize, anchor: Rect) {
        if let Some(c) = self.subs.cue(cue) {
            let offset = crate::subtitles::char_to_byte(&c.plain_text, pos);
            self.select_grapheme(cue, offset, anchor);
        }
    }

    /// Looks `text` up in a root popup, as a click on a subtitle word does but
    /// with no cue behind it: the GUI test harness's `lookup` (TEST §2.1).
    #[cfg(feature = "test-harness")]
    pub fn look_up_text(&mut self, text: &str) {
        let query = nikomiru_text::scan::normalize(text);
        if query.is_empty() {
            return;
        }
        self.hover.reset();
        let playing = self.view.loaded && !self.view.paused;
        let commands =
            self.popups
                .open_from_subtitle(playing, query, Origin::Glossary, Rect::default());
        self.run_popup_commands(commands);
    }

    fn run_popup_commands(&mut self, commands: Vec<Command>) {
        for c in commands {
            match c {
                Command::Pause => {
                    let _ = self.services.player.set_pause(true);
                }
                Command::Resume => {
                    self.end_replay();
                    let _ = self.services.player.set_pause(false);
                }
                Command::Lookup { generation, text } => self.lookups.push((generation, text)),
            }
        }
        if !self.popups.is_open() {
            self.hover.reset();
            self.end_replay();
        }
        // the bar stays while a popup is open (PLY-004)
        self.controls.set_popup_open(self.popups.is_open());
        self.sync_highlight();
    }

    fn sync_highlight(&mut self) {
        self.subs.highlight = self.popups.highlight().and_then(|(cue, start, chars)| {
            let text = &self.subs.cue(cue)?.plain_text;
            let end = text
                .get(start..)?
                .char_indices()
                .nth(chars)
                .map_or(text.len(), |(i, _)| start + i);
            Some((cue, start..end))
        });
        if !self.popups.is_open() {
            self.subs.selection = None;
        }
    }

    /// A link or a selection in popup `level` (POP-011, POP-012).
    pub fn popup_lookup(&mut self, level: usize, text: &str, anchor: Rect) {
        let query = nikomiru_text::scan::normalize(text);
        if query.is_empty() {
            return;
        }
        let commands = self.popups.open_nested(level, query, anchor);
        self.run_popup_commands(commands);
    }

    /// A popup's border was dragged to `width` × `height` (POP-006): the next
    /// popups take that size until the app closes. `config.toml` keeps the
    /// configured one: the app does not write it yet.
    pub fn popup_resized(&mut self, width: f64, height: f64) {
        self.popups.set_size((width, height));
    }

    /// A click inside popup `level`: the popups above it close.
    pub fn popup_focus(&mut self, level: usize) {
        if self.popups.stack().len() > level + 1 {
            self.popups.focus(level);
        }
    }

    /// Esc or Back: the last popup closes (POP-012).
    pub fn close_popup(&mut self) {
        let commands = self.popups.close_last();
        self.run_popup_commands(commands);
    }

    /// A click outside the popups: they all close.
    pub fn close_all_popups(&mut self) {
        if self.popups.is_open() {
            let commands = self.popups.close_all();
            self.run_popup_commands(commands);
        }
    }

    /// Popup `level` is on screen with its content (T-PERF-06).
    pub fn popup_rendered(&mut self, level: usize) {
        let Some(p) = self.popups.stack().get(level) else {
            return;
        };
        let headwords = p
            .reply
            .as_ref()
            .map(|r| {
                r.content
                    .entries
                    .iter()
                    .map(|e| e.expression.clone())
                    .collect()
            })
            .unwrap_or_default();
        self.events.push(UiEvent::PopupRendered {
            level,
            query: p.query.clone(),
            headwords,
        });
    }

    /// The screenshots to take (file names), taken by the UI layer.
    pub fn take_screenshots(&mut self) -> Vec<String> {
        std::mem::take(&mut self.screenshots)
    }

    /// Saves the frame shown as `name` in the screenshots folder, off the UI
    /// thread; `done` gets the file written.
    pub fn start_screenshot(
        &self,
        name: String,
        done: Box<dyn FnOnce(Result<PathBuf, String>) + Send>,
    ) {
        crate::screenshot::start(
            self.services.player.clone(),
            self.services.screenshots_dir.clone(),
            name,
            done,
        );
    }

    /// A screenshot was written, or could not be (PLY-011).
    pub fn screenshot_done(&mut self, result: Result<PathBuf, String>) {
        let message = match &result {
            Ok(path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.tr("toast-screenshot", &[("name", name)])
            }
            Err(reason) => self.tr("toast-screenshot-failed", &[("reason", reason.clone())]),
        };
        self.toasts.push(message);
        if let Ok(path) = result {
            self.events.push(UiEvent::ScreenshotSaved(path));
        }
    }

    /// The lookups to start, taken by the UI layer.
    pub fn take_lookups(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.lookups)
    }

    /// Starts a lookup on the dictionary service.
    pub fn start_lookup(&self, text: String, done: Box<dyn FnOnce(Result<Reply, String>) + Send>) {
        self.services.dictionaries.lookup(text, done);
    }

    fn dict_effects(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Message(id, args) => {
                    let message = self.tr(id, &args);
                    self.toasts.push(message);
                }
                Effect::Error(message) => self.toasts.push(message),
                Effect::Start {
                    zip,
                    replace,
                    generation,
                    cancel,
                } => self.dict_imports.push(DictImport {
                    zip,
                    replace,
                    generation,
                    cancel,
                }),
            }
        }
    }

    /// Reads the dictionaries list again (the page opens).
    pub fn dict_refresh(&mut self) {
        let effects = self.dict_page.refresh(self.services.dictionaries.as_ref());
        self.dict_effects(effects);
    }

    /// A zip chosen in the dialog or dropped on the window.
    pub fn dict_choose(&mut self, zip: PathBuf) {
        let effects = self
            .dict_page
            .choose(self.services.dictionaries.as_ref(), zip);
        self.dict_effects(effects);
    }

    /// The answer to the update offer (DIC-016).
    pub fn dict_confirm(&mut self, update: bool) {
        let effects = self.dict_page.confirm(update);
        self.dict_effects(effects);
    }

    /// Stops the running import.
    pub fn dict_cancel(&mut self) {
        self.dict_page.cancel();
    }

    /// The imports to start, taken by the UI layer.
    pub fn take_dict_imports(&mut self) -> Vec<DictImport> {
        std::mem::take(&mut self.dict_imports)
    }

    /// Starts an import on the dictionary service.
    pub fn start_dict_import(
        &self,
        import: DictImport,
        progress: Box<dyn Fn(nikomiru_dict::import::Progress) + Send>,
        done: Box<dyn FnOnce(Result<String, String>) + Send>,
    ) {
        self.services.dictionaries.import(
            import.zip,
            import.replace,
            import.cancel,
            progress,
            done,
        );
    }

    /// Progress of an import.
    pub fn dict_progress(&mut self, generation: u64, progress: nikomiru_dict::import::Progress) {
        self.dict_page.progress(generation, progress);
    }

    /// An import ended.
    pub fn dict_finished(&mut self, generation: u64, result: Result<String, String>) {
        let ok = result.is_ok();
        let effects =
            self.dict_page
                .finished(self.services.dictionaries.as_ref(), generation, result);
        if ok {
            self.events.push(UiEvent::DictionaryImported);
        }
        self.dict_effects(effects);
    }

    /// Moves a dictionary up or down in its group (DIC-031).
    pub fn dict_move(&mut self, id: &str, delta: i32) {
        let effects = self
            .dict_page
            .move_by(self.services.dictionaries.as_ref(), id, delta);
        self.dict_effects(effects);
    }

    /// Moves a dictionary to a gap of its group, where it was dropped (DIC-031).
    pub fn dict_move_to(&mut self, id: &str, to: usize) {
        let effects = self
            .dict_page
            .move_to(self.services.dictionaries.as_ref(), id, to);
        self.dict_effects(effects);
    }

    /// Enables or disables a dictionary (DIC-032).
    pub fn dict_set_enabled(&mut self, id: &str, enabled: bool) {
        let effects = self
            .dict_page
            .set_enabled(self.services.dictionaries.as_ref(), id, enabled);
        self.dict_effects(effects);
    }

    /// Deletes a dictionary (DIC-033).
    pub fn dict_delete(&mut self, id: &str) {
        let effects = self
            .dict_page
            .delete(self.services.dictionaries.as_ref(), id);
        self.dict_effects(effects);
    }

    /// Renames a dictionary (DIC-033).
    pub fn dict_rename(&mut self, id: &str, name: &str) {
        let effects = self
            .dict_page
            .rename(self.services.dictionaries.as_ref(), id, name);
        self.dict_effects(effects);
    }

    /// Sets a dictionary's block state (DIC-034).
    pub fn dict_set_collapse(&mut self, id: &str, state: &str) {
        let effects = self
            .dict_page
            .set_collapse(self.services.dictionaries.as_ref(), id, state);
        self.dict_effects(effects);
    }

    /// Saves the user's CSS of a dictionary (DIC-035).
    pub fn dict_set_user_css(&mut self, id: &str, css: &str) {
        let effects = self
            .dict_page
            .set_user_css(self.services.dictionaries.as_ref(), id, css);
        self.dict_effects(effects);
    }

    /// A lookup answered.
    pub fn lookup_done(&mut self, generation: u64, result: Result<Reply, String>) {
        let reply = match result {
            Ok(reply) => reply,
            Err(message) => {
                self.toasts.push(message);
                Reply {
                    empty: true,
                    ..Reply::default()
                }
            }
        };
        let entries: Vec<(String, String)> = reply
            .content
            .entries
            .iter()
            .map(|e| (e.expression.clone(), e.reading.clone()))
            .collect();
        if self.popups.deliver(generation, reply) {
            self.sync_highlight();
            self.apply_last_card();
            if !entries.is_empty() {
                self.anki_jobs.push(crate::mining::AnkiJob::Check {
                    generation,
                    request: crate::mining::CheckRequest { entries },
                });
            }
        }
    }

    /// A click on the time of a list row: playback goes to the cue.
    pub fn seek_to_cue(&self, cue: usize) {
        if cue < self.subs.timeline().cues().len() {
            let ms = self.subs.timeline().start_pos(cue);
            let _ = self.services.player.seek_absolute(ms as f64 / 1000.0);
        }
    }

    /// The obscuring that applies now (SUB-014).
    pub fn obscure_now(&self) -> Obscure {
        crate::subtitles::obscured(
            self.subs.mode,
            self.view.loaded && !self.view.paused,
            self.popups.is_open(),
            self.subs.hovered,
        )
    }

    /// The configured subtitle font size (1080p reference) and whether ASS
    /// styles are respected.
    pub fn subtitle_style(&self) -> (f64, bool) {
        let c = &self.services.config.subtitles;
        (c.font_size, c.respect_ass_styles)
    }
}

/// A short name for a subtitle source.
fn source_name(source: &Source) -> String {
    match source {
        Source::File(p) => p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        Source::Embedded { id, .. } | Source::Bitmap(id) => format!("#{id}"),
    }
}

/// The date of a day counted from 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_become_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(20_386), (2025, 10, 25));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            url_to_path("file:///v/My%20Show/ep%2001.mkv"),
            PathBuf::from("/v/My Show/ep 01.mkv")
        );
        assert_eq!(
            url_to_path("file:///v/%E6%97%A5%E6%9C%AC.mkv"),
            PathBuf::from("/v/日本.mkv")
        );
        assert_eq!(
            url_to_path("/plain/path.mkv"),
            PathBuf::from("/plain/path.mkv")
        );
        assert_eq!(
            url_to_path("file:///bad%zz.mkv"),
            PathBuf::from("/bad%zz.mkv")
        );
        // a `%` and one hex digit before a multibyte character (ERR-002)
        assert_eq!(
            url_to_path("file:///x%a日.mkv"),
            PathBuf::from("/x%a日.mkv")
        );
    }
}
