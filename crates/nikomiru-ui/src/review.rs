// SPDX-License-Identifier: GPL-3.0-only

//! The Review tab of the Library page (REV-004, REV-005): the jobs the
//! app's `library` worker runs for it, what it answers, and what the session
//! keeps of the card shown.

use std::path::PathBuf;

use nikomiru_library::review::{Grade, Memory, Queue, Settings};

use crate::catalog::Cover;

/// What the review asks of the library worker; each answers with the review
/// as it is after it.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewJob {
    /// Read it.
    Load,
    /// Answer a card.
    Answer(u64, Grade),
    /// Put back the memory a card had before its last answer.
    Undo(u64, Memory),
    /// Delete a card and the files no other card keeps.
    Delete(u64),
    /// A card starts over as new (REV-009).
    Reset(u64),
}

/// A card in the list of the Cards view (REV-009).
#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    /// Its id.
    pub id: u64,
    /// Headword.
    pub expression: String,
    /// Its reading.
    pub reading: String,
    /// Its sentence.
    pub sentence: String,
    /// Its memory.
    pub memory: Memory,
}

impl CardRow {
    /// Whether it holds `query` in its word, reading or sentence; an empty
    /// query holds everything.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim();
        query.is_empty()
            || self.expression.contains(query)
            || self.reading.contains(query)
            || self.sentence.contains(query)
    }
}

/// The review, read by the library worker.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Review {
    /// The cards to show and the counts.
    pub queue: Queue,
    /// The picture of the first card, decoded.
    pub picture: Option<Cover>,
    /// The sentence audio of the first card.
    pub audio: Option<PathBuf>,
    /// Every card, the newest first (REV-009).
    pub list: Vec<CardRow>,
}

/// What the session keeps of the Review tab.
#[derive(Debug, Default)]
pub struct ReviewPage {
    /// The review, once read.
    pub review: Option<Review>,
    /// A job is running.
    pub busy: bool,
    /// The answer of the card shown is visible.
    pub revealed: bool,
    /// The card answered last and its memory before: Undo puts it back.
    pub undo: Option<(u64, Memory)>,
    /// The Delete button was pressed once, for this card.
    pub confirm_delete: Option<u64>,
    /// The last error.
    pub error: Option<String>,
    /// The jobs asked and not yet answered: the worker answers them in order.
    pub(crate) running: usize,
}

impl ReviewPage {
    /// The card shown.
    pub fn current(&self) -> Option<&(u64, nikomiru_library::review::ReviewCard)> {
        self.review.as_ref()?.queue.cards.first()
    }

    /// A job was asked of the library worker. A job other than a read
    /// changes the cards: the next one shows its front at once.
    pub fn asked(&mut self, job: &ReviewJob) {
        self.running += 1;
        self.busy = true;
        self.error = None;
        self.confirm_delete = None;
        if *job != ReviewJob::Load {
            self.revealed = false;
        }
    }

    /// The library worker answered a job. The page is busy until the last
    /// job asked is answered; a read that shows the same card again keeps
    /// its answer in sight (AK-BUG-002).
    pub fn answered(&mut self, result: Result<Review, String>) {
        self.running = self.running.saturating_sub(1);
        self.busy = self.running > 0;
        match result {
            Ok(r) => {
                let shown = self.current().map(|(id, _)| *id);
                if r.queue.cards.first().map(|(id, _)| *id) != shown {
                    self.revealed = false;
                }
                self.review = Some(r);
            }
            Err(e) => {
                self.revealed = false;
                self.error = Some(e);
            }
        }
    }
}

/// `[review]` of the settings as the schedule takes it.
pub fn settings(config: &nikomiru_core::config::ReviewConfig) -> Settings {
    Settings {
        retention: config.retention,
        max_days: config.max_interval_days,
        new_per_day: config.new_per_day,
    }
}

/// How long until `due_ms`, short: `1 min`, `6 min`, `3 h`, `4 d`,
/// `2.5 mo`, `1.2 y` (REV-004), as a message id and its number.
pub fn span(now_ms: i64, due_ms: i64) -> (&'static str, String) {
    let minutes = ((due_ms - now_ms).max(0) as f64 / 60_000.0)
        .round()
        .max(1.0);
    let hours = minutes / 60.0;
    let days = hours / 24.0;
    let one = |v: f64| {
        if v < 10.0 && (v - v.round()).abs() >= 0.05 {
            format!("{v:.1}")
        } else {
            format!("{}", v.round())
        }
    };
    if minutes < 60.0 {
        ("review-minutes", format!("{minutes}"))
    } else if hours < 23.5 {
        ("review-hours", one(hours))
    } else if days < 30.0 {
        ("review-days", format!("{}", days.round()))
    } else if days < 365.0 {
        ("review-months", one(days / 30.0))
    } else {
        ("review-years", one(days / 365.0))
    }
}

