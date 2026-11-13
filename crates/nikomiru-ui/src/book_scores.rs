// SPDX-License-Identifier: GPL-3.0-only

//! Whole-book comprehension requested from the Books list (KNW-016).
//! Four retained counts, one requested analysis at a time; statuses rescore
//! those counts without reopening files. Results never open the reader.

use crate::words::{Counted, Score, Words};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Completed book counts retained in memory.
pub const KEPT: usize = 4;

/// A whole-book analysis, stamped against replacement/removal/dictionary edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The library book.
    pub uid: String,
    /// Its local EPUB or saved HTML file.
    pub path: PathBuf,
    /// Dictionary generation.
    pub generation: u64,
    /// Monotonic analysis id, never reset by clearing the cache.
    pub serial: u64,
}

/// An analysed book, with no partial count for excluded sources.
#[derive(Debug)]
pub enum Outcome {
    /// All text counted, paragraphs taking the place of subtitle lines.
    Counted(Counted),
    /// No text source, including manga.
    NoText,
    /// One of the source/text/paragraph analysis limits was exceeded.
    TooLarge,
}

/// What the Books list displays for an analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Reading/counting off the UI thread.
    Counting,
    /// Whole-book score, using current word statuses.
    Score(Score),
    /// The source has no analysable text.
    NoText,
    /// The source exceeds the analysis limits.
    TooLarge,
    /// The translated source/dictionary failure, available on hover.
    Failed(String),
}

#[derive(Debug)]
struct Entry {
    cancel: nikomiru_core::Cancel,
    request: Request,
    status: Status,
    counted: Option<Arc<Counted>>,
    revision: Option<u64>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Ephemeral whole-book counts; separate from chapter reading state.
#[derive(Debug, Default)]
pub struct Scores {
    generation: Option<u64>,
    serial: u64,
    entries: HashMap<String, Entry>,
    pending: Vec<Request>,
}

impl Scores {
    fn prepare(&mut self, generation: u64) {
        if self.generation != Some(generation) {
            self.clear();
            self.generation = Some(generation);
        }
    }

    /// Discard counts and pending requests; keep ids monotonic for old replies.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending.clear();
    }

    /// Forget a removed/replaced book and its not-yet-dispatched work.
    pub fn forget(&mut self, uid: &str) {
        self.entries.remove(uid);
        self.pending.retain(|r| r.uid != uid);
    }

    /// Whether an analysis is already requested/running.
    pub fn busy(&self) -> bool {
        self.entries.values().any(|e| e.status == Status::Counting)
    }

