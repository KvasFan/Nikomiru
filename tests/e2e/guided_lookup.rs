// SPDX-License-Identifier: GPL-3.0-only

//! M109: original paused lookup targets and physical-key secondary-track visibility.
use super::support::{Desktop, Harness, exit_status, fixture};
use nikomiru_library::words::{WordMark, WordStatus};
use serde_json::json;
use std::time::Duration;
fn source(desk: &Desktop, h: &mut Harness) -> String {
    let video = desk.path("guided.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let primary = desk.path("guided.ja.srt");
    std::fs::write(
        &primary,
        "1\n00:00:00,000 --> 00:00:29,000\nかな猫と犬と食べました。\n",
    )
    .unwrap();
    h.open(video.to_str().unwrap());
    h.wait("first-frame");
    h.drop_files(&[primary.to_str().unwrap()]);
    h.state_until("original primary text", |s| {
        s["subtitles"]["cues"] == 1
            && s["subtitles"]["source"] == format!("file:{}", primary.display())
    });
    primary.to_str().unwrap().into()
}
fn down(h: &mut Harness, repeat: bool) {
    h.cmd(json!({"cmd":"key-event","code":"KeyT","pressed":true,"repeat":repeat}));
}
fn up(h: &mut Harness) {
    h.cmd(
        json!({"cmd":"key-event","code":"KeyT","pressed":false,"mods":["Shift"],"text_focus":true}),
    );
}
// T-GDL-001, T-GDL-002, T-GDL-004: real segmentation skips known and learning identities without mutating them or seeking.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn paused_lookup_uses_exact_unknown_words_and_an_independent_kanji_rule() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let mut initial = vec![
        (
            "猫".into(),
            "ねこ".into(),
            WordMark {
                status: WordStatus::Known,
                changed_at: 10,
            },
        ),
        (
            "犬".into(),
            "いぬ".into(),
            WordMark {
                status: WordStatus::Learning,
                changed_at: 20,
            },
        ),
    ];
    initial.push((
        "と".into(),
        "と".into(),
        WordMark {
            status: WordStatus::Ignored,
            changed_at: 30,
        },
    ));
    let library_path = desk.path("data/nikomiru/library.redb");
    {
        let lib = nikomiru_library::Library::open(&library_path).unwrap();
        lib.set_word_statuses(&initial).unwrap();
    }
    desk.write_config("[player]\nmpv_options={ao='null'}\n[lookup]\npause_lookup='unknown'\n[shortcuts]\nlookup_first_kanji=['KeyK']\n");
    let (mut app, mut h) = desk.launch();
    source(&desk, &mut h);
    h.state_until("playing original cue", |s| {
        s["paused"] == false && !s["subtitles"]["visible"].as_array().unwrap().is_empty()
    });
    let before = h.state();
    h.key("Space");
    let state = h.state_until("first exact unknown inflected verb", |s| {
        s["paused"] == true
            && s["popups"][0]["query"] == "食べました"
            && s["popups"][0]["loading"] == false
    });
    assert!(
        (state["time_pos"].as_f64().unwrap() - before["time_pos"].as_f64().unwrap()).abs() < 0.6
    );
    h.key("Escape");
    h.state_until("guided popup leaves original pause", |s| {
        s["paused"] == true && s["popups"].as_array().unwrap().is_empty()
    });
    // Applying a different optional rule does not itself look up or change any word status.
    h.cmd(json!({"cmd":"settings","tab":"lookup","save":"[lookup]\npause_lookup='kanji'\n"}));
    h.key("Escape");
    h.key("Space");
    h.state_until("resumed", |s| s["paused"] == false);
    h.key("Space");
    h.state_until(
        "independent kanji character, even though cat is known",
        |s| s["popups"][0]["query"] == "猫" && s["paused"] == true,
    );
    h.key("Escape");
    h.cmd(json!({"cmd":"settings","tab":"lookup","save":"[lookup]\npause_lookup='off'\n"}));
    h.key("Escape");
    h.key("KeyK");
    h.state_until("explicit optional first-kanji shortcut", |s| {
        s["popups"][0]["query"] == "猫"
    });
    h.key("Escape");
    let changed = desk.path("changed.srt");
    std::fs::write(&changed, "1\n00:00:00,000 --> 00:00:29,000\n犬です。\n").unwrap();
    h.drop_files(&[changed.to_str().unwrap()]);
    h.state_until("changed source has no old popup or queued target", |s| {
        s["subtitles"]["source"] == format!("file:{}", changed.display())
            && s["popups"].as_array().unwrap().is_empty()
            && s["guided_lookup"]["target"].is_null()
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let lib = nikomiru_library::Library::open(&library_path).unwrap();
    let statuses = lib.word_statuses().unwrap();
    assert_eq!(statuses.len(), initial.len());
    for (e, r, mark) in initial {
        assert_eq!(statuses[&nikomiru_library::words::word_key(&e, &r)], mark);
    }
}
// T-GDL-003, T-GDL-004: genuine window-handler down/up/focus events change only the transient secondary visibility.
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn translation_peek_releases_on_modifiers_focus_pages_popups_and_source_changes() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config(
        "[player]\nmpv_options={ao='null'}\n[shortcuts]\npeek_translation=['KeyT','KeyU']\n",
    );
    let (mut app, mut h) = desk.launch();
    source(&desk, &mut h);
    let secondary = desk.path("translation.en.srt");
    std::fs::write(
        &secondary,
        "1\n00:00:00,000 --> 00:00:29,000\ncat, dog and food\n",
    )
    .unwrap();
    // Add the local text source through the ordinary source loader, then restore the primary source.
    let primary = desk.path("guided.ja.srt");
    h.drop_files(&[secondary.to_str().unwrap()]);
    h.state_until("secondary source listed", |s| {
        s["subtitles"]["source"] == format!("file:{}", secondary.display())
    });
    h.drop_files(&[primary.to_str().unwrap()]);
    h.state_until("primary restored", |s| {
        s["subtitles"]["source"] == format!("file:{}", primary.display())
    });
    let key = format!("file:{}", secondary.display());
    h.cmd(json!({"cmd":"secondary","key":key,"obscure":"hidden"}));
    h.state_until("secondary hidden during playback", |s| {
        s["secondary"]["cues"] == 1 && s["secondary"]["obscure"] == "hidden" && s["paused"] == false
    });
    down(&mut h, false);
    h.state_until("held secondary visible", |s| {
        s["guided_lookup"]["peeking"] == true && s["secondary"]["obscure"] == "none"
    });
    down(&mut h, true);
    // A second configured key cannot steal ownership from the original held key.
    h.cmd(json!({"cmd":"key-event","code":"KeyU","pressed":true}));
    up(&mut h);
    h.cmd(json!({"cmd":"key-event","code":"KeyU","pressed":false}));
    h.state_until(
        "original physical key released despite modifier and text focus changes",
        |s| s["guided_lookup"]["peeking"] == false && s["secondary"]["obscure"] == "hidden",
    );
    down(&mut h, false);
    h.cmd(json!({"cmd":"focus-lost"}));
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    down(&mut h, true);
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    up(&mut h);
    h.cmd(json!({"cmd":"key-event","code":"KeyT","pressed":true,"text_focus":true}));
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    up(&mut h);
    down(&mut h, false);
    h.cmd(json!({"cmd":"settings","tab":"lookup"}));
    h.state_until("covering page clears reveal", |s| {
        s["settings_open"] == true && s["guided_lookup"]["peeking"] == false
    });
    h.key("Escape");
    down(&mut h, true);
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    up(&mut h);
    down(&mut h, false);
    h.cmd(json!({"cmd":"click_grapheme","layer":"main","cue_index":0,"grapheme_index":2}));
    h.wait("popup-rendered");
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    down(&mut h, false);
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    up(&mut h);
    h.key("Escape");
    h.state_until("resumed original playback", |s| s["paused"] == false);
    down(&mut h, false);
    h.cmd(json!({"cmd":"secondary","key":"off"}));
    let state = h.state();
    assert_eq!(state["guided_lookup"]["peeking"], false);
    assert_eq!(state["guided_lookup"]["secondary_mode"], "hidden");
    down(&mut h, false);
    assert_eq!(h.state()["guided_lookup"]["peeking"], false);
    up(&mut h);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
