// SPDX-License-Identifier: GPL-3.0-only

//! Requirements ↔ tests ↔ milestones traceability (TEST §8).
//!
//! - Requirements are the cells and paragraphs of SPECIFICATION.md that start with an ID.
//! - Milestones, their status and their assigned requirements come from ROADMAP.md.
//! - Tests are recognized by the markers in source comments (`crates/`, `tests/`,
//!   `xtask/`): the prefix `T-` followed by the requirement ID, with an optional
//!   lowercase suffix.
//!
//! A marker is valid if it matches a requirement, the "Measure" column of an NFR
//! (SPECIFICATION §7) or a case in the TEST.md catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::util::{rel, split_comment, walk};

/// Key of a test: prefix (`PLY`, `E2E-ANKI`, …) and number; `None` = any number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestKey {
    /// The part between `T-` and the number.
    pub prefix: String,
    /// Test number; `None` in `T-XXX-*` patterns.
    pub number: Option<u32>,
}

impl TestKey {
    fn matches(&self, other: &TestKey) -> bool {
        self.prefix == other.prefix && (self.number.is_none() || self.number == other.number)
    }
}

/// Splits `T-<prefix>-<number>[-suffix | …number | /number…]` into one or more keys.
pub fn parse_test_id(id: &str) -> Vec<TestKey> {
    // Ranges: `T-LKP-020…024` or `T-KEY-001…T-KEY-004`.
    if let Some((left, right)) = id.split_once('…') {
        let Some(TestKey {
            prefix,
            number: Some(a),
        }) = parse_test_id(left).into_iter().next()
        else {
            return Vec::new();
        };
        // the final number is the last group of digits, in both forms
        let b = right
            .split(|c: char| !c.is_ascii_digit())
            .rfind(|s| !s.is_empty())
            .and_then(|s| s.parse::<u32>().ok());
        return match b {
            Some(b) if a <= b => (a..=b)
                .map(|n| TestKey {
                    prefix: prefix.clone(),
                    number: Some(n),
                })
                .collect(),
            _ => Vec::new(),
        };
    }
    let Some(body) = id.strip_prefix("T-") else {
        return Vec::new();
    };
    let parts: Vec<&str> = body.split('-').collect();
    let Some(pos) = parts
        .iter()
        .position(|p| *p == "*" || p.starts_with(|c: char| c.is_ascii_digit()))
    else {
        return Vec::new();
    };
    if pos == 0 {
        return Vec::new();
    }
    let prefix = parts[..pos].join("-");
    if parts[pos] == "*" {
        return vec![TestKey {
            prefix,
            number: None,
        }];
    }
    parts[pos]
        .split('/')
        .filter_map(|s| s.parse::<u32>().ok())
        .map(|n| TestKey {
            prefix: prefix.clone(),
            number: Some(n),
        })
        .collect()
}

/// Requirements of SPECIFICATION.md and the tests listed for the NFRs.
#[derive(Debug, Default)]
pub struct Spec {
    /// IDs of all the defined requirements.
    pub reqs: BTreeSet<String>,
    /// Removed (struck-through) requirements: they stay in the document but need no tests.
    pub removed: BTreeSet<String>,
    /// For every NFR, the tests in the "Measure" column.
    pub nfr_tests: BTreeMap<String, Vec<TestKey>>,
}

const REQ_ID: &str = r"[A-Z]{2,4}-\d{3}|NFR-[A-Z0-9]+-\d{2}";

/// Reads the requirements from SPECIFICATION.md.
pub fn parse_spec(md: &str) -> Result<Spec> {
    let row = Regex::new(&format!(r"^\|\s*(~~)?({REQ_ID})(?:~~)?\s*\|"))?;
    let para = Regex::new(&format!(r"^\s*(?:[-*]\s+)?(?:\*\*)?({REQ_ID})\b"))?;
    let test = Regex::new(r"T-[A-Z0-9]+(?:-[A-Z0-9]+)*-(?:\d+|\*)")?;
    let mut spec = Spec::default();
    for line in md.lines() {
        if let Some(c) = row.captures(line) {
            let id = c[2].to_string();
            if c.get(1).is_some() {
                spec.removed.insert(id.clone());
            }
            if id.starts_with("NFR-") {
                let last = line
                    .trim()
                    .trim_end_matches('|')
                    .rsplit('|')
                    .next()
                    .unwrap_or("");
                let keys = test
                    .find_iter(last)
                    .flat_map(|m| parse_test_id(m.as_str()))
                    .collect();
                spec.nfr_tests.insert(id.clone(), keys);
            }
            spec.reqs.insert(id);
        } else if let Some(c) = para.captures(line) {
            spec.reqs.insert(c[1].to_string());
        }
    }
    if spec.reqs.is_empty() {
        bail!("SPECIFICATION: no requirements found");
    }
    Ok(spec)
}

