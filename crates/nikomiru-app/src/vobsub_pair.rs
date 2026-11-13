// SPDX-License-Identifier: GPL-3.0-only

//! Local companion discovery, bounded reads and two-source cache identity
//! for external VobSub (SUB-031). Index text never names another file.

use nikomiru_core::Cancel;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

fn companion(path: &Path, extension: &str) -> io::Result<PathBuf> {
    let exact = path.with_extension(extension);
    if exact.is_file() {
        return Ok(exact);
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut candidates: Vec<_> = std::fs::read_dir(path.parent().unwrap_or(Path::new(".")))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
                && p.extension()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(extension))
                && p.is_file()
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("missing .{extension} companion"),
        )
    })
}

/// Find the index and program stream beside either selected member.
pub fn sources(path: &Path) -> io::Result<(PathBuf, PathBuf)> {
    Ok((companion(path, "idx")?, companion(path, "sub")?))
}

/// Read a regular local source within an actual-byte bound and cancellation.
pub fn read(path: &Path, limit: u64, cancel: &Cancel) -> io::Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VobSub source exceeds its read limit",
        ));
    }
    let mut input = std::fs::File::open(path)?.take(limit + 1);
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 << 10];
    loop {
        if cancel.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VobSub source exceeds its read limit",
        ));
    }
    Ok(bytes)
}

/// Cache identity includes both files' path/size/time, the stream and model.
pub fn cache_path(cache: &Path, index: &Path, sub: &Path, track: u8) -> io::Result<PathBuf> {
    let idx_key = nikomiru_media::extract::video_key(index)?;
    let sub_key = nikomiru_media::extract::video_key(sub)?;
    let model = &nikomiru_ocr::MODEL[0].sha256[..8];
    Ok(cache.join(format!("vob-{idx_key}-{sub_key}-{track}.read-{model}.srt")))
}

#[cfg(test)]
mod tests {
    use super::*;
    // T-SUB-031: either member and mixed-case extensions find the same pair;
    // changing either source invalidates a cached reading.
    #[test]
    fn companions_and_cache_keys_follow_both_local_sources() {
        let dir = tempfile::tempdir().unwrap();
        let idx = dir.path().join("Show.IDX");
        let sub = dir.path().join("Show.SUB");
        std::fs::write(&idx, b"index").unwrap();
        std::fs::write(&sub, b"stream").unwrap();
        assert_eq!(sources(&idx).unwrap(), (idx.clone(), sub.clone()));
        assert_eq!(sources(&sub).unwrap(), (idx.clone(), sub.clone()));
        let key = cache_path(dir.path(), &idx, &sub, 0).unwrap();
        std::fs::write(&sub, b"changed stream").unwrap();
        let next = cache_path(dir.path(), &idx, &sub, 0).unwrap();
        assert_ne!(key, next);
        std::fs::write(&idx, b"changed index").unwrap();
        assert_ne!(next, cache_path(dir.path(), &idx, &sub, 0).unwrap());
        assert_ne!(
            cache_path(dir.path(), &idx, &sub, 0).unwrap(),
            cache_path(dir.path(), &idx, &sub, 1).unwrap()
        );
        std::fs::remove_file(&sub).unwrap();
        assert!(sources(&idx).is_err());
    }
    // T-SUB-031: actual read bounds, regular files and cancellation.
    #[test]
    fn bounded_pair_reads_reject_large_missing_and_cancelled_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input");
        std::fs::write(&path, b"hello").unwrap();
        assert!(read(&path, 4, &Cancel::new()).is_err());
        assert_eq!(read(&path, 5, &Cancel::new()).unwrap(), b"hello");
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(
            read(&path, 5, &cancel).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(read(dir.path(), 50, &Cancel::new()).is_err());
    }
}
