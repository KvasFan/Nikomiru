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
    /// Word audio (AUD-002) by term: `/words/list` answers a Yomitan list of
    /// `/words/file/<term>`, `/words/direct` the audio itself.
    pub words: BTreeMap<String, Vec<u8>>,
    /// The AniList id of Jimaku entries (MET-003 with M7).
    pub entry_anilist: BTreeMap<u64, u64>,
    /// The series AniList and Jikan know (MET-001, MET-002).
    pub series: Vec<FakeSeries>,
    /// The bytes of every cover, at `/covers/<anilist id>.jpg`.
    pub cover: Vec<u8>,
}

/// A series of the fake AniList and Jikan.
#[derive(Debug, Clone, Default)]
pub struct FakeSeries {
    /// Its AniList id.
    pub anilist_id: u64,
    /// Its MyAnimeList id.
    pub mal_id: u64,
    /// Its titles.
    pub romaji: String,
    /// See `romaji`.
    pub english: Option<String>,
    /// See `romaji`.
    pub native: Option<String>,
    /// Its episodes.
    pub episodes: Option<u32>,
}

impl FakeSeries {
    fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        !q.is_empty()
            && std::iter::once(&self.romaji)
                .chain(self.english.as_ref())
                .chain(self.native.as_ref())
                .any(|t| t.to_lowercase().contains(&q) || q.contains(&t.to_lowercase()))
    }
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

    /// The AniList URL, for `[online] anilist_url`.
    pub fn anilist_url(&self) -> String {
        format!("http://127.0.0.1:{}/anilist", self.port)
    }

    /// The Jikan base URL, for `[online] jikan_url`.
    pub fn jikan_url(&self) -> String {
        format!("http://127.0.0.1:{}/jikan/v4", self.port)
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
    respond_as(stream, status, retry_after, "application/json", body);
}

fn respond_as(
    stream: &mut TcpStream,
    status: u16,
    retry_after: Option<u64>,
    content_type: &str,
    body: &[u8],
) {
    let retry = retry_after.map_or_else(String::new, |s| format!("Retry-After: {s}\r\n"));
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n",
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
    if request.target.contains("/words/") {
        let (status, content_type, body) = word(shared, &request, port);
        return respond_as(&mut stream, status, None, content_type, &body);
    }
    let (status, body) = answer(shared, &request, port);
    respond(&mut stream, status, None, &body);
}

/// `%XX` sequences decoded as UTF-8.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The word audio routes (AUD-002).
fn word(shared: &Shared, r: &Request, port: u16) -> (u16, &'static str, Vec<u8>) {
    let target = r.target.find("/words/").map_or("", |i| &r.target[i..]);
    let Ok(url) = Url::parse(&format!("http://fake{target}")) else {
        return (400, "text/plain", Vec::new());
    };
    let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    let term = query.get("term").cloned().unwrap_or_default();
    let d = lock(&shared.data);
    let segments: Vec<String> = url
        .path_segments()
        .map(|s| s.map(percent_decoded).collect())
        .unwrap_or_default();
    match segments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["words", "list"] => {
            let sources: Vec<Value> = d
                .words
                .contains_key(&term)
                .then(|| {
                    json!({
                        "name": "fake",
                        "url": format!("http://127.0.0.1:{port}/words/file/{}", crate::audio::encode_component(&term)),
                    })
                })
                .into_iter()
                .collect();
            let body = json!({"type": "audioSourceList", "audioSources": sources});
            (200, "application/json", json_body(&body))
        }
        ["words", "file", name] => match d.words.get(*name) {
            Some(bytes) => (200, "audio/mpeg", bytes.clone()),
            None => (404, "text/plain", Vec::new()),
        },
        ["words", "direct"] => match d.words.get(&term) {
            Some(bytes) => (200, "audio/mpeg", bytes.clone()),
            None => (404, "text/plain", Vec::new()),
        },
        _ => (404, "text/plain", Vec::new()),
    }
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
        ("GET", ["jimaku", "api", "entries", "search"]) if query.contains_key("anilist_id") => {
            let id: u64 = query["anilist_id"].parse().unwrap_or(0);
            let found: Vec<Value> = d
                .entries
                .iter()
                .filter(|(e, ..)| d.entry_anilist.get(e) == Some(&id))
                .map(|(e, name, a)| json!({"id": e, "name": name, "anilist_id": id, "flags": {"anime": a}}))
                .collect();
            (200, json_body(&Value::Array(found)))
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
        // AniList (MET-001)
        ("POST", ["anilist"]) => {
            let body: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            let search = body["variables"]["search"].as_str().unwrap_or_default();
            let media: Vec<Value> = d
                .series
                .iter()
                .filter(|s| s.matches(search))
                .map(|s| {
                    json!({
                        "id": s.anilist_id, "idMal": s.mal_id, "format": "TV",
                        "episodes": s.episodes, "seasonYear": 2023,
                        "title": {"romaji": s.romaji, "english": s.english, "native": s.native},
                        "synonyms": [],
                        "coverImage": {"large": format!("http://127.0.0.1:{port}/covers/{}.jpg", s.anilist_id)},
                        "siteUrl": format!("https://anilist.co/anime/{}", s.anilist_id),
                    })
                })
                .collect();
            (200, json_body(&json!({"data": {"Page": {"media": media}}})))
        }
        // Jikan (MET-002)
        ("GET", ["jikan", "v4", "anime"]) => {
            let q = query.get("q").cloned().unwrap_or_default();
            let data: Vec<Value> = d
                .series
                .iter()
                .filter(|s| s.matches(&q))
                .map(|s| {
                    json!({
                        "mal_id": s.mal_id, "title": s.romaji, "title_english": s.english,
                        "title_japanese": s.native, "episodes": s.episodes, "type": "TV",
                        "images": {"jpg": {"image_url": format!("http://127.0.0.1:{port}/covers/{}.jpg", s.anilist_id)}},
                        "url": format!("https://myanimelist.net/anime/{}", s.mal_id),
                    })
                })
                .collect();
            (200, json_body(&json!({"data": data})))
        }
        ("GET", ["covers", _]) => (200, d.cover.clone()),
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
