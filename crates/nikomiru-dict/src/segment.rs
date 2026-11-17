// SPDX-License-Identifier: GPL-3.0-only

//! The words of a text without a morphological analyser (LKP-060): from each
//! place, the longest headword the dictionaries find there, as a lookup
//! would show it first; where none starts, one character of text no
//! dictionary knows. Like Fushi's `japanese_language.dart`, with no MeCab or
//! IPADIC (their licenses).

use std::ops::Range;

use crate::lookup::{Entry, Lookup};
use crate::pitch::{Category, category, morae};
use crate::store::StoreError;
use crate::yomitan::PitchPosition;

/// A headword and its reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headword {
    /// Headword.
    pub expression: String,
    /// Reading (the headword itself when the dictionary gives none).
    pub reading: String,
    /// Its pitch accent: the first downstep of the first pitch dictionary
    /// that has it (LKP-042, LKP-062).
    pub pitch: Option<Category>,
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

/// The accent of an entry: the first downstep of its first pitch
/// dictionary (LKP-062); patterns are left out.
fn pitch_of(e: &Entry) -> Option<Category> {
    let reading = if e.reading.is_empty() {
        &e.expression
    } else {
        &e.reading
    };
    e.pitches
        .iter()
        .flat_map(|g| &g.pitches)
        .find_map(|p| match p.position {
            PitchPosition::Downstep(n) => {
                let rules = format!("{} {}", e.rules, p.tags.join(" "));
                Some(category(morae(reading).len(), n, &rules))
            }
            PitchPosition::Pattern(_) => None,
        })
}

/// The characters of a whole word (LNG-001): letters, digits, marks, and
/// apostrophes or hyphens inside it.
fn in_word(c: char) -> bool {
    !starts_no_word(c) || matches!(c, '\'' | '’' | '-')
}

/// A text split into whole words, each looked up whole (LNG-001): a word
/// the dictionaries do not know as a whole stays one piece.
fn segment_words(lookup: &Lookup, text: &str) -> Result<Vec<Word>, StoreError> {
    let mut words: Vec<Word> = Vec::new();
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        if starts_no_word(c) {
            let end = at + c.len_utf8();
            match words.last_mut() {
                Some(w) if w.headword.is_none() && !text[w.range.clone()].chars().any(in_word) => {
                    w.range.end = end;
                }
                _ => words.push(Word {
                    range: at..end,
                    headword: None,
                }),
            }
            at = end;
            continue;
        }
        let mut end = at;
        for (i, ch) in text[at..].char_indices() {
            if !in_word(ch) {
                break;
            }
            end = at + i + ch.len_utf8();
        }
        // a word does not end with an apostrophe or a hyphen
        while end > at && text[..end].ends_with(['\'', '’', '-']) {
            end -= text[..end].chars().next_back().map_or(1, char::len_utf8);
        }
        let end = end.max(at + c.len_utf8());
        let word = &text[at..end];
        let headword = lookup
            .first(word)?
            .filter(|e| e.matched == word)
            .map(|e| Headword {
                pitch: pitch_of(&e),
                expression: e.expression,
                reading: e.reading,
            });
        words.push(Word {
            range: at..end,
            headword,
        });
        at = end;
    }
    Ok(words)
}

/// Splits `text` into words (LKP-060), or into whole words when the lookup
/// says so (LNG-001).
pub fn segment(lookup: &Lookup, text: &str) -> Result<Vec<Word>, StoreError> {
    if lookup.whole_words() {
        return segment_words(lookup, text);
    }
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
                    pitch: pitch_of(&e),
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
