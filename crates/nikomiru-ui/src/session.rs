// SPDX-License-Identifier: GPL-3.0-only

//! The playback session behind the player page, apart from its drawing: it opens files,
//! filters and reduces player events into a [`ViewState`], saves the position
//! (LIB-006) and turns shortcut actions into player commands.

use std::path::{Path, PathBuf};

use nikomiru_core::{Clock, Config};
use nikomiru_library::progress::ProgressSaver;
use nikomiru_library::{Library, SubtitleChoice, VideoRecord, Writer};
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
    /// Open the settings on the tab shown last (SET-008).
    OpenSettings,
}

/// Milestones of a load, for the GUI test harness (TEST §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// The file is open.
    FileLoaded,
    /// The next episode's countdown started (LIB-009).
    NextUp,
    /// A word was played, or none was found (AUD-004).
    WordAudio {
        /// An audio was found.
        found: bool,
    },
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
    /// The chapters of the file, in order (PLY-019).
    pub chapters: Vec<nikomiru_player::player::Chapter>,
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
            chapters: Vec::new(),
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
    /// Media servers (SPECIFICATION §6.13).
    pub servers: Box<dyn crate::servers::ServerService>,
    /// Backup and sync (SPECIFICATION §6.14).
    pub sync: Box<dyn crate::sync::SyncService>,
    /// The library worker (LIB-002…LIB-012).
    pub catalog: Box<dyn crate::catalog::LibraryService>,
    /// The word audio worker (AUD-001…AUD-008).
    pub word_audio: Box<dyn crate::word_audio::WordAudioService>,
    /// The thumbnails of the progress bar (PLY-014).
    pub thumbnails: Box<dyn crate::thumbnails::ThumbnailService>,
    /// Where downloaded subtitles are kept (MET-008).
    pub subtitles_dir: PathBuf,
    /// Where the mpv presets are kept (PLY-016).
    pub mpv_presets_dir: PathBuf,
    /// The user's own `mpv.conf` (`$XDG_CONFIG_HOME/mpv/mpv.conf`), offered
    /// when Nikomiru sets no mpv option yet (PLY-016).
    pub user_mpv_conf: PathBuf,
    /// `config.toml`, where the settings pages save.
    pub config_file: PathBuf,
    /// Where the media of a card are made (`$XDG_CACHE_HOME/nikomiru/mining`):
    /// the frame of the click is saved there.
    pub mining_dir: PathBuf,
    /// The settings came from `config.toml` (or there is none yet): saving may
    /// write it. When it could not be read, the settings in memory are the
    /// defaults, and writing them would lose the user's file.
    pub config_writable: bool,
    /// The logs' folder (Settings → Advanced).
    pub logs_dir: PathBuf,
    /// `[lookup] max_results`, shared with the workers that look words up:
    /// the settings change it at once (SET-002).
    pub lookup_limit: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// How the interface looks (SET-003, SET-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// The dark theme, or the light.
    pub dark: bool,
    /// The accent.
    pub accent: [u8; 3],
    /// The interface scale in percent.
    pub scale: u16,
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
    /// The file; for a media server's video, the relay's address (SRV-009).
    path: PathBuf,
    /// The folder of its downloaded subtitles (MET-008, SRV-010).
    subtitles: Option<PathBuf>,
    /// What a media server says of it.
    server: Option<ServerPlaying>,
}

/// A media server's video playing (SRV-011).
struct ServerPlaying {
    video: crate::servers::ServerVideo,
    /// When the server was last told.
    reported: std::time::Duration,
    /// It was marked watched.
    played: bool,
}

/// How often the server hears of a video playing (SRV-011).
const SERVER_REPORT: std::time::Duration = std::time::Duration::from_secs(10);

/// The settings page whose settings are being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SavingPage {
    #[default]
    Anki,
    Services,
    Player,
    Audio,
    /// General, Subtitles, Lookup, Shortcuts, Advanced.
    Settings,
}

/// How long the next episode waits (LIB-009).
const NEXT_COUNTDOWN: std::time::Duration = std::time::Duration::from_secs(5);
/// The cues split into words ahead of the one playing (LKP-060).
const AHEAD: usize = 3;

