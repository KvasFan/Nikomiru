// SPDX-License-Identifier: GPL-3.0-only

//! The line tools' worker (M121…M125): whisper.cpp for subtitles read from
//! the speech (SUB-062) and for shadowing (SHD-001), DeepL for translations
//! (TRN-001), Claude for explanations (AIX-001). Each job runs on a thread
//! of its own, only when asked for.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use nikomiru_core::{Cancel, Config};
use nikomiru_media::process::{JobError, run, run_lines};
use nikomiru_media::speech::{self, Reading};
use nikomiru_media::tools::{Search, find};
use nikomiru_online::http::Http;
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::line_tools::{AssistService, Done, ExplainJob, GenerateJob, ShadowJob, Shadowed};

/// The largest SRT whisper.cpp may write: a few hours of speech.
const MAX_SRT: u64 = 8 << 20;

/// The worker.
#[derive(Clone)]
pub struct Assist {
    translator: Translator,
    /// Where the setup assistant installs tools (INS-007).
    tools_dir: PathBuf,
    /// whisper.cpp's models (INS-008).
    speech_dir: PathBuf,
    /// Temporary WAV and SRT files.
    work_dir: PathBuf,
    generating: Arc<Mutex<Cancel>>,
    recording: Arc<Mutex<Cancel>>,
}

impl std::fmt::Debug for Assist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assist")
            .field("speech_dir", &self.speech_dir)
            .finish()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The name of whisper.cpp's program here.
fn whisper_name() -> String {
    format!("whisper-cli{}", std::env::consts::EXE_SUFFIX)
}

impl Assist {
    /// A worker installing nothing: the setup assistant does that.
    pub fn new(
        translator: Translator,
        tools_dir: PathBuf,
        speech_dir: PathBuf,
        work_dir: PathBuf,
    ) -> Self {
        Self {
            translator,
            tools_dir,
            speech_dir,
            work_dir,
            generating: Arc::new(Mutex::new(Cancel::new())),
            recording: Arc::new(Mutex::new(Cancel::new())),
        }
    }

    fn tr(&self, id: &str) -> String {
        self.translator.tr(id, &[])
    }

