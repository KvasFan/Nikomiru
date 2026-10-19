// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan glossaries as a safe view model for QML (POP-009, NFR-SEC-01): a
//! port of the structure of Fushi's `renderStructuredContent`
//! (`fushi/assets/popup/popup.js`, ac8034e), producing data instead of HTML.
//!
//! Block elements (`div`, lists, tables, `details`) become [`Block`]s; inline
//! content becomes flat [`Run`]s with their style already inherited, which QML
//! lays out in a flow of plain-text items. Only the tags of the allowlist
//! survive; `script`, `style` and similar are dropped with their content;
//! styles go through [`Style`]; images are paths inside the dictionary; links
//! are a lookup or an `http(s)` address. Nothing from the dictionary reaches
//! QML as markup.

use serde::Serialize;
use serde_json::Value;

use crate::css::{Sheet, kebab};
use crate::style::Style;

/// Nesting deeper than this is flattened (hostile input).
const MAX_DEPTH: usize = 48;
/// Nodes read per glossary at most.
const MAX_NODES: usize = 20_000;

/// Tags whose content is never shown.
const DROPPED: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "svg", "math", "template", "noscript", "link",
    "meta", "form", "input", "button", "select", "textarea", "frame", "frameset", "applet",
    "audio", "video", "source",
];

/// A link target (POP-011).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "to", rename_all = "lowercase")]
pub enum Target {
    /// A new lookup in a nested popup.
    Lookup(String),
    /// A web page, opened in the system browser.
    External(String),
}

/// A dictionary image (POP-010).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    /// Path inside the dictionary (normalized, never `..`).
    pub path: String,
    /// Width, in `units`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Height, in `units`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// `px` or `em`.
    pub units: &'static str,
    /// Nearest-neighbour scaling.
    pub pixelated: bool,
    /// Shown as a link to open it instead of the image.
    pub collapsed: bool,
    /// Tooltip.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Replacement text.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub alt: String,
}

/// Inline content.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Run {
    /// Text.
    Text {
        /// The text, shown as plain text.
        t: String,
        /// Its style, inherited and own.
        s: Style,
    },
    /// Text with a reading above it.
    Ruby {
        /// Base text.
        t: String,
        /// Reading.
        rt: String,
        /// Style.
        s: Style,
    },
    /// A link.
    Link {
        /// Its text (without `<rt>`, BUG-2456).
        t: String,
        /// Where it goes.
        target: Target,
        /// Style.
        s: Style,
    },
    /// An image.
    Image {
        /// The image.
        image: Image,
    },
    /// A line break.
    Br,
}

/// A table cell.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    /// Grid row of its top-left corner.
    pub row: u32,
    /// Grid column of its top-left corner.
    pub col: u32,
    /// `th`.
    pub header: bool,
    /// Columns spanned.
    pub colspan: u32,
    /// Rows spanned.
    pub rowspan: u32,
    /// Box style.
    pub s: Style,
    /// Width of its longest line, in `em`: what the column needs (CJK
    /// characters count 1, others about half).
    pub natural_em: f32,
    /// Content.
    pub blocks: Vec<Block>,
}

/// Width in `em` of the longest line of some blocks.
fn natural_em(blocks: &[Block]) -> f32 {
    let char_em = |c: char| if (c as u32) >= 0x1100 { 1.0 } else { 0.55 };
    let mut widest: f32 = 0.0;
    for b in blocks {
        let w = match b {
            Block::Flow { runs } => {
                let mut line: f32 = 0.0;
                let mut best: f32 = 0.0;
                for r in runs {
                    match r {
                        Run::Br => {
                            best = best.max(line);
                            line = 0.0;
                        }
                        Run::Text { t, .. } | Run::Link { t, .. } | Run::Ruby { t, .. } => {
                            line += t.chars().map(char_em).sum::<f32>();
                        }
                        Run::Image { image } => {
                            line += image
                                .width
                                .map_or(2.0, |w| if image.units == "em" { w } else { w / 16.0 })
                        }
                    }
                }
                best.max(line)
            }
            Block::Box { blocks, .. } => natural_em(blocks),
            Block::List { items, .. } => items
                .iter()
                .map(|i| natural_em(i) + 1.5)
                .fold(0.0, f32::max),
            Block::Table { rows, .. } => rows
                .iter()
                .map(|r| r.iter().map(|c| c.natural_em).sum::<f32>())
                .fold(0.0, f32::max),
            Block::Details { blocks, .. } => natural_em(blocks) + 1.0,
            Block::Pairs { items } => items
                .iter()
                .map(|(t, tags)| {
                    t.chars().map(char_em).sum::<f32>()
                        + tags
                            .iter()
                            .map(|g| g.chars().count() as f32 * 0.6 + 0.8)
                            .sum::<f32>()
                })
                .fold(0.0, f32::max),
        };
        widest = widest.max(w);
    }
    widest
}

