// SPDX-License-Identifier: GPL-3.0-only

//! Measurements for the S1 exit criteria (ROADMAP M0): dropped frames and CPU
//! (T-PERF-03), resizing and fullscreen, 50 player open/close cycles without RSS
//! growth above 10%.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use dioxus::desktop::{DesktopContext, LogicalSize};
use futures_channel::mpsc::UnboundedSender;
use serde_json::{Value, json};

use crate::compositor;
use crate::player::{self, Options, RENDER_SIZE, RENDERS, UiMsg};

/// CPU ticks per second (`getconf CLK_TCK`, 100 on Linux x86_64).
const CLK_TCK: f64 = 100.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Interactive,
    Measure(u64),
    Stress(u32),
    Exercise,
}

fn stat_fields(pid: &str) -> Option<(u32, u64)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // fields after "(comm)": state, ppid, …, utime (14th), stime (15th)
    let rest = &s[s.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((
        f.get(1)?.parse().ok()?,
        f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?,
    ))
}

fn descendants(root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let name = e.file_name();
            let Some(pid) = name.to_str().and_then(|n| n.parse::<u32>().ok()) else {
                continue;
            };
            if let Some((ppid, _)) = stat_fields(&pid.to_string()) {
                children.entry(ppid).or_default().push(pid);
            }
        }
    }
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(p) = stack.pop() {
        for c in children.get(&p).into_iter().flatten() {
            out.push(*c);
            stack.push(*c);
        }
    }
    out
}

fn status_kib(pid: &str, key: &str) -> f64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0.0)
}

fn comm(pid: u32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct Snapshot {
    at: Instant,
    own_ticks: u64,
    desc_ticks: HashMap<u32, u64>,
    drops: i64,
    delayed: i64,
    mistimed: i64,
    renders: u64,
}

fn snapshot() -> Snapshot {
    let me = std::process::id();
    let desc_ticks = descendants(me)
        .into_iter()
        .filter_map(|p| Some((p, stat_fields(&p.to_string())?.1)))
        .collect();
    Snapshot {
        at: Instant::now(),
        own_ticks: stat_fields("self").map_or(0, |(_, t)| t),
        desc_ticks,
        drops: player::get_i64("frame-drop-count").unwrap_or(-1),
        delayed: player::get_i64("vo-delayed-frame-count").unwrap_or(-1),
        mistimed: player::get_i64("mistimed-frame-count").unwrap_or(-1),
        renders: RENDERS.get(),
    }
}

fn compare(a: &Snapshot, b: &Snapshot) -> Value {
    let secs = b.at.duration_since(a.at).as_secs_f64();
    let pct = |ticks: u64| ticks as f64 / CLK_TCK / secs * 100.0;
    let mut per_child = Vec::new();
    let mut desc_total = 0u64;
    for (pid, t1) in &b.desc_ticks {
        let d = t1.saturating_sub(*a.desc_ticks.get(pid).unwrap_or(&0));
        desc_total += d;
        per_child.push(json!({ "pid": pid, "comm": comm(*pid), "cpu_pct": pct(d),
            "rss_mib": status_kib(&pid.to_string(), "VmRSS:") / 1024.0 }));
    }
    let own = b.own_ticks.saturating_sub(a.own_ticks);
    let (w, h, scale) = RENDER_SIZE.get();
    json!({
        "seconds": secs,
        "frame_drop_count": b.drops - a.drops,
        "vo_delayed_frame_count": b.delayed - a.delayed,
        "mistimed_frame_count": b.mistimed - a.mistimed,
        "renders_per_second": (b.renders - a.renders) as f64 / secs,
        "cpu_pct_own_process": pct(own),
        "cpu_pct_descendants": pct(desc_total),
        "cpu_pct_total": pct(own + desc_total),
        "descendants": per_child,
        "rss_mib_own": status_kib("self", "VmRSS:") / 1024.0,
        "hwdec_current": player::get_string("hwdec-current"),
        "estimated_vf_fps": player::get_f64("estimated-vf-fps"),
        "display_fps": player::get_f64("display-fps"),
        "render_size": [w, h],
        "scale_factor": scale,
    })
}

fn finish(report: Value) -> ! {
    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_default()
    );
    std::process::exit(0)
}

fn after(ms: u64, f: impl FnOnce() + 'static) {
    glib::timeout_add_local_once(Duration::from_millis(ms), f);
}

