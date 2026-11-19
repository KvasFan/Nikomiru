// SPDX-License-Identifier: GPL-3.0-only

//! Kavita's HTTP API (SRV-020…SRV-022): its libraries of manga, comics and
//! pictures, each series' chapters as its "episodes", whose pages are
//! fetched to be read in the manga reader, and the page reached reported
//! back. The user's API key is kept; each session asks for a token with it.

use reqwest::Url;
use serde_json::{Value, json};

use super::{
    Item, ItemKind, Library, MediaServer, PAGE, Page, ParentKind, PlayState, Playable,
    ServerSubtitle, Stream, join, success,
};
use crate::error::OnlineError;
use crate::http::Http;

const NAME: &str = "Kavita";
/// Between the parts of an item's id.
const SEP: char = '~';
/// Kavita's library types read as pictures: manga, comics, images.
const PICTURES: [i64; 3] = [0, 1, 3];

/// A Kavita server, logged in.
pub struct Kavita {
    http: Http,
    base: Url,
    api_key: String,
    token: Option<String>,
}

impl std::fmt::Debug for Kavita {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kavita")
            .field("base", &self.base.as_str())
            .finish_non_exhaustive()
    }
}

fn protocol(reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: NAME.into(),
        reason: reason.into(),
    }
}

/// A chapter's place: library, series, volume and chapter ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    /// The library.
    pub library: i64,
    /// The series.
    pub series: i64,
    /// The volume.
    pub volume: i64,
    /// The chapter.
    pub chapter: i64,
}

impl Place {
    /// Its item id.
    pub fn id(&self) -> String {
        format!(
            "{}{SEP}{}{SEP}{}{SEP}{}",
            self.library, self.series, self.volume, self.chapter
        )
    }

    /// The place an item id names; `None` for a series' id.
    pub fn of(id: &str) -> Option<Self> {
        let parts: Vec<i64> = id
            .split(SEP)
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        match parts.as_slice() {
            [library, series, volume, chapter] => Some(Self {
                library: *library,
                series: *series,
                volume: *volume,
                chapter: *chapter,
            }),
            _ => None,
        }
    }
}

fn series_id(library: i64, series: i64) -> String {
    format!("{library}{SEP}{series}")
}

fn parse_series(id: &str) -> Option<(i64, i64)> {
    let (library, series) = id.split_once(SEP)?;
    if series.contains(SEP) {
        return None;
    }
    Some((library.parse().ok()?, series.parse().ok()?))
}

fn number(value: &Value) -> i64 {
    value.as_i64().unwrap_or(0)
}

fn series_item(value: &Value, library: i64) -> Option<Item> {
    let id = number(&value["id"]);
    if id == 0 {
        return None;
    }
    let library = value["libraryId"].as_i64().unwrap_or(library);
    let pages = value["pages"].as_u64().unwrap_or(0);
    let read = value["pagesRead"].as_u64().unwrap_or(0);
    Some(Item {
        id: series_id(library, id),
        kind: ItemKind::Series,
        title: value["name"].as_str().unwrap_or_default().to_string(),
        series: None,
        parent: None,
        season: None,
        episode: None,
        cover: Some(id.to_string()),
        played: pages > 0 && read >= pages,
        position_ms: read,
        duration_ms: pages,
        last_played_s: None,
        anilist_id: None,
    })
}

