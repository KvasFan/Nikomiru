// SPDX-License-Identifier: GPL-3.0-only

//! The current text chapter's count, score and word browser (KNW-014/015).
//! Completed counts survive segmentation-cache eviction; statuses remain shared
//! with the player. Long chapters keep their reader without a partial score.

use crate::words::{Counted, ListedWord, Score, Words};
use nikomiru_books::Chapter;
use std::sync::Arc;

/// Largest paragraph sent to the word worker (KNW-002).
pub const MAX_PARAGRAPH_BYTES: usize = 64 << 10;
/// Largest chapter analysed as a whole (KNW-014).
pub const MAX_CHAPTER_BYTES: usize = 256 << 10;
/// Keep room in the segmentation cache for the player's subtitle lines.
pub const MAX_PARAGRAPHS: usize = 1024;
const BATCH: usize = 16;

/// A reader's current chapter; derived data, never persisted.
#[derive(Debug, Default)]
pub struct Study {
    key: Option<(String, usize, u64)>,
    counted: Option<Counted>,
    revision: Option<u64>,
    /// Completed score, or the number of text paragraphs counted so far.
    pub score: Option<Result<Score, usize>>,
    /// Words with their current statuses, once the count is complete.
    pub words: Option<Arc<Vec<ListedWord>>>,
    /// Nonempty text paragraphs in this chapter.
    pub paragraphs: usize,
    /// The chapter exceeds one of the analysis bounds.
    pub limited: bool,
}

