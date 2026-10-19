// SPDX-License-Identifier: GPL-3.0-only

//! The part of a dictionary's `styles.css` that Nikomiru applies (DIC-035):
//! rules whose selectors are a single compound of a tag, `gloss-sc-<tag>`
//! classes and `data-sc-*` attributes (what Yomitan dictionaries use to style
//! their structured content), with the properties of [`Style`]. Combinators,
//! pseudo-classes, at-rules and other properties are ignored.

use crate::style::Style;

/// One selector and its declarations.
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    tag: Option<String>,
    /// `data-sc-*` attributes: name without the prefix, required value.
    attrs: Vec<(String, Option<String>)>,
    /// (attributes and classes, tags), as CSS specificity.
    specificity: (usize, usize),
    order: usize,
    style: Style,
}

/// The applicable rules of a stylesheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    rules: Vec<Rule>,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// A parsed compound selector.
struct Selector {
    tag: Option<String>,
    attrs: Vec<(String, Option<String>)>,
    specificity: (usize, usize),
}

/// A compound selector such as `span.gloss-sc-span[data-sc-class="tag"]`.
fn selector(text: &str) -> Option<Selector> {
    let text = text.trim();
    if text.is_empty() || text.contains([' ', '>', '+', '~', ':', '*', '#', '\t', '\n']) {
        return None;
    }
    let end = text.find(['.', '[']).unwrap_or(text.len());
    let tag = &text[..end];
    if !tag.is_empty() && !is_ident(tag) {
        return None;
    }
    let tag = (!tag.is_empty()).then(|| tag.to_ascii_lowercase());
    let mut attrs = Vec::new();
    let mut classes = 0;
    let mut rest = &text[end..];
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('.') {
            let len = after.find(['.', '[']).unwrap_or(after.len());
            let class = &after[..len];
            // structured content nodes only carry `gloss-sc-<their tag>`
            let of = class.strip_prefix("gloss-sc-")?;
            if tag.as_deref().is_some_and(|t| t != of) {
                return None;
            }
            attrs.push((format!("\0tag:{of}"), None));
            classes += 1;
            rest = &after[len..];
        } else {
            let after = rest.strip_prefix('[')?;
            let close = after.find(']')?;
            let inner = &after[..close];
            let (name, value) = match inner.split_once('=') {
                Some((n, v)) => {
                    let v = v.trim();
                    let unquoted = v
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                        .unwrap_or(v);
                    (n.trim(), Some(unquoted.to_string()))
                }
                None => (inner.trim(), None),
            };
            // `~=`, `^=` and friends are not supported
            if name.ends_with(['~', '^', '$', '*', '|']) {
                return None;
            }
            let key = name
                .strip_prefix("data-sc-")
                .or_else(|| name.strip_prefix("data-sc"))?;
            attrs.push((key.to_string(), value));
            classes += 1;
            rest = &after[close + 1..];
        }
    }
    Some(Selector {
        specificity: (classes, usize::from(tag.is_some())),
        tag,
        attrs,
    })
}

fn declarations(block: &str) -> Style {
    let mut style = Style::default();
    for decl in block.split(';') {
        if let Some((property, value)) = decl.split_once(':') {
            style.set(property.trim(), value.trim());
        }
    }
    style
}

impl Sheet {
    /// The applicable part of a stylesheet. Never fails: what cannot be read
    /// is skipped.
    pub fn parse(css: &str) -> Self {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut rest = css.as_str();
        let mut order = 0;
        while let Some(open) = rest.find('{') {
            let prelude = rest[..open].trim();
            // the block, braces balanced (nested blocks of at-rules)
            let mut depth = 0;
            let mut close = None;
            for (i, c) in rest[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close) = close else { break };
            let body = &rest[open + 1..close];
            rest = &rest[close + 1..];
            // `@import url(…);` has no block: it ends at the `;` in the prelude
            let prelude = prelude.rsplit(';').next().unwrap_or_default().trim();
            if prelude.starts_with('@') || body.contains('{') {
                continue;
            }
            let style = declarations(body);
            if style.is_empty() {
                continue;
            }
            for sel in prelude.split(',') {
                if let Some(Selector {
                    tag,
                    attrs,
                    specificity,
                }) = selector(sel)
                {
                    rules.push(Rule {
                        tag,
                        attrs,
                        specificity,
                        order,
                        style: style.clone(),
                    });
                    order += 1;
                }
            }
        }
        rules.sort_by_key(|r| (r.specificity, r.order));
        Self { rules }
    }

    /// Whether no rule applies to anything.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The style of an element with this tag and these `data` entries.
    pub fn style_for(&self, tag: &str, data: &[(String, String)]) -> Style {
        let mut out = Style::default();
        for r in &self.rules {
            let tag_ok = r.tag.as_deref().is_none_or(|t| t == tag);
            let attrs_ok = r
                .attrs
                .iter()
                .all(|(name, value)| match name.strip_prefix("\0tag:") {
                    Some(of) => of == tag,
                    None => data.iter().any(|(k, v)| {
                        kebab(k) == *name && value.as_ref().is_none_or(|want| want == v)
                    }),
                });
            if tag_ok && attrs_ok {
                out.merge(&r.style);
            }
        }
        out
    }
}

/// `dataKey` → `data-key`, as the browser names `data` attributes.
pub fn kebab(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 2);
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // T-DIC-035
    #[test]
    fn simple_selectors_apply_by_specificity() {
        let sheet = Sheet::parse(
            r#"
            /* tags */
            span[data-sc-class="tag"] { font-weight: bold; color: #fff; background-color: #565656; border-radius: 0.3em; }
            [data-sc-class] { color: red; font-style: italic }
            .gloss-sc-li { margin-left: 1em }
            div .gloss-sc-span { color: blue }
            a:hover { color: green }
            @media (prefers-color-scheme: dark) { span { color: white } }
            ul[data-sc-content='glossary'] { list-style-type: "◦" }
            "#,
        );
        let tag = sheet.style_for("span", &data(&[("class", "tag")]));
        assert_eq!(
            tag.color.as_deref(),
            Some("#fff"),
            "the more specific rule wins"
        );
        assert_eq!((tag.bold, tag.italic), (Some(true), Some(true)));
        let other = sheet.style_for("div", &data(&[("class", "x")]));
        assert_eq!(other.color.as_deref(), Some("red"));
        assert!(
            sheet.style_for("span", &[]).is_empty(),
            "combinators, pseudo-classes and @media are skipped"
        );
        assert!(sheet.style_for("li", &[]).margin.left.is_some());
        let list = sheet.style_for("ul", &data(&[("content", "glossary")]));
        assert_eq!(list.list_marker.as_deref(), Some("\"◦\""));
    }

    #[test]
    fn hostile_css_is_harmless() {
        let sheet = Sheet::parse(
            "@import url(http://evil/x.css); span { background: url(http://evil/x.png); behavior: url(x) } div { color: expression(alert(1)) } }}} {",
        );
        assert!(sheet.is_empty());
        assert_eq!(kebab("subsenseNumber"), "subsense-number");
    }
}
