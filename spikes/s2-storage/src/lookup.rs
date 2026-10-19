// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan-style lookup over several redb dictionaries (LKP-004, LKP-022/023,
//! LKP-030, LKP-033…036, in reduced form): just enough to measure the realistic
//! cost of a popup query.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use redb::{ReadOnlyDatabase, ReadTransaction, ReadableDatabase};

use crate::deinflect::{Deinflector, rules_mask};
use crate::store::{BY_EXPR, BY_READING, MetaRecord, TERM_META, TERMS, TermRecord};

pub struct Dict {
    pub db: ReadOnlyDatabase,
}

impl Dict {
    pub fn open(path: &Path, cache_bytes: usize) -> Result<Self> {
        let db = redb::Builder::new()
            .set_cache_size(cache_bytes)
            .open_read_only(path)?;
        Ok(Self { db })
    }
}

pub struct Dicts {
    pub terms: Vec<Dict>,
    pub freqs: Vec<Dict>,
    pub pitch: Vec<Dict>,
}

/// A result group (LKP-036), ready for the popup.
#[derive(Debug)]
pub struct Group {
    pub expression: String,
    pub reading: String,
    pub matched: usize,
    pub steps: u8,
    pub trace: u8,
    pub frequency: i64,
    pub dict_priority: usize,
    pub score: f64,
    /// (dictionary index, decompressed glossary JSON)
    pub glossaries: Vec<(usize, String)>,
    pub frequencies: Vec<(usize, i64)>,
    pub pitches: Vec<(usize, Vec<u8>)>,
    ids: Vec<(usize, u64)>,
}

