// SPDX-License-Identifier: GPL-3.0-only

//! Finite cloze rounds, frozen distractors and explicit asynchronous schedule jobs.
use nikomiru_library::{
    cloze::Exercise,
    phrases::Phrase,
    review::{Grade, Memory, Settings},
};
use nikomiru_text::cloze::answer;
/// Maximum exercises in one explicit round.
pub const ROUND_LIMIT: usize = 100;
/// Explicit practice input mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// An exact typed answer.
    Typed,
    /// One of 2–4 distinct same-language answers.
    Choice,
}
/// A finite result, independent of phrase and word statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultKind {
    /// Exact answer.
    Correct,
    /// Different answer.
    Incorrect,
    /// Explicitly skipped without a schedule write.
    Skipped,
}
/// The immediately previous completed step, including scheduled compare/restore memory if applicable.
#[derive(Debug, Clone)]
pub struct Undo {
    /// Previous round index.
    pub index: usize,
    /// Memory before/after a committed rating; None for finite/skip.
    pub memory: Option<(Memory, Memory)>,
}
/// A bounded frozen round; changing the notebook cannot rewrite its prompts.
#[derive(Debug)]
pub struct Round {
    /// Snapshots selected at the start.
    pub rows: Vec<Exercise>,
    /// Frozen language/answer pool for distractors.
    pool: Vec<(String, String, String)>,
    /// Current zero-based position.
    pub index: usize,
    /// Input mode.
    pub mode: Mode,
    /// Scheduled mode was explicitly chosen.
    pub scheduled: bool,
    /// Editable answer, not saved to the library.
    pub input: String,
    /// Feedback is absent until explicit submission.
    pub feedback: Option<bool>,
    /// Completed outcomes, in order.
    pub results: Vec<ResultKind>,
    /// At most one undo opportunity.
    pub undo: Option<Undo>,
}
impl Round {
    /// Freeze a valid nonempty selection; no repeat loop or implicit schedule enrolment.
    pub fn new(
        rows: Vec<Exercise>,
        pool: &[Exercise],
        mode: Mode,
        scheduled: bool,
    ) -> Option<Self> {
        if rows.is_empty()
            || rows.len() > ROUND_LIMIT
            || rows
                .iter()
                .any(|r| !r.valid() || (scheduled && r.memory.is_none()))
        {
            return None;
        }
        let mut seen = std::collections::HashSet::new();
        let pool = if mode == Mode::Choice {
            pool.iter()
                .filter_map(|r| {
                    let key = answer(&r.phrase.text);
                    seen.insert((r.phrase.language.clone(), key.clone()))
                        .then(|| (r.phrase.language.clone(), r.phrase.text.clone(), key))
                })
                .collect()
        } else {
            Vec::new()
        };
        let round = Self {
            rows,
            pool,
            index: 0,
            mode,
            scheduled,
            input: String::new(),
            feedback: None,
            results: Vec::new(),
            undo: None,
        };
        if mode == Mode::Choice && (0..round.rows.len()).any(|i| round.options_at(i).len() < 2) {
            return None;
        }
        Some(round)
    }
    /// The unanswered or revealed current exercise, absent at the finite summary.
    pub fn current(&self) -> Option<&Exercise> {
        self.rows.get(self.index)
    }
    fn options_at(&self, index: usize) -> Vec<String> {
        let Some(current) = self.rows.get(index) else {
            return Vec::new();
        };
        let mut options = vec![current.phrase.text.clone()];
        let correct_key = answer(&current.phrase.text);
        for (lang, text, key) in &self.pool {
            if *lang == current.phrase.language && *key != correct_key {
                options.push(text.clone());
                if options.len() == 4 {
                    break;
                }
            }
        }
        let rotate = current
            .phrase
            .id
            .bytes()
            .fold(0usize, |n, b| n.wrapping_mul(31).wrapping_add(b as usize))
            % options.len();
        options.rotate_left(rotate);
        options
    }
    /// Concealed answer choices, without a correctness marker.
    pub fn options(&self) -> Vec<String> {
        if self.mode == Mode::Choice {
            self.options_at(self.index)
        } else {
            Vec::new()
        }
    }
    /// Explicit submit only; typing does not reveal or advance.
    pub fn submit(&mut self) {
        if self.feedback.is_some() || self.input.len() > 4096 || self.input.trim().is_empty() {
            return;
        }
        if self.mode == Mode::Choice && !self.options().contains(&self.input) {
            return;
        }
        self.feedback = self.current().map(|r| r.accepts(&self.input));
    }
    /// Complete one step after feedback or explicit skip, retaining just its undo snapshot.
    pub fn advance(&mut self, skip: bool, memory: Option<(Memory, Memory)>) {
        if self.current().is_none() || (!skip && self.feedback.is_none()) {
            return;
        }
        let outcome = if skip {
            ResultKind::Skipped
        } else if self.feedback == Some(true) {
            ResultKind::Correct
        } else {
            ResultKind::Incorrect
        };
        self.results.push(outcome);
        self.undo = Some(Undo {
            index: self.index,
            memory,
        });
        self.index += 1;
        self.input.clear();
        self.feedback = None;
    }
    /// Restore a finite step or a successfully committed scheduled undo; never changes the database itself.
    pub fn restore(&mut self) {
        if let Some(undo) = self.undo.take() {
            self.index = undo.index;
            self.results.pop();
            self.input.clear();
            self.feedback = None;
        }
    }
}
/// Serial database request.
#[derive(Debug, Clone)]
pub enum Job {
    /// Read existing exercises.
    List,
    /// Explicit preview confirmation.
    Create(Box<Phrase>),
    /// Schedule enrolment.
    Enrol(Box<Exercise>),
    /// Explicit revealed rating.
    Rate {
        /// Frozen expected row.
        row: Box<Exercise>,
        /// Chosen FSRS rating.
        grade: Grade,
        /// Wall clock.
        now_ms: i64,
        /// Local offset.
        offset_s: i32,
        /// Existing review settings.
        settings: Settings,
    },
    /// One-step memory restore.
    Undo {
        /// Stable exercise id.
        id: String,
        /// Memory expected after the last rating.
        after: Memory,
        /// Memory to restore.
        before: Memory,
    },
    /// Explicit deletion of exercise plus schedule.
    Delete(String),
}
/// A completed operation's visible effect.
#[derive(Debug)]
pub enum Effect {
    /// Refresh only.
    Loaded,
    /// Clear the preview after creation.
    Created,
    /// Explicit rating committed.
    Rated {
        /// Exercise id.
        id: String,
        /// Previous memory.
        before: Memory,
        /// Updated memory.
        after: Memory,
    },
    /// Schedule undo committed.
    Undone(String),
}
/// Updated full snapshot and effect; errors retain drafts/answers.
pub type Answer = Result<(Vec<Exercise>, Effect), String>;
/// Notebook/practice state retained by the session.
#[derive(Debug, Default)]
pub struct State {
    /// The cloze page covers all sources and popups while open.
    pub open: bool,
    /// Last committed exercises.
    pub items: Vec<Exercise>,
    /// Explicit creation preview.
    pub preview: Option<Phrase>,
    /// Current frozen finite round.
    pub round: Option<Round>,
    /// Work running.
    pub busy: bool,
    /// Last operation failure.
    pub error: Option<String>,
    /// Languages offering at least two distinct canonical answers.
    pub choice_languages: std::collections::HashSet<String>,
    /// Deferred ordered work.
    pub(crate) job: Option<Job>,
}
/// Languages with at least two distinct canonical answers, calculated only after a database snapshot.
pub fn choice_languages(rows: &[Exercise]) -> std::collections::HashSet<String> {
    let mut languages = std::collections::HashMap::<&str, std::collections::HashSet<String>>::new();
    for row in rows {
        let entries = languages.entry(&row.phrase.language).or_default();
        if entries.len() < 2 {
            entries.insert(answer(&row.phrase.text));
        }
    }
    languages
        .into_iter()
        .filter_map(|(lang, keys)| (keys.len() > 1).then(|| lang.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exercise(text: &str, language: &str, n: usize) -> Exercise {
        Exercise {
            phrase: Phrase {
                id: format!("00000000-0000-4000-8000-{n:012}"),
                language: language.into(),
                text: text.into(),
                definition: String::new(),
                context: format!("{text}！"),
                status: crate::words::WordStatus::Unknown,
                source: None,
                created_ms: 1,
                changed_ms: 1,
            },
            memory: None,
        }
    }
    // T-CLO-001, T-CLO-002, T-CLO-003: distinct same-language options and a finite explicit answer/advance/skip/undo sequence.
    #[test]
    fn cloze_round_is_finite_and_distractors_respect_answer_equivalence() {
        let rows = vec![
            exercise("café", "fr", 1),
            exercise("cafe\u{301}", "fr", 2),
            exercise("chien", "fr", 3),
            exercise("猫", "ja", 4),
        ];
        let mut r = Round::new(
            vec![rows[0].clone(), rows[2].clone()],
            &rows,
            Mode::Choice,
            false,
        )
        .unwrap();
        assert_eq!(r.options().len(), 2);
        assert!(!r.options().contains(&"猫".into()));
        assert!(r.feedback.is_none());
        r.input = "cafe\u{301}".into();
        r.submit();
        assert!(r.feedback.is_none(), "choice requires one displayed option");
        r.input = "café".into();
        r.submit();
        assert_eq!(r.feedback, Some(true));
        assert_eq!(r.index, 0);
        r.advance(false, None);
        r.advance(true, None);
        assert!(r.current().is_none());
        assert_eq!(r.results, vec![ResultKind::Correct, ResultKind::Skipped]);
        r.restore();
        assert_eq!(r.index, 1);
        r.restore();
        assert_eq!(r.index, 1);
        assert!(Round::new(vec![rows[3].clone()], &rows, Mode::Choice, false).is_none());
    }
}
