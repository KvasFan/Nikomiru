// SPDX-License-Identifier: GPL-3.0-only

//! The mpv core (ARC-001): a [`Player`] owns the core; [`PlayerHandle`]s queue
//! commands, which the `mpv-commands` thread runs in order; the `mpv-events`
//! thread turns mpv events into [`PlayerEvent`]s stamped with the [`LoadToken`]
//! of their load (ARC-002), with `time-pos` throttled (ARC-004).
//!
//! The UI thread draws the video with the render API ([`crate::render`]), so
//! it must never wait for the core (`<mpv/render.h>`, "Threading"): commands
//! are queued, and their errors are logged by the command thread.
//!
//! The UI never reads mpv properties synchronously (ARC-003): its state comes
//! from the events. `PlayerHandle::property_string` exists, with the
//! `test-support` feature, for tests and the
//! GUI harness only, and never on the UI thread.

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Instant;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv};
use tracing::Level;

use crate::control::{self, LoadToken, Stamped, Throttle};
use crate::options::{FIXED, MpvOption};

/// Number of mpv cores created by this process (T-PLY-009: the core is reused).
static INSTANCES: AtomicUsize = AtomicUsize::new(0);

/// How many mpv cores this process has created.
pub fn instances_created() -> usize {
    INSTANCES.load(Ordering::SeqCst)
}

/// Errors of the player.
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    /// mpv returned an error.
    #[error("mpv: {}", mpv_reason(.0))]
    Mpv(#[from] libmpv2::Error),
    /// A thread of the player could not be started, or has stopped.
    #[error("mpv thread: {0}")]
    Thread(std::io::Error),
    /// A path mpv cannot take: it is not valid UTF-8.
    #[error("not a UTF-8 path: {}", .0.display())]
    Path(std::path::PathBuf),
}

/// A libmpv2 error in words: mpv's error codes become [`PlaybackError`]s.
fn mpv_reason(e: &libmpv2::Error) -> String {
    match e {
        libmpv2::Error::Raw(code) => PlaybackError::from_code(*code).to_string(),
        other => format!("{other:?}"),
    }
}

impl nikomiru_core::Localized for PlayerError {
    fn message_id(&self) -> &'static str {
        match self {
            PlayerError::Mpv(_) => "error-player",
            PlayerError::Thread(_) => "error-player-thread",
            PlayerError::Path(_) => "error-player-path",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            PlayerError::Mpv(e) => vec![("reason", mpv_reason(e))],
            PlayerError::Thread(e) => vec![("reason", e.to_string())],
            PlayerError::Path(p) => vec![("path", p.display().to_string())],
        }
    }
}

/// Why mpv could not play a file (PLY-012), from mpv's error codes
/// (`mpv_error` in `<mpv/client.h>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PlaybackError {
    /// `MPV_ERROR_LOADING_FAILED`: missing, unreadable or broken file.
    #[error("loading failed")]
    LoadingFailed,
    /// `MPV_ERROR_AO_INIT_FAILED`.
    #[error("audio output initialization failed")]
    AudioOutput,
    /// `MPV_ERROR_VO_INIT_FAILED`.
    #[error("video output initialization failed")]
    VideoOutput,
    /// `MPV_ERROR_NOTHING_TO_PLAY`: no audio or video stream.
    #[error("nothing to play")]
    NothingToPlay,
    /// `MPV_ERROR_UNKNOWN_FORMAT`.
    #[error("unrecognized file format")]
    UnknownFormat,
    /// `MPV_ERROR_UNSUPPORTED`.
    #[error("not supported")]
    Unsupported,
    /// Any other code.
    #[error("mpv error {0}")]
    Other(i32),
}

impl PlaybackError {
    /// From an `mpv_error` code.
    pub fn from_code(code: i32) -> Self {
        match code {
            -13 => PlaybackError::LoadingFailed,
            -14 => PlaybackError::AudioOutput,
            -15 => PlaybackError::VideoOutput,
            -16 => PlaybackError::NothingToPlay,
            -17 => PlaybackError::UnknownFormat,
            -18 => PlaybackError::Unsupported,
            other => PlaybackError::Other(other),
        }
    }
}

