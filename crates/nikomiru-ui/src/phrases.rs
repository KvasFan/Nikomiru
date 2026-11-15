// SPDX-License-Identifier: GPL-3.0-only

//! Bounded local phrase notebook jobs and exports (PHR-001…PHR-003).
use nikomiru_library::phrases::Phrase;

/// A serial library operation.
#[derive(Debug, Clone)]
pub enum Job {
    /// Refresh the notebook.
    List,
    /// Explicitly save one draft.
    Save(Phrase),
    /// Explicit deletion by stable id.
    Delete(String),
}
/// An operation's complete resulting snapshot, or a failure.
pub type Answer = Result<(Vec<Phrase>, bool), String>;
/// The notebook's retained local state.
#[derive(Debug, Default)]
pub struct State {
    /// Most recently committed snapshot.
    pub items: Vec<Phrase>,
    /// Explicit editable draft; retained on write failure.
    pub editor: Option<Phrase>,
    /// A read/write is pending.
    pub busy: bool,
    /// Last operation error.
    pub error: Option<String>,
}
/// Deterministic search over phrase, meaning, context and source title.
pub fn matches(p: &Phrase, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    [&p.text, &p.definition, &p.context]
        .into_iter()
        .any(|s| s.to_lowercase().contains(&q))
        || p.source
            .as_ref()
            .is_some_and(|s| s.title.to_lowercase().contains(&q))
}
/// Export the exact shown snapshot, neutralising spreadsheet formulas in every text column.
pub fn tsv(rows: &[Phrase]) -> Result<String, &'static str> {
    use crate::word_export::field;
    let mut out = String::from(
        "phrase\tdefinition\tcontext\tstatus\tlanguage\tsource\tuid\tchapter\tparagraph\toffset\tstart_ms\tend_ms\n",
    );
    if rows.len() > nikomiru_library::phrases::MAX_RECORDS {
        return Err("phrases-error");
    }
    for p in rows {
        if !p.valid() {
            return Err("phrases-error");
        }
        let mut fields = vec![
            field(&p.text),
            field(&p.definition),
            field(&p.context),
            p.status.name().into(),
            field(&p.language),
        ];
        if let Some(s) = &p.source {
            fields.extend([
                field(&s.title),
                field(&s.uid),
                s.chapter.to_string(),
                s.paragraph.to_string(),
                s.offset.to_string(),
                s.start_ms.to_string(),
                s.end_ms.to_string(),
            ]);
        } else {
            fields.extend(std::iter::repeat_n(String::new(), 7));
        }
        let line = fields.join("\t");
        if out.len().saturating_add(line.len() + 1) > nikomiru_library::phrases::MAX_BYTES {
            return Err("phrases-error");
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn phrase() -> Phrase {
        Phrase {
            id: "00000000-0000-4000-8000-000000000001".into(),
            language: "ja".into(),
            text: "猫が好き".into(),
            definition: "=definition\tA".into(),
            context: "猫が好きです。".into(),
            status: crate::words::WordStatus::Learning,
            source: None,
            created_ms: 1,
            changed_ms: 1,
        }
    }
    // T-PHR-002: frozen UTF-8 exports neutralise formulas; search includes meaning and source title.
    #[test]
    fn phrase_export_retains_literal_context_and_independent_status() {
        let p = phrase();
        let text = tsv(std::slice::from_ref(&p)).unwrap();
        assert!(text.contains("猫が好き\t'=definition A\t猫が好きです。\tlearning\tja"));
        assert!(matches(&p, "DEFINITION"));
        assert!(!matches(&p, "犬"));
        let mut bad = p;
        bad.text = "犬".into();
        assert!(tsv(&[bad]).is_err());
    }
}
