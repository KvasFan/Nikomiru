// SPDX-License-Identifier: GPL-3.0-only

//! MDict HTML as Yomitan structured content (DIC-036): an allowlist of tags,
//! bold/italic/underline/raised/lowered text as styles, images and sounds by
//! a safe relative path into the paired `.mdd`, links as lookups or web
//! addresses. Nothing else of the HTML survives: no attribute, class, inline
//! style or script, and the result is drawn by the same safe renderer as
//! Yomitan's glossaries (NFR-SEC-01).

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::structured::{DROPPED, MAX_DEPTH, MAX_NODES, media_path};

/// A resource path as both the definitions and the `.mdd` keys write it:
/// slashes, no leading one, lower case, never outside the dictionary.
pub(crate) fn resource_key(path: &str) -> Option<String> {
    let path = path.trim().trim_start_matches("file://");
    if path.is_empty() || path.contains("://") || path.starts_with("data:") {
        return None;
    }
    media_path(&path.to_lowercase())
}

/// A node being built: its tag (`None` for the root) and content.
struct Node {
    tag: Option<&'static str>,
    attrs: Map<String, Value>,
    content: Vec<Value>,
    /// The HTML name that closes it.
    name: String,
}

/// Percent-encodes a lookup query for `?query=`.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        from = start + name.len();
        let before_ok = start == 0 || lower.as_bytes()[start - 1].is_ascii_whitespace();
        let rest = tag[from..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        let value = if let Some(q) = rest.strip_prefix('"') {
            q.split('"').next()?
        } else if let Some(q) = rest.strip_prefix('\'') {
            q.split('\'').next()?
        } else {
            rest.split(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
                .next()?
        };
        return Some(super::html_to_text(value));
    }
    None
}

/// What an HTML element becomes: its structured-content tag and style.
fn mapped(name: &str) -> Option<(&'static str, Option<(&'static str, &'static str)>)> {
    Some(match name {
        "p" | "div" | "blockquote" | "center" | "section" | "article" | "dl" | "dt" | "dd" => {
            ("div", None)
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => ("div", Some(("fontWeight", "bold"))),
        "span" | "font" | "label" | "small" | "big" | "abbr" | "cite" | "code" | "kbd" | "samp"
        | "var" | "mark" => ("span", None),
        "b" | "strong" => ("span", Some(("fontWeight", "bold"))),
        "i" | "em" => ("span", Some(("fontStyle", "italic"))),
        "u" | "ins" => ("span", Some(("textDecorationLine", "underline"))),
        "s" | "strike" | "del" => ("span", Some(("textDecorationLine", "line-through"))),
        "sup" => ("span", Some(("verticalAlign", "super"))),
        "sub" => ("span", Some(("verticalAlign", "sub"))),
        "ruby" => ("ruby", None),
        "rt" => ("rt", None),
        "rp" => ("rp", None),
        "ul" => ("ul", None),
        "ol" => ("ol", None),
        "li" => ("li", None),
        "table" => ("table", None),
        "thead" => ("thead", None),
        "tbody" => ("tbody", None),
        "tfoot" => ("tfoot", None),
        "tr" => ("tr", None),
        "td" => ("td", None),
        "th" => ("th", None),
        "a" => ("a", None),
        _ => return None,
    })
}

fn finish(node: Node) -> Value {
    let mut o = node.attrs;
    if let Some(tag) = node.tag {
        o.insert("tag".into(), json!(tag));
    }
    if !node.content.is_empty() {
        o.insert("content".into(), Value::Array(node.content));
    }
    Value::Object(o)
}

/// The link an `a` element becomes, from its `href`: a lookup, a web page or
/// a sound of the dictionary; `None` for other schemes (kept as text).
fn link(href: &str, refs: &mut BTreeSet<String>) -> Option<String> {
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    for scheme in ["entry://", "bword://"] {
        if lower.starts_with(scheme) {
            let word = href[scheme.len()..].trim().trim_start_matches('#');
            return (!word.is_empty()).then(|| format!("?query={}", encode(word)));
        }
    }
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Some(href.to_string());
    }
    if let Some(path) = lower.strip_prefix("sound://") {
        let key = resource_key(&href[8..])?;
        let _ = path;
        refs.insert(key.clone());
        return Some(format!("sound:{key}"));
    }
    None
}

