// SPDX-License-Identifier: GPL-3.0-only

//! Mixed kanji and kana spellings (LKP-031, BUG-3261): `棚にあげる` for
//! `棚に上げる`. Neither index has the mixed form, so every kanji group is
//! replaced with its readings, the full readings are looked up, and a hit is
//! accepted only if the query spells it out. A port of Fushi's
//! `native/fushidicts/fushidicts_src/scan/mixed_orthography.cpp` (ac8034e).

/// Kanji groups beyond which a text is not tried.
pub const MAX_KANJI_RUNS: usize = 3;
/// Readings tried per kanji group.
pub const MAX_READINGS_PER_RUN: usize = 16;
/// Full readings tried per text.
pub const MAX_CANDIDATES: usize = 64;
/// Kana a kanji can take in a reading (承る = うけたまわ·る needs 4).
const MAX_KANA_PER_KANJI: usize = 6;

/// A kanji, or 々 〆 ヶ, which take a kanji's place in a headword.
fn is_kanji(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF | 0x2A700..=0x2EBEF
        | 0x30000..=0x3134F | 0xF900..=0xFAFF | 0x2F800..=0x2FA1F | 0x3005 | 0x3006 | 0x30F6)
}

fn is_kana(c: char) -> bool {
    !is_kanji(c)
        && matches!(c as u32, 0x3041..=0x3096 | 0x309D..=0x309F | 0x30A1..=0x30FA | 0x30FC..=0x30FE)
}

