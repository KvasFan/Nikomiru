// SPDX-License-Identifier: GPL-3.0-only

//! Minimal deinflector with the same structure as Yomitan's (LKP-020…022): suffix
//! rules with input and output conditions as bit masks, applied recursively up to
//! depth 10.
//!
//! The rules approximate Yomitan's Japanese tables (~130 rules) only to measure the
//! cost of a lookup: real deinflection arrives in M3 with the
//! `assets/transforms/ja.json` tables.

pub const V1: u32 = 1 << 0;
pub const V5: u32 = 1 << 1;
pub const VK: u32 = 1 << 2;
pub const VS: u32 = 1 << 3;
pub const ADJ_I: u32 = 1 << 4;
pub const MASU: u32 = 1 << 5;
pub const TE: u32 = 1 << 6;
pub const TA: u32 = 1 << 7;

/// A rule: `suffix_in` → `suffix_out` if the current conditions intersect
/// `cond_in` (or if the text is the original one, with conditions 0).
#[derive(Debug, Clone)]
pub struct Rule {
    pub suffix_in: String,
    pub suffix_out: String,
    pub cond_in: u32,
    pub cond_out: u32,
}

/// A deinflected form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deinflection {
    pub text: String,
    pub conditions: u32,
    pub trace: u8,
}

/// The conditions of a term, from the dictionary's `rules` field (LKP-023).
pub fn rules_mask(rules: &str) -> u32 {
    rules
        .split_whitespace()
        .map(|r| match r {
            "v1" => V1,
            "v5" => V5,
            "vk" => VK,
            "vs" => VS,
            "adj-i" => ADJ_I,
            _ => 0,
        })
        .fold(0, |a, b| a | b)
}

/// Godan: ending, i-row, a-row, e-row, o-row, -te form, -ta form.
const GODAN: [(&str, &str, &str, &str, &str, &str, &str); 9] = [
    ("く", "き", "か", "け", "こ", "いて", "いた"),
    ("ぐ", "ぎ", "が", "げ", "ご", "いで", "いだ"),
    ("す", "し", "さ", "せ", "そ", "して", "した"),
    ("つ", "ち", "た", "て", "と", "って", "った"),
    ("ぬ", "に", "な", "ね", "の", "んで", "んだ"),
    ("ぶ", "び", "ば", "べ", "ぼ", "んで", "んだ"),
    ("む", "み", "ま", "め", "も", "んで", "んだ"),
    ("る", "り", "ら", "れ", "ろ", "って", "った"),
    ("う", "い", "わ", "え", "お", "って", "った"),
];

pub struct Deinflector {
    /// Rules grouped by the last character of `suffix_in`.
    by_last: std::collections::HashMap<char, Vec<Rule>>,
    pub rule_count: usize,
}

