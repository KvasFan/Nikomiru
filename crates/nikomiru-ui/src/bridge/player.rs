// SPDX-License-Identifier: GPL-3.0-only

//! `PlayerController`: the QML side of [`crate::session::Session`]. Properties
//! mirror the view state; invokables forward to the session. Player events reach
//! the controller on the Qt thread through [`deliver`].

use std::pin::Pin;
use std::sync::Mutex;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use nikomiru_player::PlayerEvent;
use nikomiru_player::control::Stamped;

use crate::popup::Rect;
use crate::session::{Services, Session, url_to_path};
use crate::shortcuts::Action;

#[cxx_qt::bridge]
pub mod qobject {
    // SAFETY: plain declarations of the Qt string type from cxx-qt-lib.
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, loaded)]
        #[qproperty(QString, title)]
        #[qproperty(f64, time_pos)]
        #[qproperty(f64, duration)]
        #[qproperty(bool, paused)]
        #[qproperty(f64, volume)]
        #[qproperty(bool, muted)]
        #[qproperty(f64, speed)]
        #[qproperty(QString, audio_tracks)]
        #[qproperty(i32, aid)]
        #[qproperty(QString, error_text)]
        #[qproperty(QString, toast)]
        #[qproperty(i32, toast_serial)]
        #[qproperty(bool, controls_visible)]
        #[qproperty(bool, window_fullscreen)]
        #[qproperty(QString, subtitle_cues)]
        #[qproperty(QString, subtitle_list)]
        #[qproperty(i32, subtitle_list_row)]
        #[qproperty(QString, subtitle_menu)]
        #[qproperty(QString, subtitle_key)]
        #[qproperty(QString, subtitle_encoding)]
        #[qproperty(QString, subtitle_encoding_label)]
        #[qproperty(QString, subtitle_forced_encoding)]
        #[qproperty(bool, subtitles_disabled)]
        #[qproperty(QString, subtitle_obscure)]
        #[qproperty(f64, subtitle_font_size)]
        #[qproperty(bool, respect_ass)]
        #[qproperty(i32, subtitle_play_res_y)]
        #[qproperty(i32, video_width)]
        #[qproperty(i32, video_height)]
        #[qproperty(QString, popups)]
        #[qproperty(QString, dict_page)]
        type PlayerController = super::PlayerControllerRust;

        /// Opens a file (path or `file://` URL).
        #[qinvokable]
        fn open_file(self: Pin<&mut PlayerController>, path: &QString);

        /// The video item has its render context.
        #[qinvokable]
        fn renderer_ready(self: Pin<&mut PlayerController>);

        /// A key event; returns the UI effect to apply ("", "fullscreen", "back",
        /// "open_file").
        #[qinvokable]
        fn key_event(
            self: Pin<&mut PlayerController>,
            native_scan_code: u32,
            modifiers: u32,
            pressed: bool,
            auto_repeat: bool,
            text_focus: bool,
        ) -> QString;

        /// Runs an action by name (buttons); returns the UI effect.
        #[qinvokable]
        fn action(self: Pin<&mut PlayerController>, name: &QString) -> QString;

        /// Absolute seek in seconds.
        #[qinvokable]
        fn seek_to(self: Pin<&mut PlayerController>, seconds: f64);

        /// Volume from the slider.
        #[qinvokable]
        fn set_volume_level(self: Pin<&mut PlayerController>, volume: f64);

        /// Audio track from the menu (0 = none).
        #[qinvokable]
        fn select_audio(self: Pin<&mut PlayerController>, id: i32);

        /// The mouse moved (PLY-004).
        #[qinvokable]
        fn mouse_moved(self: Pin<&mut PlayerController>);

        /// Periodic tick from QML: updates the visibility of the controls.
        #[qinvokable]
        fn tick(self: Pin<&mut PlayerController>);

        /// Translated interface string.
        #[qinvokable]
        fn tr(self: &PlayerController, id: &QString) -> QString;

        /// Subtitle menu: a source key, or "off".
        #[qinvokable]
        fn select_subtitles(self: Pin<&mut PlayerController>, key: &QString);

        /// A subtitle file from the dialog or a drop (path or `file://` URL).
        #[qinvokable]
        fn open_subtitle_file(self: Pin<&mut PlayerController>, url: &QString);

        /// Forces a subtitle encoding ("" = automatic, SUB-005).
        #[qinvokable]
        fn force_encoding(self: Pin<&mut PlayerController>, encoding: &QString);

        /// A click on a grapheme of the overlay: cue index, byte offset and
        /// the rectangle of the cue in the window (the popup must not cover it).
        #[qinvokable]
        fn grapheme_clicked(
            self: Pin<&mut PlayerController>,
            cue: i32,
            offset: i32,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        );

        /// A click in the text of a list row, at a QML text position, with the
        /// rectangle of the row.
        #[qinvokable]
        fn list_text_clicked(
            self: Pin<&mut PlayerController>,
            cue: i32,
            position: i32,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        );

        /// A link or a selection in popup `level`: a nested lookup (POP-011/012).
        #[qinvokable]
        fn popup_lookup(
            self: Pin<&mut PlayerController>,
            level: i32,
            text: &QString,
            x: f64,
            y: f64,
            w: f64,
            h: f64,
        );

        /// A click inside popup `level` closes the popups above it.
        #[qinvokable]
        fn popup_focus(self: Pin<&mut PlayerController>, level: i32);

        /// Closes the last popup.
        #[qinvokable]
        fn popup_close(self: Pin<&mut PlayerController>);

        /// QML drew popup `level` with its content.
        #[qinvokable]
        fn popup_rendered(self: Pin<&mut PlayerController>, level: i32);

        /// The pointer moved over a grapheme (POP-003): window position,
        /// Shift, and the cue's rectangle.
        #[qinvokable]
        fn grapheme_hovered(
            self: Pin<&mut PlayerController>,
            cue: i32,
            offset: i32,
            x: f64,
            y: f64,
            shift: bool,
            cue_x: f64,
            cue_y: f64,
            cue_w: f64,
            cue_h: f64,
        );

        /// The pointer entered or left a popup.
        #[qinvokable]
        fn popup_hover(self: Pin<&mut PlayerController>, inside: bool);

        /// "Replay sentence" from the popup (POP-015).
        #[qinvokable]
        fn popup_replay(self: Pin<&mut PlayerController>);

        /// "Go to sentence" from the popup (POP-015).
        #[qinvokable]
        fn popup_goto(self: Pin<&mut PlayerController>);

        /// Next (+1) or previous (-1) entry of the last popup (POP-014).
        #[qinvokable]
        fn popup_entry(self: Pin<&mut PlayerController>, delta: i32);

        /// The dictionaries page opens: the list is read again.
        #[qinvokable]
        fn dict_open(self: Pin<&mut PlayerController>);

        /// A dictionary zip from the dialog or a drop (path or `file://` URL).
        #[qinvokable]
        fn dict_choose(self: Pin<&mut PlayerController>, url: &QString);

        /// The answer to the update offer.
        #[qinvokable]
        fn dict_confirm(self: Pin<&mut PlayerController>, update: bool);

        /// Stops the running import.
        #[qinvokable]
        fn dict_cancel(self: Pin<&mut PlayerController>);

        /// Moves a dictionary up (-1) or down (+1).
        #[qinvokable]
        fn dict_move(self: Pin<&mut PlayerController>, id: &QString, delta: i32);

        /// Enables or disables a dictionary.
        #[qinvokable]
        fn dict_set_enabled(self: Pin<&mut PlayerController>, id: &QString, enabled: bool);

        /// Deletes a dictionary (the page asked for confirmation).
        #[qinvokable]
        fn dict_delete(self: Pin<&mut PlayerController>, id: &QString);

        /// Renames a dictionary; empty restores the title.
        #[qinvokable]
        fn dict_rename(self: Pin<&mut PlayerController>, id: &QString, name: &QString);

        /// Sets a dictionary's popup block state: automatic, expanded, collapsed.
        #[qinvokable]
        fn dict_set_collapse(self: Pin<&mut PlayerController>, id: &QString, state: &QString);

        /// Closes every popup (a click outside).
        #[qinvokable]
        fn popup_close_all(self: Pin<&mut PlayerController>);

        /// Where a popup of the preferred size goes (POP-006): JSON rectangle.
        #[qinvokable]
        fn place_popup(
            self: &PlayerController,
            anchor_x: f64,
            anchor_y: f64,
            anchor_w: f64,
            anchor_h: f64,
            width: f64,
            height: f64,
            window_w: f64,
            window_h: f64,
        ) -> QString;

        /// A click on the time of a list row.
        #[qinvokable]
        fn list_time_clicked(self: Pin<&mut PlayerController>, cue: i32);

        /// The mouse entered or left the subtitles (SUB-014).
        #[qinvokable]
        fn subtitle_hover(self: Pin<&mut PlayerController>, inside: bool);

        /// Cue indices whose text contains `query` (JSON array).
        #[qinvokable]
        fn search_subtitles(self: &PlayerController, query: &QString) -> QString;

        /// A readable name for an encoding (`CP932` → "Japanese — Shift_JIS (CP932)").
        #[qinvokable]
        fn encoding_label(self: &PlayerController, encoding: &QString) -> QString;

        /// The window is closing: save the position.
        #[qinvokable]
        fn shutdown(self: Pin<&mut PlayerController>);

        /// Another launch asked for the window (APP-001).
        #[qsignal]
        #[cxx_name = "activateRequested"]
        fn activate_requested(self: Pin<&mut PlayerController>);

        /// The process was asked to quit (a signal): the position is already saved.
        #[qsignal]
        #[cxx_name = "quitRequested"]
        fn quit_requested(self: Pin<&mut PlayerController>);

        /// A key press and release that QML handles like a real one (GUI test
        /// harness, TEST §2.1).
        #[cfg(feature = "test_harness")]
        #[qsignal]
        #[cxx_name = "keyRequested"]
        fn key_requested(self: Pin<&mut PlayerController>, native_scan_code: u32, modifiers: u32);

        /// URLs dropped on the window, handled by QML like a real drop (GUI
        /// test harness).
        #[cfg(feature = "test_harness")]
        #[qsignal]
        #[cxx_name = "dropRequested"]
        fn drop_requested(self: Pin<&mut PlayerController>, url: &QString);
    }

    impl cxx_qt::Threading for PlayerController {}
    impl cxx_qt::Initialize for PlayerController {}
}

