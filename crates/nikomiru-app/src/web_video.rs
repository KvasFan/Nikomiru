// SPDX-License-Identifier: GPL-3.0-only

//! The cancellable yt-dlp worker and HTTP media relays (PLY-023). No shell,
//! user configuration, plugins, or network request is used at startup.

use nikomiru_core::Cancel;
use nikomiru_media::process::{Job, output};
use nikomiru_online::{http::checked_url, servers::relay::Relay};
use nikomiru_ui::{
    i18n::Translator,
    web_video::{Ready, Request, WebVideoService},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::Duration,
};

type Done = Box<dyn FnOnce(Result<Ready, String>) + Send>;
struct Work {
    request: Request,
    done: Done,
}

/// Serial background worker. A new request cancels an older one.
pub struct Worker {
    jobs: Option<mpsc::Sender<Work>>,
    thread: Option<JoinHandle<()>>,
    active: Mutex<Option<Cancel>>,
    unavailable: String,
}
impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebVideoWorker").finish_non_exhaustive()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(active) = self.active.lock()
            && let Some(cancel) = &*active
        {
            cancel.cancel();
        }
        drop(self.jobs.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Worker {
    /// Create a worker without discovering programs or contacting a site.
    pub fn start(tr: Translator) -> Self {
        let unavailable = tr.tr("webvideo-unavailable", &[]);
        let (jobs, rx) = mpsc::channel::<Work>();
        let thread = std::thread::Builder::new()
            .name("web-video".into())
            .spawn(move || {
                for work in rx {
                    (work.done)(resolve(&work.request, &tr));
                }
            });
        match thread {
            Ok(thread) => Self {
                jobs: Some(jobs),
                thread: Some(thread),
                active: Mutex::new(None),
                unavailable,
            },
            Err(_) => Self {
                jobs: None,
                thread: None,
                active: Mutex::new(None),
                unavailable,
            },
        }
    }
}
impl WebVideoService for Worker {
    fn open(&self, request: Request, done: Done) {
        if let Ok(mut active) = self.active.lock()
            && let Some(old) = active.replace(request.cancel.clone())
        {
            old.cancel();
        }
        let work = Work { request, done };
        let failed = match &self.jobs {
            Some(jobs) => jobs.send(work).err().map(|e| e.0),
            None => Some(work),
        };
        if let Some(work) = failed {
            (work.done)(Err(self.unavailable.clone()));
        }
    }
}

fn executable(setting: &str) -> Option<PathBuf> {
    let name = format!("yt-dlp{}", std::env::consts::EXE_SUFFIX);
    let chosen = if !setting.trim().is_empty() {
        Some(setting.trim().to_string())
    } else {
        std::env::var("NIKOMIRU_YT_DLP")
            .ok()
            .filter(|s| !s.is_empty())
    };
    if let Some(chosen) = chosen {
        let p = PathBuf::from(chosen);
        let p = if p.is_dir() { p.join(&name) } else { p };
        return p.is_file().then_some(p);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(&name))
            .find(|p| p.is_file())
    })
}

/// Build a fixed argument vector, with source data always passed as values.
pub fn job(program: &Path, r: &Request) -> Job {
    let languages: Vec<_> = r
        .languages
        .iter()
        .map(|l| nikomiru_subs::select::language_code(l))
        .filter(|l| {
            !l.is_empty() && l.len() <= 12 && l.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        })
        .map(|l| format!("{l}.*"))
        .collect();
    let languages = if languages.is_empty() {
        "ja.*".into()
    } else {
        languages.join(",")
    };
    let mut job = Job::new(
        program,
        [
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-remote-components",
            "--no-playlist",
            "--no-progress",
            "--no-warnings",
            "--ignore-errors",
            "--socket-timeout",
            "15",
            "--retries",
            "2",
            "--skip-download",
            "--no-simulate",
            "--write-subs",
            "--write-auto-subs",
            "--sub-format",
            "vtt/srt/ass",
            "--sub-langs",
            &languages,
            // Only the selected streams: a full yt-dlp infodict includes every
            // available format and caption and routinely exceeds the runner's
            // 64 KiB stdout limit, even for a two-minute video.
            "--print",
            "%(.{title,url,protocol,requested_formats,http_headers,is_live,has_drm})j",
            "--playlist-end",
            "1",
            "--proxy",
            &r.proxy,
            "--format",
            "bestvideo[protocol=https]+bestaudio[protocol=https]/bestvideo[protocol=http]+bestaudio[protocol=http]/best[protocol=https]/best[protocol=http]",
            "--restrict-filenames",
            "--output",
        ],
    );
    job.args.push(
        r.folder
            .join("subtitle.%(language)s.%(ext)s")
            .into_os_string(),
    );
    job.args.push("--".into());
    job.args.push(r.page.clone().into());
    job.timeout = Duration::from_secs(120);
    job
}

/// Resolve one explicit page. Only completed subtitle files enter the result;
/// signed media addresses and headers stay inside their local relays.
pub fn resolve(r: &Request, tr: &Translator) -> Result<Ready, String> {
    let failed = || tr.tr("webvideo-failed", &[]);
    checked_url(&r.page).map_err(|_| tr.tr("webvideo-invalid", &[]))?;
    if r.cancel.is_cancelled() {
        return Err(tr.tr("webvideo-cancelled", &[]));
    }
    let program = executable(&r.executable).ok_or_else(|| tr.tr("webvideo-missing", &[]))?;
    std::fs::create_dir_all(&r.folder).map_err(|_| failed())?;
    let json = output(&job(&program, r), &r.cancel).map_err(|_| {
        if r.cancel.is_cancelled() {
            tr.tr("webvideo-cancelled", &[])
        } else {
            failed()
        }
    })?;
    if r.cancel.is_cancelled() {
        return Err(tr.tr("webvideo-cancelled", &[]));
    }
    let metadata =
        nikomiru_online::web_video::parse(&json).map_err(|_| tr.tr("webvideo-stream", &[]))?;
    let video = Relay::start(metadata.video, &r.proxy).map_err(|_| failed())?;
    let audio = metadata
        .audio
        .map(|s| Relay::start(s, &r.proxy))
        .transpose()
        .map_err(|_| failed())?;
    let audio_url = audio.as_ref().map(Relay::url);
    let video_url = video.url();
    let mut relays = vec![video];
    if let Some(audio) = audio {
        relays.push(audio);
    }
    let mut subtitles: Vec<_> = std::fs::read_dir(&r.folder)
        .map_err(|_| failed())?
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_name()?.to_str()?;
            (name.starts_with("subtitle.")
                && e.file_type().is_ok_and(|t| t.is_file())
                && e.metadata().is_ok_and(|m| m.len() <= 64 << 20)
                && p.extension()
                    .is_some_and(|e| matches!(e.to_str(), Some("vtt" | "srt" | "ass"))))
            .then_some(p)
        })
        .collect();
    subtitles.sort();
    Ok(Ready {
        page: r.page.clone(),
        title: metadata.title,
        uid: nikomiru_online::web_video::uid(&r.page),
        video: video_url,
        audio: audio_url,
        subtitles,
        relays: Arc::new(relays),
    })
}
