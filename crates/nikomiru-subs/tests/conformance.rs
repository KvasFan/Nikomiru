// SPDX-License-Identifier: GPL-3.0-only

//! Conformance with Fushi's subtitle parsers (TEST §5): the cases of
//! `tests/conformance/subtitles.json`, extracted from Fushi's tests at commit
//! `ac8034e` (T-SUB-001…004). A failing case shows its provenance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use nikomiru_subs::{Format, RunStyle, Track, parse};
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/subtitles.json");
    let text = std::fs::read_to_string(path).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["cases"].as_array().unwrap().clone()
}

/// The inline style at every character of `text[range]`.
fn styles(cue: &nikomiru_subs::Cue, range: std::ops::Range<usize>) -> Vec<Option<&RunStyle>> {
    cue.plain_text[range.clone()]
        .char_indices()
        .map(|(i, _)| {
            let at = range.start + i;
            cue.markup
                .runs
                .iter()
                .find(|r| r.range.contains(&at))
                .map(|r| &r.style)
        })
        .collect()
}

fn check_span(cue: &nikomiru_subs::Cue, span: &Value) -> Result<(), String> {
    let text = span["text"].as_str().ok_or("span without text")?;
    let at = cue
        .plain_text
        .find(text)
        .ok_or_else(|| format!("span {text:?} is not in {:?}", cue.plain_text))?;
    for style in styles(cue, at..at + text.len()) {
        let flag = |name: &str, value: Option<bool>| -> Result<(), String> {
            match span.get(name) {
                Some(Value::Bool(true)) if value != Some(true) => {
                    Err(format!("{text:?}: {name} expected"))
                }
                Some(Value::Bool(false)) if value == Some(true) => {
                    Err(format!("{text:?}: {name} not expected"))
                }
                _ => Ok(()),
            }
        };
        let s = style.cloned().unwrap_or_default();
        flag("bold", s.bold)?;
        flag("italic", s.italic)?;
        flag("underline", s.underline)?;
        match span.get("color") {
            Some(Value::String(c)) if s.color.map(|c| c.hex()).as_deref() != Some(c.as_str()) => {
                return Err(format!(
                    "{text:?}: colour {:?}, expected {c}",
                    s.color.map(|c| c.hex())
                ));
            }
            Some(Value::Null) if s.color.is_some() => {
                return Err(format!("{text:?}: unexpected colour"));
            }
            _ => {}
        }
        match span.get("font") {
            Some(Value::String(f)) if s.font.as_deref() != Some(f.as_str()) => {
                return Err(format!("{text:?}: font {:?}, expected {f}", s.font));
            }
            Some(Value::Null) if s.font.is_some() => {
                return Err(format!("{text:?}: unexpected font"));
            }
            _ => {}
        }
        if let Some(size) = span.get("size").and_then(Value::as_f64)
            && s.size.map(f64::from) != Some(size)
        {
            return Err(format!("{text:?}: size {:?}, expected {size}", s.size));
        }
    }
    Ok(())
}

fn check_case(case: &Value) -> Result<(), String> {
    let format = match case["format"].as_str() {
        Some("srt") => Format::Srt,
        Some("vtt") => Format::Vtt,
        Some("ass") => Format::Ass,
        other => return Err(format!("unknown format {other:?}")),
    };
    let track: Track = parse(format, case["input"].as_str().unwrap_or(""));
    let expected = &case["expected"];
    let count = expected["cue_count"].as_u64().unwrap_or(0) as usize;
    if track.cues.len() != count {
        let texts: Vec<&str> = track.cues.iter().map(|c| c.plain_text.as_str()).collect();
        return Err(format!(
            "{} cues {texts:?}, expected {count}",
            track.cues.len()
        ));
    }
    for want in expected["cues"].as_array().into_iter().flatten() {
        let index = want["index"].as_u64().unwrap_or(0) as usize;
        let cue = track.cues.get(index).ok_or(format!("no cue {index}"))?;
        let field = |name: &str| want.get(name).filter(|v| !v.is_null());
        if let Some(v) = field("start_ms").and_then(Value::as_i64)
            && cue.start_ms != v
        {
            return Err(format!("cue {index}: start {} != {v}", cue.start_ms));
        }
        if let Some(v) = field("end_ms").and_then(Value::as_i64)
            && cue.end_ms != v
        {
            return Err(format!("cue {index}: end {} != {v}", cue.end_ms));
        }
        if let Some(v) = field("plain_text").and_then(Value::as_str)
            && cue.plain_text != v
        {
            return Err(format!("cue {index}: text {:?} != {v:?}", cue.plain_text));
        }
        if let Some(spans) = field("spans").and_then(Value::as_array) {
            if spans.is_empty() && !cue.markup.runs.is_empty() {
                return Err(format!(
                    "cue {index}: unexpected runs {:?}",
                    cue.markup.runs
                ));
            }
            for span in spans {
                check_span(cue, span).map_err(|e| format!("cue {index}: {e}"))?;
            }
        }
        if let Some(v) = field("anchor").and_then(Value::as_u64) {
            // the inline \an, else the ASS style's alignment
            let anchor = cue.markup.anchor.or_else(|| {
                cue.style
                    .as_deref()
                    .and_then(|s| track.style(s))
                    .map(|s| s.alignment)
            });
            if anchor != Some(v as u8) {
                return Err(format!("cue {index}: anchor {anchor:?} != {v}"));
            }
        }
        if let Some(pos) = field("pos").and_then(Value::as_array) {
            let (x, y) = (
                pos[0].as_f64().unwrap_or(-1.0),
                pos[1].as_f64().unwrap_or(-1.0),
            );
            match cue.markup.pos {
                Some((px, py)) if (px - x).abs() < 1e-6 && (py - y).abs() < 1e-6 => {}
                other => return Err(format!("cue {index}: pos {other:?} != ({x}, {y})")),
            }
        }
        if let Some(res) = field("play_res").and_then(Value::as_array) {
            let want = (
                res[0].as_u64().unwrap_or(0) as u32,
                res[1].as_u64().unwrap_or(0) as u32,
            );
            if track.play_res != Some(want) {
                return Err(format!("play_res {:?} != {want:?}", track.play_res));
            }
        }
        if let Some(v) = field("layer").and_then(Value::as_i64)
            && i64::from(cue.layer) != v
        {
            return Err(format!("cue {index}: layer {} != {v}", cue.layer));
        }
    }
    Ok(())
}

// T-SUB-001
// T-SUB-002
// T-SUB-003
// T-SUB-004
#[test]
fn fushi_parser_cases() {
    let cases = cases();
    assert!(cases.len() >= 80, "{} cases", cases.len());
    let mut failures = Vec::new();
    let mut run = 0;
    for case in &cases {
        if case["skip"] == true {
            assert!(
                !case["divergence"].is_null(),
                "{}: a skipped case needs a divergence",
                case["id"]
            );
            continue;
        }
        run += 1;
        if let Err(e) = check_case(case) {
            failures.push(format!(
                "{} ({} — {:?}): {e}",
                case["id"].as_str().unwrap_or("?"),
                case["provenance"]["file"].as_str().unwrap_or("?"),
                case["provenance"]["test"].as_str().unwrap_or("?"),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {run} cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