/// Rust state of the controller.
#[derive(Default)]
pub struct PlayerControllerRust {
    loaded: bool,
    title: QString,
    time_pos: f64,
    duration: f64,
    paused: bool,
    volume: f64,
    muted: bool,
    speed: f64,
    audio_tracks: QString,
    aid: i32,
    error_text: QString,
    toast: QString,
    toast_serial: i32,
    controls_visible: bool,
    /// Written by QML: the window is fullscreen (for the harness).
    window_fullscreen: bool,
    subtitle_cues: QString,
    subtitle_list: QString,
    subtitle_list_row: i32,
    subtitle_menu: QString,
    subtitle_key: QString,
    subtitle_encoding: QString,
    subtitle_encoding_label: QString,
    subtitle_forced_encoding: QString,
    subtitles_disabled: bool,
    subtitle_obscure: QString,
    subtitle_font_size: f64,
    respect_ass: bool,
    subtitle_play_res_y: i32,
    video_width: i32,
    video_height: i32,
    popups: QString,
    /// Version of the cue list already sent to QML.
    list_version: Option<u64>,
    /// Version of the popups already sent to QML.
    popup_version: Option<u64>,
    dict_page: QString,
    /// Version of the dictionaries page already sent to QML.
    dict_version: Option<u64>,
    pub(crate) session: Option<Session>,
}

