// SPDX-License-Identifier: GPL-3.0-only

//! A folder imported, or scanned again (LIB-002, LIB-004): its videos
//! grouped (`grouping`) and written to the library in one transaction.
//!
//! A scan keeps what the library knows: a known file keeps its uid,
//! progress and choices, also when it moved (same name, size and time); a
//! series is found again by its grouping key, so a renamed one keeps its
//! name, a merged one sends its episodes where it was merged, a deleted one
//! is not made again (LIB-004); an episode moved by hand stays where it was
//! put, and one removed stays out (LIB-011, LIB-012). The series of the other
//! grouping stay, so changing the grouping back loses nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use redb::ReadableTable;

use crate::db::{
    FOLDERS, FolderRecord, Grouping, Library, LibraryError, SERIES, SeriesRecord, VIDEOS,
    VideoRecord, db_err,
};
use crate::grouping::{by_folder, by_series};
use crate::scan::{Scanned, stored};
use crate::uid;

/// What an import did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// Videos new to the library.
    pub added: usize,
    /// Videos the library knew.
    pub kept: usize,
    /// Videos left out: removed by the user, or of a deleted series.
    pub skipped: usize,
    /// Series made.
    pub series_added: usize,
}

/// How many merges are followed before giving up (a loop made by hand).
const MERGES: usize = 16;

/// The series `id` ends in after the merges, if it still exists.
fn merged(series: &BTreeMap<u64, SeriesRecord>, mut id: u64) -> u64 {
    for _ in 0..MERGES {
        match series.get(&id).and_then(|s| s.merged_into) {
            Some(next) if series.get(&next).is_some_and(|s| !s.deleted) => id = next,
            _ => break,
        }
    }
    id
}

