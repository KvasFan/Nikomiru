// SPDX-License-Identifier: GPL-3.0-only

//! The words of a text without a morphological analyser (LKP-060): from each
//! place, the longest headword the dictionaries find there, as a lookup
//! would show it first; where none starts, one character of text no
//! dictionary knows. Like Fushi's `japanese_language.dart`, with no MeCab or
//! IPADIC (their licenses).

use std::ops::Range;

use crate::lookup::Lookup;
use crate::store::StoreError;

/// A headword and its reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headword {
    /// Headword.
    pub expression: String,
    /// Reading (the headword itself when the dictionary gives none).
    pub reading: String,
}

/// A piece of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// Its bytes in the text.
    pub range: Range<usize>,
    /// The headword found there; `None` for text the dictionaries do not
    /// know (punctuation, spaces, unknown words), kept in one piece.
    pub headword: Option<Headword>,
}

/// Spaces and punctuation start no word: they are not looked up.
fn starts_no_word(c: char) -> bool {
    c.is_whitespace()
        || c.is_ascii_punctuation()
        || matches!(c,
            // CJK symbols and punctuation, without the iteration marks 々 〆
            '\u{3000}'..='\u{3004}' | '\u{3007}'..='\u{303F}'
            // full-width punctuation and the half-width 。「」、・
            | '\u{FF01}'..='\u{FF0F}' | '\u{FF1A}'..='\u{FF20}' | '\u{FF3B}'..='\u{FF40}'
            | '\u{FF5B}'..='\u{FF65}'
            | '・' | '…' | '‥' | '―' | '—' | '–' | '♪' | '♫' | '→' | '←')
}

/// Splits `text` into words (LKP-060).
pub fn segment(lookup: &Lookup, text: &str) -> Result<Vec<Word>, StoreError> {
    let mut words: Vec<Word> = Vec::new();
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        let rest = &text[at..];
        let mut found = None;
        if !starts_no_word(c)
            && let Some(e) = lookup.first(rest)?.filter(|e| rest.starts_with(&e.matched))
        {
            found = Some((
                e.matched.len(),
                Headword {
                    expression: e.expression,
                    reading: e.reading,
                },
            ));
        }
        match found {
            Some((len, headword)) if len > 0 => {
                words.push(Word {
                    range: at..at + len,
                    headword: Some(headword),
                });
                at += len;
            }
            _ => {
                let end = at + c.len_utf8();
                match words.last_mut() {
                    Some(w) if w.headword.is_none() => w.range.end = end,
                    _ => words.push(Word {
                        range: at..end,
                        headword: None,
                    }),
                }
                at = end;
            }
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_and_spaces_start_no_word() {
        for c in [
            '。', '、', '「', '」', '！', '？', ' ', '\u{3000}', '・', '…', '(', '\n',
        ] {
            assert!(starts_no_word(c), "{c}");
        }
        for c in ['々', '日', 'あ', 'ア', 'ｱ', 'a', '1'] {
            assert!(!starts_no_word(c), "{c}");
        }
    }
}
