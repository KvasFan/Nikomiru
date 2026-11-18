// SPDX-License-Identifier: GPL-3.0-only

//! Tools on one line of the player (M121…M125): subtitles read from the
//! speech (SUB-062), a machine translation (TRN-001), an explanation
//! (AIX-001), shadowing (SHD-001) and text read from the picture (OCR-001).
//! Each one runs only when asked for, on the line shown in the panel.

use std::path::PathBuf;

use nikomiru_core::config::Config;

/// Reading a video's speech into subtitles (SUB-062).
#[derive(Debug, Clone)]
pub struct GenerateJob {
    /// The video.
    pub video: PathBuf,
    /// The audio stream's ff-index; `None`: the first.
    pub stream: Option<i64>,
    /// Its length, for the timeouts.
    pub duration_ms: i64,
    /// Where the video's subtitles are kept (MET-008).
    pub folder: PathBuf,
    /// The settings now.
    pub config: Box<Config>,
}

/// Shadowing one line (SHD-001): the microphone recorded, then read.
#[derive(Debug, Clone)]
pub struct ShadowJob {
    /// The line said.
    pub line: String,
    /// How long to record.
    pub ms: i64,
    /// Where the recording goes.
    pub dir: PathBuf,
    /// The settings now.
    pub config: Box<Config>,
}

/// What shadowing came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadowed {
    /// The recording.
    pub recording: PathBuf,
    /// What whisper.cpp heard; `None` without it or its model.
    pub said: Option<String>,
    /// How close that is to the line, 0–100.
    pub score: Option<u8>,
}

/// An explanation's request (AIX-001).
#[derive(Debug, Clone)]
pub struct ExplainJob {
    /// The line, the one before and the one after.
    pub ask: nikomiru_online::claude::Ask,
    /// The settings now.
    pub config: Box<Config>,
}

/// Where the answers go.
pub type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// The app's side of these tools.
pub trait AssistService: Send {
    /// Reads the speech of a video into an SRT kept with its subtitles;
    /// `progress` gets the percentage.
    fn generate(
        &self,
        job: GenerateJob,
        progress: Box<dyn Fn(u8) + Send + Sync>,
        done: Done<PathBuf>,
    );
    /// Stops the reading under way.
    fn cancel_generate(&self);
    /// Translates `text` into `target` with DeepL (TRN-001).
    fn translate(&self, text: String, target: String, config: Box<Config>, done: Done<String>);
    /// Explains a line with Claude (AIX-001).
    fn explain(&self, job: ExplainJob, done: Done<String>);
    /// Records the microphone and reads it (SHD-001).
    fn shadow(&self, job: ShadowJob, done: Done<Shadowed>);
    /// Stops the recording under way; nothing is read.
    fn stop_shadow(&self);
}

/// No such tools: the interface's tests.
#[derive(Debug, Default)]
pub struct NoAssist;

impl AssistService for NoAssist {
    fn generate(&self, _: GenerateJob, _: Box<dyn Fn(u8) + Send + Sync>, done: Done<PathBuf>) {
        done(Err(String::new()));
    }
    fn cancel_generate(&self) {}
    fn translate(&self, _: String, _: String, _: Box<Config>, done: Done<String>) {
        done(Err(String::new()));
    }
    fn explain(&self, _: ExplainJob, done: Done<String>) {
        done(Err(String::new()));
    }
    fn shadow(&self, _: ShadowJob, done: Done<Shadowed>) {
        done(Err(String::new()));
    }
    fn stop_shadow(&self) {}
}

/// An answer that is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Answer {
    /// Not asked for.
    #[default]
    None,
    /// Under way.
    Busy,
    /// The text.
    Done(String),
    /// Why there is none.
    Failed(String),
}

impl Answer {
    /// For the harness.
    pub fn json(&self) -> serde_json::Value {
        match self {
            Answer::None => serde_json::Value::Null,
            Answer::Busy => serde_json::json!({ "busy": true }),
            Answer::Done(text) => serde_json::json!({ "text": text }),
            Answer::Failed(error) => serde_json::json!({ "error": error }),
        }
    }
}

/// Where shadowing stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Shadow {
    /// Not started.
    #[default]
    Idle,
    /// The microphone is recording.
    Recording,
    /// Done.
    Done(Shadowed),
    /// Why it failed.
    Failed(String),
}

/// Reading text from the picture (OCR-001).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reading {
    /// Not asked for.
    #[default]
    Idle,
    /// The user drags over the picture.
    Choosing,
    /// The frame is saved and read.
    Busy,
}

/// The panel and what it holds.
#[derive(Debug, Clone, Default)]
pub struct LineTools {
    /// It is shown.
    pub open: bool,
    /// The line: the cue's text, or the text read from the picture.
    pub text: String,
    /// The line before it, for an explanation's context.
    pub before: String,
    /// The line after it.
    pub after: String,
    /// The cue's start and end without the offset, when it is a cue.
    pub cue: Option<(i64, i64)>,
    /// A line read from the picture: its index in the Texthooker's lines,
    /// where it is looked up (HOK-004).
    pub hooked: Option<usize>,
    /// The translation.
    pub translation: Answer,
    /// The explanation.
    pub explanation: Answer,
    /// Shadowing.
    pub shadow: Shadow,
    /// Reading from the picture.
    pub reading: Reading,
    /// Stamps answers: one for an older line is dropped.
    pub serial: u64,
}

