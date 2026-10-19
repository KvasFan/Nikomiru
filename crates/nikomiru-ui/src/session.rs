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
}

impl UiEffect {
    /// Name used by the QML layer.
    pub fn name(self) -> &'static str {
        match self {
            UiEffect::None => "",
            UiEffect::ToggleFullscreen => "fullscreen",
            UiEffect::Back => "back",
            UiEffect::OpenDialog => "open_file",
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
        }
    }
}

/// Keeps the screen on (PLY-013); the application talks to the desktop.
pub trait IdleInhibitor: Send {
    /// Inhibits the screensaver, or releases the inhibition.
    fn set(&self, inhibit: bool);
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
                self.view.audio_tracks = t.into_iter().filter(|t| t.kind == "audio").collect();
            }
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
            return UiEffect::None;
        };
        let Some(action) = self.bindings.action(Context::Player, &chord, text_focus) else {
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