/// The sentence cut around the word: before, the word, after.
pub fn sentence_parts(sentence: &str, word: Option<(u32, u32)>) -> (&str, &str, &str) {
    let range = word
        .and_then(|(s, e)| Some((usize::try_from(s).ok()?, usize::try_from(e).ok()?)))
        .filter(|&(s, e)| s <= e && sentence.get(s..e).is_some());
    match range {
        Some((s, e)) => (&sentence[..s], &sentence[s..e], &sentence[e..]),
        None => (sentence, "", ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_library::review::Grade;

    // T-REV-004
    #[test]
    fn spans_are_short() {
        const M: i64 = 60_000;
        assert_eq!(span(0, 10_000), ("review-minutes", "1".into()));
        assert_eq!(span(0, 6 * M), ("review-minutes", "6".into()));
        assert_eq!(span(0, 90 * M), ("review-hours", "1.5".into()));
        assert_eq!(span(0, 20 * 60 * M), ("review-hours", "20".into()));
        assert_eq!(span(0, 4 * 1440 * M), ("review-days", "4".into()));
        assert_eq!(span(0, 75 * 1440 * M), ("review-months", "2.5".into()));
        assert_eq!(span(0, 400 * 1440 * M), ("review-years", "1.1".into()));
        assert_eq!(span(5 * M, 0), ("review-minutes", "1".into()), "overdue");
    }

    // T-REV-009
    #[test]
    fn a_search_finds_word_reading_or_sentence() {
        let row = CardRow {
            id: 1,
            expression: "食べる".into(),
            reading: "たべる".into(),
            sentence: "パンを食べました".into(),
            memory: Memory::default(),
        };
        for query in ["", "  ", "食べ", "たべ", "パン"] {
            assert!(row.matches(query), "{query}");
        }
        assert!(!row.matches("飲む"));
    }

    // AK-BUG-002
    #[test]
    fn ak_bug_002_a_second_read_keeps_the_answer_shown() {
        let card = nikomiru_library::review::ReviewCard {
            expression: "食べる".into(),
            reading: "たべる".into(),
            sentence: String::new(),
            word: None,
            glossaries: Vec::new(),
            audio: None,
            picture: None,
            video_uid: String::new(),
            title: String::new(),
            start_ms: 0,
            end_ms: 0,
            created_at: 0,
            memory: Memory::default(),
        };
        let review = |ids: &[u64]| {
            Ok(Review {
                queue: Queue {
                    cards: ids.iter().map(|&id| (id, card.clone())).collect(),
                    ..Queue::default()
                },
                ..Review::default()
            })
        };
        let mut page = ReviewPage::default();
        // the Library page reads the review, and so does its Review tab
        page.asked(&ReviewJob::Load);
        page.asked(&ReviewJob::Load);
        page.answered(review(&[7]));
        assert!(page.busy, "the second read is still running");
        page.revealed = true;
        page.answered(review(&[7]));
        assert!(!page.busy);
        assert!(page.revealed, "the same card read again keeps its answer");
        // another card comes from its front
        page.asked(&ReviewJob::Load);
        page.answered(review(&[8]));
        assert!(!page.revealed);
        // so does the next card after an answer, even the same one again
        page.revealed = true;
        page.asked(&ReviewJob::Answer(8, Grade::Again));
        assert!(!page.revealed, "hidden as soon as it is answered");
        page.answered(review(&[8]));
        assert!(!page.revealed);
    }

    // T-REV-004
    #[test]
    fn the_word_is_cut_out_of_its_sentence() {
        assert_eq!(
            sentence_parts("パンを食べる", Some((9, 18))),
            ("パンを", "食べる", "")
        );
        assert_eq!(
            sentence_parts("パン", Some((1, 3))),
            ("パン", "", ""),
            "not a char boundary"
        );
        assert_eq!(sentence_parts("パン", None), ("パン", "", ""));
        assert_eq!(sentence_parts("パン", Some((3, 99))), ("パン", "", ""));
    }
}
