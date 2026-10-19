// SPDX-License-Identifier: GPL-3.0-only

//! L4: subtitle encodings through glibc's iconv (SUB-005).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use nikomiru_app::linux::Iconv;
use nikomiru_subs::encoding::decode;
use nikomiru_subs::{Format, parse};

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/subs")
            .join(name),
    )
    .unwrap()
}

fn texts(name: &str, forced: Option<&str>) -> (String, Vec<String>) {
    let d = decode(&bytes(name), forced, &Iconv).unwrap();
    let cues = parse(Format::Srt, &d.text).cues;
    (d.encoding, cues.into_iter().map(|c| c.plain_text).collect())
}

// T-SUB-005
#[test]
fn every_encoding_gives_the_expected_text() {
    let japanese = vec![
        "日本語の字幕です。".to_string(),
        "カタカナとひらがなと漢字\n「かぎかっこ」も使います。".to_string(),
    ];
    assert_eq!(
        texts("shift_jis.srt", None),
        ("CP932".to_string(), japanese.clone())
    );
    assert_eq!(
        texts("euc_jp.srt", None),
        ("EUC-JP".to_string(), japanese.clone())
    );
    assert_eq!(
        texts("utf16le_bom.srt", None),
        ("UTF-16LE".to_string(), japanese)
    );
    assert_eq!(
        texts("gb18030.srt", None),
        (
            "GB18030".to_string(),
            vec![
                "这是中文字幕。".to_string(),
                "我们一起学习语言吧！".to_string()
            ]
        )
    );
    assert_eq!(
        texts("cp1252.srt", None),
        (
            "CP1252".to_string(),
            vec![
                "Café, naïve façade — «déjà vu»".to_string(),
                "Ça coûte 5 € à Noël.".to_string()
            ]
        )
    );
}

// T-SUB-005
#[test]
fn a_forced_encoding_takes_precedence() {
    // every byte is valid in ISO-8859-1: the forced choice wins over detection
    let (encoding, cues) = texts("shift_jis.srt", Some("ISO-8859-1"));
    assert_eq!(encoding, "ISO-8859-1");
    assert_ne!(cues.first().map(String::as_str), Some("日本語の字幕です。"));
    // a forced encoding that cannot decode the file gives nothing (glibc's
    // CP1252 has no 0x81, a Shift_JIS lead byte)
    assert!(decode(&bytes("shift_jis.srt"), Some("UTF-8"), &Iconv).is_none());
    assert!(decode(&bytes("shift_jis.srt"), Some("CP1252"), &Iconv).is_none());
}