impl Study {
    /// Forget a closed/reloaded book or a disabled analysis.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Count a chapter in small batches, then update only its derived score
    /// and word statuses. Visible paragraphs are queued by the reader first.
    pub fn update(
        &mut self,
        uid: &str,
        chapter_index: usize,
        chapter: &Chapter,
        words: &mut Words,
    ) {
        let key = (uid.to_string(), chapter_index, words.generation());
        if self.key.as_ref() != Some(&key) {
            self.clear();
            self.key = Some(key);
            let mut bytes: usize = 0;
            for p in chapter
                .paragraphs
                .iter()
                .filter(|p| p.image.is_none() && !p.text.trim().is_empty())
            {
                self.paragraphs += 1;
                bytes = bytes.saturating_add(p.text.len());
                self.limited |= p.text.len() > MAX_PARAGRAPH_BYTES;
            }
            self.limited |= self.paragraphs > MAX_PARAGRAPHS || bytes > MAX_CHAPTER_BYTES;
        }
        if self.limited {
            return;
        }
        let texts = || {
            chapter
                .paragraphs
                .iter()
                .filter(|p| p.image.is_none())
                .map(|p| p.text.as_str())
        };
        if self.counted.is_none() {
            if words.idle() {
                let missing: Vec<_> = texts().filter(|t| !words.has(t)).take(BATCH).collect();
                for text in missing {
                    words.ask(text);
                }
            }
            self.counted = words.counted(texts());
            if self.counted.is_none() {
                self.score = Some(Err(texts()
                    .filter(|t| !t.trim().is_empty() && words.has(t))
                    .count()));
                return;
            }
        }
        if self.revision == Some(words.revision()) {
            return;
        }
        if let Some(counted) = &self.counted {
            self.score = Some(Ok(counted.score(|e, r| words.status(e, r))));
            self.words = Some(Arc::new(words.listed_counted(counted)));
            self.revision = Some(words.revision());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;
    use nikomiru_books::Paragraph;
    use nikomiru_dict::segment::{Headword, Word};

    fn chapter(texts: &[&str]) -> Chapter {
        Chapter {
            title: None,
            paragraphs: texts
                .iter()
                .map(|text| Paragraph {
                    text: (*text).into(),
                    ruby: vec![],
                    heading: false,
                    image: None,
                })
                .collect(),
        }
    }

    fn finish(words: &mut Words) {
        for (generation, text) in words.take_queue() {
            let word = Word {
                range: 0..text.len(),
                headword: Some(Headword {
                    expression: text.clone(),
                    reading: String::new(),
                    pitch: None,
                }),
            };
            words.done(generation, text, Ok(vec![word]));
        }
    }

    // T-KNW-014: bounded batches, repeated paragraphs, and shared statuses.
    #[test]
    fn count_then_rescore_without_segmenting_again() {
        let chapter = chapter(&["猫", "猫", "食べる", " "]);
        let mut words = Words::default();
        let mut study = Study::default();
        study.update("a", 0, &chapter, &mut words);
        finish(&mut words);
        study.update("a", 0, &chapter, &mut words);
        let score = study.score.unwrap().unwrap();
        assert_eq!(
            (score.words, score.known, score.lines, score.clear),
            (3, 0, 3, 0)
        );
        assert_eq!(study.words.as_ref().unwrap()[0].count, 2);
        words.set_status("猫", "", WordStatus::Known);
        words.set_status("食べる", "", WordStatus::Learning);
        study.update("a", 0, &chapter, &mut words);
        let score = study.score.unwrap().unwrap();
        assert_eq!((score.words, score.known, score.clear), (3, 2, 3));
        words.set_status("食べる", "", WordStatus::Ignored);
        study.update("a", 0, &chapter, &mut words);
        assert_eq!(study.score.unwrap().unwrap().words, 2);
        assert!(words.take_queue().is_empty());
        words.set_mined(std::collections::HashSet::from(["猫".into()]));
        study.update("a", 0, &chapter, &mut words);
        assert!(study.words.as_ref().unwrap()[0].card);
    }

    // T-KNW-014: subtitle cache eviction cannot remove a completed chapter.
    #[test]
    fn retained_count_survives_eviction_but_not_dictionary_or_chapter_changes() {
        let chapter = chapter(&["猫"]);
        let mut words = Words::default();
        let mut study = Study::default();
        study.update("a", 0, &chapter, &mut words);
        finish(&mut words);
        study.update("a", 0, &chapter, &mut words);
        for i in 0..crate::words::KEPT {
            words.done(0, format!("unrelated {i}"), Ok(vec![]));
        }
        assert!(!words.has("猫"));
        words.set_status("猫", "", WordStatus::Known);
        study.update("a", 0, &chapter, &mut words);
        assert_eq!(study.score.unwrap().unwrap().known, 1);
        assert!(words.take_queue().is_empty());
        words.forget();
        study.update("a", 0, &chapter, &mut words);
        assert_eq!(study.score, Some(Err(0)));
        assert!(study.words.is_none());
        finish(&mut words);
        study.update("a", 1, &self::chapter(&["犬"]), &mut words);
        assert_eq!(study.score, Some(Err(0)));
        assert!(study.words.is_none());
    }

    // T-KNW-014: an over-limit chapter never presents a partial score.
    #[test]
    fn limits_and_batches_leave_the_reader_available() {
        let mut words = Words::default();
        let mut study = Study::default();
        let texts: Vec<_> = (0..40).map(|i| format!("段落{i}")).collect();
        let chapter = chapter(&texts.iter().map(String::as_str).collect::<Vec<_>>());
        study.update("a", 0, &chapter, &mut words);
        assert_eq!(words.take_queue().len(), BATCH);
        words.forget();
        let long = "a".repeat(MAX_PARAGRAPH_BYTES + 1);
        let full = "a".repeat(MAX_PARAGRAPH_BYTES);
        for chapter in [
            self::chapter(&[&long]),
            self::chapter(&vec!["猫"; MAX_PARAGRAPHS + 1]),
            self::chapter(&[full.as_str(); 5]),
        ] {
            study.clear();
            study.update("a", 0, &chapter, &mut words);
            assert!(study.limited);
            assert!(study.score.is_none() && study.words.is_none());
            assert!(words.take_queue().is_empty());
        }
    }

    // T-KNW-014: image captions and whitespace are not text paragraphs.
    #[test]
    fn images_and_empty_paragraphs_do_not_consume_the_text_budget() {
        let mut chapter = chapter(&[" ", "猫"]);
        chapter.paragraphs[1].image = Some("picture.png".into());
        chapter.paragraphs[1].text = "a".repeat(MAX_PARAGRAPH_BYTES + 1);
        let mut study = Study::default();
        let mut words = Words::default();
        study.update("image", 0, &chapter, &mut words);
        assert_eq!(study.paragraphs, 0);
        assert!(!study.limited);
        assert_eq!(study.score.unwrap().unwrap().words, 0);
        assert!(words.take_queue().is_empty());
    }
}
