// SPDX-License-Identifier: GPL-3.0-only

//! Podcasts from the interface (POD-001…POD-004): the page's state, its
//! library jobs and the network work that runs off the interface's thread.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_core::Cancel;
use nikomiru_library::podcasts::{Episode, Feed, TranscriptRef};
use nikomiru_online::podcast;

/// A serial library operation.
#[derive(Debug, Clone)]
pub enum Job {
    /// Refresh the list.
    List,
    /// Merge a fetched feed into its record (or add it).
    Refresh(Feed),
    /// Forget a feed and delete its files.
    Remove(String),
    /// Record a finished download.
    Downloaded {
        /// The feed.
        feed: String,
        /// The episode.
        episode: String,
        /// The audio, relative to the podcasts folder.
        file: String,
        /// Its timed text, relative too.
        transcript: Option<String>,
    },
    /// Delete an episode's download and timed text.
    DeleteDownload {
        /// The feed.
        feed: String,
        /// The episode.
        episode: String,
    },
    /// Use a local timed-text file for an episode.
    Import {
        /// The feed.
        feed: String,
        /// The episode.
        episode: String,
        /// The file chosen.
        path: PathBuf,
    },
}

/// The feeds after a job, or why it failed.
pub type Answer = Result<Vec<Feed>, String>;

/// A download running.
#[derive(Debug, Clone)]
pub struct Download {
    /// The feed.
    pub feed: String,
    /// The episode.
    pub episode: String,
    /// Bytes so far.
    pub done: u64,
    /// The whole, if known.
    pub total: Option<u64>,
    /// Its cancellation.
    pub cancel: Cancel,
}

/// What a download came to: the audio and its timed text, relative to the
/// podcasts folder, and why there is no transcript, if there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The audio.
    pub file: String,
    /// Its timed text.
    pub transcript: Option<String>,
    /// A transcript was offered but not usable: its type.
    pub unsupported: Option<String>,
}

/// The page's state.
#[derive(Debug, Default)]
pub struct State {
    /// The page shows.
    pub open: bool,
    /// The feeds.
    pub feeds: Vec<Feed>,
    /// A library job runs.
    pub busy: bool,
    /// A feed is being fetched: its address.
    pub fetching: Option<String>,
    /// A download runs.
    pub download: Option<Download>,
    /// Incremented by every network request and cancel: older answers drop.
    pub epoch: u64,
    /// The last error.
    pub error: Option<String>,
    /// The last notice.
    pub notice: Option<String>,
    /// The episode whose timed text is being imported.
    pub importing: Option<(String, String)>,
}

impl State {
    /// An episode by its feed and id.
    pub fn episode(&self, feed: &str, episode: &str) -> Option<(&Feed, &Episode)> {
        let f = self.feeds.iter().find(|f| f.id == feed)?;
        Some((f, f.episodes.iter().find(|e| e.id == episode)?))
    }
}

/// The record of a fetched feed: its identities, and its first transcript
/// of each episode.
pub fn record(url: &str, parsed: podcast::Feed, now_ms: i64) -> Feed {
    let id = podcast::feed_id(url);
    let episodes = parsed
        .items
        .iter()
        .filter_map(|item| {
            let enclosure = item.enclosure.as_ref()?;
            Some(Episode {
                id: podcast::episode_id(url, item)?,
                guid: item.guid.clone(),
                title: if item.title.is_empty() {
                    enclosure.url.clone()
                } else {
                    item.title.clone()
                },
                link: item.link.clone(),
                published: item.published.clone(),
                audio_url: enclosure.url.clone(),
                mime: enclosure.mime.clone(),
                length: enclosure.length,
                transcript: item.transcripts.first().map(|t| TranscriptRef {
                    url: t.url.clone(),
                    mime: t.mime.clone(),
                    language: t.language.clone(),
                }),
                file: None,
                transcript_file: None,
                listed: true,
            })
        })
        .collect();
    Feed {
        id,
        url: url.trim().to_string(),
        title: parsed.title,
        link: parsed.link,
        refreshed_ms: now_ms,
        episodes,
    }
}

/// The HTTP client of the podcasts: long downloads may last hours.
fn http(proxy: &str) -> Result<nikomiru_online::http::Http, String> {
    nikomiru_online::http::Http::with_timeouts(
        proxy,
        nikomiru_online::http::CONNECT_TIMEOUT,
        Duration::from_secs(6 * 3600),
    )
    .map_err(|e| e.to_string())
}

