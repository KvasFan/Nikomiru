// SPDX-License-Identifier: GPL-3.0-only

//! Audiobookshelf's HTTP API (SRV-017…SRV-019): its libraries of books, each
//! book a "series" whose audio files are its "episodes", played through the
//! relay as a server's video is, and the listening position reported back
//! as the whole book's.

use reqwest::Url;
use serde_json::{Value, json};

use super::{
    Item, ItemKind, Library, MediaServer, PAGE, Page, ParentKind, PlayState, Playable,
    ServerSubtitle, Stream, join, success,
};
use crate::error::OnlineError;
use crate::http::Http;

const NAME: &str = "Audiobookshelf";
/// Between a book's id and its file's in a track's id.
const TRACK: char = '~';

/// An Audiobookshelf server, logged in.
pub struct Audiobookshelf {
    http: Http,
    base: Url,
    token: String,
    user_id: String,
}

impl std::fmt::Debug for Audiobookshelf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Audiobookshelf")
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

fn seconds_to_ms(value: &Value) -> u64 {
    value
        .as_f64()
        .filter(|s| s.is_finite() && *s > 0.0)
        .map_or(0, |s| (s * 1000.0) as u64)
}

/// A book's audio files, in order: their file id, name and length.
fn tracks_of(book: &Value) -> Vec<(String, String, u64)> {
    let mut files: Vec<&Value> = book["media"]["audioFiles"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|f| !f["exclude"].as_bool().unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|f| f["index"].as_u64().unwrap_or(u64::MAX));
    files
        .into_iter()
        .filter_map(|f| {
            let ino = match &f["ino"] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            let name = f["metadata"]["filename"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Some((ino, name, seconds_to_ms(&f["duration"])))
        })
        .collect()
}

/// What the user has listened to of a book: position, finished, when.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Progress {
    at_ms: u64,
    finished: bool,
    last_s: Option<i64>,
}

fn progress_of(value: &Value) -> Progress {
    Progress {
        at_ms: seconds_to_ms(&value["currentTime"]),
        finished: value["isFinished"].as_bool().unwrap_or(false),
        last_s: value["lastUpdate"].as_i64().map(|ms| ms / 1000),
    }
}

