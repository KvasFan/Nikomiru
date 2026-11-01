// SPDX-License-Identifier: GPL-3.0-only

//! XHTML into paragraphs (BOK-002): a scan of its tags, not a browser. Scripts
//! and styles are skipped, and nothing but text and furigana comes out.

use crate::{Chapter, Paragraph, Ruby};

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Text from a file: entities read, line ends gone.
pub(crate) fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match rest
            .find(';')
            .filter(|e| *e <= 10)
            .and_then(|e| Some((entity(&rest[1..e])?, e)))
        {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// What is being read inside a `<ruby>`.
#[derive(Default)]
struct OpenRuby {
    /// Where the base starts in the paragraph.
    start: usize,
    /// In `<rt>`: its text so far.
    reading: Option<String>,
    /// In `<rp>`: dropped.
    fallback: bool,
    /// Where the base of the current `<rt>` ended.
    end: usize,
}

#[derive(Default)]
struct Builder {
    chapter: Chapter,
    paragraph: Paragraph,
    ruby: Option<OpenRuby>,
    heading: bool,
}

impl Builder {
    fn text(&mut self, raw: &str) {
        let text = decode(raw);
        let text: String = text
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r' | '\t'))
            .collect();
        if let Some(ruby) = &mut self.ruby {
            if ruby.fallback {
                return;
            }
            if let Some(reading) = &mut ruby.reading {
                reading.push_str(text.trim());
                return;
            }
        }
        let text = if self.paragraph.text.is_empty() {
            text.trim_start_matches(' ')
        } else {
            &text
        };
        self.paragraph.text.push_str(text);
    }

    /// A picture: a paragraph of its own.
    fn image(&mut self, source: String) {
        self.end_paragraph();
        self.chapter.paragraphs.push(Paragraph {
            image: Some(source),
            ..Paragraph::default()
        });
    }

    fn end_paragraph(&mut self) {
        let mut paragraph = std::mem::take(&mut self.paragraph);
        self.ruby = None;
        paragraph.text.truncate(paragraph.text.trim_end().len());
        if paragraph.text.trim().is_empty() {
            return;
        }
        paragraph.heading = self.heading;
        if paragraph.heading && self.chapter.title.is_none() {
            self.chapter.title = Some(paragraph.text.clone());
        }
        self.chapter.paragraphs.push(paragraph);
    }

    fn open_reading(&mut self) {
        let end = self.paragraph.text.len();
        if let Some(ruby) = &mut self.ruby {
            ruby.end = end;
            ruby.reading = Some(String::new());
        }
    }

    fn close_reading(&mut self) {
        let Some(ruby) = &mut self.ruby else {
            return;
        };
        if let Some(text) = ruby.reading.take()
            && !text.is_empty()
            && ruby.start < ruby.end
        {
            self.paragraph.ruby.push(Ruby {
                range: ruby.start..ruby.end,
                text,
            });
        }
        // the next base of the same `<ruby>` starts here
        ruby.start = ruby.end;
    }
}

const BLOCKS: [&str; 15] = [
    "p",
    "div",
    "li",
    "tr",
    "blockquote",
    "section",
    "article",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "body",
    "br",
];

/// The picture a tag names.
fn source_of(tag: &str) -> Option<String> {
    ["src", "href", "xlink:href"].iter().find_map(|name| {
        let at = tag.find(&format!("{name}="))?;
        // not the end of a longer name
        if tag[..at].ends_with(|c: char| c.is_alphanumeric() || c == ':') {
            return None;
        }
        let value = &tag[at + name.len() + 1..];
        let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let end = value[1..].find(quote)?;
        Some(decode(&value[1..=end]))
    })
}

/// The chapter a file of the spine holds.
pub(crate) fn chapter_of(xhtml: &str) -> Chapter {
    let mut b = Builder::default();
    let mut rest = xhtml;
    while let Some(i) = rest.find('<') {
        if i > 0 {
            b.text(&rest[..i]);
        }
        rest = &rest[i..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |e| &after[e + 3..]);
            continue;
        }
        let Some(end) = rest.find('>') else {
            return finish(b);
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let name = name.rsplit(':').next().unwrap_or_default();
        match name {
            "img" | "image" if !closing => {
                if let Some(source) = source_of(tag) {
                    b.image(source);
                }
            }
            // what is not reading text, with its contents; a picture in an
            // SVG, as covers have, is a picture still
            "head" | "script" | "style" | "svg" if !closing && !tag.ends_with('/') => {
                let close = format!("</{name}");
                let lower = rest.to_ascii_lowercase();
                let end = lower.find(&close).unwrap_or(rest.len());
                if name == "svg"
                    && let Some(source) = rest[..end]
                        .split('<')
                        .find(|t| t.starts_with("image"))
                        .and_then(source_of)
                {
                    b.image(source);
                }
                rest = &rest[end..];
            }
            "ruby" if !closing => {
                b.ruby = Some(OpenRuby {
                    start: b.paragraph.text.len(),
                    ..OpenRuby::default()
                });
            }
            "ruby" => {
                b.close_reading();
                b.ruby = None;
            }
            "rt" if !closing => b.open_reading(),
            "rt" => b.close_reading(),
            "rp" => {
                if let Some(ruby) = &mut b.ruby {
                    ruby.fallback = !closing;
                }
            }
            _ if BLOCKS.contains(&name) => {
                b.end_paragraph();
                let heading = matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
                if heading {
                    b.heading = !closing;
                }
            }
            _ => {}
        }
    }
    finish(b)
}

fn finish(mut b: Builder) -> Chapter {
    b.end_paragraph();
    b.chapter
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraphs(xhtml: &str) -> Vec<Paragraph> {
        chapter_of(xhtml).paragraphs
    }

    // T-BOK-002
    #[test]
    fn blocks_are_paragraphs_and_line_ends_are_nothing() {
        let c = chapter_of(
            "<html><head><title>x</title><style>p{}</style></head><body>\n\
             <h1>第一章</h1>\n<p>吾輩は\n猫である。</p><p>名前は<em>まだ</em>無い。<br/>次の行</p>\n</body></html>",
        );
        assert_eq!(c.title.as_deref(), Some("第一章"));
        let texts: Vec<_> = c.paragraphs.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(
            texts,
            ["第一章", "吾輩は猫である。", "名前はまだ無い。", "次の行"]
        );
        assert!(c.paragraphs[0].heading && !c.paragraphs[1].heading);
    }

    // T-BOK-002: furigana stay out of the text the lookup reads
    #[test]
    fn ruby_goes_apart() {
        let p = &paragraphs(
            "<p>今日は<ruby>吾輩<rp>(</rp><rt>わがはい</rt><rp>)</rp></ruby>の\
             <ruby>猫<rt>ねこ</rt>犬<rt>いぬ</rt></ruby>です</p>",
        )[0];
        assert_eq!(p.text, "今日は吾輩の猫犬です");
        let ruby: Vec<_> = p
            .ruby
            .iter()
            .map(|r| (&p.text[r.range.clone()], r.text.as_str()))
            .collect();
        assert_eq!(ruby, [("吾輩", "わがはい"), ("猫", "ねこ"), ("犬", "いぬ")]);
    }

    // T-BOK-002, NFR-SEC-01: nothing active survives
    #[test]
    fn scripts_are_not_text() {
        let texts = paragraphs(
            "<p>本文<script>alert(1)</script><!-- 注 --></p><svg><text>絵</text></svg><p><img src='x' onerror='y'/>&amp;&#x3042;</p>",
        );
        assert_eq!(texts[1].image.as_deref(), Some("x"), "the picture stays");
        let texts: Vec<_> = texts
            .iter()
            .filter(|p| p.image.is_none())
            .map(|p| p.text.as_str())
            .collect();
        assert_eq!(texts, ["本文", "&あ"]);
    }

    // T-BOK-010
    #[test]
    fn pictures_are_paragraphs_of_their_own() {
        let p = paragraphs(
            "<p>前<img src='a.jpg' alt='x'/>後</p><svg><image xlink:href=\"b.png\"/></svg><img alt='none'/>",
        );
        let out: Vec<_> = p
            .iter()
            .map(|p| (p.text.as_str(), p.image.as_deref()))
            .collect();
        assert_eq!(
            out,
            [
                ("前", None),
                ("", Some("a.jpg")),
                ("後", None),
                ("", Some("b.png"))
            ]
        );
    }

    #[test]
    fn broken_markup_is_only_text() {
        for broken in [
            "",
            "<",
            "<p",
            "<!-- ",
            "<ruby><rt>",
            "<script>",
            "</ruby></rt>",
            "&;&#xffffffff;",
        ] {
            let _ = chapter_of(broken);
        }
    }

    #[test]
    fn namespaces_and_case_do_not_matter() {
        assert_eq!(
            paragraphs("<xhtml:P>一</xhtml:P><P>二</P>")
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>(),
            ["一", "二"]
        );
    }
}
