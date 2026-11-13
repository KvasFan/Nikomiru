// SPDX-License-Identifier: GPL-3.0-only

//! Web pages read as books (WEB-001…WEB-003, D-36): a page Nikomiru saved,
//! UTF-8, its address on its first line, read into one chapter. Only the
//! part of the page that holds the text is read: its `<main>`, else its
//! `<article>`, else its body, without menus, headers, footers, forms or
//! pictures, which would come from elsewhere.

use std::path::Path;

use crate::{Book, BookError, Chapter, markup};

/// The start of the first line of a page Nikomiru saved; the address and
/// ` -->` follow.
pub const MARK: &str = "<!-- nikomiru-page: ";

/// The largest page read.
const LIMIT: u64 = 16 << 20;

/// What is never reading text, with its contents.
const LEFT_OUT: [&str; 11] = [
    "nav", "header", "footer", "aside", "form", "noscript", "button", "select", "template",
    "iframe", "figure",
];

/// The text a page Nikomiru saves: the mark with `address`, then `html`.
pub fn saved(address: &str, html: &str) -> String {
    // an address cannot end the comment
    let address = address.replace("--", "%2D%2D").replace('\n', "");
    format!("{MARK}{address} -->\n{html}")
}

/// The address a saved page names, if it is one.
pub fn address_of(text: &str) -> Option<&str> {
    let line = text.lines().next()?;
    line.strip_prefix(MARK)?.strip_suffix(" -->")
}

/// Reads a page Nikomiru saved, or an HTML file in UTF-8.
pub(crate) fn read(path: &Path) -> Result<Book, BookError> {
    read_inner(path, LIMIT, None)
}

/// Analysis enforces its smaller bound on the bytes actually read, even if
/// the file grows after the metadata check.
pub(crate) fn read_analysis(
    path: &Path,
    cancel: &nikomiru_core::Cancel,
) -> Result<Book, BookError> {
    read_inner(path, crate::analysis::MAX_ENTRY_BYTES, Some(cancel))
}

fn read_inner(
    path: &Path,
    limit: u64,
    cancel: Option<&nikomiru_core::Cancel>,
) -> Result<Book, BookError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| BookError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    let mut file = file.take(limit + 1);
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
    if bytes.len() as u64 > limit {
        return Err(BookError::TooLarge);
    }
    let text = String::from_utf8(bytes).map_err(|_| BookError::NotPage)?;
    page(&text)
}

/// The position of the first `<name` that opens an element of that name.
fn find_open(lower: &str, name: &str, from: usize) -> Option<usize> {
    let needle = format!("<{name}");
    let mut at = from;
    while let Some(i) = lower.get(at..)?.find(&needle) {
        let start = at + i;
        let next = lower.as_bytes().get(start + needle.len()).copied();
        if matches!(next, Some(b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')) {
            return Some(start);
        }
        at = start + needle.len();
    }
    None
}

/// Where the element opened at `start` ends: after its matching end tag,
/// counting the elements of the same name inside it; the end of the text if
/// it never closes.
fn element_end(lower: &str, name: &str, start: usize) -> usize {
    let close = format!("</{name}");
    let mut depth = 0usize;
    let mut at = start;
    loop {
        let open = find_open(lower, name, at);
        let shut = lower.get(at..).and_then(|r| r.find(&close)).map(|i| at + i);
        match (open, shut) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                at = o + 1;
            }
            (_, Some(c)) => {
                depth = depth.saturating_sub(1);
                let end = lower[c..].find('>').map_or(lower.len(), |e| c + e + 1);
                if depth == 0 {
                    return end;
                }
                at = end;
            }
            (_, None) => return lower.len(),
        }
    }
}

/// The inside of the first element `name`, if there is one.
fn inside<'a>(html: &'a str, lower: &str, name: &str) -> Option<&'a str> {
    let start = find_open(lower, name, 0)?;
    let open_end = start + lower[start..].find('>')? + 1;
    let end = element_end(lower, name, start);
    let close = lower[..end]
        .rfind("</")
        .filter(|&c| c >= open_end)
        .unwrap_or(end);
    html.get(open_end..close)
}

/// `html` without the elements of [`LEFT_OUT`].
fn without_chrome(html: &str) -> String {
    let mut text = html.to_string();
    for name in LEFT_OUT {
        loop {
            let lower = text.to_ascii_lowercase();
            let Some(start) = find_open(&lower, name, 0) else {
                break;
            };
            let end = element_end(&lower, name, start);
            text.replace_range(start..end, " ");
        }
    }
    text
}

/// The page's `<title>`.
fn title_of(html: &str, lower: &str) -> Option<String> {
    let title = inside(html, lower, "title")?;
    let title = markup::decode(title)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!title.is_empty()).then_some(title)
}

