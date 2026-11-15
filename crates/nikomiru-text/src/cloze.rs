// SPDX-License-Identifier: GPL-3.0-only

//! Conservative cloze answers: canonical composition and outer whitespace only (CLO-002).
/// NFC answer key, preserving case, punctuation, script and kana distinctions.
pub fn answer(text: &str) -> String {
    icu_normalizer::ComposingNormalizer::new_nfc()
        .normalize(text.trim())
        .into_owned()
}
/// Mask every canonically equivalent literal occurrence; never leave another copy of the answer visible.
pub fn mask(context: &str, value: &str) -> Option<String> {
    let key = answer(value);
    if key.is_empty() {
        return None;
    }
    let context = icu_normalizer::ComposingNormalizer::new_nfc().normalize(context);
    context.contains(&key).then(|| context.replace(&key, "[…]"))
}
#[cfg(test)]
mod tests {
    use super::*;
    // T-CLO-001: composed/decomposed forms match, but homophones, scripts, case and punctuation do not.
    #[test]
    fn canonical_answers_and_every_occurrence_are_conservative() {
        assert_eq!(answer("\u{2003}cafe\u{301} \n"), answer("café"));
        for (a, b) in [
            ("猫", "ねこ"),
            ("ねこ", "ネコ"),
            ("A", "a"),
            ("a!", "a"),
            ("１", "1"),
        ] {
            assert_ne!(answer(a), answer(b));
        }
        assert_eq!(
            mask("café et cafe\u{301}", "café").as_deref(),
            Some("[…] et […]")
        );
        assert_eq!(mask("猫と猫です", "猫").as_deref(), Some("[…]と[…]です"));
        assert!(mask("犬です", "猫").is_none());
        assert!(mask("猫です", "").is_none());
    }
}
