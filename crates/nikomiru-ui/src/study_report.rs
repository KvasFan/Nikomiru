// SPDX-License-Identifier: GPL-3.0-only

//! Bounded snapshots of retained complete comprehension analyses (KNW-033).

use crate::words::Score;

/// Which library view's completed analyses are exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Completed whole-book analyses.
    Books,
    /// Completed series-card analyses.
    Series,
}

/// One completed analysis, already scored with current shared word statuses.
#[derive(Debug)]
pub struct Row {
    /// Library identity, separate from local file paths.
    pub uid: String,
    /// Original display title.
    pub title: String,
    /// Weighted token/text-unit totals, not averaged percentages.
    pub score: Score,
    /// Analysed, total and failed episodes; absent for books.
    pub coverage: Option<(usize, usize, usize)>,
}

/// Deterministic sanitised UTF-8 TSV with blank undefined percentages/coverage.
pub fn tsv(scope: Scope, mut rows: Vec<Row>) -> Result<String, &'static str> {
    if rows.is_empty() {
        return Err("study-report-empty");
    }
    rows.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.uid.cmp(&b.uid)));
    let mut out = String::from(
        "kind\ttitle\tuid\tword_tokens\tknown_tokens\tknown_percent\ttext_units\tclear_units\tclear_percent\tanalysed_episodes\ttotal_episodes\tfailed_sources\n",
    );
    let percent = |n: Option<usize>| n.map_or_else(String::new, |n| n.to_string());
    for row in rows {
        let (analysed, total, failed) = row.coverage.map_or_else(
            || (String::new(), String::new(), String::new()),
            |(a, t, f)| (a.to_string(), t.to_string(), f.to_string()),
        );
        let line = format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            match scope {
                Scope::Books => "book",
                Scope::Series => "series",
            },
            crate::word_export::field(&row.title),
            crate::word_export::field(&row.uid),
            row.score.words,
            row.score.known,
            percent(row.score.known_percent()),
            row.score.lines,
            row.score.clear,
            percent(row.score.clear_percent()),
            analysed,
            total,
            failed
        );
        if out.len().saturating_add(line.len()) > 16 << 20 {
            return Err("analysis-export-too-large");
        }
        out.push_str(&line);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-KNW-033: correct scope/totals/coverage, blank undefined ratios and stable safe fields.
    #[test]
    fn reports_snapshot_totals_and_coverage_without_inventing_percentages() {
        let row = |title: &str, uid: &str, score, coverage| Row {
            title: title.into(),
            uid: uid.into(),
            score,
            coverage,
        };
        let text = tsv(
            Scope::Series,
            vec![
                row("Z", "s/2", Score::default(), Some((1, 3, 1))),
                row(
                    "=A\tB",
                    "s/1",
                    Score {
                        words: 10,
                        known: 3,
                        lines: 4,
                        clear: 2,
                    },
                    Some((2, 5, 1)),
                ),
            ],
        )
        .unwrap();
        assert!(text.contains("series\t'=A B\ts/1\t10\t3\t30\t4\t2\t50\t2\t5\t1\n"));
        assert!(text.ends_with("series\tZ\ts/2\t0\t0\t\t0\t0\t\t1\t3\t1\n"));
        let book = tsv(
            Scope::Books,
            vec![row(
                "本",
                "book/1",
                Score {
                    words: 3,
                    known: 2,
                    lines: 1,
                    clear: 1,
                },
                None,
            )],
        )
        .unwrap();
        assert!(book.ends_with("book\t本\tbook/1\t3\t2\t66\t1\t1\t100\t\t\t\n"));
        assert_eq!(tsv(Scope::Books, vec![]), Err("study-report-empty"));
        assert_eq!(
            tsv(
                Scope::Books,
                vec![row(
                    &"a".repeat((16 << 20) + 1),
                    "book/large",
                    Score::default(),
                    None
                )]
            ),
            Err("analysis-export-too-large")
        );
    }
}
