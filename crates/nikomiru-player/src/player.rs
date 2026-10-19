// SPDX-License-Identifier: GPL-3.0-only

//! The mpv core (ARC-001): a [`Player`] owns the core; [`PlayerHandle`]s send
//! commands; a thread turns mpv events into [`PlayerEvent`]s stamped with the
//! [`LoadToken`] of their load (ARC-002), with `time-pos` throttled (ARC-004).
//!
//! The UI never reads mpv properties synchronously (ARC-003): its state comes
//! from the events. [`PlayerHandle::property_string`] exists for tests and the
//! GUI harness only.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv};

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
    #[error("mpv: {0}")]
    Mpv(#[from] libmpv2::Error),
    /// The events thread could not be started.
    #[error("starting the mpv events thread: {0}")]
    Thread(std::io::Error),
}

/// Where the video goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoOutput {
    /// The libmpv render API, drawn by the Qt Quick video item (the app).
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
    Error(String),
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

/// An option that mpv rejected (shown per line, PLY-006).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionError {
    /// The option.
    pub option: MpvOption,
    /// mpv's message.
    pub message: String,
}

type Sink = Arc<dyn Fn(Stamped<PlayerEvent>) + Send + Sync>;

/// Shared between the handles and the events thread.
struct Shared {
    mpv: Mpv,
    token: AtomicU64,
    pending: Mutex<Option<(LoadToken, LoadRequest)>>,
}

impl Shared {
    fn token(&self) -> LoadToken {
        LoadToken(self.token.load(Ordering::SeqCst))
    }
}

/// Sends commands to the player. Cheap to clone; usable from any thread.
#[derive(Clone)]
pub struct PlayerHandle {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for PlayerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerHandle")
            .field("token", &self.shared.token())
            .finish()
    }
}

/// Owns the mpv core and its events thread.
pub struct Player {
    shared: Arc<Shared>,
    events: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("token", &self.shared.token())
            .finish()
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
    ("video-params/w", Format::Int64),
    ("demuxer-cache-duration", Format::Double),
];

impl Player {
    /// Creates the mpv core with the fixed options (PLY-005) and starts the
    /// events thread; `sink` receives every event.
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
            }
            Ok(())
        })?;
        INSTANCES.fetch_add(1, Ordering::SeqCst);
        // `create_client(None)`: with a name, libmpv2 6.0 passes mpv a dangling
        // pointer (ADR 0001).
        let client = mpv.create_client(None)?;
        for (name, format) in OBSERVED {
            client.observe_property(name, format, 0)?;
        }
        let shared = Arc::new(Shared {
            mpv,
            token: AtomicU64::new(0),
            pending: Mutex::new(None),
        });
        let sink: Sink = Arc::new(sink);
        let thread_shared = shared.clone();
        let events = std::thread::Builder::new()
            .name("mpv-events".into())
            .spawn(move || event_loop(client, &thread_shared, &sink))
            .map_err(PlayerError::Thread)?;
        Ok(Self {
            shared,
            events: Some(events),
        })
    }

    /// A handle for sending commands.
    pub fn handle(&self) -> PlayerHandle {
        PlayerHandle {
            shared: self.shared.clone(),
        }
    }

    /// The raw `mpv_handle`, for the video item's render context. The item MUST
    /// free its render context before [`Player::shutdown`] (SPECIFICATION §5.2.2).
    pub fn raw_handle(&self) -> *mut std::ffi::c_void {
        self.shared.mpv.ctx.as_ptr().cast()
    }

    /// Stops the core in the required order: `quit`, then the join of the events
    /// thread; the core itself goes when the last handle is dropped.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
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
    fn set(&self, name: &str, value: &str) -> Result<(), PlayerError> {
        Ok(self.shared.mpv.set_property(name, value)?)
    }

    fn cmd(&self, name: &str, args: &[&str]) -> Result<(), PlayerError> {
        Ok(self.shared.mpv.command(name, args)?)
    }

    /// The token of the current load.
    pub fn current_token(&self) -> LoadToken {
        self.shared.token()
    }

    /// Loads a file, replacing the current one but keeping the core (PLY-009).
    /// Applies the options first (PLY-006) and returns the ones mpv rejected.
    pub fn load(
        &self,
        path: &str,
        request: LoadRequest,
    ) -> Result<(LoadToken, Vec<OptionError>), PlayerError> {
        let token = LoadToken(self.shared.token.fetch_add(1, Ordering::SeqCst) + 1);
        let mut errors = Vec::new();
        for option in &request.options {
            let result = if option.key == "profile" {
                self.cmd("apply-profile", &[&option.value])
            } else {
                self.set(&option.key, &option.value)
            };
            if let Err(e) = result {
                errors.push(OptionError {
                    option: option.clone(),
                    message: e.to_string(),
                });
            }
        }
        // PLY-007: resume through `start`, never with a seek after loading.
        let start = request
            .start
            .filter(|s| *s > 0.0)
            .map_or_else(|| "none".to_string(), |s| format!("{s:.3}"));
        self.set("start", &start)?;
        if let Ok(mut p) = self.shared.pending.lock() {
            *p = Some((token, request));
        }
        self.cmd("loadfile", &[path, "replace"])?;
        Ok((token, errors))
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

    /// Stores a string in the core under `user-data/nikomiru/<key>` (for the GUI
    /// harness and the tests, e.g. to check that the core is reused, PLY-009).
    pub fn set_user_data(&self, key: &str, value: &str) -> Result<(), PlayerError> {
        self.set(&format!("user-data/nikomiru/{key}"), value)
    }

    /// Reads a property as a string. For tests and the GUI harness only: the UI
    /// gets its state from events (ARC-003).
    pub fn property_string(&self, name: &str) -> Option<String> {
        self.shared.mpv.get_property::<String>(name).ok()
    }
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
            Some(Err(e)) => {
                // libmpv2 reports an `end-file` with an error as an error event
                emit(PlayerEvent::EndFile(EndFile::Error(e.to_string())));
            }
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
                    ("video-params/w", PropertyData::Int64(w)) => Some(PlayerEvent::VideoSize(
                        w,
                        client.get_property::<i64>("video-params/h").unwrap_or(0),
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