static SERVICES: Mutex<Option<Services>> = Mutex::new(None);
static RELAY: Mutex<Option<cxx_qt::CxxQtThread<qobject::PlayerController>>> = Mutex::new(None);
static PENDING_OPEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Gives the services to the controller that QML will create.
pub fn install(services: Services) {
    if let Ok(mut s) = SERVICES.lock() {
        *s = Some(services);
    }
}

/// Player event sink: forwards to the controller on the Qt thread.
pub fn deliver(event: Stamped<PlayerEvent>) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| c.as_mut().on_event(event));
    }
}

/// Opens a file from outside the UI (command line, second instance, APP-001).
pub fn request_open(path: String) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| c.as_mut().open_file(&QString::from(path.as_str())));
        return;
    }
    if let Ok(mut p) = PENDING_OPEN.lock() {
        p.push(path);
    }
}

/// Brings the window forward, for a second launch (APP-001).
pub fn request_activate() {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(|c| c.activate_requested());
    }
}

/// Saves the position and quits the application, from any thread.
pub fn request_quit() {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(|mut c| {
            c.as_mut().shutdown();
            c.quit_requested();
        });
    }
}

/// Runs `f` on the controller in the Qt thread and returns its result, waiting
/// for the window to exist (GUI test harness).
#[cfg(feature = "test_harness")]
pub(crate) fn harness_call<T: Send + 'static>(
    f: impl FnOnce(Pin<&mut qobject::PlayerController>) -> T + Send + 'static,
) -> Result<T, String> {
    use std::time::{Duration, Instant};
    let (tx, rx) = std::sync::mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut f = Some(f);
    while f.is_some() {
        if let Ok(relay) = RELAY.lock()
            && let Some(thread) = relay.as_ref()
            && let Some(f) = f.take()
        {
            let tx = tx.clone();
            thread
                .queue(move |c| {
                    let _ = tx.send(f(c));
                })
                .map_err(|e| format!("queue: {e:?}"))?;
            break;
        }
        if Instant::now() >= deadline {
            return Err("the window did not appear".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "the UI did not answer".to_string())
}

/// Progress of a dictionary import, back on the Qt thread.
fn deliver_import_progress(generation: u64, progress: nikomiru_dict::import::Progress) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| {
            c.as_mut()
                .with_session(|s| s.dict_progress(generation, progress));
            c.sync();
        });
    }
}