/// The next episode about to start (LIB-009).
#[derive(Debug, Clone)]
struct NextUp {
    path: PathBuf,
    title: String,
    /// When it starts (the monotonic clock).
    at: std::time::Duration,
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
    /// The secondary subtitle (SUB-017): a second text track, drawn without
    /// clicks, with its own obscuring mode, following the main offset.
    pub secondary: Subtitles,
    /// Its loads so far: an older answer is dropped.
    secondary_generation: u64,
    sub_generation: u64,
    sub_loads: Vec<(u64, Request)>,
    sub_auto_done: bool,
    /// Changes whenever the cue list changes, so the UI rebuilds it.
    pub sub_list_version: u64,
    /// The dictionary popups.
    pub popups: Popups,
    lookups: Vec<(u64, String)>,
    /// The words of the subtitles (LKP-060, LKP-051, LKP-061).
    words: crate::words::Words,
    /// The thumbnails of the progress bar (PLY-014).
    thumbnails: crate::thumbnails::Thumbnails,
    /// A dictionary is installed, and a card was made: two of the first
    /// steps (LIB-013).
    dictionaries_installed: bool,
    cards_made: bool,
    screenshots: Vec<String>,
    anki_jobs: Vec<crate::mining::AnkiJob>,
    /// The last card made since Nikomiru started (ANK-030).
    last_card: Option<crate::mining::LastCard>,
    /// A link whose video cannot be found: the view offers to locate it
    /// (ANK-052).
    pub missing_link: Option<MissingLink>,
    /// A page or a dialog covers the video and the popups (the view says
    /// so): the popups wait under it, and the keys go to its controls but
    /// the global shortcuts' (POP-014, NFR-A11Y-01).
    pub popups_covered: bool,
    /// A lookup asked from the keyboard (NFR-A11Y-01): the cue and the byte
    /// offset of its grapheme; the overlay, which knows where the cue is,
    /// opens it as a click would.
    keyboard_lookup: Option<(usize, usize)>,
    /// The clip of the link being opened: once its subtitles are there,
    /// playback goes to its first cue (ANK-052).
    link_clip: Option<LinkClip>,
    /// Where the next file opens, instead of where it was left.
    start_at: Option<f64>,
    /// Why the next file opens: chosen, or the next or previous episode,
    /// which start from the beginning (LIB-007).
    opening: nikomiru_library::policy::Opening,
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
    server_jobs: Vec<crate::servers::ServerJob>,
    /// The name of the picture of the screen to take (PLY-015).
    capture: Option<String>,
    sync_jobs: Vec<crate::sync::SyncJob>,
    /// Settings → Backup.
    pub backup_page: crate::sync::BackupPage,
    /// Pages to open in the browser.
    external_links: Vec<String>,
    /// The server's video being made ready (SRV-008).
    server_pending: Option<String>,
    /// That video, ready, for `open` to take.
    server_ready: Option<crate::servers::ServerVideo>,
    /// The tabs of the servers and their settings page.
    pub servers_page: crate::servers::ServersPage,
    /// The Online subtitles page (MET-009).
    pub online_page: crate::online::OnlinePage,
    /// The last message of the Subtitle services tab, and whether it is good
    /// news (MET-011).
    pub services_status: Option<(String, bool)>,
    /// The last message of the General, Subtitles, Lookup, Shortcuts and
    /// Advanced tabs, and whether it is good news.
    pub settings_status: Option<(String, bool)>,
    /// The desktop prefers a dark theme, a light one, or says nothing
    /// (SET-003).
    pub system_dark: Option<bool>,
    /// The shortcut editor waits for a key (KEY-001): the next chord goes to
    /// it instead of running an action.
    capturing: bool,
    /// The chord it got; `None` once taken, or when Esc gave up.
    captured: Option<String>,
    /// The Library page (LIB-005).
    pub library_page: crate::catalog::LibraryPage,
    /// The last message of the Word audio tab, and whether it is good news.
    pub audio_status: Option<(String, bool)>,
    /// The row of the Word audio tab a database is being chosen for.
    audio_database_row: Option<usize>,
    /// A database chosen for the tab (AUD-003): a number, its row, its path.
    pub audio_database: Option<(u64, usize, String)>,
    /// The next episode about to start (LIB-009).
    next_up: Option<NextUp>,
    /// What watching adds to the statistics (STA-001…STA-004).
    study: crate::study::Study,
    /// What the current video plays with: its own choices, else its
    /// series' (LIB-010).
    prefs: nikomiru_library::policy::Effective,
    /// The video has its own audio track: the series' language does not
    /// choose one.
    own_audio: bool,
    /// The library jobs to start.
    library_jobs: Vec<crate::catalog::LibraryJob>,
    /// The word audio jobs to start.
    word_audio_jobs: Vec<crate::word_audio::WordAudioJob>,
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
    /// Series identified whose cover is being downloaded (MET-001).
    pending_identity:
        std::collections::HashMap<u64, nikomiru_library::series_info::SeriesInfoRecord>,
    /// The "Advanced mining" dialog (ANK-040).
    advanced: Option<crate::mining::AdvancedCard>,
    /// The word's place in its cue, for the merged sentences (ANK-041).
    advanced_word: Option<usize>,
    /// The frame of the click it keeps.
    advanced_frame: Option<crate::mining::PendingFrame>,
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

/// What a server says of a video, in its record (SRV-007, SRV-011).
fn apply_server_item(record: &mut VideoRecord, item: &nikomiru_online::servers::Item) {
    record.last_position_ms = nikomiru_online::servers::later_position(
        record.last_position_ms,
        record.last_played_at.map(|ms| ms / 1000),
        item.position_ms,
        item.last_played_s,
    );
    if item.duration_ms > 0 {
        record.duration_ms = Some(item.duration_ms);
    }
    record.season = item.season;
    record.episode = item.episode;
}

fn server_item_record(
    item: &nikomiru_online::servers::Item,
) -> nikomiru_library::server_items::ServerItemRecord {
    nikomiru_library::server_items::ServerItemRecord {
        series: item.series.clone().unwrap_or_default(),
        title: item.title.clone(),
        anilist_id: item.anilist_id,
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
        let secondary_obscure = services.config.subtitles.secondary_obscure.clone();
        let popup_size = (
            services.config.popup.max_width,
            services.config.popup.max_height,
        );
        let writer = services
            .library
            .as_ref()
            .filter(|l| !l.read_only())
            .map(|l| Writer::start(std::sync::Arc::clone(l)));
        let dictionaries_installed = services.dictionaries.list().is_ok_and(|l| !l.is_empty());
        let cards_made = services
            .library
            .as_ref()
            .and_then(|l| l.mined_card_count().ok())
            .is_some_and(|n| n > 0);
        let mut words = crate::words::Words::default();
        if let Some(library) = &services.library {
            match library.mined_expressions() {
                Ok(mined) => words.set_mined(mined),
                Err(e) => tracing::warn!("reading the cards made: {e}"),
            }
        }
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
            secondary: Subtitles::with_mode(Obscure::from_config(&secondary_obscure)),
            secondary_generation: 0,
            sub_sources: Vec::new(),
            sub_tracks: Vec::new(),
            sub_generation: 0,
            sub_loads: Vec::new(),
            sub_auto_done: false,
            sub_list_version: 0,
            popups: Popups::new(popup_size),
            lookups: Vec::new(),
            words,
            thumbnails: crate::thumbnails::Thumbnails::default(),
            dictionaries_installed,
            cards_made,
            screenshots: Vec::new(),
            anki_jobs: Vec::new(),
            last_card: None,
            missing_link: None,
            popups_covered: false,
            keyboard_lookup: None,
            link_clip: None,
            start_at: None,
            opening: nikomiru_library::policy::Opening::Chosen,
            anki_page: crate::mining::AnkiPage::default(),
            config_save: None,
            config_saving: false,
            lapis_request: None,
            keep_status: false,
            saving_page: SavingPage::default(),
            online_jobs: Vec::new(),
            server_jobs: Vec::new(),
            capture: None,
            sync_jobs: Vec::new(),
            backup_page: crate::sync::BackupPage::default(),
            external_links: Vec::new(),
            server_pending: None,
            server_ready: None,
            servers_page: crate::servers::ServersPage::default(),
            online_page: crate::online::OnlinePage::default(),
            services_status: None,
            settings_status: None,
            system_dark: None,
            capturing: false,
            captured: None,
            library_page: crate::catalog::LibraryPage::default(),
            audio_status: None,
            audio_database_row: None,
            audio_database: None,
            next_up: None,
            study: crate::study::Study::default(),
            prefs: Default::default(),
            own_audio: false,
            library_jobs: Vec::new(),
            word_audio_jobs: Vec::new(),
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
            pending_identity: std::collections::HashMap::new(),
            advanced: None,
            advanced_word: None,
            advanced_frame: None,
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
        // a server's video needs its relay first (SRV-008): `server_ready`
        // comes back here
        let server = self.server_ready.take();
        let text = path.to_string_lossy();
        if server.is_none() && nikomiru_library::server_items::parse_server_uid(&text).is_some() {
            let uid = text.into_owned();
            self.server_pending = Some(uid.clone());
            self.server_jobs
                .push(crate::servers::ServerJob::Prepare(uid));
            return;
        }
        let closing_server = self.report_stopped();
        if closing_server && server.is_none() {
            self.services.servers.close();
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
        let path = match &server {
            Some(video) => PathBuf::from(&video.url),
            None => nikomiru_library::scan::identity(path),
        };
        let meta = std::fs::metadata(&path).ok();
        let library = self.services.library.as_ref().filter(|l| !l.read_only());
        let mut record = VideoRecord::default();
        let uid = library.and_then(|lib| {
            let uid = match &server {
                Some(video) => video.uid.clone(),
                None => lib
                    .uid_for_path(&path)
                    .inspect_err(|e| tracing::warn!("library: {e}"))
                    .ok()?,
            };
            record = lib
                .video(&uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()
                .flatten()
                .unwrap_or_else(|| VideoRecord {
                    path: match &server {
                        Some(video) => video.uid.clone(),
                        None => path.to_string_lossy().into_owned(),
                    },
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
        if let Some(video) = &server {
            apply_server_item(&mut record, &video.item);
        }
        // STA-001: the watched intervals of the video, and what the last one
        // leaves to save
        let coverage = match (&uid, library) {
            (Some(uid), Some(lib)) => lib
                .coverage(uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .unwrap_or_default(),
            _ => Default::default(),
        };
        if let Some(save) = self.study.start(uid.clone(), coverage) {
            self.write_study(save);
        }
        // LIB-010
        self.prefs = match (&uid, library) {
            (Some(uid), Some(lib)) => lib
                .effective(uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .unwrap_or_default(),
            _ => Default::default(),
        };
        self.own_audio = record.aid.is_some();
        if let (Some(uid), Some(w)) = (&uid, &self.writer) {
            let (uid, record) = (uid.clone(), record.clone());
            let item = server.as_ref().map(|video| server_item_record(&video.item));
            w.run(move |lib| {
                if let Err(e) = lib.put_video(&uid, &record) {
                    tracing::warn!("library: {e}");
                }
                if let Some(item) = item
                    && let Err(e) = lib.set_server_item(&uid, &item)
                {
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
        let opening =
            std::mem::replace(&mut self.opening, nikomiru_library::policy::Opening::Chosen);
        let request = LoadRequest {
            // LIB-007: from the beginning near the end
            start: self.start_at.take().or_else(|| {
                let ms = nikomiru_library::policy::start_ms(&record, opening);
                (ms > 0).then(|| ms as f64 / 1000.0)
            }),
            options,
            aid: record.aid,
            volume: record.volume,
            speed: record.speed,
        };
        self.saver.reset();
        self.next_up = None;
        self.first_frame = false;
        self.reset_subtitles();
        if let Some(delay) = self.prefs.delay_ms {
            self.subs.timeline_mut().delay_ms = delay;
        }
        self.view.loaded = false;
        self.view.title = match &server {
            Some(video) => video.item.title.clone(),
            None => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
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
        let hash = match &server {
            Some(_) => None,
            None => nikomiru_online::hash::video_hash(&path)
                .inspect_err(|e| tracing::warn!("{}: {e}", path.display()))
                .ok(),
        };
        let subtitles = match &server {
            Some(video) => Some(nikomiru_online::store::server_folder(
                &self.services.subtitles_dir,
                &video.uid,
            )),
            None => hash
                .as_ref()
                .map(|h| nikomiru_online::store::folder(&self.services.subtitles_dir, h)),
        };
        self.online_generation += 1;
        self.auto_online = None;
        self.online_page.busy = false;
        // a stream has no thumbnails: ffmpeg would download it again; a
        // server's file has, its relay reading only what ffmpeg seeks to
        self.thumbnails
            .open((path.is_file() || server.is_some()).then(|| path.clone()));
        let now = self.services.clock.monotonic();
        self.current = Some(Current {
            uid,
            path,
            subtitles,
            server: server.map(|video| ServerPlaying {
                video,
                reported: now,
                played: false,
            }),
        });
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
                self.report_server(nikomiru_online::servers::PlayState::Started);
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
                self.update_secondary();
                if let Some(ms) = self.saver.position((t * 1000.0) as u64, now) {
                    self.save_position(ms);
                }
                if !self.view.paused && self.study.due_sample(now.as_millis() as i64) {
                    self.study_sample(true);
                }
            }
            PlayerEvent::Duration(d) => self.view.duration = d,
            PlayerEvent::Pause(p) => {
                if self.current.as_ref().is_some_and(|c| c.server.is_some()) {
                    use nikomiru_online::servers::PlayState;
                    self.report_server(if p {
                        PlayState::Paused
                    } else {
                        PlayState::Playing
                    });
                }
                // STA-001: what played up to the pause counts, the pause does not
                self.study_sample(!p);
                self.view.paused = p;
                self.controls.set_playing(!p, now);
            }
            PlayerEvent::Speed(s) => {
                self.study_sample(!self.view.paused);
                self.view.speed = s;
            }
            PlayerEvent::Volume(v) => self.view.volume = v,
            PlayerEvent::Mute(m) => self.view.muted = m,
            PlayerEvent::Tracks(t) => {
                self.sub_tracks = t.iter().filter(|t| t.kind == "sub").cloned().collect();
                self.view.audio_tracks = t.into_iter().filter(|t| t.kind == "audio").collect();
                // LIB-010: the series' audio language, once
                if !std::mem::replace(&mut self.own_audio, true)
                    && let Some(lang) = self.prefs.audio_language.clone()
                    && let Some(track) = self.view.audio_tracks.iter().find(|t| {
                        t.lang
                            .as_deref()
                            .is_some_and(|l| select::same_language(&lang, l))
                    })
                {
                    let _ = self.services.player.set_aid(Some(track.id));
                }
                self.discover_subtitles();
            }
            PlayerEvent::VideoSize(w, h) => self.view.video_size = (w, h),
            PlayerEvent::Aid(a) => self.view.aid = a,
            PlayerEvent::Chapters(c) => self.view.chapters = c,
            // LIB-009: the episode ended (`keep-open` holds its last frame)
            PlayerEvent::EofReached(true) => self.episode_ended(now),
            PlayerEvent::EofReached(false) => self.cancel_next(),
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
        // STA-004: the session saved every minute
        if let Some(save) = self.study.due_save(now.as_millis() as i64) {
            self.write_study(save);
        }
        if self.next_up.as_ref().is_some_and(|n| now >= n.at) {
            self.play_next();
        }
        self.tick_server(now);
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
            Some(path)
                if path.is_file()
                    || nikomiru_library::server_items::parse_server_uid(
                        &path.to_string_lossy(),
                    )
                    .is_some() =>
            {
                self.open_at_link(&path, &link);
            }
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
            let path = nikomiru_library::scan::identity(path);
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
            SavingPage::Services => self.services_status = Some(status),
            SavingPage::Audio => self.audio_status = Some(status),
            SavingPage::Player => self.player_page.status = Some(status),
            SavingPage::Settings => self.settings_status = Some(status),
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
            query: self.online_query(video.to_path_buf()),
        });
    }

    /// The AniList id of the series of the video playing, once identified
    /// (MET-001): Jimaku is asked by it first (MET-003).
    fn anilist_id(&self) -> Option<u64> {
        if let Some(playing) = self.current.as_ref()?.server.as_ref() {
            return playing.video.item.anilist_id;
        }
        let uid = self.current.as_ref()?.uid.as_ref()?;
        let library = self.services.library.as_ref()?;
        library.anilist_id_of_video(uid).ok().flatten()
    }

    /// The online query of `video`, from its file name and its series.
    fn online_query(&self, video: PathBuf) -> crate::online::OnlineQuery {
        let current = self.current.as_ref();
        // SRV-010: a server knows what its video is, and it has no hash
        if let Some(playing) = current.and_then(|c| c.server.as_ref()) {
            let item = &playing.video.item;
            return crate::online::OnlineQuery {
                video,
                title: item.series.clone().unwrap_or_else(|| item.title.clone()),
                season: item.season,
                episode: item.episode,
                anilist_id: item.anilist_id,
                folder: current.and_then(|c| c.subtitles.clone()),
            };
        }
        crate::online::OnlineQuery {
            anilist_id: self.anilist_id(),
            ..crate::online::OnlineQuery::of_video(video)
        }
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
            self.online_page.query = video.map(|v| self.online_query(v));
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
        let anilist_id = self.anilist_id();
        let folder = self
            .current
            .as_ref()
            .filter(|c| c.server.is_some())
            .and_then(|c| c.subtitles.clone());
        let query = crate::online::OnlineQuery {
            video,
            title,
            season,
            episode,
            anilist_id,
            folder,
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
        self.saving_page = SavingPage::Services;
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.services_status = Some((text, false));
        }
    }

    /// The Subtitle services tab of the settings opened (MET-011).
    pub fn services_open(&mut self) {
        self.services_status = None;
    }

    /// A tab of the settings opened: General, Subtitles, Lookup, Shortcuts,
    /// Advanced or About.
    pub fn settings_open(&mut self) {
        self.settings_status = None;
        self.shortcut_capture(false);
    }

    /// `config.toml`.
    pub fn config_file(&self) -> &Path {
        &self.services.config_file
    }

    /// The logs' folder.
    pub fn logs_dir(&self) -> &Path {
        &self.services.logs_dir
    }

    /// The shortcut editor starts or stops waiting for a key (KEY-001).
    pub fn shortcut_capture(&mut self, on: bool) {
        self.capturing = on;
        self.captured = None;
    }

    /// Whether the shortcut editor waits for a key, and the chord it got.
    pub fn capture(&self) -> (bool, Option<&str>) {
        (self.capturing, self.captured.as_deref())
    }

    /// The settings as they are, the draft of a tab.
    pub fn config(&self) -> &Config {
        &self.services.config
    }

    /// How the interface looks: the theme chosen, or the desktop's for
    /// `"system"` (dark when it says nothing: a video player); the accent;
    /// the scale.
    pub fn look(&self) -> Look {
        let g = &self.services.config.general;
        let dark = match g.theme.as_str() {
            "light" => false,
            "dark" => true,
            _ => self.system_dark.unwrap_or(true),
        };
        Look {
            dark,
            accent: g.accent_rgb().unwrap_or([0x6a, 0xa0, 0xff]),
            scale: g.ui_scale,
        }
    }

    /// Saves a tab of the settings, the keys of `section` in `draft`: they
    /// apply at once (SET-002) and go to `config.toml` unless it could not
    /// be read. Shortcuts that conflict are refused (KEY-001).
    pub fn settings_save(&mut self, section: nikomiru_core::config::Section, draft: &Config) {
        use nikomiru_core::config::Section;
        if section == Section::Shortcuts {
            match Bindings::with_overrides(&draft.shortcuts) {
                Ok(bindings) => self.bindings = bindings,
                Err(conflicts) => {
                    let chords: Vec<String> = conflicts.into_iter().map(|c| c.chord).collect();
                    let text = self.tr("shortcuts-conflict", &[("keys", chords.join(", "))]);
                    self.settings_status = Some((text, false));
                    return;
                }
            }
        }
        self.services.config.copy_section(section, draft);
        let c = &self.services.config;
        match section {
            Section::General => self
                .services
                .translator
                .set_setting(&c.general.ui_language, |k| std::env::var(k).ok()),
            Section::Subtitles => {
                self.subs.mode = Obscure::from_config(&c.subtitles.obscure);
                self.secondary.mode = Obscure::from_config(&c.subtitles.secondary_obscure);
            }
            Section::Lookup => {
                self.services
                    .lookup_limit
                    .store(c.lookup.max_results, std::sync::atomic::Ordering::Relaxed);
                self.popups
                    .set_size((c.popup.max_width, c.popup.max_height));
            }
            _ => {}
        }
        self.keep_status = false;
        self.saving_page = SavingPage::Settings;
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("settings-not-saved", &[]);
            self.settings_status = Some((text, false));
        }
    }

    /// The Library page opened (LIB-005): the home page read again, and the
    /// series open.
    pub fn library_open(&mut self) {
        use crate::catalog::LibraryJob;
        self.library_page.status = None;
        // the video being watched is in "Continue watching" at its place,
        // and its session in the statistics
        if let Some(ms) = self.saver.flush() {
            self.save_position(ms);
        }
        if let Some(save) = self.study.save_now() {
            self.write_study(save);
        }
        if let Some(w) = &self.writer {
            w.wait();
        }
        self.library_jobs.push(LibraryJob::Home);
        self.library_jobs.push(LibraryJob::Stats);
        if let Some((id, _)) = self.library_page.series {
            self.library_jobs.push(LibraryJob::Episodes(id));
        }
    }

    /// "Identify online" on a series (MET-001, MET-002): its title asked of
    /// the metadata source of the settings.
    pub fn library_identify(&mut self, series: u64) {
        let title = self
            .library_page
            .home
            .as_ref()
            .and_then(|h| h.series.iter().find(|s| s.id == series))
            .map(|s| s.record.title.clone());
        let Some(title) = title else {
            return;
        };
        self.library_page.status = None;
        self.library_page.identify = Some(crate::catalog::Identify {
            series,
            source: None,
            choices: Vec::new(),
            busy: true,
        });
        self.online_jobs
            .push(crate::online::OnlineJob::Identify { series, title });
    }

    /// What the source found for a series: one with exactly its title is
    /// taken, otherwise the user chooses.
    pub fn identified(
        &mut self,
        series: u64,
        result: Result<
            (
                nikomiru_online::metadata::Source,
                nikomiru_online::metadata::Identified,
            ),
            String,
        >,
    ) {
        use nikomiru_online::metadata::Identified;
        if self
            .library_page
            .identify
            .as_ref()
            .is_none_or(|i| i.series != series)
        {
            return;
        }
        match result {
            Ok((source, Identified::Unique(info))) => self.take_identity(series, source, &info),
            Ok((source, Identified::Choose(choices))) => {
                self.library_page.identify = Some(crate::catalog::Identify {
                    series,
                    source: Some(source),
                    choices,
                    busy: false,
                });
            }
            Ok((source, Identified::Nothing)) => {
                self.library_page.identify = None;
                let text = self.tr("library-identify-none", &[("source", source.name().into())]);
                self.library_page.status = Some((text, false));
            }
            Err(e) => {
                self.library_page.identify = None;
                self.library_page.status = Some((e, false));
            }
        }
    }

    /// What was found online of a series forgotten.
    pub fn library_forget(&mut self, series: u64) {
        self.library_jobs.push(crate::catalog::LibraryJob::Edit(
            crate::catalog::LibraryEdit::Identify {
                id: series,
                info: None,
                cover: None,
            },
        ));
    }

    /// One of the results chosen by the user, or none (`None`).
    pub fn library_identify_choose(&mut self, index: Option<usize>) {
        let Some(identify) = self.library_page.identify.take() else {
            return;
        };
        if let (Some(i), Some(source)) = (index, identify.source)
            && let Some(info) = identify.choices.get(i)
        {
            let info = info.clone();
            self.take_identity(identify.series, source, &info);
        }
    }

    /// The series is `info`: its cover downloaded first, then both kept.
    fn take_identity(
        &mut self,
        series: u64,
        source: nikomiru_online::metadata::Source,
        info: &nikomiru_online::metadata::SeriesInfo,
    ) {
        let record = nikomiru_library::series_info::SeriesInfoRecord {
            source: source.key().into(),
            anilist_id: info.anilist_id,
            mal_id: info.mal_id,
            romaji: info.romaji.clone(),
            english: info.english.clone(),
            native: info.native.clone(),
            episodes: info.episodes,
            year: info.year,
            format: info.format.clone(),
            page_url: info.page_url.clone(),
            identified_at: self.now_ms(),
        };
        let text = self.tr("library-identified", &[("title", info.romaji.clone())]);
        self.library_page.status = Some((text, true));
        self.library_page.identify = Some(crate::catalog::Identify {
            series,
            source: Some(source),
            choices: Vec::new(),
            busy: true,
        });
        match &info.cover_url {
            Some(url) => {
                self.pending_identity.insert(series, record);
                self.online_jobs.push(crate::online::OnlineJob::Cover {
                    series,
                    url: url.clone(),
                });
            }
            None => self.keep_identity(series, record, None),
        }
    }

    /// The cover of an identified series came, or not: the series keeps what
    /// was found either way.
    pub fn identity_cover(&mut self, series: u64, cover: Result<Vec<u8>, String>) {
        let Some(record) = self.pending_identity.remove(&series) else {
            return;
        };
        let cover = cover
            .inspect_err(|e| tracing::info!("cover of series {series}: {e}"))
            .ok();
        self.keep_identity(series, record, cover);
    }

    fn keep_identity(
        &mut self,
        series: u64,
        record: nikomiru_library::series_info::SeriesInfoRecord,
        cover: Option<Vec<u8>>,
    ) {
        if cover.is_some() {
            // the new cover is read again
            self.library_page.covers.remove(&series);
            self.library_page.covers_asked.retain(|&id| id != series);
        }
        self.library_page.identify = None;
        self.library_jobs.push(crate::catalog::LibraryJob::Edit(
            crate::catalog::LibraryEdit::Identify {
                id: series,
                info: Some(Box::new(record)),
                cover,
            },
        ));
    }

    /// Unix ms now.
    fn now_ms(&self) -> i64 {
        self.services
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    /// A series opened on the page, or closed (`None`).
    pub fn library_series(&mut self, series: Option<u64>) {
        self.library_page.series = series.map(|id| (id, None));
        if let Some(id) = series {
            self.library_jobs
                .push(crate::catalog::LibraryJob::Episodes(id));
        }
    }

    /// A folder to import (LIB-002), or to scan again with a grouping.
    pub fn library_import(&mut self, root: PathBuf, grouping: nikomiru_library::Grouping) {
        self.library_page.busy = true;
        let text = self.tr("library-scanning", &[("path", root.display().to_string())]);
        self.library_page.status = Some((text, true));
        self.library_jobs
            .push(crate::catalog::LibraryJob::Import(root, grouping));
    }

    /// An edit of the library (LIB-011, LIB-012).
    pub fn library_edit(&mut self, edit: crate::catalog::LibraryEdit) {
        self.library_page.busy = true;
        self.library_jobs
            .push(crate::catalog::LibraryJob::Edit(edit));
    }

    /// The sessions as CSV, to `path` chosen in the save dialog (STA-005).
    pub fn stats_export(&mut self, path: PathBuf) {
        self.library_jobs
            .push(crate::catalog::LibraryJob::ExportCsv(path));
    }

    /// An episode played from the page.
    pub fn library_play(&mut self, path: &Path) {
        self.open(path);
    }

    /// What the library worker answered.
    pub fn library_reply(&mut self, reply: crate::catalog::LibraryReply) {
        use crate::catalog::{LibraryJob, LibraryReply};
        let page = &mut self.library_page;
        match reply {
            LibraryReply::Home(Ok(home)) => {
                for s in &home.series {
                    if !page.covers_asked.contains(&s.id) {
                        page.covers_asked.push(s.id);
                        self.library_jobs.push(LibraryJob::Cover(s.id));
                    }
                }
                page.home = Some(home);
            }
            LibraryReply::Episodes(id, Ok(detail)) => {
                if let Some((open, episodes)) = &mut page.series
                    && *open == id
                {
                    *episodes = Some(detail.episodes);
                    page.series_info = detail.info;
                }
            }
            LibraryReply::Imported(Ok(report)) => {
                page.busy = false;
                let text = self.tr(
                    "library-imported",
                    &[
                        ("added", report.added.to_string()),
                        ("series", report.series_added.to_string()),
                    ],
                );
                self.library_page.status = Some((text, true));
                self.library_open();
            }
            LibraryReply::Edited(Ok(())) => {
                page.busy = false;
                self.library_open();
            }
            LibraryReply::Cover(id, Ok(cover)) => {
                page.covers.insert(id, cover);
            }
            LibraryReply::Stats(Ok(stats)) => page.stats = Some(stats),
            LibraryReply::Exported(Ok(path)) => {
                let text = self.tr("stats-exported", &[("path", path.display().to_string())]);
                self.library_page.status = Some((text, true));
            }
            // a cover that cannot be made: the card keeps its placeholder
            LibraryReply::Cover(id, Err(e)) => tracing::debug!("cover of series {id}: {e}"),
            LibraryReply::Home(Err(e))
            | LibraryReply::Stats(Err(e))
            | LibraryReply::Exported(Err(e))
            | LibraryReply::Episodes(_, Err(e))
            | LibraryReply::Imported(Err(e))
            | LibraryReply::Edited(Err(e)) => {
                page.busy = false;
                page.status = Some((e, false));
            }
        }
    }

    /// The library jobs to start, taken by the UI layer.
    pub fn take_library_jobs(&mut self) -> Vec<crate::catalog::LibraryJob> {
        std::mem::take(&mut self.library_jobs)
    }

    /// The library worker.
    pub fn catalog(&self) -> &dyn crate::catalog::LibraryService {
        self.services.catalog.as_ref()
    }

    /// "Database…" on a row of the Word audio tab (AUD-003).
    pub fn audio_choose_database(&mut self, row: usize) {
        self.audio_database_row = Some(row);
    }

    /// The database chosen in the file dialog, for its row.
    pub fn audio_database_chosen(&mut self, path: &Path) {
        if let Some(row) = self.audio_database_row.take() {
            let n = self.audio_database.as_ref().map_or(1, |(n, _, _)| n + 1);
            self.audio_database = Some((n, row, path.to_string_lossy().into_owned()));
        }
    }

    /// `[word_audio]` as saved.
    pub fn word_audio_config(&self) -> &nikomiru_core::config::WordAudioConfig {
        &self.services.config.word_audio
    }

    /// Whether a word audio source is on: the ♪ of the entries shows.
    pub fn word_audio_on(&self) -> bool {
        self.services
            .config
            .word_audio
            .sources
            .iter()
            .any(|s| s.enabled)
    }

    fn play_word(&mut self, term: &str, reading: &str) {
        if self.word_audio_on() {
            self.word_audio_jobs
                .push(crate::word_audio::WordAudioJob::Play {
                    term: term.to_string(),
                    reading: reading.to_string(),
                });
        }
    }

    fn entry_word(&self, level: usize, entry: usize) -> Option<(u64, String, String)> {
        let p = self.popups.stack().get(level)?;
        let e = p.reply.as_ref()?.content.entries.get(entry)?;
        Some((p.generation, e.expression.clone(), e.reading.clone()))
    }

    /// ♪ of an entry (AUD-004): its first audio.
    pub fn word_audio(&mut self, level: usize, entry: usize) {
        if let Some((_, term, reading)) = self.entry_word(level, entry) {
            self.play_word(&term, &reading);
        }
    }

    /// The audio menu of an entry (AUD-004): every source asked; again, it
    /// closes.
    pub fn word_audio_menu(&mut self, level: usize, entry: usize) {
        use crate::word_audio::{AudioMenu, WordAudioJob};
        let Some((generation, term, reading)) = self.entry_word(level, entry) else {
            return;
        };
        let Some(p) = self.popups.stack_mut().get_mut(level) else {
            return;
        };
        if p.audio.as_ref().is_some_and(|(e, _)| *e == entry) {
            p.audio = None;
            return;
        }
        p.audio = Some((entry, AudioMenu::Loading));
        self.word_audio_jobs.push(WordAudioJob::List {
            level,
            generation,
            entry,
            term,
            reading,
        });
    }

    /// An audio of the menu chosen.
    pub fn word_audio_play(&mut self, url: String) {
        self.word_audio_jobs
            .push(crate::word_audio::WordAudioJob::PlayUrl(url));
    }

    /// What the sources gave for an entry's menu.
    pub fn word_audio_listed(
        &mut self,
        level: usize,
        generation: u64,
        entry: usize,
        result: Result<Vec<crate::word_audio::WordAudioItem>, String>,
    ) {
        use crate::word_audio::AudioMenu;
        let items = result.unwrap_or_else(|e| {
            self.toasts.push(e);
            Vec::new()
        });
        if let Some(p) = self.popups.stack_mut().get_mut(level)
            && p.generation == generation
            && p.audio.as_ref().is_some_and(|(e, _)| *e == entry)
        {
            p.audio = Some((entry, AudioMenu::Items(items)));
        }
    }

    /// A word played, or not.
    pub fn word_audio_played(&mut self, result: Result<bool, String>) {
        self.events.push(UiEvent::WordAudio {
            found: result == Ok(true),
        });
        match result {
            Ok(true) => {}
            Ok(false) => self.toasts.push(self.tr("word-audio-none", &[])),
            Err(e) => self.toasts.push(e),
        }
    }

    /// The word audio jobs to start, taken by the UI layer.
    pub fn take_word_audio_jobs(&mut self) -> Vec<crate::word_audio::WordAudioJob> {
        std::mem::take(&mut self.word_audio_jobs)
    }

    /// The word audio worker.
    pub fn word_audio_service(&self) -> &dyn crate::word_audio::WordAudioService {
        self.services.word_audio.as_ref()
    }

    /// Saves the word audio settings (AUD-001): `config.toml`, and the
    /// worker.
    pub fn word_audio_save(&mut self, config: nikomiru_core::config::WordAudioConfig) {
        self.services.config.word_audio = config.clone();
        self.word_audio_jobs
            .push(crate::word_audio::WordAudioJob::Configure(Box::new((
                config,
                self.services.config.network.proxy.clone(),
            ))));
        self.saving_page = SavingPage::Audio;
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.audio_status = Some((text, false));
        }
    }

    /// LIB-009: the episode ended; with `auto_next`, the next one of its
    /// series starts in 5 s, unless cancelled.
    fn episode_ended(&mut self, now: std::time::Duration) {
        if !self.services.config.player.auto_next || self.next_up.is_some() {
            return;
        }
        // SRV-008: on a server, what comes next is its own
        if let Some(playing) = self.current.as_ref().and_then(|c| c.server.as_ref()) {
            if let Some((uid, title)) = playing.video.next.clone() {
                self.next_up = Some(NextUp {
                    path: PathBuf::from(uid),
                    title,
                    at: now + NEXT_COUNTDOWN,
                });
                self.events.push(UiEvent::NextUp);
            }
            return;
        }
        let uid = self.current.as_ref().and_then(|c| c.uid.clone());
        let next = uid.and_then(|uid| {
            self.services
                .library
                .as_ref()?
                .next_episode(&uid)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()
                .flatten()
        });
        let Some((_, record)) = next else {
            return;
        };
        let path = PathBuf::from(record.path);
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.next_up = Some(NextUp {
            path,
            title,
            at: now + NEXT_COUNTDOWN,
        });
        self.events.push(UiEvent::NextUp);
    }

    /// The episode coming next (LIB-009), and the seconds left before it.
    pub fn next_up(&self) -> Option<(&str, u64)> {
        let now = self.services.clock.monotonic();
        self.next_up.as_ref().map(|n| {
            (
                n.title.as_str(),
                n.at.saturating_sub(now).as_secs_f64().ceil() as u64,
            )
        })
    }

    /// The countdown stopped: Esc, "Cancel", or playback went on.
    pub fn cancel_next(&mut self) {
        self.next_up = None;
    }

    /// The next episode now, from its beginning (LIB-007).
    pub fn play_next(&mut self) {
        if let Some(next) = self.next_up.take() {
            self.opening = nikomiru_library::policy::Opening::FromStart;
            self.open(&next.path);
            // the end paused the last one (`keep-open`): this one plays
            if let Err(e) = self.services.player.set_pause(false) {
                tracing::warn!("next episode: {e}");
            }
        }
    }

    /// How many files were opened so far: the Library page closes when one
    /// opens, from wherever.
    pub fn opened(&self) -> u64 {
        self.online_generation
    }

    /// The Player settings page opens: the saved presets, and the option set
    /// in use, which the page shows. While Nikomiru sets no mpv option, the
    /// user's own `mpv.conf` is offered in the preview (PLY-016).
    pub fn player_open(&mut self) {
        self.player_page.presets = self.presets();
        self.player_page.preview = None;
        self.player_page.status = None;
        let unset =
            self.player_set().to_mpv_conf("").trim().is_empty() && self.player_preset().is_empty();
        let path = self.services.user_mpv_conf.clone();
        if unset
            && path.is_file()
            && let Ok(text) = self.read_conf(&path)
        {
            self.preview(path.display().to_string(), None, &text);
            let text = self.tr("player-user-conf", &[]);
            self.player_page.status = Some((text, true));
        }
    }

    /// Where the import dialog starts: the folder of the user's `mpv.conf`.
    pub fn mpv_conf_folder(&self) -> Option<PathBuf> {
        self.services
            .user_mpv_conf
            .parent()
            .filter(|d| d.is_dir())
            .map(Path::to_path_buf)
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

    /// The server's worker.
    pub fn servers(&self) -> &dyn crate::servers::ServerService {
        self.services.servers.as_ref()
    }

    /// The jobs for the server's worker.
    pub fn take_server_jobs(&mut self) -> Vec<crate::servers::ServerJob> {
        std::mem::take(&mut self.server_jobs)
    }

    /// A server's video is ready, or could not be made so (SRV-012, SRV-013).
    pub fn server_ready(&mut self, uid: &str, result: Result<crate::servers::ServerVideo, String>) {
        if self.server_pending.as_deref() != Some(uid) {
            return;
        }
        self.server_pending = None;
        match result {
            Ok(video) => {
                self.server_ready = Some(video);
                self.open(Path::new(uid));
            }
            Err(message) => {
                self.start_at = None;
                self.link_clip = None;
                self.toasts.push(message);
            }
        }
    }

    /// The worker of backups and sync.
    pub fn sync(&self) -> &dyn crate::sync::SyncService {
        self.services.sync.as_ref()
    }

    /// The jobs for that worker.
    pub fn take_sync_jobs(&mut self) -> Vec<crate::sync::SyncJob> {
        std::mem::take(&mut self.sync_jobs)
    }

    /// The WebDAV folder as saved.
    pub fn sync_config(&self) -> &nikomiru_core::config::SyncConfig {
        &self.services.config.sync
    }

    /// The Backup tab opened.
    pub fn backup_open(&mut self) {
        self.backup_page.status = None;
        self.backup_page.chosen = None;
    }

    /// New settings of the WebDAV folder (BAK-004).
    pub fn sync_save(&mut self, sync: nikomiru_core::config::SyncConfig) {
        self.server_device();
        self.services.config.sync = sync.clone();
        let network = &self.services.config.network;
        self.services
            .sync
            .configure(sync, network.proxy.clone(), network.device_id.clone());
        self.servers_changed();
        let text = self.tr("sync-saved", &[]);
        self.backup_page.status = Some((text, true));
    }

    /// "Check".
    pub fn sync_check(&mut self) {
        self.backup_page.busy = true;
        self.backup_page.status = None;
        self.sync_jobs.push(crate::sync::SyncJob::Check);
    }

    /// "Sync now".
    pub fn sync_now(&mut self) {
        self.flush_position();
        self.backup_page.busy = true;
        self.backup_page.status = None;
        self.sync_jobs.push(crate::sync::SyncJob::Now);
    }

    /// The file for a backup was chosen.
    pub fn backup_to(&mut self, path: PathBuf) {
        self.flush_position();
        self.backup_page.busy = true;
        self.backup_page.status = None;
        let config = Box::new(self.services.config.clone());
        let options = self.backup_page.options;
        self.sync_jobs
            .push(crate::sync::SyncJob::Backup(path, config, options));
    }

    /// A backup to restore was chosen: its contents, to confirm.
    pub fn backup_chosen(&mut self, path: PathBuf) {
        self.backup_page.busy = true;
        self.backup_page.status = None;
        self.sync_jobs.push(crate::sync::SyncJob::Inspect(path));
    }

    /// The restore was confirmed.
    pub fn backup_restore(&mut self) {
        if let Some((path, _)) = self.backup_page.chosen.take() {
            self.backup_page.busy = true;
            let config = Box::new(self.services.config.clone());
            self.sync_jobs
                .push(crate::sync::SyncJob::Restore(path, config));
        }
    }

    /// An answer for the Backup page.
    pub fn sync_answer(&mut self, answer: crate::sync::SyncAnswer) {
        use crate::sync::SyncAnswer;
        self.backup_page.busy = false;
        let status = match answer {
            SyncAnswer::Checked(r) => r.map(|()| self.tr("sync-checked", &[])),
            SyncAnswer::Synced(r) => r.map(|done| {
                let devices = done.devices.to_string();
                let skipped = done.skipped.to_string();
                self.tr("sync-done", &[("devices", devices), ("skipped", skipped)])
            }),
            SyncAnswer::BackedUp(r) => r.map(|_| self.tr("backup-done", &[])),
            SyncAnswer::Restored(r) => r.map(|()| self.tr("backup-restored", &[])),
            SyncAnswer::Inspected(path, Ok(manifest)) => {
                self.backup_page.chosen = Some((path, manifest));
                return;
            }
            SyncAnswer::Inspected(_, Err(e)) => Err(e),
        };
        self.backup_page.status = Some(match status {
            Ok(text) => (text, true),
            Err(text) => (text, false),
        });
    }

    /// The pages to open in the browser.
    pub fn take_external_links(&mut self) -> Vec<String> {
        std::mem::take(&mut self.external_links)
    }

    /// The media servers added.
    pub fn server_list(&self) -> &[nikomiru_core::config::ServerConfig] {
        &self.services.config.servers
    }

    /// The Media servers tab opened.
    pub fn servers_open(&mut self) {
        self.servers_page.status = None;
    }

    fn server_device(&mut self) {
        let network = &mut self.services.config.network;
        if network.device_id.is_empty() {
            network.device_id = uuid::Uuid::new_v4().simple().to_string();
        }
    }

    /// Saves the servers and tells the worker.
    fn servers_changed(&mut self) {
        let config = &self.services.config;
        self.services.servers.configure(
            config.servers.clone(),
            config.network.proxy.clone(),
            config.network.device_id.clone(),
        );
        if self.services.config_writable {
            self.config_save = Some(self.services.config.clone());
        } else {
            let text = self.tr("anki-config-not-saved", &[]);
            self.servers_page.status = Some((text, false));
        }
    }

    fn add_servers(&mut self, found: crate::servers::Found) {
        let count = found.len();
        self.services.config.servers.extend(found);
        self.servers_changed();
        let text = self.tr("servers-added-count", &[("count", count.to_string())]);
        self.servers_page.status = Some((text, true));
    }

    /// A Jellyfin login from the page.
    pub fn server_jellyfin(&mut self, url: String, user: String, password: String, allow: bool) {
        self.server_device();
        self.servers_changed();
        self.servers_page.logging_in = true;
        self.servers_page.status = None;
        self.server_jobs.push(crate::servers::ServerJob::Jellyfin {
            url,
            user,
            password,
            allow_unencrypted: allow,
        });
    }

    /// A Plex sign-in from the page.
    pub fn server_plex(&mut self) {
        self.server_device();
        self.servers_changed();
        self.servers_page.logging_in = true;
        self.servers_page.status = None;
        self.server_jobs.push(crate::servers::ServerJob::Plex);
    }

    /// Stops the Plex sign-in.
    pub fn server_cancel(&mut self) {
        self.services.servers.plex_cancel();
    }

    /// A new name for a server.
    pub fn server_rename(&mut self, id: &str, name: String) {
        if let Some(server) = self.services.config.servers.iter_mut().find(|s| s.id == id) {
            server.name = name;
            self.servers_changed();
        }
    }

    /// Removes a server: what the library knows of its videos stays (SRV-001).
    pub fn server_remove(&mut self, id: &str) {
        let servers = &mut self.services.config.servers;
        let Some(at) = servers.iter().position(|s| s.id == id) else {
            return;
        };
        let server = servers.remove(at);
        self.services.servers.logout(server);
        if self.servers_page.server.as_deref() == Some(id) {
            self.servers_page = crate::servers::ServersPage::default();
        }
        self.servers_changed();
    }

    /// The libraries shown of a server (SRV-006).
    pub fn server_libraries(&mut self, id: &str, list: Vec<String>) {
        if let Some(server) = self.services.config.servers.iter_mut().find(|s| s.id == id) {
            server.libraries = list;
            self.servers_changed();
        }
    }

    /// A server's tab opened: its libraries are asked for (SRV-004).
    pub fn server_show(&mut self, id: String) {
        let status = self.servers_page.status.take();
        self.servers_page = crate::servers::ServersPage {
            server: Some(id.clone()),
            busy: true,
            status,
            ..Default::default()
        };
        self.server_jobs
            .push(crate::servers::ServerJob::Libraries(id));
    }

    /// Goes to a place of the tab.
    pub fn server_go(&mut self, path: Vec<crate::servers::Place>) {
        let page = &mut self.servers_page;
        page.path = path;
        page.search = None;
        page.listing = None;
        self.server_list_more();
    }

    fn server_list_more(&mut self) {
        let page = &mut self.servers_page;
        let (Some(server), Some(place)) = (page.server.clone(), page.path.last()) else {
            return;
        };
        let start = page.listing.as_ref().map_or(0, |l| l.items.len());
        page.busy = true;
        page.error = None;
        let job = match &page.search {
            Some(query) => crate::servers::ServerJob::Search {
                server,
                library: page.path[0].parent.clone(),
                query: query.clone(),
                start,
            },
            None => crate::servers::ServerJob::List {
                server,
                parent: place.parent.clone(),
                start,
                kind: place.kind,
            },
        };
        self.server_jobs.push(job);
    }

    /// A search in the library shown; empty, back to the library.
    pub fn server_search(&mut self, query: String) {
        let page = &mut self.servers_page;
        page.search = (!query.is_empty()).then_some(query);
        page.listing = None;
        self.server_list_more();
    }

    /// The next page of what is shown.
    pub fn server_more(&mut self) {
        if !self.servers_page.busy {
            self.server_list_more();
        }
    }

    /// Asks for a cover, once.
    pub fn server_cover(&mut self, item: String, cover: String) {
        let page = &mut self.servers_page;
        if let Some(server) = page.server.clone()
            && page.asked.insert(item.clone())
        {
            self.server_jobs.push(crate::servers::ServerJob::Cover {
                server,
                item,
                cover,
            });
        }
    }

    /// Plays an item of the tab.
    pub fn server_play(&mut self, item: &str) {
        if let Some(server) = self.servers_page.server.clone() {
            let uid = nikomiru_library::server_items::server_uid(&server, item);
            self.open(Path::new(&uid));
        }
    }

    /// An answer for the pages of media servers.
    pub fn server_answer(&mut self, answer: crate::servers::ServerAnswer) {
        use crate::servers::ServerAnswer;
        let page = &mut self.servers_page;
        match answer {
            ServerAnswer::Added(result) => {
                page.logging_in = false;
                match result {
                    Ok(found) => self.add_servers(found),
                    Err(e) => page.status = Some((e, false)),
                }
            }
            ServerAnswer::SignIn(Ok(sign_in)) => self.external_links.push(sign_in.url),
            ServerAnswer::SignIn(Err(e)) => {
                page.logging_in = false;
                page.status = Some((e, false));
            }
            ServerAnswer::Libraries(server, result) => {
                if page.server.as_deref() != Some(server.as_str()) {
                    return;
                }
                page.busy = false;
                match result {
                    Ok(libraries) => {
                        let shown = self
                            .services
                            .config
                            .servers
                            .iter()
                            .find(|s| s.id == server)
                            .map(|s| s.libraries.clone())
                            .unwrap_or_default();
                        page.libraries = libraries
                            .into_iter()
                            .filter(|l| shown.is_empty() || shown.contains(&l.id))
                            .collect();
                    }
                    Err(e) => page.error = Some(e),
                }
            }
            ServerAnswer::Listed(key, result) => {
                let shown = page.search.as_ref().or(page.path.last().map(|p| &p.parent));
                if shown != Some(&key) {
                    return;
                }
                page.busy = false;
                match result {
                    Ok(listing) => {
                        let list = page.listing.get_or_insert(crate::servers::Listing {
                            items: Vec::new(),
                            total: 0,
                        });
                        list.items.extend(listing.items);
                        list.total = listing.total;
                    }
                    Err(e) => page.error = Some(e),
                }
            }
            ServerAnswer::Cover(item, Ok(bytes)) => {
                // within the limits of every cover (SRV-013)
                match crate::catalog::Cover::decode(&bytes) {
                    Ok(cover) => {
                        page.covers.insert(item, cover);
                    }
                    Err(e) => tracing::debug!("a server's cover: {e}"),
                }
            }
            ServerAnswer::Cover(_, Err(e)) => tracing::debug!("a server's cover: {e}"),
        }
    }

    /// Tells the server of the video playing, if there is one.
    fn report_server(&self, state: nikomiru_online::servers::PlayState) {
        if let Some(playing) = self.current.as_ref().and_then(|c| c.server.as_ref()) {
            self.services.servers.report(
                playing.video.uid.clone(),
                playing.video.source.clone(),
                state,
                (self.view.time_pos * 1000.0) as u64,
                (self.view.duration * 1000.0) as u64,
            );
        }
    }

    /// Nikomiru closes: the server hears of it.
    pub fn quitting(&mut self) {
        self.report_stopped();
    }

    /// The server hears that its video closed; whether one was playing.
    fn report_stopped(&mut self) -> bool {
        self.report_server(nikomiru_online::servers::PlayState::Stopped);
        self.current
            .as_mut()
            .and_then(|c| c.server.take())
            .is_some()
    }

    /// SRV-011: every 10 s while playing, and watched at 90% (LIB-008).
    fn tick_server(&mut self, now: std::time::Duration) {
        use nikomiru_online::servers::PlayState;
        let paused = self.view.paused;
        let share = self.view.time_pos / self.view.duration.max(1.0);
        let Some(playing) = self.current.as_mut().and_then(|c| c.server.as_mut()) else {
            return;
        };
        let due = !paused && now.saturating_sub(playing.reported) >= SERVER_REPORT;
        if due {
            playing.reported = now;
        }
        let watched = !playing.played && share >= 0.9;
        if watched {
            playing.played = true;
            self.services.servers.mark_played(playing.video.uid.clone());
        }
        if due {
            self.report_server(PlayState::Playing);
        }
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

    /// The request for a card from entry `entry` of popup `level`, with its
    /// popup's generation: the reference cue as the sentence, its clip, the
    /// frame of the click.
    fn mine_request(
        &self,
        level: usize,
        entry: usize,
        action: crate::mining::CardAction,
    ) -> Option<(u64, crate::mining::MineRequest)> {
        use crate::mining::{FrameChoice, MineRequest};
        let current = self.current.as_ref()?;
        let p = self.popups.stack().get(level)?;
        let e = p.reply.as_ref()?.content.entries.get(entry)?;
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
        let clip = cue.and_then(|c| self.clip_of(c, c));
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
            title: match &current.server {
                Some(playing) => playing.video.item.title.clone(),
                None => current
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            },
            action,
            frame: FrameChoice::Click,
            audio: true,
        };
        Some((p.generation, request))
    }

    /// The audio of cues `first..=last` for a card (ANK-032): padded as the
    /// settings say, at most 30 s; `None` for no length.
    fn clip_of(&self, first: usize, last: usize) -> Option<(i64, i64)> {
        let anki = &self.services.config.anki;
        let t = self.subs.timeline();
        if last >= t.cues().len() || first > last {
            return None;
        }
        let (head, tail) = (
            i64::from(anki.audio_pad_head_ms),
            i64::from(anki.audio_pad_tail_ms),
        );
        let start = t.clip(first, head, tail).0;
        let end = t.clip(last, head, tail).1;
        let end = end.min(start + nikomiru_media::clip::MAX_AUDIO_MS);
        (end > start).then_some((start, end))
    }

    /// Sends a card's request to the worker, the entry's button busy.
    fn send_mining(
        &mut self,
        level: usize,
        generation: u64,
        request: crate::mining::MineRequest,
        frame: Option<crate::mining::PendingFrame>,
    ) {
        use crate::mining::{AnkiJob, CardState};
        let entry = request.entry;
        if let Some(card) = self
            .popups
            .stack_mut()
            .get_mut(level)
            .filter(|p| p.generation == generation)
            .and_then(|p| p.cards.get_mut(entry))
        {
            *card = CardState::Busy;
        }
        self.anki_jobs.push(AnkiJob::Mine {
            generation,
            entry,
            request: Box::new(request),
            frame,
        });
    }

    /// Makes a card from entry `entry` of popup `level` at once.
    fn start_mining(&mut self, level: usize, entry: usize, action: crate::mining::CardAction) {
        let Some((generation, request)) = self.mine_request(level, entry, action) else {
            return;
        };
        let frame = self.ask_frame();
        self.send_mining(level, generation, request, frame);
    }

    /// Shift+click on an entry's card button (ANK-040): the "Advanced
    /// mining" dialog, with the frame of the click saved now.
    pub fn card_advanced(&mut self, level: usize, entry: usize) {
        use crate::mining::{AdvancedCard, CardAction, FrameKind};
        if self.popups.stack().get(level).is_none() || self.advanced.is_some() {
            return;
        }
        let Some((_, request)) = self.mine_request(level, entry, CardAction::Add) else {
            return;
        };
        let reference = self.reference_cue();
        let at = reference.unwrap_or(0);
        self.advanced = Some(AdvancedCard {
            level,
            entry,
            expression: request.expression.clone(),
            reference,
            first: at,
            last: at,
            sentence: request.sentence.clone(),
            offset: request.offset,
            clip: request.clip,
            frame: FrameKind::Click,
            audio: request.clip.is_some(),
            can_previous: false,
            can_next: false,
        });
        self.advanced_word = request.offset;
        self.advanced_frame = self.ask_frame();
        self.advanced_merge();
    }

    /// The sentence and the clip of the cues the dialog has (ANK-041).
    fn advanced_merge(&mut self) {
        let Some(a) = self.advanced.as_ref() else {
            return;
        };
        let Some(reference) = a.reference else {
            return;
        };
        let (first, last) = (a.first, a.last);
        let t = self.subs.timeline();
        let count = t.cues().len();
        let lines: Vec<&str> = t
            .cues()
            .get(first..=last.min(count.saturating_sub(1)))
            .unwrap_or_default()
            .iter()
            .map(|c| c.plain_text.as_str())
            .collect();
        let (sentence, offset) =
            crate::mining::merge_sentence(&lines, reference - first, self.advanced_word);
        let clip = self.clip_of(first, last);
        if let Some(a) = self.advanced.as_mut() {
            a.sentence = sentence;
            a.offset = offset;
            a.clip = clip;
            a.can_previous = first > 0;
            a.can_next = last + 1 < count;
        }
    }

    /// The lookup asked from the keyboard, until the overlay opens it.
    pub fn keyboard_lookup(&self) -> Option<(usize, usize)> {
        self.keyboard_lookup
    }

    /// The root popup's lookup starts a grapheme later or earlier in its cue.
    fn step_lookup(&mut self, forward: bool) {
        let Some(Origin::Subtitle { cue, start }) =
            self.popups.stack().first().map(|p| p.origin.clone())
        else {
            return;
        };
        let Some(q) = self.subs.cue(cue) else {
            return;
        };
        // word by word once its words are known (LKP-060); by grapheme
        // before, and past the last word (into a compound's own words)
        if self.words.get(&q.plain_text).is_none() {
            self.words.ask(&q.plain_text);
        } else if let Some(offset) = self.words.step(&q.plain_text, start, forward) {
            self.keyboard_lookup = Some((cue, offset));
            return;
        }
        let offsets: Vec<usize> = q.graphemes().iter().map(|g| g.offset).collect();
        let Some(i) = offsets.iter().position(|&o| o == start) else {
            return;
        };
        let j = if forward { i + 1 } else { i.wrapping_sub(1) };
        if let Some(&offset) = offsets.get(j) {
            self.keyboard_lookup = Some((cue, offset));
        }
    }

    /// The "Advanced mining" dialog open, if it is (ANK-040).
    pub fn advanced_card(&self) -> Option<&crate::mining::AdvancedCard> {
        self.advanced.as_ref()
    }

    /// A change in the "Advanced mining" dialog (ANK-040, ANK-041).
    pub fn advanced_edit(&mut self, edit: crate::mining::AdvancedEdit) {
        use crate::mining::{AdvancedEdit, CardAction, FrameChoice, FrameKind};
        let Some(a) = self.advanced.as_mut() else {
            return;
        };
        let count = self.subs.timeline().cues().len();
        match edit {
            AdvancedEdit::Previous(true) if a.first > 0 => a.first -= 1,
            AdvancedEdit::Previous(false) if Some(a.first) < a.reference => a.first += 1,
            AdvancedEdit::Next(true) if a.last + 1 < count => a.last += 1,
            AdvancedEdit::Next(false) if a.reference.is_some_and(|r| a.last > r) => a.last -= 1,
            AdvancedEdit::Previous(_) | AdvancedEdit::Next(_) => return,
            AdvancedEdit::Start(ms) => {
                if let Some((start, end)) = a.clip.as_mut() {
                    *start = ms.clamp(0, *end - 1);
                }
                return;
            }
            AdvancedEdit::End(ms) => {
                if let Some((start, end)) = a.clip.as_mut() {
                    *end = ms.clamp(*start + 1, *start + nikomiru_media::clip::MAX_AUDIO_MS);
                }
                return;
            }
            AdvancedEdit::Frame(kind) => {
                a.frame = kind;
                return;
            }
            AdvancedEdit::Audio(on) => {
                a.audio = on && a.clip.is_some();
                return;
            }
            AdvancedEdit::Preview => {
                // the clip from its start, paused at its end, as "Replay"
                if let Some((start, end)) = a.clip {
                    let p = &self.services.player;
                    let _ = p.stop_at(Some(end as f64 / 1000.0));
                    let _ = p.seek_absolute(start as f64 / 1000.0);
                    let _ = p.set_pause(false);
                    self.replaying = true;
                }
                return;
            }
            AdvancedEdit::Cancel => {
                self.close_advanced();
                return;
            }
            AdvancedEdit::Create => {
                let a = a.clone();
                let Some((generation, mut request)) =
                    self.mine_request(a.level, a.entry, CardAction::Add)
                else {
                    self.close_advanced();
                    return;
                };
                request.sentence = a.sentence;
                request.offset = a.offset;
                request.clip = a.clip;
                request.audio = a.audio && a.clip.is_some();
                request.frame = match a.frame {
                    FrameKind::Click => FrameChoice::Click,
                    FrameKind::SentenceStart => {
                        FrameChoice::At(a.reference.map_or(request.position_ms, |_| {
                            self.subs.timeline().start_pos(a.first)
                        }))
                    }
                    FrameKind::None => FrameChoice::None,
                };
                let frame = self.advanced_frame.take();
                self.close_advanced();
                self.send_mining(a.level, generation, request, frame);
                return;
            }
        }
        self.advanced_merge();
    }

    /// Closes the "Advanced mining" dialog; the frame it kept goes.
    fn close_advanced(&mut self) {
        self.advanced = None;
        self.advanced_frame = None;
        self.end_replay();
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
                self.update_secondary();
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
                let at = self.moment();
                self.study.card(at);
                let (expression, reading) = headword;
                self.words.add_mined(expression.clone());
                self.cards_made = true;
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
        // the duration, for the watched state (LIB-008)
        let duration = (self.view.duration > 0.0).then_some((self.view.duration * 1000.0) as u64);
        w.run(move |lib| {
            if let Err(e) = lib.save_position(&uid, ms, duration, at) {
                tracing::warn!("saving the position: {e}");
            }
        });
    }

    /// Today's local day number, for the statistics page (STA-005).
    pub fn today(&self) -> i64 {
        let at = self.moment();
        let (date, _) = nikomiru_library::stats::local_day_hour(at.unix_ms, at.offset_s);
        nikomiru_library::stats::day_number(&date).unwrap_or(0)
    }

    /// When now is, for the statistics.
    fn moment(&self) -> crate::study::Moment {
        let now = self.services.clock.now();
        crate::study::Moment {
            mono_ms: self.services.clock.monotonic().as_millis() as i64,
            unix_ms: now
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64),
            offset_s: self.services.clock.utc_offset(now),
        }
    }

    /// A sample of the position for the statistics (STA-001, STA-002),
    /// `playing` from now on.
    fn study_sample(&mut self, playing: bool) {
        let at = self.moment();
        let pos_ms = (self.view.time_pos * 1000.0) as i64;
        let timeline = self.subs.timeline();
        let delay = timeline.delay_ms;
        let cues = |from: i64, to: i64| -> Vec<crate::study::CueSpan> {
            timeline
                .cues()
                .iter()
                .enumerate()
                .filter(|(_, c)| c.start_ms + delay < to && c.end_ms + delay > from)
                .map(|(i, c)| {
                    let chars = nikomiru_text::count::count_study_chars(&c.plain_text) as u64;
                    (i as u32, c.start_ms + delay, c.end_ms + delay, chars)
                })
                .collect()
        };
        self.study
            .sample(at, pos_ms, self.view.speed, playing, &cues);
    }

    /// Writes what the statistics gathered, off the UI thread.
    fn write_study(&self, save: crate::study::Save) {
        if let Some(w) = &self.writer {
            w.run(move |lib| {
                if let Err(e) = lib.save_study(
                    &save.uid,
                    Some(&save.coverage),
                    save.credit.as_ref(),
                    &save.segments,
                ) {
                    tracing::warn!("saving the statistics: {e}");
                }
            });
        }
    }

    /// Exit or file change: write the last position and the per-file settings.
    pub fn flush_position(&mut self) {
        if let Some(save) = self.study.finish() {
            self.write_study(save);
        }
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
        // the shortcut editor takes the next chord; Esc gives up
        if self.capturing {
            if pressed && !auto_repeat {
                self.capturing = false;
                self.captured = (chord != "Escape").then_some(chord);
            }
            return (UiEffect::None, true);
        }
        // a page over the video: its controls get the keys, but the global
        // ones; a popup's keys first, then the player's (KEY-003)
        let action = if self.popups_covered {
            self.bindings.action(Context::Global, &chord, text_focus)
        } else if self.popups.is_open() {
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
            // NFR-A11Y-01: the first word of the subtitle shown
            Action::LookupSubtitle => {
                self.keyboard_lookup = self.subs.active().first().map(|&cue| (cue, 0));
                Ok(())
            }
            Action::LookupPrev | Action::LookupNext => {
                self.step_lookup(action == Action::LookupNext);
                Ok(())
            }
            Action::Settings => return UiEffect::OpenSettings,
            Action::CreateCardAdvanced => {
                let top = self.popups.stack().len().checked_sub(1);
                if let Some((level, entry)) =
                    top.and_then(|l| self.popups.stack().get(l).map(|p| (l, p.entry)))
                {
                    self.card_advanced(level, entry);
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
            Action::ScreenshotSubtitles => {
                if let Some(current) = &self.current {
                    let name = crate::screenshot::file_name(&current.path, self.view.time_pos);
                    self.capture = Some(name);
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
            Action::Back if self.advanced.is_some() => {
                self.close_advanced();
                return UiEffect::None;
            }
            Action::Back if self.popups.is_open() && !self.popups_covered => {
                self.close_popup();
                return UiEffect::None;
            }
            // LIB-009: Esc stops the next episode
            Action::Back if self.next_up.is_some() => {
                self.cancel_next();
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

    /// Audio track from the menu; its language is remembered for the series
    /// (LIB-010).
    pub fn select_audio(&self, id: i64) {
        let _ = self.services.player.set_aid((id > 0).then_some(id));
        if let Some(lang) = self
            .view
            .audio_tracks
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.lang.clone())
        {
            self.remember(nikomiru_library::policy::Remember::AudioLanguage(lang));
        }
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
        self.secondary.clear(None);
        self.secondary_generation += 1;
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
        let files = |dir: &Path| -> Vec<PathBuf> {
            std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                        .map(|e| e.path())
                        .collect()
                })
                .unwrap_or_default()
        };
        let folder = video.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut entries = files(&folder);
        // the folders releases keep subtitles in (SUB-006), and the one named
        // after this video inside them (`Subs/<video name>/`)
        let stem = video
            .file_stem()
            .map(|s| s.to_os_string())
            .unwrap_or_default();
        let mut own = Vec::new();
        for name in select::SUBTITLE_FOLDERS {
            let sub = folder.join(name);
            if sub.is_dir() {
                entries.extend(files(&sub));
                own.extend(files(&sub.join(&stem)));
            }
        }
        // and the subtitles downloaded for it (MET-008)
        let downloaded = self
            .current
            .as_ref()
            .and_then(|c| c.subtitles.as_ref())
            .map(|folder| nikomiru_online::store::list(folder))
            .unwrap_or_default();
        let mut externals = select::external_files(&video, &entries);
        if externals.is_empty() {
            // none with its name: those of its episode, or of a folder of its own
            let only_video = !entries.iter().any(|p| {
                p != &video
                    && p.parent() == video.parent()
                    && p.extension().is_some_and(|e| {
                        let e = format!("*.{}", e.to_string_lossy().to_lowercase());
                        crate::view::VIDEO_PATTERNS.contains(&e.as_str())
                    })
            });
            let language =
                |name: &str| nikomiru_online::language::subtitle_language(name).map(str::to_string);
            let read = select::NameReader {
                episode: &nikomiru_online::language::subtitle_episode,
                language: &language,
            };
            externals = select::related_files(&video, &entries, &own, only_video, &read);
        }
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
        // SUB-008 (1), LIB-010: the choice remembered, if it is still there
        let saved = self.saved_choice();
        let off = matches!(self.prefs.subtitle, Some(SubtitleChoice::Off));
        let choice = preferred
            .iter()
            .find_map(|lang| select::choose(saved.as_ref(), &externals, &embedded, lang))
            .filter(|_| !off);
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
        let online = !off && matches!(source, None | Some(Source::Bitmap(_)));
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
                        secondary: false,
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
                // STA-002: the cues of this file counted so far
                let key = crate::study::Study::credit_key(&source.key(), loaded.track.cues.len());
                let uid = self.current.as_ref().and_then(|c| c.uid.clone());
                if let (Some(uid), Some(lib)) = (uid, self.services.library.as_ref()) {
                    let credit = lib
                        .credit(&uid, &key)
                        .inspect_err(|e| tracing::warn!("library: {e}"))
                        .unwrap_or_default();
                    self.study.credits(credit);
                }
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

    /// The subtitle remembered for this video (LIB-010) as a choice among
    /// its sources: an embedded track found again by its index and language,
    /// else by its language and title.
    fn saved_choice(&self) -> Option<Choice> {
        match self.prefs.subtitle.as_ref()? {
            SubtitleChoice::File(p) => Some(Choice::External(PathBuf::from(p))),
            SubtitleChoice::Embedded {
                ff_index,
                language,
                title,
            } => {
                let embedded = || self.sub_tracks.iter().filter(|t| !t.external);
                embedded()
                    .find(|t| ff_index.is_some() && t.ff_index == *ff_index && t.lang == *language)
                    .or_else(|| embedded().find(|t| t.lang == *language && t.title == *title))
                    .map(|t| Choice::Embedded(t.id))
            }
            SubtitleChoice::Off => None,
        }
    }

    /// Remembers a choice for this video's series, or for the video
    /// (LIB-010).
    fn remember(&self, choice: nikomiru_library::policy::Remember) {
        let (Some(w), Some(Current { uid: Some(uid), .. })) =
            (self.writer.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let uid = uid.clone();
        w.run(move |lib| {
            if let Err(e) = lib.remember(&uid, choice) {
                tracing::warn!("library: {e}");
            }
        });
    }

    /// The choice of `source`, as the library keeps it.
    fn choice_of(&self, source: &Source) -> SubtitleChoice {
        let track = |id: i64| self.sub_tracks.iter().find(|t| t.id == id);
        match source {
            Source::File(p) => SubtitleChoice::File(p.to_string_lossy().into_owned()),
            Source::Embedded { id, .. } | Source::Bitmap(id) => SubtitleChoice::Embedded {
                ff_index: track(*id).and_then(|t| t.ff_index),
                language: track(*id).and_then(|t| t.lang.clone()),
                title: track(*id).and_then(|t| t.title.clone()),
            },
        }
    }

    /// Menu choice: a source key, or `"off"`.
    pub fn select_subtitles(&mut self, key: &str) {
        use nikomiru_library::policy::Remember;
        let remembered = match self.sub_sources.iter().find(|s| s.key() == key) {
            Some(source) => self.choice_of(source),
            None => SubtitleChoice::Off,
        };
        self.remember(Remember::Subtitle(remembered));
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
        self.remember(nikomiru_library::policy::Remember::Subtitle(
            self.choice_of(&source),
        ));
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

    /// The secondary subtitle follows the main one's offset (SUB-017).
    fn update_secondary(&mut self) {
        let delay = self.subs.timeline().delay_ms;
        self.secondary.timeline_mut().delay_ms = delay;
        self.secondary.update((self.view.time_pos * 1000.0) as i64);
    }

    /// The secondary subtitle chosen in the menu: a source key, or `"off"`
    /// (SUB-017). Text sources only: mpv draws graphic tracks.
    pub fn select_secondary(&mut self, key: &str) {
        self.secondary_generation += 1;
        let source = self
            .sub_sources
            .iter()
            .find(|s| s.key() == key && !matches!(s, Source::Bitmap(_)))
            .cloned();
        let (Some(source), Some(video)) = (source, self.current.as_ref().map(|c| c.path.clone()))
        else {
            self.secondary.clear(None);
            return;
        };
        self.sub_loads.push((
            self.secondary_generation,
            Request {
                video,
                source,
                encoding: None,
                duration_ms: (self.view.duration * 1000.0) as i64,
                secondary: true,
            },
        ));
    }

    /// A load of the secondary subtitle ended; an older one is dropped.
    pub fn secondary_loaded(
        &mut self,
        generation: u64,
        source: Source,
        result: Result<Loaded, String>,
    ) {
        if generation != self.secondary_generation {
            return;
        }
        match result {
            Ok(loaded) => {
                self.secondary.set_track(source, loaded);
                self.update_secondary();
            }
            Err(reason) => {
                self.secondary.clear(None);
                self.toasts
                    .push(self.tr("error-subtitles", &[("reason", reason)]));
            }
        }
    }

    /// The secondary subtitle's obscuring mode (SUB-017), from the menu.
    pub fn set_secondary_obscure(&mut self, mode: Obscure) {
        self.secondary.mode = mode;
        self.services.config.subtitles.secondary_obscure = mode.name().to_string();
    }

    /// The obscuring of the secondary subtitle now: its own mode, lifted
    /// like the main one's (SUB-014, SUB-017).
    pub fn secondary_obscure_now(&self) -> Obscure {
        crate::subtitles::obscured(
            self.secondary.mode,
            self.view.loaded && !self.view.paused,
            self.popups.is_open(),
            self.subs.hovered,
        )
    }

    /// Whether the secondary subtitle goes above the main one at the bottom,
    /// rather than at the top of the video.
    pub fn secondary_at_bottom(&self) -> bool {
        self.services.config.subtitles.secondary_position == "bottom"
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
        self.remember(nikomiru_library::policy::Remember::Delay(delay_ms));
        self.subs.update((self.view.time_pos * 1000.0) as i64);
        self.update_secondary();
        self.sub_list_version += 1;
        self.toasts
            .push(self.tr("toast-sub-delay", &[("value", format!("{delay_ms:+}"))]));
    }

    /// A click on a grapheme of the overlay or of the list (POP-001): the
    /// text from there (LKP-001…LKP-003) is looked up in a popup next to
    /// `anchor`; playback pauses if it was going.
    pub fn select_grapheme(&mut self, cue: usize, offset: usize, anchor: Rect) {
        self.keyboard_lookup = None;
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
        {
            let at = self.moment();
            self.study.lookup(at);
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

    /// Whether the next frame is for a picture (PLY-015): only the video and
    /// the subtitles are drawn.
    pub fn capture_pending(&self) -> bool {
        self.capture.is_some()
    }

    /// Saves what the frame drew, off the UI thread.
    pub fn start_capture(
        &mut self,
        pixels: crate::screenshot::Pixels,
        done: Box<dyn FnOnce(Result<PathBuf, String>) + Send>,
    ) {
        if let Some(name) = self.capture.take() {
            let dir = self.services.screenshots_dir.clone();
            crate::screenshot::start_pixels(pixels, dir, name, done);
        }
    }

    /// The lookups to start, taken by the UI layer.
    pub fn take_lookups(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.lookups)
    }

    /// The subtitle texts to split into words, taken by the UI layer: the
    /// cues shown and the next few, while the overlay marks words (LKP-060).
    pub fn take_segmentations(&mut self) -> Vec<(u64, String)> {
        let c = &self.services.config.subtitles;
        if c.furigana || c.mark_mined {
            let t = self.subs.timeline();
            let pos = (self.view.time_pos * 1000.0) as i64;
            let next = t.list_position(pos).map_or(0, |i| i + 1);
            let ahead = (next..next + AHEAD).filter_map(|i| t.cues().get(i));
            for cue in self.subs.shown(false).map(|s| s.cue).chain(ahead) {
                self.words.ask(&cue.plain_text);
            }
        }
        self.words.take_queue()
    }

    /// Splits a subtitle text into words on the dictionary service.
    pub fn start_segment(
        &self,
        text: String,
        done: Box<dyn FnOnce(Result<Vec<nikomiru_dict::segment::Word>, String>) + Send>,
    ) {
        self.services.dictionaries.segment(text, done);
    }

    /// The words of a subtitle text.
    pub fn segmented(
        &mut self,
        generation: u64,
        text: String,
        result: Result<Vec<nikomiru_dict::segment::Word>, String>,
    ) {
        self.words.done(generation, text, result);
    }

    /// The rows of the subtitle list in view: their words are split, to
    /// underline the mined ones (LKP-061).
    pub fn list_rows(&mut self, cues: &[usize]) {
        if !self.services.config.subtitles.mark_mined {
            return;
        }
        for &i in cues {
            if let Some(cue) = self.subs.cue(i) {
                self.words.ask(&cue.plain_text);
            }
        }
    }

    /// The words of `text` already made into cards, in bytes, while they are
    /// underlined (LKP-061).
    pub fn mined_words(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        if !self.services.config.subtitles.mark_mined {
            return Vec::new();
        }
        self.words
            .marks(text, false, true)
            .map(|m| m.mined)
            .unwrap_or_default()
    }

    /// The last step of the progress bar the pointer was over (PLY-014).
    pub fn bar_hovered(&self) -> Option<i64> {
        self.thumbnails.last()
    }

    /// The library's first steps (LIB-013): a dictionary imported, a folder
    /// of videos added, a card made; `None` once all are done, or hidden.
    pub fn first_steps(&self) -> Option<[bool; 3]> {
        let home = self.library_page.home.as_ref()?;
        let steps = [
            self.dictionaries_installed,
            !home.series.is_empty() || !home.folders.is_empty(),
            self.cards_made,
        ];
        (self.services.config.general.first_steps && steps.contains(&false)).then_some(steps)
    }

    /// "Hide" on the first steps: saved, so they do not come back.
    pub fn hide_first_steps(&mut self) {
        self.services.config.general.first_steps = false;
        if self.services.config_writable {
            self.saving_page = SavingPage::Settings;
            self.config_save = Some(self.services.config.clone());
        }
    }

    /// The pointer is over step `at_ms` of the progress bar (PLY-014).
    pub fn bar_hover(&mut self, at_ms: i64) {
        self.thumbnails.want(at_ms);
    }

    /// The frame of the progress bar to make, taken by the UI layer.
    pub fn take_thumbnail(&mut self) -> Option<(u64, PathBuf, i64)> {
        self.thumbnails.take()
    }

    /// Makes a frame of the progress bar on the thumbnail service.
    pub fn start_thumbnail(
        &self,
        video: PathBuf,
        at_ms: i64,
        done: Box<dyn FnOnce(Result<Vec<u8>, String>) + Send>,
    ) {
        self.services.thumbnails.frame(video, at_ms, done);
    }

    /// A frame of the progress bar made.
    pub fn thumbnail_done(&mut self, generation: u64, at_ms: i64, result: Result<Vec<u8>, String>) {
        self.thumbnails.done(generation, at_ms, result);
    }

    /// The thumbnail of step `at_ms` of the progress bar, with the generation
    /// of its file.
    pub fn thumbnail(
        &self,
        at_ms: i64,
    ) -> Option<(u64, std::sync::Arc<nikomiru_dict::structured::Pixels>)> {
        self.thumbnails.get(at_ms)
    }

    /// What the overlay marks over `text` (LKP-051, LKP-061).
    pub fn word_marks(&self, text: &str) -> Option<crate::words::Marks> {
        let c = &self.services.config.subtitles;
        if !(c.furigana || c.mark_mined) {
            return None;
        }
        self.words
            .marks(text, c.furigana, c.mark_mined)
            .filter(|m| !m.is_empty())
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
            self.words.forget();
            self.dictionaries_installed = true;
        }
        self.dict_effects(effects);
    }

    /// Moves a dictionary up or down in its group (DIC-031).
    pub fn dict_move(&mut self, id: &str, delta: i32) {
        let effects = self
            .dict_page
            .move_by(self.services.dictionaries.as_ref(), id, delta);
        self.words.forget();
        self.dict_effects(effects);
    }

    /// Moves a dictionary to a gap of its group, where it was dropped (DIC-031).
    pub fn dict_move_to(&mut self, id: &str, to: usize) {
        let effects = self
            .dict_page
            .move_to(self.services.dictionaries.as_ref(), id, to);
        self.words.forget();
        self.dict_effects(effects);
    }

    /// Enables or disables a dictionary (DIC-032).
    pub fn dict_set_enabled(&mut self, id: &str, enabled: bool) {
        let effects = self
            .dict_page
            .set_enabled(self.services.dictionaries.as_ref(), id, enabled);
        self.words.forget();
        self.dict_effects(effects);
    }

    /// Deletes a dictionary (DIC-033).
    pub fn dict_delete(&mut self, id: &str) {
        let effects = self
            .dict_page
            .delete(self.services.dictionaries.as_ref(), id);
        self.words.forget();
        self.dictionaries_installed = self
            .services
            .dictionaries
            .list()
            .is_ok_and(|l| !l.is_empty());
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
            // AUD-006: the word of the first entry, without overlapping the
            // line, which the lookup paused
            if self.services.config.word_audio.autoplay
                && let Some((term, reading)) = entries.first()
            {
                self.play_word(term, reading);
            }
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
