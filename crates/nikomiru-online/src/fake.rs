// SPDX-License-Identifier: GPL-3.0-only

//! A fake Jimaku and OpenSubtitles for the tests (feature `test-support`,
//! ADR 0008 §14): an HTTP server on `127.0.0.1` that answers both providers'
//! endpoints from the data the test puts in, checks their keys, and records
//! every request.

use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};

/// A subtitle of the fake OpenSubtitles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OsSubtitle {
    /// Its file id.
    pub file_id: u64,
    /// Its file name.
    pub file_name: String,
    /// Its language.
    pub language: String,
    /// The hash of the video it was made for.
    pub moviehash: Option<String>,
    /// The title searched by `query`.
    pub title: String,
    /// Its season.
    pub season: Option<u32>,
    /// Its episode.
    pub episode: Option<u32>,
    /// Machine translated.
    pub ai_translated: bool,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

/// What the fake has.
#[derive(Debug, Clone, Default)]
pub struct Data {
    /// The Jimaku key it accepts.
    pub jimaku_key: String,
    /// Jimaku entries: id, name, anime.
    pub entries: Vec<(u64, String, bool)>,
    /// Jimaku files by entry: name and bytes.
    pub files: BTreeMap<u64, Vec<(String, Vec<u8>)>>,
    /// The OpenSubtitles key it accepts.
    pub os_key: String,
    /// The OpenSubtitles login it requires for downloads, if any.
    pub os_login: Option<(String, String)>,
    /// OpenSubtitles subtitles.
    pub os_subtitles: Vec<OsSubtitle>,
    /// OpenSubtitles downloads left.
    pub remaining: i64,
}

/// A request the fake received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method.
    pub method: String,
    /// The path with its query.
    pub target: String,
    /// The headers, names in lower case.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: String,
}

impl Request {
    /// A header's value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Default)]
struct Shared {
    data: Mutex<Data>,
    requests: Mutex<Vec<Request>>,
    /// Statuses to answer instead, with `Retry-After`.
    script: Mutex<VecDeque<(u16, Option<u64>)>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The running fake; it stops when dropped.
pub struct FakeOnline {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeOnline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeOnline")
            .field("port", &self.port)
            .finish()
    }
}

impl FakeOnline {
    /// Starts a fake with the keys `jimaku-key` and `os-key` and 10 downloads.
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared::default());
        {
            let mut d = lock(&shared.data);
            d.jimaku_key = "jimaku-key".into();
            d.os_key = "os-key".into();
            d.remaining = 10;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("fake-online".into())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        let shared = shared.clone();
                        let _ = std::thread::Builder::new()
                            .name("fake-online-conn".into())
                            .spawn(move || serve(&shared, stream, port));
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

    /// The Jimaku base URL, for `[online] jimaku_url`.
    pub fn jimaku_url(&self) -> String {
        format!("http://127.0.0.1:{}/jimaku/api", self.port)
    }

    /// The OpenSubtitles base URL, for `[online] opensubtitles_url`.
    pub fn opensubtitles_url(&self) -> String {
        format!("http://127.0.0.1:{}/os/api/v1", self.port)
    }

    /// Its own address, to use it as an HTTP proxy: a request in absolute
    /// form (`GET http://host/path`) is answered like one for `/path`.
    pub fn proxy_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The data, to prepare or to look at.
    pub fn data(&self) -> MutexGuard<'_, Data> {
        lock(&self.shared.data)
    }

    /// Answers the next request with `status` (and `Retry-After`).
    pub fn then_status(&self, status: u16, retry_after: Option<u64>) {
        lock(&self.shared.script).push_back((status, retry_after));
    }

    /// The requests so far.
    pub fn requests(&self) -> Vec<Request> {
        lock(&self.shared.requests).clone()
    }

    /// Forgets the requests so far.
    pub fn clear_requests(&self) {
        lock(&self.shared.requests).clear();
    }
}