/// The end of a dictionary import, back on the Qt thread.
fn deliver_import_done(generation: u64, result: Result<String, String>) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| {
            c.as_mut()
                .with_session(|s| s.dict_finished(generation, result));
            c.sync();
        });
    }
}

/// A finished lookup, back on the Qt thread.
fn deliver_lookup(generation: u64, result: Result<crate::popup::Reply, String>) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| {
            c.as_mut()
                .with_session(|s| s.lookup_done(generation, result));
            c.sync();
        });
    }
}

/// A finished subtitle load, back on the Qt thread.
fn deliver_subtitles(
    generation: u64,
    source: crate::subtitles::Source,
    result: Result<crate::subtitles::Loaded, String>,
) {
    if let Ok(relay) = RELAY.lock()
        && let Some(thread) = relay.as_ref()
    {
        let _ = thread.queue(move |mut c| {
            c.as_mut()
                .with_session(|s| s.subtitles_loaded(generation, source, result));
            c.sync();
        });
    }
}

fn set_text(current: &QString, new: &str) -> Option<QString> {
    let new = QString::from(new);
    (current != &new).then_some(new)
}

fn audio_json(tracks: &[nikomiru_player::player::Track]) -> String {
    let items: Vec<serde_json::Value> = tracks
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "lang": t.lang,
                "title": t.title,
                "codec": t.codec,
                "channels": t.channels,
            })
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

impl cxx_qt::Initialize for qobject::PlayerController {
    fn initialize(mut self: Pin<&mut Self>) {
        let services = SERVICES.lock().ok().and_then(|mut s| s.take());
        if let Some(services) = services {
            self.as_mut().rust_mut().session = Some(Session::new(services));
        }
        self.as_mut().sync();
        let thread = self.qt_thread();
        if let Ok(mut relay) = RELAY.lock() {
            *relay = Some(thread);
        }
        let pending: Vec<String> = PENDING_OPEN
            .lock()
            .map(|mut p| p.drain(..).collect())
            .unwrap_or_default();
        for path in pending {
            self.as_mut().open_file(&QString::from(path.as_str()));
        }
    }
}

impl qobject::PlayerController {
    fn with_session<T>(self: Pin<&mut Self>, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
        let mut this = self;
        let rust = this.as_mut().rust_mut();
        rust.get_mut().session.as_mut().map(f)
    }

