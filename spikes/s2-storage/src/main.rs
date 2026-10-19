// SPDX-License-Identifier: GPL-3.0-only

//! Spike S2 (ROADMAP M0): import of synthetic Yomitan dictionaries into redb and
//! lookup with a minimal deinflector. Measurements for NFR-PERF-04 (lookup p95 ≤
//! 10 ms, p99 ≤ 25 ms with 5 term + 3 freq + 1 pitch dictionaries) and NFR-PERF-05
//! (import of 300k entries / 150 MB of JSON ≤ 60 s with peak RAM ≤ 1 GiB).
//! Throwaway code: in M3 it is rewritten cleanly in `anikoto-dict`.
//!
//! Usage:
//!   spike-s2-storage all <directory with synth-*.zip and queries.txt> <output directory>
//!   spike-s2-storage import <zip> <db>
//!   spike-s2-storage bench <db directory> <queries.txt> [cache MiB per dictionary]

mod deinflect;
mod lookup;
mod store;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use deinflect::Deinflector;
use lookup::{Dict, Dicts, Session};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("import") if args.len() == 4 => {
            let r = store::import(Path::new(&args[2]), Path::new(&args[3]))?;
            println!("{}", serde_json::to_string(&r)?);
            Ok(())
        }
        Some("bench") if args.len() >= 4 => {
            let cache = args.get(4).map_or(Ok(64), |s| s.parse::<usize>())?;
            let r = bench(Path::new(&args[2]), Path::new(&args[3]), cache << 20)?;
            println!("{}", serde_json::to_string_pretty(&r)?);
            Ok(())
        }
        Some("all") if args.len() == 4 => all(Path::new(&args[2]), Path::new(&args[3])),
        _ => bail!(
            "usage: all <synth> <out> | import <zip> <db> | bench <dbdir> <queries> [cache MiB]"
        ),
    }
}

#[derive(Debug, Serialize)]
struct Latency {
    queries: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    mean_ms: f64,
}

fn latency(mut d: Vec<Duration>) -> Latency {
    d.sort_unstable();
    let ms = |x: Duration| x.as_secs_f64() * 1000.0;
    let at = |q: f64| {
        ms(d[((d.len() as f64 * q).ceil() as usize)
            .saturating_sub(1)
            .min(d.len() - 1)])
    };
    Latency {
        queries: d.len(),
        p50_ms: at(0.50),
        p95_ms: at(0.95),
        p99_ms: at(0.99),
        max_ms: d.last().map_or(0.0, |x| ms(*x)),
        mean_ms: d.iter().map(|x| ms(*x)).sum::<f64>() / d.len() as f64,
    }
}

#[derive(Debug, Serialize)]
struct BenchReport {
    term_dicts: usize,
    freq_dicts: usize,
    pitch_dicts: usize,
    deinflection_rules: usize,
    cache_mib_per_dict: usize,
    /// First pass: empty redb cache (the OS page cache may be warm).
    first_pass: Latency,
    /// Second pass over the same queries: warm redb cache.
    second_pass: Latency,
    avg_groups: f64,
    avg_glossaries: f64,
    queries_with_results: usize,
    rss_after_mib: f64,
    peak_rss_mib: f64,
}

fn open_all(dir: &Path, prefix: &str, cache: usize) -> Result<Vec<Dict>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "redb")
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    paths.sort();
    paths.iter().map(|p| Dict::open(p, cache)).collect()
}

