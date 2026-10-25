// SPDX-License-Identifier: GPL-3.0-only

//! What the home page shows (LIB-005, LIB-008): "Continue watching", the
//! series with the state of their episodes, a series' episodes in order;
//! and the user's edits of the library (LIB-010…LIB-012).

use std::collections::BTreeMap;

use redb::ReadableTable;

use crate::db::{
    FolderRecord, Library, LibraryError, SERIES, SeriesPrefs, SeriesRecord, VIDEOS, VideoRecord,
    db_err,
};
use crate::grouping::natural_cmp;

/// The state of an episode (LIB-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watched {
    /// Never played.
    New,
    /// Started, not finished.
    Started,
    /// Played to 90% at least once.
    Watched,
}

impl Watched {
    /// The state of `v`.
    pub fn of(v: &VideoRecord) -> Self {
        if v.completed_at.is_some() {
            Self::Watched
        } else if v.last_position_ms > 0 || v.last_played_at.is_some() {
            Self::Started
        } else {
            Self::New
        }
    }
}

/// A series on the home page.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesSummary {
    /// Its id.
    pub id: u64,
    /// Its record.
    pub record: SeriesRecord,
    /// Its episodes.
    pub episodes: usize,
    /// Those watched.
    pub watched: usize,
    /// Those started and not finished.
    pub started: usize,
    /// The last time one was played (Unix ms).
    pub last_played_at: Option<i64>,
}

/// The key of a series made by hand (LIB-011), which no scan gives.
fn manual_key(id: u64) -> String {
    format!("\u{1}manual/{id}")
}

/// Orders episodes by season, episode and name.
fn episode_order(a: &VideoRecord, b: &VideoRecord) -> std::cmp::Ordering {
    a.season
        .unwrap_or(1)
        .cmp(&b.season.unwrap_or(1))
        .then_with(|| {
            a.episode
                .unwrap_or(u32::MAX)
                .cmp(&b.episode.unwrap_or(u32::MAX))
        })
        .then_with(|| natural_cmp(&a.path, &b.path))
}

impl Library {
    /// Every video, by uid.
    pub fn videos(&self) -> Result<Vec<(String, VideoRecord)>, LibraryError> {
        self.read(|tx| {
            let t = db_err!(tx.open_table(VIDEOS))?;
            let mut out = Vec::new();
            for row in db_err!(t.iter())? {
                let (k, v) = db_err!(row)?;
                out.push((k.value().to_string(), postcard::from_bytes(v.value())?));
            }
            Ok(out)
        })
    }

    /// The series `id`.
    pub fn series(&self, id: u64) -> Result<Option<SeriesRecord>, LibraryError> {
        self.read(|tx| {
            let t = db_err!(tx.open_table(SERIES))?;
            match db_err!(t.get(id))? {
                Some(v) => Ok(Some(postcard::from_bytes(v.value())?)),
                None => Ok(None),
            }
        })
    }

    fn all_series(&self) -> Result<BTreeMap<u64, SeriesRecord>, LibraryError> {
        self.read(|tx| {
            let t = db_err!(tx.open_table(SERIES))?;
            let mut out = BTreeMap::new();
            for row in db_err!(t.iter())? {
                let (k, v) = db_err!(row)?;
                out.insert(k.value(), postcard::from_bytes(v.value())?);
            }
            Ok(out)
        })
    }

    fn put_series(&self, id: u64, record: &SeriesRecord) -> Result<(), LibraryError> {
        let bytes = postcard::to_stdvec(record)?;
        let tx = self.write()?;
        {
            let mut t = db_err!(tx.open_table(SERIES))?;
            db_err!(t.insert(id, bytes.as_slice()))?;
        }
        db_err!(tx.commit())
    }

    pub(crate) fn edit_series(
        &self,
        id: u64,
        edit: impl FnOnce(&mut SeriesRecord),
    ) -> Result<(), LibraryError> {
        if let Some(mut s) = self.series(id)? {
            edit(&mut s);
            self.put_series(id, &s)?;
        }
        Ok(())
    }

