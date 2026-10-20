// SPDX-License-Identifier: GPL-3.0-only

//! CPU, memory and GPU devices from /proc, as in S1q's `Probe`, and the two
//! measurement procedures: the video one of S1/S1q (`--measure`) and the
//! popup one (`--popups`).

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::mpv::{Mpv, RenderStats};

/// CPU ticks per second (`getconf CLK_TCK`, 100 on Linux x86_64).
const CLK_TCK: f64 = 100.0;

fn stat_fields(pid: &str) -> Option<(u32, u64)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((
        f.get(1)?.parse().ok()?,
        f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?,
    ))
}

/// The descendants of `root`, from each thread's list of children: reading
/// every process in /proc takes milliseconds (more under load) on the UI
/// thread, enough to drop frames at the start of the window.
fn descendants(root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(p) = stack.pop() {
        for task in std::fs::read_dir(format!("/proc/{p}/task"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let Ok(list) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for c in list
                .split_whitespace()
                .filter_map(|c| c.parse::<u32>().ok())
            {
                out.push(c);
                stack.push(c);
            }
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

fn drm_devices(pid: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in std::fs::read_dir(format!("/proc/{pid}/fdinfo"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Ok(text) = std::fs::read_to_string(e.path()) {
            for line in text.lines() {
                if let Some(dev) = line.strip_prefix("drm-pdev:") {
                    let dev = dev.trim().to_string();
                    if !out.contains(&dev) {
                        out.push(dev);
                    }
                }
            }
        }
    }
    out
}

/// Resident memory of the process, in MiB.
pub fn rss_mib() -> f64 {
    status_kib("self", "VmRSS:") / 1024.0
}

/// A CPU measurement window, for the process and its children.
pub struct Cpu {
    start: Instant,
    own: u64,
    desc: HashMap<u32, u64>,
}

impl Cpu {
    pub fn start() -> Cpu {
        Cpu {
            start: Instant::now(),
            own: stat_fields("self").map_or(0, |(_, t)| t),
            desc: descendants(std::process::id())
                .into_iter()
                .filter_map(|p| Some((p, stat_fields(&p.to_string())?.1)))
                .collect(),
        }
    }

    /// CPU, memory and GPU devices, added to `v`.
    pub fn finish(&self, v: &mut Value) {
        let secs = self.start.elapsed().as_secs_f64();
        let pct = |ticks: u64| ticks as f64 / CLK_TCK / secs * 100.0;
        let own = stat_fields("self")
            .map_or(0, |(_, t)| t)
            .saturating_sub(self.own);
        let mut desc_total = 0;
        for pid in descendants(std::process::id()) {
            let t = stat_fields(&pid.to_string()).map_or(0, |(_, t)| t);
            desc_total += t.saturating_sub(*self.desc.get(&pid).unwrap_or(&0));
        }
        v["cpu_seconds"] = json!(secs);
        v["cpu_pct_own_process"] = json!(pct(own));
        v["cpu_pct_descendants"] = json!(pct(desc_total));
        v["gpus"] = json!(drm_devices("self"));
        v["rss_mib_own"] = json!(rss_mib());
    }
}

struct Snap {
    t: Instant,
    drops: i64,
    delayed: i64,
    renders: u64,
    new_frames: u64,
}

fn snap(mpv: &Mpv, stats: &RenderStats) -> Snap {
    Snap {
        t: Instant::now(),
        drops: mpv.get_i64("frame-drop-count").unwrap_or(0),
        delayed: mpv.get_i64("vo-delayed-frame-count").unwrap_or(0),
        renders: stats.renders.load(Ordering::Relaxed),
        new_frames: stats.new_frames.load(Ordering::Relaxed),
    }
}

/// S1/S1q's measurement: 3 s after playback passes 0.5 s, `seconds` of
/// playback; dropped and delayed frames, renders per second, render times,
/// CPU, subtitle changes per second.
pub struct VideoMeasure {
    seconds: u64,
    first_frame: Option<Instant>,
    a: Option<(Snap, Cpu, String, u64)>,
    last_drops: i64,
    /// Drops in each second of the window, counted from its start.
    timeline: Vec<i64>,
}

/// The system's load averages (1, 5 and 15 minutes).
fn loadavg() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

impl VideoMeasure {
    pub fn new(seconds: u64) -> VideoMeasure {
        VideoMeasure {
            seconds,
            first_frame: None,
            a: None,
            last_drops: 0,
            timeline: Vec::new(),
        }
    }

    /// Call often (a few times per second); returns the report when done.
    pub fn tick(&mut self, mpv: &Mpv, stats: &RenderStats, extra: Value) -> Option<Value> {
        let now = Instant::now();
        // as S1q: from the first tick past 0.5 s of playback
        if stats.renders.load(Ordering::Relaxed) == 0
            || mpv.get_f64("time-pos").unwrap_or(0.0) <= 0.5
        {
            return None;
        }
        let first = *self.first_frame.get_or_insert(now);
        let Some((a, cpu, load_start, subs)) = &self.a else {
            if now >= first + Duration::from_secs(3) {
                let cpu = Cpu::start();
                let s = snap(mpv, stats);
                self.last_drops = s.drops;
                self.a = Some((
                    s,
                    cpu,
                    loadavg(),
                    crate::SUBTITLE_UPDATES.load(Ordering::Relaxed),
                ));
            }
            return None;
        };
        let d = mpv.get_i64("frame-drop-count").unwrap_or(0);
        let second = ((now - a.t).as_secs() as usize).min(self.seconds.max(1) as usize - 1);
        if self.timeline.len() <= second {
            self.timeline.resize(second + 1, 0);
        }
        self.timeline[second] += d - self.last_drops;
        self.last_drops = d;
        if now < a.t + Duration::from_secs(self.seconds) {
            return None;
        }
        let b = snap(mpv, stats);
        let secs = (b.t - a.t).as_secs_f64();
        let (p50, p95, max) = stats.percentiles();
        let mut v = json!({
            "mode": "measure",
            "seconds": secs,
            "frame_drop_count": b.drops - a.drops,
            "vo_delayed_frame_count": b.delayed - a.delayed,
            "renders_per_second": (b.renders - a.renders) as f64 / secs,
            "new_frames_per_second": (b.new_frames - a.new_frames) as f64 / secs,
            "subtitle_updates_per_second": (crate::SUBTITLE_UPDATES.load(Ordering::Relaxed) - subs) as f64 / secs,
            "render_us": { "p50": p50, "p95": p95, "max": max },
            "render_size": *stats.size.lock().unwrap(),
            "hwdec_current": mpv.get_string("hwdec-current"),
            "estimated_vf_fps": mpv.get_f64("estimated-vf-fps"),
            "drops_per_second_timeline": self.timeline,
            "loadavg_start": load_start,
            "loadavg_end": loadavg(),
            "env": {
                "wgpu_backend": std::env::var("WGPU_BACKEND").ok(),
                "mpv_options": std::env::var("S5_MPV_OPTIONS").ok(),
            },
        });
        cpu.finish(&mut v);
        if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
            m.extend(e);
        }
        Some(v)
    }
}

/// Marks the frame in which something was first drawn: the toolkit calls
/// [`FrameProbe::mark`] from its render path (a custom primitive or paint
/// callback inside the popup).
#[derive(Debug, Default)]
pub struct FrameProbe {
    generation: AtomicU64,
    at: Mutex<Option<Instant>>,
}

impl FrameProbe {
    pub fn mark(&self, generation: u64) {
        if self.generation.swap(generation, Ordering::AcqRel) != generation {
            *self.at.lock().unwrap() = Some(Instant::now());
        }
    }

    fn drawn(&self, generation: u64) -> Option<Instant> {
        (self.generation.load(Ordering::Acquire) == generation)
            .then(|| *self.at.lock().unwrap())
            .flatten()
    }
}

/// What the popup measurement asks the UI to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupCommand {
    /// Show popup `index`, with this generation for its frame probe.
    Show {
        index: usize,
        generation: u64,
    },
    Hide,
}

enum Phase {
    Wait(Instant),
    Shown {
        index: usize,
        generation: u64,
        at: Instant,
    },
    Hold(Instant),
}

/// Shows each popup in turn, `rounds` times, and times the first frame that
/// draws it after the state changed.
pub struct PopupMeasure {
    names: Vec<String>,
    rounds: usize,
    step: usize,
    generation: u64,
    phase: Phase,
    results: BTreeMap<String, Vec<f64>>,
    cpu: Option<Cpu>,
}

impl PopupMeasure {
    pub fn new(names: Vec<String>, rounds: usize) -> PopupMeasure {
        PopupMeasure {
            names,
            rounds,
            step: 0,
            generation: 0,
            phase: Phase::Wait(Instant::now() + Duration::from_secs(3)),
            results: BTreeMap::new(),
            cpu: None,
        }
    }

    /// Call on every frame and timer tick. `Ok(command)` for the UI, or the
    /// report when done.
    pub fn tick(&mut self, probe: &FrameProbe) -> Result<Option<PopupCommand>, Value> {
        let now = Instant::now();
        match self.phase {
            Phase::Wait(until) if now >= until => {
                if self.step == self.names.len() * self.rounds {
                    let mut v = json!({
                        "mode": "popups",
                        "first_frame_ms": self.results,
                    });
                    if let Some(cpu) = &self.cpu {
                        cpu.finish(&mut v);
                    }
                    return Err(v);
                }
                if self.cpu.is_none() {
                    self.cpu = Some(Cpu::start());
                }
                let index = self.step % self.names.len();
                self.generation += 1;
                self.phase = Phase::Shown {
                    index,
                    generation: self.generation,
                    at: now,
                };
                Ok(Some(PopupCommand::Show {
                    index,
                    generation: self.generation,
                }))
            }
            Phase::Shown {
                index,
                generation,
                at,
            } => {
                let Some(drawn) = probe.drawn(generation) else {
                    return Ok(None);
                };
                self.results
                    .entry(self.names[index].clone())
                    .or_default()
                    .push((drawn - at).as_secs_f64() * 1000.0);
                self.phase = Phase::Hold(now + Duration::from_millis(700));
                Ok(None)
            }
            Phase::Hold(until) if now >= until => {
                self.step += 1;
                self.phase = Phase::Wait(now + Duration::from_millis(300));
                Ok(Some(PopupCommand::Hide))
            }
            _ => Ok(None),
        }
    }

    /// The command being timed: the UI records when it applied it.
    pub fn applied(&mut self) {
        if let Phase::Shown {
            index, generation, ..
        } = self.phase
        {
            self.phase = Phase::Shown {
                index,
                generation,
                at: Instant::now(),
            };
        }
    }
}
