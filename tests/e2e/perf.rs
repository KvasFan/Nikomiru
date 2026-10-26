// SPDX-License-Identifier: GPL-3.0-only

//! L6: the performance requirements measured in the real app (TEST §7). The
//! off-screen KWin draws with the CPU, so playback (T-PERF-03) is measured by
//! hand on the reference machine instead; startup, opening and memory are
//! what the app itself does, and are measured here.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::support::{Desktop, exit_status};

/// The anonymous memory of process `pid`, resident or swapped out, in MiB:
/// unlike the RSS, the kernel reclaiming pages under pressure does not make
/// it fall (T-REL-003).
fn anon_mib(pid: u32) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let kib = |key: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (kib("RssAnon:") + kib("VmSwap:")) / 1024
}

/// The resident memory of process `pid`, in MiB.
fn rss_mib(pid: u32) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kib| kib / 1024)
}

// T-PERF-01: the library shows within 1.5 s of the start (the median of 5;
// the page cache is not dropped, which needs root)
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn perf_the_library_shows_within_1_5_s_of_the_start() {
    let mut times = Vec::new();
    for _ in 0..5 {
        let mut desk = Desktop::start();
        let started = Instant::now();
        let (mut app, mut h) = desk.launch();
        h.state_until("the library", |s| s["library_page"]["series"].is_array());
        times.push(started.elapsed());
        h.quit();
        assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    }
    times.sort();
    let median = times[times.len() / 2];
    eprintln!("T-PERF-01 start → library: {times:?}, median {median:?}");
    assert!(
        median <= Duration::from_millis(1500),
        "T-PERF-01: {median:?}"
    );
}