impl Drop for FakeOnline {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buffer.len() > 64 * 1024 {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let target = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_lowercase(), v.trim().to_string()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buffer[head_end..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    Some(Request {
        method,
        target,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn respond(stream: &mut TcpStream, status: u16, retry_after: Option<u64>, body: &[u8]) {
    let retry = retry_after.map_or_else(String::new, |s| format!("Retry-After: {s}\r\n"));
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn serve(shared: &Shared, mut stream: TcpStream, port: u16) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Some(request) = read_request(&mut stream) else {
        return respond(&mut stream, 400, None, b"");
    };
    lock(&shared.requests).push(request.clone());
    if let Some((status, retry)) = lock(&shared.script).pop_front() {
        return respond(&mut stream, status, retry, b"{}");
    }
    let (status, body) = answer(shared, &request, port);
    respond(&mut stream, status, None, &body);
}

fn json_body(v: &Value) -> Vec<u8> {
    v.to_string().into_bytes()
}

fn answer(shared: &Shared, r: &Request, port: u16) -> (u16, Vec<u8>) {
    // through a proxy the target is a whole URL
    let target = match r.target.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/", |i| &rest[i..]),
        None => r.target.as_str(),
    };
    let Ok(url) = Url::parse(&format!("http://fake{target}")) else {
        return (400, Vec::new());
    };
    let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    let segments: Vec<&str> = url.path().trim_matches('/').split('/').collect();
    let mut d = lock(&shared.data);
    match (r.method.as_str(), segments.as_slice()) {
        // Jimaku
        (_, ["jimaku", ..]) if r.header("authorization") != Some(d.jimaku_key.as_str()) => {
            (401, json_body(&json!({"error": "unauthorized"})))
        }
        ("GET", ["jimaku", "api", "entries", "search"]) => {
            let q = query
                .get("query")
                .cloned()
                .unwrap_or_default()
                .to_lowercase();
            let anime = query.get("anime").is_none_or(|a| a == "true");
            let found: Vec<Value> = d
                .entries
                .iter()
                .filter(|(_, name, a)| {
                    *a == anime
                        && (name.to_lowercase().contains(&q) || q.contains(&name.to_lowercase()))
                })
                .map(|(id, name, a)| json!({"id": id, "name": name, "flags": {"anime": a}}))
                .collect();
            (200, json_body(&Value::Array(found)))
        }
        ("GET", ["jimaku", "api", "entries", id, "files"]) => {
            let id: u64 = id.parse().unwrap_or(0);
            let files: Vec<Value> = d
                .files
                .get(&id)
                .map(|f| {
                    f.iter()
                        .enumerate()
                        .map(|(i, (name, bytes))| {
                            json!({
                                "name": name,
                                "size": bytes.len(),
                                "url": format!("http://127.0.0.1:{port}/jimaku/files/{id}/{i}"),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            (200, json_body(&Value::Array(files)))
        }
        ("GET", ["jimaku", "files", id, i]) => {
            let (id, i): (u64, usize) = (id.parse().unwrap_or(0), i.parse().unwrap_or(usize::MAX));
            match d.files.get(&id).and_then(|f| f.get(i)) {
                Some((_, bytes)) => (200, bytes.clone()),
                None => (404, Vec::new()),
            }
        }
        // OpenSubtitles: the temporary files need no key
        ("GET", ["os", "files", id]) => {
            let id: u64 = id.parse().unwrap_or(0);
            match d.os_subtitles.iter().find(|s| s.file_id == id) {
                Some(s) => (200, s.bytes.clone()),
                None => (404, Vec::new()),
            }
        }
        (_, ["os", ..]) if r.header("api-key") != Some(d.os_key.as_str()) => {
            (403, json_body(&json!({"message": "invalid key"})))
        }
        ("POST", ["os", "api", "v1", "login"]) => {
            let body: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            let given = (
                body["username"].as_str().unwrap_or_default().to_string(),
                body["password"].as_str().unwrap_or_default().to_string(),
            );
            if d.os_login.as_ref() == Some(&given) {
                (200, json_body(&json!({"token": "fake-token", "user": {}})))
            } else {
                (401, json_body(&json!({"message": "wrong login"})))
            }
        }
        ("GET", ["os", "api", "v1", "subtitles"]) => {
            let language = query.get("languages").cloned().unwrap_or_default();
            let hash = query.get("moviehash");
            let title = query.get("query").map(|q| q.to_lowercase());
            let num = |k: &str| query.get(k).and_then(|v| v.parse::<u32>().ok());
            let (season, episode) = (num("season_number"), num("episode_number"));
            let data: Vec<Value> = d
                .os_subtitles
                .iter()
                .filter(|s| language.is_empty() || s.language == language)
                .filter(|s| match (hash, &title) {
                    (Some(h), _) => s.moviehash.as_ref() == Some(h),
                    (None, Some(t)) => {
                        s.title.to_lowercase().contains(t.as_str())
                            && season.is_none_or(|n| s.season == Some(n))
                            && episode.is_none_or(|n| s.episode == Some(n))
                    }
                    _ => false,
                })
                .map(|s| {
                    json!({"attributes": {
                        "language": s.language, "release": s.title, "ai_translated": s.ai_translated,
                        "feature_details": {"season_number": s.season, "episode_number": s.episode},
                        "files": [{"file_id": s.file_id, "file_name": s.file_name}]
                    }})
                })
                .collect();
            (200, json_body(&json!({"data": data})))
        }
        ("POST", ["os", "api", "v1", "download"]) => {
            if d.os_login.is_some() && r.header("authorization") != Some("Bearer fake-token") {
                return (401, json_body(&json!({"message": "log in"})));
            }
            if d.remaining <= 0 {
                return (406, json_body(&json!({"message": "quota"})));
            }
            let body: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            let id = body["file_id"].as_u64().unwrap_or(0);
            if !d.os_subtitles.iter().any(|s| s.file_id == id) {
                return (404, Vec::new());
            }
            d.remaining -= 1;
            (
                200,
                json_body(&json!({
                    "link": format!("http://127.0.0.1:{port}/os/files/{id}"),
                    "remaining": d.remaining,
                })),
            )
        }
        _ => (404, Vec::new()),
    }
}
