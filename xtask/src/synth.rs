// SPDX-License-Identifier: GPL-3.0-only

//! Large synthetic Yomitan dictionaries (TEST §4.3, `cargo xtask fixtures --large`):
//! 5 term dictionaries with 300k entries each (~150 MB of JSON each), 3 frequency
//! dictionaries (in the three formats of DIC-003), 1 pitch-accent dictionary and a
//! corpus of 10k queries.
//!
//! Everything derives from a pseudo-Japanese lexicon generated with a fixed seed: the
//! dictionaries share most of their entries, like JMdict and real frequency
//! dictionaries, so lookups find results in several dictionaries at once.

use std::fmt::Write as _;
use std::io::{BufWriter, Write as _};
use std::path::Path;

use anyhow::Result;

use crate::fixtures::{fresh, stamp, zip_options};
use crate::util::SplitMix64;

/// Changing it forces regeneration.
const VERSION: &str = "synth-v5";
/// Term dictionaries (T-PERF-04: 5 term + 3 freq + 1 pitch).
pub const TERM_DICTS: u64 = 5;
/// Frequency dictionaries.
pub const FREQ_DICTS: u64 = 3;
/// Entries in each term dictionary (NFR-PERF-05).
const TERMS_PER_DICT: usize = 300_000;
/// Words in the shared lexicon.
const LEXICON: usize = 400_000;
/// Entries per `*_bank_N.json` file.
const BANK: usize = 10_000;
/// Queries in the corpus for the lookup benchmark.
const QUERIES: usize = 10_000;

const HIRAGANA: &[&str] = &[
    "あ", "い", "う", "え", "お", "か", "き", "く", "け", "こ", "さ", "し", "す", "せ", "そ", "た",
    "ち", "つ", "て", "と", "な", "に", "ぬ", "ね", "の", "は", "ひ", "ふ", "へ", "ほ", "ま", "み",
    "む", "め", "も", "や", "ゆ", "よ", "ら", "り", "る", "れ", "ろ", "わ", "が", "ぎ", "ぐ", "げ",
    "ご", "ざ", "じ", "ず", "ぜ", "ぞ", "だ", "で", "ど", "ば", "び", "ぶ", "べ", "ぼ", "きょ",
    "しゃ", "しょ", "ちゅ", "りょ",
];
const PARTICLES: &[&str] = &[
    "を", "が", "は", "に", "で", "の", "も", "と", "", "から", "まで",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Noun,
    V1,
    /// Godan verb with its ending (く, ぐ, す, …).
    V5(char),
    AdjI,
    Vs,
    Kana,
}

#[derive(Debug, Clone)]
struct Word {
    expr: String,
    reading: String,
    kind: Kind,
}

impl Kind {
    fn rules(self) -> &'static str {
        match self {
            Kind::Noun | Kind::Kana => "",
            Kind::V1 => "v1",
            Kind::V5(_) => "v5",
            Kind::AdjI => "adj-i",
            Kind::Vs => "vs",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Kind::Noun | Kind::Kana => "n",
            Kind::V1 => "v1",
            Kind::V5(_) => "v5",
            Kind::AdjI => "adj-i",
            Kind::Vs => "vs",
        }
    }
}

fn kanji(rng: &mut SplitMix64) -> char {
    // 3000 distinct ideographs in U+4E00–U+9FFF.
    let i = rng.below(3000) as u32;
    char::from_u32(0x4E00 + (i * 7) % 20_000).unwrap_or('日')
}

fn kana_run(rng: &mut SplitMix64, morae: usize) -> String {
    (0..morae).map(|_| *rng.pick(HIRAGANA)).collect()
}

fn kanji_with_reading(rng: &mut SplitMix64, n: usize) -> (String, String) {
    let mut expr = String::new();
    let mut reading = String::new();
    for _ in 0..n {
        expr.push(kanji(rng));
        let morae = rng.range(1, 2);
        reading.push_str(&kana_run(rng, morae));
        if rng.chance(0.15) {
            reading.push(if rng.chance(0.5) { 'ん' } else { 'う' });
        }
    }
    (expr, reading)
}

