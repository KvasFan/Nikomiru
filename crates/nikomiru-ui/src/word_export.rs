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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::WordStatus;

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
