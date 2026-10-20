// SPDX-License-Identifier: GPL-3.0-only

//! A term's glossary as card HTML (ANK-023), in the shape of Yomitan's Anki
//! export, which the Lapis CSS styles: the structured-content tags, `data`
//! entries as `data-sc-*` attributes and `gloss-sc-<tag>` classes. Only the
//! tags of POP-009 survive, inline styles are written back from the CSS
//! subset of DIC-035, links become `<span>`, every text is escaped and
//! nothing from the dictionary is copied as markup.
//!
//! Translated from Fushi `fushi/assets/popup/popup.js` (commit `ac8034e`):
//! `renderStructuredContent` with `exporting`, `serializeExportedGlossary`,
//! the table styles of `STRUCTURED_CONTENT_EXPORT_STYLE`.

use nikomiru_dict::css::data_attribute;
use nikomiru_dict::structured::{self, DROPPED, Image, MAX_DEPTH, MAX_NODES, Units};
use serde_json::{Map, Value};

use crate::html::escape;

/// The tags written as they are (POP-009); `a` becomes a `span`, every other
/// tag that is not dropped too.
const ALLOWED: &[&str] = &[
    "br", "ruby", "rt", "rp", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "span", "div",
    "ol", "ul", "li", "details", "summary", "img",
];

/// The inline tags some dictionaries put in plain strings (Fushi
/// `sanitizeInlineHtml`).
const INLINE: &[&str] = &[
    "b", "strong", "i", "em", "sup", "sub", "u", "span", "br", "ruby", "rt", "rp",
];

/// Yomitan's export styles for tables (`structured-content-style.json`),
/// written before the element's own.
fn table_style(tag: &str) -> &'static str {
    match tag {
        "table" => "table-layout:auto;border-collapse:collapse;",
        "thead" | "tfoot" => "font-weight:bold;",
        "th" => {
            "font-weight:bold;border-style:solid;padding:0.25em;vertical-align:top;\
             border-width:1px;border-color:currentColor;"
        }
        "td" => {
            "border-style:solid;padding:0.25em;vertical-align:top;border-width:1px;\
             border-color:currentColor;"
        }
        _ => "",
    }
}

/// Writes a term's glossary (its JSON array) as the HTML of one sense;
/// `image` gives the Anki file name of an image, or `None` to show its
/// alternative text instead.
pub fn definitions(glossary: &Value, image: &mut dyn FnMut(&Image) -> Option<String>) -> String {
    let mut w = Writer {
        out: String::new(),
        nodes: 0,
        image,
    };
    w.top(glossary);
    w.out
}

struct Writer<'a> {
    out: String,
    nodes: usize,
    image: &'a mut dyn FnMut(&Image) -> Option<String>,
}

fn is_pair(v: &Value) -> bool {
    matches!(v, Value::Array(a) if a.len() == 2 && a[0].is_string() && a[1].is_array())
}

fn is_link(v: &Value) -> bool {
    let node = match v {
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("structured-content") => {
            o.get("content")
        }
        other => Some(other),
    };
    node.and_then(|n| n.get("tag")).and_then(Value::as_str) == Some("a")
}

/// A name usable as an attribute: letters (any script), digits, `-`, `_`.
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

