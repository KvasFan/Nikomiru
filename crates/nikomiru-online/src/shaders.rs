// SPDX-License-Identifier: GPL-3.0-only

//! The optional Anime4K shader pack (THM-002, THM-003): a pinned MIT release
//! of GLSL shaders for mpv, downloaded only when the learner asks, checked
//! before it is read, and only its known shader files kept.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nikomiru_core::Cancel;
use sha2::{Digest, Sha256};

use crate::OnlineError;
use crate::http::Http;

/// The release.
pub const VERSION: &str = "v4.0.1";
/// Its archive, never a moving "latest".
pub const URL: &str = "https://github.com/bloc97/Anime4K/releases/download/v4.0.1/Anime4K_v4.0.zip";
/// Its SHA-256.
pub const SHA256: &str = "139cd282086457c5adc79caf7b75b8b825091d71c9b54958c18745fea62d7ed7";
/// Its size.
pub const BYTES: u64 = 776_303;
/// Its license (each shader carries it).
pub const LICENSE: &str = "MIT";
/// The largest shader kept.
const MAX_FILE: u64 = 1 << 20;
/// All of them at most.
const MAX_TOTAL: u64 = 4 << 20;

/// The shaders of the release, the only names kept.
pub const FILES: [&str; 39] = [
    "Anime4K_AutoDownscalePre_x2.glsl",
    "Anime4K_AutoDownscalePre_x4.glsl",
    "Anime4K_Clamp_Highlights.glsl",
    "Anime4K_Darken_Fast.glsl",
    "Anime4K_Darken_HQ.glsl",
    "Anime4K_Darken_VeryFast.glsl",
    "Anime4K_Deblur_DoG.glsl",
    "Anime4K_Deblur_Original.glsl",
    "Anime4K_Denoise_Bilateral_Mean.glsl",
    "Anime4K_Denoise_Bilateral_Median.glsl",
    "Anime4K_Denoise_Bilateral_Mode.glsl",
    "Anime4K_Restore_CNN_L.glsl",
    "Anime4K_Restore_CNN_M.glsl",
    "Anime4K_Restore_CNN_S.glsl",
    "Anime4K_Restore_CNN_Soft_L.glsl",
    "Anime4K_Restore_CNN_Soft_M.glsl",
    "Anime4K_Restore_CNN_Soft_S.glsl",
    "Anime4K_Restore_CNN_Soft_UL.glsl",
    "Anime4K_Restore_CNN_Soft_VL.glsl",
    "Anime4K_Restore_CNN_UL.glsl",
    "Anime4K_Restore_CNN_VL.glsl",
    "Anime4K_Thin_Fast.glsl",
    "Anime4K_Thin_HQ.glsl",
    "Anime4K_Thin_VeryFast.glsl",
    "Anime4K_Upscale_CNN_x2_L.glsl",
    "Anime4K_Upscale_CNN_x2_M.glsl",
    "Anime4K_Upscale_CNN_x2_S.glsl",
    "Anime4K_Upscale_CNN_x2_UL.glsl",
    "Anime4K_Upscale_CNN_x2_VL.glsl",
    "Anime4K_Upscale_Deblur_DoG_x2.glsl",
    "Anime4K_Upscale_Deblur_Original_x2.glsl",
    "Anime4K_Upscale_Denoise_CNN_x2_L.glsl",
    "Anime4K_Upscale_Denoise_CNN_x2_M.glsl",
    "Anime4K_Upscale_Denoise_CNN_x2_S.glsl",
    "Anime4K_Upscale_Denoise_CNN_x2_UL.glsl",
    "Anime4K_Upscale_Denoise_CNN_x2_VL.glsl",
    "Anime4K_Upscale_DoG_x2.glsl",
    "Anime4K_Upscale_DTD_x2.glsl",
    "Anime4K_Upscale_Original_x2.glsl",
];