fn kata_to_hira(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

fn hira_to_kata(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ぁ'..='ゖ' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Text variants with their number of preprocessor steps (LKP-010/011, reduced).
fn variants(text: &str) -> Vec<(String, u8)> {
    let mut out = vec![(text.to_string(), 0u8)];
    for v in [kata_to_hira(text), hira_to_kata(text)] {
        if !out.iter().any(|(t, _)| *t == v) {
            out.push((v, 1));
        }
    }
    out
}

pub struct Session<'a> {
    dicts: &'a Dicts,
    terms: Vec<ReadTransaction>,
    freqs: Vec<ReadTransaction>,
    pitch: Vec<ReadTransaction>,
}

impl<'a> Session<'a> {
    /// One read transaction per dictionary, opened at the start of the query.
    pub fn begin(dicts: &'a Dicts) -> Result<Self> {
        let open = |ds: &[Dict]| -> Result<Vec<ReadTransaction>> {
            ds.iter().map(|d| Ok(d.db.begin_read()?)).collect()
        };
        Ok(Self {
            dicts,
            terms: open(&dicts.terms)?,
            freqs: open(&dicts.freqs)?,
            pitch: open(&dicts.pitch)?,
        })
    }

    pub fn lookup(
        &self,
        deinflector: &Deinflector,
        text: &str,
        max_results: usize,
    ) -> Result<Vec<Group>> {
        let _ = self.dicts;
        let chars: Vec<char> = text.chars().take(16).collect();
        // (expression, reading) → merged result group
        let mut groups: HashMap<(String, String), Group> = HashMap::new();
        let term_tables: Vec<_> = self
            .terms
            .iter()
            .map(|t| {
                Ok((
                    t.open_multimap_table(BY_EXPR)?,
                    t.open_multimap_table(BY_READING)?,
                    t.open_table(TERMS)?,
                ))
            })
            .collect::<Result<_>>()?;

        for len in (1..=chars.len()).rev() {
            let prefix: String = chars[..len].iter().collect();
            if prefix.ends_with(' ') {
                continue;
            }
            for (variant, steps) in variants(&prefix) {
                for d in deinflector.deinflect(&variant) {
                    for (di, (by_expr, by_reading, terms)) in term_tables.iter().enumerate() {
                        let mut ids: Vec<u64> = Vec::new();
                        for v in by_expr.get(d.text.as_str())? {
                            ids.push(v?.value());
                        }
                        for v in by_reading.get(d.text.as_str())? {
                            ids.push(v?.value());
                        }
                        for id in ids {
                            let Some(raw) = terms.get(id)? else { continue };
                            let rec: TermRecord<'_> = postcard::from_bytes(raw.value())?;
                            // LKP-023: part-of-speech filter
                            if d.conditions != 0 && rules_mask(&rec.rules) & d.conditions == 0 {
                                continue;
                            }
                            let key = (rec.expression.to_string(), rec.reading.to_string());
                            let g = groups.entry(key).or_insert_with(|| Group {
                                expression: rec.expression.to_string(),
                                reading: rec.reading.to_string(),
                                matched: len,
                                steps,
                                trace: d.trace,
                                frequency: i64::MAX,
                                dict_priority: di,
                                score: rec.score,
                                glossaries: Vec::new(),
                                frequencies: Vec::new(),
                                pitches: Vec::new(),
                                ids: Vec::new(),
                            });
                            // LKP-033: the longest match wins, then the fewest steps
                            if (len, std::cmp::Reverse(steps))
                                > (g.matched, std::cmp::Reverse(g.steps))
                            {
                                g.matched = len;
                                g.steps = steps;
                                g.trace = d.trace;
                            }
                            g.score = g.score.max(rec.score);
                            g.dict_priority = g.dict_priority.min(di);
                            if !g.ids.contains(&(di, id)) {
                                g.ids.push((di, id));
                            }
                        }
                    }
                }
            }
        }

        // Frequencies after deduplication (LKP-035).
        let freq_tables: Vec<_> = self
            .freqs
            .iter()
            .map(|t| Ok(t.open_multimap_table(TERM_META)?))
            .collect::<Result<_>>()?;
        let mut list: Vec<Group> = groups.into_values().collect();
        for g in &mut list {
            for (fi, t) in freq_tables.iter().enumerate() {
                let mut best: Option<i64> = None;
                for v in t.get(g.expression.as_str())? {
                    if let MetaRecord::Freq { reading, value, .. } =
                        postcard::from_bytes(v?.value())?
                        && reading.as_deref().is_none_or(|r| r == g.reading)
                        && value >= 0
                    {
                        best = Some(best.map_or(value, |b| b.min(value)));
                    }
                }
                if let Some(b) = best {
                    g.frequencies.push((fi, b));
                    if fi == 0 || g.frequency == i64::MAX {
                        g.frequency = g.frequency.min(b);
                    }
                }
            }
        }
        // LKP-034 (reduced): match, steps, trace, frequency, priority, score
        list.sort_by(|a, b| {
            b.matched
                .cmp(&a.matched)
                .then(a.steps.cmp(&b.steps))
                .then(a.trace.cmp(&b.trace))
                .then(a.frequency.cmp(&b.frequency))
                .then(a.dict_priority.cmp(&b.dict_priority))
                .then(b.score.total_cmp(&a.score))
        });
        list.truncate(max_results);

        // Pitch accent and glossaries only after truncation.
        let pitch_tables: Vec<_> = self
            .pitch
            .iter()
            .map(|t| Ok(t.open_multimap_table(TERM_META)?))
            .collect::<Result<_>>()?;
        for g in &mut list {
            for (pi, t) in pitch_tables.iter().enumerate() {
                for v in t.get(g.expression.as_str())? {
                    if let MetaRecord::Pitch { reading, positions } =
                        postcard::from_bytes(v?.value())?
                        && reading == g.reading
                    {
                        g.pitches.push((pi, positions));
                    }
                }
            }
            g.ids.sort_unstable();
            for &(di, id) in &g.ids {
                let (_, _, terms) = &term_tables[di];
                if let Some(raw) = terms.get(id)? {
                    let rec: TermRecord<'_> = postcard::from_bytes(raw.value())?;
                    let json = lz4_flex::decompress_size_prepended(&rec.glossary_lz4)?;
                    g.glossaries.push((di, String::from_utf8(json)?));
                }
            }
        }
        Ok(list)
    }
}
