// SPDX-License-Identifier: GPL-3.0-only

//! Yomitan / Yomichan dictionary formats (DIC-001…DIC-003, DIC-015):
//! `index.json`, term, term meta, kanji, kanji meta and tag banks. Rows are
//! read as raw JSON slices, so a glossary is stored exactly as the file has it
//! and odd rows (format 1, decimal scores, missing fields) do not stop an
//! import.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

/// A row that is not what the format says.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FormatError(pub String);

fn err(message: impl Into<String>) -> FormatError {
    FormatError(message.into())
}

/// How frequency values sort (DIC-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FrequencyMode {
    /// Lower is more frequent (a rank): the default.
    #[default]
    RankBased,
    /// Higher is more frequent (an occurrence count).
    OccurrenceBased,
}

/// `index.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Index {
    /// Title.
    pub title: String,
    /// Revision.
    pub revision: String,
    /// Format version (1, 2 or 3).
    pub format: u32,
    /// Whether `sequence` numbers are meaningful.
    pub sequenced: bool,
    /// How frequencies sort.
    pub frequency_mode: FrequencyMode,
    /// Language of the headwords, if declared (LKP-012).
    pub source_language: Option<String>,
    /// Language of the definitions, if declared.
    pub target_language: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Attribution text.
    pub attribution: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Home page.
    pub url: Option<String>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Parses `index.json`.
pub fn parse_index(bytes: &[u8]) -> Result<Index, FormatError> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| err(format!("index.json: {e}")))?;
    let title = text(&v, "title").ok_or_else(|| err("index.json has no title"))?;
    let revision = match v.get("revision") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err(err("index.json has no revision")),
    };
    let format = v
        .get("format")
        .or_else(|| v.get("version"))
        .and_then(Value::as_u64)
        .unwrap_or(3) as u32;
    Ok(Index {
        title,
        revision,
        format,
        sequenced: v.get("sequenced").and_then(Value::as_bool).unwrap_or(false),
        frequency_mode: match v.get("frequencyMode").and_then(Value::as_str) {
            Some("occurrence-based") => FrequencyMode::OccurrenceBased,
            _ => FrequencyMode::RankBased,
        },
        source_language: text(&v, "sourceLanguage"),
        target_language: text(&v, "targetLanguage"),
        author: text(&v, "author"),
        attribution: text(&v, "attribution"),
        description: text(&v, "description"),
        url: text(&v, "url"),
    })
}

/// Glossaries this short may be a redirect (LKP-032).
pub const MAX_REDIRECT_BYTES: usize = 1024;

/// `[formOf, [rules…]]`: a glossary item that points to another headword.
pub type Redirect = (String, Vec<String>);

/// The redirects of a glossary made only of `[formOf, [rules…]]` items
/// (LKP-032, at most [`MAX_REDIRECT_BYTES`]); `None` for anything else, a
/// real definition next to a redirect label included (BUG-2566).
pub fn parse_redirects(glossary: &str) -> Option<Vec<Redirect>> {
    if glossary.len() > MAX_REDIRECT_BYTES {
        return None;
    }
    let mut rest = glossary.trim_start();
    for _ in 0..2 {
        rest = rest.strip_prefix('[')?.trim_start();
    }
    let items: Vec<Redirect> = serde_json::from_str(glossary).ok()?;
    (!items.is_empty() && items.iter().all(|(form, _)| !form.is_empty())).then_some(items)
}

/// A term entry. The glossary is the raw JSON array.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// Headword.
    pub expression: String,
    /// Reading (the headword when empty in the file).
    pub reading: String,
    /// Definition tags, space separated.
    pub definition_tags: String,
    /// Deinflection rules, space separated (`v5k vt`).
    pub rules: String,
    /// Popularity score.
    pub score: f64,
    /// The glossary, as JSON.
    pub glossary: String,
    /// Sequence number (DIC-002).
    pub sequence: i64,
    /// Term tags, space separated.
    pub term_tags: String,
}

