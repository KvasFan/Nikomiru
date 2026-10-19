// SPDX-License-Identifier: GPL-3.0-only

//! From a click to the strings a lookup tries (LKP-001…LKP-004): the query
//! text from the clicked character, its normalization and its prefixes.
//!
//! `candidates` is a port of Fushi's `native/fushidicts/fushidicts_src/scan/
//! word_scan.cpp` (commit ac8034e); the rest follows SPECIFICATION §6.6.1.

use std::ops::Range;

/// Where the clicked text comes from (LKP-002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// A subtitle cue: the query may run to the end of the cue.
    Cue,
    /// Free text (a glossary, a book): at most [`FREE_TEXT_LIMIT`] characters.
    Free,
}

/// Longest query from free text, in characters (LKP-002).
pub const FREE_TEXT_LIMIT: usize = 400;

/// Longest text normalized for a search, in characters (LKP-003).
pub const NORMALIZE_LIMIT: usize = 2000;

/// Prefixes tried by a lookup, in code points (LKP-004).
pub const SCAN_LENGTH: usize = 16;

/// Characters that end the query (LKP-002).
pub const DELIMITERS: &str =
    "。、！？…‥「」『』（）()【】〈〉《》〔〕｛｝{}［］[]・：；:;，,.─\n\r\"'“”‘’«»‹›";

/// A letter of a script that separates words with spaces (Latin, Greek,
/// Cyrillic, Armenian, Hebrew, Arabic, Georgian). CJK, kana and Hangul are not:
/// in them every character can start a word.
pub fn is_space_script_letter(c: char) -> bool {
    matches!(c as u32,
        0x41..=0x5A | 0x61..=0x7A | 0xAA | 0xB5 | 0xBA
        | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2AF | 0x1E00..=0x1EFF
        | 0x370..=0x3FF | 0x1F00..=0x1FFF | 0x400..=0x52F
        | 0x531..=0x556 | 0x561..=0x587
        | 0x5D0..=0x5EA | 0x5EF..=0x5F2
        | 0x620..=0x63F | 0x641..=0x64A | 0x66E | 0x66F | 0x671..=0x6D3 | 0x6D5
        | 0x6EE | 0x6EF | 0x6FA..=0x6FC | 0x6FF | 0x750..=0x77F | 0x8A0..=0x8BD
        | 0x10A0..=0x10C5 | 0x10D0..=0x10FA)
}

fn is_whitespace(c: char) -> bool {
    matches!(c as u32,
        0x20 | 0x09..=0x0D | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000)
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{02BC}')
}

/// The byte range of the text a click on byte `click` looks up (LKP-001,
/// LKP-002). In scripts with spaces the query starts at the beginning of the
/// clicked word, without crossing an apostrophe (`l’homme` clicked in `homme`
/// gives `homme`, as in Fushi). It ends at a delimiter, at the end of the cue
/// or after [`FREE_TEXT_LIMIT`] characters. An apostrophe inside a word
/// (`don't`, BUG-2056) does not end it, nor does a single space followed by
/// a character that could continue the query (BUG-1773), so the query never
/// ends with a space. Empty if the click is on a delimiter or a space.
pub fn query(text: &str, click: usize, context: Context) -> Range<usize> {
    let mut click = click.min(text.len());
    while !text.is_char_boundary(click) {
        click -= 1;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let Some(mut start) = chars.iter().position(|&(i, _)| i == click) else {
        return click..click;
    };
    let first = chars[start].1;
    if is_whitespace(first) || (DELIMITERS.contains(first) && !is_apostrophe(first)) {
        return click..click;
    }
    let letter = |i: usize| {
        chars
            .get(i)
            .is_some_and(|(_, c)| is_space_script_letter(*c))
    };
    let inner_apostrophe =
        |i: usize| is_apostrophe(chars[i].1) && i > 0 && letter(i - 1) && letter(i + 1);
    if is_space_script_letter(first) {
        while start > 0 && letter(start - 1) {
            start -= 1;
        }
    } else if is_apostrophe(first) {
        return click..click;
    }
    let limit = match context {
        Context::Cue => usize::MAX,
        Context::Free => FREE_TEXT_LIMIT,
    };
    let mut end = start;
    while end < chars.len() && end - start < limit {
        let c = chars[end].1;
        let stop = if is_apostrophe(c) {
            !inner_apostrophe(end)
        } else if c == ' ' {
            // a single space joins words (BUG-1773); two spaces, or a space
            // before a delimiter (`rock 'n' roll`), end the query
            chars
                .get(end + 1)
                .is_none_or(|(_, n)| is_whitespace(*n) || DELIMITERS.contains(*n))
        } else {
            is_whitespace(c) || DELIMITERS.contains(c)
        };
        if stop {
            break;
        }
        end += 1;
    }
    let byte = |i: usize| chars.get(i).map_or(text.len(), |(b, _)| *b);
    byte(start)..byte(end)
}

fn is_emoji(c: char) -> bool {
    matches!(c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D)
}

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
        || DELIMITERS.contains(c)
        || matches!(c as u32, 0x2010..=0x205E | 0x3000..=0x303F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65)
}