/// A ROADMAP milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    /// `M0`, `M1`, …
    pub id: String,
    /// `not started`, `in progress` or `done`.
    pub status: String,
    /// Assigned requirements (ranges and `*` already expanded).
    pub reqs: Vec<String>,
}

/// Reads the milestones from ROADMAP.md; `problems` receives the IDs that do not exist.
pub fn parse_roadmap(md: &str, spec: &Spec, problems: &mut Vec<String>) -> Result<Vec<Milestone>> {
    let status_row = Regex::new(r"^\|\s*(M\d+[a-z]?)\s*\|")?;
    let heading = Regex::new(r"^##\s+(M\d+[a-z]?)\b")?;
    let range = Regex::new(&format!(r"({REQ_ID})\s*(?:…|\.\.\.)\s*({REQ_ID})"))?;
    let wildcard = Regex::new(r"\b(NFR-[A-Z0-9]+)-\*")?;
    let single = Regex::new(&format!(r"\b({REQ_ID})\b"))?;

    let mut statuses = BTreeMap::new();
    let mut reqs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in md.lines() {
        if let Some(c) = status_row.captures(line) {
            let status = line
                .trim()
                .trim_end_matches('|')
                .rsplit('|')
                .next()
                .unwrap_or("")
                .trim();
            statuses.insert(c[1].to_string(), status.to_string());
        }
        if let Some(c) = heading.captures(line) {
            current = Some(c[1].to_string());
        } else if line.starts_with("## ") {
            current = None;
        }
        let Some(m) = &current else { continue };
        let Some(text) = line.strip_prefix("**Requirements**:") else {
            continue;
        };
        let list = reqs.entry(m.clone()).or_default();
        let mut rest = text.to_string();
        for c in range.captures_iter(text) {
            let (a, b) = (&c[1], &c[2]);
            let (pa, na) = split_id(a);
            let (pb, nb) = split_id(b);
            if pa != pb || na > nb {
                problems.push(format!("ROADMAP {m}: invalid range {a}…{b}"));
                continue;
            }
            for r in [a, b] {
                if !spec.reqs.contains(r) {
                    problems.push(format!("ROADMAP {m}: {r} does not exist in SPECIFICATION"));
                }
            }
            list.extend(
                spec.reqs
                    .iter()
                    .filter(|r| {
                        let (p, n) = split_id(r);
                        p == pa && (na..=nb).contains(&n) && !spec.removed.contains(*r)
                    })
                    .cloned(),
            );
            rest = rest.replace(&c[0], " ");
        }
        for c in wildcard.captures_iter(text) {
            let prefix = format!("{}-", &c[1]);
            list.extend(spec.reqs.iter().filter(|r| r.starts_with(&prefix)).cloned());
            rest = rest.replace(&c[0], " ");
        }
        for c in single.captures_iter(&rest) {
            let id = c[1].to_string();
            if spec.reqs.contains(&id) {
                list.push(id);
            } else {
                problems.push(format!("ROADMAP {m}: {id} does not exist in SPECIFICATION"));
            }
        }
    }
    let mut out = Vec::new();
    for (id, status) in statuses {
        let mut r = reqs.remove(&id).unwrap_or_default();
        r.sort();
        r.dedup();
        out.push(Milestone {
            id,
            status,
            reqs: r,
        });
    }
    for id in reqs.keys() {
        problems.push(format!("ROADMAP: {id} is not in the milestone table"));
    }
    Ok(out)
}

