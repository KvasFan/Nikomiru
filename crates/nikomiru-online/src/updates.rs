// SPDX-License-Identifier: GPL-3.0-only

//! Signed application updates (UPD-001, UPD-002, UPD-004): a channel's
//! manifest, verified with the build's minisign key before it is read, and
//! its artifact, downloaded with resumption and checked by size and hash.
//! Nothing here runs on its own.

use std::path::Path;

use minisign_verify::{PublicKey, Signature};
use nikomiru_core::Cancel;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::OnlineError;
use crate::http::Http;

/// Where the channels' manifests are published.
pub const FEED: &str = "https://github.com/KvasFan/Nikomiru/releases/download/updates";
/// The production public key: none until the maintainer creates one and
/// publishes a signed feed (UPD-001).
pub const PRODUCTION_KEY: Option<&str> = None;
/// The largest manifest or signature.
pub const MAX_MANIFEST: u64 = 64 << 10;
/// The largest artifact.
pub const MAX_ARTIFACT: u64 = 1 << 30;
/// The channels.
pub const CHANNELS: [&str; 2] = ["stable", "beta"];

/// An artifact of a release for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// `linux-x86_64`, `windows-x86_64`…
    pub platform: String,
    /// `tar.xz` or `zip`.
    pub kind: String,
    /// Its HTTPS address.
    pub url: String,
    /// Its SHA-256.
    pub sha256: String,
    /// Its size.
    pub bytes: u64,
}

/// A channel's signed manifest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// `nikomiru-update`.
    pub format: String,
    /// 1.
    pub version: u32,
    /// Its channel.
    pub channel: String,
    /// The release, `major.minor.patch`.
    pub release: String,
    /// When it was published, as written.
    pub published: String,
    /// Its notes page.
    pub notes_url: String,
    /// Its artifacts.
    pub artifacts: Vec<Artifact>,
}

fn bad(reason: &str) -> OnlineError {
    OnlineError::Unusable(format!("update: {reason}"))
}

/// `major.minor.patch` as numbers.
pub fn version_of(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().split('.');
    let v = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(v)
}

fn https(url: &str) -> bool {
    url.starts_with("https://")
        || (cfg!(any(test, feature = "test-support")) && crate::http::checked_url(url).is_ok())
}

/// Verifies `signature` (minisign) of `bytes` with `key`, then reads the
/// manifest of `channel` (UPD-001). Nothing is parsed before the signature.
pub fn verify(
    bytes: &[u8],
    signature: &str,
    key: &str,
    channel: &str,
) -> Result<Manifest, OnlineError> {
    if bytes.len() as u64 > MAX_MANIFEST || signature.len() as u64 > MAX_MANIFEST {
        return Err(bad("too large"));
    }
    let key = PublicKey::decode(key.trim())
        .or_else(|_| PublicKey::from_base64(key.trim()))
        .map_err(|_| bad("no valid key"))?;
    let signature = Signature::decode(signature.trim()).map_err(|_| bad("bad signature"))?;
    key.verify(bytes, &signature, true)
        .map_err(|_| bad("the signature does not match"))?;
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|_| bad("malformed"))?;
    let valid = manifest.format == "nikomiru-update"
        && manifest.version == 1
        && manifest.channel == channel
        && version_of(&manifest.release).is_some()
        && https(&manifest.notes_url)
        && manifest.artifacts.len() <= 16
        && manifest.artifacts.iter().all(|a| {
            matches!(a.kind.as_str(), "tar.xz" | "zip")
                && https(&a.url)
                && a.bytes > 0
                && a.bytes <= MAX_ARTIFACT
                && a.sha256.len() == 64
                && a.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && !a.platform.is_empty()
        });
    if !valid {
        return Err(bad("not a manifest of this channel"));
    }
    Ok(manifest)
}