impl Library {
    /// Imports `root`, or scans it again, with `files` its video files
    /// ([`crate::scan::video_files`]), grouped as `grouping`.
    pub fn import(
        &self,
        root: &Path,
        grouping: Grouping,
        files: &[Scanned],
        now_ms: i64,
    ) -> Result<ImportReport, LibraryError> {
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        let groups = match grouping {
            Grouping::BySeries => by_series(&paths),
            Grouping::ByFolder => by_folder(&paths),
        };
        let by_path: BTreeMap<&Path, &Scanned> =
            files.iter().map(|f| (f.path.as_path(), f)).collect();
        let root = stored(root);
        let mut report = ImportReport::default();
        let tx = self.write()?;
        {
            let mut videos_t = db_err!(tx.open_table(VIDEOS))?;
            let mut series_t = db_err!(tx.open_table(SERIES))?;
            let mut folders_t = db_err!(tx.open_table(FOLDERS))?;
            let mut videos: Vec<(String, VideoRecord)> = Vec::new();
            for row in db_err!(videos_t.iter())? {
                let (k, v) = db_err!(row)?;
                videos.push((k.value().to_string(), postcard::from_bytes(v.value())?));
            }
            let mut series: BTreeMap<u64, SeriesRecord> = BTreeMap::new();
            for row in db_err!(series_t.iter())? {
                let (k, v) = db_err!(row)?;
                series.insert(k.value(), postcard::from_bytes(v.value())?);
            }
            let mut changed: Vec<usize> = Vec::new();
            for group in groups {
                let found = series
                    .iter()
                    .find(|(_, s)| s.grouping == grouping && s.root == root && s.key == group.key)
                    .map(|(id, s)| (*id, s.deleted));
                let id = match found {
                    Some((deleted, true)) => {
                        // its known videos stay with it, out of sight
                        for e in &group.episodes {
                            let path = stored(&e.path);
                            if let Some(i) = videos.iter().position(|(_, v)| v.path == path)
                                && videos[i].1.series != Some(deleted)
                            {
                                videos[i].1.series = Some(deleted);
                                changed.push(i);
                            }
                        }
                        report.skipped += group.episodes.len();
                        continue;
                    }
                    Some((id, false)) => merged(&series, id),
                    None => {
                        let id = series.keys().max().map_or(1, |m| m + 1);
                        series.insert(
                            id,
                            SeriesRecord {
                                title: group.series.clone(),
                                key: group.key.clone(),
                                grouping,
                                root: root.clone(),
                                added_at: now_ms,
                                ..SeriesRecord::default()
                            },
                        );
                        db_err!(
                            series_t.insert(id, postcard::to_stdvec(&series[&id])?.as_slice())
                        )?;
                        report.series_added += 1;
                        id
                    }
                };
                for e in group.episodes {
                    let Some(file) = by_path.get(e.path.as_path()) else {
                        continue;
                    };
                    let path = stored(&e.path);
                    let name = e.path.file_name();
                    let known = videos.iter().position(|(_, v)| v.path == path).or_else(|| {
                        // moved: same name, size and time, and gone from where it was
                        videos.iter().position(|(_, v)| {
                            let old = Path::new(&v.path);
                            old.file_name() == name
                                && v.size == file.size
                                && v.mtime == file.mtime
                                && !old.exists()
                        })
                    });
                    let i = match known {
                        Some(i) if videos[i].1.removed => {
                            report.skipped += 1;
                            continue;
                        }
                        Some(i) => {
                            report.kept += 1;
                            i
                        }
                        None => {
                            let uid = uid::unique_uid(&e.path, |u| {
                                videos
                                    .iter()
                                    .find(|(k, _)| k == u)
                                    .map(|(_, v)| v.path.clone())
                            });
                            report.added += 1;
                            videos.push((uid, VideoRecord::default()));
                            videos.len() - 1
                        }
                    };
                    let record = &mut videos[i].1;
                    record.path = path;
                    record.size = file.size;
                    record.mtime = file.mtime;
                    record.season = e.season;
                    record.episode = e.episode;
                    record.series = Some(match record.moved_to {
                        Some(m)
                            if series
                                .get(&m)
                                .is_some_and(|s| !s.deleted && s.grouping == grouping) =>
                        {
                            m
                        }
                        _ => id,
                    });
                    changed.push(i);
                }
            }
            for i in changed {
                let (uid, record) = &videos[i];
                db_err!(videos_t.insert(uid.as_str(), postcard::to_stdvec(record)?.as_slice()))?;
            }
            let folder = match db_err!(folders_t.get(root.as_str()))? {
                Some(v) => postcard::from_bytes(v.value())?,
                None => FolderRecord {
                    added_at: now_ms,
                    ..FolderRecord::default()
                },
            };
            let folder = FolderRecord {
                grouping,
                scanned_at: now_ms,
                ..folder
            };
            db_err!(folders_t.insert(root.as_str(), postcard::to_stdvec(&folder)?.as_slice()))?;
        }
        db_err!(tx.commit())?;
        Ok(report)
    }

