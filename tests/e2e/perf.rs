// SPDX-License-Identifier: GPL-3.0-only

//! L6: the performance requirements measured in the real app (TEST §7). The
//! off-screen KWin draws with the CPU, so playback (T-PERF-03) is measured by
//! hand on the reference machine instead; startup, opening and memory are
//! what the app itself does, and are measured here.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::support::{Desktop, exit_status};

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
