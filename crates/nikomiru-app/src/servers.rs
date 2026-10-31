// SPDX-License-Identifier: GPL-3.0-only

//! The `servers` worker (SPECIFICATION §6.13): the requests to media servers
//! one at a time on a thread of their own, so that playback never waits for
//! them (SRV-011), the relay of the video playing (SRV-009), the subtitles
//! and the covers kept on disk (SRV-006, SRV-010).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nikomiru_core::config::ServerConfig;
use nikomiru_library::server_items::{parse_server_uid, server_uid};
use nikomiru_online::OnlineError;
use nikomiru_online::http::Http;
use nikomiru_online::servers::jellyfin::Jellyfin;
use nikomiru_online::servers::plex::{Plex, PlexTv};
use nikomiru_online::servers::relay::Relay;
use nikomiru_online::servers::{
    Library, MediaServer, ParentKind, PlayState, ServerKind, ServerSubtitle, Url,
    checked_server_url, http_for, next_of,
};
use nikomiru_ui::i18n::Translator;
use nikomiru_ui::servers::{Found, Listing, ServerService, ServerVideo, SignIn};
use sha2::{Digest, Sha256};

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// How often a Plex PIN is asked about, and for how long (SRV-003).
const PIN_POLL: Duration = Duration::from_secs(2);
const PIN_LIMIT: Duration = Duration::from_secs(600);
/// The most covers keep on disk (SRV-006).
const COVER_LIMIT: u64 = 500 << 20;

/// What the worker needs.
pub struct Parts {
    /// `[[servers]]` of the settings.
    pub servers: Vec<ServerConfig>,
    /// `[network] proxy`.
    pub proxy: String,
    /// `[network] device_id`.
    pub device_id: String,
    /// The address of plex.tv.
    pub plex_tv_url: String,
    /// Where subtitles are kept.
    pub subtitles_dir: PathBuf,
    /// Where covers are kept.
    pub covers_dir: PathBuf,
    /// The interface strings, for the messages.
    pub translator: Translator,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("servers", &self.servers)
            .finish_non_exhaustive()
    }
}

enum Job {
    Configure(Vec<ServerConfig>, String, String),
    Prepare(String, Done<ServerVideo>),
    Close,
    Report(String, String, PlayState, u64, u64),
    MarkPlayed(String),
    JellyfinLogin(String, String, String, bool, Done<ServerConfig>),
    PlexPin(Done<SignIn>, Done<Found>),
    Logout(Box<ServerConfig>),
    Libraries(String, Done<Vec<Library>>),
    Children(String, String, usize, ParentKind, Done<Listing>),
    Search(String, String, String, usize, Done<Listing>),
    Cover(String, String, Done<Vec<u8>>),
}

/// The service: the jobs go to the worker thread, in order.
pub struct Servers {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Set when Nikomiru closes: only the last reports are still sent.
    closing: Arc<AtomicBool>,
    /// Stops the Plex sign-in going on.
    cancel: Arc<AtomicBool>,
}

impl std::fmt::Debug for Servers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Servers").finish_non_exhaustive()
    }
}

impl Drop for Servers {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::SeqCst);
        self.cancel.store(true, Ordering::SeqCst);
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Servers {
    /// Starts the worker. It connects to nothing until a job asks (SRV-004).
    pub fn start(parts: Parts) -> Self {
        let closing = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        let (jobs, rx) = mpsc::channel::<Job>();
        let (stop, cancelled) = (Arc::clone(&closing), Arc::clone(&cancel));
        let spawned = std::thread::Builder::new()
            .name("nikomiru-servers".into())
            .spawn(move || {
                let mut worker = Worker {
                    parts,
                    connected: HashMap::new(),
                    relay: None,
                    cancel: cancelled,
                };
                for job in rx {
                    let last_report = matches!(job, Job::Report(_, _, PlayState::Stopped, _, _));
                    if stop.load(Ordering::SeqCst) && !last_report {
                        continue;
                    }
                    worker.run(job);
                }
            });
        let (jobs, worker) = match spawned {
            Ok(worker) => (Some(jobs), Some(worker)),
            Err(e) => {
                tracing::error!("the servers worker did not start: {e}");
                (None, None)
            }
        };
        Self {
            jobs,
            worker,
            closing,
            cancel,
        }
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }
}

