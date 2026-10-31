// SPDX-License-Identifier: GPL-3.0-only

//! The relay (SRV-009): a server's file on `127.0.0.1`, under a path nobody
//! can guess, for mpv and ffmpeg. They read it like any address over HTTP
//! while the token, the proxy and the server's address stay here (SRV-004).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::{Method, StatusCode};

use super::{Stream, is_private};
use crate::error::OnlineError;

/// The connections served at once.
pub const MAX_CONNECTIONS: usize = 8;
/// The largest request head.
const HEAD_LIMIT: usize = 16 << 10;
/// The answer's headers passed on.
const PASSED: [&str; 4] = [
    "content-type",
    "content-length",
    "content-range",
    "accept-ranges",
];

struct Shared {
    stream: Stream,
    client: Client,
    path: String,
    stop: AtomicBool,
    connections: AtomicUsize,
}

/// A running relay: it stops when dropped.
pub struct Relay {
    shared: Arc<Shared>,
    port: u16,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl Relay {
    /// Starts one for `stream`, its server asked through `proxy` (SRV-005).
    pub fn start(stream: Stream, proxy: &str) -> Result<Self, OnlineError> {
        let io = |e: std::io::Error| OnlineError::Io(e.to_string());
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(io)?;
        let port = listener.local_addr().map_err(io)?.port();
        let mut client = Client::builder()
            .user_agent(concat!("Nikomiru/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(crate::http::CONNECT_TIMEOUT)
            .timeout(None)
            // a redirect could take the token elsewhere (SRV-004)
            .redirect(reqwest::redirect::Policy::none());
        client = if proxy.trim().is_empty() || is_private(&stream.url) {
            client.no_proxy()
        } else {
            let p = reqwest::Proxy::all(proxy.trim())
                .map_err(|_| OnlineError::BadUrl(nikomiru_core::config::without_userinfo(proxy)))?;
            client.proxy(p)
        };
        let client = client.build().map_err(|e| OnlineError::Io(e.to_string()))?;
        let shared = Arc::new(Shared {
            stream,
            client,
            path: format!("/{}/video", uuid::Uuid::new_v4().simple()),
            stop: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
        });
        let accepting = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("server-relay".into())
            .spawn(move || accept(&listener, &accepting))
            .map_err(io)?;
        Ok(Self { shared, port })
    }

    /// The address for mpv and ffmpeg.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.port, self.shared.path)
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn accept(listener: &TcpListener, shared: &Arc<Shared>) {
    for connection in listener.incoming() {
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut connection) = connection else {
            continue;
        };
        if shared.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            shared.connections.fetch_sub(1, Ordering::SeqCst);
            // read what was asked: closing over unread bytes loses the answer
            let _ = connection.set_read_timeout(Some(Duration::from_millis(50)));
            let _ = connection.read(&mut [0u8; 2048]);
            answer(&mut connection, 503, &[]);
            continue;
        }
        let serving = Arc::clone(shared);
        let spawned = std::thread::Builder::new()
            .name("server-relay-conn".into())
            .spawn(move || {
                serve(&mut connection, &serving);
                serving.connections.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            shared.connections.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn answer(connection: &mut TcpStream, status: u16, headers: &[(String, String)]) {
    let reason = StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("Status");
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !headers.iter().any(|(n, _)| n == "content-length") {
        head.push_str("Content-Length: 0\r\n");
    }
    head.push_str("\r\n");
    let _ = connection.write_all(head.as_bytes());
}

/// The method, target and `Range` of the request on `connection`.
fn read_head(connection: &mut TcpStream) -> Option<(String, String, Option<String>)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
        if buffer.len() > HEAD_LIMIT {
            return None;
        }
        match connection.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    let head = String::from_utf8_lossy(&buffer);
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let (method, target) = (first.next()?.to_string(), first.next()?.to_string());
    let range = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.trim().to_string());
    Some((method, target, range))
}

fn serve(connection: &mut TcpStream, shared: &Shared) {
    let _ = connection.set_read_timeout(Some(Duration::from_secs(10)));
    let Some((method, target, range)) = read_head(connection) else {
        return;
    };
    let method = match method.as_str() {
        "GET" => Method::GET,
        "HEAD" => Method::HEAD,
        _ => return answer(connection, 405, &[]),
    };
    if target != shared.path {
        return answer(connection, 404, &[]);
    }
    let mut request = shared
        .client
        .request(method.clone(), shared.stream.url.clone());
    for (name, value) in &shared.stream.headers {
        request = request.header(name, value);
    }
    if let Some(range) = range {
        request = request.header(reqwest::header::RANGE, range);
    }
    let Ok(mut upstream) = request.send() else {
        return answer(connection, 502, &[]);
    };
    let status = upstream.status().as_u16();
    if !matches!(status, 200 | 206 | 416) {
        // the server's own answers, and its address in a redirect, stay here
        let refused = if matches!(status, 401 | 403 | 404) {
            status
        } else {
            502
        };
        return answer(connection, refused, &[]);
    }
    let headers: Vec<(String, String)> = PASSED
        .iter()
        .filter_map(|name| {
            let value = upstream.headers().get(*name)?.to_str().ok()?;
            Some(((*name).to_string(), value.to_string()))
        })
        .collect();
    answer(connection, status, &headers);
    if method == Method::HEAD {
        return;
    }
    let mut chunk = vec![0u8; 256 << 10];
    while !shared.stop.load(Ordering::SeqCst) {
        match upstream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            // the client left: dropping `upstream` ends the server's answer
            Ok(n) if connection.write_all(&chunk[..n]).is_err() => return,
            Ok(_) => {}
        }
    }
}
