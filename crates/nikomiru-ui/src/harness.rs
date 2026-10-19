// SPDX-License-Identifier: GPL-3.0-only

//! The GUI test harness (TEST §2.1), compiled only with the `test-harness`
//! feature. Commands go through the controller's invokables and the QML key
//! handler, like the user's input; load milestones arrive as events.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde_json::{Value, json};

use crate::bridge::player::harness_call;
use crate::session::UiEvent;
use crate::shortcuts::evdev_code;

static EVENTS: Mutex<VecDeque<(&'static str, Value)>> = Mutex::new(VecDeque::new());
static ARRIVED: Condvar = Condvar::new();

/// Records a load milestone for [`wait`].
pub(crate) fn push(event: UiEvent) {
    let entry = match event {
        UiEvent::FileLoaded => ("file-loaded", json!({})),
        UiEvent::FirstFrame => ("first-frame", json!({})),
        UiEvent::Error(code) => ("error", json!({ "code": code })),
        UiEvent::DictionaryImported => ("dictionary-imported", json!({})),
        UiEvent::PopupRendered {
            level,
            query,
            headwords,
        } => (
            "popup-rendered",
            json!({ "level": level, "query": query, "headwords": headwords }),
        ),
        UiEvent::CueShown { index, time_pos_ms } => (
            "cue-shown",
            json!({ "index": index, "time_pos": time_pos_ms as f64 / 1000.0 }),
        ),
    };
    if let Ok(mut q) = EVENTS.lock() {
        q.push_back(entry);
        ARRIVED.notify_all();
    }
}

/// Waits for `event`, dropping the events before it. An `error` event ends
/// the wait for anything else with a failure.
pub fn wait(event: &str, timeout: Duration) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let mut q = EVENTS.lock().map_err(|_| "poisoned".to_string())?;
    loop {
        while let Some((name, data)) = q.pop_front() {
            if name == event {
                return Ok(data);
            }
            if name == "error" {
                return Err(format!("error while waiting for {event}: {data}"));
            }
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| format!("timeout waiting for {event}"))?;
        q = ARRIVED
            .wait_timeout(q, left)
            .map_err(|_| "poisoned".to_string())?
            .0;
    }
}

/// Opens a file as the file dialog does.
pub fn open(path: &str) -> Result<(), String> {
    let path = path.to_string();
    harness_call(move |c| c.open_file(&QString::from(path.as_str())))
}

/// Drops files on the window, as `file://` URLs, through the QML drop handler.
pub fn drop(paths: &[String]) -> Result<(), String> {
    let url = paths
        .first()
        .map(|p| file_url(p))
        .ok_or_else(|| "drop: no paths".to_string())?;
    harness_call(move |c| c.drop_requested(&QString::from(url.as_str())))
}

/// A `file://` URL for an absolute path, percent-encoded like a desktop drop.
fn file_url(path: &str) -> String {
    let mut url = String::from("file://");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            url.push(char::from(b));
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

/// Presses and releases a key, given by its W3C `code` name, with modifiers
/// (`Ctrl`, `Shift`, `Alt`).
pub fn key(code: &str, mods: &[String]) -> Result<(), String> {
    let evdev = evdev_code(code).ok_or_else(|| format!("unknown key {code}"))?;
    let mut bits = 0u32;
    for m in mods {
        bits |= match m.as_str() {
            "Shift" => 0x0200_0000,
            "Ctrl" => 0x0400_0000,
            "Alt" => 0x0800_0000,
            other => return Err(format!("unknown modifier {other}")),
        };
    }
    // XKB keycodes are evdev codes + 8, as in QKeyEvent::nativeScanCode
    harness_call(move |c| c.key_requested(evdev + 8, bits))
}

/// Clicks grapheme `grapheme` of cue `cue` (layer `main`, the only one before
/// M7), through the overlay's click invokable.
pub fn click_grapheme(layer: &str, cue: usize, grapheme: usize) -> Result<(), String> {
    if layer != "main" {
        return Err(format!("no subtitle layer {layer}"));
    }
    harness_call(move |mut c| {
        let offset = c
            .rust()
            .session
            .as_ref()
            .and_then(|s| s.subs.cue(cue))
            .and_then(|q| q.graphemes().get(grapheme).map(|g| g.offset))
            .ok_or_else(|| format!("no grapheme {grapheme} in cue {cue}"))?;
        c.as_mut()
            .grapheme_clicked(cue as i32, offset as i32, 0.0, 0.0, 0.0, 0.0);
        Ok(())
    })?
}

/// The player state as the page shows it.
pub fn state() -> Result<Value, String> {
    harness_call(|c| {
        json!({
            "loaded": *c.loaded(),
            "title": c.title().to_string(),
            "time_pos": *c.time_pos(),
            "duration": *c.duration(),
            "paused": *c.paused(),
            "volume": *c.volume(),
            "muted": *c.muted(),
            "speed": *c.speed(),
            "aid": *c.aid(),
            "error": c.error_text().to_string(),
            "toast": c.toast().to_string(),
            "controls_visible": *c.controls_visible(),
            "fullscreen": *c.window_fullscreen(),
            "subtitles": c.rust().session.as_ref().map(|s| {
                let visible: Vec<Value> = s
                    .subs
                    .active()
                    .iter()
                    .filter_map(|&i| s.subs.cue(i).map(|q| json!({ "index": i, "text": q.plain_text })))
                    .collect();
                json!({
                    "source": s.subs.source.as_ref().map(|x| x.key()),
                    "encoding": s.subs.encoding,
                    "delay_ms": s.subs.timeline().delay_ms,
                    "cues": s.subs.timeline().cues().len(),
                    "visible": visible,
                    "selected": s.subs.selection.map(|(c, o)| json!([c, o])),
                    "obscure": s.obscure_now().name(),
                    "disabled": s.subs.disabled,
                    "highlight": s.subs.highlight.as_ref().map(|(c, r)| json!([c, r.start, r.end])),
                })
            }),
            "popups": c.rust().session.as_ref().map(|s| {
                s.popups
                    .stack()
                    .iter()
                    .map(|p| json!({
                        "query": p.query,
                        "entry": p.entry,
                        "entries": p.reply.as_ref().map_or(0, |r| r.entries),
                        "loading": p.reply.is_none(),
                        "empty": p.reply.as_ref().is_some_and(|r| r.empty),
                    }))
                    .collect::<Vec<_>>()
            }),
            "paused_for_lookup": c.rust().session.as_ref().map(|s| s.popups.paused_for_lookup()),
            "dictionaries": c.rust().session.as_ref().map(|s| s.dict_page.json()),
        })
    })
}

/// A button of the root popup, like the user's click (POP-015).
pub fn popup_button(button: &str) -> Result<(), String> {
    match button {
        "replay" => harness_call(|c| c.popup_replay()),
        "goto" => harness_call(|c| c.popup_goto()),
        other => Err(format!("no popup button {other}")),
    }
}

/// An mpv property, read-only, for metrics.
pub fn mpv_get(property: &str) -> Result<Value, String> {
    let property = property.to_string();
    harness_call(move |c| {
        c.rust()
            .session
            .as_ref()
            .and_then(|s| s.player().property_string(&property))
            .map_or(Value::Null, Value::String)
    })
}

/// Closes the window like the user does.
pub fn quit() -> Result<(), String> {
    harness_call(|mut c| {
        c.as_mut().shutdown();
        c.quit_requested();
    })
}
