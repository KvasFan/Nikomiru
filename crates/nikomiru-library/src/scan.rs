// SPDX-License-Identifier: GPL-3.0-only

//! The video files of a folder and its subfolders (LIB-002): the only part
//! of the import that reads the disk.

use std::path::{Path, PathBuf};

use nikomiru_text::filename::is_video;

/// A video file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scanned {
    /// Its path.
    pub path: PathBuf,
    /// Its size in bytes.
    pub size: u64,
    /// Its modification time (Unix seconds).
    pub mtime: i64,
}

/// The path a file is known by in the library: its folder with the links
/// resolved, and its own name as it is, so a link with an episode's name
/// stays that episode; `path` itself when its folder cannot be resolved.
pub fn identity(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => {
            let dir = if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            };
            std::fs::canonicalize(dir).map_or_else(|_| path.to_path_buf(), |d| d.join(name))
        }
        _ => path.to_path_buf(),
    }
}

/// The video files under `root`, sorted by path. A link to a video counts; a
/// link to a folder is not followed (a loop would never end). Hidden entries
/// are skipped, and a folder that cannot be read is left out.
pub fn video_files(root: &Path) -> Vec<Scanned> {
    let mut out = Vec::new();
    // the folders below are reached without links: theirs are resolved here
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut folders = vec![root];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            tracing::debug!("{} cannot be read", folder.display());
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                folders.push(path);
            } else if is_video(&path)
                && let Ok(meta) = std::fs::metadata(&path)
                && meta.is_file()
            {
                out.push(Scanned {
                    size: meta.len(),
                    mtime: mtime(&meta),
                    path,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The modification time of `meta`, in Unix seconds.
pub fn mtime(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

/// `path` as the library stores it.
pub(crate) fn stored(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The files of `paths`, for the tests: the paths of `files` under `root`.
#[cfg(test)]
pub(crate) fn under(root: &Path, files: &[&str]) -> Vec<PathBuf> {
    files.iter().map(|f| root.join(f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LIB-002
    #[test]
    fn video_files_are_found_in_every_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        for f in under(
            dir.path(),
            &[
                "Show/Season 1/Show - 01.mkv",
                "Show/Season 1/Show - 01.ja.srt",
                "Show/Season 2/Show - 01.MP4",
                "Movie.avi",
                "notes.txt",
                ".hidden/x.mkv",
            ],
        ) {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, b"x").unwrap();
        }
        // a link to a parent folder does not loop
        std::os::unix::fs::symlink(dir.path(), dir.path().join("Show/loop")).unwrap();
        let found: Vec<String> = video_files(dir.path())
            .iter()
            .map(|s| {
                s.path
                    .strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            found,
            [
                "Movie.avi",
                "Show/Season 1/Show - 01.mkv",
                "Show/Season 2/Show - 01.MP4"
            ]
        );
        assert!(video_files(&dir.path().join("missing")).is_empty());
    }

    // T-LIB-002: a link keeps its name, a linked folder is resolved
    #[test]
    fn a_file_is_known_by_its_folder_resolved_and_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("video.mkv"), b"x").unwrap();
        std::os::unix::fs::symlink(real.join("video.mkv"), real.join("Show - 01.mkv")).unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("alias")).unwrap();
        let canonical = std::fs::canonicalize(&real).unwrap();
        assert_eq!(
            identity(&dir.path().join("alias/Show - 01.mkv")),
            canonical.join("Show - 01.mkv")
        );
        let found: Vec<PathBuf> = video_files(&dir.path().join("alias"))
            .into_iter()
            .map(|s| s.path)
            .collect();
        assert_eq!(
            found,
            [canonical.join("Show - 01.mkv"), canonical.join("video.mkv")]
        );
    }
}
