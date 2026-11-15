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

/// Standalone bounded vocabulary practice sheet; readings are revealed explicitly (KNW-048).
pub fn html_sheet(
    source: &str,
    rows: &[ListedWord],
    labels: &[String; 7],
) -> Result<String, &'static str> {
    use std::fmt::Write;
    if rows.is_empty() || rows.len() > 100_000 {
        return Err("analysis-export-too-large");
    }
    fn escaped(out: &mut String, text: &str) -> Result<(), &'static str> {
        for c in text.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                _ => out.push(c),
            }
            if out.len() > 16 << 20 {
                return Err("analysis-export-too-large");
            }
        }
        Ok(())
    }
    let mut out = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Nikomiru</title><style>body{max-width:52rem;margin:2rem auto;padding:1rem;font:1.1rem system-ui}article{border-bottom:1px solid #aaa;padding:1rem 0}p{white-space:pre-wrap}summary{cursor:pointer}</style></head><body><h1>",
    );
    escaped(&mut out, source)?;
    out.push_str("</h1>\n");
    for row in rows {
        out.push_str("<article><p>");
        escaped(&mut out, &row.expression)?;
        out.push_str("</p>");
        if !row.reading.trim().is_empty() && row.reading != row.expression {
            out.push_str("<details><summary>");
            escaped(&mut out, &labels[0])?;
            out.push_str("</summary><p>");
            escaped(&mut out, &row.reading)?;
            out.push_str("</p></details>");
        }
        let status = match row.status {
            crate::words::WordStatus::Unknown => 1,
            crate::words::WordStatus::Learning => 2,
            crate::words::WordStatus::Known => 3,
            crate::words::WordStatus::Ignored => 4,
        };
        out.push_str("<small>");
        escaped(&mut out, &labels[status])?;
        out.push_str(" · ");
        escaped(&mut out, &labels[5])?;
        let _ = write!(out, ": {} · ", row.count);
        escaped(&mut out, &labels[6])?;
        out.push_str(if row.card { ": ✓" } else { ": —" });
        out.push_str("</small></article>\n");
        if out.len() > (16 << 20) - 64 {
            return Err("analysis-export-too-large");
        }
    }
    out.push_str("</body></html>\n");
    Ok(out)
}

/// Complete bounded exact-identity cue index shared by reports and recall hints.
pub(crate) fn concordance_indices(
    words: &crate::words::Words,
    cues: &[nikomiru_subs::Cue],
    identities: &[(&str, &str)],
) -> Result<Vec<Vec<usize>>, &'static str> {
    use std::collections::{HashMap, HashSet};
    if identities.is_empty() || identities.len() > 1000 || cues.len() > 100_000 {
        return Err("vocabulary-examples-unavailable");
    }
    let mut wanted = HashMap::new();
    let mut bytes = 0usize;
    for (i, (expression, reading)) in identities.iter().enumerate() {
        bytes = bytes
            .saturating_add(expression.len())
            .saturating_add(reading.len());
        if bytes > 8 << 20 || wanted.insert((*expression, *reading), i).is_some() {
            return Err("analysis-export-too-large");
        }
    }
    let mut hits = vec![Vec::new(); identities.len()];
    let mut matches = 0usize;
    let mut source_bytes = 0usize;
    for (cue_index, cue) in cues.iter().enumerate() {
        source_bytes = source_bytes.saturating_add(cue.plain_text.len());
        if source_bytes > 16 << 20 {
            return Err("analysis-export-too-large");
        }
        if cue.plain_text.trim().is_empty() {
            continue;
        }
        let split = words
            .get(&cue.plain_text)
            .ok_or("vocabulary-examples-unavailable")?;
        let mut seen = HashSet::new();
        for head in split.iter().filter_map(|w| w.headword.as_ref()) {
            if let Some(&row) = wanted.get(&(head.expression.as_str(), head.reading.as_str()))
                && seen.insert(row)
            {
                matches += 1;
                if matches > 100_000 {
                    return Err("analysis-export-too-large");
                }
                hits[row].push(cue_index);
            }
        }
    }
    Ok(hits)
}

/// Exact current-track headword examples, indexed once and emitted as a frozen bounded TSV (KNW-051).
pub fn concordance_tsv(
    words: &crate::words::Words,
    cues: &[nikomiru_subs::Cue],
    rows: &[ListedWord],
    delay: i64,
) -> Result<String, &'static str> {
    let identities = rows
        .iter()
        .map(|w| (w.expression.as_str(), w.reading.as_str()))
        .collect::<Vec<_>>();
    let hits = concordance_indices(words, cues, &identities)?;
    if hits.iter().any(Vec::is_empty) {
        return Err("vocabulary-examples-unavailable");
    }
    let mut out = String::from("expression\treading\tstart_ms\tend_ms\ttext\n");
    let mut written = 0;
    for (word, indices) in rows.iter().zip(hits) {
        for i in indices {
            let cue = &cues[i];
            let start = cue.start_ms.saturating_add(delay).max(0);
            let end = cue.end_ms.saturating_add(delay);
            if end <= start {
                continue;
            }
            let line = format!(
                "{}\t{}\t{start}\t{end}\t{}\n",
                field(&word.expression),
                field(&word.reading),
                field(&cue.plain_text)
            );
            if out.len().saturating_add(line.len()) > 16 << 20 {
                return Err("analysis-export-too-large");
            }
            out.push_str(&line);
            written += 1;
        }
    }
    if written == 0 {
        return Err("vocabulary-examples-unavailable");
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
    } // T-KNW-048: all user text/labels escape, readings stay hidden, and output expansion is bounded.
    #[test]
    fn vocabulary_html_is_complete_escaped_and_bounded() {
        let mut row = ListedWord {
            expression: "<猫>&".into(),
            reading: "ねこ\"".into(),
            count: usize::MAX,
            status: crate::words::WordStatus::Known,
            card: true,
        };
        let labels = [
            "<Reveal>", "new", "learning", "known", "ignored", "Count", "Card",
        ]
        .map(String::from);
        let html = html_sheet("<Source>", &[row.clone()], &labels).unwrap();
        assert!(html.contains("&lt;Source&gt;") && html.contains("&lt;猫&gt;&amp;"));
        assert!(
            html.contains("<details><summary>&lt;Reveal&gt;</summary><p>ねこ&quot;</p></details>")
        );
        assert!(html.contains(&usize::MAX.to_string()) && html.contains("known"));
        row.expression = "&".repeat(4 << 20);
        assert_eq!(
            html_sheet("", &[row], &labels),
            Err("analysis-export-too-large")
        );
        assert!(html_sheet("", &[], &labels).is_err());
    }
}