    /// Copies the session's view state into the properties.
    fn sync(mut self: Pin<&mut Self>) {
        let Some((view, toasts, controls)) = self.as_mut().with_session(|s| {
            (
                s.view.clone(),
                std::mem::take(&mut s.toasts),
                s.controls_visible(),
            )
        }) else {
            return;
        };
        if *self.loaded() != view.loaded {
            self.as_mut().set_loaded(view.loaded);
        }
        let title = QString::from(view.title.as_str());
        if self.title() != &title {
            self.as_mut().set_title(title);
        }
        if *self.time_pos() != view.time_pos {
            self.as_mut().set_time_pos(view.time_pos);
        }
        if *self.duration() != view.duration {
            self.as_mut().set_duration(view.duration);
        }
        if *self.paused() != view.paused {
            self.as_mut().set_paused(view.paused);
        }
        if *self.volume() != view.volume {
            self.as_mut().set_volume(view.volume);
        }
        if *self.muted() != view.muted {
            self.as_mut().set_muted(view.muted);
        }
        if *self.speed() != view.speed {
            self.as_mut().set_speed(view.speed);
        }
        let tracks = QString::from(audio_json(&view.audio_tracks).as_str());
        if self.audio_tracks() != &tracks {
            self.as_mut().set_audio_tracks(tracks);
        }
        let aid = view.aid.map_or(0, |a| a as i32);
        if *self.aid() != aid {
            self.as_mut().set_aid(aid);
        }
        let error = QString::from(view.error.as_deref().unwrap_or(""));
        if self.error_text() != &error {
            self.as_mut().set_error_text(error);
        }
        if let Some(last) = toasts.last() {
            self.as_mut().set_toast(QString::from(last.as_str()));
            let serial = self.toast_serial().wrapping_add(1);
            self.as_mut().set_toast_serial(serial);
        }
        if *self.controls_visible() != controls {
            self.as_mut().set_controls_visible(controls);
        }
        self.as_mut().sync_subtitles();
        self.as_mut().sync_popups();
        self.as_mut().sync_dictionaries();
        self.as_mut().forward_events();
    }

    /// Starts the pending imports and copies the dictionaries page.
    fn sync_dictionaries(mut self: Pin<&mut Self>) {
        let imports = self
            .as_mut()
            .with_session(Session::take_dict_imports)
            .unwrap_or_default();
        for import in imports {
            let generation = import.generation;
            self.as_mut().with_session(|s| {
                s.start_dict_import(
                    import,
                    Box::new(move |p| deliver_import_progress(generation, p)),
                    Box::new(move |result| deliver_import_done(generation, result)),
                )
            });
        }
        let seen = self.rust().dict_version;
        if let Some(Some((version, json))) = self.as_mut().with_session(|s| {
            (Some(s.dict_page.version) != seen)
                .then(|| (s.dict_page.version, s.dict_page.json().to_string()))
        }) {
            self.as_mut().rust_mut().dict_version = Some(version);
            self.as_mut().set_dict_page(QString::from(json.as_str()));
        }
    }

    /// Starts the pending lookups and copies the popups.
    fn sync_popups(mut self: Pin<&mut Self>) {
        let lookups = self
            .as_mut()
            .with_session(Session::take_lookups)
            .unwrap_or_default();
        for (generation, text) in lookups {
            self.as_mut().with_session(|s| {
                s.start_lookup(
                    text,
                    Box::new(move |result| deliver_lookup(generation, result)),
                )
            });
        }
        let seen = self.rust().popup_version;
        if let Some(Some((version, json))) = self.as_mut().with_session(|s| {
            (Some(s.popup_version) != seen).then(|| (s.popup_version, s.popups_json().to_string()))
        }) {
            self.as_mut().rust_mut().popup_version = Some(version);
            self.as_mut().set_popups(QString::from(json.as_str()));
        }
    }

