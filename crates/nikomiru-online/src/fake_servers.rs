// SPDX-License-Identifier: GPL-3.0-only

//! A fake Jellyfin, Plex and plex.tv for the tests (feature `test-support`,
//! M11): an HTTP server on `127.0.0.1` that answers from the data the test
//! puts in, checks tokens, serves one file with `Range`, and records every
//! request.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use serde_json::{Value, json};

use crate::fake::{Request, read_request};

/// The token every fake server takes.
pub const TOKEN: &str = "fake-token-3f1c";
/// The fake Jellyfin's user.
pub const USER: &str = "kana";
/// The fake Jellyfin's password.
pub const PASSWORD: &str = "pass word";

/// A video, series or season of the fake servers.
#[derive(Debug, Clone, Default)]
pub struct FakeItem {
    /// Its id.
    pub id: String,
    /// `Series`, `Season`, `Episode` or `Movie`.
    pub kind: String,
    /// Its title.
    pub title: String,
    /// The library or item it is in.
    pub parent: String,
    /// Its number in its parent.
    pub index: u32,
    /// Its position, in milliseconds.
    pub position_ms: u64,
    /// When it was last played, in seconds since the epoch.
    pub last_played_s: Option<i64>,
}

/// What the fakes have.
#[derive(Debug, Clone, Default)]
pub struct Data {
    /// Libraries: id and name.
    pub libraries: Vec<(String, String)>,
    /// The items.
    pub items: Vec<FakeItem>,
    /// The file every playable item is.
    pub file: Vec<u8>,
    /// The bytes of its external subtitle.
    pub subtitle: Vec<u8>,
    /// Polls of a PIN before its token comes.
    pub pin_polls: usize,
    /// When set, the file answers a redirect to this address.
    pub redirect_to: Option<String>,
    /// The files of the fake WebDAV, by name.
    pub dav_files: std::collections::BTreeMap<String, Vec<u8>>,
    /// The password of the fake WebDAV, Basic-encoded for `dav`:`pass`.
    pub dav_full: bool,
}

struct Shared {
    data: Mutex<Data>,
    requests: Mutex<Vec<Request>>,
    polls: AtomicUsize,
    open_files: AtomicUsize,
}

/// The server.
pub struct FakeServers {
    port: u16,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeServers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeServers")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FakeServers {
    /// Starts it on a free port.
    pub fn start(data: Data) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared {
            data: Mutex::new(data),
            requests: Mutex::new(Vec::new()),
            polls: AtomicUsize::new(0),
            open_files: AtomicUsize::new(0),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (serving, stopping) = (Arc::clone(&shared), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(mut connection) = connection else {
                    continue;
                };
                let shared = Arc::clone(&serving);
                std::thread::spawn(move || handle(&mut connection, &shared));
            }
        });
        Ok(Self {
            port,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    /// The fake Jellyfin's address.
    pub fn jellyfin_url(&self) -> String {
        format!("http://127.0.0.1:{}/jf/", self.port)
    }

    /// The fake Plex's address.
    pub fn plex_url(&self) -> String {
        format!("http://127.0.0.1:{}/px/", self.port)
    }

    /// The fake WebDAV's address.
    pub fn webdav_url(&self) -> String {
        format!("http://127.0.0.1:{}/dav/", self.port)
    }

    /// The fake plex.tv's address.
    pub fn plex_tv_url(&self) -> String {
        format!("http://127.0.0.1:{}/tv/", self.port)
    }

    /// The data, to prepare or to look at.
    pub fn data(&self) -> MutexGuard<'_, Data> {
        lock(&self.shared.data)
    }

    /// The requests so far.
    pub fn requests(&self) -> Vec<Request> {
        lock(&self.shared.requests).clone()
    }

    /// The file's answers still being sent.
    pub fn open_files(&self) -> usize {
        self.shared.open_files.load(Ordering::SeqCst)
    }
}

impl Drop for FakeServers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, extra: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status} Fake\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn send_json(stream: &mut TcpStream, value: &Value) {
    respond(
        stream,
        200,
        "Content-Type: application/json\r\n",
        value.to_string().as_bytes(),
    );
}

fn query(target: &str, name: &str) -> Option<String> {
    let url = reqwest::Url::parse(&format!("http://x{target}")).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.into_owned())
}

