// SPDX-License-Identifier: GPL-3.0-only

//! The playback session behind the player page, without Qt: it opens files,
//! filters and reduces player events into a [`ViewState`], saves the position
//! (LIB-006) and turns shortcut actions into player commands.

use std::path::{Path, PathBuf};

use nikomiru_core::{Clock, Config};
use nikomiru_library::progress::ProgressSaver;
use nikomiru_library::{Library, VideoRecord};
use nikomiru_player::control::{self, HoldToSpeed, LoadToken, Stamped};
use nikomiru_player::player::{EndFile, Track};
use nikomiru_player::{LoadRequest, PlayerEvent, PlayerHandle, options};

use crate::controls::ControlsVisibility;
use crate::i18n::Translator;
use crate::screensaver::ScreensaverPolicy;
use crate::shortcuts::{Action, Bindings, Context, Modifiers};
use crate::subtitles::{Loaded, Obscure, Request, Source, Subtitles};
use nikomiru_subs::select::{self, Choice, EmbeddedTrack};
use nikomiru_subs::timeline::Jump;

/// What the QML layer must do after an action (things Rust cannot do itself).
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

impl UiEffect {
    /// Name used by the QML layer.
    pub fn name(self) -> &'static str {
        match self {
            UiEffect::None => "",
            UiEffect::ToggleFullscreen => "fullscreen",
            UiEffect::Back => "back",
            UiEffect::OpenDialog => "open_file",
            UiEffect::ToggleSubList => "sub_list",
            UiEffect::SubSearch => "sub_search",
        }
    }
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
}