fn katakana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

fn word(rng: &mut SplitMix64) -> Word {
    let roll = rng.below(100);
    let (kind, n) = match roll {
        0..45 => (Kind::Noun, *rng.pick(&[1, 2, 2, 2, 3, 4])),
        45..60 => (
            Kind::V5(*rng.pick(&['く', 'ぐ', 'す', 'つ', 'ぬ', 'ぶ', 'む', 'る', 'う'])),
            rng.range(1, 2),
        ),
        60..72 => (Kind::V1, rng.range(1, 2)),
        72..80 => (Kind::AdjI, 1),
        80..88 => (Kind::Vs, 2),
        _ => (Kind::Kana, 0),
    };
    let (mut expr, mut reading) = kanji_with_reading(rng, n);
    let okurigana = match kind {
        Kind::V5(e) => e.to_string(),
        Kind::V1 => (*rng.pick(&[
            "べる", "める", "せる", "きる", "じる", "える", "ける", "ねる", "れる", "てる",
        ]))
        .to_string(),
        Kind::AdjI => (*rng.pick(&["い", "しい", "かしい"])).to_string(),
        Kind::Kana => {
            let morae = rng.range(2, 6);
            let k = kana_run(rng, morae);
            let k = if rng.chance(0.6) { katakana(&k) } else { k };
            return Word {
                expr: k.clone(),
                reading: k,
                kind,
            };
        }
        Kind::Noun | Kind::Vs => String::new(),
    };
    expr.push_str(&okurigana);
    reading.push_str(&okurigana);
    Word {
        expr,
        reading,
        kind,
    }
}

const SYLLABLES: &[&str] = &[
    "ka", "lo", "ren", "ti", "mo", "sa", "ver", "en", "ta", "ri", "on", "dis", "ex", "pre", "con",
    "al", "ing", "ed", "ly", "tion", "ma", "ne", "ru", "so", "ple", "ter", "ment", "be", "st",
    "ow",
];

fn gloss_word(rng: &mut SplitMix64, out: &mut String) {
    for _ in 0..rng.range(1, 3) {
        out.push_str(rng.pick(SYLLABLES));
    }
}

