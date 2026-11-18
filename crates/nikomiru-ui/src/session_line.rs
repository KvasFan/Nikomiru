// SPDX-License-Identifier: GPL-3.0-only

//! The player's line tools (M121…M125): subtitles read from the speech, a
//! line translated, explained, shadowed, or read from the picture.
use super::*;
use crate::line_tools::{Answer, ExplainJob, GenerateJob, Reading, Shadow, ShadowJob};

/// What the UI layer starts for the line tools.
#[derive(Debug)]
pub enum LineJob {
    /// Read the speech of the video (SUB-062).
    Generate(GenerateJob),
    /// Translate the line of `serial` (TRN-001).
    Translate {
        /// The line's stamp.
        serial: u64,
        /// The line.
        text: String,
        /// DeepL's target language.
        target: String,
        /// The settings.
        config: Box<Config>,
    },
    /// Explain the line of `serial` (AIX-001).
    Explain(u64, ExplainJob),
    /// Record and read the user saying the line of `serial` (SHD-001).
    Shadow(u64, ShadowJob),
    /// Save the frame shown, to read `area` of it (OCR-001).
    Frame(u64, [f32; 4]),
}

/// The current cue's index and text, the texts before and after it, and its times.
type LineNow = (usize, String, String, String, (i64, i64));

impl Session {
    /// The current cue, the one before and the one after.
    fn line_now(&self) -> Option<LineNow> {
        let index = *self.subs.active().first()?;
        let cue = self.subs.cue(index)?;
        let text = |i: Option<usize>| {
            i.and_then(|i| self.subs.cue(i))
                .map(|c| c.plain_text.clone())
                .unwrap_or_default()
        };
        Some((
            index,
            cue.plain_text.clone(),
            text(index.checked_sub(1)),
            text(index.checked_add(1)),
            (cue.start_ms, cue.end_ms),
        ))
    }

    /// Puts the current cue in the panel, when there is one; the line
    /// already there stays otherwise (a line read from the picture).
    fn line_take_current(&mut self) {
        if let Some((index, text, before, after, cue)) = self.line_now() {
            self.line_tools.set_line(text, before, after, Some(cue));
            self.line_cue = Some(index);
        } else {
            self.line_tools.open = true;
        }
    }

    /// Opens or closes the panel; opened, it holds the current line.
    pub fn line_tools_toggle(&mut self, open: bool) {
        if open {
            self.line_take_current();
        } else {
            self.line_tools.open = false;
            if self.line_tools.reading == Reading::Choosing {
                self.line_tools.reading = Reading::Idle;
            }
        }
    }

    /// The panel's line, for a request; a message when there is none.
    fn line_text(&mut self) -> Option<String> {
        let text = self.line_tools.text.trim().to_string();
        if text.is_empty() {
            self.toasts.push(self.tr("line-none", &[]));
            None
        } else {
            Some(text)
        }
    }

    /// Translates the line with DeepL (TRN-001): asked for, never by itself.
    pub fn line_translate(&mut self, current: bool) {
        if current {
            self.line_take_current();
        }
        let Some(text) = self.line_text() else {
            return;
        };
        if self.line_tools.translation == Answer::Busy {
            return;
        }
        if self.services.config.online.deepl_key.trim().is_empty() {
            self.line_tools.translation = Answer::Failed(self.tr("line-no-deepl-key", &[]));
            return;
        }
        self.line_tools.translation = Answer::Busy;
        let target = nikomiru_online::deepl::target_for(self.services.translator.language());
        self.line_jobs.push(LineJob::Translate {
            serial: self.line_tools.serial,
            text,
            target,
            config: Box::new(self.services.config.clone()),
        });
    }

    /// Explains the line with Claude (AIX-001): asked for, never by itself.
    pub fn line_explain(&mut self, current: bool) {
        if current {
            self.line_take_current();
        }
        let Some(line) = self.line_text() else {
            return;
        };
        if self.line_tools.explanation == Answer::Busy {
            return;
        }
        if self.services.config.online.claude_key.trim().is_empty() {
            self.line_tools.explanation = Answer::Failed(self.tr("line-no-claude-key", &[]));
            return;
        }
        self.line_tools.explanation = Answer::Busy;
        let ask = nikomiru_online::claude::Ask {
            line,
            before: self.line_tools.before.clone(),
            after: self.line_tools.after.clone(),
            language: crate::line_tools::language_name(self.services.translator.language()).into(),
        };
        self.line_jobs.push(LineJob::Explain(
            self.line_tools.serial,
            ExplainJob {
                ask,
                config: Box::new(self.services.config.clone()),
            },
        ));
    }

