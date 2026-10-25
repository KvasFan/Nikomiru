// SPDX-License-Identifier: GPL-3.0-only

//! The OpenSubtitles hash of a video (MET-004, MET-008): its size plus the sum
//! of the 64-bit little-endian words of its first and last 64 KiB. It names
//! the folder of the subtitles downloaded for the video, and survives renaming
//! and moving the file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The bytes read from each end.
const CHUNK: u64 = 64 * 1024;

/// A video's hash and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VideoHash {
    /// The hash.
    pub hash: u64,
    /// The size in bytes.
    pub size: u64,
}

impl VideoHash {
    /// The hash as OpenSubtitles writes it: 16 lower-case hex digits.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

fn sum_words(bytes: &[u8]) -> u64 {
    bytes.chunks(8).fold(0u64, |sum, chunk| {
        let mut word = [0u8; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u64::from_le_bytes(word))
    })
}

/// The hash of the file at `path`.
pub fn video_hash(path: &Path) -> std::io::Result<VideoHash> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut head = vec![0u8; usize::try_from(CHUNK.min(size)).unwrap_or(0)];
    file.read_exact(&mut head)?;
    let tail_len = CHUNK.min(size);
    file.seek(SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(0)];
    file.read_exact(&mut tail)?;
    let hash = size
        .wrapping_add(sum_words(&head))
        .wrapping_add(sum_words(&tail));
    Ok(VideoHash { hash, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-MET-004
    #[test]
    fn the_hash_adds_the_size_and_both_ends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.mkv");
        // zeros: only the size counts
        std::fs::write(&path, vec![0u8; 300_000]).unwrap();
        let h = video_hash(&path).unwrap();
        assert_eq!((h.hash, h.size), (300_000, 300_000));
        assert_eq!(h.hex(), "00000000000493e0");
        // a word at the start, one at the end
        let mut bytes = vec![0u8; 300_000];
        bytes[0] = 1;
        bytes[300_000 - 8] = 2;
        // in the middle: outside both ends, not counted
        bytes[150_000] = 9;
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(video_hash(&path).unwrap().hash, 300_003);
        // a small file is read once from each end
        std::fs::write(&path, [1u8, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(video_hash(&path).unwrap().hash, 8 + 1 + 1);
    }
}
