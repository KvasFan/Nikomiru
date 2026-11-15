// SPDX-License-Identifier: GPL-3.0-only

//! M46…M55: complete subtitle snapshots and study actions in the real application.

use super::support::{Desktop, exit_status, fixture};
use serde_json::json;
use std::time::Duration;

// T-SUB-035, T-SUB-037: selected, invalid and stale snapshot requests use the real save handler.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn selected_subtitle_snapshots_keep_timing_and_reject_stale_requests() {
    let mut desk = Desktop::start();
    let video = desk.path("study.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let subtitles = desk.path("study.ja.srt");
    std::fs::write(&subtitles, "1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:06,000\n犬\n\n3\n00:00:08,000 --> 00:00:10,000\n鳥\n").unwrap();
    let english = desk.path("study.en.srt");
    std::fs::write(
        &english,
        "1\n00:00:01,500 --> 00:00:02,500\ncat\n\n2\n00:00:04,000 --> 00:00:05,000\ndog\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.drop_files(&[subtitles.to_str().unwrap()]);
    h.state_until("three text cues", |s| {
        s["subtitles"]["cues"] == 3 && s["subtitles"]["incomplete"] == false
    });
    if h.state()["paused"] == false {
        h.key("Space");
    }
    let before = h.state();
    let srt = desk.path("selected.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":srt, "indices":[1]}));
    let parsed = nikomiru_subs::parse(
        nikomiru_subs::Format::Srt,
        &std::fs::read_to_string(srt).unwrap(),
    );
    assert_eq!(parsed.cues.len(), 1);
    assert_eq!(
        (parsed.cues[0].start_ms, parsed.cues[0].end_ms),
        (3000, 6000)
    );
    assert_eq!(parsed.cues[0].plain_text, "犬");
    let vtt = desk.path("selected.vtt");
    h.cmd(json!({"cmd":"subtitle-export", "path":vtt, "indices":[2,0,2], "format":"vtt"}));
    let parsed = nikomiru_subs::parse(
        nikomiru_subs::Format::Vtt,
        &std::fs::read_to_string(vtt).unwrap(),
    );
    assert_eq!(
        parsed
            .cues
            .iter()
            .map(|c| c.plain_text.as_str())
            .collect::<Vec<_>>(),
        ["猫", "鳥"]
    );
    let stale = desk.path("stale.vtt");
    h.cmd(json!({"cmd":"subtitle-export", "path":stale, "indices":[0], "format":"vtt", "version":u64::MAX}));
    assert!(!stale.exists());
    let invalid = desk.path("invalid.srt");
    h.cmd(json!({"cmd":"subtitle-export", "path":invalid, "indices":[0,9]}));
    assert!(!invalid.exists());
    // T-SUB-038: missing secondary text cannot export; loaded translations align by time.
    let missing = desk.path("missing.tsv");
    h.cmd(json!({"cmd":"subtitle-export", "path":missing, "format":"bilingual"}));
    assert!(!missing.exists());
    h.cmd(json!({"cmd":"secondary", "key":format!("file:{}", english.display())}));
    h.state_until("secondary text ready", |s| s["secondary"]["cues"] == 2);
    let bilingual = desk.path("bilingual.tsv");
    h.cmd(json!({"cmd":"subtitle-export", "path":bilingual, "format":"bilingual"}));
    assert_eq!(
        std::fs::read_to_string(bilingual).unwrap(),
        "start_ms\tend_ms\tprimary\tsecondary\n1000\t2000\t猫\tcat\n3000\t6000\t犬\tdog\n8000\t10000\t鳥\t\n"
    );
    // T-SUB-042: the actual native-save path writes portable combined SRT and retains unmatched cues.
    let combined = desk.path("bilingual.srt");
    h.cmd(json!({"cmd":"subtitle-export","path":combined,"format":"bilingual-srt"}));
    let combined = nikomiru_subs::parse(
        nikomiru_subs::Format::Srt,
        &std::fs::read_to_string(combined).unwrap(),
    );
    assert_eq!(
        combined
            .cues
            .iter()
            .map(|c| c.plain_text.as_str())
            .collect::<Vec<_>>(),
        ["猫\ncat", "犬\ndog", "鳥"]
    );
    // T-SUB-046: actual native saving writes a complete standalone bilingual HTML scope.
    let html = desk.path("study.html");
    h.cmd(json!({"cmd":"subtitle-export","path":html,"indices":[1],"format":"html"}));
    let html = std::fs::read_to_string(html).unwrap();
    assert!(html.contains("<details><summary>"));
    assert!(html.contains("犬") && html.contains("dog") && !html.contains("cat"));
    assert_eq!(html.matches("<article>").count(), 1);
    let after = h.state();
    assert_eq!(
        before["subtitles"]["version"],
        after["subtitles"]["version"]
    );
    assert_eq!(after["paused"], true);
    // T-SUB-043: matching-row navigation seeks the actual player while retaining pause.
    h.cmd(json!({"cmd":"subtitle-study","query":"犬"}));
    h.state_until("one matching line", |s| {
        s["subtitle_study"]["shown"] == json!([1])
    });
    h.cmd(json!({"cmd":"subtitle-study","forward":true}));
    h.state_until("paused at the matching line", |s| {
        s["paused"] == true
            && s["time_pos"]
                .as_f64()
                .is_some_and(|t| (2.9..=3.2).contains(&t))
    });
    // T-SUB-054: real shortcuts reuse current rendered matches, preserve pause and select/reveal only current shown cues.
    h.key("ArrowLeft");
    h.state_until("before matching cue", |s| {
        s["time_pos"].as_f64().is_some_and(|t| t < 1.0)
    });
    h.cmd(json!({"cmd":"key","code":"ArrowDown","mods":["Alt"]}));
    h.state_until("keyboard matching navigation", |s| {
        s["paused"] == true
            && s["time_pos"]
                .as_f64()
                .is_some_and(|t| (2.95..=3.2).contains(&t))
    });
    h.cmd(json!({"cmd":"subtitle-study","mask":true}));
    h.cmd(json!({"cmd":"key","code":"KeyT","mods":["Alt"]}));
    h.state_until("keyboard selected current line", |s| {
        s["subtitle_study"]["selected"] == json!([1])
    });
    h.cmd(json!({"cmd":"key","code":"KeyR","mods":["Alt"]}));
    h.state_until("keyboard revealed current line", |s| {
        s["subtitle_study"]["revealed"] == json!([1])
    });
    h.cmd(json!({"cmd":"key","code":"KeyF","mods":["Ctrl"]}));
    h.cmd(json!({"cmd":"key","code":"KeyT","mods":["Alt"],"text_focus":true}));
    assert_eq!(
        h.state()["subtitle_study"]["selected"],
        json!([1]),
        "typing focus blocks study selection"
    );
    h.cmd(json!({"cmd":"library","action":"open"}));
    let selected = h.state()["subtitle_study"]["selected"].clone();
    h.cmd(json!({"cmd":"key","code":"KeyT","mods":["Alt"]}));
    assert_eq!(
        h.state()["subtitle_study"]["selected"],
        selected,
        "covered pages block study selection"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-KNW-036: lookup renders above the library before opening any media, with no cue context.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn vocabulary_lookup_works_above_the_library_without_opening_media() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"library", "action":"open"}));
    h.cmd(json!({"cmd":"vocabulary-lookup", "text":"猫"}));
    h.wait("popup-rendered");
    let s = h.state();
    assert_eq!(s["study_popup"]["open"], true);
    assert!(s["study_popup"]["cue"].is_null());
    assert_eq!(s["popups"][0]["query"], "猫");
    assert!(s["popups"][0]["entries"].as_u64().unwrap() > 0);
    h.key("Escape");
    h.state_until("study popup closed", |s| s["study_popup"]["open"] == false);
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-KNW-037: real inflection matching, explicit clear and source invalidation without a seek.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn subtitle_concordance_finds_inflected_words_and_rejects_old_sources() {
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja"]);
    desk.write_config("[subtitles]\nword_colors = true\n");
    let video = desk.path("examples.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let text = desk.path("examples.ja.srt");
    std::fs::write(&text, "1\n00:00:01,000 --> 00:00:02,000\n食べました\n\n2\n00:00:03,000 --> 00:00:04,000\n食べる\n\n3\n00:00:05,000 --> 00:00:06,000\n猫\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.state_until("complete vocabulary", |s| {
        s["subtitles"]["words"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    });
    if h.state()["paused"] == false {
        h.key("Space");
    }
    h.state_until("paused", |s| s["paused"] == true);
    // T-KNW-038: a native-saved real analysis dictionary can be imported without installing it in the app.
    let frequency = desk.path("frequency.zip");
    h.cmd(json!({"cmd":"frequency-export", "path":frequency}));
    let imported = nikomiru_dict::import::import(
        &frequency,
        &desk.path("frequency-import"),
        &nikomiru_core::Cancel::new(),
        |_| {},
    )
    .unwrap();
    use nikomiru_dict::store::{DictStore, RedbStore};
    let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
    assert!(store.term_meta("食べる").unwrap().iter().any(|m| matches!(m, nikomiru_dict::yomitan::Meta::Frequency(f) if f.value == 2 && f.reading.as_deref() == Some("たべる"))));
    // T-KNW-044: real segmented subset retains one inflected headword occurrence.
    h.cmd(json!({"cmd":"vocabulary-scope","indices":[0]}));
    h.state_until("captured vocabulary", |s| {
        s["subtitle_study"]["vocabulary_scope"].is_u64()
    });
    let scoped = h.state()["subtitles"]["words"].as_array().unwrap().clone();
    assert!(
        scoped
            .iter()
            .any(|w| w["expression"] == "食べる" && w["count"] == 1)
    );
    assert!(!scoped.iter().any(|w| w["expression"] == "猫"));
    // T-KNW-048: native HTML saves the frozen scoped vocabulary with readings hidden.
    let html = desk.path("vocabulary.html");
    h.cmd(json!({"cmd":"vocabulary-html","path":html}));
    let html = std::fs::read_to_string(html).unwrap();
    assert!(html.contains("食べる") && html.contains("<details><summary>"));
    assert!(html.contains("たべる") && !html.contains("猫"));
    h.cmd(json!({"cmd":"vocabulary-scope"}));
    h.state_until("full vocabulary restored", |s| {
        s["subtitle_study"]["vocabulary_scope"].is_null()
    });
    let before = h.state()["time_pos"].as_f64().unwrap();
    h.cmd(json!({"cmd":"vocabulary-examples","text":"食べる","reading":"たべる"}));
    h.state_until("two exact examples", |s| {
        s["concordance"]["indices"] == json!([0, 1])
    });
    // T-SUB-053: neighbours expand the displayed scope while exact-hit identities stay unchanged.
    h.cmd(json!({"cmd":"subtitle-study","filters":{"name":"Runtime","context":1}}));
    h.state_until("word examples with context", |s| {
        s["subtitle_study"]["shown"] == json!([0, 1, 2])
    });
    assert_eq!(h.state()["concordance"]["indices"], json!([0, 1]));
    assert!((h.state()["time_pos"].as_f64().unwrap() - before).abs() < 0.1);
    h.cmd(json!({"cmd":"vocabulary-examples"}));
    assert!(h.state()["concordance"].is_null());
    h.cmd(json!({"cmd":"vocabulary-examples","text":"食べる","reading":"たべる"}));
    let other = desk.path("other.srt");
    std::fs::write(&other, "1\n00:00:01,000 --> 00:00:02,000\n猫\n").unwrap();
    h.drop_files(&[other.to_str().unwrap()]);
    h.state_until("old examples discarded", |s| {
        s["concordance"].is_null() && s["subtitles"]["cues"] == 1
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-KNW-041: presets survive actual process restart through the normal configuration writer.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn named_study_presets_survive_restart_and_delete() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.cmd(json!({"cmd":"study-preset","name":"Cats","query":"猫"}));
    // T-SUB-052: subtitle presets use the same serial persistent writer and survive restart.
    h.cmd(json!({"cmd":"subtitle-preset","name":"Study lines","preset":{"query":"猫","min_text":2,"shape":2}}));
    h.state_until("preset saved", |s| s["study_presets"][0]["name"] == "Cats");
    super::support::until(Duration::from_secs(10), "preset on disk", || {
        let config = std::fs::read_to_string(desk.path("config/nikomiru/config.toml")).ok()?;
        (config.contains("Cats") && config.contains("Study lines")).then_some(())
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let (mut app, mut h) = desk.launch();
    let s = h.state();
    assert_eq!(s["study_presets"][0]["query"], "猫");
    assert_eq!(s["subtitle_presets"][0]["min_text"], 2);
    h.cmd(json!({"cmd":"subtitle-preset","name":"Study lines"}));
    h.cmd(json!({"cmd":"study-preset","name":"Cats"}));
    super::support::until(Duration::from_secs(10), "preset removed on disk", || {
        let config = std::fs::read_to_string(desk.path("config/nikomiru/config.toml")).ok()?;
        (!config.contains("Cats") && !config.contains("Study lines")).then_some(())
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    let (mut app, mut h) = desk.launch();
    assert_eq!(h.state()["study_presets"], json!([]));
    assert_eq!(h.state()["subtitle_presets"], json!([]));
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-048: real mpv seeks each pass, stops after the finite count and cancels manual control.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn finite_cue_repetition_stops_and_cancels_in_the_real_player() {
    let mut desk = Desktop::start();
    desk.write_config("[subtitles]\nauto_pause = true\n[player]\ngap_speed = 4\n");
    let video = desk.path("repeat.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    let subtitles = desk.path("repeat.ja.srt");
    std::fs::write(
        &subtitles,
        "1\n00:00:01,000 --> 00:00:01,700\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n犬\n",
    )
    .unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.state_until("repeat cues ready", |s| s["subtitles"]["cues"] == 2);
    // T-SUB-055: ordered/deduplicated queue repeats each cue, then pauses at the last end.
    h.cmd(json!({"cmd":"practice-queue","indices":[1,0,1],"passes":2}));
    h.state_until("practice queue complete", |s| {
        s["subtitle_study"]["practice_cues"] == 0 && s["paused"] == true
    });
    assert!(
        h.state()["time_pos"]
            .as_f64()
            .is_some_and(|t| (3.8..=4.2).contains(&t))
    );
    h.cmd(json!({"cmd":"practice-queue","indices":[0,99],"passes":2}));
    assert_eq!(h.state()["subtitle_study"]["practice_cues"], 0);
    h.cmd(json!({"cmd":"practice-queue","indices":[0],"passes":2,"version":u64::MAX}));
    assert_eq!(h.state()["subtitle_study"]["practice_cues"], 0);
    h.cmd(json!({"cmd":"practice","cue":0,"passes":3}));
    h.state_until("finite repetition completed", |s| {
        s["subtitle_study"]["practice_remaining"] == 0 && s["paused"] == true
    });
    let s = h.state();
    assert!(s["next_up"].is_null());
    assert!((1.5..=1.85).contains(&s["time_pos"].as_f64().unwrap()));
    h.cmd(json!({"cmd":"practice","cue":1,"passes":5}));
    h.state_until("repeating second cue", |s| {
        s["subtitle_study"]["practice_remaining"]
            .as_u64()
            .is_some_and(|n| n > 0)
    });
    h.key("Space");
    h.state_until("manual pause cancels", |s| {
        s["subtitle_study"]["practice_remaining"].is_null()
    });
    h.cmd(json!({"cmd":"practice","cue":0,"passes":2}));
    h.state_until("two passes started", |s| {
        s["subtitle_study"]["practice_remaining"] == 2
    });
    h.cmd(json!({"cmd":"practice"}));
    h.state_until("explicit stop pauses", |s| {
        s["subtitle_study"]["practice_remaining"].is_null() && s["paused"] == true
    });
    // T-SUB-048: held speed takes a separate shortcut path and also cancels practice.
    h.cmd(json!({"cmd":"practice","cue":0,"passes":5}));
    h.state_until("practice before held speed", |s| {
        s["subtitle_study"]["practice_remaining"].is_u64()
    });
    h.key("KeyE");
    h.state_until("held speed cancels", |s| {
        s["subtitle_study"]["practice_remaining"].is_null()
    });
    h.cmd(json!({"cmd":"practice","cue":0,"passes":5}));
    h.state_until("practice before timing change", |s| {
        s["subtitle_study"]["practice_remaining"].is_u64()
    });
    h.key("KeyX");
    h.state_until("timing change cancels", |s| {
        s["subtitle_study"]["practice_remaining"].is_null()
    });
    h.cmd(json!({"cmd":"practice","cue":0,"passes":5}));
    let other = desk.path("replacement.srt");
    std::fs::write(&other, "1\n00:00:01,000 --> 00:00:02,000\n鳥\n").unwrap();
    h.drop_files(&[other.to_str().unwrap()]);
    h.state_until("source replacement cancels", |s| {
        s["subtitles"]["cues"] == 1 && s["subtitle_study"]["practice_remaining"].is_null()
    });
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-SUB-049, T-SUB-050, T-SUB-051: real shown snapshots share text/structure/duplicate predicates.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and cargo xtask fixtures"]
fn subtitle_text_filter_scopes_export_original_cues() {
    let mut desk = Desktop::start();
    let video = desk.path("filters.mkv");
    std::fs::copy(fixture("av_long.mkv"), &video).unwrap();
    std::fs::write(desk.path("filters.ja.srt"),"1\n00:00:01,000 --> 00:00:02,000\n猫\n\n2\n00:00:03,000 --> 00:00:04,000\n猫 犬\n\n3\n00:00:05,000 --> 00:00:06,000\n猫  犬\n\n4\n00:00:07,000 --> 00:00:08,000\n猫\n犬\n").unwrap();
    let (mut app, mut h) = desk.launch_with(&["--", video.to_str().unwrap()]);
    h.wait("first-frame");
    h.state_until("four captions", |s| s["subtitles"]["cues"] == 4);
    h.cmd(json!({"cmd":"subtitle-study"}));
    h.state_until("list ready", |s| {
        s["subtitle_study"]["shown"] == json!([0, 1, 2, 3])
    });
    h.cmd(json!({"cmd":"subtitle-study","filters":{"name":"Runtime","min_text":2,"unique":true}}));
    h.state_until("unique long captions", |s| {
        s["subtitle_study"]["shown"] == json!([1])
    });
    h.cmd(json!({"cmd":"subtitle-study","filters":{"name":"Runtime","min_text":2,"shape":2}}));
    h.state_until("multiline caption", |s| {
        s["subtitle_study"]["shown"] == json!([3])
    });
    let path = desk.path("multiline.srt");
    h.cmd(json!({"cmd":"subtitle-export","path":path,"indices":[3]}));
    let out = nikomiru_subs::parse(
        nikomiru_subs::Format::Srt,
        &std::fs::read_to_string(path).unwrap(),
    );
    assert_eq!(out.cues.len(), 1);
    assert_eq!(out.cues[0].start_ms, 7000);
    assert_eq!(out.cues[0].plain_text, "猫\n犬");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}
