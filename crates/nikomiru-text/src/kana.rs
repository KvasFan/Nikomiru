// SPDX-License-Identifier: GPL-3.0-only

//! Kana conversions and character classes.
//!
//! Port of Fushi's `native/fushidicts/fushidicts_src/text_processor/
//! text_processor.cpp` (commit ac8034e), which follows Yomitan's
//! `ext/js/language/ja/japanese.js` (GPL-3.0).

const HIRAGANA_START: u32 = 0x3041;
const HIRAGANA_END: u32 = 0x3096;
const KATAKANA_START: u32 = 0x30A1;
const KATAKANA_END: u32 = 0x30F6;
const KATAKANA_SMALL_KA: char = '\u{30F5}';
const KATAKANA_SMALL_KE: char = '\u{30F6}';
/// `ー`.
pub const PROLONGED_SOUND_MARK: char = '\u{30FC}';

/// The vowel of a kana (`'a'`, `'i'`, `'u'`, `'e'`, `'o'`), if it has one.
pub fn vowel_of(kana: char) -> Option<char> {
    const ROWS: [(char, &str); 5] = [
        (
            'a',
            "ぁあかがさざただなはばぱまゃやらゎわヵァアカガサザタダナハバパマャヤラヮワヷ",
        ),
        (
            'i',
            "ぃいきぎしじちぢにひびぴみりゐィイキギシジチヂニヒビピミリヰヸ",
        ),
        (
            'u',
            "ぅうくぐすずっつづぬふぶぷむゅゆるゥウクグスズッツヅヌフブプムュユルヴ",
        ),
        (
            'e',
            "ぇえけげせぜてでねへべぺめれゑヶェエケゲセゼテデネヘベペメレヱヹ",
        ),
        (
            'o',
            "ぉおこごそぞとどのほぼぽもょよろをォオコゴソゾトドノホボポモョヨロヲヺ",
        ),
    ];
    ROWS.iter()
        .find(|(_, row)| row.contains(kana))
        .map(|(v, _)| *v)
}

/// The hiragana that `ー` stands for after `prev` (`o` rows give `う`, as in
/// Yomitan).
fn prolonged_hiragana(prev: char) -> Option<char> {
    match vowel_of(prev)? {
        'a' => Some('あ'),
        'i' => Some('い'),
        'u' | 'o' => Some('う'),
        'e' => Some('え'),
        _ => None,
    }
}

fn shift(c: char, from: (u32, u32), to: u32) -> char {
    let u = c as u32;
    if (from.0..=from.1).contains(&u) {
        char::from_u32(u - from.0 + to).unwrap_or(c)
    } else {
        c
    }
}

/// Hiragana → katakana.
pub fn hiragana_to_katakana(text: &str) -> String {
    text.chars()
        .map(|c| shift(c, (HIRAGANA_START, HIRAGANA_END), KATAKANA_START))
        .collect()
}

/// Katakana → hiragana: `ー` becomes the vowel of the kana before it, `ヵ` and
/// `ヶ` stay as they are (they are counters, not kana, in most words).
pub fn katakana_to_hiragana(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last: Option<char> = None;
    for c in text.chars() {
        let converted = match c {
            KATAKANA_SMALL_KA | KATAKANA_SMALL_KE => c,
            PROLONGED_SOUND_MARK => last.and_then(prolonged_hiragana).unwrap_or(c),
            _ => shift(c, (KATAKANA_START, KATAKANA_END), HIRAGANA_START),
        };
        out.push(converted);
        last = Some(converted);
    }
    out
}

/// Hiragana (including the iteration marks `ゝゞ`).
pub fn is_hiragana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | '\u{309D}'..='\u{309F}')
}

/// Katakana, full and half width, with `ー`.
pub fn is_katakana(c: char) -> bool {
    matches!(c, '\u{30A1}'..='\u{30FA}' | '\u{30FC}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9D}')
}

/// Hiragana or katakana.
pub fn is_kana(c: char) -> bool {
    is_hiragana(c) || is_katakana(c)
}

/// A CJK ideograph (kanji), or `々`.
pub fn is_kanji(c: char) -> bool {
    matches!(c,
        '\u{3005}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
        | '\u{20000}'..='\u{2A6DF}' | '\u{2A700}'..='\u{2EBEF}' | '\u{30000}'..='\u{3134F}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // T-LKP-011-a
    #[test]
    fn katakana_to_hiragana_with_long_vowels() {
        assert_eq!(katakana_to_hiragana("カタカナ"), "かたかな");
        assert_eq!(katakana_to_hiragana("ラーメン"), "らあめん");
        assert_eq!(katakana_to_hiragana("ヵヶ"), "ヵヶ");
        assert_eq!(katakana_to_hiragana("コーヒー"), "こうひい");
        // a leading ー has no vowel to copy
        assert_eq!(katakana_to_hiragana("ーア"), "ーあ");
        assert_eq!(hiragana_to_katakana("ひらがな"), "ヒラガナ");
    }

    proptest! {
        // T-TXT-001
        #[test]
        fn hiragana_round_trips_through_katakana(s in "[\u{3041}-\u{3094}]{0,20}") {
            // ゕゖ (U+3095, U+3096) are excluded: as ヵヶ they stay katakana
            prop_assert_eq!(katakana_to_hiragana(&hiragana_to_katakana(&s)), s);
        }
    }

    #[test]
    fn classes() {
        assert!(is_kanji('日') && is_kanji('々') && !is_kanji('あ'));
        assert!(is_kana('ゝ') && is_kana('ｱ') && !is_kana('a'));
    }
}
