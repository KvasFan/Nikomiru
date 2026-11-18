// SPDX-License-Identifier: GPL-3.0-only

//! A fake AniDB UDP API for the tests of MET-006: one account, files by size
//! and ed2k hash, every packet kept.

use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// The account the fake accepts.
pub const USER: &str = "learner";
/// Its password.
pub const PASSWORD: &str = "pa&ss";
/// The client the fake accepts.
pub const CLIENT: &str = "nikomirutest";
/// The session it gives.
pub const SESSION: &str = "Ab12x";

/// What the fake knows and does.
#[derive(Debug, Default)]
pub struct Data {
    /// `FILE` answers' data lines, by size and ed2k hash.
    pub files: HashMap<(u64, String), String>,
    /// Every packet received, in order.
    pub packets: Vec<String>,
    /// Logins answered `555 BANNED`.
    pub banned: bool,
    /// The next `FILE` answered `506 INVALID SESSION`.
    pub expire_once: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The running fake; it stops when dropped.
pub struct FakeAniDb {
    address: String,
    data: Arc<Mutex<Data>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for FakeAniDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeAniDb")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

/// The value of `key` in a request's parameters.
fn param<'a>(request: &'a str, key: &str) -> Option<&'a str> {
    let (_, params) = request.split_once(' ')?;
    params
        .split('&')
        .find_map(|p| p.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
}

fn answer(data: &Mutex<Data>, request: &str) -> String {
    let mut d = lock(data);
    d.packets.push(request.to_string());
    let command = request.split(' ').next().unwrap_or_default();
    match command {
        "AUTH" => {
            if d.banned {
                return "555 BANNED\nflooding".into();
            }
            if param(request, "client") != Some(CLIENT) || param(request, "clientver") != Some("1")
            {
                return "503 CLIENT VERSION OUTDATED".into();
            }
            // the password arrives escaped as AniDB reads it
            let password = PASSWORD.replace('&', "&amp;");
            // `&` splits the parameters: rebuild the password's value
            let given = request
                .split_once("pass=")
                .and_then(|(_, rest)| rest.split_once("&protover="))
                .map(|(p, _)| p);
            if param(request, "user") == Some(USER) && given == Some(password.as_str()) {
                format!("200 {SESSION} LOGIN ACCEPTED")
            } else {
                "500 LOGIN FAILED".into()
            }
        }
        "FILE" => {
            if param(request, "s") != Some(SESSION) {
                return "501 LOGIN FIRST".into();
            }
            if std::mem::take(&mut d.expire_once) {
                return "506 INVALID SESSION".into();
            }
            let size = param(request, "size").and_then(|s| s.parse().ok());
            let hash = param(request, "ed2k").map(str::to_string);
            match size.zip(hash).and_then(|key| d.files.get(&key)) {
                Some(line) => format!("220 FILE\n{line}\n"),
                None => "320 NO SUCH FILE".into(),
            }
        }
        "LOGOUT" => "203 LOGGED OUT".into(),
        _ => "598 UNKNOWN COMMAND".into(),
    }
}

impl FakeAniDb {
    /// Starts the fake with no file.
    pub fn start() -> std::io::Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        socket.set_read_timeout(Some(Duration::from_millis(50)))?;
        let address = socket.local_addr()?.to_string();
        let data = Arc::new(Mutex::new(Data::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (data, stop) = (data.clone(), stop.clone());
            std::thread::Builder::new()
                .name("fake-anidb".into())
                .spawn(move || {
                    let mut buffer = [0u8; 2048];
                    while !stop.load(Ordering::Relaxed) {
                        let Ok((read, from)) = socket.recv_from(&mut buffer) else {
                            continue;
                        };
                        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                        let reply = answer(&data, &request);
                        let _ = socket.send_to(reply.as_bytes(), from);
                    }
                })?
        };
        Ok(Self {
            address,
            data,
            stop,
            thread: Some(thread),
        })
    }

    /// Its address, `127.0.0.1:<port>`.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// What it knows and received.
    pub fn data(&self) -> MutexGuard<'_, Data> {
        lock(&self.data)
    }
}

impl Drop for FakeAniDb {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
