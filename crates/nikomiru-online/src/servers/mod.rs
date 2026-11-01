// SPDX-License-Identifier: GPL-3.0-only

//! Media servers (SPECIFICATION §6.13, M11): Jellyfin and Plex behind one
//! trait, the addresses they may use (SRV-005) and the relay that keeps their
//! tokens away from mpv and ffmpeg (SRV-009). Direct play only: the original
//! file, never a transcode (SRV-008).

pub mod jellyfin;
pub mod plex;
pub mod relay;

use std::net::IpAddr;

pub use reqwest::Url;

use crate::error::OnlineError;
use crate::http::{Http, Response};

/// The items asked at a time (SRV-006).
pub const PAGE: usize = 100;

/// The kind of a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerKind {
    /// Jellyfin (SRV-002).
    Jellyfin,
    /// Plex (SRV-003).
    Plex,
}

impl ServerKind {
    /// Its name in `config.toml`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Jellyfin => "jellyfin",
            Self::Plex => "plex",
        }
    }

    /// The kind of a name; `None` for any other.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "jellyfin" => Some(Self::Jellyfin),
            "plex" => Some(Self::Plex),
            _ => None,
        }
    }

    /// Its name for people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Jellyfin => "Jellyfin",
            Self::Plex => "Plex",
        }
    }
}

/// A library of a server: only those of series or movies are listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    /// Its id on the server.
    pub id: String,
    /// Its name.
    pub title: String,
}

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A series: has seasons.
    Series,
    /// A season: has episodes.
    Season,
    /// An episode: can be played.
    Episode,
    /// A movie: can be played.
    Movie,
}

impl ItemKind {
    /// Whether it can be played rather than opened.
    pub fn playable(self) -> bool {
        matches!(self, Self::Episode | Self::Movie)
    }
}

/// A series, season, episode or movie of a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its id on the server.
    pub id: String,
    /// What it is.
    pub kind: ItemKind,
    /// Its title.
    pub title: String,
    /// The title of its series.
    pub series: Option<String>,
    /// The season or series it is in, for what comes next (SRV-008).
    pub parent: Option<String>,
    /// The number of its season.
    pub season: Option<u32>,
    /// The number of the episode, or of the season for a season.
    pub episode: Option<u32>,
    /// Where its cover is, for [`MediaServer::cover`].
    pub cover: Option<String>,
    /// Watched.
    pub played: bool,
    /// The position saved on the server.
    pub position_ms: u64,
    /// Its length.
    pub duration_ms: u64,
    /// When it was last played, in seconds since the epoch.
    pub last_played_s: Option<i64>,
    /// Its AniList id, when the server has one.
    pub anilist_id: Option<u64>,
}

/// A part of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The items.
    pub items: Vec<Item>,
    /// How many there are in all.
    pub total: usize,
}

/// A text subtitle a server offers beside the video (SRV-010).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSubtitle {
    /// Where it is, for [`MediaServer::subtitle`].
    pub id: String,
    /// Its language, as the server names it.
    pub language: Option<String>,
    /// Its name.
    pub title: String,
    /// Its format, for the file's extension: letters and digits only.
    pub format: String,
}

/// A file on a server, with what is needed to read it. Holds the token: it
/// goes only to the relay (SRV-004).
#[derive(Clone, PartialEq, Eq)]
pub struct Stream {
    /// The address.
    pub url: Url,
    /// The headers, the token among them.
    pub headers: Vec<(String, String)>,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("url", &self.url.as_str())
            .finish_non_exhaustive()
    }
}

/// What it takes to play an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playable {
    /// The original file.
    pub stream: Stream,
    /// What reports about it name: Jellyfin's media source.
    pub source: String,
    /// Its external subtitles.
    pub subtitles: Vec<ServerSubtitle>,
}

/// A moment of playback reported to the server (SRV-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    /// The video started.
    Started,
    /// It goes on.
    Playing,
    /// It is paused.
    Paused,
    /// It closed.
    Stopped,
}

/// A logged-in server.
pub trait MediaServer: Send {
    /// Its libraries of series or movies.
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError>;
    /// A page of what a library or an item contains.
    fn children(
        &mut self,
        parent: &str,
        start: usize,
        kind: ParentKind,
    ) -> Result<Page, OnlineError>;
    /// A page of the series and movies of a library matching `query`.
    fn search(&mut self, library: &str, query: &str, start: usize) -> Result<Page, OnlineError>;
    /// One item.
    fn item(&mut self, id: &str) -> Result<Item, OnlineError>;
    /// How to play an item.
    fn playable(&mut self, id: &str) -> Result<Playable, OnlineError>;
    /// The text of a subtitle.
    fn subtitle(&mut self, subtitle: &ServerSubtitle) -> Result<Vec<u8>, OnlineError>;
    /// The bytes of a cover.
    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError>;
    /// Reports a moment of playback.
    fn report(
        &mut self,
        id: &str,
        source: &str,
        state: PlayState,
        position_ms: u64,
        duration_ms: u64,
    ) -> Result<(), OnlineError>;
    /// Marks an item as watched.
    fn mark_played(&mut self, id: &str) -> Result<(), OnlineError>;
    /// Ends the session on the server, where it has one.
    fn logout(&mut self) -> Result<(), OnlineError>;
}

