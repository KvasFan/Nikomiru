// SPDX-License-Identifier: GPL-3.0-only

//! Single instance (APP-001). The first process holds an exclusive lock on
//! `nikomiru.lock` and listens on `nikomiru.sock`, both in `$XDG_RUNTIME_DIR`. A
//! later launch finds the lock taken, sends its targets over the socket as one
//! JSON line, waits for the reply and exits.
//!
//! The lock, not the socket, decides who is first: a socket file left by a
//! crashed instance is simply replaced, and two launches at the same moment
//! cannot both become the first.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Longest accepted request line.
const MAX_REQUEST: u64 = 1 << 20;

/// How long a second launch waits for the first instance.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Errors of the single-instance mechanism.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    /// The lock file or the socket could not be used.
    #[error("{path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// Another instance holds the lock but did not answer.
    #[error("another Nikomiru is running but does not answer on {0}")]
    NoAnswer(PathBuf),
}

/// What a launch sends to the running instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Files (absolute paths) and URLs to open.
    pub open: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
}

/// The outcome of [`acquire`].
#[derive(Debug)]
pub enum Role {
    /// This is the first instance.
    Primary(Server),
    /// The request went to the running instance: this process can exit.
    Forwarded,
}

/// The first instance's socket, not yet serving.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    guard: Guard,
}

/// Keeps the lock while it lives; when dropped, removes the socket file before
/// releasing the lock.
#[derive(Debug)]
pub struct Guard {
    socket: PathBuf,
    _lock: File,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> InstanceError + '_ {
    move |source| InstanceError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Becomes the first instance, or hands `request` to the running one. While
/// the lock is taken but nobody listens (an instance starting or shutting down,
/// or a child process between `fork` and `exec` still holding the descriptor),
/// both the lock and the socket are retried until [`TIMEOUT`].
pub fn acquire(socket: &Path, request: &Request) -> Result<Role, InstanceError> {
    let lock_path = socket.with_extension("lock");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .map_err(io(&lock_path))?;
        match lock.try_lock() {
            Ok(()) => return primary(socket, lock),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => return Err(io(&lock_path)(e)),
        }
        drop(lock);
        match UnixStream::connect(socket) {
            Ok(stream) => {
                send(stream, request).map_err(|_| InstanceError::NoAnswer(socket.to_path_buf()))?;
                return Ok(Role::Forwarded);
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return Err(InstanceError::NoAnswer(socket.to_path_buf())),
        }
    }
}

/// Holds `lock`: replaces a socket left by a crashed instance and listens.
fn primary(socket: &Path, lock: File) -> Result<Role, InstanceError> {
    match std::fs::remove_file(socket) {
        Ok(()) => tracing::info!("removed the socket of a previous instance"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(socket)(e)),
    }
    let listener = UnixListener::bind(socket).map_err(io(socket))?;
    Ok(Role::Primary(Server {
        listener,
        guard: Guard {
            socket: socket.to_path_buf(),
            _lock: lock,
        },
    }))
}

/// Sends `request` on a connected stream and waits for the answer.
fn send(mut stream: UnixStream, request: &Request) -> std::io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut line = serde_json::to_string(request).unwrap_or_default();
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut answer = String::new();
    BufReader::new(stream.take(4096)).read_line(&mut answer)?;
    match serde_json::from_str::<Reply>(&answer) {
        Ok(Reply { ok: true }) => Ok(()),
        _ => Err(std::io::Error::other("refused")),
    }
}

impl Server {
    /// Serves the requests of later launches on a thread, calling `on_request`
    /// for each one. The returned guard keeps the lock and removes the socket
    /// when dropped.
    pub fn serve(self, on_request: impl Fn(Request) + Send + 'static) -> Guard {
        let listener = self.listener;
        let spawned = std::thread::Builder::new()
            .name("instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(s) => {
                            if let Some(request) = answer(s) {
                                on_request(request);
                            }
                        }
                        Err(e) => tracing::warn!("single-instance socket: {e}"),
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("single-instance thread: {e}");
        }
        self.guard
    }
}

/// Reads one request and answers it; `None` for anything malformed.
fn answer(stream: UnixStream) -> Option<Request> {
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let mut line = String::new();
    let read = BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line);
    let request = read
        .ok()
        .and_then(|_| serde_json::from_str::<Request>(&line).ok());
    if request.is_none() {
        tracing::warn!("single-instance socket: malformed request ignored");
    }
    let reply = serde_json::to_string(&Reply {
        ok: request.is_some(),
    })
    .unwrap_or_default();
    let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn request(targets: &[&str]) -> Request {
        Request {
            open: targets.iter().map(|t| t.to_string()).collect(),
        }
    }

    // The mechanism behind T-APP-001 (the GUI flow itself is an L6 test).
    #[test]
    fn a_second_launch_hands_its_targets_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nikomiru.sock");
        let Role::Primary(server) = acquire(&socket, &request(&[])).unwrap() else {
            panic!("the first launch must be primary");
        };
        let (tx, rx) = mpsc::channel();
        let guard = server.serve(move |r| tx.send(r).unwrap());
        let role = acquire(&socket, &request(&["/v/ep1.mkv"])).unwrap();
        assert!(matches!(role, Role::Forwarded));
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(got, request(&["/v/ep1.mkv"]));
        drop(guard);
        assert!(!socket.exists(), "the socket is removed at exit");
        // the lock is free again: the next launch is primary
        assert!(matches!(
            acquire(&socket, &request(&[])).unwrap(),
            Role::Primary(_)
        ));
    }

    #[test]
    fn a_launch_while_the_first_shuts_down_becomes_primary() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nikomiru.sock");
        // the first instance has removed its socket but still holds the lock
        let held = File::create(dir.path().join("nikomiru.lock")).unwrap();
        held.lock().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(held);
        });
        assert!(matches!(
            acquire(&socket, &request(&["/v/ep1.mkv"])).unwrap(),
            Role::Primary(_)
        ));
        release.join().unwrap();
    }

    #[test]
    fn a_stale_socket_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nikomiru.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        assert!(matches!(
            acquire(&socket, &request(&[])).unwrap(),
            Role::Primary(_)
        ));
    }

    #[test]
    fn malformed_requests_are_refused_and_serving_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nikomiru.sock");
        let Role::Primary(server) = acquire(&socket, &request(&[])).unwrap() else {
            panic!("primary expected");
        };
        let (tx, rx) = mpsc::channel();
        let _guard = server.serve(move |r| tx.send(r).unwrap());
        let mut s = UnixStream::connect(&socket).unwrap();
        s.write_all(b"not json\n").unwrap();
        let mut reply = String::new();
        BufReader::new(&s).read_line(&mut reply).unwrap();
        assert_eq!(reply.trim(), r#"{"ok":false}"#);
        assert!(matches!(
            acquire(&socket, &request(&["/v/ep2.mkv"])).unwrap(),
            Role::Forwarded
        ));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            request(&["/v/ep2.mkv"])
        );
    }
}
