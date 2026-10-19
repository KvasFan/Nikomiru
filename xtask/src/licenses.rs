// SPDX-License-Identifier: GPL-3.0-only

//! License policy (SPECIFICATION §3.2, §3.5): the table of class B exceptions
//! must match `deny.toml`, `cargo deny` must pass and `THIRD_PARTY_LICENSES.html`
//! must list every distributed crate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::arch::{self, Graph, report};
use crate::util::{self, rel};

/// An exception: a crate (or a `prefix_*` group in the table) and the allowed licenses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    /// Crate name, or `prefix*` in the SPECIFICATION table.
    pub krate: String,
    /// SPDX licenses allowed for that crate.
    pub licenses: BTreeSet<String>,
}

fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// Reads the "Class B" table of SPECIFICATION §3.2.
pub fn parse_class_b_table(spec: &str) -> Result<Vec<Exception>> {
    let start = spec
        .find("**Class B")
        .context("SPECIFICATION: section \"Class B\" not found")?;
    let section = &spec[start..];
    let end = section
        .find("Rules for class B")
        .context("SPECIFICATION: end of the class B table not found")?;
    let mut out = Vec::new();
    for line in section[..end].lines() {
        let line = line.trim();
        if !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 2 {
            continue;
        }
        let licenses: BTreeSet<String> = backticked(cells[0]).into_iter().collect();
        for krate in backticked(cells[1]) {
            // The crate column also contains license expressions in backticks.
            if krate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '*'))
            {
                out.push(Exception {
                    krate,
                    licenses: licenses.clone(),
                });
            }
        }
    }
    if out.is_empty() {
        bail!("SPECIFICATION: the class B table is empty");
    }
    Ok(out)
}

/// Reads `[licenses].exceptions` from `deny.toml`.
pub fn parse_deny_exceptions(deny: &str) -> Result<Vec<Exception>> {
    let v: toml::Value = toml::from_str(deny).context("invalid deny.toml")?;
    let list = v
        .get("licenses")
        .and_then(|l| l.get("exceptions"))
        .and_then(toml::Value::as_array)
        .context("deny.toml: [licenses].exceptions missing")?;
    let mut out = Vec::new();
    for e in list {
        let krate = e
            .get("crate")
            .and_then(toml::Value::as_str)
            .context("deny.toml: exception without `crate`")?;
        let krate = krate.split('@').next().unwrap_or(krate).to_string();
        let licenses = e
            .get("allow")
            .and_then(toml::Value::as_array)
            .context("deny.toml: exception without `allow`")?
            .iter()
            .filter_map(|l| l.as_str().map(str::to_string))
            .collect();
        out.push(Exception { krate, licenses });
    }
    Ok(out)
}

fn matches(pattern: &str, krate: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => krate.starts_with(prefix),
        None => pattern == krate,
    }
}

/// Differences between the SPECIFICATION table and `deny.toml`.
pub fn compare(table: &[Exception], deny: &[Exception]) -> Vec<String> {
    let mut problems = Vec::new();
    for d in deny {
        let rows: Vec<&Exception> = table
            .iter()
            .filter(|t| matches(&t.krate, &d.krate))
            .collect();
        if rows.is_empty() {
            problems.push(format!(
                "deny.toml: exception for `{}` missing from the table of SPECIFICATION §3.2",
                d.krate
            ));
            continue;
        }
        let allowed: BTreeSet<&String> = rows.iter().flat_map(|t| &t.licenses).collect();
        for l in &d.licenses {
            if !allowed.contains(l) {
                problems.push(format!(
                    "deny.toml: `{}` allows {l}, which the table of SPECIFICATION §3.2 does not list",
                    d.krate
                ));
            }
        }
    }
    for t in table {
        if !deny.iter().any(|d| matches(&t.krate, &d.krate)) {
            problems.push(format!(
                "SPECIFICATION §3.2: `{}` has no exception in deny.toml",
                t.krate
            ));
        }
    }
    problems
}

// T-LIC-001
/// Table ↔ `deny.toml`, then `cargo deny check`.
pub fn check(root: &Path, run_cargo_deny: bool) -> Result<()> {
    let spec = std::fs::read_to_string(root.join("docs/SPECIFICATION.md"))?;
    let deny = std::fs::read_to_string(root.join("deny.toml"))?;
    let problems = compare(&parse_class_b_table(&spec)?, &parse_deny_exceptions(&deny)?);
    report("class B exceptions = SPECIFICATION §3.2", &problems)?;
    if run_cargo_deny {
        if !util::have_program("cargo-deny") {
            bail!("cargo-deny is not installed: cargo install --locked cargo-deny");
        }
        util::run(util::cargo().args([
            "deny",
            "--locked",
            "check",
            "licenses",
            "bans",
            "sources",
            "advisories",
        ]))?;
    }
    Ok(())
}

// T-LIC-002
/// Canary (nightly stage): a temporary crate that depends on `xxhash-rust`
/// (BSL-1.0, forbidden) must make `cargo deny` fail with the project
/// configuration. If it passes, the policy is not blocking anything.
pub fn canary(root: &Path) -> Result<()> {
    let dir = root.join("target/license-canary");
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"license-canary\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
         license = \"GPL-3.0-only\"\npublish = false\n\n[workspace]\n\n\
         [dependencies]\nxxhash-rust = { version = \"0.8\", features = [\"xxh3\"] }\n",
    )?;
    std::fs::write(
        dir.join("src/lib.rs"),
        "// SPDX-License-Identifier: GPL-3.0-only\n",
    )?;
    let manifest = dir.join("Cargo.toml");
    let config = root.join("deny.toml");
    let mut cmd = util::cargo();
    cmd.arg("deny")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--config")
        .arg(&config)
        .args(["check", "licenses"]);
    eprintln!("$ {}", util::display(&cmd));
    let out = cmd.output()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        bail!("the canary passed: cargo deny accepts xxhash-rust (BSL-1.0)");
    }
    if !stderr.contains("xxhash-rust") {
        bail!("cargo deny failed, but not because of xxhash-rust:\n{stderr}");
    }
    eprintln!("ok: cargo deny rejects xxhash-rust (BSL-1.0)");
    Ok(())
}

