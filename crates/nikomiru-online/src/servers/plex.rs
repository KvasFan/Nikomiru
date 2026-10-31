// SPDX-License-Identifier: GPL-3.0-only

//! Plex: the plex.tv PIN login (SRV-003) and a server's HTTP API
//! (SRV-006…SRV-011).

use reqwest::Url;
use serde_json::{Value, json};

use super::{
    Item, ItemKind, Library, MediaServer, PAGE, Page, ParentKind, PlayState, Playable,
    ServerSubtitle, Stream, join, key_url, success,
};
use crate::error::OnlineError;
use crate::http::Http;

const NAME: &str = "Plex";
/// The real plex.tv.
pub const PLEX_TV: &str = "https://plex.tv/";
/// The sign-in page of the PIN flow.
pub const SIGN_IN: &str = "https://app.plex.tv/auth#";

fn protocol(reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: NAME.into(),
        reason: reason.into(),
    }
}

fn identity(client: &str) -> [(&'static str, &str); 3] {
    [
        ("X-Plex-Client-Identifier", client),
        ("X-Plex-Product", "Nikomiru"),
        ("Accept", "application/json"),
    ]
}

/// A PIN waiting for its sign-in (SRV-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// Its id.
    pub id: u64,
    /// The code the sign-in page takes.
    pub code: String,
}

/// A server of an account (SRV-003).
#[derive(Clone, PartialEq, Eq)]
pub struct Resource {
    /// Its name.
    pub name: String,
    /// Its token.
    pub token: String,
    /// Its addresses, local ones first.
    pub addresses: Vec<String>,
}

impl std::fmt::Debug for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resource")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// plex.tv, for the login.
#[derive(Debug, Clone)]
pub struct PlexTv {
    http: Http,
    base: Url,
    client: String,
}

impl PlexTv {
    /// plex.tv at `base`, seen as device `client`.
    pub fn new(http: Http, base: Url, client: &str) -> Self {
        Self {
            http,
            base,
            client: client.to_string(),
        }
    }

    /// Asks for a PIN.
    pub fn pin(&self) -> Result<Pin, OnlineError> {
        let mut url = join(&self.base, &["api", "v2", "pins"])?;
        url.query_pairs_mut().append_pair("strong", "true");
        let answer = self
            .http
            .post_json(&url, &identity(&self.client), &json!({}))?;
        let answer = success(answer, NAME)?.json(NAME)?;
        match (answer["id"].as_u64(), answer["code"].as_str()) {
            (Some(id), Some(code)) => Ok(Pin {
                id,
                code: code.to_string(),
            }),
            _ => Err(protocol("no PIN in the answer")),
        }
    }

    /// The sign-in page for `pin`, to open in the browser.
    pub fn sign_in_url(&self, pin: &Pin) -> String {
        let query = Url::parse_with_params(
            "http://x/",
            [
                ("clientID", self.client.as_str()),
                ("code", pin.code.as_str()),
                ("context[device][product]", "Nikomiru"),
            ],
        )
        .ok()
        .and_then(|u| u.query().map(str::to_string))
        .unwrap_or_default();
        format!("{SIGN_IN}?{query}")
    }

    /// The account's token, once the user has signed in.
    pub fn token(&self, pin: &Pin) -> Result<Option<String>, OnlineError> {
        let url = join(&self.base, &["api", "v2", "pins", &pin.id.to_string()])?;
        let answer = self.http.get(&url, &identity(&self.client))?;
        let answer = success(answer, NAME)?.json(NAME)?;
        Ok(answer["authToken"]
            .as_str()
            .filter(|t| !t.is_empty())
            .map(str::to_string))
    }

    /// The account's servers.
    pub fn resources(&self, token: &str) -> Result<Vec<Resource>, OnlineError> {
        let mut url = join(&self.base, &["api", "v2", "resources"])?;
        url.query_pairs_mut().append_pair("includeHttps", "1");
        let mut headers = identity(&self.client).to_vec();
        headers.push(("X-Plex-Token", token));
        let answer = success(self.http.get(&url, &headers)?, NAME)?.json(NAME)?;
        let Some(list) = answer.as_array() else {
            return Err(protocol("no list of servers"));
        };
        Ok(list
            .iter()
            .filter(|r| {
                r["provides"]
                    .as_str()
                    .is_some_and(|p| p.split(',').any(|p| p == "server"))
            })
            .map(|r| resource_of(r, token))
            .collect())
    }
}