    /// Starts the pending subtitle loads and copies the subtitle state.
    fn sync_subtitles(mut self: Pin<&mut Self>) {
        let loads = self
            .as_mut()
            .with_session(Session::take_subtitle_loads)
            .unwrap_or_default();
        for (generation, request) in loads {
            let source = request.source.clone();
            self.as_mut().with_session(|s| {
                s.start_subtitle_load(
                    request,
                    Box::new(move |result| deliver_subtitles(generation, source, result)),
                )
            });
        }
        let seen = self.rust().list_version;
        let Some(state) = self.as_mut().with_session(|s| {
            let (font_size, respect) = s.subtitle_style();
            let pos_ms = (s.view.time_pos * 1000.0) as i64;
            (
                s.subs.overlay(respect).to_string(),
                (Some(s.sub_list_version) != seen)
                    .then(|| (s.sub_list_version, s.subs.list().to_string())),
                s.subs
                    .timeline()
                    .list_position(pos_ms)
                    .map_or(-1, |i| i as i32),
                s.subtitle_menu().to_string(),
                s.subs.source.as_ref().map(|x| x.key()).unwrap_or_default(),
                s.subs.encoding.clone().unwrap_or_default(),
                s.subs
                    .encoding
                    .as_deref()
                    .map(|e| s.encoding_label(e))
                    .unwrap_or_default(),
                s.subs.forced_encoding.clone().unwrap_or_default(),
                s.subs.disabled,
                s.obscure_now().name(),
                font_size,
                respect,
                s.subs.play_res().map_or(0, |(_, y)| y as i32),
                s.view.video_size,
            )
        }) else {
            return;
        };
        let (
            cues,
            list,
            row,
            menu,
            key,
            encoding,
            encoding_label,
            forced,
            disabled,
            obscure,
            font_size,
            respect,
            res_y,
            (vw, vh),
        ) = state;
        if let Some(v) = set_text(self.subtitle_cues(), &cues) {
            self.as_mut().set_subtitle_cues(v);
        }
        if let Some((version, list)) = list {
            self.as_mut().rust_mut().list_version = Some(version);
            self.as_mut()
                .set_subtitle_list(QString::from(list.as_str()));
        }
        if *self.subtitle_list_row() != row {
            self.as_mut().set_subtitle_list_row(row);
        }
        if let Some(v) = set_text(self.subtitle_menu(), &menu) {
            self.as_mut().set_subtitle_menu(v);
        }
        if let Some(v) = set_text(self.subtitle_key(), &key) {
            self.as_mut().set_subtitle_key(v);
        }
        if let Some(v) = set_text(self.subtitle_encoding(), &encoding) {
            self.as_mut().set_subtitle_encoding(v);
        }
        if let Some(v) = set_text(self.subtitle_encoding_label(), &encoding_label) {
            self.as_mut().set_subtitle_encoding_label(v);
        }
        if let Some(v) = set_text(self.subtitle_forced_encoding(), &forced) {
            self.as_mut().set_subtitle_forced_encoding(v);
        }
        if *self.subtitles_disabled() != disabled {
            self.as_mut().set_subtitles_disabled(disabled);
        }
        if let Some(v) = set_text(self.subtitle_obscure(), obscure) {
            self.as_mut().set_subtitle_obscure(v);
        }
        if *self.subtitle_font_size() != font_size {
            self.as_mut().set_subtitle_font_size(font_size);
        }
        if *self.respect_ass() != respect {
            self.as_mut().set_respect_ass(respect);
        }
        if *self.subtitle_play_res_y() != res_y {
            self.as_mut().set_subtitle_play_res_y(res_y);
        }
        if *self.video_width() != vw as i32 {
            self.as_mut().set_video_width(vw as i32);
        }
        if *self.video_height() != vh as i32 {
            self.as_mut().set_video_height(vh as i32);
        }
    }

    fn select_subtitles(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        self.as_mut().with_session(|s| s.select_subtitles(&key));
        self.sync();
    }

    fn open_subtitle_file(mut self: Pin<&mut Self>, url: &QString) {
        let path = url_to_path(&url.to_string());
        self.as_mut().with_session(|s| s.open_subtitle_file(&path));
        self.sync();
    }

