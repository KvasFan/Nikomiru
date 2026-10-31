// SPDX-License-Identifier: GPL-3.0-only

//! Dictionaries that are not in Yomitan's format (DIC-004): StarDict, DSL and
//! MDict, read into the same [`Term`]s, so lookups, popups and cards work on
//! them unchanged. Their definitions become plain text: their HTML is
//! untrusted and not drawn (NFR-SEC-01). Every size in a file is checked
//! against the file: nothing in one is believed.

mod dsl;
mod mdict;
mod ripemd;
mod stardict;

use std::io::Read;
use std::path::Path;

use nikomiru_core::Cancel;

use crate::import::{ImportError, MAX_ENTRIES, MAX_GLOSSARY, MAX_TOTAL};
use crate::yomitan::{FrequencyMode, Index, Term};

/// The largest block or text unpacked at once.
const UNPACKED_LIMIT: u64 = 1 << 30;
/// Entries read between two looks at the cancel flag.
const ENTRIES_PER_CHECK: usize = 4096;

/// A format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `.ifo`, with its `.idx` and `.dict` next to it.
    StarDict,
    /// `.dsl`, or `.dsl.dz`.
    Dsl,
    /// `.mdx`.
    Mdict,
}

/// The format of a file, by its name.
pub fn kind_of(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.ends_with(".ifo") {
        Some(Kind::StarDict)
    } else if name.ends_with(".dsl") || name.ends_with(".dsl.dz") {
        Some(Kind::Dsl)
    } else if name.ends_with(".mdx") {
        Some(Kind::Mdict)
    } else {
        None
    }
}

/// The error of a file that cannot be used.
pub(crate) fn bad(path: &Path, reason: impl Into<String>) -> ImportError {
    ImportError::Foreign {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        reason: reason.into(),
    }
}

/// The bytes of a file, within the limits of an import.
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, ImportError> {
    let io = |source: std::io::Error| ImportError::Io {
        path: path.to_path_buf(),
        source,
    };
    let size = std::fs::metadata(path).map_err(io)?.len();
    if size > MAX_TOTAL {
        return Err(bad(path, "too large"));
    }
    std::fs::read(path).map_err(io)
}

/// The bytes of a gzip file, however small it claims to be.
pub(crate) fn gunzip(path: &Path, bytes: &[u8]) -> Result<Vec<u8>, ImportError> {
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(bytes)
        .take(UNPACKED_LIMIT + 1)
        .read_to_end(&mut out)
        .map_err(|e| bad(path, e.to_string()))?;
    if out.len() as u64 > UNPACKED_LIMIT {
        return Err(bad(path, "too large once unpacked"));
    }
    Ok(out)
}

/// An index for a dictionary with only a title and something that changes
/// when its file does.
pub(crate) fn index(title: String, revision: String, description: Option<String>) -> Index {
    Index {
        title,
        revision,
        format: 3,
        sequenced: false,
        frequency_mode: FrequencyMode::default(),
        source_language: None,
        target_language: None,
        author: None,
        attribution: None,
        description,
        url: None,
    }
}

/// The name of a file without its extensions: the title when it has none.
pub(crate) fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.split('.').next().unwrap_or_default().to_string()
}

/// What changes when a file does.
pub(crate) fn revision_of(path: &Path) -> String {
    std::fs::metadata(path).map_or_else(|_| "1".to_string(), |m| m.len().to_string())
}

/// The dictionary's index, without reading its entries.
pub fn read_index(path: &Path, kind: Kind) -> Result<Index, ImportError> {
    match kind {
        Kind::StarDict => stardict::read_index(path),
        Kind::Dsl => dsl::read_index(path),
        Kind::Mdict => mdict::read_index(path),
    }
}

/// Collects the entries of a dictionary into [`Term`]s, within its limits.
#[derive(Debug)]
pub(crate) struct Terms<'a> {
    terms: Vec<Term>,
    cancel: &'a Cancel,
    added: usize,
}

impl<'a> Terms<'a> {
    pub(crate) fn new(cancel: &'a Cancel) -> Self {
        Self {
            terms: Vec::new(),
            cancel,
            added: 0,
        }
    }

