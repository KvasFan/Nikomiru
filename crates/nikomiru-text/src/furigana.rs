// SPDX-License-Identifier: GPL-3.0-only

//! Furigana over a headword (LKP-050): the expression is split into kana and
//! kanji groups and the reading is aligned to them; when the alignment is
//! ambiguous, the whole reading goes over the whole expression.
//!
//! Port of Yomitan's `distributeFurigana` / `segmentizeFurigana` in
//! `ext/js/language/ja/japanese.js` (GPL-3.0), working on code points.

use crate::kana::{is_kana, katakana_to_hiragana};

/// A piece of the headword with the reading above it (empty for kana).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Text of the headword.
    pub text: String,
    /// Its reading, empty when the text is its own reading.
    pub reading: String,
}

fn segment(text: &[char], reading: &[char]) -> Segment {
    Segment {
        text: text.iter().collect(),
        reading: reading.iter().collect(),
    }
}

struct Group {
    kana: bool,
    text: Vec<char>,
    normalized: Vec<char>,
}

/// Kana written differently in the reading (`ヶ` over `が`): one segment per
/// run of equal or different characters.
fn kana_segments(text: &[char], reading: &[char]) -> Vec<Segment> {
    let mut out = Vec::new();
    let same = |i: usize| reading.get(i) == text.get(i);
    let mut start = 0;
    let mut state = same(0);
    for i in 1..text.len() {
        if same(i) != state {
            out.push(segment(
                &text[start..i],
                if state { &[] } else { &reading[start..i] },
            ));
            state = !state;
            start = i;
        }
    }
    let end = text.len();
    out.push(segment(
        &text[start..end],
        if state {
            &[]
        } else {
            &reading[start..end.min(reading.len())]
        },
    ));
    out
}

fn segmentize(reading: &[char], normalized: &[char], groups: &[Group]) -> Option<Vec<Segment>> {
    let Some((group, rest)) = groups.split_first() else {
        return reading.is_empty().then(Vec::new);
    };
    let len = group.text.len();
    if group.kana {
        if !normalized.starts_with(&group.normalized) {
            return None;
        }
        let mut segments = segmentize(&reading[len..], &normalized[len..], rest)?;
        let head = if reading.starts_with(&group.text) {
            vec![segment(&group.text, &[])]
        } else {
            kana_segments(&group.text, reading)
        };
        segments.splice(0..0, head);
        return Some(segments);
    }
    let mut result = None;
    for i in (len..=reading.len()).rev() {
        if let Some(mut segments) = segmentize(&reading[i..], &normalized[i..], rest) {
            if result.is_some() {
                // more than one way to split the tail: ambiguous
                return None;
            }
            segments.insert(0, segment(&group.text, &reading[..i]));
            result = Some(segments);
        }
        // the last kanji group can only take the rest of the reading
        if rest.is_empty() {
            break;
        }
    }
    result
}

/// Distributes `reading` over `expression` (LKP-050).
pub fn distribute(expression: &str, reading: &str) -> Vec<Segment> {
    if reading.is_empty() || reading == expression {
        return vec![Segment {
            text: expression.to_string(),
            reading: String::new(),
        }];
    }
    let mut groups: Vec<Group> = Vec::new();
    for c in expression.chars() {
        let kana = is_kana(c);
        match groups.last_mut() {
            Some(g) if g.kana == kana => g.text.push(c),
            _ => groups.push(Group {
                kana,
                text: vec![c],
                normalized: Vec::new(),
            }),
        }
    }
    for g in groups.iter_mut().filter(|g| g.kana) {
        g.normalized = katakana_to_hiragana(&g.text.iter().collect::<String>())
            .chars()
            .collect();
    }
    let reading_chars: Vec<char> = reading.chars().collect();
    let normalized: Vec<char> = katakana_to_hiragana(reading).chars().collect();
    segmentize(&reading_chars, &normalized, &groups).unwrap_or_else(|| {
        vec![Segment {
            text: expression.to_string(),
            reading: reading.to_string(),
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(expression: &str, reading: &str) -> String {
        distribute(expression, reading)
            .iter()
            .map(|s| {
                if s.reading.is_empty() {
                    format!("[{}]", s.text)
                } else {
                    format!("[{}|{}]", s.text, s.reading)
                }
            })
            .collect()
    }

    // T-LKP-050
    #[test]
    fn readings_are_aligned_to_kanji_groups() {
        assert_eq!(show("食べ物", "たべもの"), "[食|た][べ][物|もの]");
        assert_eq!(show("お茶", "おちゃ"), "[お][茶|ちゃ]");
        assert_eq!(show("日本", "にっぽん"), "[日本|にっぽん]");
        assert_eq!(show("ひらがな", "ひらがな"), "[ひらがな]");
        // katakana in the headword, hiragana in the reading
        assert_eq!(
            show("アイス食べる", "あいすたべる"),
            "[アイス|あいす][食|た][べる]"
        );
        // ambiguous alignment: the reading over everything
        assert_eq!(show("取扱い", "とりあつかい"), "[取扱|とりあつか][い]");
        // a reading that does not fit
        assert_eq!(show("食べる", "のむ"), "[食べる|のむ]");
    }
}