    fn force_encoding(mut self: Pin<&mut Self>, encoding: &QString) {
        let encoding = encoding.to_string();
        self.as_mut().with_session(|s| s.force_encoding(&encoding));
        self.sync();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn grapheme_clicked(
        mut self: Pin<&mut Self>,
        cue: i32,
        offset: i32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) {
        if let (Ok(cue), Ok(offset)) = (usize::try_from(cue), usize::try_from(offset)) {
            let anchor = Rect { x, y, w, h };
            self.as_mut()
                .with_session(|s| s.select_grapheme(cue, offset, anchor));
        }
        self.sync();
    }

    #[allow(clippy::too_many_arguments)]
    fn list_text_clicked(
        mut self: Pin<&mut Self>,
        cue: i32,
        position: i32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) {
        if let (Ok(cue), Ok(position)) = (usize::try_from(cue), usize::try_from(position)) {
            let anchor = Rect { x, y, w, h };
            self.as_mut()
                .with_session(|s| s.select_in_list(cue, position, anchor));
        }
        self.sync();
    }

    #[allow(clippy::too_many_arguments)]
    fn popup_lookup(
        mut self: Pin<&mut Self>,
        level: i32,
        text: &QString,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) {
        if let Ok(level) = usize::try_from(level) {
            let text = text.to_string();
            let anchor = Rect { x, y, w, h };
            self.as_mut()
                .with_session(|s| s.popup_lookup(level, &text, anchor));
        }
        self.sync();
    }

    fn popup_focus(mut self: Pin<&mut Self>, level: i32) {
        if let Ok(level) = usize::try_from(level) {
            self.as_mut().with_session(|s| s.popup_focus(level));
        }
        self.sync();
    }

    #[allow(clippy::too_many_arguments)]
    fn grapheme_hovered(
        mut self: Pin<&mut Self>,
        cue: i32,
        offset: i32,
        x: f64,
        y: f64,
        shift: bool,
        cue_x: f64,
        cue_y: f64,
        cue_w: f64,
        cue_h: f64,
    ) {
        if let (Ok(cue), Ok(offset)) = (usize::try_from(cue), usize::try_from(offset)) {
            let anchor = Rect {
                x: cue_x,
                y: cue_y,
                w: cue_w,
                h: cue_h,
            };
            self.as_mut()
                .with_session(|s| s.grapheme_hovered(cue, offset, x, y, shift, anchor));
        }
        self.sync();
    }

    fn popup_hover(mut self: Pin<&mut Self>, inside: bool) {
        self.as_mut().with_session(|s| s.popup_hover(inside));
    }

    pub(crate) fn popup_replay(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::popup_replay);
        self.sync();
    }

