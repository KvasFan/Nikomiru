// SPDX-License-Identifier: GPL-3.0-only

//! Word audio from URL templates (AUD-002, AUD-004, AUD-008): the template
//! filled with the term and its reading, the answer read as a Yomitan list
//! or as the audio itself, the hosts that cannot be reached paused, and the
//! files kept in a cache of limited size.
//!
//! After Fushi `fushi/lib/src/media/audio/word_audio_resolver.dart` (commit
//! `ac8034e`), which takes only the Yomitan list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::OnlineError;
use crate::http::{Http, checked_url};

/// How long connecting may take (AUD-004).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// How long an answer may take (AUD-004).
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a host that could not be reached is left alone (AUD-004).
pub const HOST_PAUSE: Duration = Duration::from_secs(45);

/// The cache's size (AUD-008).
pub const CACHE_LIMIT: u64 = 500 << 20;

/// `text` as `encodeURIComponent` writes it: what is not a letter, a digit
/// or one of `-_.!~*'()` as `%XX` of its UTF-8 bytes.
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The address of a template for a word (AUD-002): `{term}` and `{reading}`
/// encoded; an empty reading is the term.
pub fn fill(template: &str, term: &str, reading: &str) -> String {
    let reading = if reading.is_empty() { term } else { reading };
    template
        .replace("{term}", &encode_component(term))
        .replace("{reading}", &encode_component(reading))
}

/// What a source answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A Yomitan list: the names and addresses of the audio files.
    List(Vec<(String, String)>),
    /// The audio itself.
    Audio(Vec<u8>),
    /// Nothing usable.
    Nothing,
}