impl ServerService for Servers {
    fn configure(&self, servers: Vec<ServerConfig>, proxy: String, device_id: String) {
        self.send(Job::Configure(servers, proxy, device_id));
    }
    fn prepare(&self, uid: String, done: Done<ServerVideo>) {
        self.send(Job::Prepare(uid, done));
    }
    fn close(&self) {
        self.send(Job::Close);
    }
    fn report(&self, uid: String, source: String, state: PlayState, at_ms: u64, duration_ms: u64) {
        self.send(Job::Report(uid, source, state, at_ms, duration_ms));
    }
    fn mark_played(&self, uid: String) {
        self.send(Job::MarkPlayed(uid));
    }
    fn jellyfin_login(
        &self,
        url: String,
        user: String,
        password: String,
        allow_unencrypted: bool,
        done: Done<ServerConfig>,
    ) {
        self.send(Job::JellyfinLogin(
            url,
            user,
            password,
            allow_unencrypted,
            done,
        ));
    }
    fn plex_login(&self, started: Done<SignIn>, done: Done<Found>) {
        self.cancel.store(false, Ordering::SeqCst);
        self.send(Job::PlexPin(started, done));
    }
    fn plex_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn logout(&self, server: ServerConfig) {
        self.send(Job::Logout(Box::new(server)));
    }
    fn libraries(&self, server: String, done: Done<Vec<Library>>) {
        self.send(Job::Libraries(server, done));
    }
    fn children(
        &self,
        server: String,
        parent: String,
        start: usize,
        kind: ParentKind,
        done: Done<Listing>,
    ) {
        self.send(Job::Children(server, parent, start, kind, done));
    }
    fn search(
        &self,
        server: String,
        library: String,
        query: String,
        start: usize,
        done: Done<Listing>,
    ) {
        self.send(Job::Search(server, library, query, start, done));
    }
    fn cover(&self, server: String, cover: String, done: Done<Vec<u8>>) {
        self.send(Job::Cover(server, cover, done));
    }
}

struct Worker {
    parts: Parts,
    /// The servers reached so far.
    connected: HashMap<String, Box<dyn MediaServer>>,
    /// The relay of the video open.
    relay: Option<Relay>,
    cancel: Arc<AtomicBool>,
}

fn listing(page: nikomiru_online::servers::Page) -> Listing {
    Listing {
        items: page.items,
        total: page.total,
    }
}

impl Worker {
    fn message(&self, e: &OnlineError) -> String {
        self.parts.translator.error(e)
    }

    fn run(&mut self, job: Job) {
        match job {
            Job::Configure(servers, proxy, device_id) => {
                self.parts.servers = servers;
                self.parts.proxy = proxy;
                self.parts.device_id = device_id;
                self.connected.clear();
            }
            Job::Prepare(uid, done) => done(self.prepare(&uid)),
            Job::Close => self.relay = None,
            Job::Report(uid, source, state, at_ms, duration_ms) => {
                let Some((server, item)) = parse_server_uid(&uid) else {
                    return;
                };
                let result = self
                    .server(server)
                    .and_then(|s| s.report(item, &source, state, at_ms, duration_ms));
                // SRV-011: dropped; the next one says more
                if let Err(e) = result {
                    tracing::info!("server report: {e}");
                    self.connected.remove(server);
                }
            }
            Job::MarkPlayed(uid) => {
                if let Some((server, item)) = parse_server_uid(&uid)
                    && let Err(e) = self.server(server).and_then(|s| s.mark_played(item))
                {
                    tracing::info!("server: {e}");
                }
            }
            Job::JellyfinLogin(url, user, password, allow, done) => {
                done(self.jellyfin_login(&url, &user, &password, allow));
            }
            Job::PlexPin(started, done) => self.plex_login(started, done),
            Job::Logout(server) => {
                let id = server.id.clone();
                if let Some(mut connected) = self.connected.remove(&id)
                    && let Err(e) = connected.logout()
                {
                    tracing::info!("server logout: {e}");
                }
            }
            Job::Libraries(server, done) => {
                let r = self.answer(&server, |s| s.libraries());
                done(r);
            }
            Job::Children(server, parent, start, kind, done) => {
                let r = self.answer(&server, |s| s.children(&parent, start, kind).map(listing));
                done(r);
            }
            Job::Search(server, library, query, start, done) => {
                let r = self.answer(&server, |s| s.search(&library, &query, start).map(listing));
                done(r);
            }
            Job::Cover(server, cover, done) => {
                let r = self.cover(&server, &cover);
                done(r);
            }
        }
    }

    fn answer<T>(
        &mut self,
        server: &str,
        ask: impl FnOnce(&mut dyn MediaServer) -> Result<T, OnlineError>,
    ) -> Result<T, String> {
        let result = self.server(server).and_then(ask);
        result.map_err(|e| {
            self.connected.remove(server);
            self.message(&e)
        })
    }