/// The query as the dictionaries see it (LKP-003): at most
/// [`NORMALIZE_LIMIT`] characters, line breaks and emoji as spaces, without
/// punctuation or spaces at the start and at the end.
pub fn normalize(text: &str) -> String {
    let mapped: String = text
        .chars()
        .take(NORMALIZE_LIMIT)
        .map(|c| {
            if c == '\n' || c == '\r' || is_emoji(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    mapped
        .trim_matches(|c: char| is_punctuation(c) || is_whitespace(c))
        .to_string()
}

/// The prefixes of the first `scan_length` code points of `text`, longest
/// first (LKP-004): never cut between two letters of a script with spaces,
/// never ending with a space.
pub fn candidates(text: &str, scan_length: usize) -> Vec<&str> {
    let ends: Vec<usize> = text
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .take(scan_length)
        .collect();
    let mut out = Vec::new();
    for &end in ends.iter().rev() {
        let prefix = &text[..end];
        let prev = prefix.chars().next_back();
        let next = text[end..].chars().next();
        let boundary_ok = match (prev, next) {
            (Some(p), Some(n)) => !(is_space_script_letter(p) && is_space_script_letter(n)),
            _ => true,
        };
        if boundary_ok && !prev.is_some_and(is_whitespace) {
            out.push(prefix);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str, click_char: usize, context: Context) -> &str {
        let click = text
            .char_indices()
            .nth(click_char)
            .map_or(text.len(), |(i, _)| i);
        &text[query(text, click, context)]
    }

    // T-LKP-001
    #[test]
    fn the_query_starts_at_the_word_or_the_character() {
        assert_eq!(q("hello world", 2, Context::Cue), "hello world");
        assert_eq!(q("日本語を", 2, Context::Cue), "語を");
        assert_eq!(q("「日本」", 0, Context::Cue), "");
    }

    // T-LKP-002
    #[test]
    fn delimiters_spaces_and_apostrophes() {
        assert_eq!(q("食べました。次の文", 0, Context::Cue), "食べました");
        assert_eq!(q("彼は「こんにちは」", 0, Context::Cue), "彼は");
        assert_eq!(q("listen to me", 1, Context::Cue), "listen to me");
        assert_eq!(q("I don't know", 3, Context::Cue), "don't know");
        assert_eq!(q("l’homme", 3, Context::Cue), "homme");
        assert_eq!(q("rock 'n' roll", 0, Context::Cue), "rock");
        assert_eq!(q("say 'hi' now", 5, Context::Cue), "hi");
        assert_eq!(q("two  spaces", 0, Context::Cue), "two");
        let long = "あ".repeat(500);
        assert_eq!(q(&long, 0, Context::Free).chars().count(), FREE_TEXT_LIMIT);
        assert_eq!(q(&long, 0, Context::Cue).chars().count(), 500);
    }

    #[test]
    fn normalization() {
        // the line break and the emoji become one space each
        assert_eq!(normalize("「食べる」\n😀走る！"), "食べる」  走る");
        assert_eq!(
            normalize(&"字".repeat(3000)).chars().count(),
            NORMALIZE_LIMIT
        );
    }

    // T-LKP-004
    #[test]
    fn candidates_never_cut_inside_a_latin_word() {
        assert_eq!(
            candidates("食べました", 16),
            ["食べました", "食べまし", "食べま", "食べ", "食"]
        );
        assert_eq!(
            candidates("listen to me", 16),
            ["listen to me", "listen to", "listen"]
        );
        let long = "あ".repeat(20);
        assert_eq!(candidates(&long, SCAN_LENGTH)[0].chars().count(), 16);
    }
}
