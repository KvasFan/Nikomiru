// SPDX-License-Identifier: GPL-3.0-only

//! The words of the subtitles (LKP-060) and what the overlay marks on them:
//! readings over their kanji (LKP-051) and the words already made into cards
//! (LKP-061). Each text is split once, away from the UI thread; the cues
//! about to come are asked ahead, so that their marks are ready when they
//! show.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use nikomiru_dict::segment::Word;
use nikomiru_text::furigana::distribute_inflected;
use nikomiru_text::kana::is_kanji;

/// Texts kept split; beyond, the cache starts again (an episode has a few
/// hundred cues).
const KEPT: usize = 4096;

/// What the overlay draws over a cue, in bytes of its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// Readings over kanji (LKP-051).
    pub readings: Vec<(Range<usize>, String)>,
    /// Words already made into cards (LKP-061).
    pub mined: Vec<Range<usize>>,
}

impl Marks {
    /// Nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.readings.is_empty() && self.mined.is_empty()
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
    }

    /// The dictionaries changed: every text is split again.
    pub fn forget(&mut self) {
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
        self.mined = expressions;
    }

    /// A card was made of `expression`.
    pub fn add_mined(&mut self, expression: String) {
        self.mined.insert(expression);
    }

    /// What to draw over `text`: readings when `furigana`, cards made when
    /// `mined`. `None` while its words are not known.
    pub fn marks(&self, text: &str, furigana: bool, mined: bool) -> Option<Marks> {
        let words = self.get(text)?;
        let mut marks = Marks::default();
        for w in words {
            let Some(h) = &w.headword else {
                continue;
            };
            let Some(surface) = text.get(w.range.clone()) else {
                continue;
            };
            if mined && self.mined.contains(&h.expression) {
                marks.mined.push(w.range.clone());
            }
            if furigana && surface.chars().any(is_kanji) {
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
        assert_eq!(words.marks("「猫が食べた」", true, true), None);
        let text = split(&mut words);
        let marks = words.marks(text, true, false).unwrap();
        assert_eq!(
            marks.readings,
            [(3..6, "ねこ".to_string()), (9..12, "た".to_string())]
        );
        assert!(marks.mined.is_empty());
        assert!(words.marks(text, false, true).unwrap().is_empty());
    }

    // T-LKP-061
    #[test]
    fn the_words_of_the_cards_made_are_marked() {
        let mut words = Words::default();
        let text = split(&mut words);
        words.set_mined(HashSet::from(["猫".to_string()]));
        let mined = words.marks(text, false, true).unwrap().mined;
        assert_eq!((mined.len(), &mined[0]), (1, &(3..6)));
        // a card just made, of an inflected word's headword
        words.add_mined("食べる".into());
        assert_eq!(words.marks(text, false, true).unwrap().mined, [3..6, 9..18]);
        assert!(words.marks(text, false, false).unwrap().is_empty());
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