/// A page's text, as a book of one chapter.
pub fn page(text: &str) -> Result<Book, BookError> {
    let address = address_of(text);
    let lower = text.to_ascii_lowercase();
    let host = address
        .and_then(|a| a.split_once("://"))
        .map(|(_, rest)| {
            rest.split(['/', '?', '#'])
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    let title = title_of(text, &lower);
    let region = inside(text, &lower, "main")
        .or_else(|| inside(text, &lower, "article"))
        .or_else(|| inside(text, &lower, "body"))
        .unwrap_or(text);
    let mut chapter: Chapter = markup::chapter_of(&without_chrome(region));
    // pictures would be fetched from elsewhere: left out
    chapter.paragraphs.retain(|p| p.image.is_none());
    if chapter.paragraphs.iter().all(|p| p.text.trim().is_empty()) {
        return Err(BookError::NoText);
    }
    let title = title
        .or_else(|| chapter.title.clone())
        .unwrap_or_else(|| host.clone());
    Ok(Book {
        title,
        author: host,
        right_to_left: false,
        manga: false,
        chapters: vec![chapter],
        images: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-KNW-016: enforce the analysis budget during the read, independently
    // of metadata checked by the caller, and honour cancellation.
    #[test]
    fn analysis_read_enforces_its_own_byte_limit_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.html");
        std::fs::write(
            &path,
            format!(
                "<!--{}--><main><p>日本語</p></main>",
                "a".repeat(crate::analysis::MAX_ENTRY_BYTES as usize)
            ),
        )
        .unwrap();
        assert_eq!(
            read_analysis(&path, &nikomiru_core::Cancel::new()),
            Err(BookError::TooLarge)
        );
        assert_eq!(
            read(&path).unwrap().chapters[0].paragraphs[0].text,
            "日本語"
        );
        let cancel = nikomiru_core::Cancel::new();
        cancel.cancel();
        assert_eq!(
            read_analysis(&path, &cancel),
            Err(BookError::Io("cancelled".into()))
        );
    }

    fn texts(book: &Book) -> Vec<&str> {
        book.chapters[0]
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect()
    }

    // T-WEB-002: the text of the page, without what goes around it
    #[test]
    fn the_main_text_is_read_and_the_rest_left_out() {
        let html = saved(
            "https://www3.nhk.or.jp/news/easy/k1/k1.html",
            r#"<!DOCTYPE html><html><head><title>ニュース &amp; 天気</title>
            <script>var x = "<p>no</p>";</script></head><body>
            <header><nav><a href="/">ホーム</a></nav></header>
            <main><article><h1>雨が<ruby>降<rt>ふ</rt></ruby>ります</h1>
            <div><p>あしたは雨です。</p><figure><img src="/a.jpg"><figcaption>写真</figcaption></figure>
            <p>かさを<b>持って</b>ください。</p><img src="/b.jpg"></div></article>
            <aside><p>関連記事</p></aside></main>
            <footer><p>著作権</p></footer></body></html>"#,
        );
        let book = page(&html).unwrap();
        assert_eq!(book.title, "ニュース & 天気");
        assert_eq!(book.author, "www3.nhk.or.jp");
        assert_eq!(
            texts(&book),
            ["雨が降ります", "あしたは雨です。", "かさを持ってください。"]
        );
        let heading = &book.chapters[0].paragraphs[0];
        assert!(heading.heading);
        assert_eq!(heading.ruby[0].text, "ふ");
        assert!(book.images.is_empty());
    }

    // T-WEB-002: without main or article, the body; nested elements of the
    // same name end at their own end
    #[test]
    fn a_page_without_main_reads_its_body() {
        let html = r#"<html><body><div class="main_text">
            吾輩は<ruby><rb>猫</rb><rp>（</rp><rt>ねこ</rt><rp>）</rp></ruby>である。<br />
            名前はまだ無い。<nav><nav>x</nav>y</nav>続き</div></body></html>"#;
        let book = page(html).unwrap();
        assert_eq!(texts(&book), ["吾輩は猫である。", "名前はまだ無い。 続き"]);
        assert_eq!(book.author, "", "no address");
        assert!(matches!(
            page("<html><nav>menu</nav></html>"),
            Err(BookError::NoText)
        ));
    }

    // T-WEB-001
    #[test]
    fn the_address_comes_back_and_cannot_close_its_comment() {
        let text = saved("https://a.example/x--y\n", "<p>本</p>");
        assert_eq!(address_of(&text), Some("https://a.example/x%2D%2Dy"));
        assert_eq!(address_of("<p>本</p>"), None);
    }
}
