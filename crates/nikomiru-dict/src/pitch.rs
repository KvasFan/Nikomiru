// SPDX-License-Identifier: GPL-3.0-only

//! Pitch accent for display (LKP-041…LKP-043): morae, high and low morae as
//! Yomitan's `isMoraPitchHigh`, the category, and the popup rows that merge
//! identical accents of several dictionaries. Ports of Fushi's
//! `fushi/assets/popup/popup.js` (ac8034e).

/// Small kana: they join the previous mora.
const SMALL: &str = "ぁぃぅぇぉゃゅょゎァィゥェォャュョヮ";

/// The morae of a reading (`しゃしん` → `しゃ`, `し`, `ん`).
pub fn morae(reading: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in reading.chars() {
        match out.last_mut() {
            Some(last) if SMALL.contains(c) => last.push(c),
            _ => out.push(c.to_string()),
        }
    }
    out
}

/// Whether mora `i` is high with the downstep after mora `position`.
pub fn is_high(i: usize, position: u32) -> bool {
    let p = position as usize;
    match p {
        0 => i > 0,
        1 => i < 1,
        _ => i > 0 && i < p,
    }
}

/// `H`/`L` for each mora and the particle after it (`はし` with 2 → `LHL`).
pub fn high_low(morae: usize, position: u32) -> String {
    (0..=morae)
        .map(|i| if is_high(i, position) { 'H' } else { 'L' })
        .collect()
}

/// The accent type (LKP-042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// No downstep.
    Heiban,
    /// Downstep after the first mora.
    Atamadaka,
    /// Downstep inside the word.
    Nakadaka,
    /// Downstep after the last mora.
    Odaka,
    /// Inflecting words with a downstep.
    Kifuku,
}

impl Category {
    /// Its name, as Yomitan writes it.
    pub fn name(self) -> &'static str {
        match self {
            Category::Heiban => "heiban",
            Category::Atamadaka => "atamadaka",
            Category::Nakadaka => "nakadaka",
            Category::Odaka => "odaka",
            Category::Kifuku => "kifuku",
        }
    }
}

/// The category of a downstep `position` on a word of `morae` morae; verbs
/// (`v*`) and i-adjectives with a downstep are kifuku.
pub fn category(morae: usize, position: u32, rules: &str) -> Category {
    if position == 0 {
        return Category::Heiban;
    }
    if rules
        .split_whitespace()
        .any(|r| r.starts_with('v') || r == "adj-i")
    {
        return Category::Kifuku;
    }
    match position as usize {
        1 => Category::Atamadaka,
        p if p >= morae => Category::Odaka,
        _ => Category::Nakadaka,
    }
}

/// The accents one dictionary gives an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The dictionary (its name).
    pub dictionary: String,
    /// Downstep positions.
    pub positions: Vec<u32>,
    /// Accents written as patterns.
    pub patterns: Vec<String>,
    /// IPA.
    pub transcriptions: Vec<String>,
}

/// A row of the popup: one accent payload and the dictionaries that give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Dictionaries, in order of appearance ("pitch accent · N" when > 1).
    pub dictionaries: Vec<String>,
    /// Downstep positions.
    pub positions: Vec<u32>,
    /// Accents written as patterns.
    pub patterns: Vec<String>,
    /// IPA.
    pub transcriptions: Vec<String>,
}

/// The rows of LKP-043: identical payloads first become one row (positions
/// compared as a set), then, with `dedupe`, what an earlier row shows is
/// removed from later ones and empty rows are dropped (BUG-2122, BUG-2397).
pub fn rows(groups: &[Group], dedupe: bool) -> Vec<Row> {
    let mut merged: Vec<(Vec<u32>, Row)> = Vec::new();
    for g in groups {
        let mut sorted = g.positions.clone();
        sorted.sort_unstable();
        let same = merged.iter_mut().find(|(k, r)| {
            *k == sorted && r.patterns == g.patterns && r.transcriptions == g.transcriptions
        });
        match same {
            Some((_, row)) => {
                if !row.dictionaries.contains(&g.dictionary) {
                    row.dictionaries.push(g.dictionary.clone());
                }
            }
            None => merged.push((
                sorted,
                Row {
                    dictionaries: vec![g.dictionary.clone()],
                    positions: g.positions.clone(),
                    patterns: g.patterns.clone(),
                    transcriptions: g.transcriptions.clone(),
                },
            )),
        }
    }
    let merged = merged.into_iter().map(|(_, r)| r);
    if !dedupe {
        return merged.collect();
    }
    let (mut positions, mut patterns, mut transcriptions) = (Vec::new(), Vec::new(), Vec::new());
    let mut out = Vec::new();
    for mut row in merged {
        row.positions.retain(|p| !positions.contains(p));
        row.patterns.retain(|p| !patterns.contains(p));
        row.transcriptions.retain(|t| !transcriptions.contains(t));
        if row.positions.is_empty() && row.patterns.is_empty() && row.transcriptions.is_empty() {
            continue;
        }
        positions.extend(row.positions.iter().copied());
        patterns.extend(row.patterns.iter().cloned());
        transcriptions.extend(row.transcriptions.iter().cloned());
        out.push(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LKP-041
    #[test]
    fn morae_and_high_low() {
        assert_eq!(morae("しゃしん"), ["しゃ", "し", "ん"]);
        assert_eq!(morae("ギター"), ["ギ", "タ", "ー"]);
        assert_eq!(high_low(2, 0), "LHH");
        assert_eq!(high_low(2, 1), "HLL");
        assert_eq!(high_low(2, 2), "LHL");
        assert_eq!(high_low(3, 2), "LHLL");
    }

    // T-LKP-042
    #[test]
    fn categories() {
        assert_eq!(category(2, 0, "n"), Category::Heiban);
        assert_eq!(category(2, 1, "n"), Category::Atamadaka);
        assert_eq!(category(3, 2, "n"), Category::Nakadaka);
        assert_eq!(category(2, 2, "n"), Category::Odaka);
        assert_eq!(category(3, 2, "v1"), Category::Kifuku);
        assert_eq!(category(3, 2, "adj-i"), Category::Kifuku);
        assert_eq!(category(3, 0, "v5k"), Category::Heiban);
        assert_eq!(Category::Kifuku.name(), "kifuku");
    }
}
