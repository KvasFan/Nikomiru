// SPDX-License-Identifier: GPL-3.0-only

//! How many units of study a text has (STA-003): a character of the scripts
//! written without spaces (Han, kana, Hangul, Thai, …) is one; a run of
//! letters or digits of the other scripts is one, a word; combining marks,
//! joiners and apostrophes inside a word count nothing and do not end it;
//! punctuation, symbols and spaces count nothing.
//!
//! Translated from Fushi `packages/fushi_engine/lib/stats/study_char_count.dart`
//! (commit `ac8034e`), with its test vectors.

use std::sync::LazyLock;

use regex::Regex;

fn re(pattern: &str) -> Regex {
    // the patterns are constants: a wrong one fails every test of this module
    Regex::new(pattern).unwrap_or_else(|e| panic!("bad pattern {pattern}: {e}"))
}

/// Counts nothing and does not end a word.
static TRANSPARENT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^[\p{M}\x{200C}\x{200D}'\x{2019}\x{02BC}]$"));
/// A letter or a digit: what may be a unit.
static LETTER_OR_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"^[\p{L}\p{N}]$"));
/// The scripts without spaces, by Script_Extensions: `ー` and `々` are
/// Common by Script, kana and Han by their extensions.
static NO_SPACE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"^[\p{scx=Han}\p{scx=Hiragana}\p{scx=Katakana}\p{scx=Hangul}\p{scx=Bopomofo}\p{scx=Yi}\p{scx=Thai}\p{scx=Lao}\p{scx=Khmer}\p{scx=Myanmar}\p{scx=Tibetan}]$",
    )
});

/// A count that goes on over several pieces of text, as if they were one: a
/// word may run from one piece into the next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StudyCounter {
    count: usize,
    in_word: bool,
}

impl StudyCounter {
    /// Counts `text` after what was added.
    pub fn add(&mut self, text: &str) {
        let mut buf = [0u8; 4];
        for c in text.chars() {
            let s = c.encode_utf8(&mut buf);
            if TRANSPARENT.is_match(s) {
                continue;
            }
            if LETTER_OR_NUMBER.is_match(s) {
                if NO_SPACE.is_match(s) {
                    if self.in_word {
                        self.count += 1;
                        self.in_word = false;
                    }
                    self.count += 1;
                } else {
                    self.in_word = true;
                }
                continue;
            }
            if self.in_word {
                self.count += 1;
                self.in_word = false;
            }
        }
    }

    /// The units so far; a word not yet ended counts.
    pub fn count(&self) -> usize {
        self.count + usize::from(self.in_word)
    }
}

/// The units of study of `text` (STA-003).
pub fn count_study_chars(text: &str) -> usize {
    let mut c = StudyCounter::default();
    c.add(text);
    c.count()
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-STA-003: Fushi `study_char_count_test.dart`
    #[test]
    fn units_of_every_script() {
        for (text, n) in [
            ("素晴らしい世界", 7),
            ("コーヒー", 4),
            ("人々", 2),
            ("ｶﾀｶﾅ", 4),
            ("你好世界", 4),
            ("\u{20BB7}野家", 3),
            ("안녕하세요", 5),
            ("สวัสดี", 4),
            ("I do not know", 4),
            ("Hello, world!", 2),
            ("I don't know", 3),
            ("I don\u{2019}t know", 3),
            ("John's book", 2),
            ("rock 'n' roll", 3),
            ("café", 1),
            ("Grüße über Straße", 3),
            ("Привет мир", 2),
            ("Καλημέρα κόσμε", 2),
            (
                "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} \u{0628}\u{0643}",
                2,
            ),
            ("\u{0643}\u{0650}\u{062A}\u{064E}\u{0627}\u{0628}", 1),
            ("2026", 1),
            ("I read 300 pages", 4),
            ("（）【】「」『』、。！？\u{3000}", 0),
            ("！？…—", 0),
            ("", 0),
            ("   \n\t", 0),
            // 〇 is a number of the Han script: one unit, not a word
            ("〇〇", 2),
            ("日本語abc", 4),
        ] {
            assert_eq!(count_study_chars(text), n, "{text}");
        }
        // pieces count as their concatenation
        let mut c = StudyCounter::default();
        c.add("ab");
        c.add("cd");
        assert_eq!(c.count(), 1);
        c.add(" 日本");
        assert_eq!(c.count(), 3);
    }
}