/// What [`MediaServer::children`] lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentKind {
    /// A library.
    Library,
    /// A series or a season.
    Item,
}

/// Whether `url` names this computer or a network of the user's own: loopback,
/// private, link-local and CGNAT addresses (Tailscale among them), and the
/// names used only on such networks (SRV-005).
pub fn is_private(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => private_v4(ip),
        Ok(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback()
                || first & 0xfe00 == 0xfc00
                || first & 0xffc0 == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(private_v4)
        }
        Err(_) => {
            let name = host.trim_end_matches('.').to_ascii_lowercase();
            !name.is_empty()
                && (!name.contains('.')
                    || [".local", ".lan", ".home.arpa", ".internal", ".ts.net"]
                        .iter()
                        .any(|end| name.ends_with(end)))
        }
    }
}

fn private_v4(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback() || ip.is_private() || ip.is_link_local() || (a == 100 && b & 0xc0 == 64)
}

/// The address of a server, if Nikomiru may use it (SRV-005): HTTPS, or HTTP
/// on a private address or with `allow_unencrypted`. A user name or password
/// in the address is refused: it would reach the logs.
pub fn checked_server_url(url: &str, allow_unencrypted: bool) -> Result<Url, OnlineError> {
    let bad = || OnlineError::BadUrl(nikomiru_core::config::without_userinfo(url));
    let parsed = Url::parse(url.trim()).map_err(|_| bad())?;
    if parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(bad());
    }
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if allow_unencrypted || is_private(&parsed) => Ok(parsed),
        _ => Err(bad()),
    }
}

/// The HTTP clients for a server: through `proxy`, except to its own network
/// (SRV-005).
pub fn http_for(url: &Url, proxy: &str) -> Result<Http, OnlineError> {
    Http::new(if is_private(url) { "" } else { proxy })
}

/// A server's name of a subtitle format, as a file's extension: Jellyfin says
/// `subrip`, and only known extensions are read as subtitles (SUB-006).
pub(crate) fn subtitle_extension(codec: &str) -> Option<String> {
    let clean: String = codec
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    match clean.as_str() {
        "subrip" | "srt" => Some("srt".into()),
        "webvtt" | "vtt" => Some("vtt".into()),
        "ass" => Some("ass".into()),
        "ssa" => Some("ssa".into()),
        _ => None,
    }
}

/// The address of a path under `base`, each segment percent-encoded: ids come
/// from the server.
pub(crate) fn join(base: &Url, segments: &[&str]) -> Result<Url, OnlineError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|()| OnlineError::BadUrl(base.to_string()))?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

/// The address of `key`, a path the server gave, on the same server: anything
/// else would take the token elsewhere (SRV-004).
pub(crate) fn key_url(base: &Url, key: &str, provider: &str) -> Result<Url, OnlineError> {
    if !key.starts_with('/') || key.starts_with("//") || key.contains('\\') {
        return Err(OnlineError::Protocol {
            provider: provider.to_string(),
            reason: "an address outside the server".into(),
        });
    }
    let url = base
        .join(key.trim_start_matches('/'))
        .map_err(|_| OnlineError::Protocol {
            provider: provider.to_string(),
            reason: "a bad address".into(),
        })?;
    Ok(url)
}

/// The answer when it is a success; 404 says the item is gone (SRV-013).
pub(crate) fn success(response: Response, provider: &str) -> Result<Response, OnlineError> {
    if response.status == 404 {
        return Err(OnlineError::Gone {
            provider: provider.to_string(),
        });
    }
    response.ok(provider)
}

/// An RFC 3339 time, in seconds since the epoch.
pub fn parse_time(text: &str) -> Option<i64> {
    let (date, time) = text.trim().split_once(['T', 't'])?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (y, m, d) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, offset) = match time.find(['Z', 'z', '+', '-']) {
        Some(i) => time.split_at(i),
        None => (time, "Z"),
    };
    let mut clock = clock.splitn(3, ':');
    let hour = clock.next()?.parse::<i64>().ok()?;
    let minute = clock.next()?.parse::<i64>().ok()?;
    let second = clock.next()?.split('.').next()?.parse::<i64>().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hour > 23 || minute > 59 {
        return None;
    }
    // Howard Hinnant's `days_from_civil`
    let year = if m <= 2 { y - 1 } else { y };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let shift = match offset.split_at_checked(1)? {
        ("Z" | "z", "") => 0,
        (sign, rest) => {
            let (h, m) = rest.split_once(':')?;
            let minutes = h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?;
            if sign == "-" {
                -minutes * 60
            } else {
                minutes * 60
            }
        }
    };
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - shift)
}

