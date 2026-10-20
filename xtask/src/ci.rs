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
        ]))?;
        // the GUI test harness and the L6 tests exist only with this feature
        util::run(cargo().args([
            "clippy",
            "-p",
            "nikomiru-app",
            "--features",
            "test-harness",
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
        step("release build (T-SEC-004)", release_build)?;
        step("GUI end-to-end (L6)", gui_tests)?;
    }
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

// T-SEC-004
/// The release build: it compiles, its feature tree has no test-only feature and
/// the binary holds none of the harness's strings. The L6 suite then starts it.
fn release_build() -> Result<()> {
    util::run(cargo().args(["build", "--release", "--locked", "-p", "nikomiru-app"]))?;
    let tree = util::output(cargo().args([
        "tree",
        "--locked",
        "-e",
        "normal,build,features",
        "-p",
        "nikomiru-app",
    ]))?;
    for feature in ["test-harness", "test-support"] {
        if tree.contains(feature) {
            bail!("the release feature tree enables `{feature}`");
        }
    }
    let binary = std::fs::read(util::root().join("target/release/nikomiru"))?;
    // messages that only the harness's code holds (nikomiru-app and nikomiru-ui)
    for needle in [
        "test harness connected",
        "the window did not appear",
        "no subtitle layer",
    ] {
        if binary.windows(needle.len()).any(|w| w == needle.as_bytes()) {
            bail!("the release binary contains the harness string {needle:?}");
        }
    }
    Ok(())
}

/// L6: `tests/e2e/`, a test target of nikomiru-app that exists only with the
/// `test-harness` feature. Each test starts its own off-screen KWin and private
/// D-Bus bus (TEST §2.1).
fn gui_tests() -> Result<()> {
    for program in ["kwin_wayland", "dbus-daemon"] {
        if !util::have_program(program) {
            eprintln!("  skipped: {program} is not installed");
            return Ok(());
        }
    }
    util::run(cargo().args([
        "test",
        "-p",
        "nikomiru-app",
        "--features",
        "test-harness",
        "--locked",
        "--test",
        "e2e",
        "--",
        "--include-ignored",
        // L7 needs a real Anki: `cargo xtask test --anki-e2e`
        "--skip",
        "l7_",
        "--test-threads",
        "1",
    ]))
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