/// Waits until playback has started (`time-pos` > 0.5 s), then calls `f`.
fn when_playing(f: impl FnOnce() + 'static) {
    let mut f = Some(f);
    let started = Instant::now();
    glib::timeout_add_local(Duration::from_millis(200), move || {
        let playing = player::get_f64("time-pos").is_some_and(|t| t > 0.5);
        if playing || started.elapsed() > Duration::from_secs(20) {
            if let Some(f) = f.take() {
                f();
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Starts the selected mode (no action in interactive mode).
pub fn start(
    mode: Mode,
    desktop: DesktopContext,
    overlay: gtk::Overlay,
    area: gtk::GLArea,
    opts: Options,
    tx: UnboundedSender<UiMsg>,
) {
    let env = compositor::describe(&desktop);
    match mode {
        Mode::Interactive => {}
        Mode::Measure(seconds) => when_playing(move || {
            // 3 s to settle, then the measurement window.
            after(3000, move || {
                let a = snapshot();
                after(seconds * 1000, move || {
                    let b = snapshot();
                    finish(json!({ "mode": "measure", "env": env, "video": opts.video,
                        "hwdec_requested": opts.hwdec, "result": compare(&a, &b) }));
                });
            });
        }),
        Mode::Exercise => when_playing(move || {
            let a = snapshot();
            let steps: Rc<Vec<(&str, Option<(f64, f64)>, Option<bool>)>> = Rc::new(
                [
                    ("960x540", Some((960.0, 540.0)), None),
                    ("1600x900", Some((1600.0, 900.0)), None),
                    ("fullscreen", None, Some(true)),
                    ("windowed", None, Some(false)),
                    ("640x360", Some((640.0, 360.0)), None),
                    ("1280x720", Some((1280.0, 720.0)), None),
                ]
                .repeat(3),
            );
            let log = Rc::new(std::cell::RefCell::new(Vec::new()));
            exercise_step(desktop, steps, 0, log, a, env, opts);
        }),
        Mode::Stress(cycles) => when_playing(move || {
            stress_cycle(overlay, area, opts, tx, 1, cycles, Vec::new(), env);
        }),
    }
}

fn exercise_step(
    desktop: DesktopContext,
    steps: Rc<Vec<(&'static str, Option<(f64, f64)>, Option<bool>)>>,
    i: usize,
    log: Rc<std::cell::RefCell<Vec<Value>>>,
    start: Snapshot,
    env: Value,
    opts: Options,
) {
    if i == steps.len() {
        let b = snapshot();
        let steps_log = log.borrow().clone();
        finish(json!({ "mode": "exercise", "env": env, "video": opts.video,
            "steps": steps_log, "result": compare(&start, &b) }));
    }
    let (name, size, fullscreen) = steps[i];
    if let Some(fs) = fullscreen {
        desktop.set_fullscreen(fs);
    }
    if let Some((w, h)) = size {
        desktop.window.set_inner_size(LogicalSize::new(w, h));
    }
    after(1500, move || {
        let (w, h, scale) = RENDER_SIZE.get();
        let inner = desktop.window.inner_size();
        log.borrow_mut()
            .push(json!({ "step": name, "render": [w, h], "scale": scale,
            "window_physical": [inner.width, inner.height] }));
        exercise_step(desktop, steps, i + 1, log, start, env, opts);
    });
}

#[allow(clippy::too_many_arguments)]
fn stress_cycle(
    overlay: gtk::Overlay,
    area: gtk::GLArea,
    opts: Options,
    tx: UnboundedSender<UiMsg>,
    cycle: u32,
    cycles: u32,
    mut rss: Vec<(u32, f64)>,
    env: Value,
) {
    let renders_at_start = RENDERS.get();
    let started = Instant::now();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        let shown = RENDERS.get().saturating_sub(renders_at_start);
        // A cycle counts once the new player has drawn at least 20 frames.
        if shown < 20 && started.elapsed() < Duration::from_secs(10) {
            return glib::ControlFlow::Continue;
        }
        let rss_now = status_kib("self", "VmRSS:") / 1024.0;
        if cycle == 5 || cycle == cycles {
            rss.push((cycle, rss_now));
        }
        eprintln!("[s1] cycle {cycle}/{cycles}: {shown} frames, RSS {rss_now:.0} MiB");
        if cycle == cycles {
            let first = rss.first().map_or(0.0, |r| r.1);
            let last = rss.last().map_or(0.0, |r| r.1);
            finish(
                json!({ "mode": "stress", "env": env, "video": opts.video, "cycles": cycles,
                "rss_mib_after_cycle_5": first, "rss_mib_at_end": last,
                "growth_pct": if first > 0.0 { (last - first) / first * 100.0 } else { 0.0 },
                "player_active": player::is_active() }),
            );
        }
        let o = opts.clone();
        let t = tx.clone();
        let new_area = compositor::replace_gl_area(&overlay, &area, |a| player::attach(a, o, t));
        stress_cycle(
            overlay.clone(),
            new_area,
            opts.clone(),
            tx.clone(),
            cycle + 1,
            cycles,
            rss.clone(),
            env.clone(),
        );
        glib::ControlFlow::Break
    });
}
