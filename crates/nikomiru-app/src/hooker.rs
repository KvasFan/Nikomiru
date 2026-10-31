// SPDX-License-Identifier: GPL-3.0-only

//! The hooker worker (SPECIFICATION §6.15): a thread that lives only while
//! the Texthooker page is open, connected to the hooker's WebSocket, trying
//! again when it is not there, and reading the clipboard if asked.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nikomiru_core::Cancel;
use nikomiru_core::config::HookerConfig;
use nikomiru_media::process::{self, Job};
use nikomiru_online::hooker::{Hook, checked_hook_url, clean, is_japanese};
use nikomiru_ui::hooker::{HookerEvent, HookerService, Reports};

/// How often the hooker is tried.
const RETRY: Duration = Duration::from_secs(3);
/// How often the clipboard is read.
const CLIPBOARD_POLL: Duration = Duration::from_secs(1);
/// How long a program that reads it may take.
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a pause is when nothing is connected.
const IDLE: Duration = Duration::from_millis(250);

/// The program that reads the clipboard here, and its arguments (HOK-003).
fn clipboard_program() -> Option<(&'static str, &'static [&'static str])> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let (program, args): (&str, &[&str]) = if wayland {
        ("wl-paste", &["--no-newline", "--type", "text"])
    } else {
        ("xclip", &["-selection", "clipboard", "-o"])
    };
    let found = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|d| Path::new(&d).join(program).is_file())
    });
    found.then_some((program, args))
}

/// The service.
#[derive(Default)]
pub struct HookerWorker {
    /// What stops the reading going on.
    stop: Mutex<Option<Arc<AtomicBool>>>,
}

impl std::fmt::Debug for HookerWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookerWorker").finish_non_exhaustive()
    }
}

impl Drop for HookerWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

impl HookerService for HookerWorker {
    fn start(&self, config: HookerConfig, reports: Reports) {
        let stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut running) = self.stop.lock()
            && let Some(old) = running.replace(Arc::clone(&stop))
        {
            old.store(true, Ordering::SeqCst);
        }
        let spawned = std::thread::Builder::new()
            .name("nikomiru-hooker".into())
            .spawn(move || read(&config, &reports, &stop));
        if let Err(e) = spawned {
            tracing::error!("the hooker worker did not start: {e}");
        }
    }

    fn stop(&self) {
        if let Ok(mut running) = self.stop.lock()
            && let Some(stop) = running.take()
        {
            stop.store(true, Ordering::SeqCst);
        }
    }
}

fn read(config: &HookerConfig, reports: &Reports, stop: &AtomicBool) {
    let url = checked_hook_url(&config.url);
    if let Err(e) = &url {
        tracing::info!("hooker: {e}");
    }
    let clipboard = if config.clipboard {
        let program = clipboard_program();
        if program.is_none() {
            reports(HookerEvent::NoClipboard);
        }
        program
    } else {
        None
    };
    let mut hook: Option<Hook> = None;
    let mut last_try: Option<Instant> = None;
    let mut last_read: Option<Instant> = None;
    let mut last_text = None;
    while !stop.load(Ordering::SeqCst) {
        if let Some(connected) = &mut hook {
            match connected.read() {
                Ok(Some(line)) => reports(HookerEvent::Line(line)),
                Ok(None) => {}
                Err(e) => {
                    tracing::info!("hooker: {e}");
                    hook = None;
                    reports(HookerEvent::Connected(false));
                }
            }
        } else {
            if let Ok(url) = &url
                && last_try.is_none_or(|t| t.elapsed() >= RETRY)
            {
                last_try = Some(Instant::now());
                if let Ok(connected) = Hook::connect(url) {
                    hook = Some(connected);
                    reports(HookerEvent::Connected(true));
                }
            }
            std::thread::sleep(IDLE);
        }
        if let Some((program, args)) = clipboard
            && last_read.is_none_or(|t| t.elapsed() >= CLIPBOARD_POLL)
        {
            last_read = Some(Instant::now());
            let mut job = Job::new(program, args);
            job.timeout = CLIPBOARD_TIMEOUT;
            // what it holds goes nowhere but the page, and only if Japanese
            if let Ok(text) = process::output(&job, &Cancel::new())
                && last_text.as_ref() != Some(&text)
            {
                if is_japanese(&text)
                    && let Some(line) = clean(&text)
                {
                    reports(HookerEvent::Line(line));
                }
                last_text = Some(text);
            }
        }
    }
}
