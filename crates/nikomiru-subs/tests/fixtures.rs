// SPDX-License-Identifier: GPL-3.0-only

//! The versioned subtitle fixtures (TEST §4.2) through the parsers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use nikomiru_subs::cue::{TextAlign, VttLine};
use nikomiru_subs::timeline::Timeline;
use nikomiru_subs::{Format, Rgb, Track, parse};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/subs")
}

fn load(name: &str) -> Track {
    let path = dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap();
    let format = Format::from_extension(&path.extension().unwrap().to_string_lossy()).unwrap();
    parse(format, &text)
}

fn texts(t: &Track) -> Vec<&str> {
    t.cues.iter().map(|c| c.plain_text.as_str()).collect()
}

// T-SUB-001
#[test]
fn srt_files() {
    let basic = load("basic.srt");
    assert_eq!(
        texts(&basic),
        [
            "今日はいい天気ですね。",
            "そうですね。\n散歩に行きましょう。",
            "駅前の喫茶店で待っています。"
        ]
    );
    let times: Vec<(i64, i64)> = basic.cues.iter().map(|c| (c.start_ms, c.end_ms)).collect();
    assert_eq!(times, [(1000, 4230), (4500, 8100), (8200, 12_000)]);
    // a BOM and CRLF change nothing
    assert_eq!(load("bom_crlf.srt"), basic);

    let tags = load("tags_braces.srt");
    assert_eq!(
        texts(&tags),
        ["上に表示される字幕", "斜体のテキスト", "赤い文字と太字"]
    );
    assert_eq!(tags.cues[0].markup.anchor, Some(8));
    let italic = &tags.cues[1].markup.runs[0];
    assert_eq!(
        (italic.range.clone(), italic.style.italic),
        (0.."斜体のテキスト".len(), Some(true))
    );
    let red = &tags.cues[2].markup.runs[0];
    assert_eq!(&tags.cues[2].plain_text[red.range.clone()], "赤い");
    assert_eq!(red.style.color, Some(Rgb(255, 0, 0)));

    assert_eq!(
        texts(&load("out_of_order.srt")),
        ["一番目", "二番目", "三番目"]
    );
    assert_eq!(
        texts(&load("negative_duration.srt")),
        ["正しい字幕", "次の字幕"]
    );
    let overlap = Timeline::new(load("overlap.srt").cues);
    assert_eq!(overlap.active(2500), [0, 1]);
}

// T-SUB-002
#[test]
fn vtt_files() {
    let settings = load("settings.vtt");
    let first = settings.cues[0].markup.vtt.as_ref().unwrap();
    assert_eq!(first.line, Some(VttLine::Percent(10.0)));
    assert_eq!(first.position, Some(25.0));
    assert_eq!(first.align, Some(TextAlign::Start));
    let second = settings.cues[1].markup.vtt.as_ref().unwrap();
    assert_eq!(
        (second.line, second.size, second.align),
        (Some(VttLine::Number(-2)), Some(50.0), Some(TextAlign::End))
    );

    assert_eq!(
        texts(&load("notes_styles.vtt")),
        ["こんにちは", "ありがとう & さようなら"]
    );

    let ruby = load("ruby.vtt");
    let cue = &ruby.cues[0];
    assert_eq!(cue.plain_text, "漢字を読む");
    let readings: Vec<(&str, &str)> = cue
        .markup
        .ruby
        .iter()
        .map(|r| (&cue.plain_text[r.range.clone()], r.text.as_str()))
        .collect();
    assert_eq!(readings, [("漢字", "かんじ"), ("読", "よ")]);
    let g = cue.graphemes();
    assert_eq!(g[0].ruby, Some(("かんじ", 2)));
}

// T-SUB-003
#[test]
fn ass_files() {
    let no_res = load("no_playres.ass");
    assert_eq!(no_res.play_res, Some((384, 288)));
    assert_eq!(texts(&no_res), ["一行目\n二行目", "真ん中\u{a0}空白"]);
    assert_eq!(no_res.cues[1].markup.pos, Some((0.5, 0.5)));

    let drawing = load("drawing.ass");
    assert_eq!(texts(&drawing), ["図形の後の文字", "普通の文字"]);

    let layers = load("layers.ass");
    let t = Timeline::new(layers.cues.clone());
    let active: Vec<&str> = t
        .active(3000)
        .iter()
        .map(|&i| layers.cues[i].plain_text.as_str())
        .collect();
    assert_eq!(active, ["台詞（下の層）", "真ん中の層", "看板（上の層）"]);

    let styles = load("styles.ass");
    assert_eq!(styles.play_res, Some((1920, 1080)));
    let sign = styles.style("Sign").unwrap();
    assert_eq!(
        (sign.alignment, sign.bold, sign.primary),
        (8, true, Rgb(255, 215, 0))
    );
    let sign_cue = &styles.cues[2];
    assert_eq!(sign_cue.style.as_deref(), Some("Sign"));
    assert_eq!(sign_cue.markup.pos, Some((0.5, 120.0 / 1080.0)));
    let mixed = &styles.cues[3];
    assert_eq!(mixed.plain_text, "強く言う赤斜め");
    let styled: Vec<&str> = mixed
        .markup
        .runs
        .iter()
        .map(|r| &mixed.plain_text[r.range.clone()])
        .collect();
    assert_eq!(styled, ["強く", "言う", "赤", "斜め"]);
    let small = &styles.cues[4];
    assert_eq!(small.markup.anchor, Some(8));
    assert_eq!(small.markup.runs[0].style.size, Some(40.0));

    // effects of SUB-019 are ignored, the text stays
    assert_eq!(texts(&load("karaoke.ass")), ["からおけ", "効果付き"]);
}

#[test]
fn malformed_files_give_what_can_be_read() {
    assert!(texts(&load("malformed_garbage.srt")).is_empty());
    assert_eq!(texts(&load("malformed_truncated.srt")), ["ok"]);
    assert_eq!(texts(&load("malformed_header.vtt")), ["still parsed"]);
    let ass = load("malformed_events.ass");
    assert_eq!(texts(&ass), ["ok"]);
    assert_eq!(ass.play_res, Some((384, 288)));
}

// T-SUB-004
#[test]
fn plain_text_never_holds_markup() {
    for entry in std::fs::read_dir(dir()).unwrap() {
        let path = entry.unwrap().path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // legacy encodings: T-SUB-005
        };
        let format = Format::from_extension(&path.extension().unwrap().to_string_lossy()).unwrap();
        for cue in parse(format, &text).cues {
            let p = &cue.plain_text;
            assert!(
                !p.contains("{\\") && !p.contains("\\N") && !p.contains("</") && !p.contains("<i>"),
                "{}: {p:?}",
                path.display()
            );
            assert!(cue.start_ms <= cue.end_ms);
            for run in &cue.markup.runs {
                assert!(p.is_char_boundary(run.range.start) && p.is_char_boundary(run.range.end));
            }
        }
    }
}
