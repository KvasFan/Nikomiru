// SPDX-License-Identifier: GPL-3.0-only

//! Explicit, bounded series comprehension on library cards (KNW-017).
//! Counts use the episode resolver; percentages sum tokens and lines rather
//! than averaging percentages. Only one episode per analysis is in flight.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::catalog::Episodes;
use crate::words::{Counted, ListedWord, Score, Words};
use nikomiru_library::VideoRecord;

/// Series results retained in memory, including pending/error entries.
pub const KEPT: usize = 4;
/// Largest series analysed in one request; no truncated series score.
pub const MAX_EPISODES: usize = 256;

/// Identifies a series analysis across refreshes and invalidations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// Library series id.
    pub series: u64,
    /// Monotonic request id.
    pub serial: u64,
}

/// A completed score and the coverage of its source subtitles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Token/line totals across episodes with usable text.
    pub score: Score,
    /// Episodes whose text was counted.
    pub scored: usize,
    /// All episodes of the series, including excluded/missing sources.
    pub total: usize,
    /// Episodes that failed probing, reading or counting.
    pub failed: usize,
}

/// What a series card displays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Waiting for the library worker's episode list.
    Loading,
    /// Subtitle analysis in progress; no partial percentage displayed.
    Counting {
        /// Episodes completed, including excluded sources.
        done: usize,
        /// All episodes in this request.
        total: usize,
    },
    /// Completed aggregate with explicit coverage.
    Ready(Summary),
    /// No source could be counted.
    Unavailable {
        /// All episodes in this request.
        total: usize,
        /// Failures, distinct from missing/excluded text.
        failed: usize,
    },
    /// Entire request exceeds the episode bound.
    TooLarge,
    /// Library metadata could not be read.
    Failed(String),
}

#[derive(Debug)]
struct Entry {
    cancel: nikomiru_core::Cancel,
    request: Request,
    status: Status,
    queue: VecDeque<(String, VideoRecord)>,
    inflight: Option<String>,
    counts: Vec<Arc<Counted>>,
    words: Option<Arc<Vec<ListedWord>>>,
    failed: usize,
    revision: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
struct Key {
    generation: u64,
    preferred: Vec<String>,
    ffmpeg: String,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Ephemeral series analyses, separate from the currently open episode list.
#[derive(Debug, Default)]
pub struct Scores {
    key: Option<Key>,
    serial: u64,
    entries: HashMap<u64, Entry>,
}

impl Scores {
    /// Clear analyses without resetting ids that may still have callbacks.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Clear removed series and changed source settings; rescore retained
    /// counts with current statuses. This never opens or probes a file.
    pub fn update(
        &mut self,
        enabled: bool,
        words: &Words,
        preferred: &[String],
        ffmpeg: &str,
        ids: &[u64],
    ) {
        let key = Key {
            generation: words.generation(),
            preferred: preferred.into(),
            ffmpeg: ffmpeg.into(),
        };
        if !enabled || self.key.as_ref() != Some(&key) {
            self.clear();
            self.key = Some(key);
        }
        self.entries.retain(|id, _| ids.contains(id));
        for entry in self.entries.values_mut() {
            let total = match entry.status {
                Status::Counting { done, total } if done == total => total,
                Status::Ready(summary) => summary.total,
                _ => continue,
            };
            if entry.counts.is_empty() {
                entry.status = Status::Unavailable {
                    total,
                    failed: entry.failed,
                };
            } else if entry.revision != Some(words.revision()) {
                let mut score = Score::default();
                for counted in &entry.counts {
                    let next = counted.score(|e, r| words.status(e, r));
                    score.words += next.words;
                    score.known += next.known;
                    score.lines += next.lines;
                    score.clear += next.clear;
                }
                entry.status = Status::Ready(Summary {
                    score,
                    scored: entry.counts.len(),
                    total,
                    failed: entry.failed,
                });
                entry.words = Some(Arc::new(
                    words.listed_many(entry.counts.iter().map(Arc::as_ref)),
                ));
                entry.revision = Some(words.revision());
            }
        }
    }

    /// An explicit analysis is pending or running.
    pub fn busy(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry.status, Status::Loading | Status::Counting { .. }))
    }