/// Block content.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Block {
    /// Inline runs, laid out in a flow.
    Flow {
        /// The runs.
        runs: Vec<Run>,
    },
    /// A box (`div`, a stray `li`).
    Box {
        /// Its style (box properties).
        s: Style,
        /// Content.
        blocks: Vec<Block>,
    },
    /// `ol` / `ul`.
    List {
        /// Numbered.
        ordered: bool,
        /// Box style, with the marker.
        s: Style,
        /// One list of blocks per item.
        items: Vec<Vec<Block>>,
    },
    /// A table, in a scrollable container.
    Table {
        /// Box style.
        s: Style,
        /// Grid columns.
        columns: u32,
        /// Rows of cells, each placed in the grid (`row`, `col`).
        rows: Vec<Vec<Cell>>,
    },
    /// `details` with its `summary`.
    Details {
        /// Open at first.
        open: bool,
        /// The summary line.
        summary: Vec<Run>,
        /// The hidden content.
        blocks: Vec<Block>,
    },
    /// `[term, [tags…]]` items (form-of entries, BUG-057).
    Pairs {
        /// Term and its tags.
        items: Vec<(String, Vec<String>)>,
    },
}

struct Ctx<'a> {
    sheet: &'a Sheet,
    nodes: usize,
    /// The elements above the current one, for descendant selectors.
    path: Vec<(String, Vec<(String, String)>)>,
}

impl Ctx<'_> {
    /// Runs `f` with `tag` (and its `data`) as the innermost ancestor.
    fn inside<R>(
        &mut self,
        tag: &str,
        o: &serde_json::Map<String, Value>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.path.push((tag.to_string(), data_of(o)));
        let r = f(self);
        self.path.pop();
        r
    }
}

#[derive(Default)]
struct Out {
    blocks: Vec<Block>,
    runs: Vec<Run>,
}

impl Out {
    fn flush(&mut self) {
        if !self.runs.is_empty() {
            let runs = std::mem::take(&mut self.runs);
            self.blocks.push(Block::Flow { runs });
        }
    }

    fn block(&mut self, b: Block) {
        self.flush();
        self.blocks.push(b);
    }

    fn finish(mut self) -> Vec<Block> {
        self.flush();
        self.blocks
    }
}

