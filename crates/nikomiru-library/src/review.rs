// SPDX-License-Identifier: GPL-3.0-only

//! Nikomiru's own review (REV-001…REV-006, D-34): the cards made in Nikomiru
//! rather than in Anki, in the `review_cards` table of `library.redb`, and
//! when each comes back. The schedule is FSRS-5, written here from the
//! published algorithm (the formulas and default parameters of
//! open-spaced-repetition's "The Algorithm"), so it is GPL like the rest and
//! needs no license exception (§3). A new table leaves the other records as
//! they are, so the schema version does not change (ADR 0007 §26).

use std::collections::HashSet;

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

pub(crate) const REVIEW_CARDS: TableDefinition<u64, &[u8]> = TableDefinition::new("review_cards");

/// Set in the id of every review card, so that it never meets one of
/// Anki's note ids in `mined_cards` (REV-002).
pub const ID_BIT: u64 = 1 << 62;

/// The parameters of FSRS-5 when nothing has been fitted to the user.
pub const PARAMETERS: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925,
    1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
];
const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;

const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;
/// The day turns at 04:00 local time, as in Anki: a late evening belongs to
/// the day it started in.
const ROLLOVER_S: i64 = 4 * 3600;
/// How far ahead a learning card may be shown when nothing else is due.
pub const LEARN_AHEAD: i64 = 20 * MINUTE;

/// The answer to a card (REV-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grade {
    /// Forgotten.
    Again = 1,
    /// Remembered with effort.
    Hard = 2,
    /// Remembered.
    Good = 3,
    /// Remembered at once.
    Easy = 4,
}

impl Grade {
    /// The four, in order.
    pub const ALL: [Grade; 4] = [Grade::Again, Grade::Hard, Grade::Good, Grade::Easy];

    fn value(self) -> f64 {
        f64::from(self as u8)
    }
}

/// Where a card is in its learning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Phase {
    /// Never answered.
    #[default]
    New,
    /// Answered today for the first time, back in minutes.
    Learning,
    /// Back in days.
    Review,
    /// Forgotten from review, back in minutes.
    Relearning,
}

/// What the schedule knows of a card.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Memory {
    /// Its phase.
    pub phase: Phase,
    /// FSRS stability, in days.
    pub stability: f64,
    /// FSRS difficulty, 1–10.
    pub difficulty: f64,
    /// When it is due, Unix ms.
    pub due_ms: i64,
    /// When it was last answered, Unix ms; 0 never.
    pub last_ms: i64,
    /// When it was first answered, Unix ms; 0 never: the new cards of a day
    /// are counted by it.
    pub first_ms: i64,
    /// Answers.
    pub reps: u32,
    /// Times forgotten after it reached review.
    pub lapses: u32,
}

/// A card of the review (REV-001).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewCard {
    /// Headword.
    pub expression: String,
    /// Its reading.
    pub reading: String,
    /// The sentence, as plain text; empty without one.
    pub sentence: String,
    /// Where the word is in it, bytes.
    pub word: Option<(u32, u32)>,
    /// The meanings: the dictionary's name, then its text.
    pub glossaries: Vec<(String, String)>,
    /// The sentence audio's file in the review folder.
    pub audio: Option<String>,
    /// The picture's file in the review folder.
    pub picture: Option<String>,
    /// The video's uid (DAT-003); empty without the library or for a book.
    pub video_uid: String,
    /// The video's or book's title.
    pub title: String,
    /// Start of the clip, ms.
    pub start_ms: u32,
    /// End of the clip, ms.
    pub end_ms: u32,
    /// When it was made or last overwritten, Unix ms.
    pub created_at: i64,
    /// Its schedule.
    pub memory: Memory,
}

impl ReviewCard {
    /// The files it keeps in the review folder.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.audio.iter().chain(&self.picture).map(String::as_str)
    }
}

/// The schedule's settings (REV-003).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The share of cards to remember when they come back, 0.70–0.97.
    pub retention: f64,
    /// The longest interval, days.
    pub max_days: u32,
    /// New cards a day.
    pub new_per_day: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retention: 0.9,
            max_days: 36_500,
            new_per_day: 20,
        }
    }
}