fn send_file(stream: &mut TcpStream, request: &Request, shared: &Shared) {
    let (bytes, redirect) = {
        let data = lock(&shared.data);
        (data.file.clone(), data.redirect_to.clone())
    };
    if let Some(to) = redirect {
        return respond(stream, 302, &format!("Location: {to}\r\n"), b"");
    }
    let total = bytes.len();
    let range = request.header("range").and_then(|r| {
        let (from, to) = r.strip_prefix("bytes=")?.split_once('-')?;
        let from: usize = from.parse().ok()?;
        let to: usize = to.parse().unwrap_or(total.saturating_sub(1));
        (from <= to && to < total).then_some((from, to))
    });
    let (status, extra, body) = match range {
        Some((from, to)) => (
            206,
            format!("Content-Range: bytes {from}-{to}/{total}\r\n"),
            &bytes[from..=to],
        ),
        None => (200, String::new(), &bytes[..]),
    };
    let head = format!(
        "HTTP/1.1 {status} Fake\r\nContent-Length: {}\r\nContent-Type: video/x-matroska\r\nAccept-Ranges: bytes\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    if request.method == "HEAD" {
        return;
    }
    shared.open_files.fetch_add(1, Ordering::SeqCst);
    for chunk in body.chunks(64 << 10) {
        if stream.write_all(chunk).is_err() {
            break;
        }
    }
    shared.open_files.fetch_sub(1, Ordering::SeqCst);
}

fn handle(stream: &mut TcpStream, shared: &Shared) {
    let Some(request) = read_request(stream) else {
        return;
    };
    lock(&shared.requests).push(request.clone());
    let path = request.target.split('?').next().unwrap_or("").to_string();
    if let Some(path) = path.strip_prefix("/jf/") {
        jellyfin(stream, &request, path, shared);
    } else if let Some(path) = path.strip_prefix("/px/") {
        plex(stream, &request, path, shared);
    } else if let Some(path) = path.strip_prefix("/tv/") {
        plex_tv(stream, &request, path, shared);
    } else if let Some(path) = path.strip_prefix("/dav/") {
        webdav(stream, &request, path, shared);
    } else {
        respond(stream, 404, "", b"");
    }
}

/// `Basic` of the fake WebDAV's user `dav` and password `pass`.
const DAV_LOGIN: &str = "Basic ZGF2OnBhc3M=";

fn webdav(stream: &mut TcpStream, request: &Request, path: &str, shared: &Shared) {
    if request.header("authorization") != Some(DAV_LOGIN) {
        return respond(stream, 401, "", b"");
    }
    let name = path.rsplit('/').next().unwrap_or_default().to_string();
    let mut data = lock(&shared.data);
    match request.method.as_str() {
        "MKCOL" => respond(
            stream,
            if path.ends_with("devices/") { 405 } else { 201 },
            "",
            b"",
        ),
        "PROPFIND" => {
            let files: String = data
                .dav_files
                .keys()
                .map(|n| {
                    format!("<d:response><d:href>/dav/nikomiru/devices/{n}</d:href></d:response>")
                })
                .collect();
            let body = format!("<d:multistatus xmlns:d=\"DAV:\">{files}</d:multistatus>");
            respond(stream, 207, "", body.as_bytes());
        }
        "PUT" if data.dav_full => respond(stream, 507, "", b""),
        "PUT" => {
            data.dav_files.insert(name, request.body_bytes.clone());
            respond(stream, 201, "", b"");
        }
        "GET" => match data.dav_files.get(&name) {
            Some(bytes) => respond(stream, 200, "", bytes),
            None => respond(stream, 404, "", b""),
        },
        _ => respond(stream, 405, "", b""),
    }
}

fn jellyfin_item(item: &FakeItem) -> Value {
    json!({
        "Id": item.id,
        "Type": item.kind,
        "Name": item.title,
        "SeriesName": "A Series",
        "ParentId": item.parent,
        "ParentIndexNumber": 1,
        "IndexNumber": item.index,
        "RunTimeTicks": 600_000_000_u64,
        "ImageTags": { "Primary": "tag" },
        "ProviderIds": { "AniList": "1234" },
        "UserData": {
            "Played": false,
            "PlaybackPositionTicks": item.position_ms * 10_000,
            "LastPlayedDate": item.last_played_s.map(|_| "2026-10-31T09:00:00.000Z"),
        },
        "MediaSources": [{
            "Id": format!("source-{}", item.id),
            "MediaStreams": [
                { "Type": "Subtitle", "Index": 2, "Codec": "subrip", "Language": "jpn",
                  "DisplayTitle": "Japanese", "IsExternal": true, "IsTextSubtitleStream": true },
                { "Type": "Subtitle", "Index": 3, "Codec": "PGSSUB", "IsExternal": true,
                  "IsTextSubtitleStream": false },
                { "Type": "Subtitle", "Index": 4, "Codec": "ass", "IsExternal": false,
                  "IsTextSubtitleStream": true },
            ],
        }],
    })
}

