// SPDX-License-Identifier: GPL-3.0-only

//! The Jimaku client (MET-003, ADR 0008 §4): entries searched by title, their
//! files, the downloads; the key goes only to the Jimaku host.
//!
//! Translated from Fushi `packages/fushi_engine/lib/media/video/jimaku_client.dart`
//! (commit `ac8034e`).

use reqwest::Url;
use serde_json::Value;

use crate::error::OnlineError;
use crate::http::{Http, checked_url};
use crate::language::{subtitle_episode, subtitle_language};
use crate::{Candidate, FileKind, Locator, Provider, ProviderId, SearchRequest};

/// The real service.
pub const BASE: &str = "https://jimaku.cc/api";

/// The entries whose files are listed for one search: the first ones are
/// nearly always the work looked for.
const ENTRIES: usize = 2;

const NAME: &str = "Jimaku";

/// A Jimaku entry: one work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its id.
    pub id: u64,
    /// Its name: romaji, else English, else Japanese.
    pub name: String,
}

/// A file of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Its name.
    pub name: String,
    /// Its address.
    pub url: String,
    /// Its size.
    pub size: Option<u64>,
}

fn protocol(reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: NAME.into(),
        reason: reason.into(),
    }
}

/// The entries of a search answer; malformed ones are skipped.
pub fn parse_entries(json: &Value) -> Result<Vec<Entry>, OnlineError> {
    let Value::Array(items) = json else {
        return Err(protocol("the search answer is not a list"));
    };
    Ok(items
        .iter()
        .filter_map(|e| {
            let id = e.get("id")?.as_u64()?;
            let text = |k: &str| {
                e.get(k)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            };
            let name = text("name")
                .or_else(|| text("english_name"))
                .or_else(|| text("japanese_name"))
                .map_or_else(|| format!("#{id}"), str::to_string);
            Some(Entry { id, name })
        })
        .collect())
}

/// The files of a files answer; malformed ones are skipped.
pub fn parse_files(json: &Value) -> Result<Vec<File>, OnlineError> {
    let Value::Array(items) = json else {
        return Err(protocol("the files answer is not a list"));
    };
    Ok(items
        .iter()
        .filter_map(|f| {
            Some(File {
                name: f.get("name")?.as_str()?.to_string(),
                url: f.get("url")?.as_str()?.to_string(),
                size: f.get("size").and_then(Value::as_u64),
            })
        })
        .collect())
}

/// A Jimaku client.
#[derive(Debug)]
pub struct Jimaku {
    http: Http,
    base: Url,
    key: String,
}

