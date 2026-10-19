// SPDX-License-Identifier: GPL-3.0-only

//! redb schema from SPECIFICATION §6.5.3 (subset) and import of a Yomitan zip.

use std::borrow::Cow;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use redb::{Builder, Durability, MultimapTableDefinition, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

pub const INFO: TableDefinition<&str, &[u8]> = TableDefinition::new("info");
pub const TERMS: TableDefinition<u64, &[u8]> = TableDefinition::new("terms");
pub const BY_EXPR: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("term_by_expression");
pub const BY_READING: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("term_by_reading");
pub const TERM_META: MultimapTableDefinition<&str, &[u8]> =
    MultimapTableDefinition::new("term_meta");

/// Term record (postcard). Text fields are read without copying.
#[derive(Debug, Serialize, Deserialize)]
pub struct TermRecord<'a> {
    #[serde(borrow)]
    pub expression: Cow<'a, str>,
    #[serde(borrow)]
    pub reading: Cow<'a, str>,
    #[serde(borrow)]
    pub def_tags: Cow<'a, str>,
    #[serde(borrow)]
    pub rules: Cow<'a, str>,
    pub score: f64,
    pub sequence: i64,
    #[serde(borrow)]
    pub term_tags: Cow<'a, str>,
    /// Glossary JSON compressed with lz4 (size prepended).
    #[serde(borrow)]
    pub glossary_lz4: Cow<'a, [u8]>,
}

/// `term_meta` record (postcard).
#[derive(Debug, Serialize, Deserialize)]
pub enum MetaRecord {
    Freq {
        reading: Option<String>,
        value: i64,
        display: Option<String>,
    },
    Pitch {
        reading: String,
        positions: Vec<u8>,
    },
}

#[derive(Deserialize)]
struct TermRow(
    String,
    String,
    Option<String>,
    String,
    f64,
    Box<RawValue>,
    i64,
    String,
);

#[derive(Deserialize)]
struct MetaRow(String, String, Box<RawValue>);

#[derive(Deserialize)]
#[serde(untagged)]
enum FreqValue {
    Number(f64),
    Text(String),
    Object {
        value: f64,
        #[serde(rename = "displayValue")]
        display: Option<String>,
    },
}

#[derive(Deserialize)]
struct FreqWithReading {
    reading: String,
    frequency: FreqValue,
}

#[derive(Deserialize)]
struct PitchData {
    reading: String,
    pitches: Vec<PitchEntry>,
}

#[derive(Deserialize)]
struct PitchEntry {
    position: serde_json::Value,
}

fn freq_value(v: FreqValue) -> (i64, Option<String>) {
    match v {
        FreqValue::Number(n) => (n as i64, None),
        // DIC-003: for strings, the leading integer is used
        FreqValue::Text(s) => {
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            (digits.parse().unwrap_or(-1), Some(s))
        }
        FreqValue::Object { value, display } => (value as i64, display),
    }
}

fn parse_meta(row: &MetaRow) -> Result<Option<MetaRecord>> {
    Ok(match row.1.as_str() {
        "freq" => {
            let raw = row.2.get();
            if let Ok(v) = serde_json::from_str::<FreqWithReading>(raw) {
                let (value, display) = freq_value(v.frequency);
                Some(MetaRecord::Freq {
                    reading: Some(v.reading),
                    value,
                    display,
                })
            } else {
                let (value, display) = freq_value(serde_json::from_str(raw)?);
                Some(MetaRecord::Freq {
                    reading: None,
                    value,
                    display,
                })
            }
        }
        "pitch" => {
            let p: PitchData = serde_json::from_str(row.2.get())?;
            let positions = p
                .pitches
                .iter()
                .filter_map(|e| e.position.as_u64().map(|n| n as u8))
                .collect();
            Some(MetaRecord::Pitch {
                reading: p.reading,
                positions,
            })
        }
        _ => None,
    })
}

/// Import result, printed as JSON on stdout.
#[derive(Debug, Serialize)]
pub struct ImportReport {
    pub dictionary: String,
    pub terms: u64,
    pub meta: u64,
    pub json_bytes: u64,
    pub seconds: f64,
    pub peak_rss_mib: f64,
    pub db_bytes: u64,
}