fn split_id(id: &str) -> (&str, u32) {
    match id.rsplit_once('-') {
        Some((p, n)) => (p, n.parse().unwrap_or(0)),
        None => (id, 0),
    }
}

/// The cases of the TEST.md catalog (first column of the tables).
pub fn parse_test_catalog(md: &str) -> Result<Vec<TestKey>> {
    let row = Regex::new(r"^\|\s*(T-[^|\s]+)\s*\|")?;
    Ok(md
        .lines()
        .filter_map(|l| row.captures(l))
        .flat_map(|c| parse_test_id(&c[1]))
        .collect())
}

/// A test marker found in a source file.
#[derive(Debug, Clone)]
pub struct Marker {
    /// The marker text (`T-PLY-012`).
    pub id: String,
    /// Normalized key.
    pub key: TestKey,
    /// `path:line`.
    pub at: String,
}

/// The IDs at the start of a comment (`// T-POP-001, T-POP-004 text…`). Mentions
/// in the middle of the text are not markers.
pub fn markers_in_comment(comment: &str) -> Vec<String> {
    static ID: &str = r"^T-[A-Z][A-Z0-9]*(?:-[A-Z][A-Z0-9]*)*-\d{2,3}(?:-[a-z0-9]+)?\b";
    let Ok(re) = Regex::new(ID) else {
        return Vec::new();
    };
    let mut rest = comment
        .trim_start_matches('/')
        .trim_start_matches('!')
        .trim_start();
    let mut out = Vec::new();
    while let Some(m) = re.find(rest) {
        out.push(m.as_str().to_string());
        rest = rest[m.end()..].trim_start_matches([',', ' ']);
    }
    out
}

