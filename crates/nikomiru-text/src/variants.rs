// SPDX-License-Identifier: GPL-3.0-only

//! The text variants a lookup tries (LKP-010…LKP-012). Every preprocessor has
//! options; the Cartesian product of the options is deduplicated, keeping for
//! each variant the minimum number of steps (one per preprocessor that changed
//! the text).
//!
//! Port of Fushi's `native/fushidicts/fushidicts_src/text_processor/
//! text_processor.cpp` (commit ac8034e), which follows Yomitan's
//! `japanese-text-preprocessors.js` and `translator.js` (GPL-3.0). Unlike Fushi,
//! the chains of other languages run only for the study language or a
//! dictionary's language (LKP-012).

use std::collections::BTreeMap;

use icu_normalizer::{ComposingNormalizer, ComposingNormalizerBorrowed};

use crate::hangul;
use crate::kana::{self, PROLONGED_SOUND_MARK};
use crate::kanji_variants::KANJI_VARIANTS;

/// A variant and the number of preprocessors that changed the text to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// The text.
    pub text: String,
    /// Preprocessors that changed it (0 = the original text).
    pub steps: u32,
}

/// Groups of preprocessors, in chain order (LKP-011, LKP-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// NFKC first: half-width katakana and full-width Latin become standard.
    Normalize,
    /// Japanese: kana, emphatic sequences, width, kanji variants, iteration
    /// marks, numerals.
    Japanese,
    /// Latin, Greek and Cyrillic: apostrophes and lower case.
    Letters,
    /// Diacritics, Arabic harakat, Hebrew points.
    Diacritics,
    /// Korean jamo, always last: NFKC would compose them back (BUG-2148).
    Korean,
}

/// Every group, in chain order: Fushi's behaviour.
pub const ALL_GROUPS: [Group; 5] = [
    Group::Normalize,
    Group::Japanese,
    Group::Letters,
    Group::Diacritics,
    Group::Korean,
];

/// The groups for a language (ISO 639-1), LKP-012.
pub fn groups_for(language: &str) -> Vec<Group> {
    match language {
        "ja" => vec![Group::Normalize, Group::Japanese],
        "ko" => vec![Group::Normalize, Group::Korean],
        "zh" => vec![Group::Normalize],
        "ar" | "fa" | "he" | "yi" => vec![Group::Normalize, Group::Diacritics],
        _ => vec![Group::Normalize, Group::Letters, Group::Diacritics],
    }
}

/// The union of the groups of several languages, in chain order.
pub fn groups_for_all<'a>(languages: impl IntoIterator<Item = &'a str>) -> Vec<Group> {
    let mut groups: Vec<Group> = languages.into_iter().flat_map(groups_for).collect();
    groups.sort();
    groups.dedup();
    groups
}

type Process = fn(&str, u8) -> String;

/// A preprocessor: its options (0 = unchanged) and the function.
struct Preprocessor {
    options: &'static [u8],
    process: Process,
}

fn nfkc(text: &str) -> String {
    let n: ComposingNormalizerBorrowed<'static> = ComposingNormalizer::new_nfkc();
    n.normalize(text).into_owned()
}

/// `ゞ`/`ヾ`: the previous kana with a dakuten, composed with NFC.
fn add_dakuten(kana: char) -> char {
    let pair: String = [kana, '\u{3099}'].iter().collect();
    let n: ComposingNormalizerBorrowed<'static> = ComposingNormalizer::new_nfc();
    let composed = n.normalize(&pair);
    let mut chars = composed.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c,
        _ => kana,
    }
}

fn is_emphatic(c: char) -> bool {
    matches!(c, 'っ' | 'ッ' | PROLONGED_SOUND_MARK)
}

/// `っっ` → `っ`, `ーー` → `ー` in the middle of the text; never removes the
/// last one (BUG-1777: the "full collapse" swallows real words).
fn collapse_emphatic(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let left = chars.iter().take_while(|c| is_emphatic(**c)).count();
    let right = chars.len() - chars.iter().rev().take_while(|c| is_emphatic(**c)).count();
    if left >= right {
        return text.to_string();
    }
    let mut out: String = chars[..left].iter().collect();
    let mut current: Option<char> = None;
    for &c in &chars[left..right] {
        if is_emphatic(c) {
            if current != Some(c) {
                current = Some(c);
                out.push(c);
            }
        } else {
            current = None;
            out.push(c);
        }
    }
    out.extend(&chars[right..]);
    out
}

