// SPDX-License-Identifier: GPL-3.0-only

//! The downloads of the setup assistant (INS-002…INS-004): Fushi's pack
//! read by range, an entry of it written to a file that resumes from its
//! length and is checked against its CRC-32, and a dictionary from the
//! catalogue checked against its SHA-256.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_core::Cancel;
pub use reqwest::Url;
use sha2::{Digest, Sha256};

use super::pack::{
    Entry, MAX_ENTRY, MAX_MANIFEST, Manifest, data_offset, locate_directory, parse_directory,
    parse_manifest, tail_length,
};
use crate::error::OnlineError;
use crate::http::Http;

/// Where the manifest of Fushi's pack is read, in order (INS-003).
pub const MANIFEST_URLS: [&str; 2] = [
    "https://fushi.moe/pack/manifest.json",
    "https://github.com/hajisensai/fushi-pack/releases/latest/download/manifest.json",
];

/// The bytes asked by one request (INS-004).
pub const CHUNK: u64 = 16 << 20;

/// The tries of a request at each address.
const ATTEMPTS: u32 = 3;

/// The largest dictionary downloaded (INS-002).
pub const MAX_DICTIONARY: u64 = 512 << 20;

fn bad(reason: impl Into<String>) -> OnlineError {
    OnlineError::Protocol {
        provider: "pack".into(),
        reason: reason.into(),
    }
}

fn io(e: &std::io::Error) -> OnlineError {
    OnlineError::Io(e.to_string())
}

/// The first manifest of `urls` that reads as one.
pub fn manifest(http: &Http, urls: &[Url]) -> Result<Manifest, OnlineError> {
    let mut last = bad("no manifest");
    for url in urls {
        match http.file(url, &[]) {
            Ok(answer) if (200..300).contains(&answer.status) => {
                if answer.body.len() > MAX_MANIFEST {
                    last = bad("manifest too large");
                    continue;
                }
                match parse_manifest(&String::from_utf8_lossy(&answer.body)) {
                    Some(m) => return Ok(m),
                    None => last = bad("not a manifest"),
                }
            }
            Ok(answer) => {
                last = OnlineError::Http {
                    provider: "pack".into(),
                    status: answer.status,
                }
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Fushi's pack, read by range through `http`.
#[derive(Debug)]
pub struct Pack<'a> {
    http: &'a Http,
    /// What its manifest says.
    pub manifest: Manifest,
    /// Waits between the rounds of tries; none in the tests.
    pub backoff: Duration,
}

impl<'a> Pack<'a> {
    /// The pack of `manifest`.
    pub fn new(http: &'a Http, manifest: Manifest) -> Self {
        Self {
            http,
            manifest,
            backoff: Duration::from_secs(2),
        }
    }

    /// `len` bytes of the pack from `from`, a request per part and chunk,
    /// each tried at every address that has it, three rounds.
    pub fn read(&self, from: u64, len: u64, cancel: &Cancel) -> Result<Vec<u8>, OnlineError> {
        let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0).min(64 << 20));
        for span in self.manifest.spans(from, len, CHUNK) {
            out.extend(self.span(&span.sources, span.len, cancel)?);
        }
        Ok(out)
    }

    fn span(
        &self,
        sources: &[(Url, u64)],
        len: u64,
        cancel: &Cancel,
    ) -> Result<Vec<u8>, OnlineError> {
        let mut last = bad("no address for this part");
        for round in 0..ATTEMPTS {
            for (url, at) in sources {
                if cancel.is_cancelled() {
                    return Err(OnlineError::Cancelled);
                }
                match self.http.range(url, *at, len) {
                    Ok(bytes) => return Ok(bytes),
                    Err(e) => {
                        tracing::warn!("pack {url} at {at}: {e}");
                        last = e;
                    }
                }
            }
            if round + 1 < ATTEMPTS {
                std::thread::sleep(self.backoff * (round + 1));
            }
        }
        Err(last)
    }

    /// The pack's entries, from its tail.
    pub fn directory(&self, cancel: &Cancel) -> Result<Vec<Entry>, OnlineError> {
        let size = self.manifest.size;
        let tail_len = tail_length(size);
        let tail = self.read(size - tail_len, tail_len, cancel)?;
        let (offset, length, count) = locate_directory(&tail, size)?;
        let directory = if offset >= size - tail_len {
            let start = usize::try_from(offset - (size - tail_len)).map_err(|_| bad("offset"))?;
            let end = start + usize::try_from(length).map_err(|_| bad("length"))?;
            tail.get(start..end)
                .ok_or_else(|| bad("directory"))?
                .to_vec()
        } else {
            self.read(offset, length, cancel)?
        };
        parse_directory(&directory, count)
    }

    /// Where an entry's bytes start.
    pub fn data_offset(&self, entry: &Entry, cancel: &Cancel) -> Result<u64, OnlineError> {
        let local = self.read(entry.header, 30, cancel)?;
        let at = data_offset(&local, entry.header)?;
        if at.saturating_add(entry.compressed) > self.manifest.size {
            return Err(bad("entry past the end of the pack"));
        }
        Ok(at)
    }

    /// A small stored entry, at most `limit` bytes, checked.
    pub fn small(
        &self,
        entry: &Entry,
        limit: u64,
        cancel: &Cancel,
    ) -> Result<Vec<u8>, OnlineError> {
        if entry.method != 0 || entry.size > limit || entry.compressed != entry.size {
            return Err(bad(format!("{} cannot be read", entry.name)));
        }
        let at = self.data_offset(entry, cancel)?;
        let bytes = self.read(at, entry.size, cancel)?;
        if crc32fast::hash(&bytes) != entry.crc32 {
            return Err(bad(format!("{}: CRC-32 does not match", entry.name)));
        }
        Ok(bytes)
    }

    /// Downloads a stored entry to `dest` (INS-004): to `dest` with `.part`
    /// added, going on from its length, then checked against the entry's
    /// CRC-32 and renamed. `progress` hears the bytes there so far.
    pub fn download(
        &self,
        entry: &Entry,
        dest: &Path,
        cancel: &Cancel,
        progress: &mut dyn FnMut(u64),
    ) -> Result<PathBuf, OnlineError> {
        if entry.method != 0 || entry.compressed != entry.size {
            return Err(bad(format!("{} is compressed", entry.name)));
        }
        if entry.size > MAX_ENTRY {
            return Err(bad(format!("{} is too large", entry.name)));
        }
        let at = self.data_offset(entry, cancel)?;
        let part = part_path(dest);
        if let Some(dir) = part.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(&e))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&part)
            .map_err(|e| io(&e))?;
        let mut done = file.metadata().map_err(|e| io(&e))?.len();
        if done > entry.size {
            file.set_len(0).map_err(|e| io(&e))?;
            done = 0;
        }
        progress(done);
        for span in self.manifest.spans(at + done, entry.size - done, CHUNK) {
            let bytes = self.span(&span.sources, span.len, cancel)?;
            file.write_all(&bytes).map_err(|e| io(&e))?;
            done += bytes.len() as u64;
            progress(done);
        }
        file.flush().map_err(|e| io(&e))?;
        file.sync_all().map_err(|e| io(&e))?;
        if file_crc(&mut file, cancel)? != entry.crc32 {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err(bad(format!("{}: CRC-32 does not match", entry.name)));
        }
        drop(file);
        std::fs::rename(&part, dest).map_err(|e| io(&e))?;
        Ok(dest.to_path_buf())
    }
}