fn rows(bytes: &[u8]) -> Result<Vec<Vec<&RawValue>>, FormatError> {
    serde_json::from_slice(bytes).map_err(|e| err(e.to_string()))
}

fn raw_str(raw: Option<&&RawValue>) -> String {
    raw.and_then(|r| {
        serde_json::from_str::<Option<String>>(r.get())
            .ok()
            .flatten()
    })
    .unwrap_or_default()
}

fn raw_f64(raw: Option<&&RawValue>) -> f64 {
    raw.and_then(|r| serde_json::from_str::<f64>(r.get()).ok())
        .unwrap_or(0.0)
}

/// Parses a `term_bank_*.json`. Format 1 puts the glossary strings after the
/// score instead of an array.
pub fn parse_terms(bytes: &[u8], format: u32) -> Result<Vec<Term>, FormatError> {
    rows(bytes)?
        .iter()
        .enumerate()
        .map(|(i, row)| {
            if row.len() < 5 {
                return Err(err(format!("term {i}: {} fields", row.len())));
            }
            let expression = raw_str(row.first());
            if expression.is_empty() {
                return Err(err(format!("term {i}: no expression")));
            }
            let reading = Some(raw_str(row.get(1)))
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| expression.clone());
            let glossary = if format == 1 {
                let items: Vec<&str> = row[5..].iter().map(|r| r.get()).collect();
                format!("[{}]", items.join(","))
            } else {
                row.get(5)
                    .map_or_else(|| "[]".to_string(), |r| r.get().to_string())
            };
            let (sequence, term_tags) = if format == 1 {
                (0, String::new())
            } else {
                (raw_f64(row.get(6)) as i64, raw_str(row.get(7)))
            };
            Ok(Term {
                expression,
                reading,
                definition_tags: raw_str(row.get(2)),
                rules: raw_str(row.get(3)),
                score: raw_f64(row.get(4)),
                glossary,
                sequence,
                term_tags,
            })
        })
        .collect()
}

/// A frequency value (DIC-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frequency {
    /// The reading it applies to, when the dictionary says.
    pub reading: Option<String>,
    /// The numeric value; for strings the leading integer, -1 if none.
    pub value: i64,
    /// What to show, if not the number.
    pub display: Option<String>,
}

/// A pitch accent position: a downstep mora, or an H/L pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PitchPosition {
    /// Mora after which the pitch falls (0 = heiban).
    Downstep(u32),
    /// A pattern such as `LHHL`.
    Pattern(String),
}

/// One pitch accent of a reading (LKP-041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pitch {
    /// The position.
    pub position: PitchPosition,
    /// Nasal morae (1-based).
    pub nasal: Vec<u32>,
    /// Devoiced morae (1-based).
    pub devoice: Vec<u32>,
    /// Tags (`v5`, `adj-i`…).
    pub tags: Vec<String>,
}

/// An IPA transcription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ipa {
    /// The transcription.
    pub ipa: String,
    /// Tags.
    pub tags: Vec<String>,
}

/// A term meta entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Meta {
    /// A frequency.
    Frequency(Frequency),
    /// Pitch accents of a reading.
    Pitch {
        /// The reading.
        reading: String,
        /// Its accents.
        pitches: Vec<Pitch>,
    },
    /// IPA transcriptions of a reading.
    Ipa {
        /// The reading.
        reading: String,
        /// The transcriptions.
        transcriptions: Vec<Ipa>,
    },
}

impl Meta {
    /// `freq`, `pitch` or `ipa`.
    pub fn mode(&self) -> &'static str {
        match self {
            Meta::Frequency(_) => "freq",
            Meta::Pitch { .. } => "pitch",
            Meta::Ipa { .. } => "ipa",
        }
    }
}

