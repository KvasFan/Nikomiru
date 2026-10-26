// SPDX-License-Identifier: GPL-3.0-only

//! Furigana over a headword (LKP-050): the expression is split into kana and
//! kanji groups and the reading is aligned to them; when the alignment is
//! ambiguous, the whole reading goes over the whole expression.
//!
//! Over a word of a subtitle (LKP-051), the text is an inflected form of the
//! headword: the reading goes over the stem they share, and the rest is kana.
//!
//! Port of Yomitan's `distributeFurigana` / `segmentizeFurigana` /
//! `distributeFuriganaInflected` in `ext/js/language/ja/japanese.js`
//! (GPL-3.0), working on code points.

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

/// The length of the prefix `a` and `b` share, in chars.
fn stem_length(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Distributes the reading of the headword `expression` over `source`, the
/// form of it found in a text (LKP-051): `食べた` under `食べる` / `たべる`
/// gives `[食|た][べた]`.
pub fn distribute_inflected(expression: &str, reading: &str, source: &str) -> Vec<Segment> {
    let chars = |s: &str| -> Vec<char> { s.chars().collect() };
    let normalized = |s: &str| chars(&katakana_to_hiragana(s));
    let source_chars = chars(source);
    let source_normalized = normalized(source);
    let mut main = chars(expression);
    let mut reading = chars(reading);
    let mut stem = stem_length(&normalized(expression), &source_normalized);
    // the source may be written in the reading's kana
    let reading_stem = stem_length(
        &normalized(&reading.iter().collect::<String>()),
        &source_normalized,
    );
    if reading_stem > 0 && reading_stem >= stem {
        main.clone_from(&reading);
        stem = reading_stem;
        reading.splice(..stem, source_chars[..stem].iter().copied());
    }
    let mut segments = Vec::new();
    if stem > 0 {
        main.splice(..stem, source_chars[..stem].iter().copied());
        let main: String = main.iter().collect();
        let mut consumed = 0;
        for s in distribute(&main, &reading.iter().collect::<String>()) {
            let start = consumed;
            consumed += s.text.chars().count();
            if consumed <= stem {
                segments.push(s);
                if consumed == stem {
                    break;
                }
            } else {
                if start < stem {
                    segments.push(Segment {
                        text: main.chars().skip(start).take(stem - start).collect(),
                        reading: String::new(),
                    });
                }
                break;
            }
        }
    }
    if stem < source_chars.len() {
        let rest: String = source_chars[stem..].iter().collect();
        match segments.last_mut() {
            Some(last) if last.reading.is_empty() => last.text.push_str(&rest),
            _ => segments.push(Segment {
                text: rest,
                reading: String::new(),
            }),
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(expression: &str, reading: &str) -> String {
        shown(&distribute(expression, reading))
    }

    fn shown(segments: &[Segment]) -> String {
        segments
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

    // T-LKP-051
    #[test]
    fn an_inflected_form_takes_the_reading_of_its_stem() {
        let show = |e, r, s| shown(&distribute_inflected(e, r, s));
        assert_eq!(show("食べる", "たべる", "食べた"), "[食|た][べた]");
        assert_eq!(show("勉強", "べんきょう", "勉強"), "[勉強|べんきょう]");
        assert_eq!(show("読む", "よむ", "読んでいる"), "[読|よ][んでいる]");
        // a kanji whose okurigana changes: no reading over a split group
        assert_eq!(show("来る", "くる", "来ない"), "[来|く][ない]");
        // written in kana: no reading at all
        assert_eq!(show("食べる", "たべる", "たべた"), "[たべた]");
        // katakana for hiragana
        assert_eq!(show("する", "する", "スル"), "[スル]");
        // nothing shared
        assert_eq!(show("行く", "いく", "ゆく"), "[ゆく]");
    }
}
