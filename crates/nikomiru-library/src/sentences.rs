// SPDX-License-Identifier: GPL-3.0-only

//! Local sentence snapshots indexed by exact dictionary identities (SEN-001…SEN-003).
use crate::{Library, LibraryError, db::db_err, phrases::Source};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
const BANK: TableDefinition<&str, &[u8]> = TableDefinition::new("sentence_bank");
/// Per-batch/import byte limit.
pub const BATCH_BYTES: usize = 8 << 20;
/// Total serialized bank budget.
pub const TOTAL_BYTES: usize = 32 << 20;
/// Global sentence count limit.
pub const MAX_SENTENCES: usize = 10000;
/// One exact dictionary identity at its original UTF-8 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Dictionary expression, not inflected surface text.
    pub expression: String,
    /// Exact reading, retaining homograph distinctions.
    pub reading: String,
    /// UTF-8 byte start in the saved sentence.
    pub start: u32,
    /// Exclusive UTF-8 byte end.
    pub end: u32,
}
/// A saved sentence, with original locations and segmented identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentence {
    /// Original sentence as plain text.
    pub text: String,
    /// Study language at indexing time.
    pub language: String,
    /// Optional original library source; never a filename or external request.
    pub source: Option<Source>,
    /// Recognized original word ranges, in order.
    pub tokens: Vec<Token>,
}
fn text_ok(s: &str, n: usize, multiline: bool) -> bool {
    s.len() <= n
        && !s
            .chars()
            .any(|c| c.is_control() && (!multiline || !matches!(c, '\n' | '\r' | '\t')))
}
/// Validate only library identifiers; imported sources cannot request local paths or URLs.
pub fn source_ok(s: &Source) -> bool {
    !s.uid.is_empty()
        && text_ok(&s.uid, 1024, false)
        && text_ok(&s.title, 1024, false)
        && !s.uid.starts_with('/')
        && !s.uid.contains(['\\'])
        && !s.uid.contains("://")
        && !["file:", "javascript:", "data:", "vbscript:"]
            .iter()
            .any(|prefix| s.uid.to_ascii_lowercase().starts_with(prefix))
        && !s.uid.split('/').any(|s| matches!(s, "." | ".."))
        && !(s.uid.as_bytes().get(1) == Some(&b':') && s.uid.as_bytes()[0].is_ascii_alphabetic())
        && if s.book {
            s.start_ms == 0 && s.end_ms == 0
        } else {
            s.end_ms > s.start_ms && s.end_ms <= i32::MAX as u32
        }
}
impl Sentence {
    /// Bounded original text, language, provenance and sorted non-overlapping UTF-8 ranges.
    pub fn valid(&self) -> bool {
        !self.text.trim().is_empty()
            && text_ok(&self.text, 16384, true)
            && !self.language.is_empty()
            && self.language.len() <= 35
            && self
                .language
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && self.source.as_ref().is_none_or(|s| {
                source_ok(s) && s.offset.checked_add(self.text.len() as u32).is_some()
            })
            && self.tokens.len() <= 256
            && {
                let mut end = 0;
                self.tokens.iter().all(|t| {
                    let good = t.start >= end
                        && t.end > t.start
                        && self.text.get(t.start as usize..t.end as usize).is_some()
                        && !t.expression.is_empty()
                        && text_ok(&t.expression, 4096, false)
                        && text_ok(&t.reading, 4096, false);
                    end = t.end;
                    good
                })
            }
    }
}
/// One explicitly replaced/deleted source or owned pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    /// Stable content/library batch id (not a path).
    pub key: String,
    /// Source/pack provenance label.
    pub title: String,
    /// SHA-256 of the dictionary registry at indexing time.
    pub dictionary: String,
    /// Complete sentence snapshot, committed atomically.
    pub sentences: Vec<Sentence>,
}
impl Batch {
    /// Full atomic validation; no partial index is exposed.
    pub fn valid(&self) -> bool {
        !self.key.is_empty()
            && self.key.len() <= 128
            && self
                .key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/:_-".contains(&c))
            && text_ok(&self.title, 1024, false)
            && self.dictionary.len() == 64
            && self.dictionary.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.sentences.is_empty()
            && self.sentences.len() <= MAX_SENTENCES
            && self.sentences.iter().all(Sentence::valid)
    }
}
/// Metadata shown without exposing the entire bank in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Batch identity.
    pub key: String,
    /// Source/pack label.
    pub title: String,
    /// Number of sentences.
    pub count: usize,
}
/// One saved exact example, with all original matching ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Example {
    /// The whole source sentence snapshot.
    pub sentence: Sentence,
    /// Batch identity/provenance.
    pub batch: String,
    /// Human-readable provenance.
    pub title: String,
    /// Original matching UTF-8 ranges.
    pub ranges: Vec<(u32, u32)>,
}
fn batches(tx: &redb::ReadTransaction) -> Result<Vec<Batch>, LibraryError> {
    let t = match tx.open_table(BANK) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
        Err(e) => return Err(LibraryError::Db(e.into())),
    };
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    let mut count = 0usize;
    for entry in db_err!(t.iter())? {
        let (_, value) = db_err!(entry)?;
        bytes = bytes.saturating_add(value.value().len());
        if rows.len() >= 256 || bytes > TOTAL_BYTES || value.value().len() > BATCH_BYTES {
            return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
        }
        let batch: Batch = postcard::from_bytes(value.value())?;
        count += batch.sentences.len();
        if !batch.valid() || count > MAX_SENTENCES {
            return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
        }
        rows.push(batch);
    }
    Ok(rows)
}
impl Library {
    /// Atomic batch replacement, retaining unrelated source snapshots and rejecting whole-bank overflow.
    pub fn put_sentence_batch(&self, batch: &Batch) -> Result<bool, LibraryError> {
        if !batch.valid() {
            return Ok(false);
        }
        let data = postcard::to_stdvec(batch)?;
        if data.len() > BATCH_BYTES {
            return Ok(false);
        }
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(BANK))?;
            let mut count = batch.sentences.len();
            let mut bytes = data.len();
            let mut sources = 1;
            for row in db_err!(t.iter())? {
                let (key, v) = db_err!(row)?;
                if key.value() == batch.key {
                    continue;
                }
                let other: Batch = postcard::from_bytes(v.value())?;
                count += other.sentences.len();
                bytes = bytes.saturating_add(v.value().len());
                sources += 1;
            }
            if count > MAX_SENTENCES || bytes > TOTAL_BYTES || sources > 256 {
                return Ok(false);
            }
            db_err!(t.insert(batch.key.as_str(), data.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(true)
    }
    /// Read only source metadata for the browser.
    pub fn sentence_batches(&self) -> Result<Vec<Summary>, LibraryError> {
        self.read(|tx| {
            Ok(batches(tx)?
                .into_iter()
                .map(|b| Summary {
                    key: b.key,
                    title: b.title,
                    count: b.sentences.len(),
                })
                .collect())
        })
    }
    /// Exact expression/reading/language query. A repeated word returns one sentence with its original ranges.
    pub fn sentence_examples(
        &self,
        expression: &str,
        reading: &str,
        language: &str,
    ) -> Result<Vec<Example>, LibraryError> {
        if expression.is_empty()
            || !text_ok(expression, 4096, false)
            || !text_ok(reading, 4096, false)
            || language.len() > 35
        {
            return Ok(Vec::new());
        }
        self.read(|tx| {
            let mut out = Vec::new();
            let mut matched = 0;
            for batch in batches(tx)? {
                for sentence in batch.sentences {
                    if sentence.language != language {
                        continue;
                    }
                    let ranges = sentence
                        .tokens
                        .iter()
                        .filter(|t| t.expression == expression && t.reading == reading)
                        .map(|t| (t.start, t.end))
                        .take(1000 - matched)
                        .collect::<Vec<_>>();
                    if ranges.is_empty() {
                        continue;
                    }
                    matched += ranges.len();
                    out.push(Example {
                        sentence,
                        batch: batch.key.clone(),
                        title: batch.title.clone(),
                        ranges,
                    });
                    if out.len() >= 50 || matched >= 1000 {
                        return Ok(out);
                    }
                }
            }
            Ok(out)
        })
    }
    /// Explicitly remove one captured source/pack, leaving study states intact.
    pub fn delete_sentence_batch(&self, key: &str) -> Result<bool, LibraryError> {
        let tx = self.write()?;
        let changed = {
            let mut t = db_err!(tx.open_table(BANK))?;
            db_err!(t.remove(key))?.is_some()
        };
        db_err!(tx.commit())?;
        Ok(changed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn batch() -> Batch {
        Batch {
            key: "book/test".into(),
            title: "Owned book".into(),
            dictionary: "a".repeat(64),
            sentences: vec![Sentence {
                text: "食べました。食べる。".into(),
                language: "ja".into(),
                source: None,
                tokens: vec![
                    Token {
                        expression: "食べる".into(),
                        reading: "たべる".into(),
                        start: 0,
                        end: 15,
                    },
                    Token {
                        expression: "食べる".into(),
                        reading: "たべる".into(),
                        start: 18,
                        end: 27,
                    },
                ],
            }],
        }
    }
    // T-SEN-001, T-SEN-002: whole-bank quotas reject overflow atomically and queries stop at their result bound.
    #[test]
    fn whole_bank_limits_and_imported_source_identifiers_are_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("l.redb")).unwrap();
        let mut b = batch();
        b.sentences = vec![b.sentences[0].clone(); MAX_SENTENCES];
        assert!(lib.put_sentence_batch(&b).unwrap());
        let mut other = batch();
        other.key = "pack/extra".into();
        assert!(!lib.put_sentence_batch(&other).unwrap());
        assert_eq!(lib.sentence_batches().unwrap().len(), 1);
        assert_eq!(
            lib.sentence_examples("食べる", "たべる", "ja")
                .unwrap()
                .len(),
            50
        );
        let source = Source {
            book: true,
            uid: "book/a".into(),
            title: "Owned".into(),
            chapter: 0,
            paragraph: 0,
            offset: 0,
            start_ms: 0,
            end_ms: 0,
        };
        for uid in [
            "/etc/passwd",
            "../secret",
            "C:\\secret",
            "file:secret",
            "https://example.invalid/a",
            "book/../secret",
            "javascript:alert(1)",
            "data:text/html,secret",
        ] {
            let mut s = source.clone();
            s.uid = uid.into();
            assert!(!source_ok(&s));
        }
        assert!(source_ok(&source));
    }
    // T-SEN-001, T-SEN-002, T-SEN-003: exact identity/language and original ranges, atomic replacement and restart/delete isolation.
    #[test]
    fn sentence_bank_queries_exact_identities_and_retains_previous_valid_batches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.redb");
        let lib = Library::open(&path).unwrap();
        let b = batch();
        assert!(b.valid());
        assert!(lib.put_sentence_batch(&b).unwrap());
        let hits = lib.sentence_examples("食べる", "たべる", "ja").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ranges, vec![(0, 15), (18, 27)]);
        assert!(
            lib.sentence_examples("食べる", "たべる", "en")
                .unwrap()
                .is_empty()
        );
        assert!(
            lib.sentence_examples("食べる", "wrong", "ja")
                .unwrap()
                .is_empty()
        );
        assert!(
            lib.sentence_examples("食", "たべる", "ja")
                .unwrap()
                .is_empty()
        );
        let mut bad = b.clone();
        bad.sentences[0].tokens[0].start = 1;
        assert!(!lib.put_sentence_batch(&bad).unwrap());
        bad = b.clone();
        bad.sentences[0].text = "a".repeat(16385);
        assert!(!lib.put_sentence_batch(&bad).unwrap());
        drop(lib);
        let lib = Library::open(&path).unwrap();
        assert_eq!(
            lib.sentence_examples("食べる", "たべる", "ja").unwrap(),
            hits
        );
        assert!(lib.word_statuses().unwrap().is_empty());
        assert!(lib.delete_sentence_batch(&b.key).unwrap());
        assert!(lib.sentence_batches().unwrap().is_empty());
    }
}
