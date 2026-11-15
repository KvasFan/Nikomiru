// SPDX-License-Identifier: GPL-3.0-only

//! Frozen finite vocabulary recall, independent of playback and global statuses (KNW-053).

use crate::words::ListedWord;
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Card {
    pub expression: String,
    pub reading: String,
    pub count: usize,
}
impl Card {
    fn valid(&self) -> bool {
        !self.expression.trim().is_empty()
            && self.expression.len() <= 4096
            && self.reading.len() <= 4096
            && !self
                .expression
                .chars()
                .chain(self.reading.chars())
                .any(char::is_control)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub(crate) struct Recall {
    pub source: String,
    pub cards: Vec<Card>,
    pub order: Vec<usize>,
    pub pending: VecDeque<usize>,
    pub revealed: bool,
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
        for c in &cards {
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
            source: source.chars().take(256).collect(),
            cards,
            pending: order.iter().copied().collect(),
            order,
            revealed: false,
            outcomes,
            history: Vec::new(),
        })
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
