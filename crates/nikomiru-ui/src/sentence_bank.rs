// SPDX-License-Identifier: GPL-3.0-only

//! Explicit owned-source sentence indexing and stamped offline queries (SEN-001…SEN-003).
use nikomiru_core::Cancel;
use nikomiru_library::{
    phrases::Source,
    sentences::{Batch, Example, Summary},
};
use std::path::PathBuf;
/// Original reader paragraph or complete primary cue, frozen at the click.
#[derive(Debug, Clone)]
pub struct Input {
    /// Original plain text.
    pub text: String,
    /// Original library position.
    pub source: Option<Source>,
}
/// Work for the dictionary worker; no source is fetched from the network.
#[derive(Debug, Clone)]
pub enum Request {
    /// Index captured original text, preserving reader/track positions.
    Captured {
        /// Source identity used for atomic replacement.
        uid: String,
        /// Source label.
        title: String,
        /// Study language.
        language: String,
        /// True to split book paragraphs into sentences.
        split: bool,
        /// Original text and locations.
        inputs: Vec<Input>,
    },
    /// Read a bounded owned JSON sentence pack off UI.
    Import {
        /// Explicitly chosen local file.
        path: PathBuf,
        /// Current dictionary study language.
        language: String,
    },
}
/// Ordered database operation; bulk segmentation is performed separately.
#[derive(Debug, Clone)]
pub enum Job {
    /// Read source metadata.
    List,
    /// Exact expression/reading/language query, stamped against later searches.
    Query {
        /// Monotonically increasing query epoch.
        epoch: u64,
        /// Exact expression.
        expression: String,
        /// Exact reading.
        reading: String,
        /// Study language.
        language: String,
    },
    /// Atomic whole-source replacement.
    Save(Box<Batch>),
    /// Explicit source deletion.
    Delete(String),
}
/// A bounded metadata/query snapshot for the event loop.
#[derive(Debug)]
pub enum Data {
    /// Batch metadata after load/save/delete.
    Batches(Vec<Summary>),
    /// Stamped exact results.
    Examples(Vec<Example>),
}
/// Operation kind for success and failure replies.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// Metadata load.
    List,
    /// Query epoch.
    Query(u64),
    /// Index commit completed.
    Saved,
    /// Explicit deletion completed.
    Deleted,
}
/// An ordered database reply, including errors, with the query stamp intact.
#[derive(Debug)]
pub struct Answer {
    /// What completed.
    pub kind: Kind,
    /// Complete result.
    pub result: Result<Data, String>,
}
/// Browser state; indexing never changes any word/phrase/review state.
#[derive(Debug, Default)]
pub struct State {
    /// Browser covers source pages.
    pub open: bool,
    /// Retained source/pack summaries.
    pub batches: Vec<Summary>,
    /// Complete current displayed query results.
    pub examples: Vec<Example>,
    /// Exact expression entry.
    pub expression: String,
    /// Exact reading entry.
    pub reading: String,
    /// Index/commit in progress.
    pub indexing: bool,
    /// Exact query in progress.
    pub querying: bool,
    /// Last translated error.
    pub error: Option<String>,
    /// Current query epoch.
    pub epoch: u64,
    /// Cancellation token while segmentation is in progress.
    pub cancel: Option<Cancel>,
    /// Whether the current index belongs to Books.
    pub(crate) book_index: bool,
    /// Frozen source request to dispatch.
    pub(crate) prepare: Option<(Request, Cancel)>,
    /// Ordered library work to dispatch.
    pub(crate) jobs: Vec<Job>,
}
impl State {
    /// Reject both success and failure replies from an older query before altering visible state.
    pub fn accept_reply(&mut self, kind: Kind) -> bool {
        if let Kind::Query(epoch) = kind {
            if epoch != self.epoch {
                return false;
            }
            self.querying = false;
        }
        if matches!(kind, Kind::Saved) {
            self.indexing = false;
        }
        true
    }
}
/// Serialize only the shown frozen results, safely for spreadsheets.
pub fn tsv(rows: &[Example]) -> Result<String, &'static str> {
    use crate::word_export::field;
    if rows.len() > 50 {
        return Err("bank-error");
    }
    let mut out = String::from(
        "sentence\tlanguage\tprovenance\tuid\tchapter\tparagraph\toffset\tstart_ms\tend_ms\tmatched_bytes\n",
    );
    for row in rows {
        if !row.sentence.valid() {
            return Err("bank-error");
        }
        let mut fields = vec![
            field(&row.sentence.text),
            field(&row.sentence.language),
            field(&row.title),
        ];
        if let Some(s) = &row.sentence.source {
            fields.extend([
                field(&s.uid),
                s.chapter.to_string(),
                s.paragraph.to_string(),
                s.offset.to_string(),
                s.start_ms.to_string(),
                s.end_ms.to_string(),
            ]);
        } else {
            fields.extend(std::iter::repeat_n(String::new(), 6));
        }
        fields.push(
            row.ranges
                .iter()
                .map(|(s, e)| format!("{s}:{e}"))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str(&fields.join("\t"));
        out.push('\n');
        if out.len() > 8 << 20 {
            return Err("bank-error");
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-SEN-002: an older query's success or failure cannot finish or replace a newer request.
    #[test]
    fn only_current_query_replies_are_accepted() {
        let mut state = State {
            epoch: 2,
            querying: true,
            ..Default::default()
        };
        assert!(!state.accept_reply(Kind::Query(1)));
        assert!(state.querying);
        assert!(state.accept_reply(Kind::Query(2)));
        assert!(!state.querying);
    }
    // T-SEN-003: literal source text remains neutralised and byte ranges remain original in a shown snapshot.
    #[test]
    fn shown_example_export_is_literal_bounded_and_formula_safe() {
        let row = Example {
            sentence: nikomiru_library::sentences::Sentence {
                text: "=猫\n猫".into(),
                language: "ja".into(),
                source: None,
                tokens: Vec::new(),
            },
            batch: "pack/a".into(),
            title: "@owned".into(),
            ranges: vec![(1, 4)],
        };
        let text = tsv(&[row]).unwrap();
        assert!(text.contains("'=猫 猫\tja\t'@owned"));
        assert!(text.ends_with("1:4\n"));
    }
}
