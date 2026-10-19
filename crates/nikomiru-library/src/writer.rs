// SPDX-License-Identifier: GPL-3.0-only

//! Writes to the library away from the UI thread. Every redb commit waits for
//! the disk (`fsync`), and the position is saved every second (LIB-006): on a
//! busy disk that can take seconds, which must never freeze the interface.
//! Writes run in order on one thread; [`Writer::wait`] is a barrier for the
//! rare reads that must see them (a file reopened at once).

use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::db::Library;

type Job = Box<dyn FnOnce(&Library) + Send>;

enum Message {
    Run(Job),
    Barrier(mpsc::Sender<()>),
}

/// The library's write thread. Dropping it writes what is queued.
pub struct Writer {
    library: Arc<Library>,
    tx: Option<mpsc::Sender<Message>>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer").finish_non_exhaustive()
    }
}

impl Writer {
    /// Starts the thread. If it cannot start, writes run on the caller's
    /// thread.
    pub fn start(library: Arc<Library>) -> Self {
        let (tx, rx) = mpsc::channel::<Message>();
        let lib = Arc::clone(&library);
        let spawned = std::thread::Builder::new()
            .name("nikomiru-library".into())
            .spawn(move || {
                for message in rx {
                    match message {
                        Message::Run(job) => job(&lib),
                        Message::Barrier(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            });
        let (tx, thread) = match spawned {
            Ok(thread) => (Some(tx), Some(thread)),
            Err(e) => {
                tracing::error!("cannot start the library thread: {e}");
                (None, None)
            }
        };
        Self {
            library,
            tx,
            thread,
        }
    }

    /// Queues a write.
    pub fn run(&self, job: impl FnOnce(&Library) + Send + 'static) {
        let job: Job = Box::new(job);
        let job = match &self.tx {
            Some(tx) => match tx.send(Message::Run(job)) {
                Ok(()) => return,
                Err(mpsc::SendError(Message::Run(job))) => job,
                Err(_) => return,
            },
            None => job,
        };
        job(&self.library);
    }

    /// Waits until every queued write is done.
    pub fn wait(&self) {
        if let Some(tx) = &self.tx {
            let (done, wait) = mpsc::channel();
            if tx.send(Message::Barrier(done)).is_ok() {
                let _ = wait.recv();
            }
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::VideoRecord;

    #[test]
    fn writes_run_in_order_and_are_done_after_wait_and_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Arc::new(Library::open(&dir.path().join("library.redb")).unwrap());
        let writer = Writer::start(Arc::clone(&lib));
        let record = VideoRecord {
            path: "/v/a.mkv".into(),
            ..VideoRecord::default()
        };
        writer.run(move |l| l.put_video("a", &record).unwrap());
        for ms in [1000, 2000, 3000] {
            writer.run(move |l| l.save_position("a", ms, 0).unwrap());
        }
        writer.wait();
        assert_eq!(lib.video("a").unwrap().unwrap().last_position_ms, 3000);
        writer.run(|l| l.save_position("a", 4000, 0).unwrap());
        drop(writer);
        assert_eq!(lib.video("a").unwrap().unwrap().last_position_ms, 4000);
    }
}
