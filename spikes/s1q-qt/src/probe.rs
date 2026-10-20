// SPDX-License-Identifier: GPL-3.0-only

//! `Probe`: the Rust side of the spike, exposed to QML. It reads the command line,
//! splits text into graphemes (as the real subtitle overlay will), and measures
//! CPU, memory and GPU usage from /proc, like the S1 spike.

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Instant;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde_json::{Value, json};
use unicode_segmentation::UnicodeSegmentation;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, video)]
        #[qproperty(QString, mode)]
        #[qproperty(i32, seconds)]
        #[qproperty(bool, fullscreen)]
        #[qproperty(bool, static_sub)]
        #[qproperty(bool, no_overlay)]
        type Probe = super::ProbeRust;

        /// The graphemes of `text`, as a JSON array of strings.
        #[qinvokable]
        fn graphemes(self: &Probe, text: &QString) -> QString;

        /// Starts a CPU measurement window.
        #[qinvokable]
        fn cpu_snapshot(self: Pin<&mut Probe>);

        /// Adds CPU/GPU/memory data to the JSON report from QML, prints it, exits.
        #[qinvokable]
        fn finish(self: &Probe, report: &QString);

        /// Resident memory of the process, in MiB.
        #[qinvokable]
        fn rss_mib(self: &Probe) -> f64;
    }
}

/// CPU ticks per second (`getconf CLK_TCK`, 100 on Linux x86_64).
const CLK_TCK: f64 = 100.0;

pub struct ProbeRust {
    video: QString,
    mode: QString,
    seconds: i32,
    fullscreen: bool,
    static_sub: bool,
    no_overlay: bool,
    start: Option<(Instant, u64, HashMap<u32, u64>)>,
}

impl Default for ProbeRust {
    fn default() -> Self {
        let mut video = String::new();
        let mut mode = "interactive".to_string();
        let mut seconds = 0;
        let mut fullscreen = false;
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--fullscreen" => fullscreen = true,
                "--measure" | "--stress" => {
                    mode = a.trim_start_matches("--").to_string();
                    seconds = it.next().and_then(|v| v.parse().ok()).unwrap_or(30);
                }
                _ => video = a,
            }
        }
        if let Ok(abs) = std::fs::canonicalize(&video) {
            video = abs.to_string_lossy().into_owned();
        }
        Self {
            video: QString::from(video.as_str()),
            mode: QString::from(mode.as_str()),
            seconds,
            fullscreen,
            static_sub: std::env::var_os("S1Q_STATIC_SUB").is_some(),
            no_overlay: std::env::var_os("S1Q_NO_OVERLAY").is_some(),
            start: None,
        }
    }
}

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
/// every process in /proc takes milliseconds (more under load) on the GUI
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

fn comm(pid: &str) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
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

impl qobject::Probe {
    fn graphemes(&self, text: &QString) -> QString {
        let s = text.to_string();
        let g: Vec<&str> = s.graphemes(true).collect();
        QString::from(serde_json::to_string(&g).unwrap_or_default().as_str())
    }

    fn cpu_snapshot(self: Pin<&mut Self>) {
        let own = stat_fields("self").map_or(0, |(_, t)| t);
        let desc = descendants(std::process::id())
            .into_iter()
            .filter_map(|p| Some((p, stat_fields(&p.to_string())?.1)))
            .collect();
        self.rust_mut().start = Some((Instant::now(), own, desc));
    }

    fn finish(&self, report: &QString) {
        let mut v: Value = serde_json::from_str(&report.to_string()).unwrap_or(json!({}));
        if let Some((t0, own0, desc0)) = &self.start {
            let secs = t0.elapsed().as_secs_f64();
            let pct = |ticks: u64| ticks as f64 / CLK_TCK / secs * 100.0;
            let own = stat_fields("self")
                .map_or(0, |(_, t)| t)
                .saturating_sub(*own0);
            let mut desc_total = 0;
            for pid in descendants(std::process::id()) {
                let t = stat_fields(&pid.to_string()).map_or(0, |(_, t)| t);
                desc_total += t.saturating_sub(*desc0.get(&pid).unwrap_or(&0));
            }
            v["cpu_seconds"] = json!(secs);
            v["cpu_pct_own_process"] = json!(pct(own));
            v["cpu_pct_descendants"] = json!(pct(desc_total));
        }
        let mut gpus = vec![json!({ "comm": "self", "drm_pdev": drm_devices("self") })];
        for pid in descendants(std::process::id()) {
            let devs = drm_devices(&pid.to_string());
            if !devs.is_empty() {
                gpus.push(json!({ "comm": comm(&pid.to_string()), "drm_pdev": devs }));
            }
        }
        v["gpus"] = json!(gpus);
        v["rss_mib_own"] = json!(status_kib("self", "VmRSS:") / 1024.0);
        v["env"] = json!({
            "qt_qpa_platform": std::env::var("QT_QPA_PLATFORM").ok(),
            "xdg_session_type": std::env::var("XDG_SESSION_TYPE").ok(),
            "qsg_render_loop": std::env::var("QSG_RENDER_LOOP").ok(),
            "mpv_options": std::env::var("S1Q_MPV_OPTIONS").ok(),
            "static_sub": self.static_sub,
            "no_overlay": self.no_overlay,
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        std::process::exit(0);
    }

    fn rss_mib(&self) -> f64 {
        status_kib("self", "VmRSS:") / 1024.0
    }
}
