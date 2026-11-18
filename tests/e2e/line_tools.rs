// SPDX-License-Identifier: GPL-3.0-only

//! M121…M125 in the real app: subtitles read from the speech by a stand-in
//! whisper-cli, the line translated by a fake DeepL and explained by a fake
//! Claude, shadowed through ffmpeg's test microphone, and text read from the
//! picture and looked up.
use super::support::{Desktop, exit_status, fixture};
use nikomiru_online::fake::FakeOnline;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

/// A whisper-cli that writes fixed lines: two for a video's audio, one for
/// a recording; it reports its progress as the real one does.
const WHISPER: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "whisper.cpp version: 1.9.5"; exit 0; fi
base=""; input=""
while [ $# -gt 0 ]; do
  case "$1" in -of) base="$2"; shift ;; -f) input="$2"; shift ;; esac
  shift
done
[ -s "$input" ] || { echo "no audio" >&2; exit 1; }
echo "whisper_print_progress_callback: progress =  50%" >&2
case "$input" in
  *recording*) printf '1\n00:00:00,000 --> 00:00:02,000\nこんにちわ\n\n' > "$base.srt" ;;
  *) printf '1\n00:00:00,000 --> 00:00:20,000\nこんにちは\n\n2\n00:00:20,000 --> 00:00:29,000\n行ってきます\n\n' > "$base.srt" ;;
esac
echo "whisper_print_progress_callback: progress = 100%" >&2
"#;

// T-SUB-062, T-TRN-001, T-TRN-002, T-AIX-001, T-AIX-002, T-SHD-001,
// T-SHD-002, T-OCR-001, T-PRV-001
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn a_line_is_heard_translated_explained_shadowed_and_read() {
    let online = FakeOnline::start().unwrap();
    online
        .data()
        .translations
        .insert("こんにちは".into(), "Hello".into());
    online.data().explanation = "こんにちは: a greeting.".into();

    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let whisper = desk.path("bin/whisper-cli");
    std::fs::create_dir_all(whisper.parent().unwrap()).unwrap();
    std::fs::write(&whisper, WHISPER).unwrap();
    std::fs::set_permissions(&whisper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let model = desk.path("model.bin");
    std::fs::write(&model, b"stand-in model").unwrap();
    desk.write_config(&format!(
        "[player]\nmpv_options = {{ ao = \"null\" }}\n\
         [online]\ndeepl_key = \"deepl-key:fx\"\ndeepl_address = \"{}\"\n\
         claude_key = \"claude-key\"\nclaude_address = \"{}\"\n\
         [paths]\nwhisper = \"{}\"\n\
         [speech]\nmodel = \"{}\"\nmicrophone = \"lavfi:sine=frequency=440:duration=10\"\n",
        online.deepl_url(),
        online.claude_url(),
        whisper.display(),
        model.display(),
    ));
    let video = desk.path("talk.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();

    let (mut app, mut h) = desk.launch();
    h.open(video.to_str().unwrap());
    h.wait("first-frame");
    h.key("Space");
    h.state_until("paused", |s| s["paused"] == true);

    // SUB-062: read from the speech, kept with the video's subtitles, shown
    h.cmd(json!({ "cmd": "line", "action": "speech" }));
    let s = h.state_until("the speech read", |s| {
        s["speech"]["running"] == false
            && s["subtitles"]["source"]
                .as_str()
                .is_some_and(|k| k.contains("whisper"))
            && s["subtitles"]["cues"] == 2
    });
    let source = s["subtitles"]["source"]
        .as_str()
        .unwrap()
        .trim_start_matches("file:")
        .to_string();
    assert!(
        source.starts_with(desk.path("data").to_str().unwrap()),
        "{source}"
    );
    assert!(source.ends_with("talk.whisper.ja.srt"), "{source}");
    assert!(
        std::fs::read_to_string(&source)
            .unwrap()
            .contains("行ってきます")
    );
    // nothing went to the online services by itself
    assert!(online.requests().is_empty(), "{:?}", online.requests());

    // the panel holds the current line and its neighbour
    h.cmd(json!({ "cmd": "line", "action": "open" }));
    let s = h.state_until("the line", |s| s["line_tools"]["text"] == "こんにちは");
    assert_eq!(s["line_tools"]["after"], "行ってきます", "{s}");

    // TRN-001: DeepL, into the interface's language
    h.cmd(json!({ "cmd": "line", "action": "translate" }));
    h.state_until("translated", |s| {
        s["line_tools"]["translation"]["text"] == "Hello"
    });
    let requests = online.requests();
    let deepl = requests
        .iter()
        .find(|r| r.target.ends_with("/v2/translate"))
        .unwrap();
    let body: Value = serde_json::from_str(&deepl.body).unwrap();
    assert_eq!(body["target_lang"], "EN-US");
    assert_eq!(body["text"][0], "こんにちは");

    // AIX-001: Claude, with the line and its neighbour
    h.cmd(json!({ "cmd": "line", "action": "explain" }));
    h.state_until("explained", |s| {
        s["line_tools"]["explanation"]["text"] == "こんにちは: a greeting."
    });
    let requests = online.requests();
    let claude = requests
        .iter()
        .find(|r| r.target.ends_with("/v1/messages"))
        .unwrap();
    assert_eq!(claude.header("x-api-key"), Some("claude-key"));
    let body: Value = serde_json::from_str(&claude.body).unwrap();
    assert_eq!(body["model"], "claude-opus-5-5");
    let asked = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        asked.contains("こんにちは") && asked.contains("行ってきます"),
        "{asked}"
    );
    assert!(
        body["system"]
            .as_str()
            .unwrap()
            .contains("Write in English")
    );

    // SHD-001: recorded, read, and compared with the line
    h.cmd(json!({ "cmd": "line", "action": "shadow" }));
    let s = h.state_until("shadowed", |s| s["line_tools"]["shadow"]["score"].is_u64());
    assert_eq!(s["line_tools"]["shadow"]["said"], "こんにちわ", "{s}");
    assert_eq!(s["line_tools"]["shadow"]["score"], 80, "{s}");
    assert!(
        std::path::Path::new(s["line_tools"]["shadow"]["file"].as_str().unwrap()).is_file(),
        "{s}"
    );
    assert_eq!(s["paused"], true);

    // OCR-001: a part of the picture read, then looked up as a hooked line
    h.cmd(json!({ "cmd": "line", "action": "read" }));
    h.state_until("choosing", |s| s["line_tools"]["reading"] == "choosing");
    h.cmd(json!({ "cmd": "line", "action": "area", "area": [0.1, 0.6, 0.8, 0.3] }));
    let s = h.state_until("read from the picture", |s| {
        s["line_tools"]["text"] == "明日も食べました" && s["line_tools"]["reading"] == "idle"
    });
    assert!(s["line_tools"]["hooked"].is_u64(), "{s}");
    assert_eq!(s["line_tools"]["translation"], Value::Null, "{s}");
    assert!(
        s["hooker"]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "明日も食べました"),
        "{s}"
    );
    h.cmd(json!({ "cmd": "line", "action": "click", "offset": 0 }));
    h.state_until("looked up", |s| {
        s["popups"]
            .as_array()
            .and_then(|p| p.first())
            .and_then(|p| p["query"].as_str())
            .is_some_and(|q| q.starts_with("明日"))
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
