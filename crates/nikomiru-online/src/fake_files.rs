// SPDX-License-Identifier: GPL-3.0-only

//! A fake file host for the tests of the setup assistant (T-INS-002…
//! T-INS-006): files by path, answered whole or by range as GitHub's
//! release assets are, each request kept; and a pack made as Fushi makes
//! its own, a stored ZIP64 split into parts, with its manifest.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::fake::{Request, read_request};

#[derive(Default)]
struct Shared {
    files: Mutex<HashMap<String, Vec<u8>>>,
    requests: Mutex<Vec<Request>>,
    /// Statuses to answer instead, for the requests of a path.
    script: Mutex<VecDeque<(String, u16)>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The running fake; it stops when dropped.
pub struct FakeFiles {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeFiles")
            .field("port", &self.port)
            .finish()
    }
}

impl FakeFiles {
    /// Starts an empty host.
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("fake-files".into())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        let shared = shared.clone();
                        let _ = std::thread::Builder::new()
                            .name("fake-files-conn".into())
                            .spawn(move || serve(&shared, stream));
                    }
                })?
        };
        Ok(Self {
            port,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// The address of `path` (`/a/b`).
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Serves `bytes` at `path`.
    pub fn put(&self, path: &str, bytes: Vec<u8>) {
        lock(&self.shared.files).insert(path.to_string(), bytes);
    }

    /// The bytes at `path`, to change them.
    pub fn with_file(&self, path: &str, change: impl FnOnce(&mut Vec<u8>)) {
        if let Some(bytes) = lock(&self.shared.files).get_mut(path) {
            change(bytes);
        }
    }

    /// Answers `status` to the next request of `path`.
    pub fn then_status(&self, path: &str, status: u16) {
        lock(&self.shared.script).push_back((path.to_string(), status));
    }

    /// The requests so far: path and `Range` header.
    pub fn requests(&self) -> Vec<(String, Option<String>)> {
        lock(&self.shared.requests)
            .iter()
            .map(|r| (r.target.clone(), r.header("range").map(str::to_string)))
            .collect()
    }

    /// Forgets the requests so far.
    pub fn clear_requests(&self) {
        lock(&self.shared.requests).clear();
    }
}

impl Drop for FakeFiles {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, headers: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn serve(shared: &Shared, mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(request) = read_request(&mut stream) else {
        return respond(&mut stream, 400, "", b"");
    };
    lock(&shared.requests).push(request.clone());
    let path = request.target.clone();
    {
        let mut script = lock(&shared.script);
        if let Some(i) = script.iter().position(|(p, _)| *p == path)
            && let Some((_, status)) = script.remove(i)
        {
            return respond(&mut stream, status, "", b"");
        }
    }
    let Some(file) = lock(&shared.files).get(&path).cloned() else {
        return respond(&mut stream, 404, "", b"");
    };
    let range = request
        .header("range")
        .and_then(|r| r.strip_prefix("bytes="))
        .and_then(|r| r.split_once('-'))
        .and_then(|(a, b)| {
            Some((
                a.parse::<usize>().ok()?,
                if b.is_empty() {
                    usize::MAX
                } else {
                    b.parse::<usize>().ok()?
                },
            ))
        });
    match range {
        Some((from, last)) if from < file.len() && from <= last => {
            let last = last.min(file.len() - 1);
            let headers = format!("Content-Range: bytes {from}-{last}/{}\r\n", file.len());
            respond(&mut stream, 206, &headers, &file[from..=last]);
        }
        Some(_) => respond(&mut stream, 416, "", b""),
        None => respond(&mut stream, 200, "", &file),
    }
}

/// A stored ZIP64 of `files`, as Fushi writes its pack.
pub fn zip64(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in files {
        let header = out.len() as u64;
        let crc = crc32fast::hash(data);
        let name_len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&[45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&u32::MAX.to_le_bytes());
        directory.extend_from_slice(&u32::MAX.to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        directory.extend_from_slice(&28u16.to_le_bytes());
        // comment, disk, internal and external attributes
        directory.extend_from_slice(&[0; 10]);
        directory.extend_from_slice(&u32::MAX.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
        directory.extend_from_slice(&1u16.to_le_bytes());
        directory.extend_from_slice(&24u16.to_le_bytes());
        directory.extend_from_slice(&(data.len() as u64).to_le_bytes());
        directory.extend_from_slice(&(data.len() as u64).to_le_bytes());
        directory.extend_from_slice(&header.to_le_bytes());
    }
    let cd_offset = out.len() as u64;
    out.extend_from_slice(&directory);
    let record = out.len() as u64;
    out.extend_from_slice(b"PK\x06\x06");
    out.extend_from_slice(&44u64.to_le_bytes());
    out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&(files.len() as u64).to_le_bytes());
    out.extend_from_slice(&(files.len() as u64).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u64).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(b"PK\x06\x07");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&record.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]);
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

impl FakeFiles {
    /// Serves `pack` in parts of `part` bytes under `/parts/`, whole at
    /// `/whole.zip`, and its manifest at `/manifest.json`, whose address
    /// is returned.
    pub fn put_pack(&self, pack: &[u8], part: usize) -> String {
        let mut parts = Vec::new();
        for (i, piece) in pack.chunks(part.max(1)).enumerate() {
            let name = format!("pack.zip.{i:03}");
            parts.push(serde_json::json!({
                "name": name,
                "offset": i * part,
                "length": piece.len(),
            }));
            self.put(&format!("/parts/{name}"), piece.to_vec());
        }
        self.put("/whole.zip", pack.to_vec());
        let manifest = serde_json::json!({
            "version": "test",
            "url": self.url("/whole.zip"),
            "size_bytes": pack.len(),
            "part_base_urls": [self.url("/parts/")],
            "parts": parts,
        });
        self.put("/manifest.json", manifest.to_string().into_bytes());
        self.url("/manifest.json")
    }
}