    /// The imported folders, by path.
    pub fn folders(&self) -> Result<Vec<(String, FolderRecord)>, LibraryError> {
        self.read(|tx| {
            let t = db_err!(tx.open_table(FOLDERS))?;
            let mut out = Vec::new();
            for row in db_err!(t.iter())? {
                let (k, v) = db_err!(row)?;
                out.push((k.value().to_string(), postcard::from_bytes(v.value())?));
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{under, video_files};

    fn tree(root: &Path, files: &[&str]) {
        for f in under(root, files) {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, f.to_string_lossy().as_bytes()).unwrap();
        }
    }

    fn library(dir: &Path) -> Library {
        Library::open(&dir.join("library.redb")).unwrap()
    }

    /// The series titles with their episodes' file names, in order.
    fn shelf(lib: &Library) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = lib
            .series_list()
            .unwrap()
            .into_iter()
            .map(|s| {
                let names = lib
                    .episodes(s.id)
                    .unwrap()
                    .into_iter()
                    .map(|(_, v)| {
                        Path::new(&v.path)
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                (s.record.title, names)
            })
            .collect();
        out.sort();
        out
    }

    // T-LIB-002
    #[test]
    fn a_folder_is_imported_by_series_or_by_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("videos");
        tree(
            &root,
            &[
                "Show/Season 1/Show - 01.mkv",
                "Show/Season 1/Show - 02.mkv",
                "Show/Season 2/Show - 01.mkv",
                "Other/Other - 01.mp4",
                "Other/cover.jpg",
            ],
        );
        let lib = library(dir.path());
        let files = video_files(&root);
        let report = lib.import(&root, Grouping::BySeries, &files, 1).unwrap();
        assert_eq!(
            report,
            ImportReport {
                added: 4,
                kept: 0,
                skipped: 0,
                series_added: 2
            }
        );
        assert_eq!(
            shelf(&lib),
            [
                ("Other".to_string(), vec!["Other - 01.mp4".to_string()]),
                (
                    "Show".to_string(),
                    vec![
                        "Show - 01.mkv".into(),
                        "Show - 02.mkv".into(),
                        "Show - 01.mkv".into()
                    ]
                ),
            ]
        );
        let folders = lib.folders().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].1.grouping, Grouping::BySeries);

        // by folder: a collection per folder
        let report = lib.import(&root, Grouping::ByFolder, &files, 2).unwrap();
        assert_eq!((report.added, report.kept, report.series_added), (0, 4, 3));
        let titles: Vec<String> = shelf(&lib).into_iter().map(|(t, _)| t).collect();
        assert_eq!(titles, ["Other", "Season 1", "Season 2"]);
    }

    // T-LIB-004
    #[test]
    fn a_scan_again_loses_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("videos");
        tree(&root, &["Show - 01.mkv", "Show - 02.mkv", "Movie.mkv"]);
        let lib = library(dir.path());
        lib.import(&root, Grouping::BySeries, &video_files(&root), 1)
            .unwrap();
        let show = lib
            .series_list()
            .unwrap()
            .into_iter()
            .find(|s| s.record.title == "Show")
            .unwrap();
        let (uid, _) = lib.episodes(show.id).unwrap().remove(0);
        lib.save_position(&uid, 30_000, None, 5).unwrap();
        let mut rec = lib.video(&uid).unwrap().unwrap();
        rec.delay_ms = Some(-400);
        lib.put_video(&uid, &rec).unwrap();
        lib.rename_series(show.id, "My show").unwrap();
        let movie = lib
            .series_list()
            .unwrap()
            .into_iter()
            .find(|s| s.record.title == "Movie")
            .unwrap();
        lib.delete_series(movie.id).unwrap();

        // the same folder, then by folder and back
        for grouping in [Grouping::BySeries, Grouping::ByFolder, Grouping::BySeries] {
            lib.import(&root, grouping, &video_files(&root), 9).unwrap();
        }
        let rec = lib.video(&uid).unwrap().unwrap();
        assert_eq!((rec.last_position_ms, rec.delay_ms), (30_000, Some(-400)));
        let titles: Vec<String> = shelf(&lib).into_iter().map(|(t, _)| t).collect();
        assert_eq!(titles, ["My show"], "the deleted film is not made again");

        // moved on disk: found again with its progress
        std::fs::create_dir(root.join("moved")).unwrap();
        std::fs::rename(root.join("Show - 01.mkv"), root.join("moved/Show - 01.mkv")).unwrap();
        let report = lib
            .import(&root, Grouping::BySeries, &video_files(&root), 10)
            .unwrap();
        assert_eq!(report.added, 0);
        let rec = lib.video(&uid).unwrap().unwrap();
        assert!(rec.path.ends_with("moved/Show - 01.mkv"), "{}", rec.path);
        assert_eq!(rec.last_position_ms, 30_000);
    }
}