impl Writer<'_> {
    fn top(&mut self, glossary: &Value) {
        let items = match glossary {
            Value::Array(items) => items.as_slice(),
            other => std::slice::from_ref(other),
        };
        if !items.is_empty() && items.iter().all(is_pair) {
            self.pairs(items);
            return;
        }
        let as_list =
            items.len() > 1 && (items.iter().all(Value::is_string) || items.iter().all(is_link));
        if as_list {
            self.out.push_str("<ul>");
            for item in items {
                self.out.push_str("<li>");
                self.item(item);
                self.out.push_str("</li>");
            }
            self.out.push_str("</ul>");
        } else {
            for item in items {
                self.item(item);
            }
        }
    }

    /// Form-of pairs (`[term, [tags…]]`): one line each.
    fn pairs(&mut self, items: &[Value]) {
        let lines: Vec<String> = items
            .iter()
            .filter_map(|i| {
                let a = i.as_array()?;
                let term = escape(a.first()?.as_str()?);
                let tags: Vec<String> = a
                    .get(1)?
                    .as_array()?
                    .iter()
                    .filter_map(|t| t.as_str().map(escape))
                    .collect();
                Some(if tags.is_empty() {
                    term
                } else {
                    format!("{term} ({})", tags.join(", "))
                })
            })
            .collect();
        self.out.push_str(&lines.join("<br>"));
    }

    fn item(&mut self, item: &Value) {
        match item {
            Value::String(s) => self.text(s),
            Value::Object(o) => match o.get("type").and_then(Value::as_str) {
                Some("text") => {
                    self.text(o.get("text").and_then(Value::as_str).unwrap_or_default())
                }
                Some("image") => self.img(o),
                Some("structured-content") => {
                    if let Some(content) = o.get("content") {
                        self.out.push_str("<span>");
                        self.content(content, 0);
                        self.out.push_str("</span>");
                    }
                }
                _ => {}
            },
            Value::Array(_) if is_pair(item) => self.pairs(std::slice::from_ref(item)),
            Value::Array(a) => {
                for i in a {
                    self.item(i);
                }
            }
            _ => {}
        }
    }

    /// Plain text; the inline HTML some dictionaries put in it is reduced to
    /// [`INLINE`] without attributes.
    fn text(&mut self, text: &str) {
        if text.contains('<') && text.contains('>') {
            self.inline_html(text);
        } else {
            self.out.push_str(&crate::html::lines(text));
        }
    }

    fn inline_html(&mut self, html: &str) {
        let mut open: Vec<String> = Vec::new();
        let mut dropped = 0usize;
        let mut rest = html;
        while !rest.is_empty() {
            let (text, after) = match rest.find('<') {
                Some(at) => (&rest[..at], &rest[at..]),
                None => (rest, ""),
            };
            if !text.is_empty() && dropped == 0 {
                self.out
                    .push_str(&crate::html::lines(&structured::decode_entities(text)));
            }
            rest = after;
            if rest.is_empty() {
                break;
            }
            // `<` not followed by a tag name is text, as a browser reads it
            let starts_tag = rest[1..]
                .trim_start_matches('/')
                .starts_with(|c: char| c.is_ascii_alphabetic());
            if !starts_tag {
                if dropped == 0 {
                    self.out.push_str("&lt;");
                }
                rest = &rest[1..];
                continue;
            }
            let Some(close) = rest.find('>') else {
                if dropped == 0 {
                    self.out
                        .push_str(&escape(&structured::decode_entities(rest)));
                }
                break;
            };
            let tag = &rest[1..close];
            rest = &rest[close + 1..];
            let closing = tag.starts_with('/');
            let name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_ascii_lowercase();
            if DROPPED.contains(&name.as_str()) {
                if closing {
                    dropped = dropped.saturating_sub(1);
                } else if !tag.ends_with('/') {
                    dropped += 1;
                }
                continue;
            }
            if dropped > 0 || !INLINE.contains(&name.as_str()) {
                continue;
            }
            if name == "br" {
                self.out.push_str("<br>");
            } else if closing {
                if let Some(at) = open.iter().rposition(|t| *t == name) {
                    for t in open.drain(at..).rev() {
                        self.out.push_str(&format!("</{t}>"));
                    }
                }
            } else if !tag.ends_with('/') && open.len() < MAX_DEPTH {
                self.out.push_str(&format!("<{name}>"));
                open.push(name);
            }
        }
        for t in open.into_iter().rev() {
            self.out.push_str(&format!("</{t}>"));
        }
    }

    fn content(&mut self, content: &Value, depth: usize) {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return;
        }
        match content {
            Value::String(s) => self.out.push_str(&crate::html::lines(s)),
            Value::Array(a) => {
                for c in a {
                    self.content(c, depth);
                }
            }
            Value::Object(o) => self.element(o, depth),
            _ => {}
        }
    }

    fn element(&mut self, o: &Map<String, Value>, depth: usize) {
        let tag = o
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or("span")
            .to_ascii_lowercase();
        if DROPPED.contains(&tag.as_str()) {
            return;
        }
        if depth >= MAX_DEPTH {
            // too deep: only the text survives
            self.out.push_str(&escape(&plain(o.get("content"))));
            return;
        }
        match tag.as_str() {
            "br" => return self.out.push_str("<br>"),
            "img" => return self.img(o),
            _ => {}
        }
        let class = if is_name(&tag) {
            format!("gloss-sc-{tag}")
        } else {
            "gloss-sc-span".to_string()
        };
        let written = if ALLOWED.contains(&tag.as_str()) {
            tag.as_str()
        } else {
            "span"
        };
        let mut attrs = format!(r#" class="{class}""#);
        if let Some(lang) = o.get("lang").and_then(Value::as_str)
            && lang.len() <= 35
            && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            attrs.push_str(&format!(r#" lang="{lang}""#));
        }
        if let Some(title) = o.get("title").and_then(Value::as_str) {
            attrs.push_str(&format!(r#" title="{}""#, escape(title)));
        }
        for (key, value) in structured::data_of(o) {
            if is_name(&key) {
                attrs.push_str(&format!(
                    r#" {}="{}""#,
                    data_attribute(&key),
                    escape(&value)
                ));
            }
        }
        if matches!(written, "td" | "th") {
            for (key, attr) in [("colSpan", "colspan"), ("rowSpan", "rowspan")] {
                if let Some(n) = o
                    .get(key)
                    .and_then(Value::as_u64)
                    .filter(|n| (2..=1000).contains(n))
                {
                    attrs.push_str(&format!(r#" {attr}="{n}""#));
                }
            }
        }
        if written == "details" && o.get("open").and_then(Value::as_bool) == Some(true) {
            attrs.push_str(" open");
        }
        let style = format!(
            "{}{}",
            table_style(written),
            structured::inline_style(o).to_css()
        );
        if !style.is_empty() {
            attrs.push_str(&format!(r#" style="{}""#, escape(&style)));
        }
        if written == "table" {
            // a wide table scrolls inside the card instead of widening it
            self.out
                .push_str(r#"<div style="display:block;max-width:100%;overflow-x:auto;">"#);
        }
        self.out.push_str(&format!("<{written}{attrs}>"));
        if let Some(content) = o.get("content") {
            self.content(content, depth + 1);
        }
        self.out.push_str(&format!("</{written}>"));
        if written == "table" {
            self.out.push_str("</div>");
        }
    }

    fn img(&mut self, o: &Map<String, Value>) {
        let Some(image) = structured::image(o) else {
            return;
        };
        let Some(file) = (self.image)(&image) else {
            if !image.alt.is_empty() {
                self.out.push_str(&escape(&image.alt));
            }
            return;
        };
        let unit = match image.units {
            Units::Em => "em",
            Units::Px => "px",
        };
        let size = |preferred: &str, given: Option<f32>| {
            o.get(preferred)
                .and_then(Value::as_f64)
                .filter(|v| *v > 0.0 && *v < 10_000.0)
                .map(|v| v as f32)
                .or(given)
        };
        let mut style = String::new();
        if let Some(w) = size("preferredWidth", image.width) {
            style.push_str(&format!("width:{w}{unit};"));
        }
        if let Some(h) = size("preferredHeight", image.height) {
            style.push_str(&format!("height:{h}{unit};"));
        }
        if let Some(v) = o.get("verticalAlign").and_then(Value::as_str).filter(|v| {
            matches!(
                *v,
                "baseline"
                    | "sub"
                    | "super"
                    | "text-top"
                    | "text-bottom"
                    | "middle"
                    | "top"
                    | "bottom"
            )
        }) {
            style.push_str(&format!("vertical-align:{v};"));
        }
        if image.pixelated {
            style.push_str("image-rendering:pixelated;");
        }
        let mut attrs = format!(r#" src="{}""#, escape(&file));
        attrs.push_str(&format!(r#" alt="{}""#, escape(&image.alt)));
        if !image.title.is_empty() {
            attrs.push_str(&format!(r#" title="{}""#, escape(&image.title)));
        }
        if !style.is_empty() {
            attrs.push_str(&format!(r#" style="{style}""#));
        }
        self.out.push_str(&format!("<img{attrs}>"));
    }
}

/// The text of some content, without its markup.
fn plain(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().map(|c| plain(Some(c))).collect(),
        Some(Value::Object(o)) => plain(o.get("content")),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn html(glossary: Value) -> String {
        definitions(&glossary, &mut |image| {
            Some(format!("nikomiru_{}.png", image.path.len()))
        })
    }

    // T-ANK-021
    #[test]
    fn plain_definitions() {
        assert_eq!(html(json!(["sun"])), "sun");
        assert_eq!(
            html(json!(["sun", "day"])),
            "<ul><li>sun</li><li>day</li></ul>"
        );
        assert_eq!(html(json!(["a<b & c"])), "a&lt;b &amp; c");
        // with both brackets it is read as inline HTML, tags only where a tag starts
        assert_eq!(html(json!(["x < y > z"])), "x &lt; y &gt; z");
        assert_eq!(html(json!(["<i>a</i> 1<2>0"])), "<i>a</i> 1&lt;2&gt;0");
        assert_eq!(html(json!(["line one\nline two"])), "line one<br>line two");
        assert_eq!(html(json!([{"type": "text", "text": "x"}])), "x");
        // inline HTML in a string: only the inline tags, without attributes
        assert_eq!(
            html(json!([
                "<b onclick=\"x()\">bold</b> <script>alert(1)</script><a href=\"http://x\">t</a>"
            ])),
            "<b>bold</b> t"
        );
        // form-of pairs
        assert_eq!(
            html(json!([["时", ["Hyōgai"]], ["时", ["alt-of"]]])),
            "时 (Hyōgai)<br>时 (alt-of)"
        );
    }

    // T-ANK-023
    #[test]
    fn structured_content_keeps_yomitans_shape() {
        let g = json!([{"type": "structured-content", "content": [
            {"tag": "span", "data": {"content": "tag", "subsenseNumber": "1"}, "style": {"fontWeight": "bold", "position": "absolute"}, "content": "名"},
            {"tag": "div", "data": {"用例囲み-g": ""}, "lang": "ja", "content": [
                {"tag": "ruby", "content": ["漢", {"tag": "rt", "content": "かん"}]},
            ]},
            {"tag": "a", "href": "?query=x", "content": "link"},
            {"tag": "table", "content": [{"tag": "tr", "content": [{"tag": "td", "colSpan": 2, "content": "cell"}]}]},
        ]}]);
        assert_eq!(
            html(g),
            "<span>\
             <span class=\"gloss-sc-span\" data-sc-content=\"tag\" data-sc-subsense-number=\"1\" style=\"font-weight:bold;\">名</span>\
             <div class=\"gloss-sc-div\" lang=\"ja\" data-sc用例囲み-g=\"\">\
             <ruby class=\"gloss-sc-ruby\">漢<rt class=\"gloss-sc-rt\">かん</rt></ruby></div>\
             <span class=\"gloss-sc-a\">link</span>\
             <div style=\"display:block;max-width:100%;overflow-x:auto;\">\
             <table class=\"gloss-sc-table\" style=\"table-layout:auto;border-collapse:collapse;\">\
             <tr class=\"gloss-sc-tr\"><td class=\"gloss-sc-td\" colspan=\"2\" style=\"border-style:solid;padding:0.25em;vertical-align:top;border-width:1px;border-color:currentColor;\">cell</td></tr>\
             </table></div></span>"
        );
    }

    // T-SEC-001 for cards
    #[test]
    fn hostile_content_is_reduced_to_the_allowlist() {
        let g = json!([{"type": "structured-content", "content": [
            {"tag": "script", "content": "alert(1)"},
            {"tag": "iframe", "content": "x"},
            {"tag": "span", "data": {"x\" onclick=\"y": "1", "ok": "\"><script>"}, "content": "safe"},
            {"tag": "div", "style": {"backgroundImage": "url(http://tracker.example/x.png)", "color": "red; background:url(x)"}, "content": "styled"},
            {"tag": "marquee", "content": "kept as span"},
            {"tag": "img", "path": "../../etc/passwd"},
            {"tag": "img", "path": "img/a.png", "alt": "<alt>"},
            {"tag": "span", "lang": "en\" onload=\"x", "title": "\"><b>", "content": "t"},
        ]}]);
        let out = html(g);
        for bad in [
            "<script", "<iframe", "onclick", "url(", "onload", "<marquee", "passwd", "<b>",
        ] {
            assert!(!out.contains(bad), "{bad} in {out}");
        }
        assert!(
            out.contains("data-sc-ok=\"&quot;&gt;&lt;script&gt;\""),
            "{out}"
        );
        assert!(
            out.contains("<span class=\"gloss-sc-marquee\">kept as span</span>"),
            "{out}"
        );
        assert!(
            out.contains(r#"<img src="nikomiru_9.png" alt="&lt;alt&gt;">"#),
            "{out}"
        );
        // an image Anki will not get shows its alternative text
        let alt = definitions(
            &json!([{"type": "image", "path": "a.png", "alt": "外字"}]),
            &mut |_| None,
        );
        assert_eq!(alt, "外字");
        // deep nesting ends without overflowing
        let mut deep = json!("bottom");
        for _ in 0..1_000 {
            deep = json!({"tag": "span", "content": deep});
        }
        let out = html(json!([{"type": "structured-content", "content": deep}]));
        assert!(out.contains("bottom"));
    }

    #[test]
    fn image_sizes_and_units() {
        let out = html(json!([{"type": "structured-content", "content":
            {"tag": "img", "path": "gaiji/x.svg", "width": 1, "height": 1, "sizeUnits": "em",
             "preferredWidth": 1.5, "verticalAlign": "middle", "title": "t"}}]));
        assert_eq!(
            out,
            r#"<span><img src="nikomiru_11.png" alt="" title="t" style="width:1.5em;height:1em;vertical-align:middle;"></span>"#
        );
    }
}