/// The file a download is written to before it is complete.
pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn file_crc(file: &mut std::fs::File, cancel: &Cancel) -> Result<u32, OnlineError> {
    file.seek(SeekFrom::Start(0)).map_err(|e| io(&e))?;
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        if cancel.is_cancelled() {
            return Err(OnlineError::Cancelled);
        }
        let n = file.read(&mut buffer).map_err(|e| io(&e))?;
        if n == 0 {
            return Ok(hasher.finalize());
        }
        hasher.update(&buffer[..n]);
    }
}

/// Whether a failure may pass by itself: no answer, a gateway's or a
/// server's error, too many requests (AK-BUG-005).
fn passing(e: &OnlineError) -> bool {
    match e {
        OnlineError::Unreachable { .. }
        | OnlineError::Timeout { .. }
        | OnlineError::RateLimited { .. } => true,
        OnlineError::Http { status, .. } => *status >= 500,
        _ => false,
    }
}

/// Downloads a dictionary of the catalogue to `dest` (INS-002): written to
/// `dest` with `.part` added, checked against `sha256` when there is one,
/// renamed. A failure that may pass by itself, such as GitHub's 504, is
/// tried again from the start, [`ATTEMPTS`] times in all, after `backoff`,
/// then twice that (AK-BUG-005).
pub fn download_file(
    http: &Http,
    url: &Url,
    sha256: Option<&str>,
    dest: &Path,
    backoff: Duration,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64),
) -> Result<PathBuf, OnlineError> {
    let mut round = 0;
    loop {
        match download_once(http, url, sha256, dest, cancel, progress) {
            Err(e) if passing(&e) && round + 1 < ATTEMPTS && !cancel.is_cancelled() => {
                tracing::warn!("dictionary {url}: {e}, trying again");
                round += 1;
                std::thread::sleep(backoff * round);
                progress(0);
            }
            result => return result,
        }
    }
}

fn download_once(
    http: &Http,
    url: &Url,
    sha256: Option<&str>,
    dest: &Path,
    cancel: &Cancel,
    progress: &mut dyn FnMut(u64),
) -> Result<PathBuf, OnlineError> {
    let part = part_path(dest);
    if let Some(dir) = part.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io(&e))?;
    }
    let mut file = std::fs::File::create(&part).map_err(|e| io(&e))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let result = http.stream(url, "setup", MAX_DICTIONARY, &mut |piece| {
        if cancel.is_cancelled() {
            return Err(OnlineError::Cancelled);
        }
        file.write_all(piece).map_err(|e| io(&e))?;
        hasher.update(piece);
        done += piece.len() as u64;
        progress(done);
        Ok(())
    });
    let result = result.and_then(|()| file.flush().map_err(|e| io(&e)));
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if let Some(want) = sha256
        && !want.eq_ignore_ascii_case(&digest)
    {
        let _ = std::fs::remove_file(&part);
        return Err(OnlineError::Protocol {
            provider: url.host_str().unwrap_or("setup").to_string(),
            reason: "SHA-256 does not match".into(),
        });
    }
    std::fs::rename(&part, dest).map_err(|e| io(&e))?;
    Ok(dest.to_path_buf())
}
