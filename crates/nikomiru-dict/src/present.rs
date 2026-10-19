// SPDX-License-Identifier: GPL-3.0-only

//! What the popup shows of an entry, beyond the lookup itself: the trace tags
//! (LKP-024) and the glossaries hidden because they are only a redirect label
//! (BUG-2566). Ports of Fushi's `fushi/assets/popup/popup.js` (ac8034e).

use serde_json::Value;

use crate::lookup::{Entry, Step};

/// The trace tags of an entry: innermost transform first (`-て « -いる`).
/// With no transform but a different spelling (アタル found as あたる), one
/// tag `アタル → あたる` without description, as Fushi shows a text variant.
pub fn trace_tags(entry: &Entry) -> Vec<Step> {
    if !entry.trace.is_empty() {
        return entry.trace.iter().rev().cloned().collect();
    }
    if entry.deinflected.is_empty() || entry.matched == entry.deinflected {
        return Vec::new();
    }
    vec![Step {
        name: format!("{} → {}", entry.matched, entry.deinflected).into(),
        description: "".into(),
    }]
}

/// Text without HTML tags, whitespace collapsed.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            c => out.push(c),
        }
    }
    // an unclosed `<` is text, as in the regex `<[^>]*>`
    if in_tag {
        return text.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `\bredirected\s+from\b` on text whose whitespace is already collapsed.
fn says_redirected_from(text: &str) -> bool {
    const NEEDLE: &str = "redirected from";
    text.match_indices(NEEDLE).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + NEEDLE.len()..].chars().next();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

/// 2: a "Redirected from …" label; 1: the bare word "redirect"; 0: neither.
fn marker(content: &Value) -> u8 {
    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
            {
                return marker(&parsed);
            }
            let text = plain(s).to_lowercase();
            if says_redirected_from(&text) {
                2
            } else {
                u8::from(text == "redirect")
            }
        }
        Value::Array(items) => items.iter().map(marker).max().unwrap_or(0),
        Value::Object(node) => match (node.get("content"), node.get("text")) {
            (Some(content), _) => marker(content),
            (None, Some(Value::String(text))) => marker(&Value::String(text.clone())),
            _ => 0,
        },
        _ => 0,
    }
}

fn visible_text(content: &Value) -> String {
    match content {
        Value::String(s) => plain(s),
        Value::Array(items) => items
            .iter()
            .map(visible_text)
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string(),
        Value::Object(node) => match (node.get("content"), node.get("text")) {
            (Some(content), _) => visible_text(content),
            (None, Some(Value::String(text))) => plain(text),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Whether the popup hides a glossary as a redirect label (BUG-2566): a
/// `redirect`/`redirected` tag, or a "Redirected from …" label (or a
/// `redirect` chip in a `non-lemma` entry) with no definition of its own next
/// to it. `glossary` is the record's glossary array.
pub fn is_redirect_label(definition_tags: &str, term_tags: &str, glossary: &Value) -> bool {
    let tags = format!("{definition_tags} {term_tags}").to_lowercase();
    let mut tags = tags.split_whitespace();
    let tag_list: Vec<&str> = tags.by_ref().collect();
    if tag_list
        .iter()
        .any(|t| *t == "redirect" || *t == "redirected")
    {
        return true;
    }
    let kind = marker(glossary);
    let marked = kind == 2 || (kind == 1 && tag_list.contains(&"non-lemma"));
    let definition = glossary.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| marker(item) == 0 && !visible_text(item).is_empty())
    });
    marked && !definition
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(tags: &str, json: &str) -> bool {
        is_redirect_label(tags, "", &serde_json::from_str(json).unwrap())
    }

    #[test]
    fn redirect_labels() {
        assert!(hidden("redirect", r#"["anything"]"#));
        assert!(hidden(
            "",
            r#"[{"type":"structured-content","content":"Redirected from give up"}]"#
        ));
        assert!(!hidden(
            "",
            r#"[["give up", ["Redirected from give up"]], {"type":"structured-content","content":"to stop trying"}]"#
        ));
        assert!(hidden(
            "non-lemma",
            r#"[{"type":"structured-content","content":{"tag":"span","content":"redirect"}}]"#
        ));
        assert!(
            !hidden(
                "",
                r#"[{"type":"structured-content","content":"redirect"}]"#
            ),
            "needs non-lemma"
        );
        assert!(!hidden("", r#"["the redirectedfrom field"]"#));
        assert!(!hidden(
            "",
            r#"[{"type":"structured-content","content":{"tag":"span","data":{"class":"redirect"},"content":"word"}}]"#
        ));
    }

    #[test]
    fn plain_text_drops_tags() {
        assert_eq!(plain("<b>Redirected</b>\n  from x"), "Redirected from x");
        assert_eq!(plain("a < b"), "a < b");
    }
}
