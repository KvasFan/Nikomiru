// SPDX-License-Identifier: GPL-3.0-only

//! The words of the subtitles (LKP-060) and what the overlay marks on them:
//! readings over their kanji (LKP-051), the words already made into cards
//! (LKP-061), and the words by what the learner knows of them, with the
//! lines that have one new word (KNW-002, KNW-003), and how much of the
//! subtitles the learner understands (KNW-007). Each text is split once,
//! away from the UI thread; the cues about to come are asked ahead, so that
//! their marks are ready when they show.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use nikomiru_dict::segment::Word;
pub use nikomiru_library::words::WordStatus;
use nikomiru_library::words::word_key;
use nikomiru_text::furigana::distribute_inflected;
use nikomiru_text::kana::is_kanji;

/// Texts kept split; beyond, the cache starts again (an episode has a few
/// hundred cues).
pub(crate) const KEPT: usize = 4096;

/// What the overlay draws over a cue, in bytes of its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// Readings over kanji (LKP-051).
    pub readings: Vec<(Range<usize>, String)>,
    /// Words already made into cards (LKP-061), while the words are not
    /// coloured by status.
    pub mined: Vec<Range<usize>>,
    /// Words not known yet (KNW-002).
    pub unknown: Vec<Range<usize>>,
    /// Words being learned (KNW-002).
    pub learning: Vec<Range<usize>>,
    /// The line has exactly one word not known (KNW-003).
    pub plus_one: bool,
}

impl Marks {
    /// Nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
            && self.mined.is_empty()
            && self.unknown.is_empty()
            && self.learning.is_empty()
            && !self.plus_one
    }
}

/// Why a word is underlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    /// A card was made of it (LKP-061).
    Mined,
    /// Not known yet (KNW-002).
    Unknown,
    /// Being learned (KNW-002).
    Learning,
}

impl Marks {
    /// The words underlined, each with why.
    pub fn underlines(&self) -> impl Iterator<Item = (&Range<usize>, Underline)> {
        fn tag(
            list: &[Range<usize>],
            u: Underline,
        ) -> impl Iterator<Item = (&Range<usize>, Underline)> {
            list.iter().map(move |r| (r, u))
        }
        tag(&self.mined, Underline::Mined)
            .chain(tag(&self.unknown, Underline::Unknown))
            .chain(tag(&self.learning, Underline::Learning))
    }
}

/// How much of a set of subtitles the learner understands (KNW-007).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    /// The words met, each time it is met: those a dictionary knows, not
    /// ignored.
    pub words: usize,
    /// Of them, the known ones.
    pub known: usize,
    /// The lines with at least one such word.
    pub lines: usize,
    /// Of them, the lines with no word not known.
    pub clear: usize,
}

impl Score {
    /// The share of the words known, in whole percent (rounded down, so
    /// that 100 means every one).
    pub fn known_percent(&self) -> Option<usize> {
        (self.words > 0).then(|| self.known * 100 / self.words)
    }

    /// The share of the lines with no word not known, in whole percent.
    pub fn clear_percent(&self) -> Option<usize> {
        (self.lines > 0).then(|| self.clear * 100 / self.lines)
    }
}

/// What to mark over a text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wanted {
    /// Readings over kanji (LKP-051).
    pub furigana: bool,
    /// Only over the words not known (KNW-004).
    pub furigana_unknown_only: bool,
    /// The words of the cards made (LKP-061).
    pub mined: bool,
    /// The words by status, and the i+1 line (KNW-002, KNW-003).
    pub colors: bool,
}

impl Wanted {
    /// Whether anything is marked, so the words must be split.
    pub fn any(&self) -> bool {
        self.furigana || self.mined || self.colors
    }
}

