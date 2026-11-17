// SPDX-License-Identifier: GPL-3.0-only

//! Original-byte guided lookup targets; no status or playback mutations (GDL-001…003).
use crate::words::WordStatus;
use nikomiru_dict::segment::Word;

pub(crate) fn first_unknown(
    text: &str,
    words: &[Word],
    status: impl Fn(&str, &str) -> WordStatus,
) -> Option<usize> {
    words.iter().find_map(|word| {
        let h = word.headword.as_ref()?;
        (word.range.start < word.range.end
            && text
                .get(word.range.clone())
                .is_some_and(|s| !s.trim().is_empty())
            && status(&h.expression, &h.reading) == WordStatus::Unknown)
            .then_some(word.range.start)
    })
}
pub(crate) fn first_kanji(text: &str) -> Option<usize> {
    text.char_indices()
        .find(|(_, c)| nikomiru_text::kana::is_kanji(*c))
        .map(|(i, _)| i)
}

/// Ownership of one pending pause transition, discarded rather than retargeted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pending {
    pub token: nikomiru_player::control::LoadToken,
    pub version: u64,
    pub dictionaries: u64,
    pub cue: usize,
    pub rule: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nikomiru_dict::segment::Headword;
    #[test]
    fn unknown_targets_use_exact_identity_and_original_utf8_ranges() {
        let text = "猫と犬と鳥";
        let word = |at, e: &str, r: &str| Word {
            range: at..at + 3,
            headword: Some(Headword {
                expression: e.into(),
                reading: r.into(),
                pitch: None,
            }),
        };
        let words = vec![
            word(0, "猫", "ねこ"),
            word(6, "犬", "いぬ"),
            word(12, "鳥", "とり"),
        ];
        assert_eq!(
            first_unknown(text, &words, |e, _| if e == "鳥" {
                WordStatus::Unknown
            } else {
                WordStatus::Known
            }),
            Some(12)
        );
        for status in [WordStatus::Known, WordStatus::Learning, WordStatus::Ignored] {
            assert_eq!(first_unknown(text, &words, |_, _| status), None);
        }
        let words = vec![word(0, "同じ", "あ"), word(6, "同じ", "い")];
        assert_eq!(
            first_unknown(text, &words, |_, r| if r == "あ" {
                WordStatus::Known
            } else {
                WordStatus::Unknown
            }),
            Some(6)
        );
        let unknown = Word {
            range: 0..3,
            headword: None,
        };
        assert_eq!(
            first_unknown(text, &[unknown], |_, _| WordStatus::Unknown),
            None
        );
    }
    #[test]
    fn kanji_is_an_independent_character_rule() {
        assert_eq!(first_kanji("かな。食べる"), Some("かな。".len()));
        assert_eq!(first_kanji("カナ abc 123"), None);
        assert_eq!(first_kanji("𠮷野家"), Some(0));
    }
}