/// The local day `unix_ms` falls in, with the 04:00 rollover.
pub fn day_of(unix_ms: i64, offset_s: i32) -> i64 {
    (unix_ms.div_euclid(1000) + i64::from(offset_s) - ROLLOVER_S).div_euclid(86_400)
}

/// When local day `day` starts, Unix ms.
pub fn day_start(day: i64, offset_s: i32) -> i64 {
    (day * 86_400 + ROLLOVER_S - i64::from(offset_s)) * 1000
}

fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    (1.0 + FACTOR * elapsed_days.max(0.0) / stability).powf(DECAY)
}

/// Days until recall falls to `retention`.
fn interval(stability: f64, retention: f64) -> f64 {
    stability / FACTOR * (retention.powf(1.0 / DECAY) - 1.0)
}

fn initial_stability(g: Grade) -> f64 {
    PARAMETERS[g as usize - 1].max(0.1)
}

fn initial_difficulty_raw(g: Grade) -> f64 {
    PARAMETERS[4] - (PARAMETERS[5] * (g.value() - 1.0)).exp() + 1.0
}

fn initial_difficulty(g: Grade) -> f64 {
    initial_difficulty_raw(g).clamp(1.0, 10.0)
}

fn next_difficulty(d: f64, g: Grade) -> f64 {
    let delta = -PARAMETERS[6] * (g.value() - 3.0);
    // linear damping, then the pull back to the difficulty of an easy card
    let damped = d + delta * (10.0 - d) / 9.0;
    let reverted =
        PARAMETERS[7] * initial_difficulty_raw(Grade::Easy) + (1.0 - PARAMETERS[7]) * damped;
    reverted.clamp(1.0, 10.0)
}

fn recall_stability(d: f64, s: f64, r: f64, g: Grade) -> f64 {
    let hard = if g == Grade::Hard {
        PARAMETERS[15]
    } else {
        1.0
    };
    let easy = if g == Grade::Easy {
        PARAMETERS[16]
    } else {
        1.0
    };
    s * (1.0
        + PARAMETERS[8].exp()
            * (11.0 - d)
            * s.powf(-PARAMETERS[9])
            * ((PARAMETERS[10] * (1.0 - r)).exp() - 1.0)
            * hard
            * easy)
}

fn forget_stability(d: f64, s: f64, r: f64) -> f64 {
    let after = PARAMETERS[11]
        * d.powf(-PARAMETERS[12])
        * ((s + 1.0).powf(PARAMETERS[13]) - 1.0)
        * (PARAMETERS[14] * (1.0 - r)).exp();
    after.min(s)
}

/// Stability after an answer on the day of the last one.
fn short_term_stability(s: f64, g: Grade) -> f64 {
    s * (PARAMETERS[17] * (g.value() - 3.0 + PARAMETERS[18])).exp()
}

