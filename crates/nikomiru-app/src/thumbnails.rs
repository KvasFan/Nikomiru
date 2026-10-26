// SPDX-License-Identifier: GPL-3.0-only

//! The thumbnails of the progress bar (PLY-014): a worker thread runs ffmpeg
//! for one frame at a time. A wish that comes while one is running replaces
//! the one waiting, which is told so: moving along the bar asks for many
//! frames, and only the last one matters.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;

use nikomiru_core::Cancel;
use nikomiru_media::clip;
use nikomiru_media::process::run;
use nikomiru_media::tools::{Search, Tools, probe};
use nikomiru_ui::dictionaries::Done;
use nikomiru_ui::thumbnails::{HEIGHT, ThumbnailService};

type Wish = (PathBuf, i64, Done<Vec<u8>>);

#[derive(Default)]
struct Slot {
    next: Option<Wish>,
    closing: bool,
}

/// The service.
pub struct ThumbnailWorker {
    slot: Arc<(Mutex<Slot>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ThumbnailWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailWorker").finish_non_exhaustive()
    }
}

impl ThumbnailWorker {
    /// Starts the worker; `ffmpeg` is the setting of EXT-001, `cache_dir`
    /// where the frames are written before they are read.
    pub fn start(ffmpeg: String, cache_dir: PathBuf) -> Self {
        let slot = Arc::new((Mutex::new(Slot::default()), Condvar::new()));
        let shared = Arc::clone(&slot);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-thumbnails".into())
            .spawn(move || {
                let mut tools: Option<Tools> = None;
                let mut n = 0u64;
                while let Some((video, at_ms, done)) = next(&shared) {
                    if tools.is_none() {
                        tools = probe(&Search::from_env(&ffmpeg))
                            .inspect_err(|e| tracing::warn!("ffmpeg for thumbnails: {e}"))
                            .ok();
                    }
                    let Some(t) = &tools else {
                        done(Err("no ffmpeg".into()));
                        continue;
                    };
                    n += 1;
                    done(frame(t, &cache_dir, n, &video, at_ms));
                }
            });
        let worker = spawned
            .inspect_err(|e| tracing::error!("cannot start the thumbnail thread: {e}"))
            .ok();
        Self { slot, worker }
    }
}

/// Waits for the next wish; `None` when closing.
fn next(slot: &(Mutex<Slot>, Condvar)) -> Option<Wish> {
    let (lock, ready) = slot;
    let mut s = lock.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        if s.closing {
            return None;
        }
        if let Some(wish) = s.next.take() {
            return Some(wish);
        }
        s = ready.wait(s).unwrap_or_else(PoisonError::into_inner);
    }
}

/// The frame of `video` at `at_ms`, as PNG bytes.
fn frame(
    tools: &Tools,
    cache_dir: &std::path::Path,
    n: u64,
    video: &std::path::Path,
    at_ms: i64,
) -> Result<Vec<u8>, String> {
    std::fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let out = cache_dir.join(format!("{n}.png"));
    let job = clip::thumbnail_job(&tools.ffmpeg, video, at_ms, HEIGHT, &out);
    run(&job, &Cancel::new()).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&out).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&out);
    bytes
}

impl ThumbnailService for ThumbnailWorker {
    fn frame(&self, video: PathBuf, at_ms: i64, done: Done<Vec<u8>>) {
        if self.worker.is_none() {
            done(Err("no thumbnail thread".into()));
            return;
        }
        let (lock, ready) = &*self.slot;
        let replaced = lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .next
            .replace((video, at_ms, done));
        ready.notify_one();
        if let Some((_, _, done)) = replaced {
            done(Err("replaced by a newer one".into()));
        }
    }
}

impl Drop for ThumbnailWorker {
    /// Stops the worker after the frame it is making.
    fn drop(&mut self) {
        let (lock, ready) = &*self.slot;
        let waiting = {
            let mut s = lock.lock().unwrap_or_else(PoisonError::into_inner);
            s.closing = true;
            s.next.take()
        };
        ready.notify_all();
        if let Some((_, _, done)) = waiting {
            done(Err("closing".into()));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // T-PLY-014
    #[test]
    fn a_newer_wish_replaces_the_one_waiting() {
        // no ffmpeg: each wish is answered with an error, but answered
        let dir = tempfile::tempdir().unwrap();
        let worker = ThumbnailWorker::start(
            dir.path().join("no-ffmpeg").to_string_lossy().into_owned(),
            dir.path().join("cache"),
        );
        let (tx, rx) = mpsc::channel();
        for at in [0, 2_000, 4_000] {
            let tx = tx.clone();
            worker.frame(
                "/v/a.mkv".into(),
                at,
                Box::new(move |r| tx.send((at, r.is_ok())).unwrap()),
            );
        }
        drop(worker);
        drop(tx);
        let answers: Vec<(i64, bool)> = rx.iter().collect();
        assert_eq!(answers.len(), 3, "every wish answered once: {answers:?}");
        assert!(answers.iter().all(|(_, ok)| !ok));
    }
}
