// SPDX-License-Identifier: GPL-3.0-only

//! Jellyfin's HTTP API (SRV-002, SRV-006…SRV-011).

use reqwest::Url;
use serde_json::{Value, json};

use super::{
    Item, ItemKind, Library, MediaServer, PAGE, Page, ParentKind, PlayState, Playable,
    ServerSubtitle, Stream, join, parse_time, success,
};
use crate::error::OnlineError;
use crate::http::Http;

const NAME: &str = "Jellyfin";
/// Microseconds in tenths: the server's ticks in a millisecond.
const TICKS_PER_MS: u64 = 10_000;
/// The fields [`item_of`] reads that are not sent unless asked.
const FIELDS: &str = "ProviderIds";

/// A Jellyfin server, logged in.
pub struct Jellyfin {
    http: Http,
    base: Url,
    token: String,
    user_id: String,
    device_id: String,
}

impl std::fmt::Debug for Jellyfin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Jellyfin")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

fn authorization(device_id: &str, token: Option<&str>) -> String {
    let device: String = device_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let mut value = format!(
        "MediaBrowser Client=\"Nikomiru\", Device=\"Nikomiru\", DeviceId=\"{device}\", Version=\"{}\"",
        env!("CARGO_PKG_VERSION")
    );
    if let Some(token) = token {
        value.push_str(&format!(", Token=\"{token}\""));
    }
    value
}

fn protocol(reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: NAME.into(),
        reason: reason.into(),
    }
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
        let url = join(&base, &["Users", "AuthenticateByName"])?;
        let header = authorization(device_id, None);
        let answer = http.post_json(
            &url,
            &[("Authorization", &header)],
            &json!({ "Username": user, "Pw": password }),
        )?;
        let answer = success(answer, NAME)?.json(NAME)?;
        let token = answer["AccessToken"].as_str();
        let user_id = answer["User"]["Id"].as_str();
        let (Some(token), Some(user_id)) = (token, user_id) else {
            return Err(protocol("no token in the answer"));
        };
        Ok(Self::new(http, base, token, user_id, device_id))
    }

    /// A session already logged in.
    pub fn new(http: Http, base: Url, token: &str, user_id: &str, device_id: &str) -> Self {
        Self {
            http,
            base,
            token: token.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
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

    fn header(&self) -> String {
        authorization(&self.device_id, Some(&self.token))
    }

    fn get(&self, url: &Url) -> Result<Value, OnlineError> {
        let answer = self.http.get(url, &[("Authorization", &self.header())])?;
        success(answer, NAME)?.json(NAME)
    }

    fn post(&self, url: &Url, body: &Value) -> Result<(), OnlineError> {
        let answer = self
            .http
            .post_json(url, &[("Authorization", &self.header())], body)?;
        success(answer, NAME).map(|_| ())
    }

    fn items(&self, query: &[(&str, &str)], start: usize) -> Result<Page, OnlineError> {
        let mut url = join(&self.base, &["Items"])?;
        url.query_pairs_mut()
            .append_pair("userId", &self.user_id)
            .append_pair("StartIndex", &start.to_string())
            .append_pair("Limit", &PAGE.to_string())
            .append_pair("Fields", FIELDS)
            .extend_pairs(query);
        Ok(page_of(&self.get(&url)?))
    }

    fn item_json(&self, id: &str) -> Result<Value, OnlineError> {
        let mut url = join(&self.base, &["Items", id])?;
        url.query_pairs_mut()
            .append_pair("userId", &self.user_id)
            .append_pair("Fields", "ProviderIds,MediaSources");
        self.get(&url)
    }
}

impl MediaServer for Jellyfin {
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError> {
        let mut url = join(&self.base, &["UserViews"])?;
        url.query_pairs_mut().append_pair("userId", &self.user_id);
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
        item_of(&self.item_json(id)?).ok_or_else(|| protocol("not a video"))
    }

    fn playable(&mut self, id: &str) -> Result<Playable, OnlineError> {
        let answer = self.item_json(id)?;
        let source = &answer["MediaSources"][0];
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
                headers: vec![("Authorization".into(), self.header())],
            },
            source: source_id,
            subtitles,
        })
    }

    fn subtitle(&mut self, subtitle: &ServerSubtitle) -> Result<Vec<u8>, OnlineError> {
        let mut parts = subtitle.id.splitn(3, '/');
        let (Some(video), Some(source), Some(index)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(protocol("a bad subtitle"));
        };
        let file = format!("Stream.{}", subtitle.format);
        let url = join(
            &self.base,
            &["Videos", video, source, "Subtitles", index, &file],
        )?;
        let answer = self.http.get(&url, &[("Authorization", &self.header())])?;
        Ok(success(answer, NAME)?.body)
    }

    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError> {
        let mut url = join(&self.base, &["Items", cover, "Images", "Primary"])?;
        url.query_pairs_mut().append_pair("maxHeight", "480");
        let answer = self.http.get(&url, &[("Authorization", &self.header())])?;
        Ok(success(answer, NAME)?.body)
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
        let mut url = join(&self.base, &["UserPlayedItems", id])?;
        url.query_pairs_mut().append_pair("userId", &self.user_id);
        self.post(&url, &json!({}))
    }

    fn logout(&mut self) -> Result<(), OnlineError> {
        self.post(&join(&self.base, &["Sessions", "Logout"])?, &json!({}))
    }
}

fn subtitle_of(stream: &Value, video: &str, source: &str) -> Option<ServerSubtitle> {
    let format: String = stream["Codec"]
        .as_str()?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if format.is_empty() {
        return None;
    }
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