/// Mode A, fast: Anime4K's own preset for lower-end GPUs.
pub const FAST: [&str; 6] = [
    "Anime4K_Clamp_Highlights.glsl",
    "Anime4K_Restore_CNN_M.glsl",
    "Anime4K_Upscale_CNN_x2_M.glsl",
    "Anime4K_AutoDownscalePre_x2.glsl",
    "Anime4K_AutoDownscalePre_x4.glsl",
    "Anime4K_Upscale_CNN_x2_S.glsl",
];

/// Mode A, quality: Anime4K's own preset for higher-end GPUs.
pub const QUALITY: [&str; 6] = [
    "Anime4K_Clamp_Highlights.glsl",
    "Anime4K_Restore_CNN_VL.glsl",
    "Anime4K_Upscale_CNN_x2_VL.glsl",
    "Anime4K_AutoDownscalePre_x2.glsl",
    "Anime4K_AutoDownscalePre_x4.glsl",
    "Anime4K_Upscale_CNN_x2_M.glsl",
];

/// The shaders of a preset, in mpv's order; `None` for none.
pub fn preset(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "anime4k-fast" => Some(&FAST),
        "anime4k-quality" => Some(&QUALITY),
        _ => None,
    }
}

/// Where the pack lives in the data folder.
pub fn folder(data: &Path) -> PathBuf {
    data.join("shaders").join(format!("anime4k-{VERSION}"))
}

/// Whether every shader of the presets is there.
pub fn installed(data: &Path) -> bool {
    let dir = folder(data);
    FAST.iter()
        .chain(QUALITY.iter())
        .all(|name| dir.join(name).is_file())
}

/// The files of `preset_name`, as paths, when the pack has them.
pub fn files(data: &Path, preset_name: &str) -> Option<Vec<PathBuf>> {
    let dir = folder(data);
    let list: Vec<PathBuf> = preset(preset_name)?.iter().map(|n| dir.join(n)).collect();
    list.iter().all(|p| p.is_file()).then_some(list)
}

fn io(e: &std::io::Error) -> OnlineError {
    OnlineError::Io(e.to_string())
}

fn bad(reason: &str) -> OnlineError {
    OnlineError::Unusable(format!("Anime4K: {reason}"))
}

