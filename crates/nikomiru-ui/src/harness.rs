// SPDX-License-Identifier: GPL-3.0-only

//! The GUI test harness (TEST §2.1), compiled only with the `test-harness`
//! feature: never in release builds (T-SEC-004). Each command runs on the UI
//! thread through the entry points of the user's input: [`Ui::key`] for keys,
//! [`Ui::dropped`] for drops, the overlay's click command for graphemes, the
//! close path for `quit`. Load milestones arrive as events.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::popup::Rect;
use crate::session::UiEvent;
use crate::shortcuts::{Modifiers, evdev_code};
use crate::ui::{Command, Inbound, Key, Ui, Waker};

static EVENTS: Mutex<VecDeque<(&'static str, Value)>> = Mutex::new(VecDeque::new());
static ARRIVED: Condvar = Condvar::new();
static RELAY: Mutex<Option<Arc<dyn Waker>>> = Mutex::new(None);

/// The UI exists: commands can reach it.
pub(crate) fn install(waker: Arc<dyn Waker>) {
    if let Ok(mut relay) = RELAY.lock() {
        *relay = Some(waker);
    }
}

/// The UI is closing: later commands fail.
pub(crate) fn uninstall() {
    if let Ok(mut relay) = RELAY.lock() {
        *relay = None;
    }
}

/// Records a load milestone for [`wait`].
pub(crate) fn push(event: UiEvent) {
    let entry = match event {
        UiEvent::FileLoaded => ("file-loaded", json!({})),
        UiEvent::FirstFrame => ("first-frame", json!({})),
        UiEvent::Error(code) => ("error", json!({ "code": code })),
        UiEvent::DictionaryImported => ("dictionary-imported", json!({})),
        UiEvent::CardCreated(note_id) => ("card-created", json!({ "note_id": note_id })),
        UiEvent::ScreenshotSaved(path) => (
            "screenshot-saved",
            json!({ "path": path.to_string_lossy() }),
        ),
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

/// Runs `f` on the UI thread and returns its result, waiting up to 10 s for
/// the window to exist and up to 10 s for the answer.
fn call<T: Send + 'static>(f: impl FnOnce(&mut Ui) -> T + Send + 'static) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let waker = loop {
        if let Some(waker) = RELAY.lock().ok().and_then(|r| r.clone()) {
            break waker;
        }
        if Instant::now() >= deadline {
            return Err("the window did not appear".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (tx, rx) = mpsc::channel();
    waker.wake(Inbound::Harness(Box::new(move |ui| {
        let _ = tx.send(f(ui));
    })));
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "the UI did not answer".to_string())
}

/// Opens a file as the file dialog does.
pub fn open(path: &str) -> Result<(), String> {
    let path = path.to_string();
    call(move |ui| {
        ui.handle(Inbound::Open(path));
    })
}

/// Drops files on the window, through the drop handler (the first counts).
pub fn drop(paths: &[String]) -> Result<(), String> {
    let path = paths
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| "drop: no paths".to_string())?;
    call(move |ui| ui.dropped(&path))
}

/// Presses and releases a key, given by its W3C `code` name, with modifiers
/// (`Ctrl`, `Shift`, `Alt`), through the window's key handler.
pub fn key(code: &str, mods: &[String], repeats: u64) -> Result<(), String> {
    let evdev = evdev_code(code).ok_or_else(|| format!("unknown key {code}"))?;
    let mut held = Modifiers::default();
    for m in mods {
        match m.as_str() {
            "Shift" => held.shift = true,
            "Ctrl" => held.ctrl = true,
            "Alt" => held.alt = true,
            other => return Err(format!("unknown modifier {other}")),
        }
    }
    call(move |ui| {
        let key = |pressed, repeat| Key {
            evdev,
            mods: held,
            pressed,
            repeat,
        };
        ui.key(key(true, false), false);
        // held: the auto-repeats of the window system
        for _ in 0..repeats {
            ui.key(key(true, true), false);
        }
        ui.key(key(false, false), false);
    })
}

/// Clicks grapheme `grapheme` of cue `cue` (layer `main`, the only one before
/// M7) through the overlay's click command. The anchor is empty, so the popup
/// takes the fallback place.
pub fn click_grapheme(layer: &str, cue: usize, grapheme: usize) -> Result<(), String> {
    if layer != "main" {
        return Err(format!("no subtitle layer {layer}"));
    }
    call(move |ui| {
        let offset = ui
            .session
            .subs
            .cue(cue)
            .and_then(|q| q.graphemes().get(grapheme).map(|g| g.offset))
            .ok_or_else(|| format!("no grapheme {grapheme} in cue {cue}"))?;
        ui.run(Command::GraphemeClicked {
            cue,
            offset,
            anchor: Rect::default(),
        });
        Ok(())
    })?
}

/// The player state as the page shows it.
pub fn state() -> Result<Value, String> {
    call(|ui| {
        let s = &ui.session;
        let v = &s.view;
        let visible: Vec<Value> = s
            .subs
            .active()
            .iter()
            .filter_map(|&i| {
                s.subs
                    .cue(i)
                    .map(|q| json!({ "index": i, "text": q.plain_text }))
            })
            .collect();
        json!({
            "loaded": v.loaded,
            "title": v.title,
            "time_pos": v.time_pos,
            "duration": v.duration,
            "paused": v.paused,
            "volume": v.volume,
            "muted": v.muted,
            "speed": v.speed,
            "aid": v.aid.unwrap_or(0),
            "error": v.error.clone().unwrap_or_default(),
            "toast": ui.view.last_toast,
            "controls_visible": s.controls_visible(),
            "fullscreen": ui.view.fullscreen,
            "subtitles": {
                "source": s.subs.source.as_ref().map(|x| x.key()),
                "encoding": s.subs.encoding,
                "delay_ms": s.subs.timeline().delay_ms,
                "cues": s.subs.timeline().cues().len(),
                "visible": visible,
                "selected": s.subs.selection.map(|(c, o)| json!([c, o])),
                "obscure": s.obscure_now().name(),
                "disabled": s.subs.disabled,
                "highlight": s.subs.highlight.as_ref().map(|(c, r)| json!([c, r.start, r.end])),
            },
            "popups": s.popups.stack().iter().map(|p| json!({
                "query": p.query,
                "entry": p.entry,
                "entries": p.reply.as_ref().map_or(0, |r| r.entries),
                "loading": p.reply.is_none(),
                "empty": p.reply.as_ref().is_some_and(|r| r.empty),
                "cards": p.cards.iter().map(|c| c.name()).collect::<Vec<_>>(),
                "menu": p.menu,
            })).collect::<Vec<_>>(),
            "paused_for_lookup": s.popups.paused_for_lookup(),
            "anki_page": json!({
                "open": ui.view.anki_page_open,
                "busy": s.anki_page.busy,
                "status": s.anki_page.status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
                "decks": s.anki_page.lists.as_ref().map(|l| l.decks.clone()),
                "models": s.anki_page.lists.as_ref().map(|l| l.models.clone()),
                "fields": s.anki_page.lists.as_ref().map(|l| l.fields.clone()),
            }),
            "player_page": json!({
                "open": ui.view.player_page_open,
                "presets": s.player_page.presets,
                "status": s.player_page.status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
                "preview": s.player_page.preview.as_ref().map(|p| json!({
                    "source": p.source,
                    "lines": p.lines.iter().map(|l| json!({
                        "line": l.line, "text": l.text, "status": format!("{:?}", l.status),
                    })).collect::<Vec<_>>(),
                })),
                "set": s.player_set().values,
                "raw": s.player_set().raw,
                "preset": s.player_preset(),
            }),
            "online_page": json!({
                "open": ui.view.online_page_open,
                "busy": s.online_page.busy,
                "status": s.online_page.status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
                "query": s.online_page.query.as_ref().map(|q| json!({
                    "title": q.title, "season": q.season, "episode": q.episode,
                })),
                "results": s.online_page.results.as_ref().map(|r| json!({
                    "files": r.candidates.iter().map(|c| json!({
                        "provider": c.provider.name(), "name": c.file_name,
                        "language": c.language, "episode": c.episode,
                    })).collect::<Vec<_>>(),
                    "failed": r.failed,
                })),
            }),
            "missing_link": s.missing_link.as_ref().map(|m| json!({
                "uid": m.link.uid,
                "path": m.path,
            })),
            "dictionaries": s.dict_page.json(),
        })
    })
}

/// Looks `text` up in a root popup, as a click on a subtitle word does but
/// with no cue behind it; the popup takes the fallback place.
pub fn lookup(text: &str) -> Result<(), String> {
    let text = text.to_string();
    call(move |ui| ui.session.look_up_text(&text))
}

/// The view model of popup `level` (POP-009), as drawn: `null` while its
/// lookup runs.
pub fn popup_content(level: usize) -> Result<Value, String> {
    call(move |ui| {
        let popup = ui
            .session
            .popups
            .stack()
            .get(level)
            .ok_or_else(|| format!("no popup at level {level}"))?;
        match &popup.reply {
            Some(reply) => serde_json::to_value(&*reply.content).map_err(|e| e.to_string()),
            None => Ok(Value::Null),
        }
    })?
}

/// A button of the root popup, like the user's click (POP-015).
pub fn popup_button(button: &str) -> Result<(), String> {
    let command = match button {
        "replay" => Command::PopupReplay,
        "goto" => Command::PopupGoto,
        other => return Err(format!("no popup button {other}")),
    };
    call(move |ui| ui.run(command))
}

/// The card button of entry `entry` of popup `level`, or an item of its
/// menu (`overwrite`, `open-in-anki`, `add-anyway`), like the user's click
/// (ANK-030).
pub fn card(level: usize, entry: usize, item: Option<&str>) -> Result<(), String> {
    let command = match item {
        None => Command::CardButton { level, entry },
        Some(name) => Command::CardMenu {
            level,
            entry,
            item: crate::mining::MenuItem::from_name(name)
                .ok_or_else(|| format!("no card menu item {name}"))?,
        },
    };
    call(move |ui| ui.run(command))
}

/// Answers the file dialog of "Locate the file" with `path`, as the portal
/// does (ANK-052).
pub fn locate(path: &str) -> Result<(), String> {
    let path = PathBuf::from(path);
    call(move |ui| {
        ui.handle(Inbound::Chosen {
            dialog: crate::view::Dialog::Locate,
            path,
        });
    })
}

/// The Player settings page (PLY-016, PLY-018): `open`, `close`;
/// `import` and `export` with the file the dialog would give (`path`);
/// `preview-apply`, `preview-cancel`; `save-preset` and `load-preset` with a
/// `name`; `apply` with the saved set and `raw` as its raw options.
pub fn player(
    action: &str,
    path: Option<String>,
    name: Option<String>,
    raw: Option<String>,
) -> Result<(), String> {
    let action = action.to_string();
    call(move |ui| {
        use crate::view::Dialog;
        let set = ui.session.player_set();
        let command = match action.as_str() {
            "open" => {
                let mut out = Vec::new();
                ui.view.open_player_page(&mut out);
                for c in out {
                    ui.run(c);
                }
                return Ok(());
            }
            "close" => {
                ui.view.close_player_page();
                return Ok(());
            }
            "import" => {
                let path = PathBuf::from(path.ok_or("missing \"path\"")?);
                ui.handle(Inbound::Chosen {
                    dialog: Dialog::MpvImport,
                    path,
                });
                return Ok(());
            }
            "export" => {
                let path = PathBuf::from(path.ok_or("missing \"path\"")?);
                ui.run(Command::PlayerExport(Box::new(set)));
                ui.view.dialog = None;
                ui.handle(Inbound::Chosen {
                    dialog: Dialog::MpvExport,
                    path,
                });
                return Ok(());
            }
            "preview-apply" => Command::PlayerPreviewApply,
            "preview-cancel" => Command::PlayerPreviewCancel,
            "save-preset" => {
                Command::PlayerSavePreset(name.ok_or("missing \"name\"")?, Box::new(set))
            }
            "load-preset" => Command::PlayerLoadPreset(name.ok_or("missing \"name\"")?),
            "apply" => {
                let mut set = set;
                if let Some(raw) = raw {
                    set.raw = raw;
                }
                Command::PlayerApply(Box::new((set, None)))
            }
            other => return Err(format!("no Player page action {other}")),
        };
        ui.run(command);
        Ok(())
    })?
}

/// The Anki settings page (ANK-006, ANK-011): `open` and `close` as its
/// button and Esc, `test`, `create-lapis` and `save` as its buttons, with the
/// settings given (the saved ones changed by `config`, a TOML `[anki]` table).
pub fn anki(action: &str, config: Option<&str>) -> Result<(), String> {
    let action = action.to_string();
    let config = config.map(str::to_string);
    call(move |ui| {
        let mut draft = ui.session.anki_config().clone();
        if let Some(text) = &config {
            let table: nikomiru_core::Config =
                nikomiru_core::Config::from_toml(&format!("[anki]\n{text}"))
                    .map_err(|e| e.to_string())?;
            draft = table.anki;
        }
        let command = match action.as_str() {
            "open" => {
                let mut out = Vec::new();
                ui.view.open_anki_page(&mut out);
                for c in out {
                    ui.run(c);
                }
                return Ok(());
            }
            "close" => {
                ui.view.close_anki_page();
                return Ok(());
            }
            "test" => Command::AnkiTest(draft),
            "create-lapis" => Command::AnkiCreateLapis(draft),
            "save" => Command::AnkiSave(draft),
            other => return Err(format!("no Anki page action {other}")),
        };
        ui.run(command);
        Ok(())
    })?
}

/// The Online subtitles page (MET-009): `open`, `close`, `search` (with the
/// title, season and episode given or as filled), `use` (a result's index).
pub fn online(
    action: &str,
    title: Option<String>,
    episode: Option<u32>,
    index: Option<usize>,
) -> Result<(), String> {
    let action = action.to_string();
    call(move |ui| {
        let command = match action.as_str() {
            "open" => {
                let mut out = Vec::new();
                ui.view.open_online_page(&mut out);
                for c in out {
                    ui.run(c);
                }
                return Ok(());
            }
            "close" => {
                ui.view.close_online_page();
                return Ok(());
            }
            "search" => {
                let filled = ui.session.online_page.query.clone();
                Command::OnlineSearch {
                    title: title
                        .or_else(|| filled.as_ref().map(|q| q.title.clone()))
                        .unwrap_or_default(),
                    season: filled.as_ref().and_then(|q| q.season),
                    episode: episode.or_else(|| filled.as_ref().and_then(|q| q.episode)),
                }
            }
            "use" => Command::OnlineUse(index.ok_or("missing \"index\"")?),
            other => return Err(format!("no Online page action {other}")),
        };
        ui.run(command);
        Ok(())
    })?
}

/// Saves a dictionary's CSS like the Save button of its editor (DIC-035).
pub fn dict_css(id: &str, css: &str) -> Result<(), String> {
    let (id, css) = (id.to_string(), css.to_string());
    call(move |ui| ui.run(Command::DictSetUserCss(id, css)))
}

/// An mpv property, read-only, for metrics. Read on this thread: a
/// synchronous read must never run on the UI thread, which renders the video.
pub fn mpv_get(property: &str) -> Result<Value, String> {
    let player = call(|ui| ui.session.player().clone())?;
    Ok(player
        .property_string(property)
        .map_or(Value::Null, Value::String))
}

/// Closes the window like the user does.
pub fn quit() -> Result<(), String> {
    call(Ui::quit)
}