fn resource_of(value: &Value, account_token: &str) -> Resource {
    let mut connections: Vec<&Value> = value["connections"]
        .as_array()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    // local first, the relays of plex.tv last
    connections.sort_by_key(|c| (c["relay"] == true, c["local"] != true));
    Resource {
        name: value["name"].as_str().unwrap_or_default().to_string(),
        token: value["accessToken"]
            .as_str()
            .unwrap_or(account_token)
            .to_string(),
        addresses: connections
            .iter()
            .filter_map(|c| c["uri"].as_str().map(str::to_string))
            .collect(),
    }
}

/// A Plex server, logged in.
pub struct Plex {
    http: Http,
    base: Url,
    token: String,
    client: String,
}

impl std::fmt::Debug for Plex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plex")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

fn number(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn item_of(value: &Value) -> Option<Item> {
    let kind = match value["type"].as_str()? {
        "show" => ItemKind::Series,
        "season" => ItemKind::Season,
        "episode" => ItemKind::Episode,
        "movie" => ItemKind::Movie,
        _ => return None,
    };
    let series = match kind {
        ItemKind::Season => value["parentTitle"].as_str(),
        _ => value["grandparentTitle"].as_str(),
    };
    Some(Item {
        id: value["ratingKey"].as_str()?.to_string(),
        kind,
        title: value["title"].as_str().unwrap_or_default().to_string(),
        series: series.map(str::to_string),
        season: number(&value["parentIndex"]),
        episode: number(&value["index"]),
        cover: value["thumb"].as_str().map(str::to_string),
        played: value["viewCount"].as_u64().unwrap_or(0) > 0,
        position_ms: value["viewOffset"].as_u64().unwrap_or(0),
        duration_ms: value["duration"].as_u64().unwrap_or(0),
        last_played_s: value["lastViewedAt"].as_i64(),
        anilist_id: None,
    })
}

fn page_of(value: &Value) -> Page {
    let container = &value["MediaContainer"];
    let items: Vec<Item> = container["Metadata"]
        .as_array()
        .map(|a| a.iter().filter_map(item_of).collect())
        .unwrap_or_default();
    let total = container["totalSize"]
        .as_u64()
        .or_else(|| container["size"].as_u64())
        .map_or(items.len(), |n| n as usize);
    Page { items, total }
}

impl Plex {
    /// A server at `base`, with its token.
    pub fn new(http: Http, base: Url, token: &str, client: &str) -> Self {
        Self {
            http,
            base,
            token: token.to_string(),
            client: client.to_string(),
        }
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        let mut headers = identity(&self.client).to_vec();
        headers.push(("X-Plex-Token", &self.token));
        headers
    }

    fn get(&self, url: &Url) -> Result<Value, OnlineError> {
        let answer = self.http.get(url, &self.headers())?;
        success(answer, NAME)?.json(NAME)
    }

    fn page(&self, mut url: Url, start: usize) -> Result<Page, OnlineError> {
        url.query_pairs_mut()
            .append_pair("X-Plex-Container-Start", &start.to_string())
            .append_pair("X-Plex-Container-Size", &PAGE.to_string());
        Ok(page_of(&self.get(&url)?))
    }

    fn metadata(&self, id: &str) -> Result<Value, OnlineError> {
        let url = join(&self.base, &["library", "metadata", id])?;
        let answer = self.get(&url)?;
        answer["MediaContainer"]["Metadata"]
            .get(0)
            .cloned()
            .ok_or_else(|| protocol("no such item"))
    }
}

impl MediaServer for Plex {
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError> {
        let answer = self.get(&join(&self.base, &["library", "sections"])?)?;
        Ok(answer["MediaContainer"]["Directory"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|d| matches!(d["type"].as_str(), Some("show" | "movie")))
                    .filter_map(|d| {
                        Some(Library {
                            id: d["key"].as_str()?.to_string(),
                            title: d["title"].as_str().unwrap_or_default().to_string(),
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
        kind: ParentKind,
    ) -> Result<Page, OnlineError> {
        let url = match kind {
            ParentKind::Library => join(&self.base, &["library", "sections", parent, "all"])?,
            ParentKind::Item => join(&self.base, &["library", "metadata", parent, "children"])?,
        };
        self.page(url, start)
    }

    fn search(&mut self, library: &str, query: &str, start: usize) -> Result<Page, OnlineError> {
        let mut url = join(&self.base, &["library", "sections", library, "all"])?;
        url.query_pairs_mut().append_pair("title", query);
        self.page(url, start)
    }

    fn item(&mut self, id: &str) -> Result<Item, OnlineError> {
        item_of(&self.metadata(id)?).ok_or_else(|| protocol("not a video"))
    }

    fn playable(&mut self, id: &str) -> Result<Playable, OnlineError> {
        let item = self.metadata(id)?;
        let part = &item["Media"][0]["Part"][0];
        let key = part["key"]
            .as_str()
            .ok_or_else(|| protocol("no file in the answer"))?;
        let subtitles = part["Stream"]
            .as_array()
            .map(|streams| streams.iter().filter_map(subtitle_of).collect())
            .unwrap_or_default();
        let headers = self
            .headers()
            .iter()
            .filter(|(k, _)| *k != "Accept")
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Ok(Playable {
            stream: Stream {
                url: key_url(&self.base, key, NAME)?,
                headers,
            },
            source: String::new(),
            subtitles,
        })
    }

    fn subtitle(&mut self, subtitle: &ServerSubtitle) -> Result<Vec<u8>, OnlineError> {
        let url = key_url(&self.base, &subtitle.id, NAME)?;
        let answer = self.http.get(&url, &self.headers())?;
        Ok(success(answer, NAME)?.body)
    }

    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError> {
        let url = key_url(&self.base, cover, NAME)?;
        let answer = self.http.get(&url, &self.headers())?;
        Ok(success(answer, NAME)?.body)
    }

    fn report(
        &mut self,
        id: &str,
        _source: &str,
        state: PlayState,
        position_ms: u64,
        duration_ms: u64,
    ) -> Result<(), OnlineError> {
        let state = match state {
            PlayState::Started | PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Stopped => "stopped",
        };
        let mut url = join(&self.base, &[":", "timeline"])?;
        url.query_pairs_mut()
            .append_pair("ratingKey", id)
            .append_pair("key", &format!("/library/metadata/{id}"))
            .append_pair("state", state)
            .append_pair("time", &position_ms.to_string())
            .append_pair("duration", &duration_ms.to_string());
        success(self.http.get(&url, &self.headers())?, NAME).map(|_| ())
    }

    fn mark_played(&mut self, id: &str) -> Result<(), OnlineError> {
        let mut url = join(&self.base, &[":", "scrobble"])?;
        url.query_pairs_mut()
            .append_pair("key", id)
            .append_pair("identifier", "com.plexapp.plugins.library");
        success(self.http.get(&url, &self.headers())?, NAME).map(|_| ())
    }

    fn logout(&mut self) -> Result<(), OnlineError> {
        Ok(())
    }
}

fn subtitle_of(stream: &Value) -> Option<ServerSubtitle> {
    if stream["streamType"].as_u64()? != 3 {
        return None;
    }
    // only external ones have a `key`: embedded ones come with the file
    let id = stream["key"].as_str()?.to_string();
    let format: String = stream["codec"]
        .as_str()?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if format.is_empty() || matches!(format.as_str(), "pgs" | "dvdsub" | "vobsub") {
        return None;
    }
    Some(ServerSubtitle {
        id,
        language: stream["languageCode"].as_str().map(str::to_string),
        title: stream["displayTitle"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        format,
    })
}
