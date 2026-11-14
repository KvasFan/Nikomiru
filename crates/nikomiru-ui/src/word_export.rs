// SPDX-License-Identifier: GPL-3.0-only

//! Local, bounded TSV snapshots of analysed vocabulary (KNW-022).

use crate::words::ListedWord;

pub(crate) fn field(text: &str) -> String {
    let clean = text.replace(['\t', '\r', '\n'], " ");
    if clean.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{clean}")
    } else {
        clean
    }
}

/// Serialise a snapshot without changing shared word statuses.
/// The existing word-list import accepts the first three TSV columns.
pub fn tsv(rows: &[ListedWord]) -> Result<String, &'static str> {
    let mut out = String::from("expression\treading\tstatus\tcount\tcard\n");
    for row in rows {
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\n",
            field(&row.expression),
            field(&row.reading),
            row.status.name(),
            row.count,
            row.card
        );
        if out.len().saturating_add(line.len()) > nikomiru_library::words::MAX_LIST_BYTES as usize {
            return Err("analysis-export-too-large");
        }
        out.push_str(&line);
    }
    Ok(out)
}

/// All historical words for one media, newest first, independent of display caps.
pub fn encounters_tsv(
    words: &nikomiru_library::encounters::Encounters,
) -> Result<String, &'static str> {
    let mut rows: Vec<_> = words.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let mut out = String::from("expression\treading\tfirst_encounter_ms\n");
    for (key, at) in rows {
        let Some((expression, reading)) = key.split_once('\n') else {
            continue;
        };
        let row = format!("{}\t{}\t{at}\n", field(expression), field(reading));
        if out.len().saturating_add(row.len()) > nikomiru_library::words::MAX_LIST_BYTES as usize {
            return Err("analysis-export-too-large");
        }
        out.push_str(&row);
    }
    Ok(out)
}