    pub(crate) fn edit_video(
        &self,
        uid: &str,
        edit: impl FnOnce(&mut VideoRecord),
    ) -> Result<(), LibraryError> {
        if let Some(mut v) = self.video(uid)? {
            edit(&mut v);
            self.put_video(uid, &v)?;
        }
        Ok(())
    }

    /// Whether `s` is on the home page: not deleted or merged, and of the
    /// grouping its folder has now.
    fn shown(s: &SeriesRecord, folders: &BTreeMap<String, FolderRecord>) -> bool {
        !s.deleted
            && s.merged_into.is_none()
            && folders
                .get(&s.root)
                .is_none_or(|f| f.grouping == s.grouping)
    }

    fn folder_map(&self) -> Result<BTreeMap<String, FolderRecord>, LibraryError> {
        Ok(self.folders()?.into_iter().collect())
    }

    /// The series of the home page, with their episodes' state, by name
    /// (LIB-005, LIB-008).
    pub fn series_list(&self) -> Result<Vec<SeriesSummary>, LibraryError> {
        let folders = self.folder_map()?;
        let videos = self.videos()?;
        let mut out: Vec<SeriesSummary> = self
            .all_series()?
            .into_iter()
            .filter(|(_, s)| Self::shown(s, &folders))
            .map(|(id, record)| {
                let mine: Vec<&VideoRecord> = videos
                    .iter()
                    .map(|(_, v)| v)
                    .filter(|v| v.series == Some(id) && !v.removed)
                    .collect();
                SeriesSummary {
                    id,
                    record,
                    episodes: mine.len(),
                    watched: mine
                        .iter()
                        .filter(|v| Watched::of(v) == Watched::Watched)
                        .count(),
                    started: mine
                        .iter()
                        .filter(|v| Watched::of(v) == Watched::Started)
                        .count(),
                    last_played_at: mine.iter().filter_map(|v| v.last_played_at).max(),
                }
            })
            .filter(|s| s.episodes > 0)
            .collect();
        out.sort_by(|a, b| natural_cmp(&a.record.title, &b.record.title));
        Ok(out)
    }

    /// The episodes of a series, by season, episode and name.
    pub fn episodes(&self, series: u64) -> Result<Vec<(String, VideoRecord)>, LibraryError> {
        let mut out: Vec<(String, VideoRecord)> = self
            .videos()?
            .into_iter()
            .filter(|(_, v)| v.series == Some(series) && !v.removed)
            .collect();
        out.sort_by(|a, b| episode_order(&a.1, &b.1));
        Ok(out)
    }

    /// "Continue watching" (LIB-005): the videos started and not finished,
    /// the last played first, at most `limit`.
    pub fn continue_watching(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, VideoRecord)>, LibraryError> {
        let series = self.all_series()?;
        let mut out: Vec<(String, VideoRecord)> = self
            .videos()?
            .into_iter()
            .filter(|(_, v)| {
                Watched::of(v) == Watched::Started
                    && v.last_played_at.is_some()
                    && !v.removed
                    && v.series
                        .is_none_or(|id| series.get(&id).is_none_or(|s| !s.deleted))
            })
            .collect();
        out.sort_by_key(|(_, v)| std::cmp::Reverse(v.last_played_at));
        out.truncate(limit);
        Ok(out)
    }

    /// Renames a series (LIB-011); a scan keeps the name.
    pub fn rename_series(&self, id: u64, title: &str) -> Result<(), LibraryError> {
        let title = title.trim().to_string();
        self.edit_series(id, |s| s.title = title)
    }

    /// Deletes a series from the library (LIB-004, LIB-012): a scan does not
    /// make it again; its files stay.
    pub fn delete_series(&self, id: u64) -> Result<(), LibraryError> {
        self.edit_series(id, |s| s.deleted = true)
    }

