// SPDX-License-Identifier: GPL-3.0-only

//! Where downloaded subtitles are kept (MET-008, ADR 0008 §10):
//! `$XDG_DATA_HOME/nikomiru/subtitles/<hash>/<video name>.<language>.<ext>`,
//! never next to the video.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::VideoHash;
use crate::language::{extension, is_subtitle};

/// The folder of the subtitles downloaded for the video of `hash`, under the
/// data folder `data_dir`.
pub fn folder(data_dir: &Path, hash: &VideoHash) -> PathBuf {
    data_dir.join("subtitles").join(hash.hex())
}

/// A file name part without the characters file systems or shells mind.
fn clean(part: &str) -> String {
    let s: String = part
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() { "subtitles".into() } else { s }
}

/// Saves `bytes` in `folder` as `<video stem>.<language>.<ext of file_name>`,
/// or with ` (2)`, ` (3)`… when the name is taken; the path written.
pub fn save(
    folder: &Path,
    video: &Path,
    language: &str,
    file_name: &str,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    let ext = extension(file_name)
        .filter(|_| is_subtitle(file_name))
        .unwrap_or_else(|| "srt".into());
    let stem = clean(
        &video
            .file_stem()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default(),
    );
    let language = clean(language);
    std::fs::create_dir_all(folder)?;
    let mut n = 1u32;
    loop {
        let name = if n == 1 {
            format!("{stem}.{language}.{ext}")
        } else {
            format!("{stem} ({n}).{language}.{ext}")
        };
        let path = folder.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let written = file.write_all(bytes).and_then(|()| file.sync_all());
                if let Err(e) = written {
                    let _ = std::fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => n += 1,
            Err(e) => return Err(e),
        }
    }
}

/// The subtitle files in `folder`, sorted; none if it does not exist.
pub fn list(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| is_subtitle(&n.to_string_lossy()))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-008
    #[test]
    fn saved_by_video_name_and_language_never_over_another() {
        let dir = tempfile::tempdir().unwrap();
        let hash = VideoHash {
            hash: 0xabc,
            size: 1,
        };
        let f = folder(dir.path(), &hash);
        assert_eq!(f, dir.path().join("subtitles/0000000000000abc"));
        let video = Path::new("/v/Show - 01.mkv");
        let a = save(&f, video, "ja", "x.ass", b"a").unwrap();
        let b = save(&f, video, "ja", "y.ASS", b"b").unwrap();
        assert_eq!(a.file_name().unwrap(), "Show - 01.ja.ass");
        assert_eq!(b.file_name().unwrap(), "Show - 01 (2).ja.ass");
        let c = save(&f, Path::new("/v/../.hidden:x.mkv"), "j/a", "z.zip", b"c").unwrap();
        assert_eq!(c.file_name().unwrap(), "hidden_x.j_a.srt");
        std::fs::write(f.join("notes.txt"), b"x").unwrap();
        // sorted by name: ` (2)` before `.`
        assert_eq!(list(&f), [b, a, c]);
        assert!(list(&dir.path().join("none")).is_empty());
    }
}
