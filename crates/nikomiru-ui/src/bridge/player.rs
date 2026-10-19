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

        /// The window is closing: save the position.
        #[qinvokable]
        fn shutdown(self: Pin<&mut PlayerController>);
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
    session: Option<Session>,
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
    }

    fn on_event(mut self: Pin<&mut Self>, event: Stamped<PlayerEvent>) {
        if self.as_mut().with_session(|s| s.on_event(event)) == Some(true) {
            self.sync();
        }
    }

    fn open_file(mut self: Pin<&mut Self>, path: &QString) {
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

    fn tick(self: Pin<&mut Self>) {
        self.sync();
    }

    fn tr(&self, id: &QString) -> QString {
        let text = self.rust().session.as_ref().map_or_else(
            || id.to_string(),
            |s| s.translator().tr(&id.to_string(), &[]),
        );
        QString::from(text.as_str())
    }

    fn shutdown(mut self: Pin<&mut Self>) {
        self.as_mut().with_session(Session::flush_position);
        if let Ok(mut relay) = RELAY.lock() {
            *relay = None;
        }
    }
}