// T-PERF-02: a large file (1080p60, 5 min, H.264 and 10-bit HEVC) shows its
// first frame within 1 s of being opened
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures --perf`"]
fn perf_a_large_file_shows_its_first_frame_within_1_s() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["series"].is_array());
    for name in ["av_hd60_h264.mkv", "av_hd60_hevc10.mkv"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/fixtures")
            .join(name);
        if !path.exists() {
            eprintln!("T-PERF-02: {name} is missing (`cargo xtask fixtures --perf`)");
            continue;
        }
        let started = Instant::now();
        h.open(&path.canonicalize().unwrap().to_string_lossy());
        h.wait("first-frame");
        let elapsed = started.elapsed();
        eprintln!("T-PERF-02 {name}: open → first frame {elapsed:?}");
        assert!(
            elapsed <= Duration::from_secs(1),
            "T-PERF-02 {name}: {elapsed:?}"
        );
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
}

// T-PERF-09: at rest with the library open, at most 400 MB resident
#[test]
#[ignore = "L6: needs kwin_wayland and dbus-daemon"]
fn perf_at_rest_the_app_holds_at_most_400_mb() {
    let mut desk = Desktop::start();
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    std::thread::sleep(Duration::from_secs(30));
    let rss = rss_mib(app.id());
    eprintln!("T-PERF-09 idle RSS: {rss} MiB");
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert!(rss <= 381, "T-PERF-09: {rss} MiB (400 MB = 381 MiB)");
}

/// The CPU time of process `pid` so far (user and system), from
/// `/proc/<pid>/stat`.
fn cpu_time(pid: u32) -> Duration {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    // the fields after the command's closing parenthesis; utime and stime are
    // the 14th and 15th of the line, the 12th and 13th after it
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    let fields: Vec<u64> = rest
        .split_whitespace()
        .map(|f| f.parse().unwrap_or(0))
        .collect();
    let ticks = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(100);
    Duration::from_millis((fields[11] + fields[12]) * 1000 / ticks)
}

// T-PERF-03: 1080p60 H.264 and 10-bit HEVC play with the controls and the
// subtitles over them, hardware decoding on, with no frame dropped and at most
// a quarter of one core. On the session the test runs in, since the
// off-screen KWin draws with the CPU: by hand on the reference machine,
// `NIKOMIRU_PERF_SESSION=1 cargo xtask perf-playback` (a window opens there);
// `NIKOMIRU_PERF_SECONDS` shortens the 5 minutes.
#[test]
#[ignore = "L7: on the session the test runs in, by hand: cargo xtask perf-playback"]
fn l7_perf_playback_drops_no_frame() {
    if std::env::var_os("NIKOMIRU_PERF_SESSION").is_none() {
        eprintln!("skipped: set NIKOMIRU_PERF_SESSION=1 to play on this session");
        return;
    }
    let seconds: u64 = std::env::var("NIKOMIRU_PERF_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let mut desk = Desktop::this_session();
    desk.write_config("[player]\nmpv_options = { ao = \"null\", hwdec = \"auto-safe\" }\n");
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    let mut failures = Vec::new();
    for name in ["av_hd60_h264.mkv", "av_hd60_hevc10.mkv"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/fixtures")
            .join(name);
        if !path.exists() {
            failures.push(format!("{name} is missing: `cargo xtask fixtures --perf`"));
            continue;
        }
        h.open(&path.canonicalize().unwrap().to_string_lossy());
        h.wait("first-frame");
        let property = |h: &mut crate::support::Harness, name: &str| {
            h.cmd(serde_json::json!({ "cmd": "mpv_property", "name": name }))
                .as_str()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0)
        };
        let (drops, delayed) = (
            property(&mut h, "frame-drop-count"),
            property(&mut h, "vo-delayed-frame-count"),
        );
        let (cpu, started) = (cpu_time(app.id()), Instant::now());
        // the controls stay over the video, as when the user moves the mouse
        while started.elapsed() < Duration::from_secs(seconds) {
            h.cmd(serde_json::json!({ "cmd": "mouse_moved" }));
            std::thread::sleep(Duration::from_secs(1));
        }
        let share = (cpu_time(app.id()) - cpu).as_secs_f64() / started.elapsed().as_secs_f64();
        let dropped = property(&mut h, "frame-drop-count") - drops;
        let late = property(&mut h, "vo-delayed-frame-count") - delayed;
        let hwdec = h.cmd(serde_json::json!({ "cmd": "mpv_property", "name": "hwdec-current" }));
        eprintln!(
            "T-PERF-03 {name}: {dropped} dropped, {late} delayed, CPU {:.1}% of a core, hwdec {hwdec}",
            share * 100.0
        );
        if dropped > 0 || late > 0 || share > 0.25 {
            failures.push(format!(
                "{name}: {dropped} dropped, {late} delayed, CPU {:.1}%",
                share * 100.0
            ));
        }
    }
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert!(failures.is_empty(), "T-PERF-03: {failures:#?}");
}

// T-REL-003: a long session keeps its memory: a 10-minute file played in a
// loop, a word looked up every 20 s, the settings and another file every
// 5 minutes; the anonymous memory at the end, resident or swapped, is at
// most 64 MiB above its highest point of the first 10 minutes. Two hours by default
// (`NIKOMIRU_SOAK_MINUTES`); in the off-screen KWin, not in the L6 suite.
#[test]
#[ignore = "L7: two hours; run by hand before a release"]
fn l7_soak_a_long_session_keeps_its_memory() {
    let minutes: u64 = std::env::var("NIKOMIRU_SOAK_MINUTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    let fixture = |name: &str| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/fixtures")
            .join(name)
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let mut desk = Desktop::start();
    desk.import_dictionaries(&["mini-ja", "mini-freq", "mini-pitch"]);
    desk.write_config("[player]\nmpv_options = { ao = \"null\", loop-file = \"inf\" }\n");
    let (mut app, mut h) = desk.launch();
    h.state_until("the library", |s| s["library_page"]["open"] == true);
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    let started = Instant::now();
    let (mut warm, mut samples) = (0u64, Vec::new());
    let mut last_sample = Duration::ZERO;
    let mut round = 0u64;
    while started.elapsed() < Duration::from_secs(minutes * 60) {
        round += 1;
        h.cmd(serde_json::json!({ "cmd": "lookup", "text": "食べました" }));
        h.wait("popup-rendered");
        h.key("Escape");
        if round.is_multiple_of(15) {
            // every 5 minutes: the settings, another file and back
            h.cmd(serde_json::json!({ "cmd": "settings", "tab": "dictionaries" }));
            std::thread::sleep(Duration::from_millis(500));
            h.cmd(serde_json::json!({ "cmd": "settings", "tab": "close" }));
            h.open(&fixture("av_basic.mkv"));
            h.wait("first-frame");
            h.open(&fixture("av_long.mkv"));
            h.wait("first-frame");
        }
        std::thread::sleep(Duration::from_secs(20));
        if started.elapsed() - last_sample >= Duration::from_secs(60) {
            last_sample = started.elapsed();
            let rss = anon_mib(app.id());
            if started.elapsed() <= Duration::from_secs(600) {
                warm = warm.max(rss);
            }
            samples.push(rss);
        }
    }
    let end = anon_mib(app.id());
    eprintln!(
        "T-REL-003 anonymous memory by minute (MiB): {samples:?}; first 10 min at most {warm}, at the end {end}"
    );
    h.quit();
    assert!(exit_status(&mut app, Duration::from_secs(15)).success());
    assert!(end <= warm + 64, "T-REL-003: {warm} MiB → {end} MiB");
}