/// A glossary (the JSON array of a term) as blocks, styled by the
/// dictionary's `sheet`.
pub fn render(glossary: &Value, sheet: &Sheet) -> Vec<Block> {
    let mut ctx = Ctx {
        sheet,
        nodes: 0,
        path: Vec::new(),
    };
    let items = match glossary {
        Value::Array(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    if !items.is_empty() && items.iter().all(is_pair) {
        return vec![pairs(items)];
    }
    let as_list = items.len() > 1
        && (items.iter().all(Value::is_string)
            || items
                .iter()
                .all(|i| structured(i).get("tag") == Some(&json_str("a"))));
    if as_list {
        let list = items
            .iter()
            .map(|item| {
                let mut out = Out::default();
                item_into(&mut ctx, item, &mut out);
                out.finish()
            })
            .collect();
        return vec![Block::List {
            ordered: false,
            s: Style::default(),
            items: list,
        }];
    }
    let mut out = Out::default();
    for item in items {
        item_into(&mut ctx, item, &mut out);
    }
    out.finish()
}

fn json_str(s: &str) -> Value {
    Value::String(s.to_string())
}

/// The content of a `structured-content` item, or the item.
fn structured(item: &Value) -> &Value {
    match item {
        Value::Object(o) if o.get("type") == Some(&json_str("structured-content")) => {
            o.get("content").unwrap_or(item)
        }
        _ => item,
    }
}

fn is_pair(item: &Value) -> bool {
    matches!(item.as_array().map(Vec::as_slice), Some([Value::String(_), Value::Array(tags)]) if tags.iter().all(Value::is_string))
}

fn pairs(items: &[Value]) -> Block {
    let items = items
        .iter()
        .filter_map(|i| {
            let a = i.as_array()?;
            let tags = a
                .get(1)?
                .as_array()?
                .iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect();
            Some((a.first()?.as_str()?.to_string(), tags))
        })
        .collect();
    Block::Pairs { items }
}

/// One top-level item of a glossary.
fn item_into(ctx: &mut Ctx<'_>, item: &Value, out: &mut Out) {
    match item {
        Value::String(s) => text_into(s, &Style::default(), out),
        Value::Object(o) => match o.get("type").and_then(Value::as_str) {
            Some("text") => text_into(
                o.get("text").and_then(Value::as_str).unwrap_or_default(),
                &Style::default(),
                out,
            ),
            Some("image") => {
                if let Some(image) = image(o) {
                    out.runs.push(Run::Image { image });
                }
            }
            Some("structured-content") => {
                if let Some(content) = o.get("content") {
                    content_into(ctx, content, &Style::default(), out, 0);
                }
            }
            _ => {}
        },
        Value::Array(_) if is_pair(item) => out.block(pairs(std::slice::from_ref(item))),
        Value::Array(a) => {
            for i in a {
                item_into(ctx, i, out);
            }
        }
        _ => {}
    }
}

/// Plain text: line breaks become [`Run::Br`], `http(s)` addresses links.
fn text_into(text: &str, style: &Style, out: &mut Out) {
    if text.contains('<') && text.contains('>') {
        return inline_html_into(text, style, out);
    }
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.runs.push(Run::Br);
        }
        let line = line.trim_end_matches('\r');
        let mut rest = line;
        while let Some(at) = rest.find("http://").or_else(|| rest.find("https://")) {
            if at > 0 {
                out.runs.push(Run::Text {
                    t: rest[..at].to_string(),
                    s: style.clone(),
                });
            }
            let len = rest[at..]
                .find(|c: char| c.is_whitespace() || "<>　，、。！）)\"'".contains(c))
                .unwrap_or(rest.len() - at);
            let url = &rest[at..at + len];
            out.runs.push(Run::Link {
                t: url.to_string(),
                target: Target::External(url.to_string()),
                s: style.clone(),
            });
            rest = &rest[at + len..];
        }
        if !rest.is_empty() {
            out.runs.push(Run::Text {
                t: rest.to_string(),
                s: style.clone(),
            });
        }
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let entity = rest.find(';').filter(|&e| e <= 10).map(|e| &rest[1..e]);
        let decoded = entity.and_then(|e| match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        });
        match (decoded, entity) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &rest[e.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The inline HTML some dictionaries put in plain strings (Fushi
/// `sanitizeInlineHtml`): `b strong i em sup sub span br ruby rt rp` are
/// understood, every other tag is dropped (keeping its text), attributes are
/// ignored.
fn inline_html_into(html: &str, base: &Style, out: &mut Out) {
    let mut stack: Vec<(String, Style)> = vec![(String::new(), base.clone())];
    let mut ruby: Option<(String, String, bool)> = None; // base, rt, inside rt
    let mut dropped = 0usize;
    let mut rest = html;
    while !rest.is_empty() {
        let (text, after) = match rest.find('<') {
            Some(0) => ("", rest),
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, ""),
        };
        if !text.is_empty() && dropped == 0 {
            let t = decode_entities(text);
            let style = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
            match ruby.as_mut() {
                Some((b, _, false)) => b.push_str(&t),
                Some((_, rt, true)) => rt.push_str(&t),
                None => text_into(&t, &style, out),
            }
        }
        rest = after;
        if rest.is_empty() {
            break;
        }
        let Some(close) = rest.find('>') else {
            // a lone `<`: text
            if dropped == 0 {
                text_into(&decode_entities(rest), base, out);
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
        if dropped > 0 {
            continue;
        }
        match (name.as_str(), closing) {
            ("br", _) => out.runs.push(Run::Br),
            ("ruby", false) => ruby = Some((String::new(), String::new(), false)),
            ("ruby", true) => {
                if let Some((t, rt, _)) = ruby.take() {
                    let s = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
                    out.runs.push(Run::Ruby { t, rt, s });
                }
            }
            ("rt", open) => {
                if let Some(r) = ruby.as_mut() {
                    r.2 = !open;
                }
            }
            ("rp", _) => {}
            (n, false) if stack.len() < MAX_DEPTH => {
                let mut s = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
                match n {
                    "b" | "strong" => s.bold = Some(true),
                    "i" | "em" => s.italic = Some(true),
                    "sup" => s.vertical_align = Some("super"),
                    "sub" => s.vertical_align = Some("sub"),
                    "u" => s.underline = Some(true),
                    _ => {}
                }
                if !tag.ends_with('/') {
                    stack.push((n.to_string(), s));
                }
            }
            (n, true) => {
                if let Some(at) = stack.iter().rposition(|(t, _)| t == n)
                    && at > 0
                {
                    stack.truncate(at);
                }
            }
            _ => {}
        }
    }
    if let Some((t, rt, _)) = ruby {
        out.runs.push(Run::Ruby {
            t,
            rt,
            s: base.clone(),
        });
    }
}

/// Visible text of content, without `rt` and `rp` (BUG-2456).
fn base_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(base_text).collect(),
        Value::Object(o) => match o.get("tag").and_then(Value::as_str) {
            Some("rt" | "rp") => String::new(),
            Some(t) if DROPPED.contains(&t) => String::new(),
            _ => o.get("content").map(base_text).unwrap_or_default(),
        },
        _ => String::new(),
    }
}

/// Text of `rt` elements.
fn rt_text(content: &Value) -> String {
    match content {
        Value::Array(a) => a.iter().map(rt_text).collect(),
        Value::Object(o) if o.get("tag").and_then(Value::as_str) == Some("rt") => {
            o.get("content").map(base_text).unwrap_or_default()
        }
        Value::Object(o) if o.get("tag").and_then(Value::as_str) != Some("rp") => {
            o.get("content").map(rt_text).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// A media path inside the dictionary: relative, `/` separated, no `..`.
pub fn media_path(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    let p = p.trim_start_matches('/');
    if p.is_empty() || p.contains(':') || p.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return None;
    }
    Some(p.to_string())
}

fn image(o: &serde_json::Map<String, Value>) -> Option<Image> {
    let path = media_path(o.get("path")?.as_str()?)?;
    let num = |k: &str| {
        o.get(k)
            .and_then(Value::as_f64)
            .filter(|v| *v > 0.0 && *v < 10_000.0)
            .map(|v| v as f32)
    };
    let text = |k: &str| {
        o.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let rendering = o.get("imageRendering").and_then(Value::as_str);
    Some(Image {
        path,
        width: num("width"),
        height: num("height"),
        units: if o.get("sizeUnits").and_then(Value::as_str) == Some("em") {
            "em"
        } else {
            "px"
        },
        pixelated: o.get("pixelated").and_then(Value::as_bool).unwrap_or(false)
            || matches!(rendering, Some("pixelated" | "crisp-edges")),
        collapsed: o.get("collapsed").and_then(Value::as_bool).unwrap_or(false),
        title: text("title"),
        alt: text("alt"),
    })
}

fn data_of(o: &serde_json::Map<String, Value>) -> Vec<(String, String)> {
    o.get("data")
        .and_then(Value::as_object)
        .map(|d| {
            d.iter()
                .filter_map(|(k, v)| Some((kebab(k), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// The element's own style: the sheet's, then its `style` object.
fn own_style(ctx: &Ctx<'_>, tag: &str, o: &serde_json::Map<String, Value>) -> Style {
    let data = data_of(o);
    let mut path: Vec<crate::css::Element<'_>> = ctx
        .path
        .iter()
        .map(|(t, d)| (t.as_str(), d.as_slice()))
        .collect();
    path.push((tag, &data));
    let mut style = ctx.sheet.style_for(&path);
    if let Some(inline) = o.get("style").and_then(Value::as_object) {
        let mut s = Style::default();
        for (k, v) in inline {
            let value = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            s.set(k, &value);
        }
        style.merge(&s);
    }
    style
}

fn content_into(
    ctx: &mut Ctx<'_>,
    content: &Value,
    inherited: &Style,
    out: &mut Out,
    depth: usize,
) {
    ctx.nodes += 1;
    if ctx.nodes > MAX_NODES {
        return;
    }
    match content {
        Value::String(s) => text_into(s, inherited, out),
        Value::Array(a) => {
            for c in a {
                content_into(ctx, c, inherited, out, depth);
            }
        }
        Value::Object(o) => element_into(ctx, o, inherited, out, depth),
        _ => {}
    }
}

fn children(
    ctx: &mut Ctx<'_>,
    o: &serde_json::Map<String, Value>,
    inherited: &Style,
    depth: usize,
) -> Vec<Block> {
    let mut inner = Out::default();
    if let Some(c) = o.get("content") {
        content_into(ctx, c, inherited, &mut inner, depth + 1);
    }
    inner.finish()
}

/// Runs of inline content (a summary line).
fn runs_of(blocks: Vec<Block>) -> Vec<Run> {
    let mut runs = Vec::new();
    for b in blocks {
        match b {
            Block::Flow { runs: r } => {
                if !runs.is_empty() {
                    runs.push(Run::Br);
                }
                runs.extend(r);
            }
            Block::Box { blocks, .. } => runs.extend(runs_of(blocks)),
            _ => {}
        }
    }
    runs
}

fn element_into(
    ctx: &mut Ctx<'_>,
    o: &serde_json::Map<String, Value>,
    inherited: &Style,
    out: &mut Out,
    depth: usize,
) {
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
        text_into(&base_text(&Value::Object(o.clone())), inherited, out);
        return;
    }
    let own = own_style(ctx, &tag, o);
    let mut text_style = inherited.clone();
    text_style.merge(&own);
    let inner_inherited = text_style.inherited();
    ctx.path.push((tag.clone(), data_of(o)));
    element_body(ctx, o, &tag, own, text_style, &inner_inherited, out, depth);
    ctx.path.pop();
}

/// The rendering of an element, with the element on the path.
#[allow(clippy::too_many_arguments)]
fn element_body(
    ctx: &mut Ctx<'_>,
    o: &serde_json::Map<String, Value>,
    tag: &str,
    own: Style,
    text_style: Style,
    inner_inherited: &Style,
    out: &mut Out,
    depth: usize,
) {
    let inner_inherited = inner_inherited.clone();
    match tag {
        "br" => out.runs.push(Run::Br),
        "img" => {
            if let Some(image) = image(o) {
                out.runs.push(Run::Image { image });
            }
        }
        "ruby" => {
            let content = o.get("content").cloned().unwrap_or(Value::Null);
            out.runs.push(Run::Ruby {
                t: base_text(&content),
                rt: rt_text(&content),
                s: text_style,
            });
        }
        "rt" | "rp" => {}
        "a" => {
            let content = o.get("content").cloned().unwrap_or(Value::Null);
            let text = base_text(&content);
            let href = o.get("href").and_then(Value::as_str).unwrap_or_default();
            let target = if href.starts_with("http://") || href.starts_with("https://") {
                Target::External(href.to_string())
            } else {
                let query = href
                    .split_once('?')
                    .and_then(|(_, q)| {
                        q.split('&').find_map(|kv| {
                            let (k, v) = kv.split_once('=')?;
                            (k == "query").then(|| percent_decode(v))
                        })
                    })
                    .filter(|q| !q.is_empty());
                Target::Lookup(query.unwrap_or_else(|| text.clone()))
            };
            out.runs.push(Run::Link {
                t: text,
                target,
                s: text_style,
            });
        }
        "div" | "li" | "summary" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" => {
            let blocks = children(ctx, o, &inner_inherited, depth);
            out.block(Block::Box { s: own, blocks });
        }
        "ol" | "ul" => {
            let items = match o.get("content") {
                Some(Value::Array(items)) => items.clone(),
                Some(single) => vec![single.clone()],
                None => Vec::new(),
            };
            let items = items
                .iter()
                .map(|item| {
                    let mut inner = Out::default();
                    match item {
                        Value::Object(li)
                            if li.get("tag").and_then(Value::as_str) == Some("li") =>
                        {
                            let li_style = own_style(ctx, "li", li);
                            let mut s = inner_inherited.clone();
                            s.merge(&li_style);
                            let blocks = ctx.inside("li", li, |ctx| {
                                children(ctx, li, &s.inherited(), depth + 1)
                            });
                            if li_style.is_empty() {
                                return blocks;
                            }
                            inner.block(Block::Box {
                                s: li_style,
                                blocks,
                            });
                        }
                        other => content_into(ctx, other, &inner_inherited, &mut inner, depth + 1),
                    }
                    inner.finish()
                })
                .collect();
            out.block(Block::List {
                ordered: tag == "ol",
                s: own,
                items,
            });
        }
        "table" => {
            let mut rows = Vec::new();
            collect_rows(
                ctx,
                o.get("content"),
                &inner_inherited,
                depth + 1,
                &mut rows,
            );
            let columns = place_cells(&mut rows);
            out.block(Block::Table {
                s: own,
                columns,
                rows,
            });
        }
        "details" => {
            let mut summary = Vec::new();
            let mut body = Out::default();
            let content = match o.get("content") {
                Some(Value::Array(items)) => items.clone(),
                Some(single) => vec![single.clone()],
                None => Vec::new(),
            };
            for c in &content {
                match c {
                    Value::Object(s) if s.get("tag").and_then(Value::as_str) == Some("summary") => {
                        summary.extend(runs_of(children(ctx, s, &inner_inherited, depth + 1)));
                    }
                    other => content_into(ctx, other, &inner_inherited, &mut body, depth + 1),
                }
            }
            out.block(Block::Details {
                open: o.get("open").and_then(Value::as_bool).unwrap_or(false),
                summary,
                blocks: body.finish(),
            });
        }
        // span and unknown inline tags: the text inherits the style
        _ => {
            if let Some(c) = o.get("content") {
                content_into(ctx, c, &text_style, out, depth + 1);
            }
        }
    }
}

fn collect_rows(
    ctx: &mut Ctx<'_>,
    content: Option<&Value>,
    inherited: &Style,
    depth: usize,
    rows: &mut Vec<Vec<Cell>>,
) {
    let items = match content {
        Some(Value::Array(items)) => items.clone(),
        Some(single) => vec![single.clone()],
        None => return,
    };
    for item in &items {
        let Value::Object(o) = item else { continue };
        match o.get("tag").and_then(Value::as_str) {
            Some(t @ ("thead" | "tbody" | "tfoot")) if depth < MAX_DEPTH => {
                ctx.inside(t, o, |ctx| {
                    collect_rows(ctx, o.get("content"), inherited, depth + 1, rows)
                });
            }
            Some("tr") => {
                let cells = match o.get("content") {
                    Some(Value::Array(c)) => c.clone(),
                    Some(single) => vec![single.clone()],
                    None => Vec::new(),
                };
                let row: Vec<Cell> = ctx.inside("tr", o, |ctx| {
                    cells
                        .iter()
                        .filter_map(|c| {
                            let Value::Object(cell) = c else { return None };
                            let tag = cell.get("tag").and_then(Value::as_str)?;
                            if tag != "td" && tag != "th" {
                                return None;
                            }
                            let s = own_style(ctx, tag, cell);
                            let mut text = inherited.clone();
                            text.merge(&s);
                            let blocks = ctx.inside(tag, cell, |ctx| {
                                children(ctx, cell, &text.inherited(), depth + 1)
                            });
                            let span = |k: &str| {
                                cell.get(k)
                                    .and_then(Value::as_u64)
                                    .map_or(1, |n| n.clamp(1, 64) as u32)
                            };
                            Some(Cell {
                                row: 0,
                                col: 0,
                                header: tag == "th",
                                colspan: span("colSpan"),
                                rowspan: span("rowSpan"),
                                natural_em: natural_em(&blocks),
                                blocks,
                                s,
                            })
                        })
                        .collect()
                });
                rows.push(row);
            }
            _ => {}
        }
    }
}

/// Places the cells in a grid, as HTML tables do: each cell takes the first
/// free column of its row, and row spans keep their columns taken below.
/// Returns the number of columns.
fn place_cells(rows: &mut [Vec<Cell>]) -> u32 {
    let mut taken: Vec<Vec<bool>> = vec![Vec::new(); rows.len()];
    let mut columns = 0;
    for (r, row) in rows.iter_mut().enumerate() {
        let mut col = 0usize;
        for cell in row.iter_mut() {
            while taken[r].get(col).copied().unwrap_or(false) {
                col += 1;
            }
            cell.row = r as u32;
            cell.col = col as u32;
            let span_rows = (cell.rowspan as usize).clamp(1, taken.len() - r);
            cell.rowspan = span_rows as u32;
            let end = col + cell.colspan as usize;
            for line in taken.iter_mut().skip(r).take(span_rows) {
                if line.len() < end {
                    line.resize(end, false);
                }
                for slot in &mut line[col..end] {
                    *slot = true;
                }
            }
            col = end;
            columns = columns.max(col as u32);
        }
    }
    columns
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(
                    std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"),
                    16,
                ) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blocks(glossary: Value) -> Vec<Block> {
        render(&glossary, &Sheet::default())
    }

    fn texts(blocks: &[Block]) -> String {
        serde_json::to_string(blocks).unwrap()
    }

    // T-POP-009
    #[test]
    fn strings_lists_and_breaks() {
        assert_eq!(
            blocks(json!(["to eat"])),
            [Block::Flow {
                runs: vec![Run::Text {
                    t: "to eat".into(),
                    s: Style::default()
                }]
            }]
        );
        let list = blocks(json!(["to eat", "to live on"]));
        assert!(
            matches!(&list[..], [Block::List { ordered: false, items, .. }] if items.len() == 2)
        );
        let lines = blocks(json!(["one\ntwo"]));
        let Block::Flow { runs } = &lines[0] else {
            panic!()
        };
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[1], Run::Br);
    }

    // T-POP-009
    #[test]
    fn ruby_table_list_details_and_links() {
        let g = json!([{"type": "structured-content", "content": [
            {"tag": "ruby", "content": ["漢", {"tag": "rp", "content": "("}, {"tag": "rt", "content": "かん"}, {"tag": "rp", "content": ")"}]},
            {"tag": "table", "content": [{"tag": "tbody", "content": [
                {"tag": "tr", "content": [{"tag": "th", "content": "a"}, {"tag": "td", "colSpan": 2, "content": "b"}]}
            ]}]},
            {"tag": "ol", "content": [{"tag": "li", "content": "one"}, {"tag": "li", "content": "two"}]},
            {"tag": "details", "content": [{"tag": "summary", "content": "more"}, "hidden"]},
            {"tag": "a", "href": "?query=%E9%A3%9F%E3%81%B9%E3%82%8B&wildcards=off", "content": "x"},
            {"tag": "a", "href": "?other=1", "content": [{"tag": "ruby", "content": ["本", {"tag": "rt", "content": "ほん"}]}]},
            {"tag": "a", "href": "https://example.org/x", "content": "site"}
        ]}]);
        let b = blocks(g);
        let Block::Flow { runs } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(
            runs[0],
            Run::Ruby {
                t: "漢".into(),
                rt: "かん".into(),
                s: Style::default()
            }
        );
        let Block::Table { rows, columns, .. } = &b[1] else {
            panic!()
        };
        assert!(rows[0][0].header && rows[0][1].colspan == 2);
        assert_eq!((*columns, rows[0][1].col), (3, 1));
        assert!(matches!(&b[2], Block::List { ordered: true, items, .. } if items.len() == 2));
        assert!(
            matches!(&b[3], Block::Details { open: false, summary, blocks } if summary.len() == 1 && blocks.len() == 1)
        );
        let Block::Flow { runs } = &b[4] else {
            panic!()
        };
        let targets: Vec<&Target> = runs
            .iter()
            .filter_map(|r| match r {
                Run::Link { target, .. } => Some(target),
                _ => None,
            })
            .collect();
        assert_eq!(
            targets,
            [
                &Target::Lookup("食べる".into()),
                &Target::Lookup("本".into()),
                &Target::External("https://example.org/x".into())
            ],
            "without ?query= the link text without <rt> is looked up (BUG-2456)"
        );
    }

    // T-POP-009, T-SEC-001
    #[test]
    fn hostile_content_is_dropped() {
        let g = json!([{"type": "structured-content", "content": [
            {"tag": "script", "content": "alert(1)"},
            {"tag": "iframe", "content": "frame"},
            {"tag": "style", "content": "* { color: red }"},
            {"tag": "span", "onclick": "alert(1)", "style": {"color": "red", "backgroundImage": "url(http://evil/x)", "position": "fixed"}, "content": "safe"},
            {"tag": "img", "path": "http://evil/x.png"},
            {"tag": "img", "path": "../../etc/passwd"},
            {"tag": "img", "path": "img/a.png", "width": 2, "height": 2}
        ]}, "<b>bold</b><script>alert(1)</script><img src=x onerror=alert(1)>"]);
        let json = texts(&blocks(g));
        for bad in [
            "alert",
            "frame",
            "evil",
            "passwd",
            "position",
            "onclick",
            "color: red",
        ] {
            assert!(!json.contains(bad), "{bad} in {json}");
        }
        assert!(json.contains("safe") && json.contains("img/a.png") && json.contains("bold"));
        assert!(json.contains(r##""color":"red""##));
    }

    #[test]
    fn styles_come_from_the_sheet_and_inline() {
        let sheet = Sheet::parse(
            r#"span[data-sc-class="tag"] { font-weight: bold; background-color: #565656 } div { margin-left: 1em }"#,
        );
        let g = json!([{"type": "structured-content", "content": {"tag": "div", "style": {"marginLeft": 2}, "content": [
            {"tag": "span", "data": {"class": "tag"}, "style": {"color": "white"}, "content": "n"}, " noun"
        ]}}]);
        let b = render(&g, &sheet);
        let Block::Box { s, blocks } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(
            s.margin.left,
            Some(crate::style::Length::Em(2.0)),
            "inline style wins"
        );
        let Block::Flow { runs } = &blocks[0] else {
            panic!()
        };
        let Run::Text { s: chip, .. } = &runs[0] else {
            panic!()
        };
        assert_eq!(
            (chip.bold, chip.background.as_deref(), chip.color.as_deref()),
            (Some(true), Some("#565656"), Some("white"))
        );
        let Run::Text { s: plain, .. } = &runs[1] else {
            panic!()
        };
        assert!(
            plain.bold.is_none() && plain.background.is_none(),
            "a span's style stays in the span"
        );
    }

    #[test]
    fn form_of_pairs_and_inline_html() {
        assert_eq!(
            blocks(json!([["时", ["Hyōgai"]], ["时", ["alt-of"]]])),
            [Block::Pairs {
                items: vec![
                    ("时".into(), vec!["Hyōgai".into()]),
                    ("时".into(), vec!["alt-of".into()])
                ]
            }]
        );
        let b = blocks(json!(["<ruby>日<rt>ひ</rt></ruby> &amp; <b>bold</b>"]));
        let Block::Flow { runs } = &b[0] else {
            panic!()
        };
        assert_eq!(
            runs[0],
            Run::Ruby {
                t: "日".into(),
                rt: "ひ".into(),
                s: Style::default()
            }
        );
        assert!(matches!(&runs[2], Run::Text { t, s } if t == "bold" && s.bold == Some(true)));
        assert!(matches!(&runs[1], Run::Text { t, .. } if t == " & "));
    }

    #[test]
    fn table_cells_are_placed_with_spans() {
        let cell = |rowspan: u64, colspan: u64| json!({"tag": "td", "rowSpan": rowspan, "colSpan": colspan, "content": "c"});
        let g = json!([{"type": "structured-content", "content": {"tag": "table", "content": [
            {"tag": "tr", "content": [cell(2, 1), cell(1, 2)]},
            {"tag": "tr", "content": [cell(1, 1), cell(1, 1)]}
        ]}}]);
        let b = blocks(g);
        let Block::Table { rows, columns, .. } = &b[0] else {
            panic!()
        };
        assert_eq!(*columns, 3);
        let places: Vec<(u32, u32)> = rows.iter().flatten().map(|c| (c.row, c.col)).collect();
        assert_eq!(
            places,
            [(0, 0), (0, 1), (1, 1), (1, 2)],
            "the row span keeps column 0 taken"
        );
    }

    #[test]
    fn deep_nesting_is_flattened() {
        let mut v = json!("deep");
        for _ in 0..500 {
            v = json!({"tag": "div", "content": v});
        }
        let json = texts(&render(
            &json!([{"type": "structured-content", "content": v}]),
            &Sheet::default(),
        ));
        assert!(json.contains("deep"));
    }

    #[test]
    fn media_paths_stay_inside() {
        assert_eq!(media_path("img/a.png").as_deref(), Some("img/a.png"));
        assert_eq!(media_path("\\img\\a.png").as_deref(), Some("img/a.png"));
        for bad in [
            "../a.png",
            "img/../../a",
            "http://x/a.png",
            "C:/a.png",
            "",
            "a//b",
        ] {
            assert_eq!(media_path(bad), None, "{bad}");
        }
    }

    proptest::proptest! {
        // T-FUZZ-006
        #[test]
        fn any_json_renders_without_panic(seed in proptest::collection::vec(0u8..12, 0..200)) {
            // a random tree of allowed and forbidden tags, strings and styles
            let tags = ["div", "span", "ruby", "rt", "table", "tr", "td", "ol", "li", "a", "script", "img"];
            let mut stack: Vec<Value> = vec![json!("x")];
            for b in seed {
                let child = stack.pop().unwrap_or(json!("y"));
                let node = json!({"tag": tags[usize::from(b)], "content": [child, "t"], "href": "?query=q", "path": "img/a.png", "style": {"fontSize": "200%", "color": "#000"}});
                stack.push(node);
                if b % 3 == 0 { stack.push(json!("z")); }
            }
            let out = render(&json!([{"type": "structured-content", "content": stack}]), &Sheet::default());
            let json = serde_json::to_string(&out).unwrap();
            proptest::prop_assert!(!json.contains("script"));
        }
    }
}