fn alphanumeric_to_fullwidth(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '0'..='9' | 'A'..='Z' | 'a'..='z' => char::from_u32(c as u32 + 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .collect()
}

fn standardize_kanji(text: &str) -> String {
    text.chars()
        .map(|c| {
            KANJI_VARIANTS
                .binary_search_by_key(&(c as u32), |&(v, _)| v)
                .ok()
                .and_then(|i| char::from_u32(KANJI_VARIANTS[i].1))
                .unwrap_or(c)
        })
        .collect()
}

/// `佐々木` → `佐佐木`, `こゝろ` → `こころ`, `こゞ` → `こご`.
fn expand_iteration_marks(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        if let Some(&mark) = chars.get(i + 1) {
            let expanded = match mark {
                '々' | 'ゝ' | 'ヽ' => Some(c),
                'ゞ' | 'ヾ' => Some(add_dakuten(c)),
                _ => None,
            };
            if let Some(e) = expanded {
                out.push(e);
                i += 1;
            }
        }
        i += 1;
    }
    out
}

/// Full-width digits → kanji numerals (`２` → `二`).
fn numbers_to_kanji(text: &str) -> String {
    const KANJI: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    text.chars()
        .map(|c| match c {
            '０'..='９' => KANJI[(c as u32 - '０' as u32) as usize],
            _ => c,
        })
        .collect()
}

/// Lower case for ASCII, Latin-1, Greek and Cyrillic (no language rules: the
/// Turkish dotless i is out of scope, as in Fushi).
fn to_lowercase(text: &str) -> String {
    text.chars()
        .map(|c| {
            let u = c as u32;
            let lower = match u {
                0x41..=0x5A
                | 0xC0..=0xD6
                | 0xD8..=0xDE
                | 0x391..=0x3A1
                | 0x3A3..=0x3AB
                | 0x410..=0x42F => u + 0x20,
                0x400..=0x40F => u + 0x50,
                _ => u,
            };
            char::from_u32(lower).unwrap_or(c)
        })
        .collect()
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{02BC}')
}

/// BUG-2056: `don’t` and `don't` both reach dictionaries written either way.
fn apostrophes(text: &str, to: char) -> String {
    text.chars()
        .map(|c| if is_apostrophe(c) { to } else { c })
        .collect()
}

fn is_combining_to_strip(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x064B..=0x065F | 0x0670 | 0x0640 | 0x0591..=0x05BD
        | 0x05BF | 0x05C1 | 0x05C2 | 0x05C4 | 0x05C5 | 0x05C7)
}

fn precomposed_base(c: char) -> char {
    match c {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'Ç' => 'C',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'Ñ' => 'N',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'O',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'Ý' => 'Y',
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

fn identity_or(text: &str, option: u8, f: impl Fn(&str) -> String) -> String {
    if option == 1 {
        f(text)
    } else {
        text.to_string()
    }
}

const BINARY: &[u8] = &[0, 1];
const TERNARY: &[u8] = &[0, 1, 2];

fn preprocessors(group: Group) -> Vec<Preprocessor> {
    match group {
        Group::Normalize => vec![Preprocessor {
            options: BINARY,
            process: |t, o| identity_or(t, o, nfkc),
        }],
        Group::Japanese => vec![
            Preprocessor {
                options: TERNARY,
                process: |t, o| match o {
                    1 => kana::katakana_to_hiragana(t),
                    2 => kana::hiragana_to_katakana(t),
                    _ => t.to_string(),
                },
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, collapse_emphatic),
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, alphanumeric_to_fullwidth),
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, standardize_kanji),
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, expand_iteration_marks),
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, numbers_to_kanji),
            },
        ],
        Group::Letters => vec![
            Preprocessor {
                options: TERNARY,
                process: |t, o| match o {
                    1 => apostrophes(t, '\''),
                    2 => apostrophes(t, '\u{2019}'),
                    _ => t.to_string(),
                },
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, to_lowercase),
            },
        ],
        Group::Diacritics => vec![
            Preprocessor {
                options: BINARY,
                process: |t, o| {
                    identity_or(t, o, |t| {
                        t.chars().filter(|c| !is_combining_to_strip(*c)).collect()
                    })
                },
            },
            Preprocessor {
                options: BINARY,
                process: |t, o| identity_or(t, o, |t| t.chars().map(precomposed_base).collect()),
            },
        ],
        Group::Korean => vec![Preprocessor {
            options: BINARY,
            process: |t, o| identity_or(t, o, hangul::disassemble),
        }],
    }
}