fn title_of(book: &Value) -> String {
    book["media"]["metadata"]["title"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn book_item(book: &Value, progress: Progress) -> Option<Item> {
    let id = book["id"].as_str()?.to_string();
    Some(Item {
        cover: Some(id.clone()),
        series: book["media"]["metadata"]["authorName"]
            .as_str()
            .map(str::to_string),
        id,
        kind: ItemKind::Series,
        title: title_of(book),
        parent: None,
        season: None,
        episode: None,
        played: progress.finished,
        position_ms: progress.at_ms,
        duration_ms: seconds_to_ms(&book["media"]["duration"]),
        last_played_s: progress.last_s,
        anilist_id: None,
    })
}

/// The tracks of `book` as episodes, each with its part of the position.
fn track_items(book: &Value, progress: Progress) -> Vec<Item> {
    let Some(id) = book["id"].as_str() else {
        return Vec::new();
    };
    let title = title_of(book);
    let mut start = 0u64;
    tracks_of(book)
        .into_iter()
        .enumerate()
        .map(|(i, (ino, name, duration_ms))| {
            let end = start + duration_ms;
            let item = Item {
                id: format!("{id}{TRACK}{ino}"),
                kind: ItemKind::Episode,
                title: if name.is_empty() {
                    format!("{}", i + 1)
                } else {
                    name
                },
                series: Some(title.clone()),
                parent: Some(id.to_string()),
                season: None,
                episode: u32::try_from(i + 1).ok(),
                cover: Some(id.to_string()),
                played: progress.finished || progress.at_ms >= end,
                position_ms: if (start..end).contains(&progress.at_ms) {
                    progress.at_ms - start
                } else {
                    0
                },
                duration_ms,
                last_played_s: progress.last_s,
                anilist_id: None,
            };
            start = end;
            item
        })
        .collect()
}

/// Where a track starts in its book and how long the book is: what a
/// report names (SRV-019).
fn source_of(start_ms: u64, total_ms: u64) -> String {
    format!("{start_ms}:{total_ms}")
}

fn parse_source(source: &str) -> Option<(u64, u64)> {
    let (start, total) = source.split_once(':')?;
    Some((start.parse().ok()?, total.parse().ok()?))
}

impl Audiobookshelf {
    /// Logs in with a user name and password, which are not kept
    /// (SRV-017): the user's API token is.
    pub fn login(http: Http, base: Url, user: &str, password: &str) -> Result<Self, OnlineError> {
        let answer = http.post_json(
            &join(&base, &["login"])?,
            &[],
            &json!({ "username": user, "password": password }),
        )?;
        let answer = success(answer, NAME)?.json(NAME)?;
        let user = &answer["user"];
        let token = user["token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .or_else(|| user["accessToken"].as_str());
        let (Some(token), Some(user_id)) = (token, user["id"].as_str()) else {
            return Err(protocol("no token in the answer"));
        };
        Ok(Self::new(http, base, token, user_id))
    }

    /// A session already logged in.
    pub fn new(http: Http, base: Url, token: &str, user_id: &str) -> Self {
        Self {
            http,
            base,
            token: token.to_string(),
            user_id: user_id.to_string(),
        }
    }

    /// The token.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The user's id.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn get(&self, url: &Url) -> Result<Value, OnlineError> {
        let bearer = self.bearer();
        let answer = self.http.get(url, &[("Authorization", &bearer)])?;
        success(answer, NAME)?.json(NAME)
    }

    fn api(&self, segments: &[&str]) -> Result<Url, OnlineError> {
        let mut path = vec!["api"];
        path.extend_from_slice(segments);
        join(&self.base, &path)
    }

    fn book(&self, id: &str) -> Result<Value, OnlineError> {
        let mut url = self.api(&["items", id])?;
        url.query_pairs_mut().append_pair("expanded", "1");
        self.get(&url)
    }

    /// What the user listened to, by book.
    fn progress(&self, id: &str) -> Result<Progress, OnlineError> {
        let bearer = self.bearer();
        let answer = self.http.get(
            &self.api(&["me", "progress", id])?,
            &[("Authorization", &bearer)],
        )?;
        // none yet: a 404
        if answer.status == 404 {
            return Ok(Progress::default());
        }
        Ok(progress_of(&success(answer, NAME)?.json(NAME)?))
    }

    fn all_progress(&self) -> Result<std::collections::HashMap<String, Progress>, OnlineError> {
        let me = self.get(&self.api(&["me"])?)?;
        Ok(me["mediaProgress"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some((p["libraryItemId"].as_str()?.to_string(), progress_of(p)))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn books_page(&self, books: &[Value], total: usize) -> Result<Page, OnlineError> {
        let progress = self.all_progress()?;
        let items = books
            .iter()
            .filter_map(|b| {
                let id = b["id"].as_str()?;
                book_item(b, progress.get(id).copied().unwrap_or_default())
            })
            .collect();
        Ok(Page { items, total })
    }

    fn patch_progress(&self, id: &str, body: &Value) -> Result<(), OnlineError> {
        let bearer = self.bearer();
        let answer = self.http.send_json(
            "PATCH",
            &self.api(&["me", "progress", id])?,
            &[("Authorization", &bearer)],
            body,
        )?;
        success(answer, NAME).map(|_| ())
    }
}

impl MediaServer for Audiobookshelf {
    fn libraries(&mut self) -> Result<Vec<Library>, OnlineError> {
        let answer = self.get(&self.api(&["libraries"])?)?;
        Ok(answer["libraries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|l| l["mediaType"].as_str() == Some("book"))
                    .filter_map(|l| {
                        Some(Library {
                            id: l["id"].as_str()?.to_string(),
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
                let mut url = self.api(&["libraries", parent, "items"])?;
                url.query_pairs_mut()
                    .append_pair("limit", &PAGE.to_string())
                    .append_pair("page", &(start / PAGE).to_string())
                    .append_pair("sort", "media.metadata.title");
                let answer = self.get(&url)?;
                let books = answer["results"].as_array().cloned().unwrap_or_default();
                let total = answer["total"].as_u64().map_or(books.len(), |n| n as usize);
                self.books_page(&books, total)
            }
            ParentKind::Item => {
                let book = self.book(parent)?;
                let items = track_items(&book, self.progress(parent)?);
                let total = items.len();
                Ok(Page {
                    items: items.into_iter().skip(start).take(PAGE).collect(),
                    total,
                })
            }
        }
    }

    fn search(&mut self, library: &str, query: &str, _start: usize) -> Result<Page, OnlineError> {
        let mut url = self.api(&["libraries", library, "search"])?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &PAGE.to_string());
        let answer = self.get(&url)?;
        let books: Vec<Value> = answer["book"]
            .as_array()
            .map(|a| a.iter().map(|b| b["libraryItem"].clone()).collect())
            .unwrap_or_default();
        let total = books.len();
        self.books_page(&books, total)
    }

    fn item(&mut self, id: &str) -> Result<Item, OnlineError> {
        let (book_id, track) = match id.split_once(TRACK) {
            Some((book, _)) => (book, true),
            None => (id, false),
        };
        let book = self.book(book_id)?;
        let progress = self.progress(book_id)?;
        if track {
            track_items(&book, progress)
                .into_iter()
                .find(|t| t.id == id)
                .ok_or(OnlineError::Gone {
                    provider: NAME.into(),
                })
        } else {
            book_item(&book, progress).ok_or_else(|| protocol("a book without an id"))
        }
    }

    fn playable(&mut self, id: &str) -> Result<Playable, OnlineError> {
        let (book_id, ino) = id
            .split_once(TRACK)
            .ok_or_else(|| protocol("only a book's files are played"))?;
        let book = self.book(book_id)?;
        let tracks = tracks_of(&book);
        let total: u64 = tracks.iter().map(|t| t.2).sum();
        let mut start = 0;
        for (file, _, duration) in &tracks {
            if file == ino {
                return Ok(Playable {
                    stream: Stream {
                        url: self.api(&["items", book_id, "file", ino])?,
                        headers: vec![("Authorization".into(), self.bearer())],
                    },
                    source: source_of(start, total),
                    subtitles: Vec::new(),
                });
            }
            start += duration;
        }
        Err(OnlineError::Gone {
            provider: NAME.into(),
        })
    }

    fn subtitle(&mut self, _: &ServerSubtitle) -> Result<Vec<u8>, OnlineError> {
        Err(protocol("no subtitles"))
    }

    fn cover(&mut self, cover: &str) -> Result<Vec<u8>, OnlineError> {
        let bearer = self.bearer();
        let answer = self.http.get(
            &self.api(&["items", cover, "cover"])?,
            &[("Authorization", &bearer)],
        )?;
        Ok(success(answer, NAME)?.body)
    }

    fn report(
        &mut self,
        id: &str,
        source: &str,
        _state: PlayState,
        position_ms: u64,
        _duration_ms: u64,
    ) -> Result<(), OnlineError> {
        let Some((book, _)) = id.split_once(TRACK) else {
            return Ok(());
        };
        let (start, total) =
            parse_source(source).ok_or_else(|| protocol("no place in the book"))?;
        let at = if total > 0 {
            (start + position_ms).min(total)
        } else {
            start + position_ms
        };
        let total_s = total as f64 / 1000.0;
        let at_s = at as f64 / 1000.0;
        self.patch_progress(
            book,
            &json!({
                "currentTime": at_s,
                "duration": total_s,
                "progress": if total > 0 { (at as f64 / total as f64).min(1.0) } else { 0.0 },
                "isFinished": false,
            }),
        )
    }

    fn mark_played(&mut self, id: &str) -> Result<(), OnlineError> {
        let (book_id, ino) = match id.split_once(TRACK) {
            Some((book, ino)) => (book, Some(ino)),
            None => (id, None),
        };
        let book = self.book(book_id)?;
        let tracks = tracks_of(&book);
        let total: u64 = tracks.iter().map(|t| t.2).sum();
        // a file heard to its end: the book from the next one; the last, or
        // the book: finished
        let end = match ino {
            Some(ino) => {
                let mut end = 0;
                for (file, _, duration) in &tracks {
                    end += duration;
                    if file == ino {
                        break;
                    }
                }
                end
            }
            None => total,
        };
        let finished = end >= total;
        self.patch_progress(
            book_id,
            &json!({
                "currentTime": end as f64 / 1000.0,
                "duration": total as f64 / 1000.0,
                "progress": if finished { 1.0 } else { end as f64 / total.max(1) as f64 },
                "isFinished": finished,
            }),
        )
    }

    fn logout(&mut self) -> Result<(), OnlineError> {
        let bearer = self.bearer();
        let answer = self.http.post_json(
            &join(&self.base, &["logout"])?,
            &[("Authorization", &bearer)],
            &json!({}),
        )?;
        success(answer, NAME).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Value {
        json!({
            "id": "li_1",
            "media": {
                "metadata": { "title": "銀河鉄道の夜", "authorName": "宮沢賢治" },
                "duration": 30.0,
                "audioFiles": [
                    { "ino": "b", "index": 2, "duration": 20.0, "metadata": { "filename": "02.mp3" } },
                    { "ino": "a", "index": 1, "duration": 10.0, "metadata": { "filename": "01.mp3" } },
                    { "ino": "x", "index": 3, "duration": 5.0, "exclude": true, "metadata": { "filename": "x.mp3" } }
                ]
            }
        })
    }

    // T-SRV-018: a book's files in order, each with its part of the
    // position heard
    #[test]
    fn a_books_files_are_its_episodes() {
        let progress = Progress {
            at_ms: 12_500,
            finished: false,
            last_s: Some(7),
        };
        let tracks = track_items(&book(), progress);
        let ids: Vec<&str> = tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["li_1~a", "li_1~b"]);
        assert_eq!(tracks[0].title, "01.mp3");
        assert_eq!((tracks[0].played, tracks[0].position_ms), (true, 0));
        assert_eq!((tracks[1].played, tracks[1].position_ms), (false, 2_500));
        assert_eq!(tracks[1].episode, Some(2));
        assert_eq!(tracks[1].series.as_deref(), Some("銀河鉄道の夜"));
        let item = book_item(&book(), progress).unwrap();
        assert_eq!(
            (item.kind, item.duration_ms, item.series.as_deref()),
            (ItemKind::Series, 30_000, Some("宮沢賢治"))
        );
        assert_eq!(
            parse_source(&source_of(10_000, 30_000)),
            Some((10_000, 30_000))
        );
        assert_eq!(parse_source("x"), None);
    }
}
