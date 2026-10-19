// SPDX-License-Identifier: GPL-3.0-only

//! Conformance with Fushi's text processing (TEST §5): the cases of
//! `tests/conformance/text.json`, extracted from Fushi's tests at commit
//! `ac8034e`. Indices in the file count code points; a failing case shows its
//! id and provenance.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use nikomiru_text::hangul;
use nikomiru_text::scan::{Context, candidates, query};
use nikomiru_text::sentence::sentence;
use nikomiru_text::variants::{ALL_GROUPS, variants};
use serde_json::Value;

fn cases() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/text.json");
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["cases"].as_array().unwrap().clone()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

/// The byte offset of code point `n` (the end of the text past it).
fn byte(text: &str, n: usize) -> usize {
    text.char_indices().nth(n).map_or(text.len(), |(i, _)| i)
}

fn chars(text: &str, bytes: usize) -> usize {
    text[..bytes].chars().count()
}

fn membership(got: &[String], case: &Value) -> Result<(), String> {
    for p in strings(&case["present"]) {
        if !got.contains(&p) {
            return Err(format!("{p:?} missing from {got:?}"));
        }
    }
    for a in strings(&case["absent"]) {
        if got.contains(&a) {
            return Err(format!("{a:?} must not be in {got:?}"));
        }
    }
    Ok(())
}

fn check(case: &Value) -> Result<(), String> {
    let s = |k: &str| case[k].as_str().unwrap_or_default();
    let n = |k: &str| case[k].as_u64().unwrap_or_default() as usize;
    match s("kind") {
        "variants" => {
            assert_eq!(s("languages"), "all", "only the full chain is used");
            let got: Vec<String> = variants(s("input"), &ALL_GROUPS)
                .into_iter()
                .map(|v| v.text)
                .collect();
            membership(&got, case)
        }
        "candidates" => {
            let got: Vec<String> = candidates(s("input"), n("scan_length"))
                .into_iter()
                .map(String::from)
                .collect();
            match case.get("expected") {
                Some(e) if strings(e) != got => Err(format!("{got:?}, expected {:?}", strings(e))),
                Some(_) => Ok(()),
                None => membership(&got, case),
            }
        }
        "hangul" => {
            let got = match s("function") {
                "disassemble" => hangul::disassemble(s("input")),
                "reassemble" => hangul::reassemble(s("input")),
                f => return Err(format!("unknown function {f}")),
            };
            (got == s("expected"))
                .then_some(())
                .ok_or(format!("{got:?}"))
        }
        "query" => {
            let text = s("text");
            let context = match s("context") {
                "cue" => Context::Cue,
                _ => Context::Free,
            };
            let got = &text[query(text, byte(text, n("click")), context)];
            (got == s("expected"))
                .then_some(())
                .ok_or(format!("{got:?}"))
        }
        "sentence" => {
            let text = s("text");
            let click = n("click");
            let len = case["options"]["sel_len"].as_u64().unwrap() as usize;
            let at = byte(text, click);
            let found = sentence(text, at..byte(text, click + len));
            let got = &text[found.range.clone()];
            if got != s("expected") {
                return Err(format!("{got:?}"));
            }
            if let Some(sel) = case.get("expected_selection") {
                let start = chars(got, found.selection.start);
                let length = got[found.selection.clone()].chars().count();
                let want = (n_of(sel, "start"), n_of(sel, "length"));
                if (start, length) != want {
                    return Err(format!(
                        "selection {:?}, expected {want:?}",
                        (start, length)
                    ));
                }
            }
            Ok(())
        }
        kind => Err(format!("unknown kind {kind}")),
    }
}

fn n_of(v: &Value, k: &str) -> usize {
    v[k].as_u64().unwrap() as usize
}

// T-LKP-001, T-LKP-002, T-LKP-004, T-LKP-010, T-LKP-011, T-LKP-012, T-TXT-007
#[test]
fn fushi_text_vectors() {
    let all = cases();
    let mut failures = Vec::new();
    let mut run = 0;
    for case in all.iter().filter(|c| c["skip"] != Value::Bool(true)) {
        run += 1;
        if let Err(e) = check(case) {
            failures.push(format!(
                "{} ({} › {}): {e}",
                case["id"].as_str().unwrap(),
                case["provenance"]["file"].as_str().unwrap_or("?"),
                case["provenance"]["test"].as_str().unwrap_or("?"),
            ));
        }
    }
    assert!(run >= 160, "only {run} cases ran");
    assert!(
        failures.is_empty(),
        "{} of {run} cases fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