/// The variants of `text` for the preprocessor `groups` (in chain order),
/// sorted by text, each with its minimum number of steps.
pub fn variants(text: &str, groups: &[Group]) -> Vec<Variant> {
    let mut current: BTreeMap<String, u32> = BTreeMap::from([(text.to_string(), 0)]);
    let mut ordered: Vec<Group> = groups.to_vec();
    ordered.sort();
    ordered.dedup();
    for group in ordered {
        for p in preprocessors(group) {
            let mut next: BTreeMap<String, u32> = BTreeMap::new();
            for (variant, steps) in &current {
                for &option in p.options {
                    let processed = (p.process)(variant, option);
                    let s = if processed == *variant {
                        *steps
                    } else {
                        steps + 1
                    };
                    next.entry(processed)
                        .and_modify(|old| *old = (*old).min(s))
                        .or_insert(s);
                }
            }
            current = next;
        }
    }
    current
        .into_iter()
        .map(|(text, steps)| Variant { text, steps })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn japanese(text: &str) -> Vec<String> {
        variants(text, &groups_for("ja"))
            .into_iter()
            .map(|v| v.text)
            .collect()
    }

    // T-LKP-011-b
    #[test]
    fn nfkc_folds_widths() {
        assert!(japanese("ｶﾀｶﾅ").contains(&"カタカナ".to_string()));
        assert!(japanese("１２３").contains(&"123".to_string()));
    }

    // T-LKP-011-c
    #[test]
    fn emphatic_sequences_collapse_but_never_vanish() {
        let v = japanese("すっっごい");
        assert!(v.contains(&"すっごい".to_string()));
        assert!(!v.contains(&"すごい".to_string()), "BUG-1777");
    }

    // T-LKP-011-d
    #[test]
    fn iteration_marks_expand() {
        assert!(japanese("時々").contains(&"時時".to_string()));
        assert!(japanese("こゝろ").contains(&"こころ".to_string()));
        assert!(japanese("こゞ").contains(&"こご".to_string()));
    }

    // T-LKP-011-e
    #[test]
    fn digits_become_kanji_numerals() {
        assert!(japanese("３").contains(&"三".to_string()));
        assert!(japanese("2月").contains(&"二月".to_string()));
    }

    #[test]
    fn kanji_variants_are_standardized() {
        // 髙 (U+9AD9) is a variant of 高
        assert!(japanese("髙い").contains(&"高い".to_string()));
    }

    // T-LKP-012
    #[test]
    fn other_chains_only_for_their_languages() {
        let ja = japanese("Café");
        assert!(!ja.contains(&"cafe".to_string()), "no diacritics for ja");
        let fr: Vec<String> = variants("Café", &groups_for("fr"))
            .into_iter()
            .map(|v| v.text)
            .collect();
        assert!(fr.contains(&"cafe".to_string()));
        let all: Vec<String> = variants("Café", &ALL_GROUPS)
            .into_iter()
            .map(|v| v.text)
            .collect();
        assert!(all.contains(&"cafe".to_string()));
        assert_eq!(
            groups_for_all(["ja", "en"]),
            [
                Group::Normalize,
                Group::Japanese,
                Group::Letters,
                Group::Diacritics
            ]
        );
    }

    proptest! {
        // T-LKP-010
        #[test]
        fn variants_are_unique_and_the_input_has_no_steps(s in "[ぁ-んァ-ン一-龯ーっッ０-９a-zA-Z々ゝ ]{0,8}") {
            let v = variants(&s, &ALL_GROUPS);
            let mut texts: Vec<&str> = v.iter().map(|x| x.text.as_str()).collect();
            let n = texts.len();
            texts.dedup();
            prop_assert_eq!(texts.len(), n);
            let original = v.iter().find(|x| x.text == s);
            prop_assert_eq!(original.map(|x| x.steps), Some(0));
            prop_assert!(v.iter().all(|x| x.text == s || x.steps > 0));
        }
    }
}