fn glossary(rng: &mut SplitMix64, out: &mut String) {
    out.push('[');
    let senses = rng.range(2, 6);
    for s in 0..senses {
        if s > 0 {
            out.push(',');
        }
        let words = rng.range(7, 27);
        if rng.chance(0.1) {
            out.push_str(r#"{"type":"structured-content","content":[{"tag":"ul","content":[{"tag":"li","content":""#);
            for w in 0..words {
                if w > 0 {
                    out.push(' ');
                }
                gloss_word(rng, out);
            }
            out.push_str(r#""}]}]}"#);
        } else {
            out.push('"');
            for w in 0..words {
                if w > 0 {
                    out.push(' ');
                }
                gloss_word(rng, out);
            }
            out.push('"');
        }
    }
    out.push(']');
}

fn mix(a: u64, b: u64) -> u64 {
    let mut r = SplitMix64::new(a.wrapping_mul(0x1000_0001).wrapping_add(b));
    r.next_u64()
}

struct DictWriter {
    zip: zip::ZipWriter<BufWriter<std::fs::File>>,
    prefix: &'static str,
    bank: usize,
    in_bank: usize,
}

impl DictWriter {
    fn create(path: &Path, index: &str, prefix: &'static str) -> Result<Self> {
        let file = BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("index.json", zip_options(1))?;
        zip.write_all(index.as_bytes())?;
        Ok(Self {
            zip,
            prefix,
            bank: 0,
            in_bank: 0,
        })
    }

    fn entry(&mut self, json: &str) -> Result<()> {
        if self.in_bank == 0 {
            self.bank += 1;
            self.zip.start_file(
                format!("{}_{}.json", self.prefix, self.bank),
                zip_options(1),
            )?;
            self.zip.write_all(b"[")?;
        } else {
            self.zip.write_all(b",")?;
        }
        self.zip.write_all(json.as_bytes())?;
        self.in_bank += 1;
        if self.in_bank == BANK {
            self.zip.write_all(b"]")?;
            self.in_bank = 0;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        if self.in_bank > 0 {
            self.zip.write_all(b"]")?;
        }
        self.zip.finish()?.flush()?;
        Ok(())
    }
}

fn index(title: &str, extra: &str) -> String {
    format!(
        r#"{{"title":"{title}","revision":"{VERSION}","format":3,"sequenced":true,"author":"Nikomiru xtask","description":"Synthetic dictionary generated for tests"{extra}}}"#
    )
}

/// Generates the dictionaries and the corpus in `dir` (if the recipe has changed).
pub fn generate(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let marker = dir.join("queries.txt");
    let recipe = format!("{VERSION} {TERMS_PER_DICT} {LEXICON} {QUERIES}");
    if fresh(&marker, &recipe) {
        eprintln!("  = synthetic dictionaries");
        return Ok(());
    }
    let started = std::time::Instant::now();
    let mut rng = SplitMix64::new(1);
    let lexicon: Vec<Word> = (0..LEXICON).map(|_| word(&mut rng)).collect();

    for d in 1..=TERM_DICTS {
        let path = dir.join(format!("synth-term-{d}.zip"));
        eprintln!(
            "  + {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        let mut w = DictWriter::create(&path, &index(&format!("Synth Term {d}"), ""), "term_bank")?;
        let mut rng = SplitMix64::new(100 + d);
        let mut json = String::with_capacity(1024);
        let mut written = 0;
        for (i, word) in lexicon.iter().enumerate() {
            if written == TERMS_PER_DICT {
                break;
            }
            // ~75% of the lexicon, different for each dictionary
            if mix(i as u64, d).is_multiple_of(4) {
                continue;
            }
            json.clear();
            let _ = write!(
                json,
                r#"["{}","{}","{}","{}",{},"#,
                word.expr,
                word.reading,
                word.kind.tag(),
                word.kind.rules(),
                rng.below(100)
            );
            glossary(&mut rng, &mut json);
            let _ = write!(json, r#",{},"""#, i + 1);
            json.push(']');
            w.entry(&json)?;
            written += 1;
        }
        w.finish()?;
    }

    for d in 1..=FREQ_DICTS {
        let path = dir.join(format!("synth-freq-{d}.zip"));
        eprintln!(
            "  + {}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        let mut w = DictWriter::create(
            &path,
            &index(
                &format!("Synth Freq {d}"),
                r#","frequencyMode":"rank-based""#,
            ),
            "term_meta_bank",
        )?;
        let mut rank = 0u64;
        let mut json = String::new();
        for (i, word) in lexicon.iter().enumerate() {
            if mix(i as u64, 50 + d) % 8 >= 3 {
                continue;
            }
            rank += 1;
            json.clear();
            // the three value formats of DIC-003
            let _ = match d {
                1 => write!(json, r#"["{}","freq",{rank}]"#, word.expr),
                2 => write!(
                    json,
                    r#"["{}","freq",{{"value":{rank},"displayValue":"{rank}㋕"}}]"#,
                    word.expr
                ),
                _ => write!(
                    json,
                    r#"["{}","freq",{{"reading":"{}","frequency":{{"value":{rank},"displayValue":"{rank}"}}}}]"#,
                    word.expr, word.reading
                ),
            };
            w.entry(&json)?;
        }
        w.finish()?;
    }

    {
        let path = dir.join("synth-pitch.zip");
        eprintln!("  + synth-pitch.zip");
        let mut w = DictWriter::create(&path, &index("Synth Pitch", ""), "term_meta_bank")?;
        let mut rng = SplitMix64::new(300);
        let mut json = String::new();
        for (i, word) in lexicon.iter().enumerate() {
            if mix(i as u64, 77).is_multiple_of(2) {
                continue;
            }
            json.clear();
            let morae = word.reading.chars().count();
            let _ = write!(
                json,
                r#"["{}","pitch",{{"reading":"{}","pitches":[{{"position":{}}}]}}]"#,
                word.expr,
                word.reading,
                rng.below(morae + 1)
            );
            w.entry(&json)?;
        }
        w.finish()?;
    }

    let mut rng = SplitMix64::new(400);
    let mut out = String::with_capacity(QUERIES * 64);
    for _ in 0..QUERIES {
        for seg in 0..rng.range(2, 4) {
            let w = rng.pick(&lexicon);
            out.push_str(&inflect(w, &mut rng));
            if seg == 0 || rng.chance(0.7) {
                out.push_str(rng.pick(PARTICLES));
            }
        }
        out.push('\n');
    }
    std::fs::write(&marker, out)?;
    stamp(&marker, &recipe)?;
    eprintln!(
        "  synthetic dictionaries generated in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// A plausible inflected form of the word, for the query corpus.
fn inflect(w: &Word, rng: &mut SplitMix64) -> String {
    let stem = |n: usize| -> String {
        let count = w.expr.chars().count();
        w.expr.chars().take(count.saturating_sub(n)).collect()
    };
    match w.kind {
        Kind::Noun | Kind::Kana => w.expr.clone(),
        Kind::Vs => format!(
            "{}{}",
            w.expr,
            rng.pick(&["する", "します", "した", "して", "しない", "される"])
        ),
        Kind::V1 => format!(
            "{}{}",
            stem(1),
            rng.pick(&[
                "る",
                "ます",
                "ました",
                "ない",
                "なかった",
                "て",
                "ている",
                "た",
                "られる",
                "させる",
                "たい",
                "よう",
                "れば",
                "ません"
            ])
        ),
        Kind::AdjI => format!(
            "{}{}",
            stem(1),
            rng.pick(&["い", "かった", "くない", "くて", "ければ", "く", "さ"])
        ),
        Kind::V5(e) => {
            let (i, a, ee, o, te, ta) = match e {
                'く' => ("き", "か", "け", "こ", "いて", "いた"),
                'ぐ' => ("ぎ", "が", "げ", "ご", "いで", "いだ"),
                'す' => ("し", "さ", "せ", "そ", "して", "した"),
                'つ' => ("ち", "た", "て", "と", "って", "った"),
                'ぬ' => ("に", "な", "ね", "の", "んで", "んだ"),
                'ぶ' => ("び", "ば", "べ", "ぼ", "んで", "んだ"),
                'む' => ("み", "ま", "め", "も", "んで", "んだ"),
                'る' => ("り", "ら", "れ", "ろ", "って", "った"),
                _ => ("い", "わ", "え", "お", "って", "った"),
            };
            let s = stem(1);
            match rng.below(10) {
                0 => w.expr.clone(),
                1 => format!("{s}{i}ます"),
                2 => format!("{s}{i}ました"),
                3 => format!("{s}{a}ない"),
                4 => format!("{s}{a}なかった"),
                5 => format!("{s}{te}"),
                6 => format!("{s}{ta}"),
                7 => format!("{s}{te}いる"),
                8 => format!("{s}{ee}ば"),
                _ => format!("{s}{o}う"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_deterministic_and_well_formed() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(1);
        for _ in 0..1000 {
            let (x, y) = (word(&mut a), word(&mut b));
            assert_eq!(x.expr, y.expr);
            assert!(!x.expr.is_empty() && !x.reading.is_empty());
            assert!(!x.expr.contains(['"', '\\']));
        }
    }

    #[test]
    fn godan_inflections_keep_the_stem() {
        let w = Word {
            expr: "書く".into(),
            reading: "かく".into(),
            kind: Kind::V5('く'),
        };
        let mut rng = SplitMix64::new(3);
        for _ in 0..50 {
            assert!(inflect(&w, &mut rng).starts_with('書'));
        }
    }

    #[test]
    fn glossary_is_valid_json() {
        let mut rng = SplitMix64::new(9);
        for _ in 0..200 {
            let mut s = String::new();
            glossary(&mut rng, &mut s);
            let v: serde_json::Value = serde_json::from_str(&s).unwrap();
            assert!(v.is_array());
        }
    }
}
