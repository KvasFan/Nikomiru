// SPDX-License-Identifier: GPL-3.0-only

//! The subtitle files inside a ZIP archive (MET-003, ADR 0008 §7), opened in
//! memory with the rules of DIC-011: names only, never paths; limits on the
//! entries and on the unpacked size.

use std::io::{Cursor, Read};

use crate::SubtitleFile;
use crate::error::OnlineError;
use crate::language::is_subtitle;

/// The most entries an archive may have.
pub const MAX_ENTRIES: usize = 1000;

/// The most bytes its subtitle files may unpack to.
pub const MAX_UNPACKED: u64 = 64 << 20;

/// The subtitle files of the archive `bytes`, by their names without folders.
pub fn subtitle_files(bytes: &[u8]) -> Result<Vec<SubtitleFile>, OnlineError> {
    let bad = |e: &dyn std::fmt::Display| OnlineError::Archive(e.to_string());
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(&e))?;
    if zip.len() > MAX_ENTRIES {
        return Err(OnlineError::Archive(format!(
            "more than {MAX_ENTRIES} entries"
        )));
    }
    let mut out = Vec::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| bad(&e))?;
        if entry.is_dir() || entry.is_symlink() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|e| bad(&e))?
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_string();
        if name.is_empty() || name.starts_with('.') || !is_subtitle(&name) {
            continue;
        }
        let room = MAX_UNPACKED - total;
        let mut data = Vec::new();
        (&mut entry)
            .take(room + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad(&e))?;
        total += data.len() as u64;
        if total > MAX_UNPACKED {
            return Err(OnlineError::TooLarge {
                limit_mib: MAX_UNPACKED >> 20,
            });
        }
        out.push(SubtitleFile { name, bytes: data });
    }
    if out.is_empty() {
        return Err(OnlineError::Archive("no subtitle file inside".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            w.start_file(*name, options).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    // T-MET-003
    #[test]
    fn only_the_subtitle_files_by_their_names() {
        let bytes = zip_of(&[
            ("Show/Show - 01.ja.srt", b"1"),
            ("../../etc/Show - 02.ass", b"2"),
            ("Show/readme.txt", b"x"),
            ("__MACOSX/._Show - 01.ja.srt", b"junk"),
        ]);
        let files = subtitle_files(&bytes).unwrap();
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Show - 01.ja.srt", "Show - 02.ass"]);
        assert_eq!(files[1].bytes, b"2");
        assert!(subtitle_files(&zip_of(&[("a.txt", b"x")])).is_err());
        assert!(subtitle_files(b"not a zip").is_err());
    }
}