    /// Start or refresh one series, returning the metadata request.
    pub fn request(&mut self, series: u64) -> Option<Request> {
        if self.busy() {
            return None;
        }
        if self.entries.len() >= KEPT
            && !self.entries.contains_key(&series)
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.request.serial)
                .map(|(id, _)| *id)
        {
            self.entries.remove(&oldest);
        }
        self.serial += 1;
        let request = Request {
            series,
            serial: self.serial,
        };
        self.entries.insert(
            series,
            Entry {
                cancel: nikomiru_core::Cancel::new(),
                request,
                status: Status::Loading,
                queue: VecDeque::new(),
                inflight: None,
                counts: Vec::new(),
                words: None,
                failed: 0,
                revision: None,
            },
        );
        Some(request)
    }

    /// Accept only the current request's library episode list.
    pub fn loaded(&mut self, request: Request, result: Result<Episodes, String>) {
        let Some(entry) = self
            .entries
            .get_mut(&request.series)
            .filter(|e| e.request == request && e.status == Status::Loading)
        else {
            return;
        };
        match result {
            Ok(episodes) if episodes.len() > MAX_EPISODES => entry.status = Status::TooLarge,
            Ok(episodes) => {
                entry.status = Status::Counting {
                    done: 0,
                    total: episodes.len(),
                };
                entry.queue = episodes.into();
            }
            Err(error) => entry.status = Status::Failed(error),
        }
    }

    /// Take at most one episode, leaving normal episode-list requests ahead
    /// of the next analysis job on the existing counter worker.
    pub fn next_episode(&mut self) -> Option<(Request, String, VideoRecord)> {
        for entry in self.entries.values_mut() {
            if entry.inflight.is_none()
                && let Some((uid, video)) = entry.queue.pop_front()
            {
                entry.inflight = Some(uid.clone());
                return Some((entry.request, uid, video));
            }
        }
        None
    }

    /// Retain a full episode count, or record an unavailable/failed source.
    /// Old and duplicate replies cannot advance progress or replace results.
    pub fn done(&mut self, request: Request, uid: &str, result: Result<Option<Counted>, String>) {
        let Some(entry) = self
            .entries
            .get_mut(&request.series)
            .filter(|e| e.request == request && e.inflight.as_deref() == Some(uid))
        else {
            return;
        };
        entry.inflight = None;
        if let Status::Counting { done, .. } = &mut entry.status {
            *done += 1;
        }
        match result {
            Ok(Some(counted)) => entry.counts.push(Arc::new(counted)),
            Ok(None) => {}
            Err(error) => {
                entry.failed += 1;
                tracing::debug!(
                    series = request.series,
                    episode = uid,
                    "series score: {error}"
                );
            }
        }
    }

    /// Discard a requested analysis and stop its worker before accepting new work.
    pub fn cancel(&mut self, series: u64) {
        self.entries.remove(&series);
    }

    /// Cancellation identity of the current request only.
    pub fn token(&self, request: Request) -> Option<nikomiru_core::Cancel> {
        self.entries
            .get(&request.series)
            .filter(|e| e.request == request)
            .map(|e| e.cancel.clone())
    }

    /// Vocabulary from all counted episodes of a completed request only.
    pub fn words(&self, series: u64) -> Option<(u64, &Arc<Vec<ListedWord>>)> {
        let entry = self.entries.get(&series)?;
        Some((entry.request.serial, entry.words.as_ref()?))
    }

    /// A card's current analysis state.
    pub fn get(&self, series: u64) -> Option<&Status> {
        self.entries.get(&series).map(|e| &e.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;

    // T-KNW-019: stop metadata or episode work, discard queue and late results.
    #[test]
    fn cancelled_series_stops_inflight_and_never_dispatches_remaining_episodes() {
        let mut scores = Scores::default();
        let old = scores.request(1).unwrap();
        let loading = scores.token(old).unwrap();
        scores.cancel(1);
        assert!(loading.is_cancelled());
        scores.loaded(old, Ok(episodes(2)));
        assert!(scores.next_episode().is_none());
        let request = scores.request(1).unwrap();
        scores.loaded(request, Ok(episodes(2)));
        let (_, uid, _) = scores.next_episode().unwrap();
        let active = scores.token(request).unwrap();
        scores.cancel(1);
        assert!(active.is_cancelled());
        assert!(scores.next_episode().is_none());
        let new = scores.request(1).unwrap();
        scores.done(request, &uid, Ok(Some(count(1, "猫"))));
        assert_eq!(scores.get(1), Some(&Status::Loading));
        assert!(!scores.token(new).unwrap().is_cancelled());
    }

    // T-KNW-021: repeated words sum across sources, distinct readings stay separate.
    #[test]
    fn complete_series_vocabulary_sums_sources_and_tracks_shared_statuses() {
        let mut scores = Scores::default();
        let mut words = Words::default();
        update(&mut scores, &words);
        let request = scores.request(1).unwrap();
        scores.loaded(request, Ok(episodes(3)));
        finish(
            &mut scores,
            Ok(Some(Counted::from_lines([vec![
                ("猫".into(), "ねこ".into()),
                ("猫".into(), "びょう".into()),
            ]]))),
        );
        assert!(scores.words(1).is_none());
        finish(
            &mut scores,
            Ok(Some(Counted::from_lines([vec![
                (
                    "猫".into(),
                    "ねこ".into()
                );
                2
            ]]))),
        );
        finish(&mut scores, Err("source unavailable".into()));
        update(&mut scores, &words);
        let (_, rows) = scores.words(1).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((&*rows[0].reading, rows[0].count), ("ねこ", 3));
        assert_eq!((&*rows[1].reading, rows[1].count), ("びょう", 1));
        let Some(Status::Ready(summary)) = scores.get(1) else {
            panic!()
        };
        assert_eq!((summary.scored, summary.total, summary.failed), (2, 3, 1));
        words.set_status("猫", "ねこ", WordStatus::Known);
        update(&mut scores, &words);
        assert_eq!(scores.words(1).unwrap().1[0].status, WordStatus::Known);
        scores.request(1).unwrap();
        assert!(scores.words(1).is_none());
    }

    fn episodes(n: usize) -> Episodes {
        (0..n)
            .map(|i| {
                (
                    format!("ep/{i}"),
                    VideoRecord {
                        path: format!("{i}.mkv"),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }
    fn count(n: usize, word: &str) -> Counted {
        Counted::from_lines([vec![(word.into(), word.into()); n]])
    }
    fn update(scores: &mut Scores, words: &Words) {
        scores.update(true, words, &["ja".into()], "", &[1, 2, 3, 4, 5]);
    }
    fn finish(scores: &mut Scores, result: Result<Option<Counted>, String>) -> (Request, String) {
        let (request, uid, _) = scores.next_episode().unwrap();
        assert!(
            scores.next_episode().is_none(),
            "only one episode in flight"
        );
        scores.done(request, &uid, result);
        (request, uid)
    }

    // T-KNW-017: weighted totals, missing/error coverage and live rescoring.
    #[test]
    fn scores_sum_occurrences_instead_of_averaging_percentages() {
        let mut scores = Scores::default();
        let mut words = Words::default();
        words.set_status("猫", "猫", WordStatus::Known);
        update(&mut scores, &words);
        let request = scores.request(1).unwrap();
        assert!(scores.request(2).is_none());
        scores.loaded(request, Ok(episodes(4)));
        finish(&mut scores, Ok(Some(count(1, "猫"))));
        finish(&mut scores, Ok(Some(count(9, "犬"))));
        finish(&mut scores, Ok(None));
        finish(&mut scores, Err("missing file".into()));
        update(&mut scores, &words);
        let Some(Status::Ready(summary)) = scores.get(1) else {
            panic!("expected full aggregate")
        };
        assert_eq!((summary.scored, summary.total, summary.failed), (2, 4, 1));
        assert_eq!(summary.score.known_percent(), Some(10));
        assert_eq!(summary.score.clear_percent(), Some(50));
        words.set_status("犬", "犬", WordStatus::Learning);
        update(&mut scores, &words);
        let Some(Status::Ready(summary)) = scores.get(1) else {
            panic!("expected rescored aggregate")
        };
        assert_eq!(summary.score.known_percent(), Some(10));
        assert_eq!(summary.score.clear_percent(), Some(100));
        words.set_status("犬", "犬", WordStatus::Ignored);
        update(&mut scores, &words);
        let Some(Status::Ready(summary)) = scores.get(1) else {
            panic!("expected ignored excluded")
        };
        assert_eq!(summary.score.words, 1);
        assert_eq!(summary.score.known_percent(), Some(100));
        assert!(scores.next_episode().is_none());
    }

    // T-KNW-017: no partial percentage, duplicate or replaced callbacks.
    #[test]
    fn progress_and_refresh_reject_duplicate_and_stale_answers() {
        let mut scores = Scores::default();
        let words = Words::default();
        update(&mut scores, &words);
        let old = scores.request(1).unwrap();
        scores.loaded(old, Ok(episodes(2)));
        let (request, uid) = finish(&mut scores, Ok(Some(count(1, "猫"))));
        scores.done(request, &uid, Ok(Some(count(1, "猫"))));
        update(&mut scores, &words);
        assert_eq!(scores.get(1), Some(&Status::Counting { done: 1, total: 2 }));
        finish(&mut scores, Ok(None));
        update(&mut scores, &words);
        let new = scores.request(1).unwrap();
        scores.loaded(old, Ok(episodes(1)));
        scores.done(old, "ep/0", Ok(Some(count(10, "猫"))));
        assert_eq!(scores.get(1), Some(&Status::Loading));
        scores.loaded(new, Ok(episodes(1)));
        finish(&mut scores, Ok(None));
        update(&mut scores, &words);
        assert_eq!(
            scores.get(1),
            Some(&Status::Unavailable {
                total: 1,
                failed: 0
            })
        );
    }

    // T-KNW-017: settings, dictionaries, removal and disabled colours clear
    // every retained/in-flight count without reusing request ids.
    #[test]
    fn invalidations_discard_counts_and_reject_old_replies() {
        let mut scores = Scores::default();
        let mut words = Words::default();
        update(&mut scores, &words);
        let request = scores.request(1).unwrap();
        scores.update(true, &words, &["en".into()], "", &[1]);
        scores.loaded(request, Ok(episodes(1)));
        assert!(scores.get(1).is_none());
        let request = scores.request(1).unwrap();
        scores.update(true, &words, &["en".into()], "/other/ffmpeg", &[1]);
        scores.loaded(request, Ok(episodes(1)));
        assert!(scores.get(1).is_none());
        let request = scores.request(1).unwrap();
        words.forget();
        scores.update(true, &words, &["en".into()], "/other/ffmpeg", &[1]);
        scores.loaded(request, Ok(episodes(1)));
        assert!(scores.get(1).is_none());
        let request = scores.request(1).unwrap();
        scores.update(true, &words, &["en".into()], "/other/ffmpeg", &[]);
        assert!(scores.get(1).is_none());
        scores.loaded(request, Ok(episodes(1)));
        let request = scores.request(1).unwrap();
        scores.update(false, &words, &["en".into()], "/other/ffmpeg", &[1]);
        scores.loaded(request, Ok(episodes(1)));
        assert!(scores.get(1).is_none());
        assert!(scores.request(1).unwrap().serial > request.serial);
    }

    // T-KNW-017: cache/episode bounds, empty series and library failures.
    #[test]
    fn limits_and_failures_never_keep_truncated_results() {
        let mut scores = Scores::default();
        let words = Words::default();
        update(&mut scores, &words);
        for id in 1..=5 {
            let request = scores.request(id).unwrap();
            scores.loaded(request, Ok(episodes(1)));
            finish(&mut scores, Ok(Some(count(1, "猫"))));
            update(&mut scores, &words);
        }
        assert!(scores.get(1).is_none());
        assert_eq!(scores.entries.len(), KEPT);
        let request = scores.request(2).unwrap();
        scores.loaded(request, Ok(episodes(MAX_EPISODES + 1)));
        assert_eq!(scores.get(2), Some(&Status::TooLarge));
        assert!(scores.next_episode().is_none());
        let request = scores.request(2).unwrap();
        scores.loaded(request, Ok(Vec::new()));
        update(&mut scores, &words);
        assert_eq!(
            scores.get(2),
            Some(&Status::Unavailable {
                total: 0,
                failed: 0
            })
        );
        let request = scores.request(2).unwrap();
        scores.loaded(request, Err("read only".into()));
        assert_eq!(scores.get(2), Some(&Status::Failed("read only".into())));
        assert!(!scores.busy());
    }
}