/// A frequency in any of its shapes: number, string, `{value, displayValue}`.
fn frequency_value(v: &Value) -> Option<(i64, Option<String>)> {
    match v {
        Value::Number(n) => n.as_f64().map(|f| (f as i64, None)),
        Value::String(s) => {
            let digits: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
            Some((digits.parse().unwrap_or(-1), Some(s.clone())))
        }
        Value::Object(o) => {
            let (value, _) = frequency_value(o.get("value")?)?;
            let display = o
                .get("displayValue")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some((value, display))
        }
        _ => None,
    }
}

fn frequency(data: &Value) -> Option<Frequency> {
    if let Some(reading) = data.get("reading").and_then(Value::as_str)
        && let Some(f) = data.get("frequency")
    {
        let (value, display) = frequency_value(f)?;
        return Some(Frequency {
            reading: Some(reading.to_string()),
            value,
            display,
        });
    }
    let (value, display) = frequency_value(data)?;
    Some(Frequency {
        reading: None,
        value,
        display,
    })
}

fn numbers(v: Option<&Value>) -> Vec<u32> {
    match v {
        Some(Value::Number(n)) => n.as_u64().map(|n| vec![n as u32]).unwrap_or_default(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|n| n.as_u64().map(|n| n as u32))
            .collect(),
        _ => Vec::new(),
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_meta(mode: &str, data: &Value) -> Option<Meta> {
    match mode {
        "freq" => frequency(data).map(Meta::Frequency),
        "pitch" => {
            let reading = data.get("reading")?.as_str()?.to_string();
            let pitches = data
                .get("pitches")?
                .as_array()?
                .iter()
                .filter_map(|p| {
                    let position = match p.get("position")? {
                        Value::Number(n) => PitchPosition::Downstep(n.as_u64()? as u32),
                        Value::String(s) => PitchPosition::Pattern(s.clone()),
                        _ => return None,
                    };
                    Some(Pitch {
                        position,
                        nasal: numbers(p.get("nasal")),
                        devoice: numbers(p.get("devoice")),
                        tags: strings(p.get("tags")),
                    })
                })
                .collect();
            Some(Meta::Pitch { reading, pitches })
        }
        "ipa" => {
            let reading = data.get("reading")?.as_str()?.to_string();
            let transcriptions = data
                .get("transcriptions")?
                .as_array()?
                .iter()
                .filter_map(|t| {
                    Some(Ipa {
                        ipa: t.get("ipa")?.as_str()?.to_string(),
                        tags: strings(t.get("tags")),
                    })
                })
                .collect();
            Some(Meta::Ipa {
                reading,
                transcriptions,
            })
        }
        _ => None,
    }
}

/// Parses a `term_meta_bank_*.json` or `kanji_meta_bank_*.json`: `(key, meta)`
/// pairs; unknown modes and malformed rows are skipped.
pub fn parse_meta_bank(bytes: &[u8]) -> Result<Vec<(String, Meta)>, FormatError> {
    let rows: Vec<Value> = serde_json::from_slice(bytes).map_err(|e| err(e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let key = row.get(0)?.as_str()?.to_string();
            let mode = row.get(1)?.as_str()?;
            Some((key, parse_meta(mode, row.get(2)?)?))
        })
        .collect())
}

/// A kanji entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kanji {
    /// The character.
    pub character: String,
    /// On readings, space separated.
    pub onyomi: String,
    /// Kun readings, space separated.
    pub kunyomi: String,
    /// Tags, space separated.
    pub tags: String,
    /// Meanings.
    pub meanings: Vec<String>,
    /// Statistics (strokes, grade, frequency…) as name → value.
    pub stats: Vec<(String, String)>,
}