fn bench(dir: &Path, queries: &Path, cache: usize) -> Result<BenchReport> {
    let dicts = Dicts {
        terms: open_all(dir, "synth-term", cache)?,
        freqs: open_all(dir, "synth-freq", cache)?,
        pitch: open_all(dir, "synth-pitch", cache)?,
    };
    if dicts.terms.is_empty() {
        bail!("no dictionaries in {}", dir.display());
    }
    let text = std::fs::read_to_string(queries)?;
    let queries: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let deinflector = Deinflector::japanese();
    let mut passes = Vec::new();
    let (mut groups, mut glossaries, mut hits) = (0usize, 0usize, 0usize);
    for pass in 0..2 {
        let mut times = Vec::with_capacity(queries.len());
        for q in &queries {
            let t = Instant::now();
            let session = Session::begin(&dicts)?;
            let res = session.lookup(&deinflector, q, 10)?;
            times.push(t.elapsed());
            if pass == 0 {
                groups += res.len();
                glossaries += res.iter().map(|g| g.glossaries.len()).sum::<usize>();
                hits += usize::from(!res.is_empty());
            }
        }
        passes.push(latency(times));
    }
    let n = queries.len() as f64;
    let second = passes.pop().context("missing pass")?;
    let first = passes.pop().context("missing pass")?;
    Ok(BenchReport {
        term_dicts: dicts.terms.len(),
        freq_dicts: dicts.freqs.len(),
        pitch_dicts: dicts.pitch.len(),
        deinflection_rules: deinflector.rule_count,
        cache_mib_per_dict: cache >> 20,
        first_pass: first,
        second_pass: second,
        avg_groups: groups as f64 / n,
        avg_glossaries: glossaries as f64 / n,
        queries_with_results: hits,
        rss_after_mib: store::rss_mib(),
        peak_rss_mib: store::peak_rss_mib(),
    })
}

fn all(synth: &Path, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let exe = std::env::current_exe()?;
    let mut zips: Vec<PathBuf> = std::fs::read_dir(synth)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "zip"))
        .collect();
    zips.sort();
    let mut imports = Vec::new();
    for zip in &zips {
        let stem = zip
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let db = out.join(format!("{stem}.redb"));
        // One process per import: VmHWM measures the peak of that import alone.
        let o = Command::new(&exe)
            .arg("import")
            .arg(zip)
            .arg(&db)
            .output()?;
        if !o.status.success() {
            bail!(
                "import of {} failed:\n{}",
                zip.display(),
                String::from_utf8_lossy(&o.stderr)
            );
        }
        let r: serde_json::Value = serde_json::from_slice(&o.stdout)?;
        eprintln!(
            "import {stem}: {} terms, {} meta, {:.0} MB JSON in {:.1} s, peak RSS {:.0} MiB, db {:.0} MB",
            r["terms"],
            r["meta"],
            r["json_bytes"].as_f64().unwrap_or(0.0) / 1e6,
            r["seconds"].as_f64().unwrap_or(0.0),
            r["peak_rss_mib"].as_f64().unwrap_or(0.0),
            r["db_bytes"].as_f64().unwrap_or(0.0) / 1e6,
        );
        imports.push(r);
    }
    let mut benches = Vec::new();
    for cache_mib in [16usize, 64] {
        let o = Command::new(&exe)
            .arg("bench")
            .arg(out)
            .arg(synth.join("queries.txt"))
            .arg(cache_mib.to_string())
            .output()?;
        if !o.status.success() {
            bail!("bench failed:\n{}", String::from_utf8_lossy(&o.stderr));
        }
        let b: serde_json::Value = serde_json::from_slice(&o.stdout)?;
        eprintln!(
            "lookup (cache {cache_mib} MiB/dict): 1st pass p50 {:.2} p95 {:.2} p99 {:.2} max {:.2} ms; 2nd pass p95 {:.2} p99 {:.2} ms; RSS {:.0} MiB",
            b["first_pass"]["p50_ms"].as_f64().unwrap_or(0.0),
            b["first_pass"]["p95_ms"].as_f64().unwrap_or(0.0),
            b["first_pass"]["p99_ms"].as_f64().unwrap_or(0.0),
            b["first_pass"]["max_ms"].as_f64().unwrap_or(0.0),
            b["second_pass"]["p95_ms"].as_f64().unwrap_or(0.0),
            b["second_pass"]["p99_ms"].as_f64().unwrap_or(0.0),
            b["rss_after_mib"].as_f64().unwrap_or(0.0),
        );
        benches.push(b);
    }
    let report = serde_json::json!({ "imports": imports, "lookup": benches });
    let path = out.join("report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    eprintln!("report: {}", path.display());
    Ok(())
}
