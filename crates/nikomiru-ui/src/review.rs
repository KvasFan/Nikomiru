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
}

impl ReviewPage {
    /// The card shown.
    pub fn current(&self) -> Option<&(u64, nikomiru_library::review::ReviewCard)> {
        self.review.as_ref()?.queue.cards.first()
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