/// A series' chapters, volume by volume, as its episodes: their position
/// and length are pages.
fn chapters(volumes: &Value, library: i64, series: i64, title: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let mut volumes: Vec<&Value> = volumes
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    volumes.sort_by(|a, b| {
        let key = |v: &Value| {
            v["minNumber"]
                .as_f64()
                .or_else(|| v["number"].as_f64())
                .unwrap_or(0.0)
        };
        key(a).total_cmp(&key(b))
    });
    for volume in volumes {
        let volume_id = number(&volume["id"]);
        let volume_name = volume["name"].as_str().unwrap_or_default();
        let mut list: Vec<&Value> = volume["chapters"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        list.sort_by(|a, b| {
            let key = |c: &Value| {
                c["minNumber"]
                    .as_f64()
                    .or_else(|| c["range"].as_str().and_then(|r| r.parse().ok()))
                    .unwrap_or(0.0)
            };
            key(a).total_cmp(&key(b))
        });
        for chapter in list {
            let pages = chapter["pages"].as_u64().unwrap_or(0);
            let read = chapter["pagesRead"].as_u64().unwrap_or(0);
            let range = chapter["range"].as_str().unwrap_or_default();
            let name = chapter["title"].as_str().filter(|t| !t.is_empty());
            let place = Place {
                library,
                series,
                volume: volume_id,
                chapter: number(&chapter["id"]),
            };
            out.push(Item {
                id: place.id(),
                kind: ItemKind::Episode,
                title: match name {
                    Some(name) => name.to_string(),
                    None if range.is_empty() => format!("Vol. {volume_name}"),
                    None if volume_name.is_empty() => format!("Ch. {range}"),
                    None => format!("Vol. {volume_name} Ch. {range}"),
                },
                series: Some(title.to_string()),
                parent: Some(series_id(library, series)),
                season: None,
                episode: u32::try_from(out.len() + 1).ok(),
                cover: Some(series.to_string()),
                played: pages > 0 && read >= pages,
                position_ms: read,
                duration_ms: pages,
                last_played_s: None,
                anilist_id: None,
            });
        }
    }
    out
}

impl Kavita {
    /// Logs in with a user name and password, which are not kept
    /// (SRV-020): the user's API key is.
    pub fn login(http: Http, base: Url, user: &str, password: &str) -> Result<Self, OnlineError> {
        let answer = http.post_json(
            &join(&base, &["api", "Account", "login"])?,
            &[],
            &json!({ "username": user, "password": password }),
        )?;
        let answer = success(answer, NAME)?.json(NAME)?;
        let (Some(key), Some(token)) = (answer["apiKey"].as_str(), answer["token"].as_str()) else {
            return Err(protocol("no API key in the answer"));
        };
        Ok(Self {
            http,
            base,
            api_key: key.to_string(),
            token: Some(token.to_string()),
        })
    }

    /// A server with the user's API key: a token is asked for at the first
    /// request.
    pub fn new(http: Http, base: Url, api_key: &str) -> Self {
        Self {
            http,
            base,
            api_key: api_key.to_string(),
            token: None,
        }
    }

    /// The API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    fn api(&self, segments: &[&str]) -> Result<Url, OnlineError> {
        let mut path = vec!["api"];
        path.extend_from_slice(segments);
        join(&self.base, &path)
    }

    fn bearer(&mut self) -> Result<String, OnlineError> {
        if self.token.is_none() {
            let mut url = self.api(&["Plugin", "authenticate"])?;
            url.query_pairs_mut()
                .append_pair("apiKey", &self.api_key)
                .append_pair("pluginName", "Nikomiru");
            let answer = self.http.post_json(&url, &[], &json!({}))?;
            let answer = success(answer, NAME)?.json(NAME)?;
            self.token = Some(
                answer["token"]
                    .as_str()
                    .ok_or_else(|| protocol("no token for the API key"))?
                    .to_string(),
            );
        }
        Ok(format!(
            "Bearer {}",
            self.token.as_deref().unwrap_or_default()
        ))
    }

    fn get(&mut self, url: &Url) -> Result<Value, OnlineError> {
        let bearer = self.bearer()?;
        let answer = self.http.get(url, &[("Authorization", &bearer)])?;
        success(answer, NAME)?.json(NAME)
    }

    fn post(&mut self, url: &Url, body: &Value) -> Result<Value, OnlineError> {
        let bearer = self.bearer()?;
        let answer = self
            .http
            .post_json(url, &[("Authorization", &bearer)], body)?;
        let answer = success(answer, NAME)?;
        Ok(if answer.body.is_empty() {
            Value::Null
        } else {
            answer.json(NAME)?
        })
    }

    fn volumes(&mut self, series: i64) -> Result<Value, OnlineError> {
        let mut url = self.api(&["Series", "volumes"])?;
        url.query_pairs_mut()
            .append_pair("seriesId", &series.to_string());
        self.get(&url)
    }

    fn series_title(&mut self, series: i64) -> Result<String, OnlineError> {
        let url = self.api(&["Series", &series.to_string()])?;
        Ok(self.get(&url)?["name"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// The pages of a chapter, in order, each with what is needed to fetch
    /// it (SRV-021); the token stays in the headers.
    pub fn chapter_pages(&mut self, id: &str) -> Result<Vec<Stream>, OnlineError> {
        let place = Place::of(id).ok_or_else(|| protocol("not a chapter"))?;
        let item = self.item(id)?;
        let bearer = self.bearer()?;
        (0..item.duration_ms)
            .map(|page| {
                let mut url = self.api(&["Reader", "image"])?;
                url.query_pairs_mut()
                    .append_pair("chapterId", &place.chapter.to_string())
                    .append_pair("page", &page.to_string())
                    .append_pair("apiKey", &self.api_key);
                Ok(Stream {
                    url,
                    headers: vec![("Authorization".into(), bearer.clone())],
                })
            })
            .collect()
    }
}

impl MediaServer for Kavita {
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError> {
        let answer = self.get(&self.api(&["Library", "libraries"])?)?;
        Ok(answer
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|l| l["type"].as_i64().is_some_and(|t| PICTURES.contains(&t)))
                    .filter_map(|l| {
                        Some(Library {
                            id: l["id"].as_i64()?.to_string(),
                            title: l["name"].as_str().unwrap_or_default().to_string(),
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
        match kind {
            ParentKind::Library => {
                let library: i64 = parent.parse().map_err(|_| protocol("a library's id"))?;
                let mut url = self.api(&["Series", "all-v2"])?;
                url.query_pairs_mut()
                    .append_pair("PageNumber", &(start / PAGE + 1).to_string())
                    .append_pair("PageSize", &PAGE.to_string());
                // the series of this library, by name
                let filter = json!({
                    "statements": [{ "comparison": 0, "field": 19, "value": library.to_string() }],
                    "combination": 1,
                    "sortOptions": { "sortField": 1, "isAscending": true },
                    "limitTo": 0,
                });
                let bearer = self.bearer()?;
                let answer = self
                    .http
                    .post_json(&url, &[("Authorization", &bearer)], &filter)?;
                let answer = success(answer, NAME)?;
                let series = answer.json(NAME)?;
                let items: Vec<Item> = series
                    .as_array()
                    .map(|a| a.iter().filter_map(|s| series_item(s, library)).collect())
                    .unwrap_or_default();
                let total = start + items.len() + usize::from(items.len() == PAGE);
                Ok(Page { items, total })
            }
            ParentKind::Item => {
                let (library, series) =
                    parse_series(parent).ok_or_else(|| protocol("not a series"))?;
                let title = self.series_title(series)?;
                let items = chapters(&self.volumes(series)?, library, series, &title);
                let total = items.len();
                Ok(Page {
                    items: items.into_iter().skip(start).take(PAGE).collect(),
                    total,
                })
            }
        }
    }

    fn search(&mut self, library: &str, query: &str, _start: usize) -> Result<Page, OnlineError> {
        let library: i64 = library.parse().map_err(|_| protocol("a library's id"))?;
        let mut url = self.api(&["Search", "search"])?;
        url.query_pairs_mut().append_pair("queryString", query);
        let answer = self.get(&url)?;
        let items: Vec<Item> = answer["series"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|s| s["libraryId"].as_i64() == Some(library))
                    .filter_map(|s| {
                        let mut s = s.clone();
                        s["id"] = s["seriesId"].clone();
                        series_item(&s, library)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let total = items.len();
        Ok(Page { items, total })
    }

    fn item(&mut self, id: &str) -> Result<Item, OnlineError> {
        if let Some(place) = Place::of(id) {
            let title = self.series_title(place.series)?;
            return chapters(
                &self.volumes(place.series)?,
                place.library,
                place.series,
                &title,
            )
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(OnlineError::Gone {
                provider: NAME.into(),
            });
        }
        let (library, series) = parse_series(id).ok_or_else(|| protocol("not a series"))?;
        let url = self.api(&["Series", &series.to_string()])?;
        series_item(&self.get(&url)?, library).ok_or(OnlineError::Gone {
            provider: NAME.into(),
        })
    }

    fn playable(&mut self, _: &str) -> Result<Playable, OnlineError> {
        Err(protocol("a chapter is read, not played"))
    }

    fn subtitle(&mut self, _: &ServerSubtitle) -> Result<Vec<u8>, OnlineError> {
        Err(protocol("no subtitles"))
    }

    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError> {
        let mut url = self.api(&["Image", "series-cover"])?;
        url.query_pairs_mut()
            .append_pair("seriesId", cover)
            .append_pair("apiKey", &self.api_key);
        let bearer = self.bearer()?;
        let answer = self.http.get(&url, &[("Authorization", &bearer)])?;
        Ok(success(answer, NAME)?.body)
    }

    fn report(
        &mut self,
        id: &str,
        _source: &str,
        _state: PlayState,
        page: u64,
        _pages: u64,
    ) -> Result<(), OnlineError> {
        let Some(place) = Place::of(id) else {
            return Ok(());
        };
        let url = self.api(&["Reader", "progress"])?;
        self.post(
            &url,
            &json!({
                "libraryId": place.library,
                "seriesId": place.series,
                "volumeId": place.volume,
                "chapterId": place.chapter,
                "pageNum": page,
            }),
        )
        .map(drop)
    }

    fn mark_played(&mut self, id: &str) -> Result<(), OnlineError> {
        let pages = self.item(id)?.duration_ms;
        self.report(id, "", PlayState::Stopped, pages, pages)
    }

    fn logout(&mut self) -> Result<(), OnlineError> {
        Ok(())
    }

    fn pages(&mut self, id: &str) -> Result<Vec<Stream>, OnlineError> {
        self.chapter_pages(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-SRV-021: a series' chapters in the order of their volumes and
    // numbers, their position and length in pages
    #[test]
    fn a_series_chapters_are_its_episodes() {
        let volumes = json!([
            { "id": 12, "minNumber": 2.0, "name": "2", "chapters": [
                { "id": 30, "range": "10", "minNumber": 10.0, "pages": 20, "pagesRead": 0 } ] },
            { "id": 11, "minNumber": 1.0, "name": "1", "chapters": [
                { "id": 21, "range": "2", "minNumber": 2.0, "pages": 18, "pagesRead": 5 },
                { "id": 20, "range": "1", "minNumber": 1.0, "pages": 24, "pagesRead": 24, "title": "賭博" } ] }
        ]);
        let items = chapters(&volumes, 3, 7, "カイジ");
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["3~7~11~20", "3~7~11~21", "3~7~12~30"]);
        assert_eq!((items[0].title.as_str(), items[0].played), ("賭博", true));
        assert_eq!(
            (
                items[1].title.as_str(),
                items[1].position_ms,
                items[1].duration_ms
            ),
            ("Vol. 1 Ch. 2", 5, 18)
        );
        assert_eq!(items[2].parent.as_deref(), Some("3~7"));
        assert_eq!(
            Place::of("3~7~11~21"),
            Some(Place {
                library: 3,
                series: 7,
                volume: 11,
                chapter: 21
            })
        );
        assert_eq!(Place::of("3~7"), None);
        assert_eq!(parse_series("3~7"), Some((3, 7)));
    }
}