/// Katakana as hiragana (ヽヾ too); anything else unchanged.
fn fold(c: char) -> char {
    match c as u32 {
        0x30A1..=0x30F6 | 0x30FD | 0x30FE => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

/// The text with its katakana as hiragana.
pub fn to_hiragana(text: &str) -> String {
    text.chars().map(fold).collect()
}

/// Whether the text has both kanji and kana: only those take this path.
pub fn is_mixed(text: &str) -> bool {
    text.chars().any(is_kanji) && text.chars().any(is_kana)
}

/// The full readings (hiragana, without duplicates and without `text` itself)
/// made from the readings `run_readings` gives for each kanji group, the
/// common ones first. Empty if a group has none or there are more than
/// [`MAX_KANJI_RUNS`] groups.
pub fn reading_candidates(
    text: &str,
    mut run_readings: impl FnMut(&str) -> Vec<String>,
) -> Vec<String> {
    let s: Vec<char> = text.chars().collect();
    let runs = s
        .iter()
        .enumerate()
        .filter(|&(i, &c)| is_kanji(c) && (i == 0 || !is_kanji(s[i - 1])))
        .count();
    if runs == 0 || runs > MAX_KANJI_RUNS {
        return Vec::new();
    }
    // fewer readings per group when there are more groups
    let mut per_run = MAX_READINGS_PER_RUN;
    while per_run > 1 && per_run.pow(runs as u32) > MAX_CANDIDATES {
        per_run -= 1;
    }
    let mut pieces: Vec<Vec<String>> = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let kanji = is_kanji(s[i]);
        let mut j = i;
        while j < s.len() && is_kanji(s[j]) == kanji {
            j += 1;
        }
        let run: String = s[i..j].iter().collect();
        if kanji {
            let mut options: Vec<String> = Vec::new();
            for reading in run_readings(&run) {
                let folded = to_hiragana(&reading);
                if folded.is_empty() || !folded.chars().all(is_kana) || options.contains(&folded) {
                    continue;
                }
                options.push(folded);
                if options.len() >= per_run {
                    break;
                }
            }
            if options.is_empty() {
                return Vec::new();
            }
            pieces.push(options);
        } else {
            pieces.push(vec![to_hiragana(&run)]);
        }
        i = j;
    }
    let total: usize = pieces.iter().map(Vec::len).product();
    let mut out: Vec<String> = Vec::with_capacity(total);
    for n in 0..total {
        let mut rest = n;
        let mut candidate = String::new();
        for options in &pieces {
            candidate.push_str(&options[rest % options.len()]);
            rest /= options.len();
        }
        if candidate != text && !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

/// Whether `query` is a mixed spelling of the headword: its kana match the
/// query, and each kanji group appears as written or is spelled out in full
/// with its part of the reading (never half: `足がく` is not `足掻く`). Kana
/// compare without regard to hiragana or katakana.
pub fn matches(query: &str, expression: &str, reading: &str) -> bool {
    if reading.is_empty() || query == expression {
        return false;
    }
    let q: Vec<char> = query.chars().collect();
    let e: Vec<char> = expression.chars().collect();
    let r: Vec<char> = reading.chars().collect();
    match_from(&q, 0, &e, 0, &r, 0)
}

fn match_from(q: &[char], qi: usize, e: &[char], ei: usize, r: &[char], ri: usize) -> bool {
    let Some(&c) = e.get(ei) else {
        return qi == q.len() && ri == r.len();
    };
    if is_kanji(c) {
        let mut run_end = ei;
        while run_end < e.len() && is_kanji(e[run_end]) {
            run_end += 1;
        }
        let run_len = run_end - ei;
        let literal = q.get(qi..qi + run_len) == Some(&e[ei..run_end]);
        let max_len = (MAX_KANA_PER_KANJI * run_len).min(r.len() - ri);
        for len in 1..=max_len {
            // written as kanji, taking `len` kana of the reading
            if literal && len >= run_len && match_from(q, qi + run_len, e, run_end, r, ri + len) {
                return true;
            }
            // spelled out: the next `len` kana of the query are that part of the reading
            if qi + len <= q.len()
                && (0..len).all(|k| is_kana(q[qi + k]) && fold(q[qi + k]) == fold(r[ri + k]))
                && match_from(q, qi + len, e, run_end, r, ri + len)
            {
                return true;
            }
        }
        return false;
    }
    if is_kana(c) {
        return qi < q.len()
            && ri < r.len()
            && fold(q[qi]) == fold(c)
            && fold(r[ri]) == fold(c)
            && match_from(q, qi + 1, e, ei + 1, r, ri + 1);
    }
    // other characters (・, digits, Latin): in the query as written; the
    // reading may or may not have them
    if q.get(qi) == Some(&c) {
        return (r.get(ri) == Some(&c) && match_from(q, qi + 1, e, ei + 1, r, ri + 1))
            || match_from(q, qi + 1, e, ei + 1, r, ri);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LKP-031
    #[test]
    fn mixed_spellings_match_whole_kanji_groups() {
        assert!(matches("棚にあげる", "棚に上げる", "たなにあげる"));
        assert!(
            !matches("取扱い", "取り扱い", "とりあつかい"),
            "取扱 is one group here"
        );
        assert!(matches("目をつける", "目を付ける", "めをつける"));
        assert!(matches("たなに上げる", "棚に上げる", "たなにあげる"));
        assert!(
            !matches("足がく", "足掻く", "あがく"),
            "half a group is not spelled out"
        );
        assert!(
            !matches("日が", "僻", "ひが"),
            "the query's kanji must be in the headword"
        );
        assert!(
            !matches("棚に上げる", "棚に上げる", "たなにあげる"),
            "not mixed: the same"
        );
        // a full kana spelling matches too; is_mixed keeps such queries off this path
        assert!(matches("ヤキモチ", "焼き餅", "やきもち"));
        assert!(is_mixed("棚にあげる") && !is_mixed("たなにあげる") && !is_mixed("棚"));
    }

    #[test]
    fn candidates_combine_the_readings_of_each_group() {
        let readings = |run: &str| match run {
            "棚" => vec!["たな".to_string(), "ダナ".to_string()],
            "上" => vec!["うえ".to_string(), "あ".to_string()],
            _ => Vec::new(),
        };
        assert_eq!(
            reading_candidates("棚に上げる", readings),
            [
                "たなにうえげる",
                "だなにうえげる",
                "たなにあげる",
                "だなにあげる"
            ]
        );
        assert!(
            reading_candidates("猫に上げる", readings).is_empty(),
            "猫 has no reading"
        );
        assert!(reading_candidates("一に二に三に四", |_| vec!["x".into()]).is_empty());
        // three groups: at most 4 readings each, 64 candidates
        let many = |_: &str| {
            (0..20)
                .map(|i| "あいうえおかきくけこさしすせそたちつてと"[i * 3..i * 3 + 3].to_string())
                .collect()
        };
        assert_eq!(reading_candidates("一に二に三", many).len(), MAX_CANDIDATES);
    }
}
