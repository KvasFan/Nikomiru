// SPDX-License-Identifier: GPL-3.0-only

//! Spike S5: what the iced and egui versions share, so they differ only in
//! the toolkit.

pub mod mpv;
pub mod popup;
pub mod probe;

use std::sync::atomic::{AtomicU64, Ordering};

use unicode_segmentation::UnicodeSegmentation;

/// How many times the subtitle has changed, for the measured rate.
pub static SUBTITLE_UPDATES: AtomicU64 = AtomicU64::new(0);

/// What to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// Play, with the overlay; keys and clicks are counted.
    Interactive,
    /// S1/S1q's measurement, for this many seconds.
    Measure(u64),
    /// Show each popup of this file in turn and time the first frame.
    Popups(String),
}

/// The command line: `<video> [--fullscreen] [--measure SECONDS | --popups FILE]`.
#[derive(Debug, Clone)]
pub struct Args {
    pub video: String,
    pub fullscreen: bool,
    pub mode: Mode,
}

impl Args {
    pub fn parse() -> Args {
        let mut video = String::new();
        let mut fullscreen = false;
        let mut mode = Mode::Interactive;
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--fullscreen" => fullscreen = true,
                "--measure" => {
                    mode = Mode::Measure(it.next().and_then(|v| v.parse().ok()).unwrap_or(30))
                }
                "--popups" => mode = Mode::Popups(it.next().unwrap_or_default()),
                _ => video = a,
            }
        }
        if let Ok(abs) = std::fs::canonicalize(&video) {
            video = abs.to_string_lossy().into_owned();
        }
        Args {
            video,
            fullscreen,
            mode,
        }
    }
}

/// The fake subtitle of S1/S1q at time `t`, as graphemes (each one is a
/// clickable item, as in the real subtitle overlay).
pub fn subtitle(t: f64) -> Vec<String> {
    SUBTITLE_UPDATES.fetch_add(1, Ordering::Relaxed);
    format!("字幕のテストです・{t:>7.2}秒・クリックしてね")
        .graphemes(true)
        .map(str::to_string)
        .collect()
}
