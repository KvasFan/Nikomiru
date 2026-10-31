// SPDX-License-Identifier: GPL-3.0-only

//! Media servers from the interface (SPECIFICATION §6.13): the jobs the app's
//! `servers` worker runs, and the state of the tabs of the library and of the
//! settings page.

use std::collections::BTreeMap;

use nikomiru_core::config::ServerConfig;
use nikomiru_online::servers::{Item, Library, ParentKind, PlayState};

type Done<T> = Box<dyn FnOnce(Result<T, String>) + Send>;

/// A video of a server, ready to play through the relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVideo {
    /// Its uid (SRV-007).
    pub uid: String,
    /// The relay's address.
    pub url: String,
    /// What the server knows of it.
    pub item: Item,
    /// What reports name (SRV-011).
    pub source: String,
    /// The next one and its title (SRV-008).
    pub next: Option<(String, String)>,
}

/// What a login found: servers to add.
pub type Found = Vec<ServerConfig>;

/// A page of a server's tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// The items so far.
    pub items: Vec<Item>,
    /// How many there are.
    pub total: usize,
}

/// A Plex sign-in waiting for the browser (SRV-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    /// The page to open.
    pub url: String,
}

/// The app's `servers` worker. The answers come back on its thread; errors
/// are translated messages. Each call only queues: none blocks the interface.
pub trait ServerService: Send {
    /// New settings.
    fn configure(&self, servers: Vec<ServerConfig>, proxy: String, device_id: String);
    /// Makes a video ready to play.
    fn prepare(&self, uid: String, done: Done<ServerVideo>);
    /// The video closed: the relay goes.
    fn close(&self);
    /// A moment of playback (SRV-011).
    fn report(&self, uid: String, source: String, state: PlayState, at_ms: u64, duration_ms: u64);
    /// Marks a video watched (SRV-011).
    fn mark_played(&self, uid: String);
    /// Logs in to a Jellyfin (SRV-002).
    fn jellyfin_login(
        &self,
        url: String,
        user: String,
        password: String,
        allow_unencrypted: bool,
        done: Done<ServerConfig>,
    );
    /// Starts a Plex sign-in (SRV-003): `started` gets the page to open, then
    /// `done` the account's servers.
    fn plex_login(&self, started: Done<SignIn>, done: Done<Found>);
    /// Stops the Plex sign-in going on.
    fn plex_cancel(&self);
    /// Ends the session of a server being removed.
    fn logout(&self, server: ServerConfig);
    /// A server's libraries.
    fn libraries(&self, server: String, done: Done<Vec<Library>>);
    /// A page of what a library or item holds.
    fn children(
        &self,
        server: String,
        parent: String,
        start: usize,
        kind: ParentKind,
        done: Done<Listing>,
    );
    /// A page of the search in a library.
    fn search(
        &self,
        server: String,
        library: String,
        query: String,
        start: usize,
        done: Done<Listing>,
    );
    /// A cover's bytes.
    fn cover(&self, server: String, cover: String, done: Done<Vec<u8>>);
}

/// No servers: for the tests of the interface.
#[derive(Debug, Default)]
pub struct NoServers;

impl ServerService for NoServers {
    fn configure(&self, _: Vec<ServerConfig>, _: String, _: String) {}
    fn prepare(&self, _: String, done: Done<ServerVideo>) {
        done(Err(String::new()));
    }
    fn close(&self) {}
    fn report(&self, _: String, _: String, _: PlayState, _: u64, _: u64) {}
    fn mark_played(&self, _: String) {}
    fn jellyfin_login(&self, _: String, _: String, _: String, _: bool, done: Done<ServerConfig>) {
        done(Err(String::new()));
    }
    fn plex_login(&self, started: Done<SignIn>, _: Done<Found>) {
        started(Err(String::new()));
    }
    fn plex_cancel(&self) {}
    fn logout(&self, _: ServerConfig) {}
    fn libraries(&self, _: String, done: Done<Vec<Library>>) {
        done(Err(String::new()));
    }
    fn children(&self, _: String, _: String, _: usize, _: ParentKind, done: Done<Listing>) {
        done(Err(String::new()));
    }
    fn search(&self, _: String, _: String, _: String, _: usize, done: Done<Listing>) {
        done(Err(String::new()));
    }
    fn cover(&self, _: String, _: String, done: Done<Vec<u8>>) {
        done(Err(String::new()));
    }
}

/// A job for the worker, taken by the UI layer: its answer needs the waker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerJob {
    /// Makes this video ready.
    Prepare(String),
    /// Logs in to a Jellyfin.
    Jellyfin {
        /// Its address.
        url: String,
        /// The user.
        user: String,
        /// The password: dropped once sent (SRV-002).
        password: String,
        /// Plain HTTP allowed (SRV-005).
        allow_unencrypted: bool,
    },
    /// Signs in to Plex.
    Plex,
    /// The libraries of the server shown.
    Libraries(String),
    /// A page of what is shown.
    List {
        /// The server.
        server: String,
        /// What is listed.
        parent: String,
        /// From where.
        start: usize,
        /// What it is.
        kind: ParentKind,
    },
    /// A page of a search.
    Search {
        /// The server.
        server: String,
        /// The library.
        library: String,
        /// What is searched.
        query: String,
        /// From where.
        start: usize,
    },
    /// The cover of an item.
    Cover {
        /// The server.
        server: String,
        /// Its id, also the key of the textures.
        item: String,
        /// Where it is.
        cover: String,
    },
}

/// What comes back for the pages.
#[derive(Debug)]
pub enum ServerAnswer {
    /// The servers a login found.
    Added(Result<Found, String>),
    /// The Plex page to open.
    SignIn(Result<SignIn, String>),
    /// A server's libraries.
    Libraries(String, Result<Vec<Library>, String>),
    /// A page, and what it lists.
    Listed(String, Result<Listing, String>),
    /// A cover, by item.
    Cover(String, Result<Vec<u8>, String>),
}

/// A place in a server's tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// What is listed.
    pub parent: String,
    /// Its name.
    pub title: String,
    /// What it is.
    pub kind: ParentKind,
}

/// A server's tab of the library page (SRV-006), and the settings page.
#[derive(Debug, Default)]
pub struct ServersPage {
    /// The server shown.
    pub server: Option<String>,
    /// Its libraries.
    pub libraries: Vec<Library>,
    /// From the library down to what is shown.
    pub path: Vec<Place>,
    /// What is shown.
    pub listing: Option<Listing>,
    /// The search shown, when one is.
    pub search: Option<String>,
    /// An answer is awaited.
    pub busy: bool,
    /// What failed.
    pub error: Option<String>,
    /// The covers decoded, by item.
    pub covers: BTreeMap<String, crate::catalog::Cover>,
    /// The covers asked for.
    pub asked: std::collections::BTreeSet<String>,
    /// The settings page's last message, and whether it is good news.
    pub status: Option<(String, bool)>,
    /// A login is going on.
    pub logging_in: bool,
}
