// SPDX-License-Identifier: GPL-3.0-only

//! Safe loose documents and durable pasted sources (BOK-011…BOK-014).
use crate::{Book, BookError, Chapter, Paragraph};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

/// Largest original TXT/Markdown/pasted text, before rendering.
pub const MAX_BYTES: usize = 4 << 20;
/// Largest encoded managed source (escaping can double valid input).
pub const MAX_DOCUMENT_BYTES: usize = MAX_BYTES * 2 + 4096;
/// Nonempty reading paragraphs per document.
pub const MAX_PARAGRAPHS: usize = 8192;
/// Largest clean paragraph, matching the reader's segmentation limit.
pub const MAX_PARAGRAPH_BYTES: usize = 64 << 10;

/// Explicit interpretation of a loose document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Every nonempty source line is a literal reading paragraph.
    Plain,
    /// CommonMark, represented as safe reading paragraphs.
    Markdown,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u8,
    title: String,
    format: Format,
    text: String,
}

fn valid_title(title: &str) -> bool {
    !title.trim().is_empty() && title.chars().count() <= 256 && !title.chars().any(char::is_control)
}
fn source(text: &str) -> Result<String, BookError> {
    if text.len() > MAX_BYTES {
        return Err(BookError::TooLarge);
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(BookError::InvalidText);
    }
    Ok(text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn escaped(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
}
// Only ruby structure may enter the markup reader. Attributes and every other tag are inert.
fn ruby_html(raw: &str, out: &mut String) {
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        escaped(&rest[..start], out);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[1..end];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        if matches!(name, "ruby" | "rt" | "rp") {
            out.push('<');
            if closing {
                out.push('/');
            }
            out.push_str(name);
            out.push('>');
        } else {
            escaped(&rest[..=end], out);
        }
        rest = &rest[end + 1..];
    }
    escaped(rest, out);
}
fn markdown(text: &str, cancel: Option<&nikomiru_core::Cancel>) -> Result<Chapter, BookError> {
    let mut html = String::new();
    for event in Parser::new(text) {
        if let Some(cancel) = cancel {
            crate::analysis::check_cancel(cancel)?;
        }
        match event {
            Event::Start(Tag::Heading { .. }) => html.push_str("<h1>"),
            Event::End(TagEnd::Heading(_)) => html.push_str("</h1>"),
            Event::Start(Tag::Paragraph | Tag::Item | Tag::CodeBlock(_)) => html.push_str("<p>"),
            Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::CodeBlock) => {
                html.push_str("</p>")
            }
            Event::Text(t) | Event::Code(t) => escaped(&t, &mut html),
            Event::Html(t) | Event::InlineHtml(t) => ruby_html(&t, &mut html),
            Event::SoftBreak => html.push(' '),
            Event::HardBreak | Event::Rule => html.push_str("<br>"),
            _ => {}
        }
        if html.len() > MAX_BYTES * 6 {
            return Err(BookError::TooLarge);
        }
    }
    Ok(crate::markup::chapter_of(&html))
}
fn parse_inner(
    title: &str,
    text: &str,
    format: Format,
    cancel: Option<&nikomiru_core::Cancel>,
) -> Result<Book, BookError> {
    if !valid_title(title) {
        return Err(BookError::InvalidText);
    }
    let text = source(text)?;
    let chapter = match format {
        Format::Plain => {
            let mut paragraphs = Vec::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                if let Some(cancel) = cancel {
                    crate::analysis::check_cancel(cancel)?;
                }
                if line.len() > MAX_PARAGRAPH_BYTES || paragraphs.len() >= MAX_PARAGRAPHS {
                    return Err(BookError::TooLarge);
                }
                paragraphs.push(Paragraph {
                    text: line.into(),
                    ..Default::default()
                });
            }
            Chapter {
                paragraphs,
                ..Default::default()
            }
        }
        Format::Markdown => markdown(&text, cancel)?,
    };
    if chapter.paragraphs.is_empty() {
        return Err(BookError::NoText);
    }
    if chapter.paragraphs.len() > MAX_PARAGRAPHS
        || chapter
            .paragraphs
            .iter()
            .any(|p| p.text.len() > MAX_PARAGRAPH_BYTES)
    {
        return Err(BookError::TooLarge);
    }
    let mut digest = Sha256::new();
    for part in [
        title.as_bytes(),
        if format == Format::Plain {
            b"plain"
        } else {
            b"markdown"
        },
        text.as_bytes(),
    ] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part);
    }
    Ok(Book {
        identity: Some(format!(
            "text:{}",
            digest
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )),
        title: title.into(),
        chapters: vec![chapter],
        ..Default::default()
    })
}
/// Parse validated UTF-8 into the existing book/lookup model without executing Markdown or HTML.
pub fn parse(title: &str, text: &str, format: Format) -> Result<Book, BookError> {
    parse_inner(title, text, format, None)
}

