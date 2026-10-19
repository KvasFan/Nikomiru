// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask ci [--full]`: the "fast" and "full" stages of the pipeline (TEST §10).
//! The CI of the chosen service calls only this command (ROADMAP M0).

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};

use crate::util::{self, cargo};
use crate::{arch, fixtures, licenses, trace};

fn step(name: &str, f: impl FnOnce() -> Result<()>) -> Result<()> {
    eprintln!("\n▶ {name}");
    let t = Instant::now();
    f()?;
    eprintln!("✓ {name} ({:.1} s)", t.elapsed().as_secs_f64());
    Ok(())
}

/// Runs the pipeline.
pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let mut full = false;
    for a in args {
        match a.as_str() {
            "--full" => full = true,
            other => bail!("ci: unknown option {other}"),
        }
    }
    let started = Instant::now();
    step("formatting (cargo fmt --check)", || {
        util::run(cargo().args(["fmt", "--all", "--check"]))
    })?;
    step("clippy -D warnings", || {
        util::run(cargo().args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ]))
    })?;
    step("architecture and headers", || arch::check_all(root))?;
    step("licenses (cargo deny)", || licenses::check(root, true))?;
    step("traceability", || trace::run(root))?;
    if full {
        step("fixtures", || fixtures::run(root, &[]))?;
    }
    step("test", || tests(full))?;
    if full {
        step("THIRD_PARTY_LICENSES.html", || {
            licenses::about(root).map(|_| ())
        })?;
    }
    eprintln!(
        "\nci{}: all green in {:.0} s",
        if full { " --full" } else { "" },
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// L1–L3 always; with `full` also the `#[ignore]` tests that use the generated
/// fixtures (L4, L5), excluding the external L7 ones, whose name starts with `l7_`
/// (TEST §2).
fn tests(full: bool) -> Result<()> {
    if util::have_program("cargo-nextest") {
        let mut cmd = cargo();
        cmd.args([
            "nextest",
            "run",
            "--workspace",
            "--locked",
            "--no-tests",
            "warn",
        ]);
        if full {
            cmd.args(["--run-ignored", "all", "-E", "not test(/^l7_|::l7_/)"]);
        }
        util::run(&mut cmd)?;
        util::run(cargo().args(["test", "--doc", "--workspace", "--locked"]))
    } else {
        let mut cmd = cargo();
        cmd.args(["test", "--workspace", "--locked"]);
        if full {
            cmd.args(["--", "--include-ignored", "--skip", "l7_"]);
        }
        util::run(&mut cmd)
    }
}