    /// Merges `from` into `into` (LIB-011): its episodes go there, now and
    /// at every scan.
    pub fn merge_series(&self, from: u64, into: u64) -> Result<(), LibraryError> {
        if from == into || self.series(into)?.is_none() {
            return Ok(());
        }
        self.edit_series(from, |s| s.merged_into = Some(into))?;
        for (uid, v) in self.videos()? {
            if v.series == Some(from) || v.moved_to == Some(from) {
                self.edit_video(&uid, |v| {
                    if v.series == Some(from) {
                        v.series = Some(into);
                    }
                    if v.moved_to == Some(from) {
                        v.moved_to = Some(into);
                    }
                })?;
            }
        }
        Ok(())
    }

    /// Moves an episode to another series (LIB-011); a scan keeps it there.
    pub fn move_episode(&self, uid: &str, to: u64) -> Result<(), LibraryError> {
        if self.series(to)?.is_none() {
            return Ok(());
        }
        self.edit_video(uid, |v| {
            v.series = Some(to);
            v.moved_to = Some(to);
        })
    }

    /// Splits episodes off into a new series named `title` (LIB-011), with
    /// the grouping and folder of the first one's series; its id.
    pub fn split_series(
        &self,
        uids: &[String],
        title: &str,
        now_ms: i64,
    ) -> Result<Option<u64>, LibraryError> {
        let Some(first) = uids.first().map(|u| self.video(u)).transpose()?.flatten() else {
            return Ok(None);
        };
        let from = first
            .series
            .and_then(|id| self.series(id).ok().flatten())
            .unwrap_or_default();
        let id = self.all_series()?.keys().max().map_or(1, |m| m + 1);
        self.put_series(
            id,
            &SeriesRecord {
                title: title.trim().to_string(),
                key: manual_key(id),
                grouping: from.grouping,
                root: from.root,
                added_at: now_ms,
                prefs: from.prefs,
                ..SeriesRecord::default()
            },
        )?;
        for uid in uids {
            self.move_episode(uid, id)?;
        }
        Ok(Some(id))
    }

    /// Removes a video from the library (LIB-012): a scan does not bring it
    /// back; the file stays.
    pub fn remove_video(&self, uid: &str) -> Result<(), LibraryError> {
        self.edit_video(uid, |v| v.removed = true)
    }

    /// Sets what the episodes of a series inherit (LIB-010).
    pub fn set_series_prefs(&self, id: u64, prefs: SeriesPrefs) -> Result<(), LibraryError> {
        self.edit_series(id, |s| s.prefs = prefs)
    }