/// Reads an answer (AUD-002): `audio/*` is the audio, a JSON
/// `audioSourceList` gives the files (an address relative to `base` taken
/// from it).
pub fn parse(content_type: Option<&str>, body: &[u8], base: &reqwest::Url) -> Answer {
    if content_type.is_some_and(|t| t.starts_with("audio/")) {
        return Answer::Audio(body.to_vec());
    }
    let Ok(json) = serde_json::from_slice::<Value>(body) else {
        return Answer::Nothing;
    };
    if json["type"] != "audioSourceList" {
        return Answer::Nothing;
    }
    let list = json["audioSources"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let url = base.join(s["url"].as_str()?).ok()?;
                    let name = s["name"].as_str().unwrap_or_default().to_string();
                    Some((name, url.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Answer::List(list)
}

/// An audio file a source gives for a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordAudio {
    /// The source's name.
    pub source: String,
    /// The name the source gives it, if any.
    pub name: String,
    /// Where it is (the key of the cache).
    pub url: String,
    /// The audio, when the source answered with it.
    pub bytes: Option<Vec<u8>>,
}

/// Asks the sources, pausing the hosts that cannot be reached.
#[derive(Debug)]
pub struct Resolver {
    http: Http,
    paused: Mutex<HashMap<String, Instant>>,
}

impl Resolver {
    /// A resolver through `proxy` (SET-007).
    pub fn new(proxy: &str) -> Result<Self, OnlineError> {
        Ok(Self {
            http: Http::with_timeouts(proxy, CONNECT_TIMEOUT, RESPONSE_TIMEOUT)?,
            paused: Mutex::new(HashMap::new()),
        })
    }

    fn paused(&self, url: &reqwest::Url) -> bool {
        let host = url.host_str().unwrap_or_default().to_string();
        let mut paused = self.paused.lock().unwrap_or_else(|e| e.into_inner());
        match paused.get(&host) {
            Some(until) if Instant::now() < *until => true,
            Some(_) => {
                paused.remove(&host);
                false
            }
            None => false,
        }
    }

    fn pause(&self, url: &reqwest::Url) {
        let host = url.host_str().unwrap_or_default().to_string();
        self.paused
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(host, Instant::now() + HOST_PAUSE);
    }

    /// What the source `name` with `template` has for the word (AUD-002,
    /// AUD-004): nothing on a 4xx answer; an error for a host that cannot
    /// be reached, which is then left alone for 45 s.
    pub fn query(
        &self,
        name: &str,
        template: &str,
        term: &str,
        reading: &str,
    ) -> Result<Vec<WordAudio>, OnlineError> {
        let url = checked_url(&fill(template, term, reading))?;
        if self.paused(&url) {
            return Err(OnlineError::Unreachable {
                host: url.host_str().unwrap_or_default().to_string(),
                reason: "paused".into(),
            });
        }
        let response = self.http.file(&url, &[]).inspect_err(|e| {
            if matches!(
                e,
                OnlineError::Unreachable { .. } | OnlineError::Timeout { .. }
            ) {
                self.pause(&url);
            }
        })?;
        if (400..500).contains(&response.status) {
            return Ok(Vec::new());
        }
        let response = response.ok(name)?;
        Ok(
            match parse(response.content_type.as_deref(), &response.body, &url) {
                Answer::Audio(bytes) => vec![WordAudio {
                    source: name.to_string(),
                    name: name.to_string(),
                    url: url.to_string(),
                    bytes: Some(bytes),
                }],
                Answer::List(list) => list
                    .into_iter()
                    .map(|(n, u)| WordAudio {
                        source: name.to_string(),
                        name: n,
                        url: u,
                        bytes: None,
                    })
                    .collect(),
                Answer::Nothing => Vec::new(),
            },
        )
    }

    /// The audio at `url`, at most 8 MiB.
    pub fn fetch(&self, url: &str) -> Result<Vec<u8>, OnlineError> {
        let url = checked_url(url)?;
        let response = self
            .http
            .file(&url, &[])?
            .ok(url.host_str().unwrap_or_default())?;
        if response.body.len() > 8 << 20 {
            return Err(OnlineError::TooLarge { limit_mib: 8 });
        }
        Ok(response.body)
    }
}

/// The cache of word audio (AUD-008): a file per address, the least
/// recently used ones removed above the limit.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
    limit: u64,
}

/// FNV-1a: a name for an address that stays the same between versions.
fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

impl Cache {
    /// A cache in `dir` of at most `limit` bytes.
    pub fn new(dir: PathBuf, limit: u64) -> Self {
        Self { dir, limit }
    }

    /// The file of `url`.
    pub fn path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.audio", fnv(url)))
    }

    /// The file of `url` if it is kept, marked as used now.
    pub fn get(&self, url: &str) -> Option<PathBuf> {
        let path = self.path(url);
        let file = std::fs::File::options().write(true).open(&path).ok()?;
        let _ = file.set_modified(std::time::SystemTime::now());
        Some(path)
    }

    /// Keeps `bytes` for `url`, then makes room; its file.
    pub fn put(&self, url: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(url);
        let partial = path.with_extension("part");
        std::fs::write(&partial, bytes)?;
        std::fs::rename(&partial, &path)?;
        self.evict(&path);
        Ok(path)
    }

    /// Removes the least recently used files until the cache fits, never
    /// `keep`.
    fn evict(&self, keep: &Path) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                Some((meta.modified().ok()?, meta.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        files.sort();
        for (_, len, path) in files {
            if total <= self.limit {
                break;
            }
            if path != keep && std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-AUD-002
    #[test]
    fn templates_and_answers() {
        assert_eq!(
            fill(
                "http://localhost:5050/?term={term}&reading={reading}",
                "食べる",
                "たべる"
            ),
            "http://localhost:5050/?term=%E9%A3%9F%E3%81%B9%E3%82%8B&reading=%E3%81%9F%E3%81%B9%E3%82%8B"
        );
        assert_eq!(fill("x?r={reading}", "a b&c", ""), "x?r=a%20b%26c");
        let base = reqwest::Url::parse("http://localhost:5050/list?term=x").unwrap();
        let list = br#"{"type":"audioSourceList","audioSources":[{"name":"NHK","url":"/a/1.mp3"},{"name":"x","url":"https://cdn.example.org/2.mp3"},{"bad":1}]}"#;
        assert_eq!(
            parse(Some("application/json"), list, &base),
            Answer::List(vec![
                ("NHK".into(), "http://localhost:5050/a/1.mp3".into()),
                ("x".into(), "https://cdn.example.org/2.mp3".into()),
            ])
        );
        assert_eq!(
            parse(Some("audio/mpeg"), b"ID3", &base),
            Answer::Audio(b"ID3".to_vec())
        );
        assert_eq!(parse(None, b"<html>", &base), Answer::Nothing);
        assert_eq!(parse(None, br#"{"type":"other"}"#, &base), Answer::Nothing);
    }

    // T-AUD-008
    #[test]
    fn the_cache_drops_the_least_recently_used() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path().join("audio"), 25);
        let old = std::time::SystemTime::now() - Duration::from_secs(100);
        cache.put("a", &[0; 10]).unwrap();
        cache.put("b", &[0; 10]).unwrap();
        for url in ["a", "b"] {
            let f = std::fs::File::options()
                .write(true)
                .open(cache.path(url))
                .unwrap();
            f.set_modified(old).unwrap();
        }
        // `a` used again: `b` is the least recently used
        assert!(cache.get("a").is_some());
        cache.put("c", &[0; 10]).unwrap();
        assert!(cache.path("a").exists());
        assert!(!cache.path("b").exists());
        assert!(cache.path("c").exists());
        assert!(cache.get("b").is_none());
        // a file larger than the cache is still kept, alone
        cache.put("d", &[0; 40]).unwrap();
        assert!(cache.path("d").exists());
    }
}