/// The card's memory after answering `grade` at `now_ms` (REV-003).
pub fn answer(
    memory: &Memory,
    grade: Grade,
    now_ms: i64,
    offset_s: i32,
    settings: &Settings,
) -> Memory {
    let today = day_of(now_ms, offset_s);
    let mut next = *memory;
    next.reps += 1;
    next.last_ms = now_ms;
    if memory.first_ms == 0 {
        next.first_ms = now_ms;
    }
    let in_days = |days: u32| day_start(today + i64::from(days), offset_s);
    let days_for = |s: f64| -> u32 {
        let days = interval(s, settings.retention).round();
        days.clamp(1.0, f64::from(settings.max_days.max(1))) as u32
    };
    match memory.phase {
        Phase::New => {
            next.stability = initial_stability(grade);
            next.difficulty = initial_difficulty(grade);
            match grade {
                Grade::Again => learning(&mut next, now_ms + MINUTE, Phase::Learning),
                Grade::Hard => learning(&mut next, now_ms + 6 * MINUTE, Phase::Learning),
                Grade::Good => learning(&mut next, now_ms + 10 * MINUTE, Phase::Learning),
                Grade::Easy => {
                    // never sooner than Good would bring it back after its step
                    let good = days_for(initial_stability(Grade::Good));
                    let days = days_for(next.stability).max(good + 1);
                    graduate(&mut next, in_days(days));
                }
            }
        }
        Phase::Learning | Phase::Relearning => {
            next.difficulty = next_difficulty(memory.difficulty, grade);
            next.stability = short_term_stability(memory.stability, grade);
            let phase = memory.phase;
            match grade {
                Grade::Again => learning(&mut next, now_ms + MINUTE, phase),
                Grade::Hard => learning(&mut next, now_ms + 6 * MINUTE, phase),
                Grade::Good => {
                    let due = in_days(days_for(next.stability));
                    graduate(&mut next, due);
                }
                Grade::Easy => {
                    let good = days_for(short_term_stability(memory.stability, Grade::Good));
                    let days = days_for(next.stability).max(good + 1);
                    graduate(&mut next, in_days(days));
                }
            }
        }
        Phase::Review => {
            let elapsed = (now_ms - memory.last_ms) as f64 / DAY as f64;
            let r = retrievability(elapsed, memory.stability);
            let (d, s) = (memory.difficulty, memory.stability);
            next.difficulty = next_difficulty(d, grade);
            if grade == Grade::Again {
                next.lapses += 1;
                next.stability = forget_stability(d, s, r);
                learning(&mut next, now_ms + 10 * MINUTE, Phase::Relearning);
            } else {
                // Hard, Good and Easy each further than the one before
                let days = |g: Grade| days_for(recall_stability(d, s, r, g));
                let hard = days(Grade::Hard);
                let good = days(Grade::Good).max(hard + 1);
                let chosen = match grade {
                    Grade::Hard => hard.min(good),
                    Grade::Good => good,
                    _ => days(Grade::Easy).max(good + 1),
                };
                next.stability = recall_stability(d, s, r, grade);
                graduate(&mut next, in_days(chosen.min(settings.max_days.max(1))));
            }
        }
    }
    next
}

fn learning(memory: &mut Memory, due_ms: i64, phase: Phase) {
    memory.phase = phase;
    memory.due_ms = due_ms;
}

fn graduate(memory: &mut Memory, due_ms: i64) {
    memory.phase = Phase::Review;
    memory.due_ms = due_ms;
}

/// What the review holds now (REV-004).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Queue {
    /// The cards to show, in order: learning ones due, the reviews due,
    /// then the new ones the day still allows.
    pub cards: Vec<(u64, ReviewCard)>,
    /// New cards still to learn today, among `cards`.
    pub new: usize,
    /// Learning and relearning cards, among `cards`.
    pub learning: usize,
    /// Reviews due, among `cards`.
    pub review: usize,
    /// All the cards.
    pub total: usize,
    /// When the next card comes back after these, Unix ms.
    pub next_due_ms: Option<i64>,
}

impl Library {
    pub(crate) fn review_queue_rows(&self) -> Result<Vec<(u64, ReviewCard)>, LibraryError> {
        self.review_rows()
    }