impl LineTools {
    /// Puts `text` in the panel: the earlier answers go with the old line.
    pub fn set_line(
        &mut self,
        text: String,
        before: String,
        after: String,
        cue: Option<(i64, i64)>,
    ) {
        if self.text != text || self.cue != cue {
            self.serial += 1;
            self.translation = Answer::None;
            self.explanation = Answer::None;
            if self.shadow != Shadow::Recording {
                self.shadow = Shadow::Idle;
            }
            self.hooked = None;
        }
        self.open = true;
        self.text = text;
        self.before = before;
        self.after = after;
        self.cue = cue;
    }

    /// For the harness.
    pub fn json(&self) -> serde_json::Value {
        let shadow = match &self.shadow {
            Shadow::Idle => serde_json::Value::Null,
            Shadow::Recording => serde_json::json!({ "recording": true }),
            Shadow::Done(s) => serde_json::json!({
                "recording": false,
                "file": s.recording,
                "said": s.said,
                "score": s.score,
            }),
            Shadow::Failed(e) => serde_json::json!({ "error": e }),
        };
        serde_json::json!({
            "open": self.open,
            "text": self.text,
            "before": self.before,
            "after": self.after,
            "hooked": self.hooked,
            "translation": self.translation.json(),
            "explanation": self.explanation.json(),
            "shadow": shadow,
            "reading": match self.reading {
                Reading::Idle => "idle",
                Reading::Choosing => "choosing",
                Reading::Busy => "busy",
            },
        })
    }
}

/// How long shadowing records for a cue `ms` long: the time to say it
/// again, unhurried, within SHD-001's bounds.
pub fn recording_ms(cue_ms: Option<i64>) -> i64 {
    match cue_ms {
        Some(ms) if ms > 0 => {
            (ms * 3 / 2 + 1_000).clamp(2_000, nikomiru_media::speech::MAX_RECORDING_MS)
        }
        _ => 5_000,
    }
}

/// The part of the picture between two points of the window, as shares of
/// `video` (left, top, width, height); `None` when it is too small to hold
/// a character.
pub fn area(video: [f32; 4], from: (f32, f32), to: (f32, f32)) -> Option<[f32; 4]> {
    let [vx, vy, vw, vh] = video;
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let clamp = |x: f32, lo: f32, hi: f32| x.max(lo).min(hi);
    let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
    let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
    let (x0, x1) = (clamp(x0, vx, vx + vw), clamp(x1, vx, vx + vw));
    let (y0, y1) = (clamp(y0, vy, vy + vh), clamp(y1, vy, vy + vh));
    if x1 - x0 < 8.0 || y1 - y0 < 8.0 {
        return None;
    }
    Some([
        (x0 - vx) / vw,
        (y0 - vy) / vh,
        (x1 - x0) / vw,
        (y1 - y0) / vh,
    ])
}

/// The language explanations are written in: the interface's, by name.
pub fn language_name(code: &str) -> &'static str {
    match code.split(['-', '_']).next().unwrap_or_default() {
        "it" => "Italian",
        "ja" => "Japanese",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "uk" => "Ukrainian",
        "zh" => "Chinese",
        "ko" => "Korean",
        _ => "English",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-TRN-002: a new line leaves the old line's answers behind.
    #[test]
    fn a_new_line_drops_the_old_answers() {
        let mut t = LineTools::default();
        t.set_line("一".into(), String::new(), String::new(), Some((0, 1000)));
        let serial = t.serial;
        t.translation = Answer::Done("one".into());
        t.shadow = Shadow::Failed("x".into());
        // the same line again: kept
        t.set_line("一".into(), String::new(), String::new(), Some((0, 1000)));
        assert_eq!(t.serial, serial);
        assert_eq!(t.translation, Answer::Done("one".into()));
        t.set_line("二".into(), "一".into(), String::new(), Some((1000, 2000)));
        assert_eq!(t.serial, serial + 1);
        assert_eq!(t.translation, Answer::None);
        assert_eq!(t.shadow, Shadow::Idle);
        assert_eq!(t.json()["before"], "一");
        // a recording goes on until it ends
        t.shadow = Shadow::Recording;
        t.set_line("三".into(), String::new(), String::new(), None);
        assert_eq!(t.shadow, Shadow::Recording);
    }

    // T-SHD-001: the recording leaves time to say the line again.
    #[test]
    fn the_recording_is_long_enough() {
        assert_eq!(recording_ms(Some(2_000)), 4_000);
        assert_eq!(recording_ms(Some(100)), 2_000);
        assert_eq!(recording_ms(Some(60_000)), 30_000);
        assert_eq!(recording_ms(None), 5_000);
    }

    // T-OCR-001: the area dragged is taken in the picture, as its shares.
    #[test]
    fn the_area_is_a_share_of_the_picture() {
        let video = [100.0, 50.0, 800.0, 450.0];
        assert_eq!(
            area(video, (500.0, 275.0), (100.0, 50.0)),
            Some([0.0, 0.0, 0.5, 0.5])
        );
        // outside the picture: cut to it
        assert_eq!(
            area(video, (0.0, 0.0), (1000.0, 600.0)),
            Some([0.0, 0.0, 1.0, 1.0])
        );
        assert_eq!(area(video, (200.0, 200.0), (203.0, 260.0)), None);
        assert_eq!(area([0.0, 0.0, 0.0, 10.0], (0.0, 0.0), (5.0, 5.0)), None);
        assert_eq!(language_name("it"), "Italian");
        assert_eq!(language_name("pt-BR"), "Portuguese");
        assert_eq!(language_name("system"), "English");
    }
}
