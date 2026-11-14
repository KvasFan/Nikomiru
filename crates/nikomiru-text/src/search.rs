// SPDX-License-Identifier: GPL-3.0-only

//! Display-list search, independent of dictionary lookup preprocessing (KNW-026).

fn fold(text: &str) -> String {
    let normal = icu_normalizer::ComposingNormalizer::new_nfkc().normalize(text);
    normal
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if ('\u{30a1}'..='\u{30f6}').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// A query normalised once; every whitespace-separated term must match.
#[derive(Debug)]
pub struct Query {
    terms: Vec<String>,
}

impl Query {
    /// Fold widths, composition, lowercase and basic kana script.
    pub fn new(query: &str) -> Self {
        Self {
            terms: fold(query).split_whitespace().map(str::to_string).collect(),
        }
    }

    /// Match across original display fields without changing their bytes/identity.
    pub fn matches(&self, fields: &[&str]) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let fields: Vec<_> = fields.iter().map(|field| fold(field)).collect();
        self.terms
            .iter()
            .all(|term| fields.iter().any(|field| field.contains(term)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-026: original spelling/offsets are never a product of search folding.
    #[test]
    fn width_kana_case_composition_and_terms_share_one_matching_rule() {
        let fields = ["食べる ＡＢＣ", "タベル ｶﾞｯｺｳ コーヒー"];
        for q in [
            "たべる",
            "ﾀﾍﾞﾙ",
            "abc",
            "ＡｂＣ",
            "がっこう",
            "カ\u{3099}ッコウ",
            "こーひー",
            "食べる タベル abc",
            "　 ",
        ] {
            assert!(Query::new(q).matches(&fields), "{q}");
        }
        assert!(!Query::new("食べる 犬").matches(&fields));
        assert!(!Query::new("taberu").matches(&fields));
        assert_eq!(fields[0], "食べる ＡＢＣ");
    }
}