    fn review_rows(&self) -> Result<Vec<(u64, ReviewCard)>, LibraryError> {
        self.read(|tx| match tx.open_table(REVIEW_CARDS) {
            Ok(t) => {
                let mut out = Vec::new();
                for row in db_err!(t.iter())? {
                    let (k, v) = db_err!(row)?;
                    out.push((k.value(), postcard::from_bytes(v.value())?));
                }
                Ok(out)
            }
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(Vec::new()),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Adds a card (REV-001); its id has [`ID_BIT`] set.
    pub fn add_review_card(&self, card: &ReviewCard) -> Result<u64, LibraryError> {
        let bytes = postcard::to_stdvec(card)?;
        let tx = self.write()?;
        let id = {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let mut id = ID_BIT | (card.created_at.max(0) as u64 & (ID_BIT - 1));
            while db_err!(t.get(id))?.is_some() {
                id += 1;
            }
            db_err!(t.insert(id, bytes.as_slice()))?;
            id
        };
        db_err!(tx.commit())?;
        Ok(id)
    }

    /// The card `id`.
    pub fn review_card(&self, id: u64) -> Result<Option<ReviewCard>, LibraryError> {
        self.read(|tx| match tx.open_table(REVIEW_CARDS) {
            Ok(t) => match db_err!(t.get(id))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Writes `card` over card `id`, keeping its schedule (an overwrite,
    /// ANK-037); returns the card it replaced.
    pub fn replace_review_card(
        &self,
        id: u64,
        card: &ReviewCard,
    ) -> Result<Option<ReviewCard>, LibraryError> {
        let tx = self.write()?;
        let before = {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let before: Option<ReviewCard> = match db_err!(t.get(id))? {
                Some(v) => Some(postcard::from_bytes(v.value())?),
                None => None,
            };
            if let Some(before) = &before {
                let card = ReviewCard {
                    memory: before.memory,
                    ..card.clone()
                };
                db_err!(t.insert(id, postcard::to_stdvec(&card)?.as_slice()))?;
            }
            before
        };
        db_err!(tx.commit())?;
        Ok(before)
    }

    /// Removes card `id` and its record in `mined_cards` (REV-005); returns
    /// it, for its files.
    pub fn delete_review_card(&self, id: u64) -> Result<Option<ReviewCard>, LibraryError> {
        let tx = self.write()?;
        let removed = {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let removed = db_err!(t.remove(id))?;
            let removed: Option<ReviewCard> = match removed {
                Some(v) => Some(postcard::from_bytes(v.value())?),
                None => None,
            };
            let mut mined = db_err!(tx.open_table(crate::cards::MINED_CARDS))?;
            db_err!(mined.remove(id))?;
            // it does not come back from another device (REV-006)
            if removed.is_some() {
                crate::sync::bury(&tx, &crate::sync::review_key(id))?;
            }
            removed
        };
        db_err!(tx.commit())?;
        Ok(removed)
    }

    /// The files the cards keep, to know which can go.
    pub fn review_files(&self) -> Result<HashSet<String>, LibraryError> {
        Ok(self
            .review_rows()?
            .iter()
            .flat_map(|(_, c)| c.files().map(str::to_string).collect::<Vec<_>>())
            .collect())
    }

    /// For each headword and reading, whether the review has it (ANK-030).
    pub fn review_has(&self, words: &[(String, String)]) -> Result<Vec<bool>, LibraryError> {
        let known: HashSet<(String, String)> = self
            .review_rows()?
            .into_iter()
            .map(|(_, c)| (c.expression, c.reading))
            .collect();
        Ok(words.iter().map(|w| known.contains(w)).collect())
    }

    /// Answers card `id` (REV-003); returns its new memory.
    pub fn answer_review_card(
        &self,
        id: u64,
        grade: Grade,
        now_ms: i64,
        offset_s: i32,
        settings: &Settings,
    ) -> Result<Option<Memory>, LibraryError> {
        let tx = self.write()?;
        let memory = {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let card: Option<ReviewCard> = match db_err!(t.get(id))? {
                Some(v) => Some(postcard::from_bytes(v.value())?),
                None => None,
            };
            match card {
                Some(mut card) => {
                    let before = card.memory;
                    card.memory = answer(&before, grade, now_ms, offset_s, settings);
                    db_err!(t.insert(id, postcard::to_stdvec(&card)?.as_slice()))?;
                    // REV-007
                    let elapsed_days = if before.last_ms == 0 {
                        0.0
                    } else {
                        (now_ms - before.last_ms) as f64 / DAY as f64
                    };
                    crate::review_log::note(
                        &tx,
                        crate::review_log::key(now_ms, id),
                        &crate::review_log::Answer {
                            card: id,
                            grade,
                            phase: before.phase,
                            elapsed_days,
                            stability: card.memory.stability,
                        },
                    )?;
                    Some(card.memory)
                }
                None => None,
            }
        };
        db_err!(tx.commit())?;
        Ok(memory)
    }

    /// Every card, the newest first (REV-009).
    pub fn review_cards(&self) -> Result<Vec<(u64, ReviewCard)>, LibraryError> {
        let mut rows = self.review_rows()?;
        rows.sort_by_key(|(id, c)| std::cmp::Reverse((c.created_at, *id)));
        Ok(rows)
    }

    /// Card `id` starts over as a new card (REV-009); its answers stay in
    /// the log. Its memory is dated `now_ms`, so that the sync takes the
    /// reset over what other devices knew of it (REV-006).
    pub fn reset_review_card(&self, id: u64, now_ms: i64) -> Result<bool, LibraryError> {
        let tx = self.write()?;
        let found = {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let card: Option<ReviewCard> = match db_err!(t.get(id))? {
                Some(v) => Some(postcard::from_bytes(v.value())?),
                None => None,
            };
            match card {
                Some(mut card) => {
                    card.memory = Memory {
                        last_ms: now_ms,
                        ..Memory::default()
                    };
                    db_err!(t.insert(id, postcard::to_stdvec(&card)?.as_slice()))?;
                    true
                }
                None => false,
            }
        };
        db_err!(tx.commit())?;
        Ok(found)
    }

    /// Puts back the memory a card had before an answer (REV-004's undo).
    pub fn restore_review_memory(&self, id: u64, memory: &Memory) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(REVIEW_CARDS))?;
            let card: Option<ReviewCard> = match db_err!(t.get(id))? {
                Some(v) => Some(postcard::from_bytes(v.value())?),
                None => None,
            };
            if let Some(mut card) = card {
                // the answer taken back leaves the log (REV-007)
                if card.memory.last_ms != memory.last_ms {
                    crate::review_log::forget(
                        &tx,
                        crate::review_log::key(card.memory.last_ms, id),
                    )?;
                }
                card.memory = *memory;
                db_err!(t.insert(id, postcard::to_stdvec(&card)?.as_slice()))?;
            }
        }
        db_err!(tx.commit())
    }

    /// What to review at `now_ms` (REV-004).
    pub fn review_queue(
        &self,
        now_ms: i64,
        offset_s: i32,
        settings: &Settings,
    ) -> Result<Queue, LibraryError> {
        let rows = self.review_rows()?;
        let today = day_of(now_ms, offset_s);
        let learned_today = rows
            .iter()
            .filter(|(_, c)| c.memory.first_ms != 0 && day_of(c.memory.first_ms, offset_s) == today)
            .count();
        let new_left = (settings.new_per_day as usize).saturating_sub(learned_today);
        let mut learning = Vec::new();
        let mut review = Vec::new();
        let mut new = Vec::new();
        let mut later = Vec::new();
        for (id, card) in &rows {
            let m = &card.memory;
            match m.phase {
                Phase::New => new.push((*id, card.clone())),
                Phase::Learning | Phase::Relearning if m.due_ms <= now_ms => {
                    learning.push((*id, card.clone()));
                }
                Phase::Review if m.due_ms <= now_ms => review.push((*id, card.clone())),
                _ => later.push((*id, card.clone())),
            }
        }
        learning.sort_by_key(|(id, c)| (c.memory.due_ms, *id));
        review.sort_by_key(|(id, c)| (c.memory.due_ms, *id));
        new.sort_by_key(|(id, c)| (c.created_at, *id));
        new.truncate(new_left);
        later.sort_by_key(|(id, c)| (c.memory.due_ms, *id));
        // with nothing else to do, a learning card soon due comes early
        if learning.is_empty() && review.is_empty() && new.is_empty() {
            let soon = later.iter().position(|(_, c)| {
                matches!(c.memory.phase, Phase::Learning | Phase::Relearning)
                    && c.memory.due_ms <= now_ms + LEARN_AHEAD
            });
            if let Some(i) = soon {
                learning.push(later.remove(i));
            }
        }
        let next_due_ms = later.iter().map(|(_, c)| c.memory.due_ms).min();
        let (n_learning, n_review, n_new) = (learning.len(), review.len(), new.len());
        let mut cards = learning;
        cards.extend(review);
        cards.extend(new);
        Ok(Queue {
            cards,
            new: n_new,
            learning: n_learning,
            review: n_review,
            total: rows.len(),
            next_due_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000_000; // 2025-10-09 08:53:20 UTC
    const SETTINGS: Settings = Settings {
        retention: 0.9,
        max_days: 36_500,
        new_per_day: 20,
    };

    fn card(expression: &str, created_at: i64) -> ReviewCard {
        ReviewCard {
            expression: expression.into(),
            reading: "たべる".into(),
            sentence: "パンを食べる".into(),
            word: Some((9, 18)),
            glossaries: vec![("JMdict".into(), "to eat".into())],
            audio: Some("nikomiru_0123456789abcdef.m4a".into()),
            picture: Some("nikomiru_fedcba9876543210.jpg".into()),
            video_uid: "video/av_basic".into(),
            title: "av_basic".into(),
            start_ms: 1_880,
            end_ms: 2_700,
            created_at,
            memory: Memory::default(),
        }
    }

    fn days_until(m: &Memory, now: i64) -> i64 {
        day_of(m.due_ms, 0) - day_of(now, 0)
    }

    // T-REV-003
    #[test]
    fn the_first_answers_follow_fsrs() {
        let new = Memory::default();
        let again = answer(&new, Grade::Again, NOW, 0, &SETTINGS);
        assert_eq!((again.phase, again.due_ms - NOW), (Phase::Learning, MINUTE));
        assert!((again.stability - 0.40255).abs() < 1e-9);
        // D0(1) = w4 = 7.1949
        assert!((again.difficulty - 7.1949).abs() < 1e-9);
        let good = answer(&new, Grade::Good, NOW, 0, &SETTINGS);
        assert_eq!(
            (good.phase, good.due_ms - NOW),
            (Phase::Learning, 10 * MINUTE)
        );
        assert!((good.stability - 3.173).abs() < 1e-9);
        // D0(3) = w4 - e^(2 w5) + 1
        assert!((good.difficulty - (7.1949 - (2.0f64 * 0.5345).exp() + 1.0)).abs() < 1e-9);
        assert_eq!((good.reps, good.first_ms, good.last_ms), (1, NOW, NOW));
        // Easy goes straight to review: S0(4) = 15.69 days at 90 %
        let easy = answer(&new, Grade::Easy, NOW, 0, &SETTINGS);
        assert_eq!(easy.phase, Phase::Review);
        assert_eq!(days_until(&easy, NOW), 16);
        assert_eq!(easy.due_ms, day_start(day_of(NOW, 0) + 16, 0));
    }

    // T-REV-003
    #[test]
    fn at_ninety_percent_the_interval_is_the_stability() {
        assert!((interval(7.0, 0.9) - 7.0).abs() < 1e-9);
        assert!((retrievability(7.0, 7.0) - 0.9).abs() < 1e-9);
        assert!(interval(7.0, 0.8) > 7.0 && interval(7.0, 0.95) < 7.0);
    }

    // T-REV-003
    #[test]
    fn a_learnt_card_comes_back_further_each_time() {
        let mut m = answer(&Memory::default(), Grade::Good, NOW, 0, &SETTINGS);
        let mut now = NOW + 10 * MINUTE;
        m = answer(&m, Grade::Good, now, 0, &SETTINGS);
        assert_eq!(m.phase, Phase::Review);
        let mut last = days_until(&m, now);
        assert!(last >= 1, "graduated to days: {last}");
        for _ in 0..6 {
            now = m.due_ms + 3_600_000;
            m = answer(&m, Grade::Good, now, 0, &SETTINGS);
            let days = days_until(&m, now);
            assert!(days > last, "{days} after {last}");
            last = days;
        }
        assert_eq!(m.reps, 8);
        assert_eq!(m.lapses, 0);
    }

    // T-REV-003
    #[test]
    fn the_buttons_of_a_review_are_in_order() {
        let mut m = answer(&Memory::default(), Grade::Good, NOW, 0, &SETTINGS);
        m = answer(&m, Grade::Good, NOW + 10 * MINUTE, 0, &SETTINGS);
        let now = m.due_ms + 3_600_000;
        let due: Vec<Memory> = Grade::ALL
            .iter()
            .map(|g| answer(&m, *g, now, 0, &SETTINGS))
            .collect();
        assert_eq!(due[0].phase, Phase::Relearning);
        assert_eq!(due[0].due_ms - now, 10 * MINUTE);
        assert_eq!(due[0].lapses, 1);
        assert!(due[0].stability <= m.stability, "forgetting never helps");
        assert!(due[0].difficulty > m.difficulty);
        let days: Vec<i64> = due[1..].iter().map(|d| days_until(d, now)).collect();
        assert!(
            days[0] >= 1 && days[0] < days[1] && days[1] < days[2],
            "{days:?}"
        );
        assert!(due[3].difficulty < m.difficulty);
    }

    // T-REV-003
    #[test]
    fn retention_and_the_longest_interval_are_kept() {
        let mut m = answer(&Memory::default(), Grade::Easy, NOW, 0, &SETTINGS);
        let strict = Settings {
            retention: 0.97,
            ..SETTINGS
        };
        let loose = Settings {
            retention: 0.8,
            ..SETTINGS
        };
        let now = m.due_ms;
        let a = answer(&m, Grade::Good, now, 0, &strict);
        let b = answer(&m, Grade::Good, now, 0, &loose);
        assert!(days_until(&a, now) < days_until(&b, now));
        let capped = Settings {
            max_days: 30,
            ..SETTINGS
        };
        for _ in 0..10 {
            let now = m.due_ms;
            m = answer(&m, Grade::Easy, now, 0, &capped);
            assert!(days_until(&m, now) <= 30);
        }
    }

    // T-REV-003
    #[test]
    fn the_day_turns_at_four_local_time() {
        // 03:59 and 04:00 in UTC+2
        let offset = 7_200;
        let at = |h: i64, m: i64| (20_000 * 86_400 + h * 3600 + m * 60 - 7_200) * 1000;
        assert_eq!(day_of(at(3, 59), offset), 19_999);
        assert_eq!(day_of(at(4, 0), offset), 20_000);
        assert_eq!(day_start(20_000, offset), at(4, 0));
    }

    // T-REV-002, T-REV-005
    #[test]
    fn cards_are_kept_overwritten_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert_eq!(
            lib.review_queue(NOW, 0, &SETTINGS).unwrap(),
            Queue::default()
        );
        let a = lib.add_review_card(&card("食べる", NOW)).unwrap();
        let b = lib.add_review_card(&card("飲む", NOW)).unwrap();
        assert_ne!(a, b, "same moment, another id");
        assert!(a & ID_BIT != 0 && b & ID_BIT != 0);
        assert_eq!(lib.review_card(a).unwrap(), Some(card("食べる", NOW)));
        assert_eq!(
            lib.review_has(&[
                ("食べる".into(), "たべる".into()),
                ("食べる".into(), "くう".into()),
            ])
            .unwrap(),
            vec![true, false]
        );
        // an overwrite keeps the schedule
        lib.answer_review_card(a, Grade::Good, NOW, 0, &SETTINGS)
            .unwrap();
        let mut other = card("食べる", NOW + 1);
        other.sentence = "また食べる".into();
        lib.replace_review_card(a, &other).unwrap();
        let kept = lib.review_card(a).unwrap().unwrap();
        assert_eq!(kept.sentence, "また食べる");
        assert_eq!(kept.memory.reps, 1);
        // deleting takes the mined record too
        lib.put_mined_card(
            a,
            &crate::MinedCard {
                expression: "食べる".into(),
                reading: "たべる".into(),
                sentence: String::new(),
                video_uid: String::new(),
                start_ms: 0,
                end_ms: 0,
                source_id: String::new(),
                created_at: NOW,
            },
        )
        .unwrap();
        assert_eq!(lib.review_files().unwrap().len(), 2);
        assert!(lib.delete_review_card(a).unwrap().is_some());
        assert_eq!(lib.mined_card(a).unwrap(), None);
        assert_eq!(lib.review_card(a).unwrap(), None);
        assert_eq!(
            lib.review_files().unwrap().len(),
            2,
            "b keeps the same files"
        );
        assert_eq!(lib.schema(), crate::db::SCHEMA_VERSION, "no migration");
    }

    // T-REV-009: the cards listed newest first; one starts over as new, and
    // the sync takes the reset
    #[test]
    fn a_card_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let old = lib.add_review_card(&card("古い", NOW)).unwrap();
        let new = lib.add_review_card(&card("新しい", NOW + 5)).unwrap();
        let listed: Vec<u64> = lib
            .review_cards()
            .unwrap()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(listed, [new, old]);
        lib.answer_review_card(old, Grade::Easy, NOW, 0, &SETTINGS)
            .unwrap();
        assert!(lib.reset_review_card(old, NOW + 60_000).unwrap());
        let memory = lib.review_card(old).unwrap().unwrap().memory;
        assert_eq!(
            (memory.phase, memory.reps, memory.first_ms),
            (Phase::New, 0, 0)
        );
        assert_eq!(lib.review_queue(NOW + 60_000, 0, &SETTINGS).unwrap().new, 2);
        assert_eq!(lib.review_log().unwrap().len(), 1, "its answer stays");
        assert!(!lib.reset_review_card(12345, NOW).unwrap());
    }

    // T-REV-004
    #[test]
    fn the_queue_puts_learning_first_and_counts_new_cards_by_day() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let settings = Settings {
            new_per_day: 2,
            ..SETTINGS
        };
        let ids: Vec<u64> = (0..4)
            .map(|i| {
                lib.add_review_card(&card(&format!("語{i}"), NOW + i))
                    .unwrap()
            })
            .collect();
        let q = lib.review_queue(NOW, 0, &settings).unwrap();
        assert_eq!((q.new, q.learning, q.review, q.total), (2, 0, 0, 4));
        assert_eq!(
            q.cards.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            ids[..2]
        );
        // the first is learnt, the second forgotten: one minute later the
        // forgotten one is due, and only one new card is left for today
        lib.answer_review_card(ids[0], Grade::Easy, NOW, 0, &settings)
            .unwrap();
        lib.answer_review_card(ids[1], Grade::Again, NOW, 0, &settings)
            .unwrap();
        let q = lib.review_queue(NOW + 30_000, 0, &settings).unwrap();
        assert_eq!(
            (q.new, q.learning, q.review),
            (0, 1, 0),
            "early: learn ahead"
        );
        let q = lib.review_queue(NOW + 2 * MINUTE, 0, &settings).unwrap();
        assert_eq!((q.new, q.learning, q.review), (0, 1, 0));
        assert_eq!(q.cards[0].0, ids[1]);
        let tomorrow = day_start(day_of(NOW, 0) + 1, 0);
        lib.answer_review_card(ids[1], Grade::Good, NOW + 2 * MINUTE, 0, &settings)
            .unwrap();
        let q = lib.review_queue(NOW + 3 * MINUTE, 0, &settings).unwrap();
        assert!(q.cards.is_empty());
        assert_eq!(q.next_due_ms, Some(tomorrow));
        // a new day: two new cards again, and the review due first
        let q = lib.review_queue(tomorrow, 0, &settings).unwrap();
        assert_eq!((q.new, q.learning, q.review), (2, 0, 1));
        assert_eq!(q.cards[0].0, ids[1]);
        // the undo puts the memory back
        let before = lib.review_card(ids[2]).unwrap().unwrap().memory;
        lib.answer_review_card(ids[2], Grade::Good, tomorrow, 0, &settings)
            .unwrap();
        lib.restore_review_memory(ids[2], &before).unwrap();
        assert_eq!(lib.review_card(ids[2]).unwrap().unwrap().memory, before);
    }
}