/// The texts split, asked and to ask, and the headwords of the cards made.
#[derive(Debug, Default)]
pub struct Words {
    split: HashMap<String, Arc<Vec<Word>>>,
    asked: HashSet<String>,
    queue: Vec<String>,
    /// Changes with the dictionaries: older answers are dropped.
    generation: u64,
    mined: HashSet<String>,
    /// The statuses set, by headword and reading (KNW-001).
    statuses: HashMap<String, WordStatus>,
    /// What the cards of Anki's decks say, by headword (KNW-005).
    anki: HashMap<String, WordStatus>,
    /// Changes whenever a text is split or a status may have changed, so
    /// that a score is counted again (KNW-007).
    revision: u64,
}

impl Words {
    /// Asks for the words of `text`, unless they are known or asked.
    pub fn ask(&mut self, text: &str) {
        if text.trim().is_empty() || self.split.contains_key(text) || self.asked.contains(text) {
            return;
        }
        self.asked.insert(text.to_string());
        self.queue.push(text.to_string());
    }

    /// The texts to split, with the generation their answer must carry.
    pub fn take_queue(&mut self) -> Vec<(u64, String)> {
        let generation = self.generation;
        std::mem::take(&mut self.queue)
            .into_iter()
            .map(|t| (generation, t))
            .collect()
    }

    /// The answer for `text`. A failure counts as no words: the text is not
    /// asked again until the dictionaries change.
    pub fn done(&mut self, generation: u64, text: String, result: Result<Vec<Word>, String>) {
        if generation != self.generation {
            return;
        }
        self.asked.remove(&text);
        let words = result.unwrap_or_else(|e| {
            tracing::warn!("splitting a subtitle into words: {e}");
            Vec::new()
        });
        if self.split.len() >= KEPT {
            self.split.clear();
        }
        self.split.insert(text, Arc::new(words));
        self.revision += 1;
    }

    /// Changes whenever a score may change (KNW-007).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// No text is being split nor waits to be.
    pub fn idle(&self) -> bool {
        self.asked.is_empty() && self.queue.is_empty()
    }

    /// Whether the words of `text` are known.
    pub fn has(&self, text: &str) -> bool {
        text.trim().is_empty() || self.split.contains_key(text)
    }

    /// How much of `texts`, the lines of a subtitle, the learner
    /// understands (KNW-007); while some are not split yet, how many are.
    /// A line counts each time it comes; ignored words count for nothing.
    pub fn score<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> Result<Score, usize> {
        let mut score = Score::default();
        let (mut split, mut missing) = (0, false);
        for text in texts {
            if text.trim().is_empty() {
                continue;
            }
            let Some(words) = self.get(text) else {
                missing = true;
                continue;
            };
            split += 1;
            let (mut counted, mut clear) = (false, true);
            for h in words.iter().filter_map(|w| w.headword.as_ref()) {
                match self.status(&h.expression, &h.reading) {
                    WordStatus::Ignored => continue,
                    WordStatus::Known => score.known += 1,
                    WordStatus::Learning => {}
                    WordStatus::Unknown => clear = false,
                }
                score.words += 1;
                counted = true;
            }
            if counted {
                score.lines += 1;
                score.clear += usize::from(clear);
            }
        }
        if missing { Err(split) } else { Ok(score) }
    }

    /// The dictionaries changed: every text is split again.
    pub fn forget(&mut self) {
        self.revision += 1;
        self.generation += 1;
        self.split.clear();
        self.asked.clear();
        self.queue.clear();
    }

    /// The words of `text`, once split.
    pub fn get(&self, text: &str) -> Option<&[Word]> {
        self.split.get(text).map(|w| w.as_slice())
    }

    /// The headwords of the cards made (from the library).
    pub fn set_mined(&mut self, expressions: HashSet<String>) {
        self.revision += 1;
        self.mined = expressions;
    }

    /// A card was made of `expression`.
    pub fn add_mined(&mut self, expression: String) {
        self.revision += 1;
        self.mined.insert(expression);
    }

    /// The statuses set (from the library).
    pub fn set_statuses(&mut self, statuses: HashMap<String, WordStatus>) {
        self.revision += 1;
        self.statuses = statuses;
    }

    /// The learner set the status of a word.
    pub fn set_status(&mut self, expression: &str, reading: &str, status: WordStatus) {
        self.revision += 1;
        self.statuses.insert(word_key(expression, reading), status);
    }