/// Read one bounded source, with cancellable reads/Markdown events during whole-book analysis.
pub fn read(path: &Path, cancel: Option<&nikomiru_core::Cancel>) -> Result<Book, BookError> {
    if !std::fs::metadata(path)
        .map_err(|e| BookError::Io(e.to_string()))?
        .is_file()
    {
        return Err(BookError::InvalidText);
    }
    let managed = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("nikomiru-text"));
    let limit = if managed {
        MAX_DOCUMENT_BYTES
    } else {
        MAX_BYTES
    };
    let limit = if cancel.is_some() {
        limit.min(crate::analysis::MAX_ENTRY_BYTES as usize)
    } else {
        limit
    };
    let mut file = std::fs::File::open(path)
        .map_err(|e| BookError::Io(e.to_string()))?
        .take(limit as u64 + 1);
    let mut bytes = Vec::new();
    let mut chunk = [0; 16 << 10];
    loop {
        if let Some(cancel) = cancel {
            crate::analysis::check_cancel(cancel)?;
        }
        let n = file
            .read(&mut chunk)
            .map_err(|e| BookError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    if bytes.len() > limit {
        return Err(BookError::TooLarge);
    }
    if managed {
        let document: Document =
            serde_json::from_slice(&bytes).map_err(|_| BookError::InvalidText)?;
        if document.version != 1 {
            return Err(BookError::InvalidText);
        }
        parse_inner(&document.title, &document.text, document.format, cancel)
    } else {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(BookError::InvalidText)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| BookError::InvalidText)?;
        let format = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
        {
            Format::Plain
        } else {
            Format::Markdown
        };
        parse_inner(title, text, format, cancel)
    }
}
/// Validate and atomically retain a pasted document. Repeated identical input reuses the same path.
pub fn save(
    dir: &Path,
    title: &str,
    text: &str,
    format: Format,
) -> Result<(PathBuf, Book), BookError> {
    let book = parse(title, text, format)?;
    let id = book
        .identity
        .as_deref()
        .ok_or(BookError::InvalidText)?
        .trim_start_matches("text:");
    let path = dir.join(format!("{id}.nikomiru-text"));
    let bytes = serde_json::to_vec(&Document {
        version: 1,
        title: title.into(),
        format,
        text: source(text)?,
    })
    .map_err(|_| BookError::InvalidText)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(BookError::TooLarge);
    }
    nikomiru_core::fs::write_atomic(&path, &bytes, 0o600)
        .map_err(|e| BookError::Io(e.to_string()))?;
    Ok((path, book))
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-BOK-011: plain content retains markup literally, normalises line endings and enforces input bounds.
    #[test]
    fn literal_unicode_and_limits() {
        let b = parse(
            "読書",
            "\u{feff}猫\r\n\r\n<ruby>字&amp;</ruby>\r犬",
            Format::Plain,
        )
        .unwrap();
        assert_eq!(
            b.chapters[0]
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            ["猫", "<ruby>字&amp;</ruby>", "犬"]
        );
        for text in [" ", "\0", "x\u{1b}"] {
            assert!(parse("Text", text, Format::Plain).is_err());
        }
        assert_eq!(
            parse("Text", &"x".repeat(MAX_BYTES + 1), Format::Plain),
            Err(BookError::TooLarge)
        );
        assert_eq!(
            parse("Text", &"x".repeat(MAX_PARAGRAPH_BYTES + 1), Format::Plain),
            Err(BookError::TooLarge)
        );
        assert_eq!(
            parse("Text", &"x\n".repeat(MAX_PARAGRAPHS + 1), Format::Plain),
            Err(BookError::TooLarge)
        );
    }
    // T-BOK-012: CommonMark syntax is read as text; HTML/code cannot execute, and ruby ranges retain Unicode bytes.
    #[test]
    fn markdown_keeps_headings_labels_code_and_safe_ruby() {
        let b=parse("Book","# 見出し\n\n**猫**と[犬](https://example.invalid)\n\n- 鳥\n- 魚\n\n`<b>&amp;</b>`\n\n<ruby onclick='bad()'>漢字<rt>かんじ</rt></ruby>\n\n<script>alert(1)</script>\n\n![絵](https://example.invalid/p.png)",Format::Markdown).unwrap();
        let ps = &b.chapters[0].paragraphs;
        assert!(ps[0].heading);
        assert_eq!(ps[0].text, "見出し");
        assert!(ps.iter().any(|p| p.text == "猫と犬"));
        assert!(ps.iter().any(|p| p.text == "<b>&amp;</b>"));
        let ruby = ps.iter().find(|p| !p.ruby.is_empty()).unwrap();
        assert_eq!(ruby.text, "漢字");
        assert_eq!(ruby.ruby[0].range, 0..6);
        assert_eq!(ruby.ruby[0].text, "かんじ");
        assert!(
            ps.iter()
                .any(|p| p.text.contains("<script>alert(1)</script>"))
        );
        assert!(b.images.is_empty());
        assert!(!ps.iter().any(|p| p.text.contains("https://")));
    }
    // T-BOK-013, T-BOK-014: retained sources round-trip, isolate equal-length text and leave no partial document after failure.
    #[test]
    fn managed_roundtrip_identity_and_failure() {
        let dir = tempfile::tempdir().unwrap();
        let (path, book) = save(dir.path(), "Text", "猫", Format::Plain).unwrap();
        assert_eq!(read(&path, None).unwrap(), book);
        assert_eq!(
            save(dir.path(), "Text", "猫", Format::Plain).unwrap().0,
            path
        );
        assert_ne!(
            book.identity,
            parse("Text", "犬", Format::Plain).unwrap().identity
        );
        let raw = dir.path().join("Text.txt");
        std::fs::write(&raw, "猫").unwrap();
        assert_eq!(read(&raw, None).unwrap().identity, book.identity);
        std::fs::create_dir(dir.path().join("moved")).unwrap();
        let moved = dir.path().join("moved/Text.txt");
        std::fs::rename(&raw, &moved).unwrap();
        assert_eq!(read(&moved, None).unwrap().identity, book.identity);
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, "file").unwrap();
        assert!(save(&blocked, "Text", "猫", Format::Plain).is_err());
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["version"] = 2.into();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(read(&path, None), Err(BookError::InvalidText));
    }
    // T-BOK-011: reading and analysis use identical paragraphs, with cancellation and strict UTF-8 rejection.
    #[test]
    fn file_analysis_parity_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("文.MD");
        std::fs::write(&path, "# 文\n\n猫と犬").unwrap();
        assert_eq!(
            crate::read(&path).unwrap(),
            crate::analysis::read_text(&path, &nikomiru_core::Cancel::new()).unwrap()
        );
        let cancel = nikomiru_core::Cancel::new();
        cancel.cancel();
        assert!(crate::analysis::read_text(&path, &cancel).is_err());
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert_eq!(crate::read(&path), Err(BookError::InvalidText));
    }
}
