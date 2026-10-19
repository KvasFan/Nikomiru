// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask`: Nikomiru's development tasks (ROADMAP M0, TEST §10).
//!
//! They are written in Rust rather than with `just`, which is CC0 and therefore
//! outside the license policy (SPECIFICATION §3.5).

#![forbid(unsafe_code)]

mod arch;
mod ci;
mod evil;
mod fixtures;
mod licenses;
mod synth;
mod trace;
mod util;

use std::process::ExitCode;

use anyhow::{Result, bail};

const HELP: &str = "\
cargo xtask <command>

  ci [--full]          fmt, clippy, architecture, licenses, traceability, tests;
                       --full adds fixtures, L4/L5 tests and THIRD_PARTY_LICENSES
  fixtures [--large] [--perf]
                       generates test media and dictionaries in target/fixtures
                       (--large: synthetic dictionaries with 300k entries; --perf:
                       5-minute 1080p60 videos for T-PERF-03)
  bench                T-PERF-04/05: imports the synthetic dictionaries (generated
                       with fixtures --large if needed) and times 10k lookups
  check-headers        SPDX headers, unsafe, documentation, dependencies
  check-licenses [--canary]
                       deny.toml exceptions = SPECIFICATION §3.2, then cargo deny;
                       --canary: checks that a BSL-1.0 crate is rejected
  trace                requirements ↔ tests ↔ milestones matrix in target/trace.md
  about                generates target/THIRD_PARTY_LICENSES.html
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = util::root();
    let Some((cmd, rest)) = args.split_first() else {
        eprint!("{HELP}");
        return Ok(());
    };
    match cmd.as_str() {
        "ci" => ci::run(&root, rest),
        "fixtures" => fixtures::run(&root, rest),
        "bench" => bench(&root),
        "check-headers" => arch::check_all(&root),
        "check-licenses" if rest.iter().any(|a| a == "--canary") => licenses::canary(&root),
        "check-licenses" => licenses::check(&root, true),
        "trace" => trace::run(&root),
        "about" => licenses::about(&root).map(|_| ()),
        "help" | "-h" | "--help" => {
            eprint!("{HELP}");
            Ok(())
        }
        other => bail!("unknown command: {other}\n\n{HELP}"),
    }
}

/// The dictionary benchmarks (TEST §6.15): release build, one test thread.
fn bench(root: &std::path::Path) -> Result<()> {
    fixtures::run(root, &["--large".to_string()])?;
    util::run(util::cargo().args([
        "test",
        "--release",
        "--locked",
        "-p",
        "nikomiru-dict",
        "--test",
        "perf",
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads",
        "1",
    ]))
}
