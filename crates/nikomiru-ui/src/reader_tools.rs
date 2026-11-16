// SPDX-License-Identifier: GPL-3.0-only

//! Reader annotations and the library-wide text search (RDR-001…RDR-003): the
//! page's state, its serial library jobs, the shown TSV and the bounded
//! in-memory search index.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use nikomiru_core::Cancel;
use nikomiru_library::annotations::{Annotation, Kind};
use nikomiru_text::literal::{Needle, folded_copy, snippet};

/// Books searched at most.
pub const MAX_BOOKS: usize = 512;
/// The in-memory index's text budget.
pub const MAX_INDEX_BYTES: usize = 64 << 20;
/// Results at most.
pub const MAX_HITS: usize = 200;
/// Results at most from one book.
pub const MAX_BOOK_HITS: usize = 20;
/// Characters around a hit in its snippet.
const AROUND: usize = 40;

/// The page's tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// Annotations.
    #[default]
    Annotations,
    /// Library text search.
    Search,
}

/// A serial library operation on annotations.
#[derive(Debug, Clone)]
pub enum Job {
    /// Refresh.
    List,
    /// Save one draft.
    Save(Annotation),
    /// Explicit deletion by id.
    Delete(String),
}

/// The resulting snapshot, and whether a draft was saved; or a failure.
pub type Answer = Result<(Vec<Annotation>, bool), String>;

/// How much of the paragraph a new annotation covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The word looked up.
    Word,
    /// Its sentence.
    Sentence,
    /// The paragraph.
    Paragraph,
}

/// The page's retained state.
#[derive(Debug, Default)]
pub struct State {
    /// The page shows.
    pub open: bool,
    /// Its tab.
    pub tab: Tab,
    /// Committed annotations of every book.
    pub items: Vec<Annotation>,
    /// The editable draft, kept on failure.
    pub editor: Option<Annotation>,
    /// A job is pending.
    pub busy: bool,
    /// The last error.
    pub error: Option<String>,
    /// Show only the open book's annotations.
    pub this_book: bool,
    /// The library search.
    pub search: Search,
}

/// The library search's state.
#[derive(Debug, Default)]
pub struct Search {
    /// The query searched last.
    pub query: String,
    /// Incremented by every search and cancel: older replies are dropped.
    pub epoch: u64,
    /// A search runs.
    pub running: bool,
    /// Its cancellation.
    pub cancel: Option<Cancel>,
    /// The last complete result.
    pub outcome: Option<Outcome>,
    /// The last error.
    pub error: Option<String>,
}

/// Whether `a` matches a search over the list.
pub fn matches(a: &Annotation, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || [&a.quote, &a.name, &a.note, &a.title]
            .into_iter()
            .any(|s| s.to_lowercase().contains(&q))
}