impl Jimaku {
    /// A client of the Jimaku at `base` with the API key `key`.
    pub fn new(http: Http, base: &str, key: &str) -> Result<Self, OnlineError> {
        if key.trim().is_empty() {
            return Err(OnlineError::NoKey {
                provider: NAME.into(),
            });
        }
        Ok(Self {
            http,
            base: checked_url(base)?,
            key: key.trim().to_string(),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, OnlineError> {
        let base = self.base.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}/{path}"))
            .map_err(|_| OnlineError::BadUrl(format!("{base}/{path}")))
    }

    fn headers(&self) -> [(&str, &str); 2] {
        [
            ("Authorization", self.key.as_str()),
            ("Accept", "application/json"),
        ]
    }

    /// The entries for `query`: anime first, then live action (Jimaku filters
    /// on `anime`, `true` by default: Fushi's BUG-1694).
    pub fn search_entries(&self, query: &str) -> Result<Vec<Entry>, OnlineError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        for anime in ["true", "false"] {
            let mut url = self.endpoint("entries/search")?;
            url.query_pairs_mut()
                .append_pair("query", query.trim())
                .append_pair("anime", anime);
            let answer = self.http.get(&url, &self.headers())?.ok(NAME)?;
            let entries = parse_entries(&answer.json(NAME)?)?;
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
        Ok(Vec::new())
    }

    /// The entries of the AniList series `id` (MET-003).
    pub fn search_anilist(&self, id: u64) -> Result<Vec<Entry>, OnlineError> {
        let mut url = self.endpoint("entries/search")?;
        url.query_pairs_mut()
            .append_pair("anilist_id", &id.to_string());
        let answer = self.http.get(&url, &self.headers())?.ok(NAME)?;
        parse_entries(&answer.json(NAME)?)
    }

    /// The files of an entry.
    pub fn files(&self, entry: u64) -> Result<Vec<File>, OnlineError> {
        let url = self.endpoint(&format!("entries/{entry}/files"))?;
        let answer = self.http.get(&url, &self.headers())?.ok(NAME)?;
        parse_files(&answer.json(NAME)?)
    }

    /// A file address the key may go to: on the Jimaku host.
    fn file_url(&self, url: &str) -> Result<Url, OnlineError> {
        let parsed = checked_url(url)?;
        let same = parsed.host_str() == self.base.host_str()
            && parsed.port_or_known_default() == self.base.port_or_known_default();
        if same {
            Ok(parsed)
        } else {
            Err(OnlineError::BadUrl(url.to_string()))
        }
    }
}

impl Provider for Jimaku {
    fn id(&self) -> ProviderId {
        ProviderId::Jimaku
    }

    fn search(&mut self, request: &SearchRequest) -> Result<Vec<Candidate>, OnlineError> {
        let mut out = Vec::new();
        // the series identified first, then its title (MET-003)
        let mut entries = match request.anilist_id {
            Some(id) => self.search_anilist(id)?,
            None => Vec::new(),
        };
        if entries.is_empty() {
            entries = self.search_entries(&request.title)?;
        }
        for entry in entries.into_iter().take(ENTRIES) {
            for file in self.files(entry.id)? {
                out.push(Candidate {
                    provider: ProviderId::Jimaku,
                    locator: Locator::Url(file.url),
                    language: subtitle_language(&file.name).map(str::to_string),
                    episode: subtitle_episode(&file.name),
                    kind: FileKind::of(&file.name),
                    file_name: file.name,
                    source: entry.name.clone(),
                    machine_translated: false,
                    size: file.size,
                    order: out.len(),
                });
            }
        }
        Ok(out)
    }

    fn download(&mut self, candidate: &Candidate) -> Result<Vec<u8>, OnlineError> {
        let Locator::Url(url) = &candidate.locator else {
            return Err(protocol("not a Jimaku file"));
        };
        let url = self.file_url(url)?;
        Ok(self.http.file(&url, &self.headers())?.ok(NAME)?.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // T-MET-003: Fushi `parseJimakuEntries`, `parseJimakuFiles`
    #[test]
    fn answers_are_read_leniently() {
        let entries = parse_entries(&json!([
            {"id": 1, "name": "Sousou no Frieren", "anilist_id": 154587},
            {"id": 2, "name": "", "english_name": "Frieren"},
            {"id": 3, "japanese_name": "葬送のフリーレン"},
            {"id": 4},
            {"name": "no id"},
            "junk"
        ]))
        .unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["Sousou no Frieren", "Frieren", "葬送のフリーレン", "#4"]
        );
        assert!(parse_entries(&json!({"error": "x"})).is_err());
        let files = parse_files(&json!([
            {"name": "a.srt", "url": "https://jimaku.cc/entry/1/download/a.srt", "size": 10},
            {"name": "b.srt"},
        ]))
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size, Some(10));
    }

    #[test]
    fn the_key_stays_on_the_jimaku_host() {
        let http = Http::new("").unwrap();
        assert!(matches!(
            Jimaku::new(http.clone(), BASE, " "),
            Err(OnlineError::NoKey { .. })
        ));
        let j = Jimaku::new(http, BASE, "k").unwrap();
        assert!(
            j.file_url("https://jimaku.cc/entry/1/download/a.srt")
                .is_ok()
        );
        assert!(j.file_url("https://evil.example/a.srt").is_err());
        assert!(j.file_url("http://jimaku.cc/a.srt").is_err());
    }
}
