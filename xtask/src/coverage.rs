// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask coverage` (NFR-MAINT-02, T-MAINT-002): the line coverage of
//! the logic crates, measured with `cargo llvm-cov` (MIT/Apache), at least
//! 80% for each.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::util;

/// The crates and the share of their lines the tests must run.
const CRATES: [&str; 4] = [
    "nikomiru-text",
    "nikomiru-subs",
    "nikomiru-dict",
    "nikomiru-anki",
];
const MINIMUM: f64 = 80.0;

/// Not counted: the tests themselves and the fake servers.
const IGNORED: &str = "(tests/|fake\\.rs)";

// T-MAINT-002
/// Measures, then checks each crate against [`MINIMUM`].
pub fn run(root: &Path) -> Result<()> {
    if !util::have_program("cargo-llvm-cov") {
        bail!(
            "cargo-llvm-cov is needed: `cargo install --locked cargo-llvm-cov` and \
             `rustup component add llvm-tools-preview`"
        );
    }
    let packages: Vec<String> = CRATES
        .iter()
        .flat_map(|c| ["-p".into(), (*c).to_string()])
        .collect();
    util::run(util::cargo().args(["llvm-cov", "clean", "--workspace"]))?;
    // the L4 tests too (the generated fixtures); not the L7 ones, nor the one
    // that kills an import halfway, whose profile is cut short
    util::run(
        util::cargo()
            .args(["llvm-cov", "--no-report", "--locked"])
            .args(&packages)
            .args([
                "--",
                "--include-ignored",
                "--skip",
                "l7_",
                "--skip",
                "killed_halfway",
            ]),
    )?;
    drop_truncated(&root.join("target/llvm-cov-target"))?;
    let out = util::cargo()
        .args([
            "llvm-cov",
            "report",
            "--summary-only",
            "--ignore-filename-regex",
            IGNORED,
        ])
        .output()
        .context("cargo llvm-cov report")?;
    if !out.status.success() {
        bail!(
            "cargo llvm-cov report: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let report = String::from_utf8_lossy(&out.stdout);
    let shares = shares(&report);
    let mut low = Vec::new();
    for name in CRATES {
        let share = shares.get(name).copied().unwrap_or(0.0);
        eprintln!("{name}: {share:.1}% of its lines");
        if share < MINIMUM {
            low.push(format!("{name} {share:.1}%"));
        }
    }
    if !low.is_empty() {
        bail!("line coverage under {MINIMUM}%: {}", low.join(", "));
    }
    eprintln!("ok: line coverage");
    Ok(())
}

/// The profiles that `llvm-profdata` cannot read: those of a process killed
/// while writing it would make the whole report fail.
fn drop_truncated(dir: &Path) -> Result<()> {
    // llvm-tools of the toolchain in use
    let rustc = |arg: &str| -> Result<String> {
        let out = std::process::Command::new("rustc").arg(arg).output()?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let sysroot = rustc("--print=sysroot")?;
    let host = rustc("-vV")?
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .context("rustc's host")?;
    let tool = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/llvm-profdata");
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "profraw") {
            let ok = std::process::Command::new(&tool)
                .arg("show")
                .arg(&path)
                .output()
                .is_ok_and(|o| o.status.success());
            if !ok {
                eprintln!("dropping a truncated profile: {}", path.display());
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// The share of lines run of each crate, from `llvm-cov report`'s rows.
fn shares(report: &str) -> BTreeMap<String, f64> {
    let mut lines: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for row in report.lines() {
        let Some((file, rest)) = row.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((name, _)) = file.split_once("/src/") else {
            continue;
        };
        let numbers: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        // regions, missed, functions, missed, lines, missed (the percentages
        // do not parse as integers)
        if let [_, _, _, _, total, missed, ..] = numbers[..] {
            let e = lines.entry(name.to_string()).or_default();
            e.0 += total;
            e.1 += missed;
        }
    }
    lines
        .into_iter()
        .filter(|(_, (total, _))| *total > 0)
        .map(|(name, (total, missed))| (name, 100.0 * (total - missed) as f64 / total as f64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_add_up_by_crate() {
        let report = "Filename  Regions Missed Cover Functions Missed Executed Lines Missed Cover\n\
            nikomiru-text/src/a.rs   10 1 90.00% 2 0 100.00% 100 10 90.00% 0 0 -\n\
            nikomiru-text/src/b.rs   10 1 90.00% 2 0 100.00% 100 30 70.00% 0 0 -\n\
            TOTAL 20 2 90.00% 4 0 100.00% 200 40 80.00% 0 0 -\n";
        let s = shares(report);
        assert!((s["nikomiru-text"] - 80.0).abs() < 0.01, "{s:?}");
        assert_eq!(s.len(), 1);
    }
}