impl nikomiru_core::Localized for PlaybackError {
    fn message_id(&self) -> &'static str {
        match self {
            PlaybackError::LoadingFailed => "playback-loading-failed",
            PlaybackError::AudioOutput => "playback-audio-output",
            PlaybackError::VideoOutput => "playback-video-output",
            PlaybackError::NothingToPlay => "playback-nothing-to-play",
            PlaybackError::UnknownFormat => "playback-unknown-format",
            PlaybackError::Unsupported => "playback-unsupported",
            PlaybackError::Other(_) => "playback-other",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            PlaybackError::Other(code) => vec![("code", code.to_string())],
            _ => Vec::new(),
        }
    }
}

/// Where the video goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoOutput {
    /// The libmpv render API, drawn by the UI ([`crate::render`]).
    RenderApi,
    /// No video and no audio output (headless tests, L5).
    Null,
}

/// A track of `track-list` (PLY-003).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    /// mpv track id (`aid`/`sid`/`vid`).
    pub id: i64,
    /// `audio`, `video` or `sub`.
    pub kind: String,
    /// Language tag, if any.
    pub lang: Option<String>,
    /// Title, if any.
    pub title: Option<String>,
    /// Codec name.
    pub codec: Option<String>,
    /// Audio channels.
    pub channels: Option<i64>,
    /// Index in the container, for ffmpeg (`-map 0:<ff-index>`).
    pub ff_index: Option<i64>,
    /// Currently selected.
    pub selected: bool,
    /// From an external file.
    pub external: bool,
    /// That file, for an external track: its `ff_index` counts the streams
    /// of that file, not of the video.
    pub external_filename: Option<String>,
}

/// A chapter (PLY-001).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chapter {
    /// Start time in seconds.
    pub time: f64,
    /// Title, if any.
    pub title: Option<String>,
}

/// Why a file stopped (PLY-012).
#[derive(Debug, Clone, PartialEq)]
pub enum EndFile {
    /// End of the file reached.
    Eof,
    /// Stopped or replaced by another `loadfile`.
    Stopped,
    /// Could not be played (missing file, unsupported format…).
    Error(PlaybackError),
}

/// An event of the player, for the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    /// The file is open (PLY-008 has been applied).
    FileLoaded,
    /// The file ended.
    EndFile(EndFile),
    /// Playback position in seconds (≤ 30 per second, ARC-004).
    TimePos(f64),
    /// Duration in seconds.
    Duration(f64),
    /// Paused or playing.
    Pause(bool),
    /// Playback speed.
    Speed(f64),
    /// Volume in percent.
    Volume(f64),
    /// Muted.
    Mute(bool),
    /// The tracks of the file.
    Tracks(Vec<Track>),
    /// Selected audio track (`None` = no audio).
    Aid(Option<i64>),
    /// The chapters of the file.
    Chapters(Vec<Chapter>),
    /// End of file reached while `keep-open` holds the last frame.
    EofReached(bool),
    /// A seek is in progress.
    Seeking(bool),
    /// Paused while waiting for the cache.
    PausedForCache(bool),
    /// Video size in pixels.
    VideoSize(i64, i64),
    /// Seconds of data ahead in the demuxer cache.
    CacheAhead(f64),
    /// The core is shutting down.
    Shutdown,
}

/// What a `load` needs besides the path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadRequest {
    /// Resume position in seconds (PLY-007); `None` = from the start.
    pub start: Option<f64>,
    /// Options applied before `loadfile` (PLY-006, from [`crate::options::load_options`]).
    pub options: Vec<MpvOption>,
    /// Audio track to select once loaded.
    pub aid: Option<i64>,
    /// Volume to apply once loaded (PLY-008).
    pub volume: Option<f64>,
    /// Speed to apply once loaded (PLY-008).
    pub speed: Option<f64>,
}

type Sink = Arc<dyn Fn(Stamped<PlayerEvent>) + Send + Sync>;

/// Where the result of a command with a reply arrives ([`PlayerHandle::screenshot`]).
pub type CommandReply = mpsc::Receiver<Result<(), PlayerError>>;