    pub(crate) fn popup_goto(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::popup_goto);
        self.sync();
    }

    fn popup_entry(mut self: Pin<&mut Self>, delta: i32) {
        self.as_mut()
            .with_session(|s| s.popup_entry(i64::from(delta)));
        self.sync();
    }

    fn popup_rendered(mut self: Pin<&mut Self>, level: i32) {
        if let Ok(level) = usize::try_from(level) {
            self.as_mut().with_session(|s| s.popup_rendered(level));
        }
        self.sync();
    }

    fn dict_open(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::dict_refresh);
        self.sync();
    }

    fn dict_choose(mut self: Pin<&mut Self>, url: &QString) {
        let path = url_to_path(&url.to_string());
        self.as_mut().with_session(|s| s.dict_choose(path));
        self.sync();
    }

    fn dict_confirm(mut self: Pin<&mut Self>, update: bool) {
        self.as_mut().with_session(|s| s.dict_confirm(update));
        self.sync();
    }

    fn dict_cancel(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::dict_cancel);
        self.sync();
    }

    fn dict_move(mut self: Pin<&mut Self>, id: &QString, delta: i32) {
        let id = id.to_string();
        self.as_mut().with_session(|s| s.dict_move(&id, delta));
        self.sync();
    }

    fn dict_set_enabled(mut self: Pin<&mut Self>, id: &QString, enabled: bool) {
        let id = id.to_string();
        self.as_mut()
            .with_session(|s| s.dict_set_enabled(&id, enabled));
        self.sync();
    }

    fn dict_delete(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        self.as_mut().with_session(|s| s.dict_delete(&id));
        self.sync();
    }

    fn dict_rename(mut self: Pin<&mut Self>, id: &QString, name: &QString) {
        let (id, name) = (id.to_string(), name.to_string());
        self.as_mut().with_session(|s| s.dict_rename(&id, &name));
        self.sync();
    }

    fn dict_set_collapse(mut self: Pin<&mut Self>, id: &QString, state: &QString) {
        let (id, state) = (id.to_string(), state.to_string());
        self.as_mut()
            .with_session(|s| s.dict_set_collapse(&id, &state));
        self.sync();
    }

    fn popup_close(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::close_popup);
        self.sync();
    }

    fn popup_close_all(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::close_all_popups);
        self.sync();
    }

    #[allow(clippy::too_many_arguments)]
    fn place_popup(
        &self,
        anchor_x: f64,
        anchor_y: f64,
        anchor_w: f64,
        anchor_h: f64,
        width: f64,
        height: f64,
        window_w: f64,
        window_h: f64,
    ) -> QString {
        let anchor = Rect {
            x: anchor_x,
            y: anchor_y,
            w: anchor_w,
            h: anchor_h,
        };
        let r = crate::popup::place(anchor, (width, height), (window_w, window_h), 8.0);
        QString::from(
            serde_json::json!({"x": r.x, "y": r.y, "w": r.w, "h": r.h})
                .to_string()
                .as_str(),
        )
    }

    fn list_time_clicked(mut self: Pin<&mut Self>, cue: i32) {
        if let Ok(cue) = usize::try_from(cue) {
            self.as_mut().with_session(|s| s.seek_to_cue(cue));
        }
    }

    fn subtitle_hover(mut self: Pin<&mut Self>, inside: bool) {
        self.as_mut().with_session(|s| s.subtitle_hover(inside));
        self.sync();
    }

    fn encoding_label(&self, encoding: &QString) -> QString {
        let encoding = encoding.to_string();
        let label = self
            .rust()
            .session
            .as_ref()
            .map_or_else(|| encoding.clone(), |s| s.encoding_label(&encoding));
        QString::from(label.as_str())
    }

    fn search_subtitles(&self, query: &QString) -> QString {
        let hits = self
            .rust()
            .session
            .as_ref()
            .map(|s| s.subs.timeline().search(&query.to_string()))
            .unwrap_or_default();
        QString::from(serde_json::json!(hits).to_string().as_str())
    }

    fn on_event(mut self: Pin<&mut Self>, event: Stamped<PlayerEvent>) {
        let changed = self.as_mut().with_session(|s| s.on_event(event)) == Some(true);
        self.as_mut().forward_events();
        if changed {
            self.sync();
        }
    }

    /// Hands the session's milestones to the test harness (dropped otherwise).
    fn forward_events(mut self: Pin<&mut Self>) {
        let events = self
            .as_mut()
            .with_session(|s| std::mem::take(&mut s.events))
            .unwrap_or_default();
        #[cfg(feature = "test_harness")]
        for e in events {
            crate::harness::push(e);
        }
        #[cfg(not(feature = "test_harness"))]
        drop(events);
    }

    pub(crate) fn open_file(mut self: Pin<&mut Self>, path: &QString) {
        let path = url_to_path(&path.to_string());
        self.as_mut().with_session(|s| s.open(&path));
        self.sync();
    }

    fn renderer_ready(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::renderer_ready);
        self.sync();
    }

    fn key_event(
        mut self: Pin<&mut Self>,
        native_scan_code: u32,
        modifiers: u32,
        pressed: bool,
        auto_repeat: bool,
        text_focus: bool,
    ) -> QString {
        let effect = self.as_mut().with_session(|s| {
            s.key(
                native_scan_code,
                modifiers,
                pressed,
                auto_repeat,
                text_focus,
            )
        });
        self.sync();
        QString::from(effect.map_or("", |e| e.name()))
    }

    fn action(mut self: Pin<&mut Self>, name: &QString) -> QString {
        let Some(action) = Action::from_name(&name.to_string()) else {
            return QString::default();
        };
        let effect = self.as_mut().with_session(|s| s.action(action));
        self.sync();
        QString::from(effect.map_or("", |e| e.name()))
    }

    fn seek_to(mut self: Pin<&mut Self>, seconds: f64) {
        self.as_mut().with_session(|s| s.seek_to(seconds));
    }

    fn set_volume_level(mut self: Pin<&mut Self>, volume: f64) {
        self.as_mut().with_session(|s| s.set_volume(volume));
    }

    fn select_audio(mut self: Pin<&mut Self>, id: i32) {
        self.as_mut()
            .with_session(|s| s.select_audio(i64::from(id)));
    }

    fn mouse_moved(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::mouse_moved);
        self.sync();
    }

    fn tick(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::tick);
        self.sync();
    }

    fn tr(&self, id: &QString) -> QString {
        let text = self.rust().session.as_ref().map_or_else(
            || id.to_string(),
            |s| s.translator().tr(&id.to_string(), &[]),
        );
        QString::from(text.as_str())
    }

    pub(crate) fn shutdown(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::flush_position);
        if let Ok(mut relay) = RELAY.lock() {
            *relay = None;
        }
    }
}
