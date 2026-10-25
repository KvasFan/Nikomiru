// SPDX-License-Identifier: GPL-3.0-only

//! The GUI test harness's connection (TEST §2.1), only with the `test-harness`
//! feature: the test listens on a Unix socket, `nikomiru --test-harness
//! <socket>` connects to it, and each JSON line is a command with a JSON line
//! for answer: `{"ok":true,"result":…}` or `{"ok":false,"error":"…"}`.
//!
//! Commands: `{"cmd":"open","path":…}`, `{"cmd":"wait","event":…,"timeout_ms":…}`,
//! `{"cmd":"key","code":"Space","mods":["Ctrl"],"repeat":0}` (`repeat`: auto-repeats
//! while held), `{"cmd":"drop","paths":[…]}`,
//! `{"cmd":"click_grapheme","layer":"main","cue_index":0,"grapheme_index":1}`,
//! `{"cmd":"popup_button","button":"replay"|"goto"}`, `{"cmd":"lookup","text":…}`,
//! `{"cmd":"popup_content","level":0}`, `{"cmd":"dict_css","id":…,"css":…}`,
//! `{"cmd":"card","level":0,"entry":0,"item":"add-anyway"}` (`item` optional),
//! `{"cmd":"locate","path":…}`, `{"cmd":"anki","action":"save","config":"deck = \"x\""}`,
//! `{"cmd":"online","action":"open"|"search"|"use","title":…,"episode":…,"index":…}`,
//! `{"cmd":"align"}`, `{"cmd":"secondary","key":…,"obscure":…}`, `{"cmd":"player","action":"open"|"import"|"export"|"preview-apply"|"save-preset"|…,"path":…,"name":…}`,
//! `{"cmd":"settings","tab":"general"|"player"|…|"close","save"?:"<toml>"}`,
//! `{"cmd":"word_audio","action":"play"|"menu"|"url","level":0,"entry":0,"url":…}`,
//! `{"cmd":"library","action":"open"|"import"|"series"|"play"|…,"path":…,"grouping":…,"id":…}`,
//! `{"cmd":"services","providers":["opensubtitles",…],"auto":true}`, `{"cmd":"state"}`,
//! `{"cmd":"mpv_get","property":…}`, `{"cmd":"quit"}`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_ui::harness;
use serde_json::{Value, json};

/// Connects to `socket` and serves the commands on a thread.
pub fn start(socket: &Path) {
    let socket = socket.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("harness".into())
        .spawn(move || {
            if let Err(e) = serve(&socket) {
                tracing::error!("test harness: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::error!("test harness thread: {e}");
    }
}

fn serve(socket: &PathBuf) -> std::io::Result<()> {
    let stream = UnixStream::connect(socket)?;
    let mut out = stream.try_clone()?;
    tracing::info!("test harness connected to {}", socket.display());
    for line in BufReader::new(stream).lines() {
        let line = line?;
        let command: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let quit = command["cmd"] == "quit";
        let answer = match run(&command) {
            Ok(result) => json!({ "ok": true, "result": result }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        writeln!(out, "{answer}")?;
        if quit {
            break;
        }
    }
    Ok(())
}

fn text<'a>(command: &'a Value, field: &str) -> Result<&'a str, String> {
    command[field]
        .as_str()
        .ok_or_else(|| format!("missing \"{field}\""))
}

fn run(command: &Value) -> Result<Value, String> {
    match command["cmd"].as_str() {
        Some("open") => harness::open(text(command, "path")?).map(|()| Value::Null),
        Some("wait") => {
            let timeout = Duration::from_millis(command["timeout_ms"].as_u64().unwrap_or(10_000));
            harness::wait(text(command, "event")?, timeout)
        }
        Some("key") => {
            let mods: Vec<String> = command["mods"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect();
            let repeats = command["repeat"].as_u64().unwrap_or(0);
            harness::key(text(command, "code")?, &mods, repeats).map(|()| Value::Null)
        }
        Some("drop") => {
            let paths: Vec<String> = command["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect();
            harness::drop(&paths).map(|()| Value::Null)
        }
        Some("click_grapheme") => {
            let index = |name: &str| {
                command[name]
                    .as_u64()
                    .map(|v| v as usize)
                    .ok_or_else(|| format!("missing \"{name}\""))
            };
            let layer = command["layer"].as_str().unwrap_or("main");
            harness::click_grapheme(layer, index("cue_index")?, index("grapheme_index")?)
                .map(|()| Value::Null)
        }
        Some("popup_button") => {
            harness::popup_button(text(command, "button")?).map(|()| Value::Null)
        }
        Some("lookup") => harness::lookup(text(command, "text")?).map(|()| Value::Null),
        Some("popup_content") => {
            let level = command["level"]
                .as_u64()
                .ok_or_else(|| "missing \"level\"".to_string())?;
            harness::popup_content(level as usize)
        }
        Some("state") => harness::state(),
        Some("dict_css") => {
            harness::dict_css(text(command, "id")?, text(command, "css")?).map(|()| Value::Null)
        }
        Some("card") => {
            let index = |name: &str| {
                command[name]
                    .as_u64()
                    .map(|v| v as usize)
                    .ok_or_else(|| format!("missing \"{name}\""))
            };
            harness::card(index("level")?, index("entry")?, command["item"].as_str())
                .map(|()| Value::Null)
        }
        Some("locate") => harness::locate(text(command, "path")?).map(|()| Value::Null),
        Some("anki") => harness::anki(text(command, "action")?, command["config"].as_str())
            .map(|()| Value::Null),
        Some("align") => harness::align().map(|()| Value::Null),
        Some("settings") => {
            harness::settings(text(command, "tab")?, command["save"].as_str()).map(|()| Value::Null)
        }
        Some("word_audio") => harness::word_audio(
            text(command, "action")?,
            command["level"].as_u64().unwrap_or(0) as usize,
            command["entry"].as_u64().unwrap_or(0) as usize,
            command["url"].as_str().map(str::to_string),
        )
        .map(|()| Value::Null),
        Some("library") => harness::library(
            text(command, "action")?,
            command["path"].as_str().map(str::to_string),
            command["grouping"].as_str().map(str::to_string),
            command["id"].as_u64(),
            command["title"].as_str().map(str::to_string),
        )
        .map(|()| Value::Null),
        Some("services") => harness::services(
            command["providers"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            command["auto"].as_bool(),
        )
        .map(|()| Value::Null),
        Some("secondary") => harness::secondary(
            command["key"].as_str().map(str::to_string),
            command["obscure"].as_str().map(str::to_string),
        )
        .map(|()| Value::Null),
        Some("player") => harness::player(
            text(command, "action")?,
            command["path"].as_str().map(str::to_string),
            command["name"].as_str().map(str::to_string),
            command["raw"].as_str().map(str::to_string),
        )
        .map(|()| Value::Null),
        Some("online") => harness::online(
            text(command, "action")?,
            command["title"].as_str().map(str::to_string),
            command["episode"]
                .as_u64()
                .and_then(|e| u32::try_from(e).ok()),
            command["index"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok()),
        )
        .map(|()| Value::Null),
        Some("mpv_get") => harness::mpv_get(text(command, "property")?),
        Some("quit") => harness::quit().map(|()| Value::Null),
        _ => Err(format!("unknown command {command}")),
    }
}