/// A command for the core, run in order on the `mpv-commands` thread.
enum Command {
    /// Options, `start` and `loadfile` of a load.
    Load {
        token: LoadToken,
        path: String,
        request: LoadRequest,
    },
    /// Sets a property.
    Set(String, String),
    /// Runs a command.
    Run(&'static str, Vec<String>),
    /// Runs a command and sends its result back.
    Reply(
        &'static str,
        Vec<String>,
        mpsc::Sender<Result<(), PlayerError>>,
    ),
    /// Ends the thread ([`Player::shutdown`]).
    Stop,
}

/// Shared between the handles and the events thread.
struct Shared {
    mpv: Mpv,
    token: AtomicU64,
    pending: Mutex<Option<(LoadToken, LoadRequest)>>,
    /// The values of the Player settings' options before any was set: the
    /// value an option goes back to when the user unsets it (PLY-018).
    defaults: std::collections::BTreeMap<String, String>,
}

impl Shared {
    fn token(&self) -> LoadToken {
        LoadToken(self.token.load(Ordering::SeqCst))
    }
}

/// Sends commands to the player. Cheap to clone; usable from any thread, and
/// never blocks on the core.
#[derive(Clone)]
pub struct PlayerHandle {
    shared: Arc<Shared>,
    commands: mpsc::Sender<Command>,
}

impl std::fmt::Debug for PlayerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerHandle")
            .field("token", &self.shared.token())
            .finish()
    }
}

/// Owns the mpv core and its command and events threads.
pub struct Player {
    shared: Arc<Shared>,
    commands: mpsc::Sender<Command>,
    command_thread: Option<JoinHandle<()>>,
    events: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("token", &self.shared.token())
            .finish()
    }
}