struct LimitedBytes(Vec<u8>);
impl std::io::Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 16 << 20 {
            return Err(std::io::Error::other("analysis-export-too-large"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Complete Yomitan v3 frequency dictionary, with exact occurrence values and readings.
pub fn frequency_zip(source: &str, rows: &[ListedWord]) -> Result<Vec<u8>, &'static str> {
    use std::io::Write;
    if rows.is_empty() {
        return Err("frequency-export-invalid");
    }
    if rows.len() > 100_000 {
        return Err("analysis-export-too-large");
    }
    let mut identities = std::collections::HashSet::new();
    let mut bytes = 0usize;
    for row in rows {
        bytes = bytes
            .saturating_add(row.expression.len())
            .saturating_add(row.reading.len());
        if bytes > 8 << 20 {
            return Err("analysis-export-too-large");
        }
        if row.expression.trim().is_empty()
            || row
                .expression
                .chars()
                .chain(row.reading.chars())
                .any(char::is_control)
            || row.count == 0
            || i64::try_from(row.count).is_err()
            || !identities.insert(nikomiru_library::words::word_key(
                &row.expression,
                &row.reading,
            ))
        {
            return Err("frequency-export-invalid");
        }
    }
    let mut sorted: Vec<_> = rows.iter().collect();
    sorted.sort_by(|a, b| {
        a.expression
            .cmp(&b.expression)
            .then_with(|| a.reading.cmp(&b.reading))
    });
    let mut bank = LimitedBytes(Vec::new());
    bank.write_all(b"[")
        .map_err(|_| "analysis-export-too-large")?;
    for (i, row) in sorted.into_iter().enumerate() {
        if i != 0 {
            bank.write_all(b",")
                .map_err(|_| "analysis-export-too-large")?;
        }
        serde_json::to_writer(&mut bank, &serde_json::json!([row.expression, "freq", {"reading":row.reading, "frequency":row.count}])).map_err(|_| "analysis-export-too-large")?;
    }
    bank.write_all(b"]")
        .map_err(|_| "analysis-export-too-large")?;
    let source: String = source
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect();
    let source = if source.trim().is_empty() {
        "Study"
    } else {
        source.trim()
    };
    let index = serde_json::to_vec(&serde_json::json!({"title":format!("Nikomiru frequency: {source}"), "format":3, "revision":uuid::Uuid::new_v4().to_string(), "frequencyMode":"occurrence-based", "sequenced":false, "description":"Occurrence counts from a local study analysis; no definitions or sentences."})).map_err(|_| "frequency-export-invalid")?;
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, bytes) in [
        ("index.json", index.as_slice()),
        ("term_meta_bank_1.json", bank.0.as_slice()),
    ] {
        archive
            .start_file(name, options)
            .map_err(|_| "frequency-export-invalid")?;
        archive
            .write_all(bytes)
            .map_err(|_| "frequency-export-invalid")?;
    }
    let out = archive
        .finish()
        .map_err(|_| "frequency-export-invalid")?
        .into_inner();
    if out.len() > 16 << 20 {
        return Err("analysis-export-too-large");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;

    // T-KNW-038: the emitted archive parses as a v3 occurrence-based dictionary and retains readings.
    #[test]
    fn frequency_dictionary_round_trips_and_rejects_invalid_identities() {
        use std::io::Read;
        let row = |e: &str, r: &str, n| ListedWord {
            expression: e.into(),
            reading: r.into(),
            count: n,
            status: WordStatus::Unknown,
            card: false,
        };
        let rows = vec![
            row("生", "なま", 7),
            row("猫", "ねこ", 3),
            row("生", "せい", 12),
        ];
        let bytes = frequency_zip("Episode 日本語", &rows).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        let mut index = Vec::new();
        zip.by_name("index.json")
            .unwrap()
            .read_to_end(&mut index)
            .unwrap();
        let index = nikomiru_dict::yomitan::parse_index(&index).unwrap();
        assert!(index.title.contains("Episode 日本語"));
        assert_eq!(
            index.frequency_mode,
            nikomiru_dict::yomitan::FrequencyMode::OccurrenceBased
        );
        let mut bank = Vec::new();
        zip.by_name("term_meta_bank_1.json")
            .unwrap()
            .read_to_end(&mut bank)
            .unwrap();
        let parsed = nikomiru_dict::yomitan::parse_meta_bank(&bank, &nikomiru_core::Cancel::new())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.len(), 3);
        assert!(parsed.iter().any(|(e,m)| e == "生" && matches!(m, nikomiru_dict::yomitan::Meta::Frequency(f) if f.value == 12 && f.reading.as_deref() == Some("せい"))));
        assert!(frequency_zip("x", &[rows[0].clone(), rows[0].clone()]).is_err());
        assert!(frequency_zip("x", &[row("猫", "ねこ", 0)]).is_err());
        assert!(frequency_zip("x", &[row("", "", 1)]).is_err());
        assert!(frequency_zip("x", &[]).is_err());
        assert_eq!(
            frequency_zip("x", &[row(&"x".repeat((8 << 20) + 1), "", 1)]),
            Err("analysis-export-too-large")
        );
    }

    // T-KNW-038: the existing importer/store accepts the exported ZIP and preserves occurrence metadata.
    #[test]
    fn exported_frequency_dictionary_imports_into_the_existing_store() {
        use nikomiru_dict::store::{DictStore, RedbStore};
        let root = tempfile::tempdir().unwrap();
        let row = ListedWord {
            expression: "猫".into(),
            reading: "ねこ".into(),
            count: 42,
            status: WordStatus::Learning,
            card: true,
        };
        let file = root.path().join("frequency.zip");
        std::fs::write(&file, frequency_zip("Local episode", &[row]).unwrap()).unwrap();
        let imported = nikomiru_dict::import::import(
            &file,
            &root.path().join("dicts"),
            &nikomiru_core::Cancel::new(),
            |_| {},
        )
        .unwrap();
        let store = RedbStore::open(&imported.dir.join("dict.redb")).unwrap();
        assert_eq!(
            store.info().index.frequency_mode,
            nikomiru_dict::yomitan::FrequencyMode::OccurrenceBased
        );
        assert!(store.term_meta("猫").unwrap().iter().any(|m| matches!(m, nikomiru_dict::yomitan::Meta::Frequency(f) if f.value == 42 && f.reading.as_deref() == Some("ねこ"))));
    }

    // T-KNW-030: export full history beyond display cap and preserve first times.
    #[test]
    fn history_export_includes_all_rows_with_original_timestamps() {
        let mut words = nikomiru_library::encounters::Encounters::new();
        for i in 0..1005 {
            words.insert(format!("word{i}\nreading{i}"), i);
        }
        words.insert("=formula\nread\ting".into(), 2000);
        let text = encounters_tsv(&words).unwrap();
        assert_eq!(text.lines().count(), 1007);
        assert!(text.starts_with("expression\treading\tfirst_encounter_ms\n'=formula\tread ing\t2000\nword1004\treading1004\t1004\n"));
        assert!(text.ends_with("word0\treading0\t0\n"));
        assert_eq!(words.len(), 1006);
    }

    // T-KNW-022: literal UTF-8 fields, spreadsheet safety and whole-output bounds.
    #[test]
    fn tsv_retains_counts_status_and_card_and_never_exports_a_partial_list() {
        let mut rows = vec![
            ListedWord {
                expression: "猫".into(),
                reading: "ねこ".into(),
                count: 3,
                status: WordStatus::Learning,
                card: true,
            },
            ListedWord {
                expression: " =1+2\tX".into(),
                reading: "a\nb".into(),
                count: 1,
                status: WordStatus::Unknown,
                card: false,
            },
        ];
        let text = tsv(&rows).unwrap();
        assert!(text.starts_with("expression\treading\tstatus\tcount\tcard\n"));
        assert!(text.contains("猫\tねこ\tlearning\t3\ttrue\n"));
        assert!(text.contains("' =1+2 X\ta b\tunknown\t1\tfalse\n"));
        assert_eq!(
            nikomiru_library::words::parse_list(&text)[0],
            ("猫".into(), "ねこ".into(), WordStatus::Learning)
        );
        rows[0].expression = "a".repeat(nikomiru_library::words::MAX_LIST_BYTES as usize);
        assert_eq!(tsv(&rows), Err("analysis-export-too-large"));
    }
}
