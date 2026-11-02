// SPDX-License-Identifier: GPL-3.0-only

//! What was answered in the review (REV-007), in the `review_log` table of
//! `library.redb`, and the review's statistics drawn from it and from the
//! cards (REV-008). A new table: the schema version does not change.

use std::collections::BTreeMap;

use redb::{ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};
use crate::review::{Grade, Phase, ReviewCard, day_of};

pub(crate) const REVIEW_LOG: TableDefinition<u128, &[u8]> = TableDefinition::new("review_log");

/// A card is mature once it comes back three weeks or more after an answer.
pub const MATURE_DAYS: i64 = 21;
/// The days of the statistics' chart and of their true retention.
pub const DAYS: i64 = 30;
/// The days ahead of the forecast.
pub const AHEAD: i64 = 7;

/// An answer (REV-007).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// The card.
    pub card: u64,
    /// The grade given.
    pub grade: Grade,
    /// Where the card was before it.
    pub phase: Phase,
    /// Days since the answer before it; 0 for the first.
    pub elapsed_days: f64,
    /// Its stability after it, in days.
    pub stability: f64,
}

/// The key of an answer: when, then the card, so that the log is in order.
pub fn key(at_ms: i64, card: u64) -> u128 {
    (u128::from(at_ms.max(0) as u64) << 64) | u128::from(card)
}

/// When the answer of `key` was given.
pub fn time_of(key: u128) -> i64 {
    (key >> 64) as i64
}

/// Notes an answer, inside the transaction that gives it.
pub(crate) fn note(tx: &WriteTransaction, key: u128, answer: &Answer) -> Result<(), LibraryError> {
    let mut t = db_err!(tx.open_table(REVIEW_LOG))?;
    db_err!(t.insert(key, postcard::to_stdvec(answer)?.as_slice()))?;
    Ok(())
}

/// Forgets an answer taken back, inside the transaction that takes it back.
pub(crate) fn forget(tx: &WriteTransaction, key: u128) -> Result<(), LibraryError> {
    let mut t = db_err!(tx.open_table(REVIEW_LOG))?;
    db_err!(t.remove(key))?;
    Ok(())
}

/// The review's statistics (REV-008).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReviewStats {
    /// Answers today, and how many of them were not Again.
    pub today: (u32, u32),
    /// Answers on each of the last [`DAYS`] days, the oldest first.
    pub days: Vec<u32>,
    /// Of the answers of the last [`DAYS`] days on cards in review, the share
    /// remembered; `None` without one.
    pub retention: Option<f64>,
    /// Cards never answered.
    pub new: u32,
    /// Cards learning or relearning.
    pub learning: u32,
    /// Cards in review back in under three weeks.
    pub young: u32,
    /// Cards in review back in three weeks or more.
    pub mature: u32,
    /// Cards due on each of the next [`AHEAD`] days, today first, with the
    /// overdue ones.
    pub ahead: Vec<u32>,
}

/// The statistics of `cards` and `log` at `now_ms`.
pub fn stats(
    cards: &[ReviewCard],
    log: &BTreeMap<u128, Answer>,
    now_ms: i64,
    offset_s: i32,
) -> ReviewStats {
    let today = day_of(now_ms, offset_s);
    let mut out = ReviewStats {
        days: vec![0; DAYS as usize],
        ahead: vec![0; AHEAD as usize],
        ..ReviewStats::default()
    };
    let (mut kept, mut asked) = (0u32, 0u32);
    for (key, answer) in log {
        let ago = today - day_of(time_of(*key), offset_s);
        if !(0..DAYS).contains(&ago) {
            continue;
        }
        out.days[(DAYS - 1 - ago) as usize] += 1;
        let remembered = answer.grade != Grade::Again;
        if ago == 0 {
            out.today.0 += 1;
            out.today.1 += u32::from(remembered);
        }
        if answer.phase == Phase::Review {
            asked += 1;
            kept += u32::from(remembered);
        }
    }
    out.retention = (asked > 0).then(|| f64::from(kept) / f64::from(asked));
    for card in cards {
        let m = &card.memory;
        match m.phase {
            Phase::New => out.new += 1,
            Phase::Learning | Phase::Relearning => out.learning += 1,
            Phase::Review => {
                let interval = day_of(m.due_ms, offset_s) - day_of(m.last_ms, offset_s);
                if interval >= MATURE_DAYS {
                    out.mature += 1;
                } else {
                    out.young += 1;
                }
            }
        }
        if m.phase != Phase::New {
            let ahead = (day_of(m.due_ms, offset_s) - today).max(0);
            if ahead < AHEAD {
                out.ahead[ahead as usize] += 1;
            }
        }
    }
    out
}

