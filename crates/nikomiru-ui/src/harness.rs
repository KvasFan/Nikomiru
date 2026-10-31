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
        UiEvent::NextUp => ("next-up", json!({})),
        UiEvent::WordAudio { found } => ("word-audio", json!({ "found": found })),
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
    let name = code.to_string();
    call(move |ui| {
        let key = |pressed, repeat| Key {
            evdev,
            mods: held,
            pressed,
            repeat,
        };
        let taken = ui.key(key(true, false), false);
        // held: the auto-repeats of the window system
        for _ in 0..repeats {
            ui.key(key(true, true), false);
        }
        ui.key(key(false, false), false);
        // what the window gives egui: a key no shortcut took, on a page
        if !taken && (ui.page_open() || held.ctrl) {
            let short = name
                .strip_prefix("Key")
                .or_else(|| name.strip_prefix("Digit"))
                .unwrap_or(&name);
            if let Some(k) = egui::Key::from_name(short) {
                let modifiers = egui::Modifiers {
                    alt: held.alt,
                    ctrl: held.ctrl,
                    shift: held.shift,
                    command: held.ctrl,
                    mac_cmd: false,
                };
                for pressed in [true, false] {
                    ui.inject(egui::Event::Key {
                        key: k,
                        physical_key: Some(k),
                        pressed,
                        repeat: false,
                        modifiers,
                    });
                }
            }
        }
    })
}

/// "Hide" on the library's first steps (LIB-013).
pub fn hide_first_steps() -> Result<(), String> {
    call(|ui| ui.run(Command::HideFirstSteps))
}

