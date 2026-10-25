// SPDX-License-Identifier: GPL-3.0-only

//! The OpenSubtitles client (MET-004, ADR 0008 §5): searches by file hash,
//! then by title and episode, in the study language; downloads through a
//! temporary link; an optional login for the user's quota.
//!
//! Translated from Fushi `packages/fushi_engine/lib/media/video/subtitle/open_subtitles_client.dart`
//! (commit `ac8034e`).

use reqwest::Url;
use serde_json::{Value, json};

use crate::error::OnlineError;
use crate::http::{Http, checked_url};
use crate::language::{language_of_token, subtitle_episode};
use crate::{Candidate, FileKind, Locator, Provider, ProviderId, SearchRequest};

/// The real service.
pub const BASE: &str = "https://api.opensubtitles.com/api/v1";

const NAME: &str = "OpenSubtitles";

fn protocol(reason: &str) -> OnlineError {
    OnlineError::Protocol {
        provider: NAME.into(),
        reason: reason.into(),
    }
}

/// The files of a search answer, one candidate each; malformed ones skipped.
pub fn parse_search(json: &Value) -> Result<Vec<Candidate>, OnlineError> {
    let Some(Value::Array(data)) = json.get("data") else {
        return Err(protocol("the search answer has no data list"));
    };
    let mut out = Vec::new();
    for item in data {
        let Some(attributes) = item.get("attributes") else {
            continue;
        };
        let text = |k: &str| {
            attributes
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let language = language_of_token(text("language"))
            .map(str::to_string)
            .or_else(|| (!text("language").is_empty()).then(|| text("language").to_lowercase()));
        let feature_episode = attributes
            .get("feature_details")
            .and_then(|f| f.get("episode_number"))
            .and_then(Value::as_u64)
            .and_then(|e| u32::try_from(e).ok());
        let machine = attributes.get("ai_translated") == Some(&Value::Bool(true))
            || attributes.get("machine_translated") == Some(&Value::Bool(true));
        let Some(Value::Array(files)) = attributes.get("files") else {
            continue;
        };
        for file in files {
            let (Some(id), Some(name)) = (
                file.get("file_id").and_then(Value::as_u64),
                file.get("file_name").and_then(Value::as_str),
            ) else {
                continue;
            };
            // OpenSubtitles serves every file as SRT
            let name = if FileKind::of(name) == FileKind::Subtitle {
                name.to_string()
            } else {
                format!("{name}.srt")
            };
            out.push(Candidate {
                provider: ProviderId::OpenSubtitles,
                locator: Locator::FileId(id),
                episode: feature_episode.or_else(|| subtitle_episode(&name)),
                kind: FileKind::Subtitle,
                file_name: name,
                source: text("release").to_string(),
                language: language.clone(),
                machine_translated: machine,
                size: None,
                order: out.len(),
            });
        }
    }
    Ok(out)
}

/// An OpenSubtitles client.
#[derive(Debug)]
pub struct OpenSubtitles {
    http: Http,
    base: Url,
    key: String,
    login: Option<(String, String)>,
    token: Option<String>,
    /// Downloads left today, as the last download said.
    pub remaining: Option<i64>,
}

impl OpenSubtitles {
    /// A client of the OpenSubtitles at `base` with the consumer key `key`,
    /// and a login when `username` is not empty.
    pub fn new(
        http: Http,
        base: &str,
        key: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, OnlineError> {
        if key.trim().is_empty() {
            return Err(OnlineError::NoKey {
                provider: NAME.into(),
            });
        }
        Ok(Self {
            http,
            base: checked_url(base)?,
            key: key.trim().to_string(),
            login: (!username.trim().is_empty())
                .then(|| (username.trim().to_string(), password.to_string())),
            token: None,
            remaining: None,
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, OnlineError> {
        let base = self.base.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}/{path}"))
            .map_err(|_| OnlineError::BadUrl(format!("{base}/{path}")))
    }

    fn headers(&self) -> Vec<(&str, String)> {
        let mut h = vec![
            ("Api-Key", self.key.clone()),
            ("Accept", "application/json".to_string()),
        ];
        if let Some(token) = &self.token {
            h.push(("Authorization", format!("Bearer {token}")));
        }
        h
    }

    fn log_in(&mut self) -> Result<(), OnlineError> {
        let Some((username, password)) = &self.login else {
            return Ok(());
        };
        let url = self.endpoint("login")?;
        let body = json!({"username": username, "password": password});
        self.token = None;
        let headers = self.headers();
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let answer = self.http.post_json(&url, &headers, &body)?.ok(NAME)?;
        let token = answer
            .json(NAME)?
            .get("token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| protocol("the login answer has no token"))?;
        self.token = Some(token);
        Ok(())
    }

    /// One search: the parameters sorted and in lower case, as the API wants
    /// them (it redirects otherwise).
    fn query(&self, mut params: Vec<(&str, String)>) -> Result<Vec<Candidate>, OnlineError> {
        params.sort();
        let mut url = self.endpoint("subtitles")?;
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in &params {
                pairs.append_pair(k, &v.to_lowercase());
            }
        }
        let headers = self.headers();
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let answer = self.http.get(&url, &headers)?.ok(NAME)?;
        parse_search(&answer.json(NAME)?)
    }

    fn download_link(&self, id: u64) -> Result<crate::http::Response, OnlineError> {
        let url = self.endpoint("download")?;
        let headers = self.headers();
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.http.post_json(&url, &headers, &json!({"file_id": id}))
    }
}

impl Provider for OpenSubtitles {
    fn id(&self) -> ProviderId {
        ProviderId::OpenSubtitles
    }

    fn search(&mut self, request: &SearchRequest) -> Result<Vec<Candidate>, OnlineError> {
        let language = || ("languages", request.language.clone());
        if let Some(hash) = request.hash {
            let found = self.query(vec![
                language(),
                ("moviebytesize", hash.size.to_string()),
                ("moviehash", hash.hex()),
            ])?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
        if request.title.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut params = vec![language(), ("query", request.title.trim().to_string())];
        if let Some(season) = request.season {
            params.push(("season_number", season.to_string()));
        }
        if let Some(episode) = request.episode {
            params.push(("episode_number", episode.to_string()));
        }
        self.query(params)
    }

    fn download(&mut self, candidate: &Candidate) -> Result<Vec<u8>, OnlineError> {
        let Locator::FileId(id) = candidate.locator else {
            return Err(protocol("not an OpenSubtitles file"));
        };
        if self.login.is_some() && self.token.is_none() {
            self.log_in()?;
        }
        let mut answer = self.download_link(id)?;
        if answer.status == 401 && self.login.is_some() {
            self.log_in()?;
            answer = self.download_link(id)?;
        }
        let json = answer.ok(NAME)?.json(NAME)?;
        self.remaining = json.get("remaining").and_then(Value::as_i64);
        let link = json
            .get("link")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("the download answer has no link"))?;
        // a temporary link: HTTPS, and no key
        let link = checked_url(link)?;
        Ok(self.http.file(&link, &[])?.ok(NAME)?.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-004
    #[test]
    fn search_answers() {
        let found = parse_search(&json!({"data": [
            {"attributes": {
                "language": "ja", "release": "Show S01E02", "ai_translated": false,
                "feature_details": {"season_number": 1, "episode_number": 2},
                "files": [{"file_id": 10, "file_name": "Show.S01E02.ja.srt"}]
            }},
            {"attributes": {
                "language": "ja", "release": "Show", "ai_translated": true,
                "files": [{"file_id": 11, "file_name": "show 02"}, {"file_name": "no id"}]
            }},
            {"junk": 1}
        ]}))
        .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].episode, Some(2));
        assert_eq!(found[0].language.as_deref(), Some("ja"));
        assert_eq!(found[0].locator, Locator::FileId(10));
        assert_eq!(found[1].file_name, "show 02.srt");
        assert!(found[1].machine_translated);
        assert!(parse_search(&json!([])).is_err());
    }

    #[test]
    fn a_key_is_needed() {
        let http = Http::new("").unwrap();
        assert!(matches!(
            OpenSubtitles::new(http, BASE, "", "", ""),
            Err(OnlineError::NoKey { .. })
        ));
    }
}
