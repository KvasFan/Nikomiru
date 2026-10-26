// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask`: Nikomiru's development tasks (ROADMAP M0, TEST §10).
//!
//! They are written in Rust rather than with `just`, which is CC0 and therefore
//! outside the license policy (SPECIFICATION §3.5).

#![forbid(unsafe_code)]

mod arch;
mod ci;
mod coverage;
mod evil;
mod fixtures;
mod flatpak;
mod i18n;
mod install;
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
  check-i18n           every Fluent key used and existing, no untranslated text in the views
  coverage             line coverage of the logic crates with cargo-llvm-cov, ≥ 80% each
  flatpak-sources      packaging/flatpak/cargo-sources.json from Cargo.lock, for an offline Flatpak build
  flatpak              the Flatpak of the last commit, into target/nikomiru.flatpak
  check-licenses [--canary]
                       deny.toml exceptions = SPECIFICATION §3.2, then cargo deny;
                       --canary: checks that a BSL-1.0 crate is rejected
  trace                requirements ↔ tests ↔ milestones matrix in target/trace.md
  about                generates target/THIRD_PARTY_LICENSES.html
  test --anki-e2e      L7: the real Anki at NIKOMIRU_ANKI_URL (a throwaway profile
                       with AnkiConnect): Create Lapis deck and a card from the app
  install [--prefix DIR]
                       release build in DIR/bin (default ~/.local) and the desktop
                       entry in DIR/share/applications, registered for nikomiru://
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
        "check-i18n" => i18n::check(&root),
        "coverage" => coverage::run(&root),
        "flatpak-sources" => flatpak::run(&root),
        "flatpak" => flatpak::build(&root),
        "check-licenses" if rest.iter().any(|a| a == "--canary") => licenses::canary(&root),
        "check-licenses" => licenses::check(&root, true),
        "trace" => trace::run(&root),
        "about" => licenses::about(&root).map(|_| ()),
        "install" => install::run(&root, rest),
        "test" if rest.iter().any(|a| a == "--anki-e2e") => anki_e2e(&root),
        "help" | "-h" | "--help" => {
            eprint!("{HELP}");
            Ok(())
        }
        other => bail!("unknown command: {other}\n\n{HELP}"),
    }
}

/// The L7 test against a real Anki (T-E2E-ANKI-001).
fn anki_e2e(root: &std::path::Path) -> Result<()> {
    if std::env::var_os("NIKOMIRU_ANKI_URL").is_none() {
        bail!(
            "set NIKOMIRU_ANKI_URL to the AnkiConnect of a throwaway Anki profile \
             (e.g. http://127.0.0.1:8765): the test adds the Lapis note type and cards"
        );
    }
    fixtures::run(root, &[])?;
    util::run(util::cargo().args([
        "test",
        "--locked",
        "-p",
        "nikomiru-app",
        "--features",
        "test-harness",
        "--test",
        "e2e",
        "--",
        "--ignored",
        "--exact",
        "l7_a_card_in_a_real_anki",
    ]))
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
