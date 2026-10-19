// SPDX-License-Identifier: GPL-3.0-only

//! Deinflection on Yomitan's language transforms (LKP-020…LKP-024): a port of
//! Fushi's `native/fushidicts/fushidicts_src/deinflector.cpp` (commit ac8034e).
//!
//! Unlike Fushi, which loads every language into one table, a [`Deinflector`]
//! holds one language: rules of other languages never apply and condition bits
//! never collide (LKP-012).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Deserialize;

/// Deepest chain of transforms (LKP-022).
pub const MAX_DEPTH: usize = 10;

/// Most conditions a language can declare (LKP-021).
pub const MAX_CONDITIONS: usize = 64;

/// Languages with transform tables, from `assets/transforms/manifest.json`.
pub const LANGUAGES: &[&str] = &[
    "ar", "de", "el", "en", "eo", "es", "eu", "fr", "ga", "grc", "ja", "ka", "ko", "la", "sga",
    "sq", "tl", "yi",
];

macro_rules! tables {
    ($($lang:literal => [$($file:literal),+]),+ $(,)?) => {
        fn embedded(language: &str) -> &'static [&'static str] {
            match language {
                $($lang => &[$(include_str!(concat!("../../../assets/transforms/", $file, ".json"))),+],)+
                _ => &[],
            }
        }
    };
}

