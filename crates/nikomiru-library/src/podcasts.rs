// SPDX-License-Identifier: GPL-3.0-only

//! Podcast feeds and their episodes (POD-001…POD-004), in an additive table.
//! Downloaded files live in the data folder under names derived from the
//! identities; records only name them relatively.

use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::{Library, LibraryError, db::db_err};

const PODCASTS: TableDefinition<&str, &[u8]> = TableDefinition::new("podcasts");
/// Feeds at most.
pub const MAX_FEEDS: usize = 100;
/// Episodes of a feed at most (listed or kept for their download).
pub const MAX_EPISODES: usize = 1000;
/// Combined record size.
pub const MAX_BYTES: usize = 32 << 20;

/// A transcript a feed offers for an episode.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TranscriptRef {
    /// Its address.
    pub url: String,
    /// Its declared type.
    pub mime: String,
    /// Its declared language.
    pub language: String,
}

/// An episode.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Episode {
    /// Its identity (32 hex digits).
    pub id: String,
    /// Its GUID, if the feed has one.
    pub guid: String,
    /// Its title.
    pub title: String,
    /// Its web page.
    pub link: String,
    /// Its publication date, as the feed writes it.
    pub published: String,
    /// Its audio's address.
    pub audio_url: String,
    /// Its audio's declared type.
    pub mime: String,
    /// Its audio's declared length.
    pub length: Option<u64>,
    /// The first transcript the feed offers.
    pub transcript: Option<TranscriptRef>,
    /// The downloaded audio, relative to the podcasts folder.
    pub file: Option<String>,
    /// Its timed text beside it, relative to the podcasts folder.
    pub transcript_file: Option<String>,
    /// The feed still lists it.
    pub listed: bool,
}

/// A feed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Feed {
    /// Its identity (32 hex digits).
    pub id: String,
    /// Its address.
    pub url: String,
    /// Its title.
    pub title: String,
    /// Its web page.
    pub link: String,
    /// When it was last refreshed, Unix ms.
    pub refreshed_ms: i64,
    /// Its episodes, in feed order, then those kept for their download.
    pub episodes: Vec<Episode>,
}