/// Fetches and reads a feed.
pub fn fetch(proxy: &str, url: &str, cancel: &Cancel) -> Result<podcast::Feed, String> {
    podcast::fetch(&http(proxy)?, url, cancel).map_err(|e| e.to_string())
}

/// A language usable in a sidecar's name, or none.
fn language(text: &str) -> Option<String> {
    let lower = text.trim().to_ascii_lowercase();
    (!lower.is_empty()
        && lower.len() <= 16
        && lower.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
    .then_some(lower)
}

/// Whether `bytes` is timed text of `ext` with at least one line.
pub fn timed_text(ext: &str, bytes: &[u8]) -> bool {
    let Some(format) = nikomiru_subs::Format::from_extension(ext) else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)) else {
        return false;
    };
    !nikomiru_subs::parse(format, text).cues.is_empty()
}

/// Where an episode's audio is downloaded, relative to the podcasts folder.
pub fn audio_name(feed: &str, episode: &Episode) -> Option<String> {
    let enclosure = podcast::Enclosure {
        url: episode.audio_url.clone(),
        mime: episode.mime.clone(),
        length: episode.length,
    };
    let ext = podcast::audio_extension(&enclosure)?;
    Some(format!("{feed}/{}.{ext}", episode.id))
}

/// Whether an episode has a partial download to resume.
pub fn partial(dir: &Path, feed: &str, episode: &Episode) -> bool {
    audio_name(feed, episode)
        .is_some_and(|name| nikomiru_online::setup::fetch::part_path(&dir.join(name)).is_file())
}