tables! {
    "ar" => ["ar"], "de" => ["de"], "el" => ["el"], "en" => ["en", "en_irregular"],
    "eo" => ["eo"], "es" => ["es"], "eu" => ["eu"], "fr" => ["fr"], "ga" => ["ga"],
    "grc" => ["grc"], "ja" => ["ja"], "ka" => ["ka"], "ko" => ["ko"], "la" => ["la"],
    "sga" => ["sga"], "sq" => ["sq"], "tl" => ["tl"], "yi" => ["yi"],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleJson {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    from_suffix: String,
    #[serde(default)]
    to_suffix: String,
    #[serde(default)]
    from_prefix: String,
    #[serde(default)]
    to_prefix: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    conditions_in: Vec<String>,
    #[serde(default)]
    conditions_out: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransformJson {
    name: String,
    #[serde(default)]
    description: String,
    rules: Vec<RuleJson>,
    /// The suffix rules also apply to the verb of a phrasal verb (`gave up`).
    #[serde(default)]
    phrasal_verb: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConditionJson {
    #[serde(default)]
    sub_conditions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhrasalJson {
    condition: String,
    particles: Vec<String>,
    prepositions: Vec<String>,
}

/// Maps are sorted by key, as in Fushi (`std::map`): the order of the rules
/// decides which trace a tie keeps (LKP-033).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TablesJson {
    language: String,
    conditions: BTreeMap<String, ConditionJson>,
    transforms: BTreeMap<String, TransformJson>,
    phrasal_verbs: Option<PhrasalJson>,
}

/// A transform, as shown in the trace (LKP-024).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transform {
    /// Its name (`-て`, `past`).
    pub name: String,
    /// What it means.
    pub description: String,
}

#[derive(Debug, Clone)]
struct Rule {
    to: String,
    conditions_in: u64,
    conditions_out: u64,
    transform: u16,
    whole_word: bool,
    phrasal: bool,
}

#[derive(Debug, Default)]
struct Phrasal {
    conditions_out: u64,
    particles: HashSet<String>,
    /// Particles and prepositions.
    words: HashSet<String>,
    /// `picked it up` → `picked up`.
    interposed: Vec<Rule>,
}

/// One form that a text may come from (LKP-022).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deinflection {
    /// The form.
    pub text: String,
    /// What the form must be (0 = anything: the text as written).
    pub conditions: u64,
    /// Transforms removed, outermost first (indices for [`Deinflector::transform`]).
    pub trace: Vec<u16>,
}

/// Tables that cannot be used.
#[derive(Debug, thiserror::Error)]
#[error("transform tables: {0}")]
pub struct TablesError(String);

/// The deinflector of one language.
#[derive(Debug, Default)]
pub struct Deinflector {
    language: String,
    conditions: HashMap<String, u64>,
    suffix: HashMap<String, Vec<Rule>>,
    prefix: HashMap<String, Vec<Rule>>,
    max_suffix: usize,
    max_prefix: usize,
    transforms: Vec<Transform>,
    phrasal: Option<Phrasal>,
}

impl Deinflector {
    /// The deinflector of `language` from the embedded tables; `None` for a
    /// language without them.
    pub fn for_language(language: &str) -> Option<Self> {
        let tables = embedded(language);
        if tables.is_empty() {
            return None;
        }
        // the embedded tables are tested (T-LKP-020): they always parse.
        // Georgian's table says `kat`: the dictionaries' code wins.
        let mut d = Self::from_json(tables).ok()?;
        d.language = language.to_string();
        Some(d)
    }

    /// A deinflector from Yomitan transform tables of one language (`en.json`
    /// and `en_irregular.json` together).
    pub fn from_json(tables: &[&str]) -> Result<Self, TablesError> {
        let mut d = Deinflector::default();
        for json in tables {
            let t: TablesJson =
                serde_json::from_str(json).map_err(|e| TablesError(e.to_string()))?;
            d.load(t)?;
        }
        Ok(d)
    }

    fn load(&mut self, t: TablesJson) -> Result<(), TablesError> {
        if self.language.is_empty() {
            self.language = t.language;
        } else if self.language != t.language {
            return Err(TablesError(format!(
                "{} mixed with {}",
                t.language, self.language
            )));
        }
        for name in t.conditions.keys() {
            if !self.conditions.contains_key(name) {
                let bit = self.conditions.len();
                if bit >= MAX_CONDITIONS {
                    return Err(TablesError(format!(
                        "more than {MAX_CONDITIONS} conditions"
                    )));
                }
                self.conditions.insert(name.clone(), 1 << bit);
            }
        }
        // sub-conditions, to a fixed point (LKP-021)
        loop {
            let mut changed = false;
            for (name, c) in &t.conditions {
                let mut bits = self.conditions[name];
                for sub in &c.sub_conditions {
                    bits |= self.conditions.get(sub).copied().unwrap_or(0);
                }
                if bits != self.conditions[name] {
                    self.conditions.insert(name.clone(), bits);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mask = |names: &[String]| {
            names
                .iter()
                .filter_map(|n| self.conditions.get(n))
                .fold(0, |a, b| a | b)
        };
        if let Some(p) = &t.phrasal_verbs {
            let phrasal = self.phrasal.get_or_insert_with(Phrasal::default);
            phrasal.conditions_out = mask(std::slice::from_ref(&p.condition));
            phrasal.particles.extend(p.particles.iter().cloned());
            phrasal
                .words
                .extend(p.particles.iter().chain(&p.prepositions).cloned());
        }
        for transform in t.transforms.into_values() {
            let id = u16::try_from(self.transforms.len())
                .map_err(|_| TablesError("too many transforms".into()))?;
            for r in &transform.rules {
                let rule = |to: &str, whole_word: bool| Rule {
                    to: to.to_string(),
                    conditions_in: mask(&r.conditions_in),
                    conditions_out: mask(&r.conditions_out),
                    transform: id,
                    whole_word,
                    phrasal: transform.phrasal_verb,
                };
                match r.kind.as_str() {
                    "suffix" => {
                        self.max_suffix = self.max_suffix.max(r.from_suffix.chars().count());
                        let rule = rule(&r.to_suffix, false);
                        self.suffix
                            .entry(r.from_suffix.clone())
                            .or_default()
                            .push(rule);
                    }
                    "wholeWord" => {
                        self.max_suffix = self.max_suffix.max(r.from.chars().count());
                        let rule = rule(&r.to, true);
                        self.suffix.entry(r.from.clone()).or_default().push(rule);
                    }
                    "prefix" => {
                        self.max_prefix = self.max_prefix.max(r.from_prefix.chars().count());
                        let rule = rule(&r.to_prefix, false);
                        self.prefix
                            .entry(r.from_prefix.clone())
                            .or_default()
                            .push(rule);
                    }
                    "phrasalVerbInterposedObject" => {
                        let rule = rule("", false);
                        self.phrasal
                            .get_or_insert_with(Phrasal::default)
                            .interposed
                            .push(rule);
                    }
                    other => return Err(TablesError(format!("unknown rule type {other}"))),
                }
            }
            self.transforms.push(Transform {
                name: transform.name,
                description: transform.description,
            });
        }
        Ok(())
    }

    /// The language of the tables.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// A transform of a trace.
    pub fn transform(&self, id: u16) -> &Transform {
        &self.transforms[usize::from(id)]
    }

    /// The bits of a condition name (`v1`), with its sub-conditions.
    pub fn condition(&self, name: &str) -> u64 {
        self.conditions.get(name).copied().unwrap_or(0)
    }

    /// The conditions a term with these `rules` (space-separated part-of-speech
    /// tags) satisfies; `*` satisfies all (LKP-023).
    pub fn rules_conditions(&self, rules: &str) -> u64 {
        let mut bits = 0;
        for tag in rules.split_whitespace() {
            if tag == "*" {
                return u64::MAX;
            }
            bits |= self.condition(tag);
        }
        bits
    }

    /// Every form `text` may come from, `text` itself first (LKP-022).
    pub fn deinflect(&self, text: &str) -> Vec<Deinflection> {
        let mut out = Vec::new();
        self.walk(text, 0, &mut Vec::new(), &mut out, 0);
        out
    }

    fn walk(
        &self,
        text: &str,
        conditions: u64,
        trace: &mut Vec<u16>,
        out: &mut Vec<Deinflection>,
        depth: usize,
    ) {
        let len = text.chars().count();
        if len == 0 || depth > MAX_DEPTH {
            return;
        }
        out.push(Deinflection {
            text: text.to_string(),
            conditions,
            trace: trace.clone(),
        });
        if len == 1 {
            return;
        }
        let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        // suffixes, longest first
        for n in (1..=self.max_suffix.min(len)).rev() {
            let cut = offsets[len - n];
            let Some(rules) = self.suffix.get(&text[cut..]) else {
                continue;
            };
            let head = &text[..cut];
            for rule in rules {
                if (rule.whole_word && !head.is_empty()) || !applies(conditions, rule) {
                    continue;
                }
                trace.push(rule.transform);
                self.walk(
                    &format!("{head}{}", rule.to),
                    rule.conditions_out,
                    trace,
                    out,
                    depth + 1,
                );
                trace.pop();
            }
        }
        // prefixes, shortest first, never the whole text
        for &cut in offsets.iter().skip(1).take(self.max_prefix.min(len - 1)) {
            let Some(rules) = self.prefix.get(&text[..cut]) else {
                continue;
            };
            for rule in rules.iter().filter(|r| applies(conditions, r)) {
                trace.push(rule.transform);
                self.walk(
                    &format!("{}{}", rule.to, &text[cut..]),
                    rule.conditions_out,
                    trace,
                    out,
                    depth + 1,
                );
                trace.pop();
            }
        }
        if let Some(phrasal) = &self.phrasal {
            self.phrasal(phrasal, text, conditions, trace, out, depth);
        }
    }

    /// English phrasal verbs (BUG-2549), Yomitan's two regex rules: the verb
    /// before a particle is deinflected on its own (`gave up` → `give up`), and
    /// an object between verb and particle is removed (`picked it up` →
    /// `picked up`). Particles match whole words only.
    fn phrasal(
        &self,
        phrasal: &Phrasal,
        text: &str,
        conditions: u64,
        trace: &mut Vec<u16>,
        out: &mut Vec<Deinflection>,
        depth: usize,
    ) {
        let Some((head, tail)) = text.split_once(' ') else {
            return;
        };
        if head.is_empty() || tail.is_empty() {
            return;
        }
        let next = tail.split(' ').next().unwrap_or_default();
        if next.is_empty() {
            return;
        }
        if phrasal.words.contains(next) {
            let offsets: Vec<usize> = head.char_indices().map(|(i, _)| i).collect();
            let len = offsets.len();
            for n in (1..=self.max_suffix.min(len)).rev() {
                let cut = offsets[len - n];
                let Some(rules) = self.suffix.get(&head[cut..]) else {
                    continue;
                };
                let stem = &head[..cut];
                for rule in rules {
                    if !rule.phrasal
                        || (rule.whole_word && !stem.is_empty())
                        || !applies(conditions, rule)
                    {
                        continue;
                    }
                    trace.push(rule.transform);
                    let form = format!("{stem}{} {tail}", rule.to);
                    self.walk(&form, phrasal.conditions_out, trace, out, depth + 1);
                    trace.pop();
                }
            }
        } else if !phrasal.interposed.is_empty() {
            // the first phrasal word after the object must be a particle
            let mut at = 0;
            let mut particle = None;
            for token in tail.split(' ') {
                if phrasal.words.contains(token) {
                    if phrasal.particles.contains(token) {
                        particle = Some(at);
                    }
                    break;
                }
                at += token.len() + 1;
            }
            if let Some(p) = particle.filter(|&p| p > 0) {
                for rule in phrasal.interposed.iter().filter(|r| applies(conditions, r)) {
                    trace.push(rule.transform);
                    let form = format!("{head} {}", &tail[p..]);
                    self.walk(&form, rule.conditions_out, trace, out, depth + 1);
                    trace.pop();
                }
            }
        }
    }
}

fn applies(conditions: u64, rule: &Rule) -> bool {
    conditions == 0 || conditions & rule.conditions_in != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(d: &Deinflector, x: &Deinflection) -> Vec<String> {
        x.trace
            .iter()
            .map(|&t| d.transform(t).name.clone())
            .collect()
    }

    /// The traces that lead from `text` to `form` with a condition among
    /// `rules`.
    fn reaches(d: &Deinflector, text: &str, form: &str, rules: &str) -> Vec<Vec<String>> {
        let pos = d.rules_conditions(rules);
        d.deinflect(text)
            .iter()
            .filter(|x| x.text == form && (x.conditions == 0 || x.conditions & pos != 0))
            .map(|x| trace(d, x))
            .collect()
    }

    // T-LKP-020
    #[test]
    fn every_embedded_table_loads() {
        for lang in LANGUAGES {
            let d = Deinflector::for_language(lang).unwrap_or_else(|| panic!("{lang}"));
            assert_eq!(d.language(), *lang);
            assert!(!d.transforms.is_empty(), "{lang}");
        }
        assert!(Deinflector::for_language("xx").is_none());
        assert!(Deinflector::for_language("en").unwrap().phrasal.is_some());
    }

    // T-LKP-021
    #[test]
    fn sub_conditions_are_expanded() {
        let ja = Deinflector::for_language("ja").unwrap();
        let v = ja.condition("v");
        assert_ne!(ja.condition("v1") & v, 0);
        assert_eq!(
            ja.condition("v1d") & !v,
            0,
            "v1d is inside v1, which is inside v"
        );
        assert_eq!(ja.rules_conditions("*"), u64::MAX);
        assert_eq!(
            ja.rules_conditions("v1 vt"),
            ja.condition("v1") | ja.condition("vt")
        );
    }

    // T-LKP-020, T-LKP-022, T-LKP-024
    #[test]
    fn japanese_forms() {
        let ja = Deinflector::for_language("ja").unwrap();
        assert!(!reaches(&ja, "食べました", "食べる", "v1").is_empty());
        assert!(!reaches(&ja, "行かなかった", "行く", "v5").is_empty());
        assert!(!reaches(&ja, "書いた", "書く", "v5").is_empty());
        assert!(!reaches(&ja, "高くない", "高い", "adj-i").is_empty());
        let yonde = reaches(&ja, "読んでいる", "読む", "v5");
        assert!(
            yonde.contains(&vec!["-いる".to_string(), "-て".to_string()]),
            "{yonde:?}"
        );
        // the text itself comes first, with no conditions
        let first = &ja.deinflect("食べる")[0];
        assert_eq!((first.text.as_str(), first.conditions), ("食べる", 0));
    }

    #[test]
    fn english_forms_and_phrasal_verbs() {
        let en = Deinflector::for_language("en").unwrap();
        assert!(!reaches(&en, "walked", "walk", "v").is_empty());
        assert!(!reaches(&en, "gave up", "give up", "v_phr").is_empty());
        assert!(!reaches(&en, "picked it up", "pick up", "v_phr").is_empty());
        assert!(reaches(&en, "gave upstairs", "give upstairs", "v_phr").is_empty());
    }

    // T-LKP-022
    #[test]
    fn chains_stop_at_the_maximum_depth() {
        let ja = Deinflector::for_language("ja").unwrap();
        for text in [
            "食べさせられなかったらしいです",
            "させられさせられさせられさせられさせられた",
        ] {
            let all = ja.deinflect(text);
            assert!(all.iter().all(|d| d.trace.len() <= MAX_DEPTH + 1));
        }
    }

    #[test]
    fn broken_tables_are_refused() {
        assert!(Deinflector::from_json(&["{"]).is_err());
        let bad = r#"{"language":"xx","conditions":{},"transforms":{"t":{"name":"t","rules":[{"type":"regex"}]}}}"#;
        assert!(Deinflector::from_json(&[bad]).is_err());
    }
}
