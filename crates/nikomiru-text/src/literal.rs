// SPDX-License-Identifier: GPL-3.0-only

//! Literal text search with original offsets (RDR-003): canonical composition
//! and case folding, never regular expressions or dictionary preprocessing.

use std::borrow::Cow;
use std::ops::Range;

/// Longest query, in bytes.
pub const MAX_QUERY: usize = 256;

/// NFC, then lowercase; borrowed when the text is already folded.
pub fn fold(text: &str) -> Cow<'_, str> {
    let normal = icu_normalizer::ComposingNormalizer::new_nfc().normalize(text);
    if normal.chars().all(|c| !c.is_uppercase()) {
        return normal;
    }
    Cow::Owned(normal.chars().flat_map(char::to_lowercase).collect())
}

/// A folded query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle(String);

impl Needle {
    /// A query of 1–256 bytes whose folded form is not blank.
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() || query.len() > MAX_QUERY || query.chars().any(char::is_control) {
            return None;
        }
        let folded = fold(query).into_owned();
        (!folded.trim().is_empty()).then_some(Self(folded))
    }

    /// The folded query.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// At most `limit` non-overlapping matches in `text`, as byte ranges of
    /// the original text. `folded` is [`fold`] of `text` when it differs from
    /// it (`None` means the text is its own folded form).
    pub fn find(&self, text: &str, folded: Option<&str>, limit: usize) -> Vec<Range<usize>> {
        let Some(folded) = folded else {
            return text
                .match_indices(self.0.as_str())
                .take(limit)
                .map(|(at, m)| at..at + m.len())
                .collect();
        };
        let boundaries: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        let folded_len = |b: usize| fold(&text[..b]).len();
        let mut out = Vec::new();
        for (at, m) in folded.match_indices(self.0.as_str()) {
            if out.len() >= limit {
                break;
            }
            // the last boundary whose folded prefix ends at or before the match
            let first = boundaries.partition_point(|&b| folded_len(b) <= at);
            let start = boundaries[first.saturating_sub(1)];
            // the first boundary whose folded prefix covers the match
            let last = boundaries.partition_point(|&b| folded_len(b) < at + m.len());
            let Some(&end) = boundaries.get(last) else {
                continue;
            };
            if end > start && fold(&text[start..end]).contains(self.0.as_str()) {
                out.push(start..end);
            }
        }
        out
    }
}

/// The folded form to keep beside `text`, or `None` if it is the same.
pub fn folded_copy(text: &str) -> Option<String> {
    match fold(text) {
        Cow::Borrowed(same) if same == text => None,
        Cow::Owned(f) if f == text => None,
        other => Some(other.into_owned()),
    }
}

/// A snippet of at most `around` characters on each side of `range`, on
/// character boundaries, with its range inside the snippet.
pub fn snippet(text: &str, range: Range<usize>, around: usize) -> (String, Range<usize>) {
    let start = text[..range.start]
        .char_indices()
        .rev()
        .nth(around.saturating_sub(1))
        .map_or(0, |(i, _)| i);
    let end = text[range.end..]
        .char_indices()
        .nth(around)
        .map_or(text.len(), |(i, _)| range.end + i);
    (
        text[start..end].replace(['\n', '\r', '\t'], " "),
        range.start - start..range.end - start,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-RDR-003: Japanese offsets stay original; case and composition fold.
    #[test]
    fn literal_matches_keep_original_offsets() {
        let n = Needle::new("猫").unwrap();
        let text = "私は猫と猫が好き";
        assert_eq!(
            n.find(text, folded_copy(text).as_deref(), 10),
            vec![6..9, 12..15]
        );
        assert_eq!(n.find(text, None, 1), vec![6..9], "limited");
        let text = "Le CAFE\u{301} et le Café";
        let folded = folded_copy(text);
        assert!(folded.is_some());
        let hits = Needle::new("café")
            .unwrap()
            .find(text, folded.as_deref(), 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(&text[hits[0].clone()], "CAFE\u{301}");
        assert_eq!(&text[hits[1].clone()], "Café");
        // kana and kanji are not folded into each other
        assert!(
            Needle::new("ねこ")
                .unwrap()
                .find("猫ネコ", None, 9)
                .is_empty()
        );
    }

    // T-RDR-003: bounded queries.
    #[test]
    fn queries_are_bounded() {
        assert!(Needle::new("").is_none());
        assert!(Needle::new(" ").is_none());
        assert!(Needle::new("a\u{0}").is_none());
        assert!(Needle::new(&"a".repeat(MAX_QUERY + 1)).is_none());
        assert!(Needle::new(&"a".repeat(MAX_QUERY)).is_some());
        assert_eq!(Needle::new("ABC").unwrap().as_str(), "abc");
        assert_eq!(folded_copy("猫です"), None);
    }

    #[test]
    fn snippets_stay_on_char_boundaries() {
        let text = "一二三四五六七八九";
        let (s, r) = snippet(text, 12..15, 2);
        assert_eq!(s, "三四五六七");
        assert_eq!(&s[r], "五");
        let (s, r) = snippet(text, 0..3, 2);
        assert_eq!(s, "一二三");
        assert_eq!(&s[r], "一");
    }
}