/// Downloads an episode's audio, then the transcript its feed offers when
/// Nikomiru reads it (POD-002, POD-003).
pub fn download(
    proxy: &str,
    dir: &Path,
    feed: &str,
    episode: &Episode,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Fetched, String> {
    let http = http(proxy)?;
    let file = audio_name(feed, episode).ok_or("not audio")?;
    podcast::download(
        &http,
        &episode.audio_url,
        &dir.join(&file),
        cancel,
        progress,
    )
    .map_err(|e| e.to_string())?;
    let mut fetched = Fetched {
        file,
        transcript: None,
        unsupported: None,
    };
    if let Some(t) = &episode.transcript {
        match podcast::transcript_extension(&t.mime) {
            Some(ext) => {
                let bytes =
                    podcast::transcript(&http, &t.url, cancel).map_err(|e| e.to_string())?;
                if !timed_text(ext, &bytes) {
                    fetched.unsupported = Some(t.mime.clone());
                } else {
                    let name = match language(&t.language) {
                        Some(lang) => format!("{feed}/{}.{lang}.{ext}", episode.id),
                        None => format!("{feed}/{}.{ext}", episode.id),
                    };
                    nikomiru_core::fs::write_atomic(&dir.join(&name), &bytes, 0o600)
                        .map_err(|e| e.to_string())?;
                    fetched.transcript = Some(name);
                }
            }
            None => fetched.unsupported = Some(t.mime.clone()),
        }
    }
    Ok(fetched)
}

/// The largest imported timed text.
const MAX_IMPORT: u64 = 4 << 20;

/// Copies a local SRT/WebVTT/ASS/LRC file beside an episode's audio, after
/// checking it, replacing the timed text it had.
pub fn import(dir: &Path, feed: &str, episode: &Episode, path: &Path) -> Result<String, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), "srt" | "vtt" | "ass" | "ssa" | "lrc"))
        .ok_or("unsupported")?;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_IMPORT {
        return Err("too large".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if !timed_text(&ext, &bytes) {
        return Err("no timed text".into());
    }
    let name = format!("{feed}/{}.{ext}", episode.id);
    nikomiru_core::fs::write_atomic(&dir.join(&name), &bytes, 0o600).map_err(|e| e.to_string())?;
    if let Some(old) = episode.transcript_file.as_ref().filter(|o| **o != name) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    Ok(name)
}

/// Runs a library job; files are changed only for validated relative names.
pub fn run(lib: &nikomiru_library::Library, dir: &Path, job: Job) -> Answer {
    let err = |e: nikomiru_library::LibraryError| e.to_string();
    let find = |feed: &str| -> Result<Feed, String> {
        lib.podcast_feeds()
            .map_err(err)?
            .into_iter()
            .find(|f| f.id == feed)
            .ok_or_else(|| "feed".to_string())
    };
    let save = |feed: &Feed| -> Result<(), String> {
        if lib.put_podcast_feed(feed).map_err(err)? {
            Ok(())
        } else {
            Err("limits".into())
        }
    };
    match job {
        Job::List => {}
        Job::Refresh(fresh) => {
            let old = lib
                .podcast_feeds()
                .map_err(err)?
                .into_iter()
                .find(|f| f.id == fresh.id);
            save(&Feed::merged(old.as_ref(), fresh))?;
        }
        Job::Remove(id) => {
            if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
                let folder = dir.join(&id);
                if folder.is_dir() {
                    std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
                }
            }
            lib.remove_podcast_feed(&id).map_err(err)?;
        }
        Job::Downloaded {
            feed,
            episode,
            file,
            transcript,
        } => {
            let mut f = find(&feed)?;
            let e = f
                .episodes
                .iter_mut()
                .find(|e| e.id == episode)
                .ok_or("episode")?;
            e.file = Some(file);
            if transcript.is_some() {
                e.transcript_file = transcript;
            }
            save(&f)?;
        }
        Job::DeleteDownload { feed, episode } => {
            let mut f = find(&feed)?;
            let id = f.id.clone();
            let e = f
                .episodes
                .iter_mut()
                .find(|e| e.id == episode)
                .ok_or("episode")?;
            for name in [e.file.take(), e.transcript_file.take()]
                .into_iter()
                .flatten()
            {
                if nikomiru_library::podcasts::safe_file(&id, &name) {
                    let _ = std::fs::remove_file(dir.join(&name));
                }
            }
            // a partial download goes too
            if let Some(name) = audio_name(&id, e) {
                let _ =
                    std::fs::remove_file(nikomiru_online::setup::fetch::part_path(&dir.join(name)));
            }
            save(&f)?;
        }
        Job::Import {
            feed,
            episode,
            path,
        } => {
            let mut f = find(&feed)?;
            let id = f.id.clone();
            let e = f
                .episodes
                .iter_mut()
                .find(|e| e.id == episode)
                .ok_or("episode")?;
            e.transcript_file = Some(import(dir, &id, e, &path)?);
            save(&f)?;
        }
    }
    lib.podcast_feeds().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-POD-001, T-POD-003: records from a parsed feed; imports are checked and replace.
    #[test]
    fn feeds_become_records_and_imports_are_checked() {
        let parsed = podcast::Feed {
            title: "T".into(),
            link: String::new(),
            items: vec![
                podcast::Item {
                    guid: "g".into(),
                    enclosure: Some(podcast::Enclosure {
                        url: "https://x/1.mp3".into(),
                        mime: "audio/mpeg".into(),
                        length: None,
                    }),
                    ..Default::default()
                },
                podcast::Item {
                    guid: "no-audio".into(),
                    ..Default::default()
                },
            ],
        };
        let feed = record("https://x/feed", parsed, 5);
        assert_eq!(
            feed.episodes.len(),
            1,
            "items without audio are not episodes"
        );
        assert_eq!(feed.episodes[0].title, "https://x/1.mp3");
        assert!(feed.valid());
        let dir = tempfile::tempdir().unwrap();
        let lib = nikomiru_library::Library::open(&dir.path().join("l.redb")).unwrap();
        let data = dir.path().join("podcasts");
        let feeds = run(&lib, &data, Job::Refresh(feed.clone())).unwrap();
        assert_eq!(feeds.len(), 1);
        let lrc = dir.path().join("song.lrc");
        std::fs::write(&lrc, "[00:01.00]猫\n[00:03.00]犬\n").unwrap();
        let bad = dir.path().join("bad.lrc");
        std::fs::write(&bad, "no times").unwrap();
        let ep = feed.episodes[0].id.clone();
        let import = |path: &Path| {
            run(
                &lib,
                &data,
                Job::Import {
                    feed: feed.id.clone(),
                    episode: ep.clone(),
                    path: path.to_path_buf(),
                },
            )
        };
        assert!(import(&bad).is_err());
        let feeds = import(&lrc).unwrap();
        let name = feeds[0].episodes[0].transcript_file.clone().unwrap();
        assert!(name.ends_with(".lrc"));
        assert!(data.join(&name).is_file());
        let srt = dir.path().join("a.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\n猫\n").unwrap();
        let feeds = import(&srt).unwrap();
        assert!(!data.join(&name).exists(), "the old timed text goes");
        assert!(
            feeds[0].episodes[0]
                .transcript_file
                .as_ref()
                .unwrap()
                .ends_with(".srt")
        );
        let feeds = run(&lib, &data, Job::Remove(feed.id.clone())).unwrap();
        assert!(feeds.is_empty());
        assert!(!data.join(&feed.id).exists());
    }
}