// T-LIC-003
/// Generates `target/THIRD_PARTY_LICENSES.html` for the distributed binary
/// (`anikoto-app`) and checks that it lists every crate in its graph.
pub fn about(root: &Path) -> Result<PathBuf> {
    if !util::have_program("cargo-about") {
        bail!("cargo-about is not installed: cargo install --locked cargo-about --features cli");
    }
    let out = root.join("target/THIRD_PARTY_LICENSES.html");
    std::fs::create_dir_all(root.join("target"))?;
    util::run(util::cargo().args([
        "about",
        "generate",
        "--locked",
        "--fail",
        "--manifest-path",
        "crates/anikoto-app/Cargo.toml",
        "--output-file",
        &out.to_string_lossy(),
        "about.hbs",
    ]))?;
    let html = std::fs::read_to_string(&out)?;
    let metadata = arch::cargo_metadata()?;
    let graph = Graph::from_metadata(&metadata)?;
    let versions: Vec<(String, String)> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some((
                p["id"].as_str()?.to_string(),
                p["version"].as_str()?.to_string(),
            ))
        })
        .collect();
    let app = graph
        .members
        .iter()
        .find(|m| graph.names.get(*m).is_some_and(|n| n == "anikoto-app"))
        .context("anikoto-app is not in the workspace")?;
    let mut missing = Vec::new();
    for dep in graph.closure(app) {
        if graph.members.contains(&dep) {
            continue;
        }
        let name = graph.names.get(&dep).cloned().unwrap_or_default();
        let version = versions
            .iter()
            .find(|(id, _)| *id == dep)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if !html.contains(&format!("{name} {version}")) {
            missing.push(format!("{name} {version} does not appear in {}", rel(&out)));
        }
    }
    report("THIRD_PARTY_LICENSES.html complete", &missing)?;
    eprintln!("generated {}", rel(&out));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "\
**Class B — mandatory exceptions, allowed only for the listed crates.**

| License | Crate (verified version) | Required by |
|---|---|---|
| `Unicode-3.0` | `icu_*` 2.3, `litemap`; `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`) | `url` |
| `Zlib` | `slotmap` | `dioxus-core` |
| `BSD-2-Clause`, `BSD-3-Clause` | `rav1e` | `image` |

Rules for class B:
";

    fn exc(krate: &str, licenses: &[&str]) -> Exception {
        Exception {
            krate: krate.into(),
            licenses: licenses.iter().map(|l| (*l).to_string()).collect(),
        }
    }

    #[test]
    fn table_is_parsed_without_license_expressions() {
        let t = parse_class_b_table(SPEC).unwrap();
        let names: Vec<&str> = t.iter().map(|e| e.krate.as_str()).collect();
        assert_eq!(
            names,
            ["icu_*", "litemap", "unicode-ident", "slotmap", "rav1e"]
        );
        assert_eq!(t[4], exc("rav1e", &["BSD-2-Clause", "BSD-3-Clause"]));
    }

    #[test]
    fn deny_toml_is_parsed() {
        let deny = r#"
[licenses]
exceptions = [
    { crate = "slotmap", allow = ["Zlib"] },
    { crate = "litemap@0.8.3", allow = ["Unicode-3.0"] },
]
"#;
        assert_eq!(
            parse_deny_exceptions(deny).unwrap(),
            vec![exc("slotmap", &["Zlib"]), exc("litemap", &["Unicode-3.0"])]
        );
    }

    #[test]
    fn identical_lists_have_no_problems() {
        let table = parse_class_b_table(SPEC).unwrap();
        let deny = vec![
            exc("icu_normalizer", &["Unicode-3.0"]),
            exc("icu_provider", &["Unicode-3.0"]),
            exc("litemap", &["Unicode-3.0"]),
            exc("unicode-ident", &["Unicode-3.0"]),
            exc("slotmap", &["Zlib"]),
            exc("rav1e", &["BSD-2-Clause"]),
        ];
        assert_eq!(compare(&table, &deny), Vec::<String>::new());
    }

    #[test]
    fn extra_missing_and_wrong_license_are_reported() {
        let table = parse_class_b_table(SPEC).unwrap();
        let deny = vec![
            exc("icu_normalizer", &["Unicode-3.0"]),
            exc("litemap", &["Unicode-3.0"]),
            exc("unicode-ident", &["Unicode-3.0"]),
            exc("slotmap", &["Zlib", "MPL-2.0"]),
            exc("rav1e", &["BSD-2-Clause"]),
            exc("yoke-derive", &["Unicode-3.0"]),
        ];
        let p = compare(&table, &deny);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].contains("`slotmap` allows MPL-2.0"));
        assert!(p[1].contains("`yoke-derive` missing"));

        let p = compare(&table, &deny[1..]);
        assert!(
            p.iter().any(|m| m.contains("`icu_*` has no exception")),
            "{p:?}"
        );
    }
}