/// The shown rows as TSV, formulas neutralised.
pub fn tsv(rows: &[Annotation]) -> Result<String, &'static str> {
    use crate::word_export::field;
    let mut out =
        String::from("kind\tname\tquote\tnote\tbook\tuid\tchapter\tparagraph\tstart\tend\n");
    if rows.len() > nikomiru_library::annotations::MAX_RECORDS {
        return Err("annotations-error");
    }
    for a in rows {
        if !a.valid() {
            return Err("annotations-error");
        }
        let line = [
            a.kind.name().to_string(),
            field(&a.name),
            field(&a.quote),
            field(&a.note),
            field(&a.title),
            field(&a.book),
            a.chapter.to_string(),
            a.paragraph.to_string(),
            a.start.to_string(),
            a.end.to_string(),
        ]
        .join("\t");
        if out.len().saturating_add(line.len() + 1) > 2 * nikomiru_library::annotations::MAX_BYTES {
            return Err("annotations-error");
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// The byte range `scope` covers around `range` in `text`.
pub fn scoped(text: &str, range: std::ops::Range<usize>, scope: Scope) -> std::ops::Range<usize> {
    match scope {
        Scope::Word if text.get(range.clone()).is_some_and(|w| !w.is_empty()) => range,
        Scope::Word | Scope::Sentence => {
            let sentence = nikomiru_text::sentence::sentence(text, range).range;
            if sentence.is_empty() {
                0..text.len()
            } else {
                sentence
            }
        }
        Scope::Paragraph => 0..text.len(),
    }
}

/// The first bytes of `text[range]`, at most `limit`, on a char boundary.
pub fn clip(text: &str, range: std::ops::Range<usize>, limit: usize) -> std::ops::Range<usize> {
    let mut end = range.end.min(range.start + limit).min(text.len());
    while end > range.start && !text.is_char_boundary(end) {
        end -= 1;
    }
    range.start..end
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The book's uid.
    pub uid: String,
    /// Its title.
    pub title: String,
    /// Original chapter.
    pub chapter: u32,
    /// Original paragraph.
    pub paragraph: u32,
    /// Original byte range.
    pub start: u32,
    /// End of it.
    pub end: u32,
    /// The original text matched.
    pub text: String,
    /// The text around it, and the hit's range in it.
    pub snippet: (String, std::ops::Range<usize>),
}

/// What a search found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Its epoch.
    pub epoch: u64,
    /// The results.
    pub hits: Vec<Hit>,
    /// Books searched.
    pub searched: usize,
    /// The titles of books that could not be read or have changed.
    pub failed: Vec<String>,
    /// More results were left out.
    pub truncated: bool,
}

/// A search to run.
#[derive(Debug, Clone)]
pub struct Request {
    /// Its epoch.
    pub epoch: u64,
    /// The query.
    pub query: String,
    /// The books: uid, file and title.
    pub books: Vec<(String, PathBuf, String)>,
}

#[derive(Debug)]
struct Paragraph {
    chapter: u32,
    paragraph: u32,
    text: String,
    folded: Option<String>,
}

#[derive(Debug)]
struct Indexed {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
    paragraphs: Vec<Paragraph>,
    bytes: usize,
}

/// The per-process index of book text, shared by the searches.
#[derive(Debug, Default)]
pub struct Index {
    books: HashMap<String, Indexed>,
    bytes: usize,
    /// Books read from their files so far (for tests).
    pub reads: usize,
}

impl Index {
    /// Bytes held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

fn stamp(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| (meta.len(), meta.modified().ok()))
}

/// Reads a book's text into an index entry, checking it is still that book.
fn index_book(
    uid: &str,
    path: &Path,
    support: &dyn nikomiru_books::documents::Support,
    cancel: &Cancel,
) -> Result<Option<Indexed>, ()> {
    let (size, modified) = stamp(path).ok_or(())?;
    let book = match nikomiru_books::analysis::read_text_with_support(path, cancel, support) {
        Ok(book) => book,
        Err(nikomiru_books::BookError::NoText) => return Ok(None),
        Err(_) => return Err(()),
    };
    let found = book.identity.clone().unwrap_or_else(|| {
        nikomiru_library::books::book_uid(&book.title, &book.author, book.chars())
    });
    if found != uid || book.manga {
        return Err(());
    }
    let mut paragraphs = Vec::new();
    let mut bytes = 0;
    for (c, chapter) in book.chapters.into_iter().enumerate() {
        for (p, paragraph) in chapter.paragraphs.into_iter().enumerate() {
            if paragraph.image.is_some() || paragraph.text.is_empty() {
                continue;
            }
            let folded = folded_copy(&paragraph.text);
            bytes += paragraph.text.len() + folded.as_ref().map_or(0, String::len);
            paragraphs.push(Paragraph {
                chapter: c as u32,
                paragraph: p as u32,
                text: paragraph.text,
                folded,
            });
        }
    }
    Ok(Some(Indexed {
        path: path.to_path_buf(),
        size,
        modified,
        paragraphs,
        bytes,
    }))
}

fn search_book(
    needle: &Needle,
    uid: &str,
    title: &str,
    book: &Indexed,
    hits: &mut Vec<Hit>,
) -> bool {
    let mut found = 0;
    for p in &book.paragraphs {
        if found >= MAX_BOOK_HITS || hits.len() >= MAX_HITS {
            return true;
        }
        let limit = (MAX_BOOK_HITS - found).min(MAX_HITS - hits.len());
        for range in needle.find(&p.text, p.folded.as_deref(), limit + 1) {
            if found >= MAX_BOOK_HITS || hits.len() >= MAX_HITS {
                return true;
            }
            found += 1;
            hits.push(Hit {
                uid: uid.into(),
                title: title.into(),
                chapter: p.chapter,
                paragraph: p.paragraph,
                start: range.start as u32,
                end: range.end as u32,
                text: p.text[range.clone()].into(),
                snippet: snippet(&p.text, range, AROUND),
            });
        }
    }
    false
}

/// Runs a search over the index, reading the books it lacks or that have
/// changed on disk; the caller drops the answer if a newer search started.
pub fn search(
    request: &Request,
    index: &Mutex<Index>,
    support: &dyn nikomiru_books::documents::Support,
    cancel: &Cancel,
) -> Result<Outcome, String> {
    let needle = Needle::new(&request.query).ok_or_else(String::new)?;
    let mut outcome = Outcome {
        epoch: request.epoch,
        ..Outcome::default()
    };
    for (uid, path, title) in request.books.iter().take(MAX_BOOKS) {
        if cancel.is_cancelled() {
            return Err(String::new());
        }
        if outcome.hits.len() >= MAX_HITS {
            outcome.truncated = true;
            break;
        }
        let current = stamp(path);
        let fresh = {
            let index = index.lock().map_err(|_| String::new())?;
            index
                .books
                .get(uid)
                .is_some_and(|b| b.path == *path && current.as_ref() == Some(&(b.size, b.modified)))
        };
        if !fresh {
            let read = index_book(uid, path, support, cancel);
            if cancel.is_cancelled() {
                return Err(String::new());
            }
            let mut index = index.lock().map_err(|_| String::new())?;
            index.reads += 1;
            if let Some(old) = index.books.remove(uid) {
                index.bytes -= old.bytes;
            }
            match read {
                Ok(Some(book)) => {
                    if index.bytes + book.bytes <= MAX_INDEX_BYTES {
                        index.bytes += book.bytes;
                        index.books.insert(uid.clone(), book);
                    } else {
                        // searched once, not kept
                        outcome.searched += 1;
                        outcome.truncated |=
                            search_book(&needle, uid, title, &book, &mut outcome.hits);
                        continue;
                    }
                }
                Ok(None) => continue,
                Err(()) => {
                    outcome.failed.push(title.clone());
                    continue;
                }
            }
        }
        let index = index.lock().map_err(|_| String::new())?;
        if let Some(book) = index.books.get(uid) {
            outcome.searched += 1;
            outcome.truncated |= search_book(&needle, uid, title, book, &mut outcome.hits);
        }
    }
    Ok(outcome)
}

/// A new annotation's kind from its stable name, for commands.
pub fn kind(name: &str) -> Option<Kind> {
    Kind::from_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_book(dir: &Path, name: &str, text: &str) -> (String, PathBuf, String) {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        let book = nikomiru_books::read(&path).unwrap();
        let uid = book.identity.clone().unwrap();
        (uid, path, book.title)
    }

    // T-RDR-003: cross-book offsets, limits, cache invalidation and changed books.
    #[test]
    fn the_library_search_reads_once_and_follows_changes() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_book(dir.path(), "A.txt", "猫が好き。\n\n犬と猫。\n");
        let b = write_book(dir.path(), "B.md", "# Title\n\nThe CAT sat.\n");
        let missing = (
            "book/none".to_string(),
            dir.path().join("gone.txt"),
            "Gone".to_string(),
        );
        let index = Mutex::new(Index::default());
        let support = nikomiru_books::documents::Builtin;
        let request = |query: &str, epoch| Request {
            epoch,
            query: query.into(),
            books: vec![a.clone(), b.clone(), missing.clone()],
        };
        let out = search(&request("猫", 1), &index, &support, &Cancel::new()).unwrap();
        assert_eq!(out.epoch, 1);
        assert_eq!(out.hits.len(), 2);
        assert_eq!(
            (out.hits[0].paragraph, out.hits[0].start, out.hits[0].end),
            (0, 0, 3)
        );
        assert_eq!((out.hits[1].paragraph, out.hits[1].start), (1, 6));
        assert_eq!(out.failed, vec!["Gone".to_string()]);
        assert_eq!(out.searched, 2);
        let reads = index.lock().unwrap().reads;
        let out = search(&request("cat", 2), &index, &support, &Cancel::new()).unwrap();
        assert_eq!(out.hits.len(), 1);
        assert_eq!(out.hits[0].text, "CAT");
        assert_eq!(out.hits[0].uid, b.0);
        assert_eq!(
            index.lock().unwrap().reads,
            reads + 1,
            "only the missing file is read again"
        );
        // the file changes: its identity no longer matches the library's
        std::fs::write(&a.1, "猫猫猫。\n").unwrap();
        let out = search(&request("猫", 3), &index, &support, &Cancel::new()).unwrap();
        assert!(out.failed.contains(&a.2), "{out:?}");
        assert!(out.hits.iter().all(|h| h.uid != a.0));
        let cancel = Cancel::new();
        cancel.cancel();
        assert!(search(&request("猫", 4), &index, &support, &cancel).is_err());
        assert!(search(&request("", 5), &index, &support, &Cancel::new()).is_err());
    }

    // T-RDR-003: result limits per book and overall.
    #[test]
    fn results_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let text = "猫。\n\n".repeat(50);
        let books: Vec<_> = (0..12)
            .map(|i| write_book(dir.path(), &format!("{i}.txt"), &format!("{i}\n\n{text}")))
            .collect();
        let index = Mutex::new(Index::default());
        let out = search(
            &Request {
                epoch: 1,
                query: "猫".into(),
                books,
            },
            &index,
            &nikomiru_books::documents::Builtin,
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(out.hits.len(), MAX_HITS);
        assert!(out.truncated);
        let first = &out.hits[0].uid;
        assert_eq!(
            out.hits.iter().filter(|h| &h.uid == first).count(),
            MAX_BOOK_HITS
        );
        assert!(index.lock().unwrap().bytes() > 0);
    }

    // T-RDR-001, T-RDR-002: scopes, formula-safe shown TSV and list search.
    #[test]
    fn scopes_exports_and_matches() {
        let text = "前の文。猫が好きです。後";
        let word = 12..15;
        assert_eq!(&text[scoped(text, word.clone(), Scope::Word)], "猫");
        assert_eq!(
            &text[scoped(text, word.clone(), Scope::Sentence)],
            "猫が好きです。"
        );
        assert_eq!(&text[scoped(text, 0..0, Scope::Word)], "前の文。");
        assert_eq!(scoped(text, word, Scope::Paragraph), 0..text.len());
        assert_eq!(&text[clip(text, 0..text.len(), 4)], "前");
        let a = Annotation {
            id: "00000000-0000-4000-8000-000000000001".into(),
            kind: Kind::Note,
            book: "book/x".into(),
            title: "=T".into(),
            chapter: 0,
            paragraph: 0,
            start: 0,
            end: 3,
            quote: "猫".into(),
            name: String::new(),
            note: "+note\tx".into(),
            created_ms: 1,
            changed_ms: 1,
        };
        let out = tsv(std::slice::from_ref(&a)).unwrap();
        assert!(
            out.contains("note\t\t猫\t'+note x\t'=T\tbook/x\t0\t0\t0\t3\n"),
            "{out}"
        );
        assert!(matches(&a, "NOTE"));
        assert!(!matches(&a, "犬"));
        let mut bad = a;
        bad.end = 9;
        assert!(tsv(&[bad]).is_err());
    }
}
