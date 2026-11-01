// SPDX-License-Identifier: GPL-3.0-only

//! T-FUZZ-001…003 and T-SUB-004 (L2): the parsers on arbitrary input never
//! panic, every cue has `start ≤ end`, and markup never reaches `plain_text`.
//! Nightly runs raise the number of cases with `PROPTEST_CASES`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nikomiru_subs::{Format, parse};
use proptest::prelude::*;

fn check(format: Format, text: &str) {
    for cue in parse(format, text).cues {
        assert!(cue.start_ms <= cue.end_ms, "{cue:?}");
        for r in cue
            .markup
            .runs
            .iter()
            .map(|r| &r.range)
            .chain(cue.markup.ruby.iter().map(|r| &r.range))
        {
            assert!(r.start < r.end && r.end <= cue.plain_text.len(), "{cue:?}");
            assert!(
                cue.plain_text.is_char_boundary(r.start) && cue.plain_text.is_char_boundary(r.end)
            );
        }
        let _ = cue.graphemes();
    }
}

/// Pieces that make the parsers take their less common paths.
fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\n\n".to_string()),
        Just(" --> ".to_string()),
        Just("00:00:01,000 --> 00:00:02,000\n".to_string()),
        Just("00:01.000 --> 00:02.000 line:10% align:end\n".to_string()),
        Just("Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,".to_string()),
        Just("[Events]\n".to_string()),
        Just("[Script Info]\nPlayResX: 0\n".to_string()),
        Just("Format: Layer, Text\n".to_string()),
        Just("WEBVTT\n".to_string()),
        Just("NOTE x\n".to_string()),
        Just("{\\an8\\pos(1,2)\\b1\\c&HFF&\\p1}".to_string()),
        Just("{\\p0}".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("\\N".to_string()),
        Just("\\".to_string()),
        Just("<i>".to_string()),
        Just("</i>".to_string()),
        Just("<ruby>".to_string()),
        Just("<rt>".to_string()),
        Just("</rt></ruby>".to_string()),
        Just("<font color=\"#".to_string()),
        Just("&#x1F600;".to_string()),
        Just("&".to_string()),
        "[ぁ-んァ-ン一-龯a-z0-9 ,.:]{0,8}",
        any::<char>().prop_map(String::from),
    ]
}

fn structured() -> impl Strategy<Value = String> {
    prop::collection::vec(piece(), 0..40).prop_map(|p| p.concat())
}

proptest! {
    // T-FUZZ-001
    #[test]
    fn srt_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512), text in structured()) {
        check(Format::Srt, &String::from_utf8_lossy(&bytes));
        check(Format::Srt, &text);
    }

    // T-FUZZ-001
    #[test]
    fn lrc_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512), text in structured()) {
        check(Format::Lrc, &String::from_utf8_lossy(&bytes));
        check(Format::Lrc, &format!("[00:01.00]{text}"));
    }

    // T-FUZZ-002
    #[test]
    fn vtt_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512), text in structured()) {
        check(Format::Vtt, &String::from_utf8_lossy(&bytes));
        check(Format::Vtt, &text);
    }

    // T-FUZZ-003
    #[test]
    fn ass_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512), text in structured()) {
        check(Format::Ass, &String::from_utf8_lossy(&bytes));
        check(Format::Ass, &text);
    }

    // T-SUB-004
    #[test]
    fn tags_never_reach_the_plain_text(
        words in prop::collection::vec("[ぁ-ん一-龯a-z]{1,6}", 1..6),
        tags in prop::collection::vec(prop_oneof![
            Just("<i>"), Just("</i>"), Just("<b>"), Just("</b>"), Just("<u>"), Just("</u>"),
            Just("<font color=\"#00ff00\">"), Just("</font>"), Just("{\\an8}"), Just("{\\i1}"),
            Just("{note}"),
        ], 1..6),
    ) {
        let mut line = String::new();
        for (i, w) in words.iter().enumerate() {
            line.push_str(tags[i % tags.len()]);
            line.push_str(w);
        }
        let srt = format!("1\n00:00:01,000 --> 00:00:02,000\n{line}\n");
        let cue = &parse(Format::Srt, &srt).cues[0];
        prop_assert_eq!(&cue.plain_text, &words.concat());
    }
}
