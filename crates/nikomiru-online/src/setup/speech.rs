// SPDX-License-Identifier: GPL-3.0-only

//! The pinned whisper.cpp models (INS-008, SUB-062): fetched from where
//! their makers publish them, at a fixed revision, checked by size and
//! SHA-256. Both are MIT.

use std::path::{Path, PathBuf};

/// One model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    /// Its name on disk.
    pub name: &'static str,
    /// Where it is fetched.
    pub url: &'static str,
    /// Its SHA-256, in hex.
    pub sha256: &'static str,
    /// Its size.
    pub bytes: u64,
}

/// Whisper large-v3-turbo, quantised to 5 bits: good Japanese at a size and
/// speed an ordinary computer manages; then Silero's voice-activity model.
pub const MODEL: [Pinned; 2] = [
    Pinned {
        name: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin",
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        bytes: 574_041_195,
    },
    Pinned {
        name: "ggml-silero-v6.2.0.bin",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin",
        sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
        bytes: 885_098,
    },
];

/// The license of both.
pub const LICENSE: &str = "MIT";

/// All of them, in bytes.
pub fn bytes() -> u64 {
    MODEL.iter().map(|m| m.bytes).sum()
}

/// Whether every file is in `dir` at its size.
pub fn installed(dir: &Path) -> bool {
    MODEL.iter().all(|m| {
        std::fs::metadata(dir.join(m.name))
            .is_ok_and(|meta| meta.is_file() && meta.len() == m.bytes)
    })
}

/// The speech model in `dir`, when installed.
pub fn model(dir: &Path) -> Option<PathBuf> {
    installed(dir).then(|| dir.join(MODEL[0].name))
}

/// The voice-activity model in `dir`, when installed.
pub fn vad(dir: &Path) -> Option<PathBuf> {
    installed(dir).then(|| dir.join(MODEL[1].name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-INS-008: pinned revisions, hashes and sizes; a partial folder is not installed.
    #[test]
    fn the_models_are_pinned() {
        for m in MODEL {
            assert!(m.url.starts_with("https://huggingface.co/"));
            assert!(m.url.contains("/resolve/") && !m.url.contains("/main/"));
            assert!(m.url.ends_with(m.name));
            assert_eq!(m.sha256.len(), 64);
        }
        assert_eq!(bytes(), 574_041_195 + 885_098);
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        std::fs::write(dir.path().join(MODEL[1].name), b"short").unwrap();
        assert!(model(dir.path()).is_none());
        assert!(vad(dir.path()).is_none());
    }
}