/// Looks for markers in source comments.
pub fn find_markers(root: &Path) -> Result<Vec<Marker>> {
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in ["crates", "tests", "xtask"] {
        let dir = root.join(dir);
        if dir.is_dir() {
            walk(&dir, "rs", &mut files)?;
        }
    }
    let mut out = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).with_context(|| rel(&file))?;
        let mut in_block = false;
        for (i, line) in text.lines().enumerate() {
            let (_, comment) = split_comment(line, &mut in_block);
            for id in markers_in_comment(&comment) {
                for key in parse_test_id(&id) {
                    out.push(Marker {
                        id: id.clone(),
                        key,
                        at: format!("{}:{}", rel(&file), i + 1),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// Result of the analysis.
#[derive(Debug, Default)]
pub struct Report {
    /// The content of `target/trace.md`.
    pub markdown: String,
    /// Conditions that make the command fail.
    pub problems: Vec<String>,
}

/// Cross-references requirements, milestones and markers.
pub fn analyze(
    spec: &Spec,
    milestones: &[Milestone],
    catalog: &[TestKey],
    markers: &[Marker],
    mut problems: Vec<String>,
) -> Report {
    // requirement → markers that verify it
    let mut tests: BTreeMap<&str, Vec<&Marker>> = BTreeMap::new();
    for m in markers {
        let mut known = false;
        if let Some(n) = m.key.number {
            let direct = format!("{}-{:03}", m.key.prefix, n);
            if let Some(r) = spec.reqs.get(&direct) {
                tests.entry(r).or_default().push(m);
                known = true;
            }
        }
        for (nfr, keys) in &spec.nfr_tests {
            if keys.iter().any(|k| k.matches(&m.key)) {
                tests.entry(nfr).or_default().push(m);
                known = true;
            }
        }
        if !known && !catalog.iter().any(|k| k.matches(&m.key)) {
            problems.push(format!(
                "{}: test {} matches neither a requirement nor a TEST.md case",
                m.at, m.id
            ));
        }
    }

    // requirement → milestone
    let mut owner: BTreeMap<&str, &Milestone> = BTreeMap::new();
    for ms in milestones {
        for r in &ms.reqs {
            if let Some(prev) = owner.insert(r, ms) {
                problems.push(format!(
                    "{r} appears in two milestones: {} and {}",
                    prev.id, ms.id
                ));
            }
        }
    }
    for ms in milestones.iter().filter(|m| m.status == "done") {
        for r in &ms.reqs {
            if !tests.contains_key(r.as_str()) {
                problems.push(format!("{r} ({}, done) has no tests", ms.id));
            }
        }
    }

    let mut md = String::new();
    let _ = writeln!(md, "# Requirements ↔ tests traceability\n");
    let _ = writeln!(
        md,
        "Generated by `cargo xtask trace` (TEST §8). Do not edit by hand.\n"
    );
    let _ = writeln!(
        md,
        "| Milestone | Status | Requirements | With tests |\n|---|---|---|---|"
    );
    for ms in milestones {
        let covered = ms
            .reqs
            .iter()
            .filter(|r| tests.contains_key(r.as_str()))
            .count();
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} |",
            ms.id,
            ms.status,
            ms.reqs.len(),
            covered
        );
    }
    let _ = writeln!(md, "\n| Requirement | Milestone | Tests |\n|---|---|---|");
    for r in &spec.reqs {
        let ms = owner.get(r.as_str()).map_or("—", |m| m.id.as_str());
        let t = tests
            .get(r.as_str())
            .map(|ms| {
                ms.iter()
                    .map(|m| format!("{} ({})", m.id, m.at))
                    .collect::<Vec<_>>()
                    .join("<br>")
            })
            .unwrap_or_default();
        let _ = writeln!(md, "| {r} | {ms} | {t} |");
    }
    if !problems.is_empty() {
        let _ = writeln!(md, "\n## Problems\n");
        for p in &problems {
            let _ = writeln!(md, "- {p}");
        }
    }
    Report {
        markdown: md,
        problems,
    }
}

/// `cargo xtask trace`.
pub fn run(root: &Path) -> Result<()> {
    let spec = parse_spec(&std::fs::read_to_string(
        root.join("docs/SPECIFICATION.md"),
    )?)?;
    let mut problems = Vec::new();
    let milestones = parse_roadmap(
        &std::fs::read_to_string(root.join("docs/ROADMAP.md"))?,
        &spec,
        &mut problems,
    )?;
    let catalog = parse_test_catalog(&std::fs::read_to_string(root.join("docs/TEST.md"))?)?;
    let markers = find_markers(root)?;
    let report = analyze(&spec, &milestones, &catalog, &markers, problems);
    let out = root.join("target/trace.md");
    std::fs::create_dir_all(root.join("target"))?;
    std::fs::write(&out, &report.markdown)?;
    let assigned: usize = milestones.iter().map(|m| m.reqs.len()).sum();
    eprintln!(
        "trace: {} requirements, {assigned} assigned to a milestone, {} markers → {}",
        spec.reqs.len(),
        markers.len(),
        rel(&out)
    );
    crate::arch::report("traceability", &report.problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(prefix: &str, n: Option<u32>) -> TestKey {
        TestKey {
            prefix: prefix.into(),
            number: n,
        }
    }

    #[test]
    fn test_ids_are_normalized() {
        assert_eq!(parse_test_id("T-PLY-012"), [key("PLY", Some(12))]);
        assert_eq!(parse_test_id("T-LKP-011-a"), [key("LKP", Some(11))]);
        assert_eq!(parse_test_id("T-SEC-03"), [key("SEC", Some(3))]);
        assert_eq!(parse_test_id("T-E2E-ANKI-001"), [key("E2E-ANKI", Some(1))]);
        assert_eq!(parse_test_id("T-A11Y-001"), [key("A11Y", Some(1))]);
        assert_eq!(parse_test_id("T-FUZZ-*"), [key("FUZZ", None)]);
        assert_eq!(parse_test_id("T-MET-LIVE-*"), [key("MET-LIVE", None)]);
        assert_eq!(
            parse_test_id("T-POP-001/004/005"),
            [
                key("POP", Some(1)),
                key("POP", Some(4)),
                key("POP", Some(5))
            ]
        );
        assert_eq!(parse_test_id("T-LKP-020…022").len(), 3);
        assert_eq!(parse_test_id("T-KEY-001…T-KEY-004").len(), 4);
        assert!(parse_test_id("PLY-001").is_empty());
    }

    const SPEC: &str = "\
| ID | Requirement |
|---|---|
| PLY-001 | Commands |
| PLY-002 | Press and hold |
| ~~PLY-003~~ | Removed |
| PLY-010 | Speed |

- **ARC-001**: player API.
- ARC-011: `unsafe` only where needed.
**ARC-010 — Dependency rules** (text).
Text that mentions SUB-001 without defining it.

| ID | Requirement | Measure |
|---|---|---|
| NFR-PERF-01 | Startup | T-PERF-01 |
| NFR-REL-02 | Fuzzing | T-FUZZ-* |
";

    #[test]
    fn spec_requirements_and_nfr_measures() {
        let s = parse_spec(SPEC).unwrap();
        let reqs: Vec<&str> = s.reqs.iter().map(String::as_str).collect();
        assert_eq!(
            reqs,
            [
                "ARC-001",
                "ARC-010",
                "ARC-011",
                "NFR-PERF-01",
                "NFR-REL-02",
                "PLY-001",
                "PLY-002",
                "PLY-003",
                "PLY-010"
            ]
        );
        assert_eq!(s.nfr_tests["NFR-REL-02"], [key("FUZZ", None)]);
        assert!(s.removed.contains("PLY-003"));
    }

    const ROADMAP: &str = "\
| Milestone | Title | Status |
|---|---|---|
| M0 | Foundations | done |
| M1 | Player | in progress |

## M0 — Foundations

**Requirements**: ARC-010, ARC-011, NFR-PERF-*; license policy.

## M1 — Player

**Requirements**: PLY-001…PLY-010, ARC-001, ZZZ-001.
";

    #[test]
    fn roadmap_ranges_wildcards_and_unknown_ids() {
        let s = parse_spec(SPEC).unwrap();
        let mut problems = Vec::new();
        let ms = parse_roadmap(ROADMAP, &s, &mut problems).unwrap();
        assert_eq!(ms[0].id, "M0");
        assert_eq!(ms[0].status, "done");
        assert_eq!(ms[0].reqs, ["ARC-010", "ARC-011", "NFR-PERF-01"]);
        // PLY-003 is struck through: it is not part of the range
        assert_eq!(ms[1].reqs, ["ARC-001", "PLY-001", "PLY-002", "PLY-010"]);
        assert_eq!(
            problems,
            ["ROADMAP M1: ZZZ-001 does not exist in SPECIFICATION"]
        );
    }

    #[test]
    fn only_leading_ids_are_markers() {
        assert_eq!(markers_in_comment("// T-PLY-012"), ["T-PLY-012"]);
        assert_eq!(
            markers_in_comment("// T-POP-001, T-POP-004 state machine"),
            ["T-POP-001", "T-POP-004"]
        );
        assert_eq!(
            markers_in_comment("/// T-LKP-011-a katakana"),
            ["T-LKP-011-a"]
        );
        assert!(markers_in_comment("// copies for EXT-002 and T-SEC-005").is_empty());
        assert!(markers_in_comment("/// The marker (`T-PLY-012`).").is_empty());
    }

    fn marker(id: &str) -> Marker {
        Marker {
            id: id.into(),
            key: parse_test_id(id).remove(0),
            at: "x.rs:1".into(),
        }
    }

    #[test]
    fn analysis_flags_untested_unknown_and_duplicated() {
        let s = parse_spec(SPEC).unwrap();
        let mut problems = Vec::new();
        let mut ms = parse_roadmap(ROADMAP, &s, &mut problems).unwrap();
        ms[1].reqs.push("ARC-010".into());
        let catalog = parse_test_catalog("| T-LIC-001 | cargo deny |\n").unwrap();
        let markers = [
            marker("T-ARC-010"),
            marker("T-PERF-01"),
            marker("T-LIC-001"),
            marker("T-XYZ-001"),
        ];
        let r = analyze(&s, &ms, &catalog, &markers, Vec::new());
        assert_eq!(r.problems.len(), 3, "{:?}", r.problems);
        assert!(r.problems[0].contains("T-XYZ-001 matches neither"));
        assert!(r.problems[1].contains("ARC-010 appears in two milestones"));
        assert!(r.problems[2].contains("ARC-011 (M0, done) has no tests"));
        assert!(
            r.markdown
                .contains("| NFR-PERF-01 | M0 | T-PERF-01 (x.rs:1) |")
        );
    }
}
