// SPDX-License-Identifier: GPL-3.0-only

//! The sentence around a selection in flat text (T-TXT-007): a port of Fushi's
//! `extractSentenceAt` (`fushi/lib/lookup/sentence_extraction.dart`, commit
//! ac8034e). Subtitles use the whole cue as the sentence (ANK-024); this is
//! for free text, where a paragraph holds many sentences.

use std::ops::Range;

/// Characters that end a sentence. Unlike Fushi's DOM reader, a line break
/// ends one too: flat text has no layout whitespace.
pub const SENTENCE_ENDS: &str = "。！？.!?\n\r";

/// Characters pulled into the sentence when they follow its end (`。」`).
pub const CLOSERS: &str = "。、！？…‥」』）)】〉》〕｝}］]";

/// A sentence found by [`sentence`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// Byte range of the sentence in the text, without surrounding spaces.
    pub range: Range<usize>,
    /// Byte range of the selection, relative to `range.start`.
    pub selection: Range<usize>,
}

fn floor(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The sentence of `text` that contains `selection` (byte offsets, clamped to
/// the text). It starts after the last sentence end before the selection and
/// stops at the first one at or after the selection's end, inclusive, plus the
/// closers that follow it directly.
pub fn sentence(text: &str, selection: Range<usize>) -> Sentence {
    let sel_start = floor(text, selection.start);
    let sel_end = floor(text, selection.end.max(sel_start));
    let start = text[..sel_start]
        .char_indices()
        .rev()
        .find(|&(_, c)| SENTENCE_ENDS.contains(c))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let mut end = text[sel_end..]
        .char_indices()
        .find(|&(_, c)| SENTENCE_ENDS.contains(c))
        .map_or(text.len(), |(i, c)| sel_end + i + c.len_utf8());
    end += text[end..]
        .chars()
        .take_while(|&c| CLOSERS.contains(c))
        .map(char::len_utf8)
        .sum::<usize>();
    let raw = &text[start..end];
    let trimmed_start = start + (raw.len() - raw.trim_start().len());
    let trimmed_end = trimmed_start + raw.trim().len();
    let rel_start = sel_start.saturating_sub(trimmed_start);
    Sentence {
        range: trimmed_start..trimmed_end,
        selection: rel_start..rel_start + (sel_end - sel_start),
    }
}

/// Every sentence of `text`, in order, by the same rules as [`sentence`]
/// (RDR-004): their byte ranges, trimmed, none of them empty.
pub fn sentences(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let blank = rest.len() - rest.trim_start().len();
        at += blank;
        if at >= text.len() {
            break;
        }
        let found = sentence(text, at..at).range;
        let end = if found.end > at {
            found.end
        } else {
            at + text[at..].chars().next().map_or(1, char::len_utf8)
        };
        let range = found.start.max(at)..found.end.max(at);
        if !range.is_empty() {
            out.push(range);
        }
        at = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-RDR-004: sentence focus covers the paragraph in order.
    #[test]
    fn a_paragraph_splits_into_its_sentences() {
        let text = "彼は「こんにちは」と言った。次の文。 Last one";
        let parts: Vec<&str> = sentences(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(
            parts,
            ["彼は「こんにちは」と言った。", "次の文。", "Last one"]
        );
        assert!(sentences("").is_empty());
        assert!(sentences("  \n ").is_empty());
        assert_eq!(sentences("。。"), vec![0..6]);
    }

    fn around<'a>(text: &'a str, word: &str) -> &'a str {
        let at = text.find(word).unwrap();
        &text[sentence(text, at..at + word.len()).range]
    }

    // T-TXT-007
    #[test]
    fn the_sentence_of_the_click() {
        let text = "彼は「こんにちは」と言った。次の文。";
        assert_eq!(around(text, "言"), "彼は「こんにちは」と言った。");
        assert_eq!(around(text, "次"), "次の文。");
        assert_eq!(around("Hi! Is it «done»? Yes.", "done"), "Is it «done»?");
    }
}
