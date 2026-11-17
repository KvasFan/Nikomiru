// SPDX-License-Identifier: GPL-3.0-only

//! Jellyfin's HTTP API (SRV-002, SRV-006…SRV-011), and Emby's (SRV-014):
//! the API Jellyfin was forked from, with its own path prefix, headers,
//! per-user routes, server identity and direct-play checks.

use reqwest::Url;
use serde_json::{Value, json};

use super::{
    Item, ItemKind, Library, MediaServer, PAGE, Page, ParentKind, PlayState, Playable,
    ServerSubtitle, Stream, join, parse_time, subtitle_extension, success,
};
use crate::error::OnlineError;
use crate::http::Http;

/// Which of the two servers this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// Jellyfin (SRV-002).
    Jellyfin,
    /// Emby (SRV-014).
    Emby,
}

impl Flavor {
    fn name(self) -> &'static str {
        match self {
            Flavor::Jellyfin => "Jellyfin",
            Flavor::Emby => "Emby",
        }
    }
}
/// Microseconds in tenths: the server's ticks in a millisecond.
const TICKS_PER_MS: u64 = 10_000;
/// The fields [`item_of`] reads that are not sent unless asked.
const FIELDS: &str = "ProviderIds";

/// A Jellyfin or Emby server, logged in.
pub struct Jellyfin {
    http: Http,
    base: Url,
    token: String,
    user_id: String,
    device_id: String,
    flavor: Flavor,
}

impl std::fmt::Debug for Jellyfin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jellyfin")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

fn authorization(flavor: Flavor, device_id: &str, token: Option<&str>) -> String {
    let device: String = device_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let scheme = match flavor {
        Flavor::Jellyfin => "MediaBrowser",
        Flavor::Emby => "Emby",
    };
    let mut value = format!(
        "{scheme} Client=\"Nikomiru\", Device=\"Nikomiru\", DeviceId=\"{device}\", Version=\"{}\"",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(token) = token {
        value.push_str(&format!(", Token=\"{token}\""));
    }
    value
}

fn protocol_of(flavor: Flavor, reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: flavor.name().into(),
        reason: reason.into(),
    }
}

/// The address an Emby answers on: its base with `/emby` (SRV-014).
fn emby_base(base: &Url) -> Result<Url, OnlineError> {
    if base.path().trim_end_matches('/').ends_with("/emby") {
        return Ok(base.clone());
    }
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|()| OnlineError::BadUrl(base.to_string()))?
        .pop_if_empty()
        .push("emby");
    Ok(url)
}

fn number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn item_of(value: &Value) -> Option<Item> {
    let kind = match value["Type"].as_str()? {
        "Series" => ItemKind::Series,
        "Season" => ItemKind::Season,
        "Episode" => ItemKind::Episode,
        "Movie" => ItemKind::Movie,
        _ => return None,
    };
    let id = value["Id"].as_str()?.to_string();
    let user = &value["UserData"];
    Some(Item {
        cover: value["ImageTags"]["Primary"].as_str().map(|_| id.clone()),
        id,
        kind,
        title: value["Name"].as_str().unwrap_or_default().to_string(),
        series: value["SeriesName"].as_str().map(str::to_string),
        parent: value["SeasonId"]
            .as_str()
            .or_else(|| value["ParentId"].as_str())
            .map(str::to_string),
        season: number(&value["ParentIndexNumber"]),
        episode: number(&value["IndexNumber"]),
        played: user["Played"].as_bool().unwrap_or(false),
        position_ms: user["PlaybackPositionTicks"].as_u64().unwrap_or(0) / TICKS_PER_MS,
        duration_ms: value["RunTimeTicks"].as_u64().unwrap_or(0) / TICKS_PER_MS,
        last_played_s: user["LastPlayedDate"].as_str().and_then(parse_time),
        anilist_id: value["ProviderIds"]["AniList"]
            .as_str()
            .and_then(|id| id.parse().ok()),
    })
}

fn page_of(value: &Value) -> Page {
    let items: Vec<Item> = value["Items"]
        .as_array()
        .map(|a| a.iter().filter_map(item_of).collect())
        .unwrap_or_default();
    let total = value["TotalRecordCount"]
        .as_u64()
        .map_or(items.len(), |n| n as usize);
    Page { items, total }
}

impl Jellyfin {
    /// Logs in with a user name and password, which are not kept (SRV-002).
    pub fn login(
        http: Http,
        base: Url,
        user: &str,
        password: &str,
        device_id: &str,
    ) -> Result<Self, OnlineError> {
        Self::login_as(Flavor::Jellyfin, http, base, user, password, device_id)
    }