impl Library {
    /// Every answer, in order (REV-007).
    pub fn review_log(&self) -> Result<BTreeMap<u128, Answer>, LibraryError> {
        self.read(|tx| match tx.open_table(REVIEW_LOG) {
            Ok(t) => {
                let mut out = BTreeMap::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.insert(k.value(), postcard::from_bytes(v.value())?);
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(BTreeMap::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// The review's statistics at `now_ms` (REV-008).
    pub fn review_stats(&self, now_ms: i64, offset_s: i32) -> Result<ReviewStats, LibraryError> {
        let cards: Vec<ReviewCard> = self
            .review_queue_rows()?
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        Ok(stats(&cards, &self.review_log()?, now_ms, offset_s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{Memory, Settings};

    const NOW: i64 = 1_760_000_000_000;
    const DAY: i64 = 86_400_000;

    fn card(phase: Phase, last_ms: i64, due_ms: i64) -> ReviewCard {
        ReviewCard {
            expression: "語".into(),
            reading: String::new(),
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
            memory: Memory {
                phase,
                last_ms,
                due_ms,
                ..Memory::default()
            },
        }
    }

    fn answer(grade: Grade, phase: Phase) -> Answer {
        Answer {
            card: 1,
            grade,
            phase,
            elapsed_days: 1.0,
            stability: 1.0,
        }
    }

    // T-REV-008
    #[test]
    fn the_statistics_count_days_retention_maturity_and_what_comes() {
        let mut log = BTreeMap::new();
        log.insert(key(NOW, 1), answer(Grade::Good, Phase::Review));
        log.insert(key(NOW - 60_000, 2), answer(Grade::Again, Phase::Review));
        log.insert(key(NOW - 60_000, 3), answer(Grade::Again, Phase::New));
        log.insert(key(NOW - 2 * DAY, 1), answer(Grade::Easy, Phase::Review));
        log.insert(key(NOW - 40 * DAY, 1), answer(Grade::Again, Phase::Review));
        let cards = [
            card(Phase::New, 0, 0),
            card(Phase::Learning, NOW, NOW + 60_000),
            card(Phase::Review, NOW - DAY, NOW + 3 * DAY),
            card(Phase::Review, NOW - DAY, NOW + 40 * DAY),
            card(Phase::Review, NOW - 9 * DAY, NOW - 2 * DAY),
        ];
        let s = stats(&cards, &log, NOW, 0);
        assert_eq!(s.today, (3, 1));
        assert_eq!(s.days[DAYS as usize - 1], 3);
        assert_eq!(s.days[DAYS as usize - 3], 1);
        assert_eq!(s.days.iter().sum::<u32>(), 4, "40 days ago is out");
        // two of three remembered in review, the new card left out
        assert_eq!(s.retention, Some(2.0 / 3.0));
        assert_eq!((s.new, s.learning, s.young, s.mature), (1, 1, 2, 1));
        // the learning card and the overdue one today, one in three days
        assert_eq!(s.ahead, [2, 0, 0, 1, 0, 0, 0]);
        assert_eq!(stats(&[], &BTreeMap::new(), NOW, 0).retention, None);
        assert_eq!(time_of(key(NOW, 7)), NOW);
    }

    // T-REV-007: an answer is noted, and an undo forgets it
    #[test]
    fn answers_are_noted_and_an_undo_forgets_them() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let id = lib.add_review_card(&card(Phase::New, 0, 0)).unwrap();
        let before = lib.review_card(id).unwrap().unwrap().memory;
        lib.answer_review_card(id, Grade::Good, NOW, 0, &Settings::default())
            .unwrap();
        let log = lib.review_log().unwrap();
        assert_eq!(log.len(), 1);
        let (k, a) = log.iter().next().unwrap();
        assert_eq!(
            (time_of(*k), a.card, a.grade, a.phase),
            (NOW, id, Grade::Good, Phase::New)
        );
        lib.restore_review_memory(id, &before).unwrap();
        assert!(lib.review_log().unwrap().is_empty());
        assert_eq!(lib.review_stats(NOW, 0).unwrap().new, 1);
    }
}
