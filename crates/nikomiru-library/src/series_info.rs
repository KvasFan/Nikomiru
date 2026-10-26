// SPDX-License-Identifier: GPL-3.0-only

//! What AniList or Jikan said of a series (MET-001, MET-002), in a table of
//! its own (§8 `series_info`): the records of the series stay as they are,
//! and a library without it is read as one with no series identified.

use redb::TableDefinition;
use serde::{Deserialize, Serialize};

use crate::db::{Library, LibraryError, db_err};

/// By series id.
const SERIES_INFO: TableDefinition<u64, &[u8]> = TableDefinition::new("series_info");

/// A series as identified online.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SeriesInfoRecord {
    /// Where it came from: `anilist` or `jikan`.
    pub source: String,
    /// Its AniList id: Jimaku is searched by it (MET-003).
    pub anilist_id: Option<u64>,
    /// Its MyAnimeList id.
    pub mal_id: Option<u64>,
    /// Its title in romaji.
    pub romaji: String,
    /// In English.
    pub english: Option<String>,
    /// In Japanese.
    pub native: Option<String>,
    /// Its episodes, when known.
    pub episodes: Option<u32>,
    /// The year it started.
    pub year: Option<u32>,
    /// TV, movie, OVA…
    pub format: Option<String>,
    /// Its page on the source.
    pub page_url: Option<String>,
    /// When it was identified (Unix ms).
    pub identified_at: i64,
}

impl Library {
    /// What is known online of series `id`.
    pub fn series_info(&self, id: u64) -> Result<Option<SeriesInfoRecord>, LibraryError> {
        self.read(|tx| match tx.open_table(SERIES_INFO) {
            Ok(t) => match db_err!(t.get(id))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            },
            Err(redb::TableError::TableDoesNotExist(_)) => Ok(None),
            Err(e) => Err(LibraryError::Db(e.into())),
        })
    }

    /// Sets what is known online of series `id`, or forgets it (`None`).
    pub fn set_series_info(
        &self,
        id: u64,
        info: Option<&SeriesInfoRecord>,
    ) -> Result<(), LibraryError> {
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(SERIES_INFO))?;
            match info {
                Some(info) => {
                    let bytes = postcard::to_allocvec(info)?;
                    db_err!(t.insert(id, bytes.as_slice()))?;
                }
                None => {
                    db_err!(t.remove(id))?;
                }
            }
        }
        db_err!(tx.commit())
    }

    /// The AniList id of the series of video `uid`, if it was identified.
    pub fn anilist_id_of_video(&self, uid: &str) -> Result<Option<u64>, LibraryError> {
        let Some(series) = self.video(uid)?.and_then(|v| v.series) else {
            return Ok(None);
        };
        Ok(self.series_info(series)?.and_then(|i| i.anilist_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::VideoRecord;

    #[test]
    fn a_series_keeps_what_was_found_online() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        assert_eq!(lib.series_info(1).unwrap(), None);
        let id = 7;
        let info = SeriesInfoRecord {
            source: "anilist".into(),
            anilist_id: Some(154_587),
            romaji: "Sousou no Frieren".into(),
            episodes: Some(28),
            ..SeriesInfoRecord::default()
        };
        lib.set_series_info(id, Some(&info)).unwrap();
        assert_eq!(lib.series_info(id).unwrap(), Some(info));
        let video = VideoRecord {
            series: Some(id),
            ..VideoRecord::default()
        };
        lib.put_video("video/frieren-01", &video).unwrap();
        assert_eq!(
            lib.anilist_id_of_video("video/frieren-01").unwrap(),
            Some(154_587)
        );
        assert_eq!(lib.anilist_id_of_video("video/other").unwrap(), None);
        lib.set_series_info(id, None).unwrap();
        assert_eq!(lib.series_info(id).unwrap(), None);
    }
}
