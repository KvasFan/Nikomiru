// SPDX-License-Identifier: GPL-3.0-only

//! Single instance (APP-001) on Windows (ADR 0009): a named mutex in place of
//! the lock file, a named pipe in place of the Unix socket, with the same JSON
//! line and reply. The mutex, not the pipe, decides who is first, as the lock
//! does on Linux: Windows releases it when the process ends, even in a crash.
//! A later launch finds it taken, connects to the pipe, sends its targets,
//! waits for the reply and exits.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
    GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::CreateMutexW;

use crate::platform::wide;

/// Longest accepted request line.
const MAX_REQUEST: u64 = 1 << 20;

/// How long a second launch waits for the first instance.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Errors of the single-instance mechanism.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    /// The pipe could not be used.
    #[error("{path}: {source}")]
    Io {
        /// The pipe involved.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// Another instance holds the pipe but did not answer.
    #[error("another Nikomiru is running but does not answer on {0}")]
    NoAnswer(PathBuf),
}

impl nikomiru_core::Localized for InstanceError {
    fn message_id(&self) -> &'static str {
        match self {
            InstanceError::Io { .. } => "error-instance-io",
            InstanceError::NoAnswer(_) => "error-instance-no-answer",
        }
    }

    fn message_args(&self) -> Vec<(&'static str, String)> {
        match self {
            InstanceError::Io { path, source } => vec![
                ("path", path.display().to_string()),
                ("reason", source.to_string()),
            ],
            InstanceError::NoAnswer(_) => Vec::new(),
        }
    }
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

/// The first instance's pipe, not yet serving, and its mutex.
#[derive(Debug)]
pub struct Server {
    name: Vec<u16>,
    first: File,
    lock: OwnedHandle,
}

/// Holds the mutex while it lives: dropped, another launch can be first.
#[derive(Debug)]
pub struct Guard {
    _lock: OwnedHandle,
}

/// The name of this user's Nikomiru: after `socket`, a path in the user's
/// own folder, so that each user has one.
fn tag(socket: &Path) -> String {
    let path = socket.to_string_lossy().to_lowercase();
    let digest = Sha256::digest(path.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn pipe_name(socket: &Path) -> String {
    format!(r"\\.\pipe\nikomiru-{}", tag(socket))
}

/// The mutex that says an instance runs; `None` when another holds it.
fn lock(socket: &Path) -> std::io::Result<Option<OwnedHandle>> {
    let name = wide(&format!(r"Local\nikomiru-{}", tag(socket)));
    // SAFETY: `name` is NUL-terminated; default security, not owned.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let exists = last_error() == ERROR_ALREADY_EXISTS;
    // SAFETY: a valid handle that nothing else owns.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    Ok((!exists).then_some(handle))
}

fn last_error() -> u32 {
    // SAFETY: reads this thread's last error, nothing else.
    unsafe { GetLastError() }
}

/// A new instance of the pipe.
fn create(name: &[u16]) -> std::io::Result<File> {
    // SAFETY: `name` is NUL-terminated; no security attributes means the
    // default ones (full access for this user, read only for others).
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a valid handle that nothing else owns.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// Opens the running instance's pipe.
fn connect(name: &[u16]) -> Result<File, u32> {
    // SAFETY: `name` is NUL-terminated; plain read and write access.
    let handle: HANDLE = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    // SAFETY: a valid handle that nothing else owns.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// Becomes the first instance, or hands `request` to the running one. While
/// the mutex is held but the pipe has no free instance (the first instance
/// starting, or between two launches), the launch waits, up to [`TIMEOUT`].
pub fn acquire(socket: &Path, request: &Request) -> Result<Role, InstanceError> {
    let shown = PathBuf::from(pipe_name(socket));
    let name = wide(&pipe_name(socket));
    let deadline = Instant::now() + TIMEOUT;
    let io = |source| InstanceError::Io {
        path: shown.clone(),
        source,
    };
    loop {
        if let Some(lock) = lock(socket).map_err(io)? {
            let first = create(&name).map_err(io)?;
            return Ok(Role::Primary(Server { name, first, lock }));
        }
        match connect(&name) {
            Ok(pipe) => {
                send(pipe, request).map_err(|_| InstanceError::NoAnswer(shown.clone()))?;
                return Ok(Role::Forwarded);
            }
            Err(ERROR_PIPE_BUSY) => {
                // SAFETY: `name` is NUL-terminated.
                unsafe { WaitNamedPipeW(name.as_ptr(), 200) };
            }
            Err(ERROR_FILE_NOT_FOUND) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(InstanceError::NoAnswer(shown));
        }
    }
}

fn send(mut pipe: File, request: &Request) -> std::io::Result<()> {
    let mut line = serde_json::to_string(request).unwrap_or_default();
    line.push('\n');
    pipe.write_all(line.as_bytes())?;
    let mut answer = String::new();
    BufReader::new(pipe.take(4096)).read_line(&mut answer)?;
    match serde_json::from_str::<Reply>(&answer) {
        Ok(Reply { ok: true }) => Ok(()),
        _ => Err(std::io::Error::other("refused")),
    }
}

impl Server {
    /// Answers each later launch on a thread of its own, calling `on_request`
    /// with what it asked.
    pub fn serve(self, on_request: impl Fn(Request) + Send + 'static) -> Guard {
        let Server { name, first, lock } = self;
        let spawned = std::thread::Builder::new()
            .name("instance".into())
            .spawn(move || {
                let mut current = first;
                loop {
                    use std::os::windows::io::AsRawHandle;
                    // SAFETY: `current` is a pipe instance this thread owns.
                    let connected =
                        unsafe { ConnectNamedPipe(current.as_raw_handle(), std::ptr::null_mut()) };
                    if connected == 0 && last_error() != ERROR_PIPE_CONNECTED {
                        tracing::warn!("single-instance pipe: {}", std::io::Error::last_os_error());
                        return;
                    }
                    // the next instance before this one closes: a launch in
                    // between never finds the pipe free to take
                    let next = match create(&name) {
                        Ok(next) => next,
                        Err(e) => {
                            tracing::warn!("single-instance pipe: {e}");
                            return;
                        }
                    };
                    if let Some(request) = answer(&current) {
                        on_request(request);
                    }
                    current = next;
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("single-instance thread: {e}");
        }
        Guard { _lock: lock }
    }
}

fn answer(mut pipe: &File) -> Option<Request> {
    let mut line = String::new();
    let read = BufReader::new(pipe.take(MAX_REQUEST)).read_line(&mut line);
    let request = read
        .ok()
        .and_then(|_| serde_json::from_str::<Request>(&line).ok());
    if request.is_none() {
        tracing::warn!("single-instance pipe: malformed request ignored");
    }
    let reply = serde_json::to_string(&Reply {
        ok: request.is_some(),
    })
    .unwrap_or_default();
    let _ = pipe.write_all(format!("{reply}\n").as_bytes());
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_second_launch_hands_its_targets_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("nikomiru.sock");
        let Role::Primary(server) = acquire(&socket, &Request::default()).unwrap() else {
            panic!("the first launch must be primary");
        };
        let (tx, rx) = mpsc::channel();
        let guard = server.serve(move |r| tx.send(r).unwrap());
        let request = Request {
            open: vec![r"C:\v\ep1.mkv".into()],
        };
        assert!(matches!(
            acquire(&socket, &request).unwrap(),
            Role::Forwarded
        ));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), request);
        // the mutex released, the next launch is first again
        drop(guard);
        assert!(matches!(
            acquire(&socket, &Request::default()).unwrap(),
            Role::Primary(_)
        ));
    }
}