    /// Request an initial analysis or explicit refresh. No automatic rescans.
    pub fn request(&mut self, uid: String, path: PathBuf, generation: u64) {
        self.prepare(generation);
        if self.busy() {
            return;
        }
        if self.entries.len() >= KEPT
            && !self.entries.contains_key(&uid)
            && let Some(old) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.request.serial)
                .map(|(uid, _)| uid.clone())
        {
            self.forget(&old);
        }
        self.serial += 1;
        let request = Request {
            uid: uid.clone(),
            path,
            generation,
            serial: self.serial,
        };
        self.pending.push(request.clone());
        self.entries.insert(
            uid,
            Entry {
                cancel: nikomiru_core::Cancel::new(),
                request,
                status: Status::Counting,
                counted: None,
                revision: None,
            },
        );
    }

    /// Keep only current library paths and dictionaries; rescore on status,
    /// Anki or card changes. Disabled colours discard the analysis state.
    pub fn update(
        &mut self,
        enabled: bool,
        records: &[(String, nikomiru_library::books::BookRecord)],
        words: &Words,
    ) {
        self.prepare(words.generation());
        if !enabled {
            self.clear();
            return;
        }
        self.entries.retain(|uid, e| {
            records
                .iter()
                .any(|(u, b)| u == uid && e.request.path == Path::new(&b.path))
        });
        self.pending
            .retain(|r| self.entries.get(&r.uid).is_some_and(|e| e.request == *r));
        for e in self.entries.values_mut() {
            if let Some(counted) = &e.counted
                && e.revision != Some(words.revision())
            {
                e.status =
                    Status::Score(counted.score(|word, reading| words.status(word, reading)));
                e.revision = Some(words.revision());
            }
        }
    }

    /// Pending jobs, taken by the UI layer.
    pub fn take(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.pending)
    }

    /// Accept only the current request's answer. No reading-position writes.
    pub fn done(&mut self, request: &Request, result: Result<Outcome, String>) {
        let Some(e) = self
            .entries
            .get_mut(&request.uid)
            .filter(|e| e.request == *request)
        else {
            return;
        };
        e.status = match result {
            Ok(Outcome::Counted(counted)) => {
                e.counted = Some(Arc::new(counted));
                // Scored at the next pump with the newest statuses.
                Status::Counting
            }
            Ok(Outcome::NoText) => Status::NoText,
            Ok(Outcome::TooLarge) => Status::TooLarge,
            Err(error) => Status::Failed(error),
        };
    }

    /// Discard a requested analysis and stop its worker before accepting new work.
    pub fn cancel(&mut self, uid: &str) {
        self.entries.remove(uid);
        self.pending.retain(|r| r.uid != uid);
    }

    /// Cancellation identity of the current request only.
    pub fn token(&self, request: &Request) -> Option<nikomiru_core::Cancel> {
        self.entries
            .get(&request.uid)
            .filter(|e| e.request == *request)
            .map(|e| e.cancel.clone())
    }

    /// The score or analysis state for a library row.
    pub fn get(&self, uid: &str) -> Option<&Status> {
        self.entries.get(uid).map(|e| &e.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;
    use nikomiru_library::books::BookRecord;

    // T-KNW-019: cancellation frees the slot and rejects a late old answer.
    #[test]
    fn cancelled_book_stops_worker_and_cannot_replace_restart() {
        let mut scores = Scores::default();
        scores.request("book/0".into(), "0.epub".into(), 0);
        let old = scores.take().pop().unwrap();
        let cancel = scores.token(&old).unwrap();
        scores.cancel("book/0");
        assert!(cancel.is_cancelled());
        assert!(!scores.busy());
        scores.request("book/0".into(), "0.epub".into(), 0);
        let new = scores.take().pop().unwrap();
        scores.done(&old, Ok(count()));
        assert_eq!(scores.get("book/0"), Some(&Status::Counting));
        assert!(!scores.token(&new).unwrap().is_cancelled());
        let active = scores.token(&new).unwrap();
        scores.clear();
        assert!(active.is_cancelled());
    }

    fn records() -> Vec<(String, BookRecord)> {
        (0..6)
            .map(|i| {
                (
                    format!("book/{i}"),
                    BookRecord {
                        path: format!("{i}.epub"),
                        ..BookRecord::default()
                    },
                )
            })
            .collect()
    }

    fn count() -> Outcome {
        Outcome::Counted(Counted::from_lines([
            vec![("猫".into(), "ねこ".into()), ("猫".into(), "ねこ".into())],
            vec![("読む".into(), "よむ".into())],
        ]))
    }

    // T-KNW-016: one job at a time, full counts retained and rescored.
    #[test]
    fn whole_book_counts_follow_shared_statuses_without_new_jobs() {
        let mut scores = Scores::default();
        let mut words = Words::default();
        let records = records();
        scores.request("book/0".into(), "0.epub".into(), 0);
        scores.request("book/1".into(), "1.epub".into(), 0);
        let jobs = scores.take();
        assert_eq!(jobs.len(), 1);
        assert!(scores.busy());
        scores.done(&jobs[0], Ok(count()));
        scores.update(true, &records, &words);
        assert_eq!(
            scores.get("book/0"),
            Some(&Status::Score(Score {
                words: 3,
                known: 0,
                lines: 2,
                clear: 0
            }))
        );
        words.set_status("猫", "ねこ", WordStatus::Known);
        words.set_status("読む", "よむ", WordStatus::Learning);
        scores.update(true, &records, &words);
        assert_eq!(
            scores.get("book/0"),
            Some(&Status::Score(Score {
                words: 3,
                known: 2,
                lines: 2,
                clear: 2
            }))
        );
        assert!(!scores.busy());
        assert!(scores.take().is_empty());
    }

    // T-KNW-016: ids survive cache clearing and reject old refresh replies.
    #[test]
    fn stale_results_cannot_replace_a_newer_request_or_dictionary() {
        let mut scores = Scores::default();
        let mut words = Words::default();
        let mut records = records();
        scores.request("book/0".into(), "0.epub".into(), 0);
        let old = scores.take().pop().unwrap();
        scores.update(false, &records, &words);
        scores.request("book/0".into(), "0.epub".into(), 0);
        let fresh = scores.take().pop().unwrap();
        assert_ne!(old.serial, fresh.serial);
        scores.done(&old, Ok(count()));
        assert_eq!(scores.get("book/0"), Some(&Status::Counting));
        words.forget();
        scores.update(true, &records, &words);
        scores.done(&fresh, Ok(count()));
        assert!(scores.get("book/0").is_none());
        scores.request("book/0".into(), "0.epub".into(), words.generation());
        let old = scores.take().pop().unwrap();
        records[0].1.path = "moved.epub".into();
        scores.update(true, &records, &words);
        scores.done(&old, Ok(count()));
        assert!(scores.get("book/0").is_none());
    }

    // T-KNW-016: bounded cache, explicit refresh, and excluded/error states.
    #[test]
    fn cache_eviction_and_failures_keep_no_partial_score() {
        let mut scores = Scores::default();
        let words = Words::default();
        let records = records();
        for (uid, record) in records.iter().take(KEPT + 1) {
            scores.request(uid.clone(), record.path.clone().into(), 0);
            let request = scores.take().pop().unwrap();
            scores.done(&request, Ok(count()));
            scores.update(true, &records, &words);
        }
        assert!(scores.get("book/0").is_none());
        assert_eq!(scores.entries.len(), KEPT);
        for (outcome, status) in [
            (Ok(Outcome::NoText), Status::NoText),
            (Ok(Outcome::TooLarge), Status::TooLarge),
            (
                Err("missing file".into()),
                Status::Failed("missing file".into()),
            ),
        ] {
            scores.request("book/1".into(), "1.epub".into(), 0);
            let request = scores.take().pop().unwrap();
            scores.done(&request, outcome);
            scores.update(true, &records, &words);
            assert_eq!(scores.get("book/1"), Some(&status));
        }
        scores.request("book/1".into(), "1.epub".into(), 0);
        let request = scores.take().pop().unwrap();
        scores.forget("book/1");
        scores.done(&request, Ok(count()));
        assert!(scores.get("book/1").is_none());
    }
}