// SAFETY: the signature matches `<mpv/client.h>`: `int mpv_request_log_messages(
// mpv_handle *ctx, const char *min_level)`.
unsafe extern "C" {
    /// `mpv_request_log_messages` from `<mpv/client.h>`: libmpv2 6.0 does not
    /// expose it. libmpv itself is linked by `libmpv2-sys`.
    fn mpv_request_log_messages(
        ctx: *mut std::ffi::c_void,
        min_level: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

/// Sends libmpv's messages to the events thread, which writes them to the log
/// (SPECIFICATION §5.2.2 #11): `warn` and above, `v` when debug logging is on.
fn request_log_messages(client: &Mpv) {
    let level = if tracing::enabled!(target: "mpv", Level::DEBUG) {
        c"v"
    } else {
        c"warn"
    };
    // SAFETY: `client.ctx` is a live mpv handle owned by `client`, and `level` is a
    // NUL-terminated string that mpv copies before returning.
    let rc = unsafe { mpv_request_log_messages(client.ctx.as_ptr().cast(), level.as_ptr()) };
    if rc < 0 {
        tracing::warn!(target: "mpv", "mpv_request_log_messages failed: {rc}");
    }
}

/// Writes one libmpv message to the log.
fn log_message(prefix: &str, level: &str, text: &str) {
    let text = text.trim_end();
    match level {
        "fatal" | "error" => tracing::error!(target: "mpv", "[{prefix}] {text}"),
        "warn" => tracing::warn!(target: "mpv", "[{prefix}] {text}"),
        "info" => tracing::info!(target: "mpv", "[{prefix}] {text}"),
        "v" | "debug" => tracing::debug!(target: "mpv", "[{prefix}] {text}"),
        _ => tracing::trace!(target: "mpv", "[{prefix}] {text}"),
    }
}

/// Properties observed by the events thread (ARC-001; node properties through
/// their sub-properties, because libmpv2 6.0 has no node support).
const OBSERVED: [(&str, Format); 14] = [
    ("time-pos", Format::Double),
    ("duration", Format::Double),
    ("pause", Format::Flag),
    ("speed", Format::Double),
    ("volume", Format::Double),
    ("mute", Format::Flag),
    ("track-list/count", Format::Int64),
    ("aid", Format::String),
    ("chapter-list/count", Format::Int64),
    ("eof-reached", Format::Flag),
    ("seeking", Format::Flag),
    ("paused-for-cache", Format::Flag),
    ("video-params/dw", Format::Int64),
    ("demuxer-cache-duration", Format::Double),
];

impl Player {
    /// Creates the mpv core with the fixed options (PLY-005) and starts the
    /// command and events threads; `sink` receives every event.
    pub fn new(
        output: VideoOutput,
        sink: impl Fn(Stamped<PlayerEvent>) + Send + Sync + 'static,
    ) -> Result<Self, PlayerError> {
        let mpv = Mpv::with_initializer(|init| {
            for (k, v) in FIXED {
                let v = match (k, output) {
                    ("vo", VideoOutput::Null) => "null",
                    _ => v,
                };
                init.set_property(k, v)?;
            }
            if output == VideoOutput::Null {
                init.set_property("ao", "null")?;
            } else {
                for o in crate::options::core_options(crate::options::nvidia_driver_loaded()) {
                    init.set_property(&o.key, o.value.as_str())?;
                }
            }
            Ok(())
        })?;
        INSTANCES.fetch_add(1, Ordering::SeqCst);
        // `create_client(None)`: with a name, libmpv2 6.0 passes mpv a dangling
        // pointer (ADR 0001).
        let client = mpv.create_client(None)?;
        request_log_messages(&client);
        for (name, format) in OBSERVED {
            client.observe_property(name, format, 0)?;
        }
        let defaults = crate::settings::CONTROLS
            .iter()
            .filter(|c| c.key != "profile")
            .filter_map(|c| {
                mpv.get_property::<String>(c.key)
                    .ok()
                    .map(|v| (c.key.to_string(), v))
            })
            .collect();
        let shared = Arc::new(Shared {
            mpv,
            token: AtomicU64::new(0),
            pending: Mutex::new(None),
            defaults,
        });
        let sink: Sink = Arc::new(sink);
        let (commands, queue) = mpsc::channel();
        let command_shared = shared.clone();
        let command_sink = sink.clone();
        let command_thread = std::thread::Builder::new()
            .name("mpv-commands".into())
            .spawn(move || command_loop(&command_shared, &queue, &command_sink))
            .map_err(PlayerError::Thread)?;
        let thread_shared = shared.clone();
        let events = std::thread::Builder::new()
            .name("mpv-events".into())
            .spawn(move || event_loop(client, &thread_shared, &sink))
            .map_err(PlayerError::Thread)?;
        Ok(Self {
            shared,
            commands,
            command_thread: Some(command_thread),
            events: Some(events),
        })
    }

    /// A handle for sending commands.
    pub fn handle(&self) -> PlayerHandle {
        PlayerHandle {
            shared: self.shared.clone(),
            commands: self.commands.clone(),
        }
    }

    /// The raw `mpv_handle`, for the render context ([`crate::render`]), which
    /// MUST be freed before [`Player::shutdown`] (SPECIFICATION §5.2.2).
    pub fn raw_handle(&self) -> *mut std::ffi::c_void {
        self.shared.mpv.ctx.as_ptr().cast()
    }

    /// Stops the core in the required order: the commands already queued, then
    /// `quit`, then the join of the events thread; the core itself goes when the
    /// last handle is dropped. Commands sent afterwards fail.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(t) = self.command_thread.take() {
            let _ = self.commands.send(Command::Stop);
            let _ = t.join();
        }
        let _ = self.shared.mpv.command("quit", &[]);
        if let Some(t) = self.events.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

impl PlayerHandle {
    fn send(&self, command: Command) -> Result<(), PlayerError> {
        self.commands.send(command).map_err(|_| {
            PlayerError::Thread(std::io::Error::other("the command thread has stopped"))
        })
    }

    fn set(&self, name: &str, value: &str) -> Result<(), PlayerError> {
        self.send(Command::Set(name.to_string(), value.to_string()))
    }

    fn cmd(&self, name: &'static str, args: &[&str]) -> Result<(), PlayerError> {
        self.send(Command::Run(
            name,
            args.iter().map(|a| (*a).to_string()).collect(),
        ))
    }

    /// The token of the current load.
    pub fn current_token(&self) -> LoadToken {
        self.shared.token()
    }

    /// Loads a file, replacing the current one but keeping the core (PLY-009),
    /// after its options (PLY-006); returns the token of the load at once. The
    /// options mpv rejects are logged by name; if mpv refuses the load itself,
    /// an [`EndFile::Error`] with this token follows.
    pub fn load(&self, path: &str, request: LoadRequest) -> Result<LoadToken, PlayerError> {
        let token = LoadToken(self.shared.token.fetch_add(1, Ordering::SeqCst) + 1);
        self.send(Command::Load {
            token,
            path: path.to_string(),
            request,
        })?;
        Ok(token)
    }

    /// Play or pause.
    pub fn set_pause(&self, paused: bool) -> Result<(), PlayerError> {
        self.set("pause", if paused { "yes" } else { "no" })
    }

    /// Toggle play/pause.
    pub fn toggle_pause(&self) -> Result<(), PlayerError> {
        self.cmd("cycle", &["pause"])
    }

    /// Seek by `seconds` (negative = back).
    pub fn seek_relative(&self, seconds: f64) -> Result<(), PlayerError> {
        self.cmd("seek", &[&format!("{seconds:.3}"), "relative+exact"])
    }

    /// Seek to an absolute position in seconds.
    pub fn seek_absolute(&self, seconds: f64) -> Result<(), PlayerError> {
        self.cmd(
            "seek",
            &[&format!("{:.3}", seconds.max(0.0)), "absolute+exact"],
        )
    }

    /// Set the speed, clamped to 0.25–4.0, through the `speed` property only
    /// (PLY-010).
    pub fn set_speed(&self, speed: f64) -> Result<(), PlayerError> {
        self.set("speed", &format!("{:.2}", control::clamp_speed(speed)))
    }

    /// Set the volume, clamped to 0–150.
    pub fn set_volume(&self, volume: f64) -> Result<(), PlayerError> {
        self.set("volume", &format!("{:.1}", control::clamp_volume(volume)))
    }

    /// Mute or unmute.
    pub fn set_mute(&self, mute: bool) -> Result<(), PlayerError> {
        self.set("mute", if mute { "yes" } else { "no" })
    }

    /// One frame forward or back (pauses).
    pub fn frame_step(&self, forward: bool) -> Result<(), PlayerError> {
        self.cmd(
            if forward {
                "frame-step"
            } else {
                "frame-back-step"
            },
            &[],
        )
    }

    /// Next (+1) or previous (−1) chapter.
    pub fn chapter(&self, delta: i32) -> Result<(), PlayerError> {
        self.cmd("add", &["chapter", &delta.to_string()])
    }

    /// Select an audio track (`None` = no audio).
    pub fn set_aid(&self, aid: Option<i64>) -> Result<(), PlayerError> {
        self.set(
            "aid",
            &aid.map_or_else(|| "no".to_string(), |a| a.to_string()),
        )
    }

    /// Stops playback at `seconds` (or no longer): mpv pauses on that frame,
    /// because `keep-open` turns the end into a pause (SUB-013, cue replay).
    pub fn stop_at(&self, seconds: Option<f64>) -> Result<(), PlayerError> {
        self.set(
            "end",
            &seconds.map_or_else(|| "none".to_string(), |s| format!("{s:.3}")),
        )
    }

    /// Lets mpv draw an embedded graphic subtitle track (PGS, VobSub, DVB), or
    /// none. Text subtitles are never drawn by mpv: Nikomiru draws them
    /// (SPECIFICATION §6.4).
    pub fn show_bitmap_subtitles(&self, id: Option<i64>) -> Result<(), PlayerError> {
        match id {
            Some(id) => {
                self.set("sid", &id.to_string())?;
                self.set("sub-visibility", "yes")
            }
            None => {
                self.set("sid", "no")?;
                self.set("sub-visibility", "no")
            }
        }
    }

    /// Saves the frame shown, without subtitles or OSD, to `path`, in the format
    /// of its extension (PLY-011, ANK-034). The command runs in order with the
    /// others; its result comes on the returned channel, which a worker waits
    /// on, never the UI thread.
    pub fn screenshot(&self, path: &Path) -> Result<CommandReply, PlayerError> {
        let file = path
            .to_str()
            .ok_or_else(|| PlayerError::Path(path.to_path_buf()))?;
        let (reply, result) = mpsc::channel();
        self.send(Command::Reply(
            "screenshot-to-file",
            vec![file.to_string(), "video".to_string()],
            reply,
        ))?;
        Ok(result)
    }

    /// Sets an mpv option now (SET-002, PLY-018), in order with the other
    /// commands: `profile` applies the profile, any other is `set`. Its result
    /// comes on the returned channel, for a worker to wait on.
    pub fn apply_option(
        &self,
        option: &crate::options::MpvOption,
    ) -> Result<CommandReply, PlayerError> {
        let (reply, result) = mpsc::channel();
        let command = if option.key == "profile" {
            Command::Reply("apply-profile", vec![option.value.clone()], reply)
        } else {
            Command::Reply("set", vec![option.key.clone(), option.value.clone()], reply)
        };
        self.send(command)?;
        Ok(result)
    }

    /// The value of a Player settings option before any was set, as mpv
    /// reported it when the core was made.
    pub fn option_default(&self, key: &str) -> Option<String> {
        self.shared.defaults.get(key).cloned()
    }

    /// Stores a string in the core under `user-data/nikomiru/<key>` (for the GUI
    /// harness and the tests, e.g. to check that the core is reused, PLY-009).
    pub fn set_user_data(&self, key: &str, value: &str) -> Result<(), PlayerError> {
        self.set(&format!("user-data/nikomiru/{key}"), value)
    }

    /// Reads a property as a string, synchronously. Only with the `test-support`
    /// feature, for the tests and the GUI harness, and never on the UI thread: the
    /// UI gets its state from events (ARC-003).
    #[cfg(feature = "test-support")]
    pub fn property_string(&self, name: &str) -> Option<String> {
        self.shared.mpv.get_property::<String>(name).ok()
    }
}

/// Runs the queued commands in order until [`Command::Stop`] or the last sender
/// goes. Errors are logged: the caller does not wait for them.
fn command_loop(shared: &Shared, queue: &mpsc::Receiver<Command>, sink: &Sink) {
    let mpv = &shared.mpv;
    for command in queue {
        match command {
            Command::Stop => break,
            Command::Set(name, value) => {
                if let Err(e) = mpv.set_property(&name, value.as_str()) {
                    tracing::warn!(target: "mpv", "setting {name}: {}", mpv_reason(&e));
                }
            }
            Command::Run(name, args) => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                if let Err(e) = mpv.command(name, &args) {
                    tracing::warn!(target: "mpv", "{name}: {}", mpv_reason(&e));
                }
            }
            Command::Reply(name, args, reply) => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let _ = reply.send(mpv.command(name, &args).map_err(PlayerError::from));
            }
            Command::Load {
                token,
                path,
                request,
            } => {
                if let Err(e) = load(shared, token, &path, request) {
                    tracing::error!(target: "mpv", "loadfile: {}", mpv_reason(&e));
                    let code = match e {
                        libmpv2::Error::Raw(code) => code,
                        _ => -13,
                    };
                    sink(Stamped {
                        token,
                        value: PlayerEvent::EndFile(EndFile::Error(PlaybackError::from_code(code))),
                    });
                }
            }
        }
    }
}