/// Checks `archive` against `expected` (SHA-256 and size), then puts its
/// known shaders in the pack's folder under `data`, replacing an older one
/// only once the new one is whole.
pub fn unpack(
    archive: &Path,
    data: &Path,
    expected: (&str, u64),
    cancel: &Cancel,
) -> Result<PathBuf, OnlineError> {
    let bytes = std::fs::read(archive).map_err(|e| io(&e))?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if bytes.len() as u64 != expected.1 || !digest.eq_ignore_ascii_case(expected.0) {
        return Err(bad("the archive is not the release"));
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| bad("archive"))?;
    let dest = folder(data);
    let stage = dest.with_extension("staging");
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| io(&e))?;
    let result = (|| {
        let mut total = 0u64;
        for i in 0..zip.len() {
            if cancel.is_cancelled() {
                return Err(OnlineError::Cancelled);
            }
            let mut entry = zip.by_index(i).map_err(|_| bad("archive"))?;
            let name = entry.name().map_err(|_| bad("archive"))?.to_string();
            // only the release's own shader names; anything else is skipped
            let Some(known) = FILES.iter().find(|f| **f == name) else {
                continue;
            };
            if entry.size() > MAX_FILE {
                return Err(bad("a shader is too large"));
            }
            let mut text = Vec::new();
            (&mut entry)
                .take(MAX_FILE + 1)
                .read_to_end(&mut text)
                .map_err(|e| io(&e))?;
            total += text.len() as u64;
            if text.len() as u64 > MAX_FILE || total > MAX_TOTAL {
                return Err(bad("too large"));
            }
            std::fs::write(stage.join(known), &text).map_err(|e| io(&e))?;
        }
        if FAST
            .iter()
            .chain(QUALITY.iter())
            .any(|n| !stage.join(n).is_file())
        {
            return Err(bad("shaders are missing"));
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e);
    }
    let old = dest.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    if dest.exists() {
        std::fs::rename(&dest, &old).map_err(|e| io(&e))?;
    }
    if let Err(e) = std::fs::rename(&stage, &dest) {
        let _ = std::fs::rename(&old, &dest);
        return Err(io(&e));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(dest)
}

/// Downloads the pinned release and installs it (THM-002).
pub fn install(http: &Http, data: &Path, cancel: &Cancel) -> Result<PathBuf, OnlineError> {
    let url = reqwest::Url::parse(URL).map_err(|_| OnlineError::BadUrl(URL.into()))?;
    let archive = data.join("shaders").join("anime4k.zip");
    crate::setup::fetch::download_file(
        http,
        &url,
        Some(SHA256),
        &archive,
        Duration::from_secs(2),
        cancel,
        &mut |_| {},
    )?;
    let result = unpack(&archive, data, (SHA256, BYTES), cancel);
    let _ = std::fs::remove_file(&archive);
    result
}

/// Removes the pack.
pub fn remove(data: &Path) -> std::io::Result<()> {
    let dir = folder(data);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            out.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(bytes).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    // T-THM-002: a checked archive gives only the known shaders; anything else is refused.
    #[test]
    fn the_pack_is_checked_and_kept_whole() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let mut entries: Vec<(&str, &[u8])> = FILES
            .iter()
            .filter(|n| FAST.contains(n) || QUALITY.contains(n))
            .map(|n| (*n, b"//!HOOK MAIN\n".as_slice()))
            .collect();
        entries.push(("../evil.glsl", b"x"));
        entries.push(("README.md", b"x"));
        let bytes = archive(&entries);
        let path = dir.path().join("a.zip");
        std::fs::write(&path, &bytes).unwrap();
        // a different hash or size: nothing is read
        assert!(
            unpack(
                &path,
                &data,
                (&"0".repeat(64), bytes.len() as u64),
                &Cancel::new()
            )
            .is_err()
        );
        assert!(unpack(&path, &data, (&sha(&bytes), 1), &Cancel::new()).is_err());
        assert!(!installed(&data));
        let dest = unpack(
            &path,
            &data,
            (&sha(&bytes), bytes.len() as u64),
            &Cancel::new(),
        )
        .unwrap();
        assert!(installed(&data));
        assert!(!dir.path().join("evil.glsl").exists() && !data.join("evil.glsl").exists());
        assert!(!dest.join("README.md").exists());
        assert_eq!(files(&data, "anime4k-fast").unwrap().len(), 6);
        assert!(files(&data, "off").is_none());
        // an archive missing preset shaders leaves the installed pack as it was
        let partial = archive(&[(FAST[0], b"x")]);
        std::fs::write(&path, &partial).unwrap();
        assert!(
            unpack(
                &path,
                &data,
                (&sha(&partial), partial.len() as u64),
                &Cancel::new()
            )
            .is_err()
        );
        assert!(installed(&data));
        remove(&data).unwrap();
        assert!(!installed(&data));
    }

    // T-THM-002: a shader over the bound is refused.
    #[test]
    fn oversized_shaders_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let big = vec![b'a'; MAX_FILE as usize + 1];
        let bytes = archive(&[(FAST[0], &big)]);
        let path = dir.path().join("a.zip");
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            unpack(
                &path,
                dir.path(),
                (&sha(&bytes), bytes.len() as u64),
                &Cancel::new()
            )
            .is_err()
        );
        assert!(!folder(dir.path()).exists());
    }

    // T-THM-002 (L7): the pinned release, downloaded for real.
    #[test]
    #[ignore = "L7: downloads the pinned Anime4K release"]
    fn l7_the_pinned_release_installs() {
        let dir = tempfile::tempdir().unwrap();
        let http =
            Http::with_timeouts("", Duration::from_secs(10), Duration::from_secs(120)).unwrap();
        install(&http, dir.path(), &Cancel::new()).unwrap();
        assert!(installed(dir.path()));
        assert_eq!(
            std::fs::read_dir(folder(dir.path())).unwrap().count(),
            FILES.len()
        );
    }
}