/// Whether `release` is newer than `current` and not older than `floor`, the
/// newest release already accepted (UPD-002).
pub fn offered(release: &str, current: &str, floor: Option<&str>) -> bool {
    let (Some(r), Some(c)) = (version_of(release), version_of(current)) else {
        return false;
    };
    r > c && floor.and_then(version_of).is_none_or(|f| r >= f)
}

/// The artifact for `platform`.
pub fn artifact<'a>(manifest: &'a Manifest, platform: &str) -> Option<&'a Artifact> {
    manifest.artifacts.iter().find(|a| a.platform == platform)
}

fn small(http: &Http, url: &str) -> Result<Vec<u8>, OnlineError> {
    let url = crate::http::checked_url(url)?;
    let mut out = Vec::new();
    http.stream_from(
        &url,
        "update",
        0,
        MAX_MANIFEST,
        &mut |_, _| Ok(()),
        &mut |piece| {
            out.extend_from_slice(piece);
            Ok(())
        },
    )?;
    Ok(out)
}

/// Fetches and verifies the manifest of `channel` from `feed` with `key`.
pub fn check(http: &Http, feed: &str, channel: &str, key: &str) -> Result<Manifest, OnlineError> {
    if !CHANNELS.contains(&channel) {
        return Err(bad("unknown channel"));
    }
    let base = feed.trim_end_matches('/');
    let bytes = small(http, &format!("{base}/{channel}.json"))?;
    let signature = small(http, &format!("{base}/{channel}.json.minisig"))?;
    let signature = String::from_utf8(signature).map_err(|_| bad("bad signature"))?;
    verify(&bytes, &signature, key, channel)
}

/// Downloads `artifact` to `dest`, resuming a partial file, and checks its
/// size and SHA-256 (UPD-004); a mismatch removes it.
pub fn download(
    http: &Http,
    artifact: &Artifact,
    dest: &Path,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), OnlineError> {
    let url = crate::http::checked_url(&artifact.url)?;
    crate::setup::fetch::resumable(http, &url, dest, artifact.bytes, cancel, progress)?;
    let mut file = std::fs::File::open(dest).map_err(|e| OnlineError::Io(e.to_string()))?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|e| OnlineError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buffer[..n]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if size != artifact.bytes || !digest.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = std::fs::remove_file(dest);
        return Err(bad("the download is not the release"));
    }
    Ok(())
}

/// Signing for the tests: a fixed test key, never a release key.
#[cfg(any(test, feature = "test-support"))]
pub mod test_key {
    use ed25519_compact::{KeyPair, Seed};

    const KEYNUM: [u8; 8] = *b"NIKOTEST";

    fn base64(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[(n >> (18 - 6 * i)) as usize & 63] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn pair(seed: u8) -> KeyPair {
        KeyPair::from_seed(Seed::new([seed; 32]))
    }

    /// The public key of test key `seed`, in minisign's text form.
    pub fn public(seed: u8) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend_from_slice(&KEYNUM);
        raw.extend_from_slice(pair(seed).pk.as_ref());
        format!(
            "untrusted comment: minisign public key (test)\n{}\n",
            base64(&raw)
        )
    }