fn hex32(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn plain(text: &str, limit: usize) -> bool {
    text.len() <= limit && !text.chars().any(char::is_control)
}

/// A relative file name the records may hold: `<feed>/<episode>[.lang].ext`.
pub fn safe_file(feed: &str, name: &str) -> bool {
    let Some((dir, file)) = name.split_once('/') else {
        return false;
    };
    dir == feed
        && file.len() <= 64
        && !file.starts_with('.')
        && !file.contains("..")
        && file
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}

impl Episode {
    fn valid(&self, feed: &str) -> bool {
        hex32(&self.id)
            && [
                &self.guid,
                &self.title,
                &self.link,
                &self.published,
                &self.audio_url,
                &self.mime,
            ]
            .into_iter()
            .all(|t| plain(t, 2048))
            && self.transcript.as_ref().is_none_or(|t| {
                plain(&t.url, 2048) && plain(&t.mime, 256) && plain(&t.language, 64)
            })
            && self.file.as_ref().is_none_or(|f| safe_file(feed, f))
            && self
                .transcript_file
                .as_ref()
                .is_none_or(|f| safe_file(feed, f))
    }

    fn bytes(&self) -> usize {
        self.guid.len()
            + self.title.len()
            + self.link.len()
            + self.published.len()
            + self.audio_url.len()
            + self.mime.len()
            + self
                .transcript
                .as_ref()
                .map_or(0, |t| t.url.len() + t.mime.len())
            + 64
    }
}

impl Feed {
    /// Bounds and identities.
    pub fn valid(&self) -> bool {
        let mut ids = std::collections::HashSet::new();
        hex32(&self.id)
            && plain(&self.url, 2048)
            && plain(&self.title, 2048)
            && plain(&self.link, 2048)
            && self.episodes.len() <= MAX_EPISODES
            && self
                .episodes
                .iter()
                .all(|e| e.valid(&self.id) && ids.insert(e.id.as_str()))
    }

    fn bytes(&self) -> usize {
        self.url.len()
            + self.title.len()
            + self.link.len()
            + self.episodes.iter().map(Episode::bytes).sum::<usize>()
    }

    /// The feed as refreshed into `fresh`: the episodes listed now, with the
    /// download and timed text of those already known, then the downloaded
    /// ones no longer listed (POD-001).
    pub fn merged(old: Option<&Feed>, mut fresh: Feed) -> Feed {
        let Some(old) = old else {
            return fresh;
        };
        for episode in &mut fresh.episodes {
            episode.listed = true;
            if let Some(known) = old.episodes.iter().find(|e| e.id == episode.id) {
                episode.file.clone_from(&known.file);
                episode.transcript_file.clone_from(&known.transcript_file);
            }
        }
        for kept in &old.episodes {
            if (kept.file.is_some() || kept.transcript_file.is_some())
                && !fresh.episodes.iter().any(|e| e.id == kept.id)
                && fresh.episodes.len() < MAX_EPISODES
            {
                let mut kept = kept.clone();
                kept.listed = false;
                fresh.episodes.push(kept);
            }
        }
        fresh
    }
}

impl Library {
    /// Every feed, by title; an absent table is empty.
    pub fn podcast_feeds(&self) -> Result<Vec<Feed>, LibraryError> {
        self.read(|tx| {
            let table = match tx.open_table(PODCASTS) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                Err(e) => return Err(LibraryError::Db(e.into())),
            };
            let mut out = Vec::new();
            let mut bytes = 0usize;
            for row in db_err!(table.iter())? {
                let (_, value) = db_err!(row)?;
                let feed: Feed = postcard::from_bytes(value.value())?;
                bytes += feed.bytes();
                if !feed.valid() || out.len() >= MAX_FEEDS || bytes > MAX_BYTES {
                    return Err(LibraryError::Record(postcard::Error::SerdeDeCustom));
                }
                out.push(feed);
            }
            out.sort_by(|a, b| {
                (a.title.to_lowercase(), &a.id).cmp(&(b.title.to_lowercase(), &b.id))
            });
            Ok(out)
        })
    }

    /// Saves a validated feed atomically; false if invalid or over the limits.
    pub fn put_podcast_feed(&self, feed: &Feed) -> Result<bool, LibraryError> {
        if !feed.valid() {
            return Ok(false);
        }
        let tx = self.write()?;
        {
            let mut table = db_err!(tx.open_table(PODCASTS))?;
            let mut count = 0usize;
            let mut bytes = feed.bytes();
            for row in db_err!(table.iter())? {
                let (key, value) = db_err!(row)?;
                if key.value() == feed.id {
                    continue;
                }
                let other: Feed = postcard::from_bytes(value.value())?;
                count += 1;
                bytes = bytes.saturating_add(other.bytes());
            }
            if count >= MAX_FEEDS || bytes > MAX_BYTES {
                return Ok(false);
            }
            let data = postcard::to_stdvec(feed)?;
            db_err!(table.insert(feed.id.as_str(), data.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(true)
    }

    /// Forgets a feed.
    pub fn remove_podcast_feed(&self, id: &str) -> Result<bool, LibraryError> {
        let tx = self.write()?;
        let changed = {
            let mut table = db_err!(tx.open_table(PODCASTS))?;
            db_err!(table.remove(id))?.is_some()
        };
        db_err!(tx.commit())?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(id: char, title: &str) -> Episode {
        Episode {
            id: id.to_string().repeat(32),
            title: title.into(),
            audio_url: format!("https://x/{id}.mp3"),
            listed: true,
            ..Episode::default()
        }
    }

    fn feed(episodes: Vec<Episode>) -> Feed {
        Feed {
            id: "f".repeat(32),
            url: "https://x/feed".into(),
            title: "Feed".into(),
            episodes,
            ..Feed::default()
        }
    }

    // T-POD-001: a refresh keeps downloads, also of episodes no longer listed.
    #[test]
    fn refreshes_keep_downloads() {
        let mut a = episode('a', "A");
        a.file = Some(format!("{}/{}.mp3", "f".repeat(32), "a".repeat(32)));
        let mut b = episode('b', "B");
        b.transcript_file = Some(format!("{}/{}.ja.vtt", "f".repeat(32), "b".repeat(32)));
        let old = feed(vec![a.clone(), b.clone(), episode('c', "C")]);
        let fresh = feed(vec![episode('a', "A renamed"), episode('d', "D")]);
        let merged = Feed::merged(Some(&old), fresh);
        let ids: Vec<_> = merged
            .episodes
            .iter()
            .map(|e| (e.id.chars().next().unwrap(), e.listed))
            .collect();
        assert_eq!(ids, [('a', true), ('d', true), ('b', false)]);
        assert_eq!(merged.episodes[0].title, "A renamed");
        assert_eq!(merged.episodes[0].file, a.file);
        assert!(merged.valid());
    }

    // T-POD-001, T-POD-004: real records, bounds, unsafe names and restart.
    #[test]
    fn feeds_persist_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.redb");
        let lib = Library::open(&path).unwrap();
        assert!(lib.podcast_feeds().unwrap().is_empty());
        let f = feed(vec![episode('a', "A")]);
        assert!(lib.put_podcast_feed(&f).unwrap());
        let mut unsafe_name = f.clone();
        unsafe_name.episodes[0].file = Some("../../etc/passwd".into());
        assert!(!lib.put_podcast_feed(&unsafe_name).unwrap());
        let mut dup = f.clone();
        dup.episodes.push(episode('a', "again"));
        assert!(!lib.put_podcast_feed(&dup).unwrap());
        let mut many = f.clone();
        many.episodes = (0..=MAX_EPISODES)
            .map(|i| Episode {
                id: format!("{i:032x}"),
                ..Episode::default()
            })
            .collect();
        assert!(!lib.put_podcast_feed(&many).unwrap());
        drop(lib);
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.podcast_feeds().unwrap(), vec![f.clone()]);
        assert!(lib.remove_podcast_feed(&f.id).unwrap());
        assert!(lib.podcast_feeds().unwrap().is_empty());
        assert!(safe_file(
            &"f".repeat(32),
            &format!("{}/{}.ja.vtt", "f".repeat(32), "0".repeat(32))
        ));
        assert!(!safe_file(&"f".repeat(32), "other/x.mp3"));
        assert!(!safe_file(
            &"f".repeat(32),
            &format!("{}/../x", "f".repeat(32))
        ));
    }
}