    /// The words read from Anki (KNW-005).
    pub fn set_anki(&mut self, words: HashMap<String, WordStatus>) {
        self.revision += 1;
        self.anki = words;
    }

    /// What the learner knows of a word: the status set for it, else the
    /// one set for its headword with any reading (a list, KNW-006), else
    /// what Anki's cards say (KNW-005), else learning when a card was made
    /// of it, else unknown (KNW-001).
    pub fn status(&self, expression: &str, reading: &str) -> WordStatus {
        let set = |reading: &str| self.statuses.get(&word_key(expression, reading)).copied();
        set(reading)
            .or_else(|| set(""))
            .or_else(|| self.anki.get(expression).copied())
            .unwrap_or(if self.mined.contains(expression) {
                WordStatus::Learning
            } else {
                WordStatus::Unknown
            })
    }

    /// What to draw over `text`. `None` while its words are not known.
    pub fn marks(&self, text: &str, wanted: &Wanted) -> Option<Marks> {
        let words = self.get(text)?;
        let mut marks = Marks::default();
        let mut new_words: HashSet<(&str, &str)> = HashSet::new();
        for w in words {
            let Some(h) = &w.headword else {
                continue;
            };
            let Some(surface) = text.get(w.range.clone()) else {
                continue;
            };
            let status = self.status(&h.expression, &h.reading);
            if wanted.colors {
                match status {
                    WordStatus::Unknown => {
                        marks.unknown.push(w.range.clone());
                        new_words.insert((&h.expression, &h.reading));
                    }
                    WordStatus::Learning => marks.learning.push(w.range.clone()),
                    WordStatus::Known | WordStatus::Ignored => {}
                }
            } else if wanted.mined && self.mined.contains(&h.expression) {
                marks.mined.push(w.range.clone());
            }
            let known = matches!(status, WordStatus::Known | WordStatus::Ignored);
            let reading = wanted.furigana && !(wanted.furigana_unknown_only && known);
            if reading && surface.chars().any(is_kanji) {
                let mut at = w.range.start;
                for s in distribute_inflected(&h.expression, &h.reading, surface) {
                    let end = at + s.text.len();
                    // kana under a katakana headword's reading needs none
                    if !s.reading.is_empty() && s.text.chars().any(is_kanji) {
                        marks.readings.push((at..end, s.reading));
                    }
                    at = end;
                }
            }
        }
        marks.plus_one = wanted.colors && new_words.len() == 1;
        Some(marks)
    }