    /// Shadowing (SHD-001): pauses and records the user saying the line;
    /// asked again while recording, it ends the recording.
    pub fn line_shadow(&mut self, current: bool) {
        if self.line_tools.shadow == Shadow::Recording {
            self.services.assist.stop_shadow();
            return;
        }
        if current {
            self.line_take_current();
        }
        let Some(line) = self.line_text() else {
            return;
        };
        let _ = self.services.player.set_pause(true);
        self.line_tools.shadow = Shadow::Recording;
        let ms = crate::line_tools::recording_ms(self.line_tools.cue.map(|(a, b)| b - a));
        self.line_jobs.push(LineJob::Shadow(
            self.line_tools.serial,
            ShadowJob {
                line,
                ms,
                dir: self.services.mining_dir.join("shadowing"),
                config: Box::new(self.services.config.clone()),
            },
        ));
    }

    /// Plays the last recording (SHD-001).
    pub fn line_play_recording(&mut self) {
        if let Shadow::Done(done) = &self.line_tools.shadow {
            self.word_audio_jobs
                .push(crate::word_audio::WordAudioJob::PlayFile(
                    done.recording.clone(),
                ));
        }
    }

    /// Starts choosing a part of the picture to read (OCR-001).
    pub fn line_read(&mut self) {
        if !self.view.loaded {
            return;
        }
        if !self.services.ocr.installed() {
            self.toasts.push(self.tr("line-read-no-model", &[]));
            return;
        }
        let _ = self.services.player.set_pause(true);
        self.line_tools.open = true;
        self.line_tools.reading = Reading::Choosing;
    }

    /// The part chosen, as shares of the picture.
    pub fn line_read_area(&mut self, area: [f32; 4]) {
        if self.line_tools.reading != Reading::Choosing {
            return;
        }
        self.line_tools.reading = Reading::Busy;
        self.line_tools.serial += 1;
        self.line_jobs
            .push(LineJob::Frame(self.line_tools.serial, area));
    }

    /// Choosing given up.
    pub fn line_read_cancel(&mut self) {
        if self.line_tools.reading == Reading::Choosing {
            self.line_tools.reading = Reading::Idle;
        }
    }