/// A load: options (PLY-006), `start` (PLY-007: resume through `start`, never
/// with a seek after loading), then `loadfile`.
fn load(
    shared: &Shared,
    token: LoadToken,
    path: &str,
    request: LoadRequest,
) -> Result<(), libmpv2::Error> {
    let mpv = &shared.mpv;
    for option in &request.options {
        let result = if option.key == "profile" {
            mpv.command("apply-profile", &[&option.value])
        } else {
            mpv.set_property(&option.key, option.value.as_str())
        };
        if let Err(e) = result {
            // Names only: a raw option's value may hold a token (PRV-002).
            tracing::warn!(target: "mpv", "mpv rejected option {}: {}", option.key, mpv_reason(&e));
        }
    }
    let start = request
        .start
        .filter(|s| *s > 0.0)
        .map_or_else(|| "none".to_string(), |s| format!("{s:.3}"));
    mpv.set_property("start", start.as_str())?;
    if let Ok(mut p) = shared.pending.lock() {
        *p = Some((token, request));
    }
    mpv.command("loadfile", &[path, "replace"])
}

fn opt_string(mpv: &Mpv, name: &str) -> Option<String> {
    mpv.get_property::<String>(name)
        .ok()
        .filter(|s| !s.is_empty())
}

fn read_tracks(mpv: &Mpv) -> Vec<Track> {
    let count = mpv.get_property::<i64>("track-list/count").unwrap_or(0);
    (0..count)
        .map(|i| {
            let p = |f: &str| format!("track-list/{i}/{f}");
            Track {
                id: mpv.get_property::<i64>(&p("id")).unwrap_or(0),
                kind: opt_string(mpv, &p("type")).unwrap_or_default(),
                lang: opt_string(mpv, &p("lang")),
                title: opt_string(mpv, &p("title")),
                codec: opt_string(mpv, &p("codec")),
                channels: mpv.get_property::<i64>(&p("demux-channel-count")).ok(),
                ff_index: mpv.get_property::<i64>(&p("ff-index")).ok(),
                selected: mpv.get_property::<bool>(&p("selected")).unwrap_or(false),
                external: mpv.get_property::<bool>(&p("external")).unwrap_or(false),
                external_filename: opt_string(mpv, &p("external-filename")),
            }
        })
        .collect()
}

