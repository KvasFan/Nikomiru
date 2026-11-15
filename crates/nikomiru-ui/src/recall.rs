// SPDX-License-Identifier: GPL-3.0-only

//! Frozen finite vocabulary recall, independent of playback and global statuses (KNW-053).

use crate::words::ListedWord;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

fn valid_hint(text: &str) -> bool {
    !text.trim().is_empty()
        && text.len() <= 4096
        && !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

pub(crate) fn sentence_hints(
    words: &crate::words::Words,
    cues: &[nikomiru_subs::Cue],
    cards: &[Card],
) -> Result<Vec<Vec<String>>, &'static str> {
    let ids = cards
        .iter()
        .map(|c| (c.expression.as_str(), c.reading.as_str()))
        .collect::<Vec<_>>();
    let hits = crate::word_export::concordance_indices(words, cues, &ids)?;
    let mut bytes = 0usize;
    let mut out = Vec::with_capacity(cards.len());
    for indices in hits {
        let mut seen = HashSet::new();
        let mut hints = Vec::new();
        for i in indices {
            let text = &cues[i].plain_text;
            if valid_hint(text) && seen.insert(text.as_str()) {
                bytes = bytes.saturating_add(text.len());
                if bytes > 4 << 20 {
                    return Err("recall-hints-unavailable");
                }
                hints.push(text.clone());
                if hints.len() == 3 {
                    break;
                }
            }
        }
        out.push(hints);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Card {
    pub expression: String,
    pub reading: String,
    pub count: usize,
    #[serde(default, deserialize_with = "crate::recall_file::hints")]
    pub hints: Vec<String>,
}
impl Card {
    fn valid(&self) -> bool {
        !self.expression.trim().is_empty()
            && self.expression.len() <= 4096
            && self.reading.len() <= 4096
            && self.hints.len() <= 3
            && self.hints.iter().all(|s| valid_hint(s))
            && !self
                .expression
                .chars()
                .chain(self.reading.chars())
                .any(char::is_control)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Grade {
    Again,
    Good,
    Skipped,
}
#[derive(Debug, Default, Clone)]
pub(crate) struct Outcome {
    pub attempts: u8,
    pub again: u8,
    pub last: Option<Grade>,
}
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Summary {
    pub good: usize,
    pub again: usize,
    pub skipped: usize,
    pub attempts: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Order {
    Original,
    Reverse,
    Shuffle,
}

#[derive(Debug, Clone)]
pub(crate) struct Recall {
    pub id: String,
    pub source: String,
    pub cards: Vec<Card>,
    pub order: Vec<usize>,
    pub pending: VecDeque<usize>,
    pub revealed: bool,
    pub hint_revealed: bool,
    pub hint_source: Option<String>,
    pub outcomes: Vec<Outcome>,
    pub history: Vec<(usize, Grade)>,
}
impl Recall {
    pub fn start(source: &str, rows: &[ListedWord]) -> Result<Self, &'static str> {
        if rows.is_empty() || rows.len() > 1000 {
            return Err("recall-unavailable");
        }
        if rows
            .iter()
            .any(|w| w.expression.len() > 4096 || w.reading.len() > 4096)
        {
            return Err("recall-unavailable");
        }
        let cards = rows
            .iter()
            .map(|w| Card {
                expression: w.expression.clone(),
                reading: w.reading.clone(),
                count: w.count,
                hints: Vec::new(),
            })
            .collect();
        Self::from_cards(source, cards)
    }
    pub fn from_cards(source: &str, cards: Vec<Card>) -> Result<Self, &'static str> {
        if cards.is_empty() || cards.len() > 1000 {
            return Err("recall-unavailable");
        }
        let mut seen = HashSet::new();
        let mut bytes = 0usize;
        let mut hint_bytes = 0usize;
        for c in &cards {
            for hint in &c.hints {
                hint_bytes = hint_bytes.saturating_add(hint.len());
            }
            if hint_bytes > 4 << 20 {
                return Err("recall-hints-unavailable");
            }
            bytes = bytes
                .saturating_add(c.expression.len())
                .saturating_add(c.reading.len());
            if !c.valid() || bytes > 8 << 20 || !seen.insert((&c.expression, &c.reading)) {
                return Err("recall-unavailable");
            }
        }
        let order = (0..cards.len()).collect::<Vec<_>>();
        let outcomes = vec![Outcome::default(); cards.len()];
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            source: source
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect(),
            cards,
            pending: order.iter().copied().collect(),
            order,
            revealed: false,
            hint_revealed: false,
            hint_source: None,
            outcomes,
            history: Vec::new(),
        })
    }
    pub fn restart(&mut self, order: Order, seed: u64) {
        self.order = (0..self.cards.len()).collect();
        match order {
            Order::Original => {}
            Order::Reverse => self.order.reverse(),
            Order::Shuffle => {
                let mut state = seed.max(1);
                for i in (1..self.order.len()).rev() {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let j = (state % ((i + 1) as u64)) as usize;
                    self.order.swap(i, j);
                }
            }
        }
        self.id = uuid::Uuid::new_v4().to_string();
        self.pending = self.order.iter().copied().collect();
        self.outcomes.fill(Outcome::default());
        self.history.clear();
        self.revealed = false;
        self.hint_revealed = false;
    }

    pub fn current(&self) -> Option<&Card> {
        self.pending.front().and_then(|i| self.cards.get(*i))
    }
    pub fn reveal(&mut self) {
        if self.current().is_some() {
            self.revealed = true;
        }
    }
    pub fn skip(&mut self) {
        self.finish(Grade::Skipped);
    }
    pub fn rate(&mut self, good: bool) -> bool {
        self.finish(if good { Grade::Good } else { Grade::Again })
    }
    fn finish(&mut self, grade: Grade) -> bool {
        if (grade != Grade::Skipped && !self.revealed) || self.history.len() >= self.cards.len() * 3
        {
            return false;
        }
        let Some(i) = self.pending.pop_front() else {
            return false;
        };
        let result = &mut self.outcomes[i];
        result.last = Some(grade);
        if grade != Grade::Skipped {
            result.attempts += 1;
        }
        if grade == Grade::Again {
            result.again += 1;
            if result.attempts < 3 {
                self.pending.push_back(i);
            }
        }
        self.history.push((i, grade));
        self.revealed = false;
        self.hint_revealed = false;
        true
    }
    pub fn summary(&self) -> Summary {
        Summary {
            good: self
                .outcomes
                .iter()
                .filter(|r| r.last == Some(Grade::Good))
                .count(),
            again: self.outcomes.iter().map(|r| usize::from(r.again)).sum(),
            skipped: self
                .outcomes
                .iter()
                .filter(|r| r.last == Some(Grade::Skipped))
                .count(),
            attempts: self.outcomes.iter().map(|r| usize::from(r.attempts)).sum(),
        }
    }
    pub fn remaining(&self) -> usize {
        self.pending.len()
    }
    pub fn done(&self) -> bool {
        self.pending.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;
    fn rows() -> Vec<ListedWord> {
        vec![
            ListedWord {
                expression: "猫".into(),
                reading: "ねこ".into(),
                count: 2,
                status: WordStatus::Unknown,
                card: false,
            },
            ListedWord {
                expression: "犬".into(),
                reading: "いぬ".into(),
                count: 1,
                status: WordStatus::Known,
                card: true,
            },
        ]
    }
    // T-KNW-058: restarts preserve frozen cards/hints, reset progress and shuffle each initial identity exactly once.
    #[test]
    fn recall_restarts_are_seeded_permutations_without_losing_hints() {
        let mut r = Recall::start("Study", &rows()).unwrap();
        r.cards[0].hints = vec!["猫がいる".into()];
        let cards = r.cards.clone();
        let id = r.id.clone();
        r.reveal();
        r.rate(false);
        r.restart(Order::Reverse, 0);
        assert_eq!(r.order, [1, 0]);
        assert!(r.history.is_empty());
        assert!(!r.revealed);
        assert_ne!(r.id, id);
        assert_eq!(r.cards, cards);
        r.restart(Order::Shuffle, 42);
        let order = r.order.clone();
        r.restart(Order::Shuffle, 42);
        assert_eq!(r.order, order);
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1]);
        r.restart(Order::Original, 0);
        assert_eq!(r.order, [0, 1]);
        assert_eq!(r.summary().attempts, 0);
    }

    // T-KNW-054: ratings require an answer, Again terminates after three attempts and skipped cards stay distinct.
    #[test]
    fn self_rated_recall_is_finite_and_does_not_grade_hidden_answers() {
        let mut r = Recall::start("Study", &rows()).unwrap();
        assert!(!r.rate(true));
        assert_eq!(r.remaining(), 2);
        r.reveal();
        assert!(r.rate(false));
        assert_eq!(r.current().unwrap().expression, "犬");
        r.skip();
        for _ in 0..2 {
            r.reveal();
            assert!(r.rate(false));
        }
        assert!(r.done());
        assert_eq!(r.history.len(), 4);
        let summary = r.summary();
        assert_eq!(
            (
                summary.good,
                summary.again,
                summary.skipped,
                summary.attempts
            ),
            (0, 3, 1, 3)
        );
        assert!(!r.rate(true));
        let mut r = Recall::start("Study", &rows()).unwrap();
        r.reveal();
        r.rate(true);
        r.reveal();
        r.rate(true);
        assert!(r.done());
        assert_eq!(r.summary().good, 2);
    }

    // T-KNW-053: finite frozen identities preserve order, reset answers and reject incomplete/oversized/duplicate inputs.
    #[test]
    fn finite_recall_capture_is_bounded_and_independent_of_source_mutation() {
        let mut source = rows();
        let mut r = Recall::start("Study", &source).unwrap();
        source[0].expression = "changed".into();
        assert_eq!(r.current().unwrap().expression, "猫");
        assert!(!r.revealed);
        r.reveal();
        assert!(r.revealed);
        r.skip();
        assert_eq!(r.current().unwrap().expression, "犬");
        assert!(!r.revealed);
        r.skip();
        assert!(r.done());
        r.skip();
        assert!(r.done());
        assert!(Recall::start("", &[]).is_err());
        assert!(Recall::start("", &vec![source[0].clone(); 1001]).is_err());
        assert!(Recall::start("", &vec![source[0].clone(); 2]).is_err());
        source[0].reading = "x".repeat(4097);
        assert!(Recall::start("", &source).is_err());
    }
}