    /// The frame is saved: its `area` goes to the OCR worker.
    pub fn line_frame(
        &mut self,
        serial: u64,
        area: [f32; 4],
        saved: Result<PathBuf, String>,
        waker: &std::sync::Arc<dyn crate::ui::Waker>,
    ) {
        if serial != self.line_tools.serial || self.line_tools.reading != Reading::Busy {
            if let Ok(path) = saved {
                let _ = std::fs::remove_file(path);
            }
            return;
        }
        let picture = saved.and_then(|path| {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&path);
            bytes
        });
        let picture = match picture {
            Ok(bytes) => bytes,
            Err(e) => {
                self.line_tools.reading = Reading::Idle;
                self.toasts
                    .push(self.tr("line-read-failed", &[("reason", e)]));
                return;
            }
        };
        let job = crate::ocr::OcrJob {
            uid: "frame".into(),
            page: 0,
            picture,
            area,
            preset_text: None,
            pixel_bounds: None,
        };
        let w = waker.clone();
        self.services.ocr.read(
            job,
            self.translator_clone(),
            Box::new(move |(_, _, result)| {
                w.wake(crate::ui::Inbound::LineRead(serial, result.map(|r| r.text)));
            }),
        );
    }

    /// The text read from the picture: the panel's line, looked up as a
    /// Texthooker line (HOK-004).
    pub fn line_read_done(&mut self, serial: u64, result: Result<String, String>) {
        if serial != self.line_tools.serial {
            return;
        }
        self.line_tools.reading = Reading::Idle;
        match result {
            Ok(text) if !text.trim().is_empty() => {
                let text = text.trim().to_string();
                self.hooker_page.push(text.clone());
                let index = self.hooker_page.lines.iter().rposition(|l| *l == text);
                self.line_tools
                    .set_line(text, String::new(), String::new(), None);
                self.line_tools.hooked = index;
                self.line_cue = None;
            }
            Ok(_) => self.toasts.push(self.tr("line-read-empty", &[])),
            Err(e) => self
                .toasts
                .push(self.tr("line-read-failed", &[("reason", e)])),
        }
    }

    /// A click on the panel's line, on the byte `offset`: a lookup.
    pub fn line_clicked(&mut self, offset: usize, anchor: Rect) {
        if let Some(line) = self.line_tools.hooked {
            self.hooker_clicked(line, offset, anchor);
        } else if let Some(cue) = self.line_cue
            && self
                .subs
                .cue(cue)
                .is_some_and(|c| c.plain_text == self.line_tools.text)
        {
            self.select_grapheme(cue, offset, anchor);
        }
    }

    /// An answer of the worker, for the line of `serial`.
    pub fn line_translated(&mut self, serial: u64, result: Result<String, String>) {
        if serial == self.line_tools.serial {
            self.line_tools.translation = match result {
                Ok(text) => Answer::Done(text),
                Err(e) => Answer::Failed(e),
            };
        }
    }

    /// An explanation, for the line of `serial`.
    pub fn line_explained(&mut self, serial: u64, result: Result<String, String>) {
        if serial == self.line_tools.serial {
            self.line_tools.explanation = match result {
                Ok(text) => Answer::Done(text),
                Err(e) => Answer::Failed(e),
            };
        }
    }

    /// Shadowing ended.
    pub fn line_shadowed(
        &mut self,
        serial: u64,
        result: Result<crate::line_tools::Shadowed, String>,
    ) {
        if self.line_tools.shadow != Shadow::Recording {
            return;
        }
        // a recording of an older line is not that line's
        if serial != self.line_tools.serial {
            self.line_tools.shadow = Shadow::Idle;
            return;
        }
        self.line_tools.shadow = match result {
            Ok(done) => Shadow::Done(done),
            Err(e) => Shadow::Failed(e),
        };
    }

    /// Reads the speech of the video into subtitles (SUB-062).
    pub fn speech_generate(&mut self) {
        if self.speech.is_some() {
            return;
        }
        let Some((video, folder)) = self.current.as_ref().map(|c| {
            (
                c.path.clone(),
                c.subtitles.clone().unwrap_or_else(|| {
                    nikomiru_online::store::server_folder(
                        &self.services.subtitles_dir,
                        &c.path.to_string_lossy(),
                    )
                }),
            )
        }) else {
            return;
        };
        if let Some(c) = self.current.as_mut() {
            c.subtitles = Some(folder.clone());
        }
        let (input, stream) = self.audio_source(&video);
        self.speech = Some(None);
        self.toasts.push(self.tr("speech-started", &[]));
        self.line_jobs.push(LineJob::Generate(GenerateJob {
            video: input,
            stream,
            duration_ms: (self.view.duration * 1000.0) as i64,
            folder,
            config: Box::new(self.services.config.clone()),
        }));
    }

    /// Stops reading the speech.
    pub fn speech_cancel(&mut self) {
        if self.speech.is_some() {
            self.services.assist.cancel_generate();
        }
    }

    /// How far reading the speech is.
    pub fn speech_progress(&mut self, percent: u8) {
        if self.speech.is_some() {
            self.speech = Some(Some(percent));
        }
    }

    /// The subtitles read from the speech: listed, and shown.
    pub fn speech_done(&mut self, result: Result<PathBuf, String>) {
        self.speech = None;
        match result {
            Ok(path) => {
                self.toasts.push(self.tr("speech-done", &[]));
                self.discover_subtitles();
                if !self.sub_sources.contains(&Source::File(path.clone())) {
                    self.sub_sources.push(Source::File(path.clone()));
                }
                self.use_subtitles(Source::File(path));
            }
            Err(e) if e.is_empty() => {}
            Err(e) => self.toasts.push(e),
        }
    }

    /// The speech reading for the menu: its text and share done.
    pub fn speech_status(&self) -> Option<(String, Option<f32>)> {
        Some(match self.speech? {
            Some(percent) => (
                self.tr("speech-progress", &[("percent", percent.to_string())]),
                Some(f32::from(percent) / 100.0),
            ),
            None => (self.tr("speech-preparing", &[]), None),
        })
    }

    /// Whether the speech is being read.
    pub fn speech_running(&self) -> bool {
        self.speech.is_some()
    }

    /// Starts the jobs asked for (the UI layer, each frame).
    pub fn start_line_jobs(&mut self, waker: &std::sync::Arc<dyn crate::ui::Waker>) {
        for job in std::mem::take(&mut self.line_jobs) {
            let w = waker.clone();
            match job {
                LineJob::Generate(job) => {
                    let p = waker.clone();
                    self.services.assist.generate(
                        job,
                        Box::new(move |percent| {
                            p.wake(crate::ui::Inbound::SpeechProgress(percent))
                        }),
                        Box::new(move |r| w.wake(crate::ui::Inbound::SpeechDone(r))),
                    );
                }
                LineJob::Translate {
                    serial,
                    text,
                    target,
                    config,
                } => self.services.assist.translate(
                    text,
                    target,
                    config,
                    Box::new(move |r| w.wake(crate::ui::Inbound::LineTranslated(serial, r))),
                ),
                LineJob::Explain(serial, job) => self.services.assist.explain(
                    job,
                    Box::new(move |r| w.wake(crate::ui::Inbound::LineExplained(serial, r))),
                ),
                LineJob::Shadow(serial, job) => self.services.assist.shadow(
                    job,
                    Box::new(move |r| w.wake(crate::ui::Inbound::LineShadowed(serial, r))),
                ),
                LineJob::Frame(serial, area) => {
                    let name = format!("frame-{serial}.png");
                    let from_file = self.current.as_ref().map(|c| crate::screenshot::FromFile {
                        ffmpeg: self.services.config.paths.ffmpeg.clone(),
                        video: c.path.clone(),
                        at_ms: (self.view.time_pos * 1000.0) as i64,
                    });
                    crate::screenshot::start(
                        self.services.player.clone(),
                        self.services.mining_dir.join("frames"),
                        name,
                        from_file,
                        Box::new(move |r| w.wake(crate::ui::Inbound::LineFrame(serial, area, r))),
                    );
                }
            }
        }
    }
}