/// Everything the session needs from the application.
pub struct Services {
    /// The player.
    pub player: PlayerHandle,
    /// The configuration.
    pub config: Config,
    /// The library, if it could be opened.
    pub library: Option<Library>,
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
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            out.push(b);
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
        }
    }

    fn tr(&self, id: &str, args: &[(&str, String)]) -> String {
        self.services.translator.tr(id, args)
    }

    /// The player, for the GUI test harness's `mpv_get`.
    #[cfg(feature = "test_harness")]
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
            lib.put_video(&uid, &record)
                .inspect_err(|e| tracing::warn!("library: {e}"))
                .ok()?;
            Some(uid)
        });
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
            start: (record.last_position_ms > 0).then(|| record.last_position_ms as f64 / 1000.0),
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
        match self.services.player.load(&path.to_string_lossy(), request) {
            Ok((token, rejected)) => {
                self.token = token;
                for e in rejected {
                    tracing::warn!("mpv rejected option {}: {}", e.option.key, e.message);
                }
            }
            Err(e) => {
                tracing::error!("load failed: {e}");
                self.view.error = Some(self.services.translator.error(&e));
            }
        }
        self.current = Some(Current { uid, path });
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

    /// Periodic update (from the UI's timer): the screensaver policy.
    pub fn tick(&mut self) {
        self.update_screensaver();
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
        let (Some(lib), Some(Current { uid: Some(uid), .. })) =
            (self.services.library.as_ref(), self.current.as_ref())
        else {
            return;
        };
        let at = self
            .services
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        if let Err(e) = lib.save_position(uid, ms, at) {
            tracing::warn!("saving the position: {e}");
        }
    }

    /// Exit or file change: write the last position and the per-file settings.
    pub fn flush_position(&mut self) {
        if let Some(ms) = self.saver.flush() {
            self.save_position(ms);
        }
        let (Some(lib), Some(Current { uid: Some(uid), .. })) =
            (self.services.library.as_ref(), self.current.as_ref())
        else {
            return;
        };
        if let Ok(Some(mut rec)) = lib.video(uid) {
            rec.aid = self.view.aid;
            rec.volume = Some(self.view.volume);
            rec.speed = Some(self.view.speed);
            if self.view.duration > 0.0 {
                rec.duration_ms = Some((self.view.duration * 1000.0) as u64);
            }
            let _ = lib.put_video(uid, &rec);
        }
    }

    /// The mouse moved over the window (PLY-004).
    pub fn mouse_moved(&mut self) {
        self.controls.mouse_moved(self.services.clock.monotonic());
    }

    /// Whether the control bar and the cursor are visible now (PLY-004).
    pub fn controls_visible(&self) -> bool {
        !self.view.loaded || self.controls.visible(self.services.clock.monotonic())
    }

    /// A key event from the QML layer (KEY-002: Qt's `nativeScanCode`).
    pub fn key(
        &mut self,
        native_scan_code: u32,
        qt_modifiers: u32,
        pressed: bool,
        auto_repeat: bool,
        text_focus: bool,
    ) -> UiEffect {
        let Some(chord) =
            crate::shortcuts::chord(native_scan_code, Modifiers::from_qt(qt_modifiers))
        else {
            if pressed {
                tracing::debug!("key: scan code {native_scan_code} has no name");
            }
            return UiEffect::None;
        };
        let action = self.bindings.action(Context::Player, &chord, text_focus);
        if pressed && !auto_repeat {
            tracing::debug!("key {chord} → {}", action.map_or("nothing", Action::name));
        }
        let Some(action) = action else {
            return UiEffect::None;
        };
        if !pressed {
            if action == Action::HoldSpeed
                && let Some(speed) = self.hold.key_up()
            {
                let _ = self.services.player.set_speed(speed);
            }
            return UiEffect::None;
        }
        if action == Action::HoldSpeed {
            if let Some(speed) = self.hold.key_down(self.view.speed, auto_repeat) {
                let _ = self.services.player.set_speed(speed);
            }
            return UiEffect::None;
        }
        self.action(action)
    }

    /// Runs an action (from a key or a button).
    pub fn action(&mut self, action: Action) -> UiEffect {
        let p = &self.services.player;
        let step = control::seek_step(self.services.config.player.seek_seconds);
        let result = match action {
            Action::PlayPause => p.toggle_pause(),
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
    fn reset_subtitles(&mut self) {
        self.subs.clear(None);
        self.sub_sources.clear();
        self.sub_tracks.clear();
        self.sub_generation += 1;
        self.sub_loads.clear();
        self.sub_auto_done = false;
        self.sub_list_version += 1;
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
        let externals = select::external_files(&video, &entries);
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
        if let Some(source) = source {
            self.use_subtitles(source);
        }
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
                self.sub_list_version += 1;
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
        match result {
            Ok(loaded) => {
                let name = source_name(&source);
                let encoding = loaded.encoding.as_deref().map(|e| self.encoding_label(e));
                self.subs.set_track(source, loaded);
                self.subs.update((self.view.time_pos * 1000.0) as i64);
                self.sub_list_version += 1;
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
                self.sub_list_version += 1;
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
            self.sub_list_version += 1;
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

    /// A click on a grapheme of the overlay or of the list: playback pauses and
    /// the grapheme is selected (M3 looks the word up from there).
    pub fn select_grapheme(&mut self, cue: usize, offset: usize) {
        if self
            .subs
            .cue(cue)
            .is_none_or(|c| offset >= c.plain_text.len())
        {
            return;
        }
        if !self.view.paused {
            let _ = self.services.player.set_pause(true);
        }
        self.subs.selection = Some((cue, offset));
    }

    /// A click in the text of a list row, at a QML (UTF-16) position.
    pub fn select_in_list(&mut self, cue: usize, utf16_pos: usize) {
        if let Some(c) = self.subs.cue(cue) {
            let offset = crate::subtitles::utf16_to_byte(&c.plain_text, utf16_pos);
            self.select_grapheme(cue, offset);
        }
    }

    /// A click on the time of a list row: playback goes to the cue.
    pub fn seek_to_cue(&self, cue: usize) {
        if cue < self.subs.timeline().cues().len() {
            let ms = self.subs.timeline().start_pos(cue);
            let _ = self.services.player.seek_absolute(ms as f64 / 1000.0);
        }
    }

    /// The obscuring that applies now (SUB-014): no popup exists before M3.
    pub fn obscure_now(&self) -> Obscure {
        crate::subtitles::obscured(
            self.subs.mode,
            self.view.loaded && !self.view.paused,
            false,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
