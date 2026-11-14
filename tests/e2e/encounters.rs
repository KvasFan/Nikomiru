// SPDX-License-Identifier: GPL-3.0-only

//! KNW-018: actual playback history excludes passive and paused text.

use super::support::{Desktop, Harness, exit_status, fixture};
use super::time_pos;
use nikomiru_library::Library;
use serde_json::{Value, json};
use std::time::Duration;

fn history(s: &Value) -> &Value {
    &s["library_page"]["stats"]["encounters"]
}
fn stats(h: &mut Harness, count: usize) {
    h.cmd(json!({"cmd":"library", "action":"open"}));
    h.cmd(json!({"cmd":"library", "action":"statistics"}));
    h.state_until("historical words in statistics", |s| {
        history(s).as_array().is_some_and(|rows| {
            if count == 0 {
                rows.is_empty()
            } else {
                rows.len() == 1 && rows[0]["count"] == count
            }
        })
    });
    h.cmd(json!({"cmd":"library", "action":"close"}));
}

// T-KNW-018: paused/ahead-of-time text is never recorded; played new words
// count once per episode, remain historical after a status edit and restart.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn new_words_are_recorded_only_during_playback_and_survive_a_restart() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(
        "[player]\nmpv_options = { ao = \"null\" }\n[subtitles]\nword_colors = true\n",
    );
    let video = desk.path("episode.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let subtitle = video.with_extension("ja.srt");
    std::fs::write(&subtitle, "1\n00:00:02,000 --> 00:00:05,000\n食べる。食べる\n\n2\n00:00:15,000 --> 00:00:17,000\n日本語\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.key("Space");
    h.state_until("paused before the first cue", |s| {
        s["paused"] == true && time_pos(s) < 2.0
    });
    h.state_until("both lines segmented in advance", |s| {
        s["subtitles"]["score"]["words"] == 3
    });
    h.key("ArrowRight");
    h.state_until("seeking within text while paused", |s| {
        s["paused"] == true && time_pos(s) >= 3.0
    });
    stats(&mut h, 0);
    h.key("Space");
    h.state_until("the active text is playing", |s| {
        s["paused"] == false && time_pos(s) > 3.2
    });
    h.key("Space");
    h.state_until("paused after encountering the word", |s| {
        s["paused"] == true
    });
    stats(&mut h, 1);
    // Replay the same words without reaching the merely prefetched second cue.
    h.key("ArrowLeft");
    h.state_until("back before the cue", |s| time_pos(s) < 2.0);
    h.key("Space");
    h.state_until("the word replayed", |s| {
        s["paused"] == false && time_pos(s) > 2.2
    });
    h.key("Space");
    h.state_until("paused after replay", |s| s["paused"] == true);
    stats(&mut h, 1);
    let known = desk.path("known.csv");
    std::fs::write(&known, "word,reading,status\n食べる,たべる,known\n").unwrap();
    h.cmd(json!({"cmd":"words", "action":"import", "path":known}));
    h.state_until("word became known", |s| {
        s["subtitles"]["score"]["known"] == 2
    });
    stats(&mut h, 1);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    {
        let lib = Library::open(&desk.data_file("library.redb")).unwrap();
        let history = lib.word_encounters().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history.values().next().unwrap().len(), 1);
    }
    // The same saved position now has new text, but colours are off.
    desk.write_config(
        "[player]\nmpv_options = { ao = \"null\" }\n[subtitles]\nword_colors = false\n",
    );
    std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:02:00,000\n日本語\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.state_until("new text playing with colours off", |s| {
        s["paused"] == false && time_pos(s) > 0.2
    });
    stats(&mut h, 1);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = Library::open(&desk.data_file("library.redb")).unwrap();
    assert_eq!(
        lib.word_encounters()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .len(),
        1
    );
}

// T-KNW-023: real catalogue details and clear edit, persisted reset rejects old files.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn encounter_history_can_be_inspected_and_cleared_without_losing_word_statuses() {
    let mut desk = Desktop::start();
    let path = desk.data_file("library.redb");
    let old = {
        let lib = Library::open(&path).unwrap();
        lib.record_encounters("video/history", &[("猫".into(), "ねこ".into())], 42)
            .unwrap();
        lib.set_word_status(
            "猫",
            "ねこ",
            nikomiru_library::words::WordMark {
                status: nikomiru_library::words::WordStatus::Known,
                changed_at: 50,
            },
        )
        .unwrap();
        lib.export_state(100).unwrap()
    };
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"library", "action":"open"}));
    h.cmd(json!({"cmd":"library", "action":"statistics"}));
    h.state_until("historical detail projection", |s| {
        s["library_page"]["stats"]["encounter_words"]["video/history"][0]
            == json!(["猫", "ねこ", 42])
    });
    // T-KNW-030: actual worker writes original history without changing known status.
    let exported = desk.path("history.tsv");
    h.cmd(json!({"cmd":"library", "action":"export-encounters", "title":"video/history", "path":exported}));
    h.state_until("full history export", |_| exported.exists());
    assert_eq!(
        std::fs::read_to_string(&exported).unwrap(),
        "expression\treading\tfirst_encounter_ms\n猫\tねこ\t42\n"
    );
    h.cmd(json!({"cmd":"library", "action":"clear-encounters", "path":"video/history"}));
    h.state_until("history cleared", |s| {
        history(s).as_array().is_some_and(Vec::is_empty)
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = Library::open(&path).unwrap();
    lib.apply_state(&old).unwrap();
    assert!(lib.encounter_stats(200).unwrap().is_empty());
    assert_eq!(
        lib.word_statuses().unwrap()["猫\nねこ"].status,
        nikomiru_library::words::WordStatus::Known
    );
    assert!(lib.export_state(100).unwrap().encounter_resets["video/history"] > 0);
}
