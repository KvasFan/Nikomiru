// SPDX-License-Identifier: GPL-3.0-only

//! Performance (TEST §6.15) on the synthetic dictionaries of
//! `cargo xtask fixtures --large`: T-PERF-05 (import of 300k entries) and
//! T-PERF-04 (lookup with 5 term, 3 frequency and 1 pitch dictionaries).
//! Release build only: `cargo xtask bench`.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nikomiru_core::Cancel;
use nikomiru_dict::import::import;
use nikomiru_dict::lookup::{Dictionary, Lookup, Options};
use nikomiru_dict::store::{DictStore, RedbStore};

const IMPORT_LIMIT: Duration = Duration::from_secs(60);
const IMPORT_RAM_KIB: u64 = 1 << 20;
const P95: Duration = Duration::from_millis(10);
const P99: Duration = Duration::from_millis(25);

const CHILD_ZIP: &str = "NIKOMIRU_PERF_ZIP";
const CHILD_OUT: &str = "NIKOMIRU_PERF_OUT";

fn synth(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures/dicts/synth")
        .join(name)
}

/// Dictionaries go under `target/`, not `/tmp`: hundreds of MB. With
/// `NIKOMIRU_PERF_REUSE=1` the previous import is kept (`None`: nothing to do).
fn workdir() -> (PathBuf, bool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/perf-dicts");
    let reuse = std::env::var_os("NIKOMIRU_PERF_REUSE").is_some()
        && std::fs::read_dir(&dir).is_ok_and(|r| r.count() == 9);
    if !reuse {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
    }
    (dir, !reuse)
}

fn release_only() {
    if cfg!(debug_assertions) {
        panic!("performance tests need a release build: cargo xtask bench");
    }
}

fn vm_hwm_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// Child process of the import measurement: its peak RSS is the import's.
#[test]
#[ignore = "child process of T-PERF-05"]
fn l7_perf_child_import() {
    let (Ok(zip), Ok(out)) = (std::env::var(CHILD_ZIP), std::env::var(CHILD_OUT)) else {
        return;
    };
    let started = Instant::now();
    import(Path::new(&zip), Path::new(&out), &Cancel::new(), |_| {}).unwrap();
    println!(
        "perf-import {} {}",
        started.elapsed().as_millis(),
        vm_hwm_kib()
    );
}

/// Imports `zip` into `out` in a child process: (time, peak RSS in KiB).
fn import_in_child(zip: &Path, out: &Path) -> (Duration, u64) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "perf_child_import",
            "--exact",
            "--include-ignored",
            "--nocapture",
        ])
        .env(CHILD_ZIP, zip)
        .env(CHILD_OUT, out)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = stdout
        .lines()
        .find_map(|l| l.strip_prefix("perf-import "))
        .expect("no result line");
    let mut parts = line.split(' ').map(|p| p.parse::<u64>().unwrap());
    (
        Duration::from_millis(parts.next().unwrap()),
        parts.next().unwrap(),
    )
}

fn open_all(out: &Path) -> Vec<Dictionary> {
    let mut dicts: Vec<Dictionary> = std::fs::read_dir(out)
        .unwrap()
        .flatten()
        .map(|e| {
            let store = RedbStore::open(&e.path().join("dict.redb")).unwrap();
            Dictionary {
                id: e.file_name().to_string_lossy().into_owned(),
                name: store.info().index.title.clone(),
                store: Arc::new(store),
            }
        })
        .collect();
    dicts.sort_by(|a, b| a.name.cmp(&b.name));
    dicts
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let i = ((sorted.len() as f64 * p).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i]
}

// T-PERF-05
// T-PERF-04
#[test]
#[ignore = "performance: cargo xtask bench"]
fn l7_perf_import_and_lookup() {
    release_only();
    let queries =
        std::fs::read_to_string(synth("queries.txt")).expect("cargo xtask fixtures --large");
    let (out, fresh) = workdir();
    let mut zips: Vec<String> = (1..=5).map(|d| format!("synth-term-{d}.zip")).collect();
    zips.extend((1..=3).map(|d| format!("synth-freq-{d}.zip")));
    zips.push("synth-pitch.zip".into());
    for name in zips.iter().filter(|_| fresh) {
        let (time, kib) = import_in_child(&synth(name), &out);
        println!(
            "T-PERF-05 {name}: {:.1} s, peak {} MiB",
            time.as_secs_f64(),
            kib / 1024
        );
        assert!(time <= IMPORT_LIMIT, "{name}: {time:?}");
        assert!(kib <= IMPORT_RAM_KIB, "{name}: {kib} KiB");
    }
    let dicts = open_all(&out);
    assert_eq!(dicts.len(), 9);
    let lookup = Lookup::new(dicts, "ja");
    let mut times = Vec::new();
    let mut found = 0usize;
    for q in queries.lines().filter(|l| !l.is_empty()) {
        let started = Instant::now();
        let entries = lookup.lookup(q, Options::default()).unwrap();
        times.push(started.elapsed());
        found += usize::from(!entries.is_empty());
    }
    times.sort();
    let (p50, p95, p99) = (
        percentile(&times, 0.5),
        percentile(&times, 0.95),
        percentile(&times, 0.99),
    );
    println!(
        "T-PERF-04 {} queries ({found} with results): p50 {p50:?}, p95 {p95:?}, p99 {p99:?}, max {:?}",
        times.len(),
        times.last().unwrap()
    );
    assert!(
        found * 10 >= times.len() * 9,
        "most queries must find something"
    );
    assert!(p95 <= P95 && p99 <= P99, "p95 {p95:?}, p99 {p99:?}");
}