fn read_chapters(mpv: &Mpv) -> Vec<Chapter> {
    let count = mpv.get_property::<i64>("chapter-list/count").unwrap_or(0);
    (0..count)
        .map(|i| Chapter {
            time: mpv
                .get_property::<f64>(&format!("chapter-list/{i}/time"))
                .unwrap_or(0.0),
            title: opt_string(mpv, &format!("chapter-list/{i}/title")),
        })
        .collect()
}

/// PLY-008: after `file-loaded`, stop mpv from drawing text subtitles (Nikomiru
/// draws them), reset `start`, apply audio track, volume and speed.
fn after_load(mpv: &Mpv, request: &LoadRequest) {
    let _ = mpv.set_property("start", "none");
    let _ = mpv.set_property("sid", "no");
    let _ = mpv.set_property("sub-visibility", "no");
    if let Some(aid) = request.aid {
        let _ = mpv.set_property("aid", aid.to_string().as_str());
    }
    if let Some(v) = request.volume {
        let _ = mpv.set_property(
            "volume",
            format!("{:.1}", control::clamp_volume(v)).as_str(),
        );
    }
    if let Some(s) = request.speed {
        let _ = mpv.set_property("speed", format!("{:.2}", control::clamp_speed(s)).as_str());
    }
}

fn event_loop(client: Mpv, shared: &Shared, sink: &Sink) {
    let origin = Instant::now();
    let mut throttle = Throttle::new(30);
    // Token of the file mpv is playing: it changes on `start-file`, so the last
    // events of the previous file keep the previous token (ARC-002).
    let playing = std::cell::Cell::new(LoadToken::default());
    let emit = |event: PlayerEvent| {
        sink(Stamped {
            token: playing.get(),
            value: event,
        });
    };
    loop {
        let timeout = throttle
            .deadline()
            .map_or(1.0, |d| d.saturating_sub(origin.elapsed()).as_secs_f64());
        let event = client.wait_event(timeout);
        if let Some(t) = throttle.flush(origin.elapsed()) {
            emit(PlayerEvent::TimePos(t));
        }
        match event {
            None => {}
            Some(Ok(Event::Shutdown)) => {
                emit(PlayerEvent::Shutdown);
                break;
            }
            Some(Ok(Event::StartFile)) => {
                if let Some((token, _)) = shared.pending.lock().ok().and_then(|p| p.clone()) {
                    playing.set(token);
                }
            }
            Some(Ok(Event::FileLoaded)) => {
                let pending = shared.pending.lock().ok().and_then(|mut p| p.take());
                if let Some((_, request)) = &pending {
                    after_load(&client, request);
                }
                emit(PlayerEvent::FileLoaded);
                emit(PlayerEvent::Tracks(read_tracks(&client)));
                emit(PlayerEvent::Chapters(read_chapters(&client)));
                // Core-level state (not per file) arrives once, before any load:
                // repeat it with the new token so the UI always has it.
                if let Ok(p) = client.get_property::<bool>("pause") {
                    emit(PlayerEvent::Pause(p));
                }
                if let Ok(s) = client.get_property::<f64>("speed") {
                    emit(PlayerEvent::Speed(s));
                }
                if let Ok(v) = client.get_property::<f64>("volume") {
                    emit(PlayerEvent::Volume(v));
                }
                if let Ok(m) = client.get_property::<bool>("mute") {
                    emit(PlayerEvent::Mute(m));
                }
            }
            Some(Ok(Event::EndFile(reason))) => {
                // mpv_end_file_reason: 0 eof, 2 stop, 3 quit, 4 error, 5 redirect
                let end = if reason == 0 {
                    EndFile::Eof
                } else {
                    EndFile::Stopped
                };
                emit(PlayerEvent::EndFile(end));
            }
            Some(Err(libmpv2::Error::Raw(code))) => {
                // libmpv2 reports an `end-file` with an error as an error event
                emit(PlayerEvent::EndFile(EndFile::Error(
                    PlaybackError::from_code(code),
                )));
            }
            Some(Err(e)) => {
                // a message or a property that is not valid UTF-8: not a playback error
                tracing::debug!(target: "mpv", "unreadable event: {e}");
            }
            Some(Ok(Event::LogMessage {
                prefix,
                level,
                text,
                ..
            })) => log_message(prefix, level, text),
            Some(Ok(Event::PropertyChange { name, change, .. })) => {
                let ev = match (name, change) {
                    ("time-pos", PropertyData::Double(t)) => {
                        if let Some(t) = throttle.push(t, origin.elapsed()) {
                            emit(PlayerEvent::TimePos(t));
                        }
                        None
                    }
                    ("duration", PropertyData::Double(d)) => Some(PlayerEvent::Duration(d)),
                    ("pause", PropertyData::Flag(p)) => Some(PlayerEvent::Pause(p)),
                    ("speed", PropertyData::Double(s)) => Some(PlayerEvent::Speed(s)),
                    ("volume", PropertyData::Double(v)) => Some(PlayerEvent::Volume(v)),
                    ("mute", PropertyData::Flag(m)) => Some(PlayerEvent::Mute(m)),
                    ("track-list/count", PropertyData::Int64(_)) => {
                        Some(PlayerEvent::Tracks(read_tracks(&client)))
                    }
                    ("aid", PropertyData::Str(a)) => Some(PlayerEvent::Aid(a.parse().ok())),
                    ("chapter-list/count", PropertyData::Int64(_)) => {
                        Some(PlayerEvent::Chapters(read_chapters(&client)))
                    }
                    ("eof-reached", PropertyData::Flag(e)) => Some(PlayerEvent::EofReached(e)),
                    ("seeking", PropertyData::Flag(s)) => Some(PlayerEvent::Seeking(s)),
                    ("paused-for-cache", PropertyData::Flag(p)) => {
                        Some(PlayerEvent::PausedForCache(p))
                    }
                    ("video-params/dw", PropertyData::Int64(w)) => Some(PlayerEvent::VideoSize(
                        w,
                        client.get_property::<i64>("video-params/dh").unwrap_or(0),
                    )),
                    ("demuxer-cache-duration", PropertyData::Double(c)) => {
                        Some(PlayerEvent::CacheAhead(c))
                    }
                    _ => None,
                };
                if let Some(ev) = ev {
                    emit(ev);
                }
            }
            Some(Ok(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_core::Localized;

    // T-ERR-001
    #[test]
    fn every_error_has_its_messages() {
        let players = [
            PlayerError::Mpv(libmpv2::Error::Raw(-13)),
            PlayerError::Mpv(libmpv2::Error::InvalidUtf8),
            PlayerError::Thread(std::io::Error::other("x")),
        ];
        let playback: Vec<PlaybackError> = (-20..=0).map(PlaybackError::from_code).collect();
        let mut all: Vec<&dyn Localized> = players.iter().map(|e| e as _).collect();
        all.extend(playback.iter().map(|e| e as &dyn Localized));
        let dir = std::path::Path::new(nikomiru_core::error::I18N_DIR);
        let problems = nikomiru_core::error::message_problems(dir, &all);
        assert!(problems.is_empty(), "{problems:#?}");
        // every named code has its own variant
        for (code, kind) in [
            (-13, PlaybackError::LoadingFailed),
            (-14, PlaybackError::AudioOutput),
            (-15, PlaybackError::VideoOutput),
            (-16, PlaybackError::NothingToPlay),
            (-17, PlaybackError::UnknownFormat),
            (-18, PlaybackError::Unsupported),
        ] {
            assert_eq!(PlaybackError::from_code(code), kind);
        }
        assert_eq!(
            PlayerError::Mpv(libmpv2::Error::Raw(-17)).to_string(),
            "mpv: unrecognized file format"
        );
    }
}