    /// The server `id`, reached the first time it is needed (SRV-004). Plex's
    /// addresses are tried in order, the local ones first (SRV-003).
    fn server(&mut self, id: &str) -> Result<&mut dyn MediaServer, OnlineError> {
        if !self.connected.contains_key(id) {
            let config = self
                .parts
                .servers
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .ok_or_else(|| OnlineError::Gone {
                    provider: self.parts.translator.tr("servers-this-server", &[]),
                })?;
            let connected = self.connect(&config)?;
            self.connected.insert(id.to_string(), connected);
        }
        match self.connected.get_mut(id) {
            Some(server) => Ok(server.as_mut()),
            None => Err(OnlineError::Io("no server".into())),
        }
    }

    fn connect(&self, config: &ServerConfig) -> Result<Box<dyn MediaServer>, OnlineError> {
        let kind = ServerKind::from_key(&config.kind);
        let address = |text: &str| -> Result<(Http, Url), OnlineError> {
            let url = checked_server_url(text, config.allow_unencrypted)?;
            Ok((http_for(&url, &self.parts.proxy)?, url))
        };
        let device = self.parts.device_id.as_str();
        if kind == Some(ServerKind::Jellyfin) {
            let (http, url) = address(&config.url)?;
            return Ok(Box::new(Jellyfin::new(
                http,
                url,
                &config.token,
                &config.user_id,
                device,
            )));
        }
        let mut last = OnlineError::BadUrl(config.kind.clone());
        for text in std::iter::once(&config.url).chain(&config.other_urls) {
            let attempt = address(text).and_then(|(http, url)| {
                let mut server = Plex::new(http, url, &config.token, device);
                server.libraries().map(|_| server)
            });
            match attempt {
                Ok(server) => return Ok(Box::new(server)),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn prepare(&mut self, uid: &str) -> Result<ServerVideo, String> {
        let Some((server, item)) = parse_server_uid(uid) else {
            return Err(self.message(&OnlineError::BadUrl(uid.to_string())));
        };
        let proxy = self.parts.proxy.clone();
        let folder = nikomiru_online::store::server_folder(&self.parts.subtitles_dir, uid);
        let id = server.to_string();
        let result: Result<_, OnlineError> = (|| {
            let session = self.server(&id)?;
            let info = session.item(item)?;
            let playable = session.playable(item)?;
            for (i, subtitle) in playable.subtitles.iter().enumerate() {
                keep_subtitle(session, &folder, i, subtitle);
            }
            let next = match &info.parent {
                Some(parent) => next_item(session, parent, item),
                None => None,
            };
            let relay = Relay::start(playable.stream, &proxy)?;
            Ok((info, playable.source, next, relay))
        })();
        match result {
            Ok((info, source, next, relay)) => {
                let url = relay.url();
                self.relay = Some(relay);
                Ok(ServerVideo {
                    uid: uid.to_string(),
                    url,
                    item: info,
                    source,
                    next: next.map(|(item, title)| (server_uid(&id, &item), title)),
                })
            }
            Err(e) => {
                self.connected.remove(&id);
                Err(self.message(&e))
            }
        }
    }

    fn jellyfin_login(
        &self,
        url: &str,
        user: &str,
        password: &str,
        allow_unencrypted: bool,
    ) -> Result<ServerConfig, String> {
        let attempt = || {
            let base = checked_server_url(url, allow_unencrypted)?;
            let http = http_for(&base, &self.parts.proxy)?;
            let server =
                Jellyfin::login(http, base.clone(), user, password, &self.parts.device_id)?;
            Ok::<_, OnlineError>(ServerConfig {
                id: uuid::Uuid::new_v4().simple().to_string(),
                kind: ServerKind::Jellyfin.key().into(),
                name: base.host_str().unwrap_or_default().to_string(),
                url: url.trim().to_string(),
                token: server.token().to_string(),
                user_id: server.user_id().to_string(),
                allow_unencrypted,
                ..ServerConfig::default()
            })
        };
        attempt().map_err(|e| self.message(&e))
    }

    /// The sign-in waits on a thread of its own: the worker goes on (SRV-003).
    fn plex_login(&self, started: Done<SignIn>, done: Done<Found>) {
        let message = |e: &OnlineError| self.message(e);
        let tv = (|| {
            let base = checked_server_url(&self.parts.plex_tv_url, false)?;
            let http = http_for(&base, &self.parts.proxy)?;
            Ok::<_, OnlineError>(PlexTv::new(http, base, &self.parts.device_id))
        })();
        let tv = match tv {
            Ok(tv) => tv,
            Err(e) => return started(Err(message(&e))),
        };
        let pin = match tv.pin() {
            Ok(pin) => pin,
            Err(e) => return started(Err(message(&e))),
        };
        started(Ok(SignIn {
            url: tv.sign_in_url(&pin),
        }));
        let translator = self.parts.translator.clone();
        let cancel = Arc::clone(&self.cancel);
        let waiting = move || {
            let began = Instant::now();
            let found = loop {
                if cancel.load(Ordering::SeqCst) || began.elapsed() > PIN_LIMIT {
                    return done(Err(translator.tr("servers-plex-stopped", &[])));
                }
                match tv.token(&pin) {
                    Ok(Some(token)) => break tv.resources(&token),
                    Ok(None) => std::thread::sleep(PIN_POLL),
                    Err(e) => break Err(e),
                }
            };
            done(
                found
                    .map(|servers| servers.into_iter().map(plex_config).collect())
                    .map_err(|e| translator.error(&e)),
            );
        };
        if let Err(e) = std::thread::Builder::new()
            .name("plex-sign-in".into())
            .spawn(waiting)
        {
            tracing::error!("the Plex sign-in did not start: {e}");
        }
    }

    fn cover(&mut self, server: &str, cover: &str) -> Result<Vec<u8>, String> {
        let digest = Sha256::digest(format!("{server}/{cover}").as_bytes());
        let name: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let path = self.parts.covers_dir.join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            return Ok(bytes);
        }
        let bytes = self.answer(server, |s| s.cover(cover))?;
        if std::fs::create_dir_all(&self.parts.covers_dir).is_ok()
            && std::fs::write(&path, &bytes).is_ok()
        {
            trim_folder(&self.parts.covers_dir, COVER_LIMIT);
        }
        Ok(bytes)
    }
}

fn plex_config(resource: nikomiru_online::servers::plex::Resource) -> ServerConfig {
    let mut addresses = resource.addresses.into_iter();
    ServerConfig {
        id: uuid::Uuid::new_v4().simple().to_string(),
        kind: ServerKind::Plex.key().into(),
        name: resource.name,
        url: addresses.next().unwrap_or_default(),
        other_urls: addresses.collect(),
        token: resource.token,
        ..ServerConfig::default()
    }
}

/// A subtitle's text in the video's folder, once (SRV-010). One that cannot
/// be had is left out: the video plays.
fn keep_subtitle(
    server: &mut dyn MediaServer,
    folder: &Path,
    index: usize,
    subtitle: &ServerSubtitle,
) {
    let language = subtitle.language.as_deref().unwrap_or("und");
    let clean: String = language
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let path = folder.join(format!("server-{index}.{clean}.{}", subtitle.format));
    if path.exists() {
        return;
    }
    match server.subtitle(subtitle) {
        Ok(bytes) => {
            if let Err(e) = std::fs::create_dir_all(folder)
                .and_then(|()| nikomiru_core::fs::write_atomic(&path, &bytes, 0o600))
            {
                tracing::warn!("{}: {e}", path.display());
            }
        }
        Err(e) => tracing::info!("server subtitle: {e}"),
    }
}

/// What comes after `item` in `parent`, in the server's order (SRV-008).
fn next_item(server: &mut dyn MediaServer, parent: &str, item: &str) -> Option<(String, String)> {
    let mut items = Vec::new();
    loop {
        let page = server
            .children(parent, items.len(), ParentKind::Item)
            .ok()?;
        let more = !page.items.is_empty() && items.len() + page.items.len() < page.total;
        items.extend(page.items);
        if !more {
            break;
        }
    }
    next_of(&items, item).map(|next| (next.id.clone(), next.title.clone()))
}

/// Removes the oldest files of `folder` until it holds at most `limit` bytes.
fn trim_folder(folder: &Path, limit: u64) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let mut size: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, length, path) in files {
        if size <= limit {
            return;
        }
        if std::fs::remove_file(&path).is_ok() {
            size = size.saturating_sub(length);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SRV-006
    #[test]
    fn the_oldest_covers_go_first() {
        let dir = tempfile::tempdir().unwrap();
        let file = |name: &str, age: u64| {
            let path = dir.path().join(name);
            std::fs::write(&path, [0u8; 100]).unwrap();
            let time = std::time::SystemTime::now() - Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        };
        file("old", 300);
        file("middle", 200);
        file("new", 100);
        trim_folder(dir.path(), 200);
        assert!(!dir.path().join("old").exists());
        assert!(dir.path().join("middle").exists() && dir.path().join("new").exists());
    }
}