    /// The start of the word after (or before) the one at byte `offset` of
    /// `text`, skipping text no dictionary knows; `None` at the ends or
    /// while its words are not known (LKP-060).
    pub fn step(&self, text: &str, offset: usize, forward: bool) -> Option<usize> {
        let starts: Vec<usize> = self
            .get(text)?
            .iter()
            .filter(|w| w.headword.is_some())
            .map(|w| w.range.start)
            .collect();
        if forward {
            starts.into_iter().find(|&s| s > offset)
        } else {
            starts.into_iter().rev().find(|&s| s < offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use nikomiru_dict::segment::Headword;

    use super::*;

    fn word(text: &str, range: Range<usize>, headword: Option<(&str, &str)>) -> Word {
        assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        Word {
            range,
            headword: headword.map(|(e, r)| Headword {
                expression: e.into(),
                reading: r.into(),
            }),
        }
    }

    /// `「猫が食べた」`, split as the dictionaries would.
    fn split(words: &mut Words) -> &'static str {
        let text = "「猫が食べた」";
        words.ask(text);
        let queue = words.take_queue();
        assert_eq!(queue, [(0, text.to_string())]);
        words.done(
            0,
            text.into(),
            Ok(vec![
                word(text, 0..3, None),
                word(text, 3..6, Some(("猫", "ねこ"))),
                word(text, 6..9, Some(("が", "が"))),
                word(text, 9..18, Some(("食べる", "たべる"))),
                word(text, 18..21, None),
            ]),
        );
        text
    }

    // T-LKP-051
    #[test]
    fn readings_go_over_the_kanji_of_each_word() {
        let mut words = Words::default();
        let all = Wanted {
            furigana: true,
            mined: true,
            ..Wanted::default()
        };
        assert_eq!(words.marks("「猫が食べた」", &all), None);
        let text = split(&mut words);
        let furigana = Wanted {
            furigana: true,
            ..Wanted::default()
        };
        let marks = words.marks(text, &furigana).unwrap();
        assert_eq!(
            marks.readings,
            [(3..6, "ねこ".to_string()), (9..12, "た".to_string())]
        );
        assert!(marks.mined.is_empty());
        let mined = Wanted {
            mined: true,
            ..Wanted::default()
        };
        assert!(words.marks(text, &mined).unwrap().is_empty());
    }

    // T-LKP-061
    #[test]
    fn the_words_of_the_cards_made_are_marked() {
        let mut words = Words::default();
        let text = split(&mut words);
        words.set_mined(HashSet::from(["猫".to_string()]));
        let wanted = Wanted {
            mined: true,
            ..Wanted::default()
        };
        let mined = words.marks(text, &wanted).unwrap().mined;
        assert_eq!((mined.len(), &mined[0]), (1, &(3..6)));
        // a card just made, of an inflected word's headword
        words.add_mined("食べる".into());
        assert_eq!(words.marks(text, &wanted).unwrap().mined, [3..6, 9..18]);
        assert!(words.marks(text, &Wanted::default()).unwrap().is_empty());
    }

    // T-KNW-002, T-KNW-003: the words by status; a card makes a word
    // learning; one word not known makes the line i+1
    #[test]
    fn the_words_are_marked_by_what_is_known() {
        let mut words = Words::default();
        let text = split(&mut words);
        let colors = Wanted {
            mined: true,
            colors: true,
            ..Wanted::default()
        };
        let marks = words.marks(text, &colors).unwrap();
        assert_eq!(marks.unknown, [3..6, 6..9, 9..18]);
        assert!(!marks.plus_one);
        assert!(
            marks.mined.is_empty(),
            "the colours take the underline's place"
        );
        words.set_status("が", "が", WordStatus::Known);
        words.add_mined("食べる".into());
        assert_eq!(words.status("食べる", "たべる"), WordStatus::Learning);
        let marks = words.marks(text, &colors).unwrap();
        assert_eq!(marks.unknown, std::iter::once(3..6).collect::<Vec<_>>());
        assert_eq!(marks.learning, std::iter::once(9..18).collect::<Vec<_>>());
        assert!(marks.plus_one, "only 猫 is new");
        // known, even with a card; ignored
        words.set_status("食べる", "たべる", WordStatus::Known);
        words.set_status("猫", "ねこ", WordStatus::Ignored);
        let marks = words.marks(text, &colors).unwrap();
        assert!(marks.is_empty(), "{marks:?}");
        // the same headword with another reading is another word
        assert_eq!(words.status("猫", "びょう"), WordStatus::Unknown);
    }

    // T-KNW-005, T-KNW-006: a word of a list known with any reading; Anki's
    // cards below a status set by hand
    #[test]
    fn lists_and_anki_give_statuses_under_the_ones_set() {
        let mut words = Words::default();
        words.set_status("生", "", WordStatus::Known);
        assert_eq!(words.status("生", "なま"), WordStatus::Known);
        words.set_status("生", "なま", WordStatus::Learning);
        assert_eq!(words.status("生", "なま"), WordStatus::Learning);
        assert_eq!(words.status("生", "せい"), WordStatus::Known);
        words.set_anki(HashMap::from([
            ("猫".to_string(), WordStatus::Known),
            ("犬".to_string(), WordStatus::Learning),
        ]));
        assert_eq!(words.status("猫", "ねこ"), WordStatus::Known);
        words.set_status("猫", "ねこ", WordStatus::Unknown);
        assert_eq!(
            words.status("猫", "ねこ"),
            WordStatus::Unknown,
            "set by hand wins"
        );
        assert_eq!(words.status("犬", "いぬ"), WordStatus::Learning);
    }

    // T-KNW-007: the words known and the lines with no new word, counted
    // each time they come; ignored words and lines without words left out
    #[test]
    fn the_score_counts_what_is_known() {
        let mut words = Words::default();
        let lines = ["「猫が食べた」", "♪", "", "「猫が食べた」"];
        assert_eq!(words.score(lines), Err(0), "nothing split yet");
        let text = split(&mut words);
        words.ask("♪");
        words.take_queue();
        assert!(!words.idle());
        assert_eq!(words.score(lines), Err(2), "♪ is not split yet");
        let before = words.revision();
        words.done(0, "♪".into(), Ok(vec![word("♪", 0..3, None)]));
        assert!(words.idle() && words.has("♪") && words.has(" "));
        assert!(words.revision() > before);
        let none = words.score(lines).unwrap();
        assert_eq!(
            none,
            Score {
                words: 6,
                known: 0,
                lines: 2,
                clear: 0
            }
        );
        assert_eq!(
            (none.known_percent(), none.clear_percent()),
            (Some(0), Some(0))
        );
        words.set_status("が", "が", WordStatus::Known);
        words.set_status("猫", "ねこ", WordStatus::Ignored);
        words.add_mined("食べる".into());
        let score = words.score(lines).unwrap();
        assert_eq!(
            score,
            Score {
                words: 4,
                known: 2,
                lines: 2,
                clear: 2
            }
        );
        assert_eq!(score.known_percent(), Some(50), "learning is not known");
        assert_eq!(score.clear_percent(), Some(100), "but not new either");
        words.set_status("食べる", "たべる", WordStatus::Unknown);
        assert_eq!(words.score([text]).unwrap().clear_percent(), Some(0));
        assert_eq!(words.score(["♪"]).unwrap().known_percent(), None);
    }

    // T-KNW-004: readings only over the words not known
    #[test]
    fn readings_skip_the_words_known() {
        let mut words = Words::default();
        let text = split(&mut words);
        let wanted = Wanted {
            furigana: true,
            furigana_unknown_only: true,
            ..Wanted::default()
        };
        words.set_status("猫", "ねこ", WordStatus::Known);
        let readings = words.marks(text, &wanted).unwrap().readings;
        assert_eq!(readings, [(9..12, "た".to_string())]);
        words.set_status("猫", "ねこ", WordStatus::Learning);
        assert_eq!(words.marks(text, &wanted).unwrap().readings.len(), 2);
    }

    // T-LKP-060
    #[test]
    fn texts_are_asked_once_and_dropped_when_the_dictionaries_change() {
        let mut words = Words::default();
        words.ask("猫");
        words.ask("猫");
        words.ask(" \n");
        assert_eq!(words.take_queue().len(), 1);
        words.ask("猫");
        assert!(words.take_queue().is_empty(), "already asked");
        words.forget();
        // the answer of the old dictionaries comes too late
        words.done(
            0,
            "猫".into(),
            Ok(vec![word("猫", 0..3, Some(("猫", "ねこ")))]),
        );
        assert_eq!(words.get("猫"), None);
        words.ask("猫");
        assert_eq!(words.take_queue(), [(1, "猫".to_string())]);
        words.done(1, "猫".into(), Err("no dictionary".into()));
        assert_eq!(words.get("猫"), Some(&[][..]));
    }

    // T-LKP-060
    #[test]
    fn the_keyboard_moves_word_by_word() {
        let mut words = Words::default();
        assert_eq!(words.step("「猫が食べた」", 3, true), None);
        let text = split(&mut words);
        assert_eq!(words.step(text, 3, true), Some(6));
        assert_eq!(words.step(text, 6, true), Some(9));
        assert_eq!(
            words.step(text, 9, true),
            None,
            "the closing quote is no word"
        );
        assert_eq!(words.step(text, 12, false), Some(9), "from inside a word");
        assert_eq!(words.step(text, 3, false), None);
        assert_eq!(words.step(text, 0, true), Some(3));
    }
}
