// SPDX-License-Identifier: GPL-3.0-only

//! Series metadata (MET-001, MET-002): AniList (GraphQL, no key) or Jikan
//! v4 (MyAnimeList) for the titles, the episodes, the cover and the ids of a
//! series of the library. A series is identified by its title: accepted by
//! itself only when one result has exactly that title, otherwise the user
//! chooses. Nothing goes online unless asked for (PRV-001).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use nikomiru_core::Clock;
use reqwest::Url;
use serde_json::{Value, json};

use crate::error::OnlineError;
use crate::http::{Http, checked_url};

/// The real AniList.
pub const ANILIST: &str = "https://graphql.anilist.co";
/// The real Jikan.
pub const JIKAN: &str = "https://api.jikan.moe/v4";

/// The results asked for a title.
const RESULTS: usize = 10;

/// Where metadata come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// AniList (MET-001).
    AniList,
    /// Jikan, MyAnimeList's data (MET-002).
    Jikan,
}

impl Source {
    /// Its name in `config.toml` (`[online] metadata`).
    pub fn key(self) -> &'static str {
        match self {
            Self::AniList => "anilist",
            Self::Jikan => "jikan",
        }
    }

    /// The source of a name; AniList for any other.
    pub fn from_key(key: &str) -> Self {
        if key == "jikan" {
            Self::Jikan
        } else {
            Self::AniList
        }
    }

    /// Its name for people.
    pub fn name(self) -> &'static str {
        match self {
            Self::AniList => "AniList",
            Self::Jikan => "MyAnimeList (Jikan)",
        }
    }
}

/// What a source knows of a series.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeriesInfo {
    /// Its AniList id.
    pub anilist_id: Option<u64>,
    /// Its MyAnimeList id.
    pub mal_id: Option<u64>,
    /// Its title in romaji.
    pub romaji: String,
    /// In English.
    pub english: Option<String>,
    /// In Japanese.
    pub native: Option<String>,
    /// Other titles.
    pub synonyms: Vec<String>,
    /// Its episodes, once known.
    pub episodes: Option<u32>,
    /// The year it started.
    pub year: Option<u32>,
    /// TV, movie, OVA…
    pub format: Option<String>,
    /// Its cover image.
    pub cover_url: Option<String>,
    /// Its page on the source.
    pub page_url: Option<String>,
}

impl SeriesInfo {
    /// Every title it has.
    pub fn titles(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.romaji.as_str())
            .chain(self.english.as_deref())
            .chain(self.native.as_deref())
            .chain(self.synonyms.iter().map(String::as_str))
    }
}

/// A title as it is compared: letters and digits only, in lower case.
pub fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// What a search found for a title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identified {
    /// One result has exactly that title: it is the series.
    Unique(SeriesInfo),
    /// The user chooses among these.
    Choose(Vec<SeriesInfo>),
    /// Nothing.
    Nothing,
}

/// The series of `title` among `found` (MET-001): the result with exactly
/// that title, if only one has it; otherwise the user chooses.
pub fn identify(title: &str, found: Vec<SeriesInfo>) -> Identified {
    if found.is_empty() {
        return Identified::Nothing;
    }
    let key = normalize(title);
    let exact: Vec<&SeriesInfo> = found
        .iter()
        .filter(|i| !key.is_empty() && i.titles().any(|t| normalize(t) == key))
        .collect();
    match exact.as_slice() {
        [one] => Identified::Unique((*one).clone()),
        _ => Identified::Choose(found),
    }
}

/// A source of series metadata.
pub trait MetadataSource: Send {
    /// Which one.
    fn source(&self) -> Source;
    /// The series it finds for `title`, the best first.
    fn search(&mut self, title: &str) -> Result<Vec<SeriesInfo>, OnlineError>;
}

fn protocol(provider: &str, reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: provider.into(),
        reason: reason.into(),
    }
}