    /// An entry: a headword as the dictionary writes it, and its definition.
    pub(crate) fn add(&mut self, head: &str, body: &str) -> Result<(), ImportError> {
        self.added += 1;
        if self.added.is_multiple_of(ENTRIES_PER_CHECK) && self.cancel.is_cancelled() {
            return Err(ImportError::Cancelled);
        }
        if self.terms.len() as u64 >= MAX_ENTRIES {
            return Err(ImportError::TooManyEntries);
        }
        if body.trim().is_empty() {
            return Ok(());
        }
        // a JSON list of one text, which the popup shows as it is
        let glossary = serde_json::to_string(&[body.trim()]).unwrap_or_default();
        for (expression, reading) in headwords(head) {
            if glossary.len() > MAX_GLOSSARY {
                return Err(ImportError::GlossaryTooLarge { expression });
            }
            self.terms.push(Term {
                expression,
                reading,
                definition_tags: String::new(),
                rules: String::new(),
                score: 0.0,
                glossary: glossary.clone(),
                sequence: 0,
                term_tags: String::new(),
            });
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Vec<Term> {
        self.terms
    }
}

/// The dictionary's entries.
pub fn read_terms(path: &Path, kind: Kind, cancel: &Cancel) -> Result<Vec<Term>, ImportError> {
    match kind {
        Kind::StarDict => stardict::read_terms(path, cancel),
        Kind::Dsl => dsl::read_terms(path, cancel),
        Kind::Mdict => mdict::read_terms(path, cancel),
    }
}

/// The headwords of a line as Japanese dictionaries write them: `かな【漢字・
/// 漢字】` gives each spelling with the reading; anything else is itself.
pub(crate) fn headwords(head: &str) -> Vec<(String, String)> {
    let head = head.trim();
    if head.is_empty() {
        return Vec::new();
    }
    let parts = head
        .split_once('【')
        .and_then(|(reading, rest)| Some((reading, rest.split_once('】')?.0)));
    let Some((reading, spellings)) = parts else {
        return vec![(head.to_string(), String::new())];
    };
    let reading: String = reading
        .chars()
        .filter(|c| !matches!(c, '・' | '‐' | '-' | '－' | '‧' | ' ' | '　'))
        .collect();
    let out: Vec<(String, String)> = spellings
        .split(['・', '／', '/'])
        .map(|s| s.trim_matches(['▽', '△', '×', '▼', '▲', ' ', '　']))
        .filter(|s| !s.is_empty())
        .map(|s| (s.to_string(), reading.clone()))
        .collect();
    if out.is_empty() {
        vec![(head.to_string(), String::new())]
    } else {
        out
    }
}

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

/// HTML as plain text: tags go, those that end a line end it, `script` and
/// `style` go with their contents, entities are read.
pub(crate) fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(['<', '&']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with('&') {
            let end = rest.find(';').filter(|e| *e <= 10);
            match end.and_then(|e| Some((entity(&rest[1..e])?, e))) {
                Some((c, e)) => {
                    out.push(c);
                    rest = &rest[e + 1..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
            continue;
        }
        let Some(end) = rest.find('>') else {
            rest = "";
            break;
        };
        let tag = rest[1..end].trim().to_ascii_lowercase();
        rest = &rest[end + 1..];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if matches!(name.as_str(), "script" | "style") && !tag.starts_with('/') {
            let close = format!("</{name}");
            let lower = rest.to_ascii_lowercase();
            let skip = lower.find(&close).unwrap_or(rest.len());
            rest = &rest[skip..];
            continue;
        }
        if matches!(
            name.as_str(),
            "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "hr"
        ) {
            out.push('\n');
        }
    }
    out.push_str(rest);
    tidy(&out)
}

/// Blanks shrunk, empty lines gone.
pub(crate) fn tidy(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-DIC-004
    #[test]
    fn headwords_are_read_as_dictionaries_write_them() {
        assert_eq!(headwords("食べる"), [("食べる".to_string(), String::new())]);
        assert_eq!(
            headwords("たべる【食べる】"),
            [("食べる".to_string(), "たべる".to_string())]
        );
        assert_eq!(
            headwords("た‐べる【食べる・▽喰べる】"),
            [
                ("食べる".to_string(), "たべる".to_string()),
                ("喰べる".to_string(), "たべる".to_string())
            ]
        );
        assert!(headwords("  ").is_empty());
        assert_eq!(headwords("あ【】"), [("あ【】".to_string(), String::new())]);
    }

    // T-DIC-004, NFR-SEC-01: nothing of the HTML survives but its text
    #[test]
    fn html_becomes_text() {
        assert_eq!(
            html_to_text("<b>意味</b><br>a &amp; b&#x3042;<div>次</div>"),
            "意味\na & bあ\n次"
        );
        assert_eq!(
            html_to_text("a<script>alert(1)</script><style>p{}</style>b<img src=x onerror=y>"),
            "ab"
        );
        assert_eq!(html_to_text("1 < 2 &unknown; <"), "1");
    }

    #[test]
    fn formats_go_by_the_file_name() {
        assert_eq!(kind_of(Path::new("a.IFO")), Some(Kind::StarDict));
        assert_eq!(kind_of(Path::new("a.dsl.dz")), Some(Kind::Dsl));
        assert_eq!(kind_of(Path::new("a.mdx")), Some(Kind::Mdict));
        assert_eq!(kind_of(Path::new("a.zip")), None);
    }
}
