// SPDX-License-Identifier: GPL-3.0-only

//! Interface strings (SET-004, TEST T-I18N-001, T-I18N-002): every Fluent key
//! is used and every key used exists, and the views of `nikomiru-ui` show no
//! text that is not translated.
//!
//! The checks read the sources as text, with patterns: they cover the ways
//! the code asks for a string (`tr("…")`, `.tr("…")`, `try_tr("…")`, the
//! `error-…` ids of `message_id`) and the widgets that show one.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;

use crate::arch::report;
use crate::util::{rel, walk};

/// Keys built at run time from a prefix: `action-play-pause`,
/// `encoding-utf-8`, `anki-audio-q0`, `dict-group-terms`.
const DYNAMIC: [&str; 5] = [
    "action-",
    "encoding-",
    "anki-audio-q",
    "anki-image-q",
    "dict-group-",
];

/// Literals the views may show as they are: examples of what to type, the
/// names of standards. `(file name, literal)`.
const SHOWN_AS_IS: [(&str, &str); 4] = [
    (
        "audio_page.rs",
        "http://localhost:5050/?term={term}&reading={reading}",
    ),
    ("audio_page.rs", "nhk16, forvo, jpod"),
    ("dict_page.rs", "CSS"),
    ("prefs_page.rs", "GPL-3.0-only"),
];

/// The code of a file before its tests.
fn code(path: &Path) -> Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let tests = Regex::new(r"#\[cfg\(test\)\]\s*mod\s")?;
    Ok(match tests.find(&text) {
        Some(m) => text[..m.start()].to_string(),
        None => text,
    })
}

fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// Runs the checks.
pub fn check(root: &Path) -> Result<()> {
    let ftl = root.join("assets/i18n/en/main.ftl");
    let ftl =
        std::fs::read_to_string(&ftl).with_context(|| format!("reading {}", ftl.display()))?;
    let key = Regex::new(r"^([a-z][a-z0-9-]*) =")?;
    let keys: BTreeSet<&str> = ftl
        .lines()
        .filter_map(|l| key.captures(l).and_then(|c| c.get(1)).map(|m| m.as_str()))
        .collect();

    let mut files = Vec::new();
    walk(&root.join("crates"), "rs", &mut files)?;
    let mut problems = Vec::new();
    let mut all = String::new();
    let asked = Regex::new(
        r#"(?:\btr\(\s*(?:&?\w+,\s*)?|\.tr\(\s*|try_tr\(\s*|=>\s*)"([a-z][a-z0-9]*(?:-[a-z0-9]+)+)""#,
    )?;
    for file in &files {
        let text = code(file)?;
        for c in asked.captures_iter(&text) {
            let (Some(whole), Some(k)) = (c.get(0), c.get(1)) else {
                continue;
            };
            // a match arm gives a message id only for errors
            if whole.as_str().starts_with("=>") && !k.as_str().starts_with("error-") {
                continue;
            }
            if !keys.contains(k.as_str()) {
                problems.push(format!(
                    "{}:{}: no Fluent key {}",
                    rel(file),
                    line_of(&text, k.start()),
                    k.as_str()
                ));
            }
        }
        all.push_str(&text);
    }
    for k in &keys {
        if !all.contains(&format!("\"{k}\"")) && !DYNAMIC.iter().any(|p| k.starts_with(p)) {
            problems.push(format!("assets/i18n/en/main.ftl: {k} is never used"));
        }
    }

    // the views: what a widget shows comes from Fluent
    let shown = Regex::new(
        r#"(?:\.(?:label|button|small_button|link|selectable_label|checkbox|on_hover_text|hint_text|heading)\(|RichText::new\(|Button::new\(|Label::new\(|labeled\(Role::\w+,\s*\w+,\s*)\s*(?:&?[\w.]+,\s*)?"((?:[^"\\]|\\.)*)""#,
    )?;
    let words = Regex::new(r"[A-Za-z]{2,}")?;
    let mut views = Vec::new();
    walk(&root.join("crates/nikomiru-ui/src/view"), "rs", &mut views)?;
    views.push(root.join("crates/nikomiru-ui/src/popup_view.rs"));
    for file in &views {
        let text = code(file)?;
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for c in shown.captures_iter(&text) {
            let Some(lit) = c.get(1) else {
                continue;
            };
            if words.is_match(lit.as_str()) && !SHOWN_AS_IS.contains(&(name.as_str(), lit.as_str()))
            {
                problems.push(format!(
                    "{}:{}: text shown untranslated: {:?}",
                    rel(file),
                    line_of(&text, lit.start()),
                    lit.as_str()
                ));
            }
        }
    }
    report("interface strings", &problems)
}
