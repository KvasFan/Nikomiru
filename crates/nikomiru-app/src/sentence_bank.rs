// SPDX-License-Identifier: GPL-3.0-only

//! Bounded offline segmentation of explicitly captured or imported sentences (SEN-001).
use nikomiru_core::Cancel;
use nikomiru_dict::segment::segment;
use nikomiru_library::{
    DictionaryError, DictionaryManager,
    phrases::Source,
    sentences::{BATCH_BYTES, Batch, MAX_SENTENCES, Sentence, Token},
};
use nikomiru_ui::{
    i18n::Translator,
    sentence_bank::{Input, Request},
};
use serde::{
    Deserialize, Deserializer,
    de::{SeqAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    text: String,
    language: String,
    #[serde(default)]
    source: Option<Source>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    version: u32,
    title: String,
    #[serde(deserialize_with = "entries")]
    entries: Vec<Entry>,
}
fn entries<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<Entry>, D::Error> {
    struct Bounded;
    impl<'de> Visitor<'de> for Bounded {
        type Value = Vec<Entry>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 10000 owned sentences")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while let Some(row) = seq.next_element()? {
                if rows.len() >= MAX_SENTENCES {
                    return Err(serde::de::Error::custom("sentence limit"));
                }
                rows.push(row);
            }
            Ok(rows)
        }
    }
    de.deserialize_seq(Bounded)
}
fn hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("bank-error".into());
    }
    let mut bytes = Vec::new();
    f.take(BATCH_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > BATCH_BYTES {
        return Err("bank-error".into());
    }
    Ok(bytes)
}
fn input_valid(input: &Input) -> bool {
    !input.text.is_empty()
        && input.text.len() <= 64 << 10
        && input
            .source
            .as_ref()
            .is_none_or(nikomiru_library::sentences::source_ok)
}
/// Prepare the complete bounded batch using actual dictionary identities and original byte ranges.
pub fn prepare(
    manager: &DictionaryManager,
    request: Request,
    cancel: &Cancel,
    tr: &Translator,
) -> Result<Batch, String> {
    let fail = || tr.tr("bank-error", &[]);
    if cancel.is_cancelled() {
        return Err(tr.tr("bank-cancelled", &[]));
    }
    let dictionary = manager.analysis_identity().map_err(|e| tr.error(&e))?;
    let lookup = manager.lookup().map_err(|e| tr.error(&e))?;
    if !lookup
        .dictionaries()
        .iter()
        .any(|d| d.store.info().kinds.terms)
    {
        return Err(tr.tr("error-dict-unavailable", &[]));
    }
    let mut imported = None;
    let (key, title, language, split, inputs) = match request {
        Request::Captured {
            uid,
            title,
            language,
            split,
            inputs,
        } => {
            let key = format!(
                "{}/{}",
                if split { "book" } else { "track" },
                hex(format!("{uid}\n{language}").as_bytes())
            );
            (key, title, language, split, inputs)
        }
        Request::Import { path, language } => {
            let bytes = read(&path).map_err(|_| fail())?;
            let pack: Pack = serde_json::from_slice(&bytes).map_err(|_| fail())?;
            if pack.version != 1 || pack.entries.iter().any(|e| e.language != language) {
                return Err(fail());
            }
            let key = format!("pack/{}", hex(&bytes));
            let inputs = pack
                .entries
                .into_iter()
                .map(|e| Input {
                    text: e.text,
                    source: e.source,
                })
                .collect();
            imported = Some((path, bytes));
            (key, pack.title, language, false, inputs)
        }
    };
    if inputs.is_empty()
        || inputs.len() > MAX_SENTENCES
        || inputs.iter().any(|i| !input_valid(i))
        || inputs.iter().map(|i| i.text.len()).sum::<usize>() > BATCH_BYTES
    {
        return Err(fail());
    }
    let mut sentences = Vec::new();
    let mut used = 0usize;
    for input in inputs {
        let mut at = 0;
        while at < input.text.len() {
            if cancel.is_cancelled() {
                return Err(tr.tr("bank-cancelled", &[]));
            }
            let range = if split {
                nikomiru_text::sentence::sentence(&input.text, at..at).range
            } else {
                0..input.text.len()
            };
            if range.end <= at {
                break;
            }
            at = range.end;
            if range.is_empty() {
                continue;
            }
            let text = input.text.get(range.clone()).ok_or_else(fail)?;
            if text.len() > 16384 || sentences.len() >= MAX_SENTENCES {
                return Err(fail());
            }
            let mut source = input.source.clone();
            if let Some(s) = &mut source {
                s.offset = s.offset.checked_add(range.start as u32).ok_or_else(fail)?;
            }
            let words = segment(&lookup, text).map_err(|e| tr.error(&DictionaryError::from(e)))?;
            let tokens = words
                .into_iter()
                .filter_map(|w| {
                    w.headword.map(|h| Token {
                        expression: h.expression,
                        reading: h.reading,
                        start: w.range.start as u32,
                        end: w.range.end as u32,
                    })
                })
                .collect::<Vec<_>>();
            let row = Sentence {
                text: text.into(),
                language: language.clone(),
                source,
                tokens,
            };
            if !row.valid() {
                return Err(fail());
            }
            used = used.saturating_add(serde_json::to_vec(&row).map_err(|_| fail())?.len());
            if used > BATCH_BYTES {
                return Err(fail());
            }
            sentences.push(row);
            if !split {
                break;
            }
        }
    }
    if let Some((path, bytes)) = imported
        && read(&path).map_err(|_| fail())? != bytes
    {
        return Err(tr.tr("analysis-source-changed", &[]));
    }
    if manager.analysis_identity().map_err(|e| tr.error(&e))? != dictionary {
        return Err(tr.tr("analysis-source-changed", &[]));
    }
    if cancel.is_cancelled() {
        return Err(tr.tr("bank-cancelled", &[]));
    }
    let batch = Batch {
        key,
        title,
        dictionary: hex(&dictionary),
        sentences,
    };
    if !batch.valid() {
        return Err(fail());
    }
    Ok(batch)
}
#[cfg(test)]
mod tests {
    use super::*;
    // T-SEN-001: cancellation is checked before dictionary access or any source work.
    #[test]
    fn cancelled_index_never_prepares_a_partial_batch() {
        let dir = tempfile::tempdir().unwrap();
        let lib = std::sync::Arc::new(
            nikomiru_library::Library::open(&dir.path().join("l.redb")).unwrap(),
        );
        let manager = DictionaryManager::open(
            lib,
            dir.path().join("dicts"),
            std::sync::Arc::new(nikomiru_core::SystemClock::new()),
            "ja",
        )
        .unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        let request = Request::Captured {
            uid: "book/test".into(),
            title: "Owned".into(),
            language: "ja".into(),
            split: true,
            inputs: Vec::new(),
        };
        assert!(prepare(&manager, request, &cancel, &Translator::new("en")).is_err());
    }
    // T-SEN-001, T-SEN-003: bounded owned packs reject extra fields, versions are checked before segmentation and sources stay identifiers.
    #[test]
    fn owned_sentence_packs_are_data_only_and_bounded() {
        let good =
            r#"{"version":1,"title":"Owned","entries":[{"text":"猫です。","language":"ja"}]}"#;
        assert_eq!(serde_json::from_str::<Pack>(good).unwrap().entries.len(), 1);
        assert!(
            serde_json::from_str::<Pack>(&good.replace(
                "\"version\":1",
                "\"version\":1,\"url\":\"https://example.invalid\""
            ))
            .is_err()
        );
        let rows = vec![r#"{"text":"猫","language":"ja"}"#; MAX_SENTENCES + 1].join(",");
        assert!(
            serde_json::from_str::<Pack>(&format!(
                "{{\"version\":1,\"title\":\"Owned\",\"entries\":[{rows}]}}"
            ))
            .is_err()
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.json");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(BATCH_BYTES as u64 + 1)
            .unwrap();
        assert!(read(&path).is_err());
    }
}