    /// A minisign signature of `data` by test key `seed`.
    pub fn sign(seed: u8, data: &[u8]) -> String {
        let kp = pair(seed);
        let signature = kp.sk.sign(data, None);
        let mut raw = b"Ed".to_vec();
        raw.extend_from_slice(&KEYNUM);
        raw.extend_from_slice(signature.as_ref());
        let trusted = "timestamp:0\tfile:test";
        let mut global = signature.as_ref().to_vec();
        global.extend_from_slice(trusted.as_bytes());
        let global = kp.sk.sign(&global, None);
        format!(
            "untrusted comment: signature from a test key\n{}\ntrusted comment: {trusted}\n{}\n",
            base64(&raw),
            base64(global.as_ref())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_files::FakeFiles;

    fn manifest(channel: &str, release: &str, url: &str, sha: &str, bytes: u64) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "format": "nikomiru-update", "version": 1, "channel": channel, "release": release,
            "published": "2026-11-16", "notes_url": "https://example.org/notes",
            "artifacts": [{"platform": "linux-x86_64", "kind": "zip", "url": url, "sha256": sha, "bytes": bytes}],
        }))
        .unwrap()
    }

    // T-UPD-001: the signature is checked before anything is read.
    #[test]
    fn manifests_are_verified() {
        let key = test_key::public(1);
        let m = manifest(
            "stable",
            "9.0.0",
            "https://example.org/a.zip",
            &"a".repeat(64),
            10,
        );
        let sig = test_key::sign(1, &m);
        let ok = verify(&m, &sig, &key, "stable").unwrap();
        assert_eq!(ok.release, "9.0.0");
        // tampered, other key, other channel, malformed signature
        let mut tampered = m.clone();
        let at = tampered.len() - 5;
        tampered[at] ^= 1;
        assert!(verify(&tampered, &sig, &key, "stable").is_err());
        assert!(verify(&m, &test_key::sign(2, &m), &key, "stable").is_err());
        assert!(verify(&m, &sig, &key, "beta").is_err());
        assert!(verify(&m, "not a signature", &key, "stable").is_err());
        // a signed manifest with a clear-HTTP artifact or a bad format is refused
        let plain = manifest(
            "stable",
            "9.0.0",
            "http://example.org/a.zip",
            &"a".repeat(64),
            10,
        );
        assert!(verify(&plain, &test_key::sign(1, &plain), &key, "stable").is_err());
        let odd = br#"{"format":"other"}"#;
        assert!(verify(odd, &test_key::sign(1, odd), &key, "stable").is_err());
    }

    // T-UPD-002: only newer releases, never older than the newest accepted.
    #[test]
    fn rollbacks_are_refused() {
        assert!(offered("2.120.0", "2.110.0", None));
        assert!(!offered("2.110.0", "2.110.0", None));
        assert!(!offered("2.100.0", "2.110.0", None));
        assert!(offered("2.120.0", "2.110.0", Some("2.120.0")));
        assert!(
            !offered("2.115.0", "2.110.0", Some("2.120.0")),
            "a replayed older manifest"
        );
        assert!(!offered("x", "2.110.0", None));
        assert_eq!(version_of("1.2.3"), Some((1, 2, 3)));
        assert_eq!(version_of("1.2"), None);
    }

    // T-UPD-001, T-UPD-004: the feed over the network, a resumed checked download.
    #[test]
    fn a_release_is_checked_and_downloaded() {
        let host = FakeFiles::start().unwrap();
        let payload: Vec<u8> = (0..50_000u32).map(|i| (i % 253) as u8).collect();
        let sha: String = Sha256::digest(&payload)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        host.put("/rel.zip", payload.clone());
        let m = manifest(
            "stable",
            "9.0.0",
            &host.url("/rel.zip"),
            &sha,
            payload.len() as u64,
        );
        host.put("/feed/stable.json", m.clone());
        host.put(
            "/feed/stable.json.minisig",
            test_key::sign(1, &m).into_bytes(),
        );
        let http = Http::new("").unwrap();
        let got = check(&http, &host.url("/feed"), "stable", &test_key::public(1)).unwrap();
        assert!(check(&http, &host.url("/feed"), "stable", &test_key::public(3)).is_err());
        let a = artifact(&got, "linux-x86_64").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("rel.zip");
        host.cut_after("/rel.zip", 20_000);
        assert!(download(&http, a, &dest, &Cancel::new(), &mut |_, _| {}).is_err());
        download(&http, a, &dest, &Cancel::new(), &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), payload);
        // a different file under the same name is refused and removed
        let mut wrong = a.clone();
        wrong.sha256 = "0".repeat(64);
        std::fs::remove_file(&dest).unwrap();
        assert!(download(&http, &wrong, &dest, &Cancel::new(), &mut |_, _| {}).is_err());
        assert!(!dest.exists());
    }
}