/// The episode after `id` among `items`, in their order (SRV-008).
pub fn next_of<'a>(items: &'a [Item], id: &str) -> Option<&'a Item> {
    let at = items.iter().position(|i| i.id == id)?;
    items[at + 1..].iter().find(|i| i.kind.playable())
}

/// The position to start from: the one saved last, the local one on a tie
/// (SRV-011).
pub fn later_position(
    local_ms: u64,
    local_at_s: Option<i64>,
    server_ms: u64,
    server_at_s: Option<i64>,
) -> u64 {
    match (local_at_s, server_at_s) {
        (Some(local), Some(server)) if server > local => server_ms,
        (None, Some(_)) => server_ms,
        _ => local_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SRV-005
    #[test]
    fn plain_http_only_on_the_users_own_networks() {
        for good in [
            "http://127.0.0.1:8096",
            "http://localhost:8096",
            "http://192.168.1.10:8096",
            "http://10.0.0.2",
            "http://172.16.3.4",
            "http://169.254.1.1",
            "http://100.64.0.1",
            "http://100.127.255.1",
            "http://[::1]:8096",
            "http://[fd00::1]",
            "http://[fe80::1]",
            "http://[::ffff:192.168.1.2]",
            "http://nas",
            "http://nas.local:8096",
            "http://nas.lan",
            "http://box.home.arpa",
            "http://box.tail1234.ts.net",
        ] {
            assert!(checked_server_url(good, false).is_ok(), "{good}");
        }
        for bad in [
            "http://8.8.8.8",
            "http://100.128.0.1",
            "http://172.32.0.1",
            "http://[2001:db8::1]",
            "http://media.example.org",
            "ftp://nas",
            "nas",
            "",
        ] {
            assert!(checked_server_url(bad, false).is_err(), "{bad}");
        }
        assert!(checked_server_url("http://media.example.org", true).is_ok());
        assert!(checked_server_url("https://media.example.org", false).is_ok());
        let e = checked_server_url("http://user:secret@nas", true).unwrap_err();
        assert!(!e.to_string().contains("secret"), "{e}");
    }

    // T-SRV-005: the proxy is not for the user's own network
    #[test]
    fn the_proxy_stays_out_of_private_addresses() {
        let private = Url::parse("http://192.168.1.2").unwrap();
        let public = Url::parse("https://example.org").unwrap();
        assert!(is_private(&private) && !is_private(&public));
        assert!(http_for(&private, "not a proxy").is_ok());
        assert!(http_for(&public, "not a proxy").is_err());
    }

    // T-SRV-004: a server's key never leads elsewhere
    #[test]
    fn keys_stay_on_the_server() {
        let base = Url::parse("https://plex.example.org:32400/").unwrap();
        assert_eq!(
            key_url(&base, "/library/parts/1/file.mkv", "P")
                .unwrap()
                .as_str(),
            "https://plex.example.org:32400/library/parts/1/file.mkv"
        );
        for bad in [
            "//evil.org/x",
            "https://evil.org/x",
            "evil.org",
            "/\\evil.org",
        ] {
            assert!(key_url(&base, bad, "P").is_err(), "{bad}");
        }
    }

    #[test]
    fn segments_are_encoded_under_the_base_path() {
        let base = Url::parse("http://nas/jellyfin/").unwrap();
        assert_eq!(
            join(&base, &["Items", "a/../b"]).unwrap().as_str(),
            "http://nas/jellyfin/Items/a%2F..%2Fb"
        );
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("1970-01-02T00:00:01.1234567Z"), Some(86_401));
        assert_eq!(parse_time("2026-10-31T10:00:00+01:00"), Some(1_793_437_200));
        assert_eq!(parse_time("2026-10-31T09:00:00Z"), Some(1_793_437_200));
        for bad in ["", "yesterday", "2026-13-01T00:00:00Z", "2026-10-31"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
    }

    // T-SRV-010: found on a real Jellyfin
    #[test]
    fn subtitle_formats_become_extensions() {
        assert_eq!(subtitle_extension("subrip").as_deref(), Some("srt"));
        assert_eq!(subtitle_extension("SRT").as_deref(), Some("srt"));
        assert_eq!(subtitle_extension("WebVTT").as_deref(), Some("vtt"));
        assert_eq!(subtitle_extension("ass").as_deref(), Some("ass"));
        for image in ["pgssub", "dvdsub", "../x", ""] {
            assert_eq!(subtitle_extension(image), None, "{image}");
        }
    }

    // T-SRV-011
    #[test]
    fn the_later_position_wins() {
        assert_eq!(later_position(10, Some(100), 20, Some(200)), 20);
        assert_eq!(later_position(10, Some(200), 20, Some(100)), 10);
        assert_eq!(later_position(10, Some(100), 20, Some(100)), 10);
        assert_eq!(later_position(10, None, 20, Some(100)), 20);
        assert_eq!(later_position(10, Some(100), 20, None), 10);
        assert_eq!(later_position(0, None, 20, None), 0);
    }
}
