// SPDX-License-Identifier: GPL-3.0-only

//! Hangul syllables ↔ compatibility jamo (BUG-2148): Yomitan's Korean
//! deinflection tables are written in simple jamo (`ㅂㄷㅏ`), so the text is
//! split into jamo before the rules and put back together before the
//! dictionary query.
//!
//! Port of Fushi's `native/fushidicts/fushidicts_src/text_processor/
//! text_processor.cpp` (commit ac8034e), after Yomitan's `korean.js` (GPL-3.0).

const LEAD: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
const VOWEL_BASE: u32 = 0x314F;
const VOWEL_LAST: u32 = 0x3163;
const TAIL: [Option<char>; 28] = [
    None,
    Some('ㄱ'),
    Some('ㄲ'),
    Some('ㄳ'),
    Some('ㄴ'),
    Some('ㄵ'),
    Some('ㄶ'),
    Some('ㄷ'),
    Some('ㄹ'),
    Some('ㄺ'),
    Some('ㄻ'),
    Some('ㄼ'),
    Some('ㄽ'),
    Some('ㄾ'),
    Some('ㄿ'),
    Some('ㅀ'),
    Some('ㅁ'),
    Some('ㅂ'),
    Some('ㅄ'),
    Some('ㅅ'),
    Some('ㅆ'),
    Some('ㅇ'),
    Some('ㅈ'),
    Some('ㅊ'),
    Some('ㅋ'),
    Some('ㅌ'),
    Some('ㅍ'),
    Some('ㅎ'),
];
const SYLLABLE_FIRST: u32 = 0xAC00;
const SYLLABLE_LAST: u32 = 0xD7A3;
const VOWELS: u32 = 21;
const TAILS: u32 = 28;

/// Compound jamo and the two simple ones they are made of.
const COMPOUND: [(char, char, char); 18] = [
    ('ㄳ', 'ㄱ', 'ㅅ'),
    ('ㄵ', 'ㄴ', 'ㅈ'),
    ('ㄶ', 'ㄴ', 'ㅎ'),
    ('ㄺ', 'ㄹ', 'ㄱ'),
    ('ㄻ', 'ㄹ', 'ㅁ'),
    ('ㄼ', 'ㄹ', 'ㅂ'),
    ('ㄽ', 'ㄹ', 'ㅅ'),
    ('ㄾ', 'ㄹ', 'ㅌ'),
    ('ㄿ', 'ㄹ', 'ㅍ'),
    ('ㅀ', 'ㄹ', 'ㅎ'),
    ('ㅄ', 'ㅂ', 'ㅅ'),
    ('ㅘ', 'ㅗ', 'ㅏ'),
    ('ㅙ', 'ㅗ', 'ㅐ'),
    ('ㅚ', 'ㅗ', 'ㅣ'),
    ('ㅝ', 'ㅜ', 'ㅓ'),
    ('ㅞ', 'ㅜ', 'ㅔ'),
    ('ㅟ', 'ㅜ', 'ㅣ'),
    ('ㅢ', 'ㅡ', 'ㅣ'),
];

fn push_simple(out: &mut String, jamo: char) {
    match COMPOUND.iter().find(|(c, _, _)| *c == jamo) {
        Some((_, a, b)) => {
            out.push(*a);
            out.push(*b);
        }
        None => out.push(jamo),
    }
}

fn compose(first: char, second: char) -> Option<char> {
    COMPOUND
        .iter()
        .find(|(_, a, b)| *a == first && *b == second)
        .map(|(c, _, _)| *c)
}

fn is_vowel(c: char) -> bool {
    (VOWEL_BASE..=VOWEL_LAST).contains(&(c as u32))
}

fn lead_index(c: char) -> Option<u32> {
    LEAD.iter().position(|l| *l == c).map(|i| i as u32)
}

fn tail_index(c: char) -> Option<u32> {
    TAIL.iter().position(|t| *t == Some(c)).map(|i| i as u32)
}

/// Syllables (and compound jamo) → simple compatibility jamo. Text without
/// Hangul comes back unchanged.
pub fn disassemble(text: &str) -> String {
    let has_hangul = text.chars().any(|c| {
        let u = c as u32;
        (SYLLABLE_FIRST..=SYLLABLE_LAST).contains(&u) || (0x3130..=0x318F).contains(&u)
    });
    if !has_hangul {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() * 3);
    for c in text.chars() {
        let u = c as u32;
        if (SYLLABLE_FIRST..=SYLLABLE_LAST).contains(&u) {
            let s = u - SYLLABLE_FIRST;
            let lead = s / (VOWELS * TAILS);
            let vowel = s % (VOWELS * TAILS) / TAILS;
            let tail = s % TAILS;
            push_simple(&mut out, LEAD[lead as usize]);
            push_simple(&mut out, char::from_u32(VOWEL_BASE + vowel).unwrap_or(c));
            if let Some(t) = TAIL[tail as usize] {
                push_simple(&mut out, t);
            }
        } else {
            push_simple(&mut out, c);
        }
    }
    out
}

/// Simple compatibility jamo → syllables. A consonant after a vowel ends the
/// syllable only when no vowel follows it (otherwise it starts the next one).
pub fn reassemble(text: &str) -> String {
    let t: Vec<char> = text.chars().collect();
    let n = t.len();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < n {
        let Some(lead) = lead_index(t[i]).filter(|_| i + 1 < n) else {
            out.push(t[i]);
            i += 1;
            continue;
        };
        let (mut vowel, mut vowel_len) = (t[i + 1], 1);
        if i + 2 < n
            && let Some(merged) = compose(t[i + 1], t[i + 2])
            && is_vowel(merged)
        {
            vowel = merged;
            vowel_len = 2;
        }
        if !is_vowel(vowel) {
            out.push(t[i]);
            i += 1;
            continue;
        }
        let j = i + 1 + vowel_len;
        let (mut tail, mut tail_len) = (0, 0);
        if j < n {
            if j + 1 < n
                && let Some(merged) = compose(t[j], t[j + 1])
                && let Some(ti) = tail_index(merged).filter(|ti| *ti != 0)
                && (j + 2 >= n || !is_vowel(t[j + 2]))
            {
                tail = ti;
                tail_len = 2;
            }
            if tail_len == 0
                && let Some(ti) = tail_index(t[j]).filter(|ti| *ti != 0)
                && (j + 1 >= n || !is_vowel(t[j + 1]))
            {
                tail = ti;
                tail_len = 1;
            }
        }
        let v = vowel as u32 - VOWEL_BASE;
        let syllable = SYLLABLE_FIRST + (lead * VOWELS + v) * TAILS + tail;
        out.push(char::from_u32(syllable).unwrap_or(t[i]));
        i = j + tail_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(disassemble("부드러운"), "ㅂㅜㄷㅡㄹㅓㅇㅜㄴ");
        assert_eq!(reassemble("ㅂㅜㄷㅡㄹㅓㅂㄷㅏ"), "부드럽다");
        assert_eq!(disassemble("곿"), "ㄱㅗㅏㄱㅅ");
        assert_eq!(reassemble(&disassemble("하고 읽었다")), "하고 읽었다");
        assert_eq!(disassemble("日本"), "日本");
    }
}