fn bank_number(name: &str) -> u32 {
    name.trim_end_matches(".json")
        .rsplit('_')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Imports `zip_path` into `db_path` in a single write transaction (DIC-013).
pub fn import(zip_path: &Path, db_path: &Path) -> Result<ImportReport> {
    let started = Instant::now();
    let file = std::fs::File::open(zip_path).with_context(|| zip_path.display().to_string())?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
    let mut index = String::new();
    zip.by_name("index.json")?.read_to_string(&mut index)?;

    let mut banks: Vec<String> = zip
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .filter(|n| n.starts_with("term_bank_") || n.starts_with("term_meta_bank_"))
        .collect();
    banks.sort_by_key(|n| (n.starts_with("term_meta"), bank_number(n)));

    if db_path.exists() {
        std::fs::remove_file(db_path)?;
    }
    let db = Builder::new().set_cache_size(64 << 20).create(db_path)?;
    let mut tx = db.begin_write()?;
    tx.set_durability(Durability::Immediate)?;

    let mut by_expr: Vec<(String, u64)> = Vec::new();
    let mut by_reading: Vec<(String, u64)> = Vec::new();
    let mut meta: Vec<(String, Vec<u8>)> = Vec::new();
    let mut id = 0u64;
    let mut json_bytes = 0u64;
    {
        let mut terms = tx.open_table(TERMS)?;
        let mut text = String::new();
        for name in &banks {
            text.clear();
            zip.by_name(name)?.read_to_string(&mut text)?;
            json_bytes += text.len() as u64;
            if name.starts_with("term_bank_") {
                let rows: Vec<TermRow> =
                    serde_json::from_str(&text).with_context(|| format!("{name}: invalid JSON"))?;
                for r in rows {
                    id += 1;
                    let glossary = lz4_flex::compress_prepend_size(r.5.get().as_bytes());
                    let rec = TermRecord {
                        expression: Cow::Borrowed(&r.0),
                        reading: Cow::Borrowed(&r.1),
                        def_tags: Cow::Borrowed(r.2.as_deref().unwrap_or("")),
                        rules: Cow::Borrowed(&r.3),
                        score: r.4,
                        sequence: r.6,
                        term_tags: Cow::Borrowed(&r.7),
                        glossary_lz4: Cow::Owned(glossary),
                    };
                    terms.insert(id, postcard::to_stdvec(&rec)?.as_slice())?;
                    if !r.1.is_empty() && r.1 != r.0 {
                        by_reading.push((r.1.clone(), id));
                    }
                    by_expr.push((r.0, id));
                }
            } else {
                let rows: Vec<MetaRow> =
                    serde_json::from_str(&text).with_context(|| format!("{name}: invalid JSON"))?;
                for r in &rows {
                    if let Some(m) = parse_meta(r)? {
                        meta.push((r.0.clone(), postcard::to_stdvec(&m)?));
                    }
                }
            }
        }
    }
    // Indexes are written in key order: sequential inserts into the B-tree.
    by_expr.sort_unstable();
    by_reading.sort_unstable();
    meta.sort_unstable();
    {
        let mut t = tx.open_multimap_table(BY_EXPR)?;
        for (k, v) in &by_expr {
            t.insert(k.as_str(), v)?;
        }
    }
    {
        let mut t = tx.open_multimap_table(BY_READING)?;
        for (k, v) in &by_reading {
            t.insert(k.as_str(), v)?;
        }
    }
    {
        let mut t = tx.open_multimap_table(TERM_META)?;
        for (k, v) in &meta {
            t.insert(k.as_str(), v.as_slice())?;
        }
    }
    {
        let mut info = tx.open_table(INFO)?;
        info.insert("index.json", index.as_bytes())?;
    }
    tx.commit()?;
    let mut db = db;
    if std::env::var_os("S2_COMPACT").is_some() {
        db.compact()?;
    }
    drop(db);

    let title = serde_json::from_str::<serde_json::Value>(&index)?["title"]
        .as_str()
        .unwrap_or("?")
        .to_string();
    if id == 0 && meta.is_empty() {
        bail!("{title}: empty dictionary");
    }
    Ok(ImportReport {
        dictionary: title,
        terms: id,
        meta: meta.len() as u64,
        json_bytes,
        seconds: started.elapsed().as_secs_f64(),
        peak_rss_mib: peak_rss_mib(),
        db_bytes: std::fs::metadata(db_path)?.len(),
    })
}

/// Peak RSS of the process (`VmHWM`), in MiB.
pub fn peak_rss_mib() -> f64 {
    proc_status_kib("VmHWM:") / 1024.0
}

/// Current RSS (`VmRSS`), in MiB.
pub fn rss_mib() -> f64 {
    proc_status_kib("VmRSS:") / 1024.0
}

fn proc_status_kib(key: &str) -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0.0)
}