/// Parses a `kanji_bank_*.json` (format 1 puts the meanings after the tags).
pub fn parse_kanji(bytes: &[u8], format: u32) -> Result<Vec<Kanji>, FormatError> {
    let rows: Vec<Value> = serde_json::from_slice(bytes).map_err(|e| err(e.to_string()))?;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let a = row
                .as_array()
                .ok_or_else(|| err(format!("kanji {i}: not an array")))?;
            let s = |k: usize| {
                a.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let character = s(0);
            if character.is_empty() {
                return Err(err(format!("kanji {i}: no character")));
            }
            let (meanings, stats) = if format == 1 {
                (
                    a[4.min(a.len())..]
                        .iter()
                        .filter_map(|m| m.as_str().map(str::to_string))
                        .collect(),
                    Vec::new(),
                )
            } else {
                let stats = a
                    .get(5)
                    .and_then(Value::as_object)
                    .map(|o| {
                        o.iter()
                            .map(|(k, v)| {
                                (
                                    k.clone(),
                                    v.as_str().map_or_else(|| v.to_string(), str::to_string),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (strings(a.get(4)), stats)
            };
            Ok(Kanji {
                character,
                onyomi: s(1),
                kunyomi: s(2),
                tags: s(3),
                meanings,
                stats,
            })
        })
        .collect()
}

/// A tag (DIC-002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// Name, as used in entries.
    pub name: String,
    /// Category (`partOfSpeech`, `frequent`, `name`…).
    pub category: String,
    /// Sorting order.
    pub order: i64,
    /// Explanation.
    pub notes: String,
    /// Score.
    pub score: f64,
}

/// Parses a `tag_bank_*.json`.
pub fn parse_tags(bytes: &[u8]) -> Result<Vec<Tag>, FormatError> {
    let rows: Vec<Value> = serde_json::from_slice(bytes).map_err(|e| err(e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let a = row.as_array()?;
            let s = |k: usize| {
                a.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Some(Tag {
                name: Some(s(0)).filter(|n| !n.is_empty())?,
                category: s(1),
                order: a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as i64,
                notes: s(3),
                score: a.get(4).and_then(Value::as_f64).unwrap_or(0.0),
            })
        })
        .collect())
}

/// What a dictionary contains (DIC-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Kinds {
    /// Term entries.
    pub terms: bool,
    /// Kanji entries.
    pub kanji: bool,
    /// Frequencies only.
    pub frequency: bool,
    /// Pitch accent or IPA only.
    pub pitch: bool,
}

/// DIC-015: terms if there are term banks (and kanji too if it has kanji
/// banks); otherwise kanji; otherwise the mode of the first meta.
pub fn detect_kinds(term_banks: usize, kanji_banks: usize, first_meta: Option<&str>) -> Kinds {
    if term_banks > 0 {
        return Kinds {
            terms: true,
            kanji: kanji_banks > 0,
            ..Kinds::default()
        };
    }
    if kanji_banks > 0 {
        return Kinds {
            kanji: true,
            ..Kinds::default()
        };
    }
    match first_meta {
        Some("freq") => Kinds {
            frequency: true,
            ..Kinds::default()
        },
        Some("pitch" | "ipa") => Kinds {
            pitch: true,
            ..Kinds::default()
        },
        _ => Kinds::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_fields() {
        let i = parse_index(r#"{"title":"Mini","revision":3,"format":3,"sequenced":true,"frequencyMode":"occurrence-based","sourceLanguage":"ja"}"#.as_bytes()).unwrap();
        assert_eq!(
            (i.title.as_str(), i.revision.as_str(), i.format),
            ("Mini", "3", 3)
        );
        assert_eq!(i.frequency_mode, FrequencyMode::OccurrenceBased);
        assert_eq!(i.source_language.as_deref(), Some("ja"));
        assert!(parse_index(r#"{"revision":"1"}"#.as_bytes()).is_err());
    }

    #[test]
    fn term_rows_in_both_formats() {
        let v3 = parse_terms(
            r#"[["食べる","たべる","v1 vt","v1",1.5,["to eat",{"type":"structured-content","content":"x"}],1358280,"P"],
                 ["猫","",null,"",0,["cat"],2,""]]"#.as_bytes(),
            3,
        )
        .unwrap();
        assert_eq!(v3[0].rules, "v1");
        assert_eq!(v3[0].score, 1.5);
        assert_eq!(v3[0].sequence, 1_358_280);
        assert_eq!(
            v3[0].glossary,
            r#"["to eat",{"type":"structured-content","content":"x"}]"#
        );
        assert_eq!(v3[1].reading, "猫", "an empty reading is the headword");
        let v1 = parse_terms(r#"[["犬","いぬ","n","",0,"dog","hound"]]"#.as_bytes(), 1).unwrap();
        assert_eq!(v1[0].glossary, r#"["dog","hound"]"#);
        assert!(parse_terms(r#"[["x"]]"#.as_bytes(), 3).is_err());
    }

    // T-DIC-003
    #[test]
    fn frequencies_in_every_shape() {
        let m = parse_meta_bank(
            r#"[["a","freq",12],["b","freq","1500㋕"],["c","freq",{"value":7,"displayValue":"7★"}],
                 ["d","freq",{"reading":"でぃー","frequency":{"value":3}}],["e","weird",1]]"#
                .as_bytes(),
        )
        .unwrap();
        let f: Vec<(i64, Option<String>)> = m
            .iter()
            .filter_map(|(_, x)| match x {
                Meta::Frequency(f) => Some((f.value, f.display.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            f,
            [
                (12, None),
                (1500, Some("1500㋕".into())),
                (7, Some("7★".into())),
                (3, None)
            ]
        );
        assert_eq!(m.len(), 4, "unknown modes are skipped");
    }

    #[test]
    fn pitch_and_ipa() {
        let m = parse_meta_bank(
            r#"[["箸","pitch",{"reading":"はし","pitches":[{"position":1,"nasal":2,"devoice":[1],"tags":["n"]},{"position":"LHH"}]}],
                 ["箸","ipa",{"reading":"はし","transcriptions":[{"ipa":"ha̠ɕi","tags":[]}]}]]"#.as_bytes(),
        )
        .unwrap();
        let Meta::Pitch { pitches, .. } = &m[0].1 else {
            panic!()
        };
        assert_eq!(pitches[0].position, PitchPosition::Downstep(1));
        assert_eq!(
            (pitches[0].nasal.as_slice(), pitches[0].devoice.as_slice()),
            (&[2][..], &[1][..])
        );
        assert_eq!(pitches[1].position, PitchPosition::Pattern("LHH".into()));
        assert_eq!(m[1].1.mode(), "ipa");
    }

    #[test]
    fn kanji_and_tags() {
        let k = parse_kanji(
            r#"[["日","ニチ ジツ","ひ -び","jouyou",["day","sun"],{"strokes":"4","grade":"1"}]]"#
                .as_bytes(),
            3,
        )
        .unwrap();
        assert_eq!(k[0].meanings, ["day", "sun"]);
        assert!(
            k[0].stats
                .contains(&("strokes".to_string(), "4".to_string()))
        );
        let t =
            parse_tags(r#"[["v5k","partOfSpeech",-3,"Godan verb with 'ku' ending",0]]"#.as_bytes())
                .unwrap();
        assert_eq!((t[0].category.as_str(), t[0].order), ("partOfSpeech", -3));
    }

    // T-DIC-015
    #[test]
    fn kinds_are_detected() {
        assert_eq!(
            detect_kinds(3, 0, None),
            Kinds {
                terms: true,
                ..Kinds::default()
            }
        );
        assert_eq!(
            detect_kinds(3, 1, None),
            Kinds {
                terms: true,
                kanji: true,
                ..Kinds::default()
            }
        );
        assert_eq!(
            detect_kinds(0, 2, Some("freq")),
            Kinds {
                kanji: true,
                ..Kinds::default()
            }
        );
        assert_eq!(
            detect_kinds(0, 0, Some("freq")),
            Kinds {
                frequency: true,
                ..Kinds::default()
            }
        );
        assert_eq!(
            detect_kinds(0, 0, Some("ipa")),
            Kinds {
                pitch: true,
                ..Kinds::default()
            }
        );
        assert_eq!(detect_kinds(0, 0, None), Kinds::default());
    }
}