fn text(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn number(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

const ANILIST_NAME: &str = "AniList";

const QUERY: &str = "query ($search: String, $perPage: Int) { Page(perPage: $perPage) { \
     media(search: $search, type: ANIME) { id idMal format episodes seasonYear \
     title { romaji english native } synonyms coverImage { large } siteUrl } } }";

/// The series of an AniList answer; malformed ones are skipped.
pub fn parse_anilist(json: &Value) -> Result<Vec<SeriesInfo>, OnlineError> {
    if let Some(errors) = json.get("errors").filter(|e| !e.is_null()) {
        let reason = errors[0]["message"].as_str().unwrap_or("error");
        return Err(protocol(ANILIST_NAME, reason));
    }
    let Some(media) = json["data"]["Page"]["media"].as_array() else {
        return Err(protocol(ANILIST_NAME, "no media list"));
    };
    Ok(media
        .iter()
        .filter_map(|m| {
            let title = &m["title"];
            let english = text(&title["english"]);
            let native = text(&title["native"]);
            let romaji = text(&title["romaji"]).or_else(|| english.clone())?;
            Some(SeriesInfo {
                anilist_id: Some(m["id"].as_u64()?),
                mal_id: m["idMal"].as_u64(),
                romaji,
                english,
                native,
                synonyms: m["synonyms"]
                    .as_array()
                    .map(|s| s.iter().filter_map(text).collect())
                    .unwrap_or_default(),
                episodes: number(&m["episodes"]),
                year: number(&m["seasonYear"]),
                format: text(&m["format"]),
                cover_url: text(&m["coverImage"]["large"]),
                page_url: text(&m["siteUrl"]),
            })
        })
        .collect())
}

/// An AniList client (MET-001).
#[derive(Debug)]
pub struct AniList {
    http: Http,
    url: Url,
    cache: HashMap<String, Vec<SeriesInfo>>,
}

impl AniList {
    /// A client of the AniList at `url`.
    pub fn new(http: Http, url: &str) -> Result<Self, OnlineError> {
        Ok(Self {
            http,
            url: checked_url(url)?,
            cache: HashMap::new(),
        })
    }
}

impl MetadataSource for AniList {
    fn source(&self) -> Source {
        Source::AniList
    }

    fn search(&mut self, title: &str) -> Result<Vec<SeriesInfo>, OnlineError> {
        let key = normalize(title);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(found) = self.cache.get(&key) {
            return Ok(found.clone());
        }
        let body = json!({
            "query": QUERY,
            "variables": { "search": title.trim(), "perPage": RESULTS },
        });
        let answer = self
            .http
            .post_json(&self.url, &[("Accept", "application/json")], &body)?
            .ok(ANILIST_NAME)?;
        let found = parse_anilist(&answer.json(ANILIST_NAME)?)?;
        self.cache.insert(key, found.clone());
        Ok(found)
    }
}

const JIKAN_NAME: &str = "Jikan";

/// The series of a Jikan answer; malformed ones are skipped.
pub fn parse_jikan(json: &Value) -> Result<Vec<SeriesInfo>, OnlineError> {
    let Some(data) = json["data"].as_array() else {
        return Err(protocol(JIKAN_NAME, "no data list"));
    };
    Ok(data
        .iter()
        .filter_map(|a| {
            let image = &a["images"]["jpg"];
            Some(SeriesInfo {
                anilist_id: None,
                mal_id: Some(a["mal_id"].as_u64()?),
                romaji: text(&a["title"])?,
                english: text(&a["title_english"]),
                native: text(&a["title_japanese"]),
                synonyms: a["title_synonyms"]
                    .as_array()
                    .map(|s| s.iter().filter_map(text).collect())
                    .unwrap_or_default(),
                episodes: number(&a["episodes"]),
                year: number(&a["year"]),
                format: text(&a["type"]),
                cover_url: text(&image["large_image_url"]).or_else(|| text(&image["image_url"])),
                page_url: text(&a["url"]),
            })
        })
        .collect())
}

/// Requests at most one a second, none during a pause (MET-002).
pub struct Limiter {
    clock: Arc<dyn Clock>,
    sleep: Box<dyn Fn(Duration) + Send + Sync>,
    /// When the next request may go; the end of a pause.
    next: Mutex<(Duration, Option<Duration>)>,
}

impl std::fmt::Debug for Limiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Limiter").finish_non_exhaustive()
    }
}

/// The time between two requests to Jikan.
pub const JIKAN_INTERVAL: Duration = Duration::from_secs(1);

impl Limiter {
    /// A limiter on `clock`, which waits with `sleep`.
    pub fn new(clock: Arc<dyn Clock>, sleep: Box<dyn Fn(Duration) + Send + Sync>) -> Self {
        Self {
            clock,
            sleep,
            next: Mutex::new((Duration::ZERO, None)),
        }
    }

    /// On the real clock.
    pub fn real() -> Self {
        Self::new(
            Arc::new(nikomiru_core::SystemClock::new()),
            Box::new(std::thread::sleep),
        )
    }

    /// Waits for the turn of a request; during a pause, the seconds left.
    fn turn(&self) -> Result<(), u64> {
        let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
        let now = self.clock.monotonic();
        if let Some(end) = next.1 {
            if now < end {
                return Err((end - now).as_secs().max(1));
            }
            next.1 = None;
        }
        if next.0 > now {
            (self.sleep)(next.0 - now);
        }
        next.0 = next.0.max(now) + JIKAN_INTERVAL;
        Ok(())
    }

    /// No request for `seconds` (after a 429).
    fn pause(&self, seconds: u64) {
        let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
        next.1 = Some(self.clock.monotonic() + Duration::from_secs(seconds));
    }
}

