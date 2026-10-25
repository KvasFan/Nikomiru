// SPDX-License-Identifier: GPL-3.0-only

//! Names of the media sent to Anki (ANK-036): by content, so the same bytes
//! get the same name and different bytes never share one.

use sha2::{Digest, Sha256};

/// `nikomiru_<the first 16 hex digits of the SHA-256 of bytes>.<ext>`.
pub fn name(bytes: &[u8], ext: &str) -> String {
    let digest = Sha256::digest(bytes);
    let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("nikomiru_{hex}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-ANK-036
    #[test]
    fn names_follow_the_content() {
        // SHA-256("abc") = ba7816bf8f01cfea…
        assert_eq!(name(b"abc", "m4a"), "nikomiru_ba7816bf8f01cfea.m4a");
        assert_eq!(name(b"abc", "jpg"), "nikomiru_ba7816bf8f01cfea.jpg");
        assert_ne!(name(b"abd", "m4a"), name(b"abc", "m4a"));
    }
}