    /// Logs in to a server of `flavor`. An Emby is first asked what it is:
    /// a Jellyfin answering at its address is refused (SRV-014).
    pub fn login_as(
        flavor: Flavor,
        http: Http,
        base: Url,
        user: &str,
        password: &str,
        device_id: &str,
    ) -> Result<Self, OnlineError> {
        let name = flavor.name();
        let base = match flavor {
            Flavor::Jellyfin => base,
            Flavor::Emby => {
                let base = emby_base(&base)?;
                let info = http.get(&join(&base, &["System", "Info", "Public"])?, &[])?;
                let info = success(info, name)?.json(name)?;
                let product = info["ProductName"].as_str().unwrap_or_default();
                if info["Id"].as_str().is_none_or(str::is_empty)
                    || product.to_ascii_lowercase().contains("jellyfin")
                {
                    return Err(protocol_of(flavor, "not an Emby server"));
                }
                base
            }
        };
        let url = join(&base, &["Users", "AuthenticateByName"])?;
        let header = authorization(flavor, device_id, None);
        let header_name = match flavor {
            Flavor::Jellyfin => "Authorization",
            Flavor::Emby => "X-Emby-Authorization",
        };
        let answer = http.post_json(
            &url,
            &[(header_name, &header)],
            &json!({ "Username": user, "Pw": password }),
        )?;
        let answer = success(answer, name)?.json(name)?;
        let token = answer["AccessToken"].as_str();
        let user_id = answer["User"]["Id"].as_str();
        let (Some(token), Some(user_id)) = (token, user_id) else {
            return Err(protocol_of(flavor, "no token in the answer"));
        };
        let mut server = Self::new(http, base, token, user_id, device_id);
        server.flavor = flavor;
        Ok(server)
    }

    /// A session already logged in.
    pub fn new(http: Http, base: Url, token: &str, user_id: &str, device_id: &str) -> Self {
        Self {
            http,
            base,
            token: token.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            flavor: Flavor::Jellyfin,
        }
    }

    /// An Emby session already logged in (SRV-014).
    pub fn new_emby(
        http: Http,
        base: Url,
        token: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<Self, OnlineError> {
        let mut server = Self::new(http, emby_base(&base)?, token, user_id, device_id);
        server.flavor = Flavor::Emby;
        Ok(server)
    }

    fn name(&self) -> &'static str {
        self.flavor.name()
    }

    fn protocol(&self, reason: &str) -> OnlineError {
        protocol_of(self.flavor, reason)
    }

    /// The headers of a request: Jellyfin's `Authorization`, or Emby's own.
    fn headers(&self) -> Vec<(&'static str, String)> {
        let header = authorization(self.flavor, &self.device_id, Some(&self.token));
        match self.flavor {
            Flavor::Jellyfin => vec![("Authorization", header)],
            Flavor::Emby => vec![
                ("X-Emby-Authorization", header),
                ("X-Emby-Token", self.token.clone()),
            ],
        }
    }

    /// A per-user route: Emby keeps the user in the path.
    fn user_path<'a>(&'a self, jellyfin: &[&'a str], emby: &[&'a str]) -> Vec<&'a str> {
        match self.flavor {
            Flavor::Jellyfin => jellyfin.to_vec(),
            Flavor::Emby => {
                let mut path = vec!["Users", self.user_id.as_str()];
                path.extend_from_slice(emby);
                path
            }
        }
    }

    /// The access token.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The user's id.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    fn header_refs<'a>(headers: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
        headers.iter().map(|(k, v)| (*k, v.as_str())).collect()
    }

    fn get(&self, url: &Url) -> Result<Value, OnlineError> {
        let headers = self.headers();
        let answer = self.http.get(url, &Self::header_refs(&headers))?;
        success(answer, self.name())?.json(self.name())
    }

    fn post(&self, url: &Url, body: &Value) -> Result<(), OnlineError> {
        let headers = self.headers();
        let answer = self
            .http
            .post_json(url, &Self::header_refs(&headers), body)?;
        success(answer, self.name()).map(|_| ())
    }

    fn items(&self, query: &[(&str, &str)], start: usize) -> Result<Page, OnlineError> {
        let mut url = join(&self.base, &self.user_path(&["Items"], &["Items"]))?;
        if self.flavor == Flavor::Jellyfin {
            url.query_pairs_mut().append_pair("userId", &self.user_id);
        }
        url.query_pairs_mut()
            .append_pair("StartIndex", &start.to_string())
            .append_pair("Limit", &PAGE.to_string())
            .append_pair("Fields", FIELDS)
            .extend_pairs(query);
        Ok(page_of(&self.get(&url)?))
    }

    fn item_json(&self, id: &str) -> Result<Value, OnlineError> {
        let mut url = join(&self.base, &self.user_path(&["Items", id], &["Items", id]))?;
        if self.flavor == Flavor::Jellyfin {
            url.query_pairs_mut().append_pair("userId", &self.user_id);
        }
        url.query_pairs_mut()
            .append_pair("Fields", "ProviderIds,MediaSources");
        self.get(&url)
    }
}