impl Deinflector {
    pub fn japanese() -> Self {
        let mut rules = Vec::new();
        let mut r = |i: &str, o: &str, ci: u32, co: u32| {
            rules.push(Rule {
                suffix_in: i.into(),
                suffix_out: o.into(),
                cond_in: ci,
                cond_out: co,
            })
        };
        // polite forms
        r("ました", "ます", 0, MASU);
        r("ません", "ます", 0, MASU);
        r("ましょう", "ます", 0, MASU);
        r("ます", "る", MASU, V1);
        r("します", "する", MASU, VS);
        r("きます", "くる", MASU, VK);
        // ichidan
        r("ない", "る", ADJ_I, V1);
        r("た", "る", TA, V1);
        r("て", "る", TE, V1);
        r("られる", "る", V1, V1);
        r("させる", "る", V1, V1);
        r("たい", "る", ADJ_I, V1);
        r("よう", "る", 0, V1);
        r("れば", "る", 0, V1);
        r("ろ", "る", 0, V1);
        // progressive
        r("ている", "て", V1, TE);
        r("てる", "て", V1, TE);
        r("でいる", "で", V1, TE);
        // suru / kuru
        for (i, o, ci, co) in [
            ("した", "する", TA, VS),
            ("して", "する", TE, VS),
            ("しない", "する", ADJ_I, VS),
            ("される", "する", V1, VS),
            ("させる", "する", V1, VS),
            ("したい", "する", ADJ_I, VS),
            ("しよう", "する", 0, VS),
            ("すれば", "する", 0, VS),
            ("きた", "くる", TA, VK),
            ("きて", "くる", TE, VK),
            ("こない", "くる", ADJ_I, VK),
        ] {
            r(i, o, ci, co);
        }
        // godan
        for (u, i, a, e, o, te, ta) in GODAN {
            r(&format!("{i}ます"), u, MASU, V5);
            r(&format!("{a}ない"), u, ADJ_I, V5);
            r(ta, u, TA, V5);
            r(te, u, TE, V5);
            r(&format!("{a}れる"), u, V1, V5);
            r(&format!("{a}せる"), u, V1, V5);
            r(&format!("{i}たい"), u, ADJ_I, V5);
            r(&format!("{o}う"), u, 0, V5);
            r(&format!("{e}ば"), u, 0, V5);
            r(e, u, 0, V5);
            r(&format!("{e}る"), u, V1, V5);
        }
        // -i adjectives
        for (i, o, ci) in [
            ("かった", "い", TA),
            ("くない", "い", ADJ_I),
            ("くて", "い", TE),
            ("ければ", "い", 0),
            ("く", "い", 0),
            ("さ", "い", 0),
            ("そう", "い", 0),
        ] {
            r(i, o, ci, ADJ_I);
        }
        // generic past and -te forms (Yomitan: -た, -て on any verb)
        r("なかった", "ない", TA, ADJ_I);
        r("なくて", "ない", TE, ADJ_I);

        let rule_count = rules.len();
        let mut by_last: std::collections::HashMap<char, Vec<Rule>> = Default::default();
        for rule in rules {
            if let Some(c) = rule.suffix_in.chars().last() {
                by_last.entry(c).or_default().push(rule);
            }
        }
        Self {
            by_last,
            rule_count,
        }
    }

    /// All deinflected forms of `text`, including `text` itself (LKP-022).
    pub fn deinflect(&self, text: &str) -> Vec<Deinflection> {
        let mut out = vec![Deinflection {
            text: text.to_string(),
            conditions: 0,
            trace: 0,
        }];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i].clone();
            i += 1;
            if cur.trace >= 10 {
                continue;
            }
            let Some(last) = cur.text.chars().last() else {
                continue;
            };
            for rule in self.by_last.get(&last).into_iter().flatten() {
                if cur.conditions != 0 && cur.conditions & rule.cond_in == 0 {
                    continue;
                }
                if !cur.text.ends_with(&rule.suffix_in) {
                    continue;
                }
                let stem = &cur.text[..cur.text.len() - rule.suffix_in.len()];
                if stem.is_empty() && rule.suffix_out.is_empty() {
                    continue;
                }
                let text = format!("{stem}{}", rule.suffix_out);
                if !out
                    .iter()
                    .any(|d| d.text == text && d.conditions == rule.cond_out)
                {
                    out.push(Deinflection {
                        text,
                        conditions: rule.cond_out,
                        trace: cur.trace + 1,
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(text: &str) -> Vec<(String, u32)> {
        Deinflector::japanese()
            .deinflect(text)
            .into_iter()
            .map(|d| (d.text, d.conditions))
            .collect()
    }

    #[test]
    fn common_forms() {
        assert!(forms("食べました").contains(&("食べる".into(), V1)));
        assert!(forms("行かなかった").contains(&("行く".into(), V5)));
        assert!(forms("読んでいる").contains(&("読む".into(), V5)));
        assert!(forms("書いた").contains(&("書く".into(), V5)));
        assert!(forms("高くない").contains(&("高い".into(), ADJ_I)));
        assert!(forms("勉強します").contains(&("勉強する".into(), VS)));
    }
}