    /// whisper-cli: the setting (a file or its folder), the setup
    /// assistant's, then `PATH`.
    fn whisper(&self, setting: &str) -> Option<PathBuf> {
        let name = whisper_name();
        if !setting.trim().is_empty() {
            let p = PathBuf::from(setting.trim());
            let p = if p.is_dir() { p.join(&name) } else { p };
            return p.is_file().then_some(p);
        }
        let catalog = nikomiru_online::setup::tools::catalog();
        if let Some(tool) = catalog.iter().find(|t| t.id == "whisper") {
            let managed = self
                .tools_dir
                .join(&tool.id)
                .join(&tool.version)
                .join(nikomiru_online::setup::tools::platform())
                .join(&name);
            if managed.is_file() {
                return Some(managed);
            }
        }
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join(&name))
                .find(|p| p.is_file())
        })
    }

    /// How whisper.cpp reads, from the settings; why not when it cannot.
    fn reading(&self, config: &Config) -> Result<Reading, String> {
        let whisper = self
            .whisper(&config.paths.whisper)
            .ok_or_else(|| self.tr("speech-no-whisper"))?;
        let (model, vad) = if config.speech.model.trim().is_empty() {
            let model = nikomiru_online::setup::speech::model(&self.speech_dir)
                .ok_or_else(|| self.tr("speech-no-model"))?;
            (model, nikomiru_online::setup::speech::vad(&self.speech_dir))
        } else {
            let model = PathBuf::from(config.speech.model.trim());
            if !model.is_file() {
                return Err(self.tr("speech-no-model"));
            }
            (model, nikomiru_online::setup::speech::vad(&self.speech_dir))
        };
        Ok(Reading {
            whisper,
            model,
            vad,
            language: config.speech.language.clone(),
            threads: config.speech.threads as usize,
        })
    }

    fn ffmpeg(&self, config: &Config) -> Result<PathBuf, String> {
        find(&Search::from_env(&config.paths.ffmpeg))
            .map(|(_, ffmpeg, _)| ffmpeg)
            .map_err(|e| self.translator.error(&e))
    }

    fn job_error(&self, e: &JobError) -> String {
        self.translator.error(e)
    }

    /// A fresh file name in the work folder.
    fn scratch(&self, what: &str) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.work_dir).map_err(|e| e.to_string())?;
        Ok(self
            .work_dir
            .join(format!("{what}-{}", uuid::Uuid::new_v4())))
    }

    /// Reads `wav` with whisper.cpp: the SRT's text.
    fn read_wav(
        &self,
        reading: &Reading,
        wav: &Path,
        duration_ms: i64,
        cancel: &Cancel,
        progress: &(dyn Fn(u8) + Sync),
    ) -> Result<String, String> {
        let base = self.scratch("speech")?;
        let job = speech::whisper_job(reading, wav, &base, duration_ms);
        let result = run_lines(&job, cancel, &|line| {
            if let Some(p) = speech::progress(line) {
                progress(p);
            }
        });
        let srt = speech::srt_of(&base);
        let text = result.map_err(|e| self.job_error(&e)).and_then(|()| {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(&srt)
                .and_then(|f| f.take(MAX_SRT + 1).read_to_end(&mut bytes))
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_SRT {
                return Err(self.tr("speech-failed"));
            }
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        });
        let _ = std::fs::remove_file(&srt);
        text
    }

    fn generate_now(
        &self,
        job: &GenerateJob,
        cancel: &Cancel,
        progress: &(dyn Fn(u8) + Sync),
    ) -> Result<PathBuf, String> {
        let reading = self.reading(&job.config)?;
        let ffmpeg = self.ffmpeg(&job.config)?;
        let wav = self.scratch("audio")?.with_extension("wav");
        let audio = speech::audio_job(&ffmpeg, &job.video, job.stream, &wav, job.duration_ms);
        let read = run(&audio, cancel)
            .map_err(|e| self.job_error(&e))
            .and_then(|()| self.read_wav(&reading, &wav, job.duration_ms, cancel, progress));
        let _ = std::fs::remove_file(&wav);
        let text = read?;
        // a file of nothing but silence or music: no line was heard
        if !text.contains("-->") {
            return Err(self.tr("speech-nothing"));
        }
        let language = match job.config.speech.language.trim() {
            "" | "auto" => "whisper".to_string(),
            code => format!("whisper.{code}"),
        };
        nikomiru_online::store::save(
            &job.folder,
            &job.video,
            &language,
            "speech.srt",
            text.as_bytes(),
        )
        .map_err(|e| e.to_string())
    }

    fn shadow_now(&self, job: &ShadowJob, cancel: &Cancel) -> Result<Shadowed, String> {
        let ffmpeg = self.ffmpeg(&job.config)?;
        let input = match job.config.speech.microphone.trim() {
            "" => speech::default_input().to_string(),
            set => set.to_string(),
        };
        std::fs::create_dir_all(&job.dir).map_err(|e| e.to_string())?;
        let recording = job.dir.join("recording.wav");
        let record = speech::record_job(&ffmpeg, &input, job.ms, &recording)
            .ok_or_else(|| self.tr("shadow-no-microphone"))?;
        match run(&record, cancel) {
            Ok(()) => {}
            // stopped by the user: nothing to read
            Err(JobError::Cancelled { .. }) => return Err(self.tr("shadow-stopped")),
            Err(e) => return Err(self.tr("shadow-failed") + ": " + &self.job_error(&e)),
        }
        // with whisper.cpp, what was said is read and compared
        let Ok(reading) = self.reading(&job.config) else {
            return Ok(Shadowed {
                recording,
                said: None,
                score: None,
            });
        };
        let srt = self.read_wav(&reading, &recording, job.ms, &Cancel::new(), &|_| {})?;
        let track = nikomiru_subs::parse(nikomiru_subs::Format::Srt, &srt);
        let said: Vec<String> = track
            .cues
            .iter()
            .map(|c| c.plain_text.trim().to_string())
            .collect();
        let said = said.join(" ").trim().to_string();
        let score = speech::similarity(&job.line, &said);
        Ok(Shadowed {
            recording,
            said: Some(said),
            score: Some(score),
        })
    }

    fn http(&self, config: &Config, request: Duration) -> Result<Http, String> {
        Http::with_timeouts(&config.network.proxy, Duration::from_secs(10), request)
            .map_err(|e| self.translator.error(&e))
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(work) {
        tracing::error!("cannot start the {name} thread: {e}");
    }
}

impl AssistService for Assist {
    fn generate(
        &self,
        job: GenerateJob,
        progress: Box<dyn Fn(u8) + Send + Sync>,
        done: Done<PathBuf>,
    ) {
        let cancel = Cancel::new();
        *lock(&self.generating) = cancel.clone();
        let me = self.clone();
        spawn("speech", move || {
            let result = me.generate_now(&job, &cancel, &*progress);
            if let Err(e) = &result {
                tracing::warn!("speech of {}: {e}", job.video.display());
            }
            done(if cancel.is_cancelled() {
                Err(String::new())
            } else {
                result
            })
        });
    }

    fn cancel_generate(&self) {
        lock(&self.generating).cancel();
    }

    fn translate(&self, text: String, target: String, config: Box<Config>, done: Done<String>) {
        let me = self.clone();
        spawn("translate", move || {
            let result = me.http(&config, Duration::from_secs(30)).and_then(|http| {
                nikomiru_online::deepl::translate(
                    &http,
                    &config.online.deepl_key,
                    &config.online.deepl_address,
                    &text,
                    &target,
                )
                .map(|t| t.text)
                .map_err(|e| me.translator.error(&e))
            });
            done(result);
        });
    }

    fn explain(&self, job: ExplainJob, done: Done<String>) {
        let me = self.clone();
        spawn("explain", move || {
            let config = &job.config;
            // a thought-through answer can take a while
            let result = me.http(config, Duration::from_secs(180)).and_then(|http| {
                nikomiru_online::claude::explain(
                    &http,
                    &config.online.claude_key,
                    &config.online.claude_address,
                    &config.online.claude_model,
                    &job.ask,
                )
                .map_err(|e| me.translator.error(&e))
            });
            done(result);
        });
    }

    fn shadow(&self, job: ShadowJob, done: Done<Shadowed>) {
        let cancel = Cancel::new();
        *lock(&self.recording) = cancel.clone();
        let me = self.clone();
        spawn("shadow", move || done(me.shadow_now(&job, &cancel)));
    }

    fn stop_shadow(&self) {
        lock(&self.recording).cancel();
    }
}