/// A Jikan client (MET-002): one request a second, a pause after a 429, a
/// cache of what it found.
#[derive(Debug)]
pub struct Jikan {
    http: Http,
    base: Url,
    limiter: Limiter,
    cache: HashMap<String, Vec<SeriesInfo>>,
}

impl Jikan {
    /// A client of the Jikan at `base`, its requests spaced by `limiter`.
    pub fn new(http: Http, base: &str, limiter: Limiter) -> Result<Self, OnlineError> {
        Ok(Self {
            http,
            base: checked_url(base)?,
            limiter,
            cache: HashMap::new(),
        })
    }
}

impl MetadataSource for Jikan {
    fn source(&self) -> Source {
        Source::Jikan
    }

    fn search(&mut self, title: &str) -> Result<Vec<SeriesInfo>, OnlineError> {
        let key = normalize(title);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(found) = self.cache.get(&key) {
            return Ok(found.clone());
        }
        self.limiter
            .turn()
            .map_err(|seconds| OnlineError::RateLimited {
                provider: JIKAN_NAME.into(),
                seconds,
            })?;
        let base = self.base.as_str().trim_end_matches('/');
        let mut url = Url::parse(&format!("{base}/anime"))
            .map_err(|_| OnlineError::BadUrl(format!("{base}/anime")))?;
        url.query_pairs_mut()
            .append_pair("q", title.trim())
            .append_pair("limit", &RESULTS.to_string());
        let answer = self.http.get(&url, &[("Accept", "application/json")])?;
        let answer = answer.ok(JIKAN_NAME).inspect_err(|e| {
            if let OnlineError::RateLimited { seconds, .. } = e {
                self.limiter.pause(*seconds);
            }
        })?;
        let found = parse_jikan(&answer.json(JIKAN_NAME)?)?;
        self.cache.insert(key, found.clone());
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(romaji: &str, english: Option<&str>) -> SeriesInfo {
        SeriesInfo {
            romaji: romaji.into(),
            english: english.map(str::to_string),
            ..SeriesInfo::default()
        }
    }

    // T-MET-001: accepted by itself only when exact and unique
    #[test]
    fn a_series_is_identified_only_by_an_exact_unique_title() {
        let frieren = info("Sousou no Frieren", Some("Frieren: Beyond Journey's End"));
        let second = info("Sousou no Frieren 2nd Season", None);
        let found = vec![frieren.clone(), second.clone()];
        assert_eq!(
            identify("sousou no FRIEREN", found.clone()),
            Identified::Unique(frieren.clone())
        );
        assert_eq!(
            identify("Frieren - Beyond Journey's End", found.clone()),
            Identified::Unique(frieren.clone())
        );
        // close is not exact
        assert_eq!(
            identify("Frieren", found.clone()),
            Identified::Choose(found.clone())
        );
        // two with the same title: the user chooses
        let twice = vec![frieren.clone(), frieren.clone()];
        assert_eq!(
            identify("Sousou no Frieren", twice.clone()),
            Identified::Choose(twice)
        );
        assert_eq!(identify("x", Vec::new()), Identified::Nothing);
        assert_eq!(normalize("Re:Zero − Kara"), "rezerokara");
    }

    #[test]
    fn answers_are_read_leniently() {
        let anilist = parse_anilist(&json!({"data": {"Page": {"media": [
            {"id": 154587, "idMal": 52991, "format": "TV", "episodes": 28, "seasonYear": 2023,
             "title": {"romaji": "Sousou no Frieren", "english": "Frieren", "native": "葬送のフリーレン"},
             "synonyms": ["Frieren at the Funeral"], "coverImage": {"large": "https://s4.anilist.co/x.jpg"},
             "siteUrl": "https://anilist.co/anime/154587"},
            {"id": 1, "title": {}},
            {"title": {"romaji": "no id"}},
        ]}}}))
        .unwrap();
        assert_eq!(anilist.len(), 1);
        let f = &anilist[0];
        assert_eq!(
            (f.anilist_id, f.mal_id, f.episodes),
            (Some(154587), Some(52991), Some(28))
        );
        assert_eq!(f.native.as_deref(), Some("葬送のフリーレン"));
        assert!(parse_anilist(&json!({"errors": [{"message": "Too Many Requests"}]})).is_err());
        let jikan = parse_jikan(&json!({"data": [
            {"mal_id": 52991, "title": "Sousou no Frieren", "title_japanese": "葬送のフリーレン",
             "episodes": 28, "year": 2023, "type": "TV",
             "images": {"jpg": {"image_url": "https://cdn.myanimelist.net/a.jpg"}}},
            {"title": "no id"},
        ]}))
        .unwrap();
        assert_eq!(jikan.len(), 1);
        assert_eq!(
            jikan[0].cover_url.as_deref(),
            Some("https://cdn.myanimelist.net/a.jpg")
        );
        assert!(parse_jikan(&json!([])).is_err());
    }
}