    /// Sets the cover of a series (LIB-005).
    pub fn set_cover(&self, id: u64, cover: &str) -> Result<(), LibraryError> {
        let cover = cover.to_string();
        self.edit_series(id, |s| s.cover = Some(cover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Grouping;
    use crate::scan::{Scanned, under};
    use std::path::Path;

    fn files(root: &Path, names: &[&str]) -> Vec<Scanned> {
        under(root, names)
            .into_iter()
            .map(|path| Scanned {
                path,
                size: 1,
                mtime: 1,
            })
            .collect()
    }

    fn setup() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let root = Path::new("/v");
        lib.import(
            root,
            Grouping::BySeries,
            &files(
                root,
                &[
                    "Alpha - 02.mkv",
                    "Alpha - 01.mkv",
                    "Alpha - 03.mkv",
                    "Beta - 01.mkv",
                    "Beta OVA.mkv",
                ],
            ),
            1,
        )
        .unwrap();
        (dir, lib)
    }

    fn id(lib: &Library, title: &str) -> u64 {
        lib.series_list()
            .unwrap()
            .into_iter()
            .find(|s| s.record.title == title)
            .unwrap()
            .id
    }

    fn titles(lib: &Library) -> Vec<String> {
        lib.series_list()
            .unwrap()
            .into_iter()
            .map(|s| s.record.title)
            .collect()
    }

    // T-LIB-005, T-LIB-008
    #[test]
    fn the_home_page_lists_series_and_what_to_continue() {
        let (_dir, lib) = setup();
        let alpha = id(&lib, "Alpha");
        let eps = lib.episodes(alpha).unwrap();
        let names: Vec<&str> = eps.iter().map(|(u, _)| u.as_str()).collect();
        assert_eq!(
            names,
            ["video/Alpha - 01", "video/Alpha - 02", "video/Alpha - 03"]
        );
        lib.save_position(&eps[0].0, 1_000, None, 10).unwrap();
        lib.save_position(&eps[1].0, 2_000, None, 20).unwrap();
        // finished: not to continue
        lib.save_position(&eps[2].0, 95_000, Some(100_000), 30)
            .unwrap();
        let next: Vec<String> = lib
            .continue_watching(10)
            .unwrap()
            .into_iter()
            .map(|(u, _)| u)
            .collect();
        assert_eq!(next, ["video/Alpha - 02", "video/Alpha - 01"]);
        assert_eq!(lib.continue_watching(1).unwrap().len(), 1);
        let s = lib
            .series_list()
            .unwrap()
            .into_iter()
            .find(|s| s.id == alpha)
            .unwrap();
        assert_eq!(
            (s.episodes, s.watched, s.started, s.last_played_at),
            (3, 1, 2, Some(30))
        );
    }

    // T-LIB-011
    #[test]
    fn series_are_renamed_merged_and_split() {
        let (_dir, lib) = setup();
        let alpha = id(&lib, "Alpha");
        let beta = id(&lib, "Beta");
        let ova = id(&lib, "Beta OVA");
        let first = lib.episodes(alpha).unwrap()[0].0.clone();
        lib.save_position(&first, 5_000, None, 1).unwrap();

        lib.merge_series(ova, beta).unwrap();
        assert_eq!(titles(&lib), ["Alpha", "Beta"]);
        assert_eq!(lib.episodes(beta).unwrap().len(), 2);

        let new = lib
            .split_series(std::slice::from_ref(&first), "Alpha pilot", 2)
            .unwrap()
            .unwrap();
        lib.rename_series(alpha, "Alpha!").unwrap();
        assert_eq!(titles(&lib), ["Alpha pilot", "Alpha!", "Beta"]);

        // a scan keeps all of it, and the progress
        let root = Path::new("/v");
        lib.import(
            root,
            Grouping::BySeries,
            &files(
                root,
                &[
                    "Alpha - 02.mkv",
                    "Alpha - 01.mkv",
                    "Alpha - 03.mkv",
                    "Beta - 01.mkv",
                    "Beta OVA.mkv",
                ],
            ),
            3,
        )
        .unwrap();
        assert_eq!(titles(&lib), ["Alpha pilot", "Alpha!", "Beta"]);
        assert_eq!(lib.episodes(new).unwrap()[0].0, first);
        assert_eq!(lib.episodes(beta).unwrap().len(), 2);
        assert_eq!(lib.video(&first).unwrap().unwrap().last_position_ms, 5_000);
    }

    // T-LIB-012
    #[test]
    fn a_removed_video_stays_out_and_its_file_stays() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("v");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("Show - 01.mkv"), b"x").unwrap();
        std::fs::write(root.join("Show - 02.mkv"), b"x").unwrap();
        let lib = Library::open(&dir.path().join("library.redb")).unwrap();
        let scan = || crate::scan::video_files(&root);
        lib.import(&root, Grouping::BySeries, &scan(), 1).unwrap();
        let show = lib.series_list().unwrap()[0].id;
        let (uid, _) = lib.episodes(show).unwrap().remove(0);
        lib.remove_video(&uid).unwrap();
        lib.import(&root, Grouping::BySeries, &scan(), 2).unwrap();
        assert_eq!(lib.episodes(show).unwrap().len(), 1);
        assert!(root.join("Show - 01.mkv").exists());
        // a deleted series: nothing on the page, the files stay
        lib.delete_series(show).unwrap();
        lib.import(&root, Grouping::BySeries, &scan(), 3).unwrap();
        assert!(lib.series_list().unwrap().is_empty());
        assert!(root.join("Show - 02.mkv").exists());
    }
}