/// An MDict definition as a glossary of one structured-content item; the
/// resources it uses are added to `refs`.
pub(crate) fn to_structured(html: &str, refs: &mut BTreeSet<String>) -> Value {
    let mut stack: Vec<Node> = vec![Node {
        tag: None,
        attrs: Map::new(),
        content: Vec::new(),
        name: String::new(),
    }];
    let mut nodes = 0usize;
    let mut text = String::new();
    let mut rest = html;
    let flush = |text: &mut String, stack: &mut Vec<Node>| {
        if !text.is_empty() {
            let t = super::html_to_text(text);
            // html_to_text trims lines: keep a separating blank
            let piece = if text.starts_with(char::is_whitespace) && !t.is_empty() {
                format!(" {t}")
            } else {
                t
            };
            let piece = if text.ends_with(char::is_whitespace) && !piece.is_empty() {
                format!("{piece} ")
            } else {
                piece
            };
            if !piece.is_empty()
                && let Some(top) = stack.last_mut()
            {
                top.content.push(Value::String(piece));
            }
            text.clear();
        }
    };
    while let Some(i) = rest.find('<') {
        text.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        if name.is_empty() {
            text.push('<');
            text.push_str(tag);
            text.push('>');
            continue;
        }
        if !closing && DROPPED.contains(&name.as_str()) {
            // the element and all it holds go
            let close = format!("</{name}");
            let lower = rest.to_ascii_lowercase();
            rest = lower
                .find(&close)
                .and_then(|at| rest[at..].find('>').map(|e| &rest[at + e + 1..]))
                .unwrap_or("");
            continue;
        }
        flush(&mut text, &mut stack);
        if closing {
            if let Some(at) = stack.iter().rposition(|n| n.name == name)
                && at > 0
            {
                while stack.len() > at {
                    let node = stack.pop().unwrap_or_else(|| unreachable!());
                    let value = finish(node);
                    if let Some(parent) = stack.last_mut() {
                        parent.content.push(value);
                    }
                }
            }
            continue;
        }
        nodes += 1;
        if nodes > MAX_NODES {
            break;
        }
        match name.as_str() {
            "br" => {
                if let Some(top) = stack.last_mut() {
                    top.content.push(json!({"tag": "br"}));
                }
                continue;
            }
            "hr" => {
                if let Some(top) = stack.last_mut() {
                    top.content.push(json!({"tag": "br"}));
                }
                continue;
            }
            "img" => {
                if let Some(key) = attribute(tag, "src").and_then(|s| resource_key(&s)) {
                    refs.insert(key.clone());
                    let mut o = json!({"tag": "img", "path": key});
                    if let Some(alt) = attribute(tag, "alt").filter(|a| !a.is_empty()) {
                        o["alt"] = json!(alt.chars().take(256).collect::<String>());
                    }
                    if let Some(top) = stack.last_mut() {
                        top.content.push(o);
                    }
                } else if let Some(alt) = attribute(tag, "alt").filter(|a| !a.is_empty())
                    && let Some(top) = stack.last_mut()
                {
                    top.content.push(Value::String(alt));
                }
                continue;
            }
            _ => {}
        }
        let Some((mapped_tag, style)) = mapped(&name) else {
            // unknown: its content stays, the element goes
            continue;
        };
        let mut attrs = Map::new();
        let mut tag_out = Some(mapped_tag);
        if mapped_tag == "a" {
            match attribute(tag, "href").and_then(|h| link(&h, refs)) {
                Some(href) => {
                    attrs.insert("href".into(), json!(href));
                }
                None => tag_out = Some("span"),
            }
        }
        if let Some((key, value)) = style {
            attrs.insert("style".into(), json!({ key: value }));
        }
        if stack.len() > MAX_DEPTH {
            // too deep: flattened into its parent
            continue;
        }
        stack.push(Node {
            tag: tag_out,
            attrs,
            content: Vec::new(),
            name,
        });
    }
    text.push_str(rest);
    flush(&mut text, &mut stack);
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_else(|| unreachable!());
        let value = finish(node);
        if let Some(parent) = stack.last_mut() {
            parent.content.push(value);
        }
    }
    let root = stack.pop().map(|n| n.content).unwrap_or_default();
    json!([{"type": "structured-content", "content": root}])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(html: &str) -> (Value, BTreeSet<String>) {
        let mut refs = BTreeSet::new();
        (to_structured(html, &mut refs), refs)
    }

    // T-DIC-036: allowlisted tags and styles, links by scheme, resources referenced.
    #[test]
    fn mdict_html_becomes_structured_content() {
        let (v, refs) = convert(
            r#"<div class="x" onclick="evil()"><b>食べる</b> <i>v</i><br><img src="\Img\A.PNG" alt="pic"><a href="entry://飲む">飲む</a> <a href="sound://snd/ta.mp3">▶</a> <a href="https://example.org">web</a> <a href="javascript:alert(1)">js</a></div>"#,
        );
        let content = &v[0]["content"][0];
        assert_eq!(content["tag"], "div");
        assert!(content.get("class").is_none() && content.get("onclick").is_none());
        let items = content["content"].as_array().unwrap();
        assert_eq!(
            items[0],
            json!({"tag":"span","style":{"fontWeight":"bold"},"content":["食べる"]})
        );
        assert!(items.contains(&json!({"tag":"br"})));
        assert!(items.contains(&json!({"tag":"img","path":"img/a.png","alt":"pic"})));
        assert!(
            items.contains(
                &json!({"tag":"a","href":"?query=%E9%A3%B2%E3%82%80","content":["飲む"]})
            )
        );
        assert!(items.contains(&json!({"tag":"a","href":"sound:snd/ta.mp3","content":["▶"]})));
        assert!(items.contains(&json!({"tag":"a","href":"https://example.org","content":["web"]})));
        assert!(
            items.contains(&json!({"tag":"span","content":["js"]})),
            "{items:?}"
        );
        assert_eq!(
            refs.into_iter().collect::<Vec<_>>(),
            ["img/a.png", "snd/ta.mp3"]
        );
    }

    // T-DIC-036, NFR-SEC-01: scripts, styles, unsafe paths and depth.
    #[test]
    fn hostile_html_is_contained() {
        let (v, refs) = convert(
            r#"a<script>alert("</div>")</script><style>*{}</style><iframe src="x">f</iframe><img src="http://evil/x.png"><img src="../../etc/passwd"><img src="data:image/png;base64,AA"><span style="position:fixed">b</span>"#,
        );
        let text = v.to_string();
        for bad in ["alert", "evil", "passwd", "data:", "position", "iframe"] {
            assert!(!text.contains(bad), "{bad} in {text}");
        }
        assert!(refs.is_empty());
        let deep = "<div>".repeat(500) + "x" + &"</div>".repeat(500);
        let (v, _) = convert(&deep);
        assert!(v.to_string().contains('x'));
        // the renderer accepts the result
        let model = crate::structured::render(&v, &crate::css::Sheet::default());
        assert!(!format!("{model:?}").is_empty());
    }
}