impl MediaServer for Jellyfin {
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError> {
        let mut url = join(&self.base, &self.user_path(&["UserViews"], &["Views"]))?;
        if self.flavor == Flavor::Jellyfin {
            url.query_pairs_mut().append_pair("userId", &self.user_id);
        }
        let answer = self.get(&url)?;
        let wanted = |v: &&Value| {
            matches!(
                v["CollectionType"].as_str(),
                Some("tvshows" | "movies" | "mixed") | None
            )
        };
        Ok(answer["Items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(wanted)
                    .filter_map(|v| {
                        Some(Library {
                            id: v["Id"].as_str()?.to_string(),
                            title: v["Name"].as_str().unwrap_or_default().to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn children(
        &mut self,
        parent: &str,
        start: usize,
        _kind: ParentKind,
    ) -> Result<Page, OnlineError> {
        self.items(
            &[("ParentId", parent), ("SortBy", "SortName,IndexNumber")],
            start,
        )
    }

    fn search(&mut self, library: &str, query: &str, start: usize) -> Result<Page, OnlineError> {
        self.items(
            &[
                ("ParentId", library),
                ("searchTerm", query),
                ("Recursive", "true"),
                ("IncludeItemTypes", "Series,Movie"),
            ],
            start,
        )
    }

    fn item(&mut self, id: &str) -> Result<Item, OnlineError> {
        item_of(&self.item_json(id)?).ok_or_else(|| self.protocol("not a video"))
    }

    fn playable(&mut self, id: &str) -> Result<Playable, OnlineError> {
        let answer = self.item_json(id)?;
        let source = &answer["MediaSources"][0];
        // SRV-015: an Emby says whether the file can be played as it is
        if self.flavor == Flavor::Emby
            && source["SupportsDirectPlay"] != true
            && source["SupportsDirectStream"] != true
        {
            return Err(self.protocol("the file cannot be played directly"));
        }
        let source_id = source["Id"].as_str().unwrap_or(id).to_string();
        let mut url = join(&self.base, &["Videos", id, "stream"])?;
        url.query_pairs_mut()
            .append_pair("static", "true")
            .append_pair("mediaSourceId", &source_id);
        let subtitles = source["MediaStreams"]
            .as_array()
            .map(|streams| {
                streams
                    .iter()
                    .filter(|s| {
                        s["Type"] == "Subtitle"
                            && s["IsExternal"] == true
                            && s["IsTextSubtitleStream"] == true
                    })
                    .filter_map(|s| subtitle_of(s, id, &source_id))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Playable {
            stream: Stream {
                url,
                headers: self
                    .headers()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            },
            source: source_id,
            subtitles,
        })
    }

    fn subtitle(&mut self, subtitle: &ServerSubtitle) -> Result<Vec<u8>, OnlineError> {
        let mut parts = subtitle.id.splitn(3, '/');
        let (Some(video), Some(source), Some(index)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(self.protocol("a bad subtitle"));
        };
        let file = format!("Stream.{}", subtitle.format);
        let url = join(
            &self.base,
            &["Videos", video, source, "Subtitles", index, &file],
        )?;
        let headers = self.headers();
        let answer = self.http.get(&url, &Self::header_refs(&headers))?;
        Ok(success(answer, self.name())?.body)
    }

    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError> {
        let mut url = join(&self.base, &["Items", cover, "Images", "Primary"])?;
        url.query_pairs_mut().append_pair("maxHeight", "480");
        let headers = self.headers();
        let answer = self.http.get(&url, &Self::header_refs(&headers))?;
        Ok(success(answer, self.name())?.body)
    }

    fn report(
        &mut self,
        id: &str,
        source: &str,
        state: PlayState,
        position_ms: u64,
        _duration_ms: u64,
    ) -> Result<(), OnlineError> {
        let path: &[&str] = match state {
            PlayState::Started => &["Sessions", "Playing"],
            PlayState::Playing | PlayState::Paused => &["Sessions", "Playing", "Progress"],
            PlayState::Stopped => &["Sessions", "Playing", "Stopped"],
        };
        self.post(
            &join(&self.base, path)?,
            &json!({
                "ItemId": id,
                "MediaSourceId": source,
                "PositionTicks": position_ms * TICKS_PER_MS,
                "IsPaused": state == PlayState::Paused,
                "PlayMethod": "DirectPlay",
            }),
        )
    }

    fn mark_played(&mut self, id: &str) -> Result<(), OnlineError> {
        let mut url = join(
            &self.base,
            &self.user_path(&["UserPlayedItems", id], &["PlayedItems", id]),
        )?;
        if self.flavor == Flavor::Jellyfin {
            url.query_pairs_mut().append_pair("userId", &self.user_id);
        }
        self.post(&url, &json!({}))
    }

    fn logout(&mut self) -> Result<(), OnlineError> {
        self.post(&join(&self.base, &["Sessions", "Logout"])?, &json!({}))
    }
}

fn subtitle_of(stream: &Value, video: &str, source: &str) -> Option<ServerSubtitle> {
    let format = subtitle_extension(stream["Codec"].as_str()?)?;
    let index = stream["Index"].as_u64()?;
    Some(ServerSubtitle {
        id: format!("{video}/{source}/{index}"),
        language: stream["Language"].as_str().map(str::to_string),
        title: stream["DisplayTitle"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        format,
    })
}