fn jellyfin(stream: &mut TcpStream, request: &Request, path: &str, shared: &Shared) {
    let data = lock(&shared.data).clone();
    if path == "Users/AuthenticateByName" {
        let body: Value = serde_json::from_str(&request.body).unwrap_or_default();
        return if body["Username"] == USER && body["Pw"] == PASSWORD {
            send_json(
                stream,
                &json!({ "AccessToken": TOKEN, "User": { "Id": "user-1" } }),
            );
        } else {
            respond(stream, 401, "", b"");
        };
    }
    let authorized = request
        .header("authorization")
        .is_some_and(|h| h.contains(&format!("Token=\"{TOKEN}\"")));
    if !authorized {
        return respond(stream, 401, "", b"");
    }
    let segments: Vec<&str> = path.split('/').collect();
    match segments.as_slice() {
        ["UserViews"] => {
            let items: Vec<Value> = data
                .libraries
                .iter()
                .map(|(id, name)| json!({ "Id": id, "Name": name, "CollectionType": "tvshows" }))
                .chain([json!({ "Id": "m", "Name": "Music", "CollectionType": "music" })])
                .collect();
            send_json(stream, &json!({ "Items": items }));
        }
        ["Items"] => {
            let parent = query(&request.target, "ParentId").unwrap_or_default();
            let term = query(&request.target, "searchTerm");
            let matching: Vec<&FakeItem> = data
                .items
                .iter()
                .filter(|i| {
                    term.as_ref()
                        .map_or(i.parent == parent, |t| i.title.contains(t.as_str()))
                })
                .collect();
            let start = query(&request.target, "StartIndex")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let limit = query(&request.target, "Limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(usize::MAX);
            let page: Vec<Value> = matching
                .iter()
                .skip(start)
                .take(limit)
                .map(|i| jellyfin_item(i))
                .collect();
            send_json(
                stream,
                &json!({ "Items": page, "TotalRecordCount": matching.len() }),
            );
        }
        ["Items", id] => match data.items.iter().find(|i| i.id == *id) {
            Some(item) => send_json(stream, &jellyfin_item(item)),
            None => respond(stream, 404, "", b""),
        },
        ["Items", _, "Images", "Primary"] => respond(stream, 200, "", b"cover"),
        ["Videos", _, "stream"] => send_file(stream, request, shared),
        ["Videos", _, _, "Subtitles", _, _] => respond(stream, 200, "", &data.subtitle),
        ["Sessions", ..] | ["UserPlayedItems", _] => respond(stream, 204, "", b""),
        _ => respond(stream, 404, "", b""),
    }
}

fn plex_kind(kind: &str) -> &str {
    match kind {
        "Series" => "show",
        "Season" => "season",
        "Episode" => "episode",
        _ => "movie",
    }
}

fn plex_item(item: &FakeItem) -> Value {
    json!({
        "ratingKey": item.id,
        "type": plex_kind(&item.kind),
        "title": item.title,
        "grandparentTitle": "A Series",
        "parentRatingKey": item.parent,
        "parentIndex": 1,
        "index": item.index,
        "duration": 60_000,
        "viewOffset": item.position_ms,
        "lastViewedAt": item.last_played_s,
        "thumb": format!("/library/metadata/{}/thumb/1", item.id),
        "Media": [{ "Part": [{
            "key": format!("/library/parts/{}/file.mkv", item.id),
            "Stream": [
                { "streamType": 3, "key": "/library/streams/7", "codec": "srt",
                  "languageCode": "jpn", "displayTitle": "Japanese" },
                { "streamType": 3, "codec": "ass" },
                { "streamType": 1, "codec": "h264" },
            ],
        }]}],
    })
}

fn plex(stream: &mut TcpStream, request: &Request, path: &str, shared: &Shared) {
    if request.header("x-plex-token") != Some(TOKEN) {
        return respond(stream, 401, "", b"");
    }
    let data = lock(&shared.data).clone();
    let list = |parent: &str, title: Option<String>| {
        let matching: Vec<&FakeItem> = data
            .items
            .iter()
            .filter(|i| {
                title
                    .as_ref()
                    .map_or(i.parent == parent, |t| i.title.contains(t.as_str()))
            })
            .collect();
        let start = query(&request.target, "X-Plex-Container-Start")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let size = query(&request.target, "X-Plex-Container-Size")
            .and_then(|s| s.parse().ok())
            .unwrap_or(usize::MAX);
        let page: Vec<Value> = matching
            .iter()
            .skip(start)
            .take(size)
            .map(|i| plex_item(i))
            .collect();
        json!({ "MediaContainer": { "Metadata": page, "totalSize": matching.len() } })
    };
    let segments: Vec<&str> = path.split('/').collect();
    match segments.as_slice() {
        ["library", "sections"] => {
            let mut directory: Vec<Value> = data
                .libraries
                .iter()
                .map(|(id, name)| json!({ "key": id, "title": name, "type": "show" }))
                .collect();
            directory.push(json!({ "key": "m", "title": "Music", "type": "artist" }));
            send_json(
                stream,
                &json!({ "MediaContainer": { "Directory": directory } }),
            );
        }
        ["library", "sections", id, "all"] => {
            send_json(stream, &list(id, query(&request.target, "title")));
        }
        ["library", "metadata", id, "children"] => send_json(stream, &list(id, None)),
        ["library", "metadata", id] => match data.items.iter().find(|i| i.id == *id) {
            Some(item) => send_json(
                stream,
                &json!({ "MediaContainer": { "Metadata": [plex_item(item)] } }),
            ),
            None => respond(stream, 404, "", b""),
        },
        ["library", "metadata", _, "thumb", _] => respond(stream, 200, "", b"cover"),
        ["library", "parts", ..] => send_file(stream, request, shared),
        ["library", "streams", _] => respond(stream, 200, "", &data.subtitle),
        [":", "timeline" | "scrobble"] | ["%3A", "timeline" | "scrobble"] => {
            respond(stream, 200, "", b"");
        }
        _ => respond(stream, 404, "", b""),
    }
}

fn plex_tv(stream: &mut TcpStream, request: &Request, path: &str, shared: &Shared) {
    if request.header("x-plex-client-identifier").is_none() {
        return respond(stream, 400, "", b"");
    }
    let polls_needed = lock(&shared.data).pin_polls;
    match path {
        "api/v2/pins" if request.method == "POST" => {
            send_json(stream, &json!({ "id": 42, "code": "abcd" }));
        }
        "api/v2/pins/42" => {
            let polls = shared.polls.fetch_add(1, Ordering::SeqCst) + 1;
            let token = (polls > polls_needed).then_some(TOKEN);
            send_json(stream, &json!({ "authToken": token }));
        }
        "api/v2/resources" => {
            if request.header("x-plex-token") != Some(TOKEN) {
                return respond(stream, 401, "", b"");
            }
            send_json(
                stream,
                &json!([
                    { "name": "Player", "provides": "player" },
                    { "name": "Home", "provides": "server,player", "accessToken": TOKEN,
                      "connections": [
                        { "uri": "https://relay.example.org", "local": false, "relay": true },
                        { "uri": "https://remote.example.org", "local": false, "relay": false },
                        { "uri": "http://192.168.1.2:32400", "local": true, "relay": false },
                      ] },
                ]),
            );
        }
        _ => respond(stream, 404, "", b""),
    }
}

/// A fake hooker: a WebSocket on `127.0.0.1` that sends lines to whoever
/// connects, and goes on until dropped (HOK-001).
#[derive(Debug)]
pub struct FakeHooker {
    port: u16,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
}

impl FakeHooker {
    /// Starts it; each connection gets `lines`, then stays open.
    pub fn start(lines: Vec<String>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let connections = Arc::new(AtomicUsize::new(0));
        let counting = Arc::clone(&connections);
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(connection) = connection else {
                    continue;
                };
                counting.fetch_add(1, Ordering::SeqCst);
                let (lines, stopping) = (lines.clone(), Arc::clone(&stopping));
                std::thread::spawn(move || {
                    let Ok(mut socket) = tungstenite::accept(connection) else {
                        return;
                    };
                    for line in lines {
                        if socket.send(line.into()).is_err() {
                            return;
                        }
                    }
                    while !stopping.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                });
            }
        });
        Ok(Self {
            port,
            stop,
            connections,
        })
    }

    /// Its address.
    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    /// How many connected so far.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

impl Drop for FakeHooker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}