/// Moves the pointer to `(x, y)`, in points, as the window would.
pub fn pointer(x: f32, y: f32) -> Result<(), String> {
    call(move |ui| {
        ui.run(Command::MouseMoved);
        ui.inject(egui::Event::PointerMoved(egui::pos2(x, y)));
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
        // the progress bar and the thumbnail under the pointer (PLY-014)
        let bar_progress = ui
            .view
            .bar
            .progress
            .map_or(Value::Null, |p| json!([p.min.x, p.min.y, p.max.x, p.max.y]));
        let thumbnail = s.bar_hovered().map(|at| {
            json!({
                "at_ms": at,
                "size": s.thumbnail(at).map(|(_, px)| json!(px.size)),
            })
        });
        // the words' marks of each cue shown (LKP-051, LKP-061)
        let marks: Vec<Value> = s
            .subs
            .shown(false)
            .map(|c| match s.word_marks(&c.cue.plain_text) {
                Some(m) => json!({
                    "readings": m.readings.iter().map(|(r, t)| json!([r.start, r.end, t])).collect::<Vec<_>>(),
                    "mined": m.mined.iter().map(|r| json!([r.start, r.end])).collect::<Vec<_>>(),
                }),
                None => Value::Null,
            })
            .collect();
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
        // apart from the big json! below, whose macro would recurse too deep
        let results = s.online_page.results.as_ref().map(|r| {
            let files: Vec<Value> = r
                .candidates
                .iter()
                .map(|c| {
                    json!({
                        "provider": c.provider.name(), "name": c.file_name,
                        "language": c.language, "episode": c.episode,
                    })
                })
                .collect();
            json!({ "files": files, "failed": r.failed })
        });
        let online_page = json!({
            "open": ui.view.online_page_open,
            "busy": s.online_page.busy,
            "status": s.online_page.status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
            "query": s.online_page.query.as_ref().map(|q| json!({
                "title": q.title, "season": q.season, "episode": q.episode,
            })),
            "results": results,
        });
        let advanced = s.advanced_card().map(|a| {
            json!({
                "sentence": a.sentence,
                "offset": a.offset,
                "first": a.first,
                "last": a.last,
                "clip": a.clip.map(|(s, e)| [s, e]),
                "frame": format!("{:?}", a.frame),
                "audio": a.audio,
            })
        });
        let settings_status = s
            .settings_status
            .as_ref()
            .map(|(t, ok)| json!({ "text": t, "ok": ok }));
        let look = json!({
            "dark": s.look().dark,
            "accent": s.look().accent,
            "scale": s.look().scale,
            "zoom": ui.view.zoom,
            "language": s.translator().language(),
            "subtitle_px": ui.view.overlay.font_px,
        });
        let series_info = s.library_page.series_info.as_ref().map(|i| {
            json!({
                "source": i.source, "anilist_id": i.anilist_id, "mal_id": i.mal_id,
                "romaji": i.romaji, "native": i.native, "episodes": i.episodes,
            })
        });
        let identify = s.library_page.identify.as_ref().map(|i| {
            let choices: Vec<&str> = i.choices.iter().map(|c| c.romaji.as_str()).collect();
            json!({ "series": i.series, "busy": i.busy, "choices": choices })
        });
        let library_page = {
            let page = &s.library_page;
            let home = page.home.as_ref();
            json!({
                "open": ui.view.library_page_open,
                "busy": page.busy,
                "status": page.status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
                "continue": home.map(|h| h.continue_watching.iter().map(|(u, _)| u.clone()).collect::<Vec<_>>()),
                "series": home.map(|h| h.series.iter().map(|x| json!({
                    "id": x.id, "title": x.record.title, "episodes": x.episodes,
                    "watched": x.watched, "started": x.started,
                })).collect::<Vec<_>>()),
                "folders": home.map(|h| h.folders.iter().map(|(p, f)| json!({
                    "path": p, "grouping": format!("{:?}", f.grouping),
                })).collect::<Vec<_>>()),
                "open_series": page.series.as_ref().map(|(id, eps)| json!({
                    "id": id,
                    "episodes": eps.as_ref().map(|e| e.iter().map(|(u, v)| json!({
                        "uid": u, "path": v.path, "season": v.season, "episode": v.episode,
                    })).collect::<Vec<_>>()),
                })),
                "covers": page.covers.keys().collect::<Vec<_>>(),
                "series_info": series_info,
                "identify": identify,
                "stats": page.stats.as_ref().map(|st| json!({
                    "days": st.days.iter().map(|d| json!({
                        "date": d.date, "ms": d.duration_ms, "chars": d.chars,
                    })).collect::<Vec<_>>(),
                    "sessions": st.sessions.iter().map(|(_, s, name)| json!({
                        "name": name, "ms": s.duration_ms, "chars": s.chars,
                        "lookups": s.lookups, "cards": s.cards,
                    })).collect::<Vec<_>>(),
                })),
            })
        };
        let mut state = json!({
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
                "menu": s.subtitle_menu(),
                "encoding": s.subs.encoding,
                "delay_ms": s.subs.timeline().delay_ms,
                "cues": s.subs.timeline().cues().len(),
                "visible": visible,
                "selected": s.subs.selection.map(|(c, o)| json!([c, o])),
                "obscure": s.obscure_now().name(),
                "disabled": s.subs.disabled,
                "highlight": s.subs.highlight.as_ref().map(|(c, r)| json!([c, r.start, r.end])),
                "marks": marks,
            },
            "secondary": {
                "source": s.secondary.source.as_ref().map(|x| x.key()),
                "cues": s.secondary.timeline().cues().len(),
                "delay_ms": s.secondary.timeline().delay_ms,
                "visible": s.secondary.shown(false).map(|q| q.cue.plain_text.clone()).collect::<Vec<_>>(),
                "obscure": s.secondary_obscure_now().name(),
            },
            "popups": s.popups.stack().iter().map(|p| json!({
                "query": p.query,
                "entry": p.entry,
                "entries": p.reply.as_ref().map_or(0, |r| r.entries),
                "loading": p.reply.is_none(),
                "empty": p.reply.as_ref().is_some_and(|r| r.empty),
                "cards": p.cards.iter().map(|c| c.name()).collect::<Vec<_>>(),
                "menu": p.menu,
                "audio": p.audio.as_ref().map(|(entry, menu)| match menu {
                    crate::word_audio::AudioMenu::Loading => json!({ "entry": entry, "loading": true }),
                    crate::word_audio::AudioMenu::Items(items) => json!({
                        "entry": entry,
                        "items": items.iter().map(|i| json!({ "label": i.label, "url": i.url })).collect::<Vec<_>>(),
                    }),
                }),
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
            "advanced": advanced,
            "settings_tab": ui.view.settings_tab.key(),
            "settings_open": ui.view.settings_open(),
            "settings_status": settings_status,
            "look": look,
            "next_up": s.next_up().map(|(t, n)| json!({ "title": t, "seconds": n })),
            "library_page": library_page,
            "sub_list_open": ui.view.sub_list_open,
            "services_page": json!({
                "open": ui.view.services_page_open,
                "status": s.services_status.as_ref().map(|(t, ok)| json!({ "text": t, "ok": ok })),
                "providers": s.online_config().0.providers,
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
                "import_folder": s.mpv_conf_folder(),
                "raw": s.player_set().raw,
                "preset": s.player_preset(),
            }),
            "online_page": online_page,
            "missing_link": s.missing_link.as_ref().map(|m| json!({
                "uid": m.link.uid,
                "path": m.path,
            })),
            "dictionaries": s.dict_page.json(),
        });
        state["chapters"] = json!(v.chapters.iter().map(|c| c.time).collect::<Vec<_>>());
        state["books"] = json!({
            "list": s.books_page.list.iter().map(|(_, b)| {
                json!({ "title": b.title, "read": b.read_chars, "chapter": b.chapter })
            }).collect::<Vec<_>>(),
            "open": s.books_page.open.as_ref().map(|o| o.book.title.clone()),
            "chapter": s.books_page.open.as_ref().map(|o| o.chapter),
            "status": s.books_page.status.as_ref().map(|(t, _)| t.clone()),
        });
        state["hooker"] = json!({
            "lines": s.hooker_page.lines,
            "connected": s.hooker_page.connected,
        });
        state["bar_progress"] = bar_progress;
        state["thumbnail"] = thumbnail.unwrap_or(Value::Null);
        state["first_steps"] = json!(s.first_steps());
        state
    })
}

/// Looks `text` up in a root popup, as a click on a subtitle word does but
/// with no cue behind it; the popup takes the fallback place.
pub fn lookup(text: &str) -> Result<(), String> {
    let text = text.to_string();
    call(move |ui| {
        // as a click on the video's text, which the Library page covers
        ui.view.close_library_page();
        ui.session.look_up_text(&text);
    })
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
        // Shift+click (ANK-040)
        Some("advanced") => Command::CardAdvanced { level, entry },
        Some(name) => Command::CardMenu {
            level,
            entry,
            item: crate::mining::MenuItem::from_name(name)
                .ok_or_else(|| format!("no card menu item {name}"))?,
        },
    };
    call(move |ui| ui.run(command))
}

/// ♪ of an entry (`action` `play`), its right click (`menu`), or an audio
/// of the menu (`url`) (AUD-004).
pub fn word_audio(
    action: &str,
    level: usize,
    entry: usize,
    url: Option<String>,
) -> Result<(), String> {
    let command = match action {
        "play" => Command::WordAudio { level, entry },
        "menu" => Command::WordAudioMenu { level, entry },
        "url" => Command::WordAudioPlay(url.ok_or("missing \"url\"")?),
        other => return Err(format!("no word audio action {other}")),
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

/// The settings (SET-008): the tab `tab` opened as ⚙ or a tab does, or
/// `close`d. With `save`, TOML laid over the settings in use, the tab saves
/// it as its Save button does (SET-001: General, Subtitles, Lookup,
/// Shortcuts, Advanced).
pub fn settings(tab: &str, save: Option<&str>) -> Result<(), String> {
    let tab = tab.to_string();
    let save = save.map(str::to_string);
    call(move |ui| {
        if tab == "close" {
            ui.view.close_settings();
            return Ok(());
        }
        let tab = crate::view::settings::SettingsTab::from_key(&tab)
            .ok_or_else(|| format!("no settings tab {tab}"))?;
        let mut out = Vec::new();
        ui.view.open_settings(tab, &mut out);
        for c in out {
            ui.run(c);
        }
        if let Some(text) = save {
            let section = tab
                .section()
                .ok_or_else(|| format!("the tab {} saves no section", tab.key()))?;
            let draft = ui.session.config().laid_over(&text)?;
            ui.run(Command::SettingsSave(section, Box::new(draft)));
        }
        Ok(())
    })?
}

/// Saves the Subtitle services tab (MET-011) as its Save button does, with
/// the services `providers` in their order and `auto` for the automatic
/// subtitles, the rest as saved.
pub fn services(providers: Vec<String>, auto: Option<bool>) -> Result<(), String> {
    call(move |ui| {
        let (online, network) = ui.session.online_config();
        let (mut online, network) = (online.clone(), network.clone());
        online.providers = providers;
        if let Some(auto) = auto {
            online.auto_subtitles = auto;
        }
        ui.run(Command::OnlineSave(Box::new((online, network))));
    })
}

/// The Books page (BOK-004…BOK-006): `open`, `add` (`path`), `click`
/// (`paragraph`, `offset`), `next`, `close`.
pub fn books(
    action: &str,
    path: Option<String>,
    paragraph: Option<usize>,
    offset: Option<usize>,
) -> Result<(), String> {
    let action = action.to_string();
    call(move |ui| {
        let mut out = Vec::new();
        match action.as_str() {
            "open" => ui.view.open_books_page(&mut out),
            "add" => ui
                .session
                .book_add(PathBuf::from(path.clone().unwrap_or_default())),
            "next" => out.push(Command::BookChapter(
                ui.session
                    .books_page
                    .open
                    .as_ref()
                    .map_or(0, |o| o.chapter + 1),
            )),
            "click" => out.push(Command::BookClicked {
                paragraph: paragraph.unwrap_or(0),
                offset: offset.unwrap_or(0),
                anchor: crate::popup::Rect::default(),
            }),
            _ => out.push(Command::BookClose),
        }
        for c in out {
            ui.run(c);
        }
    })
}

/// The Texthooker page (HOK-001…HOK-004): `open`, `close`, `click` (`line`,
/// at its first character).
pub fn hooker(action: &str, line: Option<usize>) -> Result<(), String> {
    let action = action.to_string();
    call(move |ui| {
        let mut out = Vec::new();
        match action.as_str() {
            "open" => ui.view.open_hooker_page(&mut out),
            "close" => ui.view.close_hooker_page(&mut out),
            _ => out.push(Command::HookerClicked {
                line: line.unwrap_or(0),
                offset: 0,
                anchor: crate::popup::Rect::default(),
            }),
        }
        for c in out {
            ui.run(c);
        }
    })
}

/// The Library page (LIB-002…LIB-012): `open`, `close`, `import` (`path`,
/// `grouping` `by-series` or `by-folder`), `series` (`id`, none to go
/// back), `play` (`path`), `rename` (`id`, `title`), `delete` (`id`).
pub fn library(
    action: &str,
    path: Option<String>,
    grouping: Option<String>,
    id: Option<u64>,
    title: Option<String>,
) -> Result<(), String> {
    use crate::catalog::LibraryEdit;
    use nikomiru_library::Grouping;
    let action = action.to_string();
    call(move |ui| {
        let path = || path.clone().map(PathBuf::from).ok_or("missing \"path\"");
        let id = || id.ok_or("missing \"id\"");
        let command = match action.as_str() {
            "open" => {
                let mut out = Vec::new();
                ui.view.open_library_page(&mut out);
                for c in out {
                    ui.run(c);
                }
                return Ok(());
            }
            "close" => {
                ui.view.close_library_page();
                return Ok(());
            }
            "statistics" | "library" => {
                ui.view.library.show_statistics(action == "statistics");
                return Ok(());
            }
            "import" => {
                let grouping = match grouping.as_deref() {
                    Some("by-folder") => Grouping::ByFolder,
                    _ => Grouping::BySeries,
                };
                Command::LibraryScan(path()?, grouping)
            }
            "series" => Command::LibrarySeries(id().ok()),
            // MET-001: "Identify online", a result chosen (`id` its index,
            // none without), "Forget"
            "identify" => Command::LibraryIdentify(id()?),
            "choose" => Command::LibraryIdentifyChoose(id().ok().map(|i| i as usize)),
            "forget" => Command::LibraryForget(id()?),
            "play" => Command::LibraryPlay(path()?),
            "rename" => Command::LibraryEdit(LibraryEdit::Rename(
                id()?,
                title.clone().unwrap_or_default(),
            )),
            "delete" => Command::LibraryEdit(LibraryEdit::Delete(id()?)),
            other => return Err(format!("no Library page action {other}")),
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

/// A property of mpv, as text (T-PERF-03: `frame-drop-count`,
/// `vo-delayed-frame-count`); `null` when mpv has none.
pub fn mpv_property(name: &str) -> Result<Option<String>, String> {
    let name = name.to_string();
    call(move |ui| ui.session.player().property_string(&name))
}

/// The pointer moved over the window: the controls show (PLY-004).
pub fn mouse_moved() -> Result<(), String> {
    call(|ui| ui.run(Command::MouseMoved))
}

/// The "Advanced mining" dialog (ANK-040, ANK-041), as its controls do:
/// `previous`, `next`, `drop-previous`, `drop-next`, `start`/`end` with
/// `value` in ms, `frame-click`, `frame-start`, `frame-none`, `audio-on`,
/// `audio-off`, `preview`, `create`, `cancel`.
pub fn advanced(edit: &str, value: Option<i64>) -> Result<(), String> {
    use crate::mining::{AdvancedEdit, FrameKind};
    let ms = || value.ok_or_else(|| "missing \"value\"".to_string());
    let edit = match edit {
        "previous" => AdvancedEdit::Previous(true),
        "drop-previous" => AdvancedEdit::Previous(false),
        "next" => AdvancedEdit::Next(true),
        "drop-next" => AdvancedEdit::Next(false),
        "start" => AdvancedEdit::Start(ms()?),
        "end" => AdvancedEdit::End(ms()?),
        "frame-click" => AdvancedEdit::Frame(FrameKind::Click),
        "frame-start" => AdvancedEdit::Frame(FrameKind::SentenceStart),
        "frame-none" => AdvancedEdit::Frame(FrameKind::None),
        "audio-on" => AdvancedEdit::Audio(true),
        "audio-off" => AdvancedEdit::Audio(false),
        "preview" => AdvancedEdit::Preview,
        "create" => AdvancedEdit::Create,
        "cancel" => AdvancedEdit::Cancel,
        other => return Err(format!("no advanced mining edit {other}")),
    };
    call(move |ui| ui.run(Command::Advanced(edit)))
}

/// The secondary subtitle (SUB-017): a source key or `off`, and the
/// obscuring mode if given.
pub fn secondary(key: Option<String>, obscure: Option<String>) -> Result<(), String> {
    call(move |ui| {
        if let Some(key) = key {
            ui.run(Command::SelectSecondary(key));
        }
        if let Some(mode) = obscure {
            ui.run(Command::SecondaryObscure(
                crate::subtitles::Obscure::from_config(&mode),
            ));
        }
    })
}

/// "Align with the audio" in the subtitle menu (SUB-018).
pub fn align() -> Result<(), String> {
    call(|ui| ui.run(Command::AlignSubtitles))
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
