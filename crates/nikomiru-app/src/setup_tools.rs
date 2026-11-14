// SPDX-License-Identifier: GPL-3.0-only

//! Private support-tool installation, without a shell or privilege changes (INS-007).

use nikomiru_core::Cancel;
use nikomiru_media::{
    process::{Job, output},
    tools::{Search, probe},
};
use nikomiru_online::{
    http::Http,
    setup::{
        fetch::{Url, download_file},
        tools::{Artifact, Tool},
    },
};
use nikomiru_ui::setup::{SetupEvent, ToolPaths, ToolStatus};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

const BINARY_LIMIT: u64 = 256 << 20;
const UNPACKED_LIMIT: u64 = 1 << 30;

fn fail(text: impl ToString) -> String {
    text.to_string()
}

/// Detect one executable using the user's setting, managed bundle, then PATH.
pub fn detect(
    tool: &Tool,
    root: &Path,
    settings: &ToolPaths,
    cancel: &Cancel,
) -> Result<PathBuf, String> {
    let platform = nikomiru_online::setup::tools::platform();
    let artifact = tool
        .artifacts
        .iter()
        .find(|a| a.platform == platform)
        .ok_or("setup-tool-unsupported")?;
    let setting = match tool.id.as_str() {
        "ffmpeg" => &settings.ffmpeg,
        "yt-dlp" => &settings.yt_dlp,
        _ => &settings.deno,
    };
    let program = &artifact.programs[0];
    let managed = root.join(&tool.id).join(&tool.version).join(&platform);
    let configured = PathBuf::from(setting);
    let mut candidates = Vec::new();
    if !setting.is_empty() {
        candidates.push(if configured.is_dir() {
            configured.join(program)
        } else {
            configured
        });
    }
    if tool.id == "ffmpeg"
        && let Ok(tools) = probe(&Search::from_env(setting))
        && validate("ffmpeg", &tools.ffmpeg, cancel).is_ok()
    {
        return Ok(tools.ffmpeg);
    }
    if tool.id == "yt-dlp"
        && setting.is_empty()
        && let Some(path) = std::env::var_os("NIKOMIRU_YT_DLP")
    {
        let path = PathBuf::from(path);
        candidates.push(if path.is_dir() {
            path.join(program)
        } else {
            path
        });
    }
    candidates.push(managed.join(program));
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|p| p.join(program)));
    }
    for path in candidates {
        if cancel.is_cancelled() {
            return Err("setup-tool-cancelled".into());
        }
        if path.is_file() && validate(&tool.id, &path, cancel).is_ok() {
            return Ok(path);
        }
    }
    Err("setup-tool-missing".into())
}

fn validate(id: &str, program: &Path, cancel: &Cancel) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Err("setup-tool-cancelled".into());
    }
    if id == "ffmpeg" {
        let tools = probe(&Search {
            setting: program.to_string_lossy().into_owned(),
            ..Search::default()
        })
        .map_err(fail)?;
        if tools.version.is_empty() {
            return Err("setup-tool-invalid".into());
        }
        let companion = program.with_file_name(format!(
            "ffprobe{}",
            if program.extension().is_some_and(|s| s == "exe") {
                ".exe"
            } else {
                ""
            }
        ));
        let mut job = Job::new(companion, ["-version"]);
        job.timeout = Duration::from_secs(30);
        let text = output(&job, cancel).map_err(fail)?;
        if !text.starts_with("ffprobe version ") {
            return Err("setup-tool-invalid".into());
        }
    } else {
        let args = if id == "yt-dlp" {
            vec!["--ignore-config", "--no-plugin-dirs", "--version"]
        } else {
            vec!["--version"]
        };
        let mut job = Job::new(program, args);
        job.timeout = Duration::from_secs(30);
        let text = output(&job, cancel).map_err(fail)?;
        if (id == "deno" && !text.starts_with("deno 2."))
            || (id == "yt-dlp" && !text.trim().starts_with("20"))
        {
            return Err("setup-tool-invalid".into());
        }
    }
    Ok(())
}

/// Offline inventory of exactly the shipped components.
pub fn inventory(
    parts: &crate::setup::Parts,
    settings: &ToolPaths,
    cancel: &Cancel,
) -> Vec<ToolStatus> {
    let platform = nikomiru_online::setup::tools::platform();
    let mut rows: Vec<_> = parts
        .tool_catalog
        .iter()
        .map(|tool| {
            let path = detect(tool, &parts.tools_dir, settings, cancel).ok();
            ToolStatus {
                id: tool.id.clone(),
                detail: path
                    .as_ref()
                    .map_or_else(String::new, |p| p.display().to_string()),
                path,
                supported: tool.artifacts.iter().any(|a| a.platform == platform),
            }
        })
        .collect();
    rows.push(ToolStatus {
        id: "ocr-model".into(),
        detail: String::new(),
        path: nikomiru_ocr::installed(&parts.model_dir).then(|| parts.model_dir.clone()),
        supported: true,
    });
    rows
}

fn copy(mut source: impl Read, target: &Path, cancel: &Cancel) -> Result<(), String> {
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(fail)?;
    let mut size = 0u64;
    let mut buffer = [0u8; 64 << 10];
    loop {
        if cancel.is_cancelled() {
            return Err("setup-tool-cancelled".into());
        }
        let n = source.read(&mut buffer).map_err(fail)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > BINARY_LIMIT {
            return Err("setup-tool-too-large".into());
        }
        out.write_all(&buffer[..n]).map_err(fail)?;
    }
    if size == 0 {
        return Err("setup-tool-invalid".into());
    }
    out.sync_all().map_err(fail)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        out.set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(fail)?;
    }
    Ok(())
}

fn safe_name(name: &Path) -> Result<String, String> {
    if name
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || name.to_string_lossy().contains(['\\', ':'])
    {
        return Err("setup-tool-archive".into());
    }
    name.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| "setup-tool-archive".into())
}

// Bound decompression even while tar skips non-selected members.
struct LimitedReader<'a, R> {
    inner: R,
    cancel: &'a Cancel,
    remaining: u64,
}
impl<R: Read> Read for LimitedReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("setup-tool-cancelled"));
        }
        if self.remaining == 0 {
            return Err(std::io::Error::other("setup-tool-too-large"));
        }
        let len = buffer.len().min(self.remaining.min(64 << 10) as usize);
        let n = self.inner.read(&mut buffer[..len])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

/// Extract allowlisted regular members only, never archive-provided output paths.
pub fn unpack(
    file: &Path,
    artifact: &Artifact,
    stage: &Path,
    cancel: &Cancel,
) -> Result<(), String> {
    let mut found = BTreeSet::new();
    match artifact.format.as_str() {
        "raw" => {
            copy(
                std::fs::File::open(file).map_err(fail)?,
                &stage.join(&artifact.programs[0]),
                cancel,
            )?;
            found.insert(artifact.programs[0].clone());
        }
        "zip" => {
            let mut archive =
                zip::ZipArchive::new(std::fs::File::open(file).map_err(fail)?).map_err(fail)?;
            if archive.len() > 20_000 {
                return Err("setup-tool-too-large".into());
            }
            let mut total = 0u64;
            for i in 0..archive.len() {
                if cancel.is_cancelled() {
                    return Err("setup-tool-cancelled".into());
                }
                let mut entry = archive.by_index(i).map_err(fail)?;
                let name = safe_name(Path::new(entry.name().map_err(fail)?.trim_end_matches('/')))?;
                total = total.saturating_add(entry.size());
                if total > UNPACKED_LIMIT {
                    return Err("setup-tool-too-large".into());
                }
                let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
                if mode != 0 && mode != 0o100000 && mode != 0o040000 {
                    return Err("setup-tool-archive".into());
                }
                if !artifact.programs.contains(&name) {
                    continue;
                }
                if entry.is_dir() || !found.insert(name.clone()) {
                    return Err("setup-tool-archive".into());
                }
                copy(&mut entry, &stage.join(name), cancel)?;
            }
        }
        "tar.xz" => {
            let file = std::fs::File::open(file).map_err(fail)?;
            let decoder = lzma_rust2::XzReader::new_mem_limit(file, false, 128 << 10);
            let mut archive = tar::Archive::new(LimitedReader {
                inner: decoder,
                cancel,
                remaining: UNPACKED_LIMIT,
            });
            let mut total = 0u64;
            for (i, entry) in archive.entries().map_err(fail)?.enumerate() {
                if i >= 20_000 {
                    return Err("setup-tool-too-large".into());
                }
                if cancel.is_cancelled() {
                    return Err("setup-tool-cancelled".into());
                }
                let mut entry = entry.map_err(fail)?;
                let name = safe_name(&entry.path().map_err(fail)?)?;
                let kind = entry.header().entry_type();
                if !kind.is_file() && !kind.is_dir() {
                    return Err("setup-tool-archive".into());
                }
                total = total.saturating_add(entry.size());
                if total > UNPACKED_LIMIT {
                    return Err("setup-tool-too-large".into());
                }
                if !artifact.programs.contains(&name) {
                    continue;
                }
                if !kind.is_file() || !found.insert(name.clone()) {
                    return Err("setup-tool-archive".into());
                }
                copy(&mut entry, &stage.join(name), cancel)?;
            }
        }
        _ => return Err("setup-tool-archive".into()),
    }
    if artifact.programs.iter().any(|p| !found.contains(p)) {
        return Err("setup-tool-archive".into());
    }
    Ok(())
}

fn publish(stage: &Path, dest: &Path) -> Result<(), String> {
    let parent = dest.parent().ok_or("setup-tool-invalid")?;
    if std::fs::symlink_metadata(dest).is_ok_and(|m| !m.is_dir() || m.file_type().is_symlink()) {
        return Err("setup-tool-damaged".into());
    }
    let backup = parent.join(format!(".previous-{}", uuid::Uuid::new_v4()));
    let previous = dest.exists();
    if previous {
        std::fs::rename(dest, &backup).map_err(fail)?;
    }
    if let Err(error) = std::fs::rename(stage, dest) {
        if previous {
            let _ = std::fs::rename(&backup, dest);
        }
        return Err(error.to_string());
    }
    if previous {
        let _ = std::fs::remove_dir_all(backup);
    }
    #[cfg(unix)]
    {
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(fail)?;
    }
    Ok(())
}

/// Install a selected bundle, leaving existing complete versions untouched on failure.
pub fn install(
    parts: &crate::setup::Parts,
    http: &Http,
    tool: &Tool,
    settings: &ToolPaths,
    events: &dyn Fn(SetupEvent),
    cancel: &Cancel,
) -> Result<PathBuf, String> {
    if let Ok(path) = detect(tool, &parts.tools_dir, settings, cancel) {
        return Ok(path);
    }
    if cancel.is_cancelled() {
        return Err("setup-tool-cancelled".into());
    }
    let platform = nikomiru_online::setup::tools::platform();
    let artifact = tool
        .artifacts
        .iter()
        .find(|a| a.platform == platform)
        .ok_or("setup-tool-unsupported")?;
    if artifact.bytes > 512 << 20 {
        return Err("setup-tool-too-large".into());
    }
    let dest = parts
        .tools_dir
        .join(&tool.id)
        .join(&tool.version)
        .join(&platform);
    let parent = dest.parent().ok_or("setup-tool-invalid")?;
    std::fs::create_dir_all(parent).map_err(fail)?;
    if crate::platform::free_space(parent).is_some_and(|n| n < artifact.bytes + (1 << 30)) {
        return Err("setup-tool-space".into());
    }
    let stage = parent.join(format!(".install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&stage).map_err(fail)?;
    let downloaded = stage.join("download");
    let result = (|| {
        download_file(
            http,
            &Url::parse(&artifact.url).map_err(fail)?,
            Some(&artifact.sha256),
            &downloaded,
            Duration::ZERO,
            cancel,
            &mut |done| {
                events(SetupEvent::Progress {
                    id: tool.id.clone(),
                    done,
                    total: artifact.bytes,
                })
            },
        )
        .map_err(fail)?;
        if std::fs::metadata(&downloaded).map_err(fail)?.len() != artifact.bytes {
            return Err("setup-tool-invalid".into());
        }
        events(SetupEvent::Checking {
            id: tool.id.clone(),
        });
        unpack(&downloaded, artifact, &stage, cancel)?;
        std::fs::remove_file(&downloaded).map_err(fail)?;
        validate(&tool.id, &stage.join(&artifact.programs[0]), cancel)?;
        std::fs::write(
            stage.join("SOURCE.txt"),
            format!(
                "{} {}\nLicense: {}\nSource: {}\nArtifact: {}\nSHA-256: {}\n",
                tool.name, tool.version, tool.license, tool.source, artifact.url, artifact.sha256
            ),
        )
        .map_err(fail)?;
        if cancel.is_cancelled() {
            return Err("setup-tool-cancelled".into());
        }
        publish(&stage, &dest)?;
        Ok(dest.join(&artifact.programs[0]))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    result
}

/// Download the existing pinned OCR model as a complete staged directory.
pub fn model(
    parts: &crate::setup::Parts,
    http: &Http,
    events: &dyn Fn(SetupEvent),
    cancel: &Cancel,
) -> Result<(), String> {
    if nikomiru_ocr::installed(&parts.model_dir) {
        return Ok(());
    }
    let parent = parts.model_dir.parent().ok_or("setup-tool-invalid")?;
    std::fs::create_dir_all(parent).map_err(fail)?;
    if crate::platform::free_space(parent)
        .is_some_and(|n| n < nikomiru_ocr::model_bytes() + (64 << 20))
    {
        return Err("setup-tool-space".into());
    }
    let stage = parent.join(format!(".ocr-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&stage).map_err(fail)?;
    let result = (|| {
        let mut before = 0;
        for file in nikomiru_ocr::MODEL {
            download_file(
                http,
                &Url::parse(file.url).map_err(fail)?,
                Some(file.sha256),
                &stage.join(file.name),
                Duration::ZERO,
                cancel,
                &mut |done| {
                    events(SetupEvent::Progress {
                        id: "ocr-model".into(),
                        done: before + done,
                        total: nikomiru_ocr::model_bytes(),
                    })
                },
            )
            .map_err(fail)?;
            if std::fs::metadata(stage.join(file.name))
                .map_err(fail)?
                .len()
                != file.bytes
            {
                return Err("setup-tool-invalid".into());
            }
            before += file.bytes;
        }
        if cancel.is_cancelled() {
            return Err("setup-tool-cancelled".into());
        }
        publish(&stage, &parts.model_dir)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-INS-007: skipped tar members are bounded and respond to cancellation too.
    #[test]
    fn decompression_is_bounded_and_cancellable() {
        let cancel = Cancel::new();
        let mut reader = LimitedReader {
            inner: std::io::repeat(0),
            cancel: &cancel,
            remaining: 3,
        };
        let mut buf = [0u8; 16];
        assert_eq!(reader.read(&mut buf).unwrap(), 3);
        assert!(reader.read(&mut buf).is_err());
        cancel.cancel();
        let mut reader = LimitedReader {
            inner: std::io::repeat(0),
            cancel: &cancel,
            remaining: 100,
        };
        assert!(reader.read(&mut buf).is_err());
    }

    // T-INS-007: an unsuccessful publication restores the previous bundle.
    #[test]
    fn failed_publication_restores_the_previous_directory() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("installed");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("old"), b"keep").unwrap();
        assert!(publish(&root.path().join("missing-stage"), &dest).is_err());
        assert_eq!(std::fs::read(dest.join("old")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    // T-INS-007: real upstream SHA-256, archive layout and executable compatibility.
    #[test]
    #[ignore = "L7: downloads the pinned upstream tool bundles and executes their version probes"]
    fn l7_pinned_support_tools_download_extract_and_run() {
        let root = tempfile::tempdir().unwrap();
        let http =
            Http::with_timeouts("", Duration::from_secs(15), Duration::from_secs(300)).unwrap();
        let cancel = Cancel::new();
        for tool in nikomiru_online::setup::tools::catalog() {
            let artifact = tool
                .artifacts
                .iter()
                .find(|a| a.platform == nikomiru_online::setup::tools::platform())
                .unwrap();
            let stage = root.path().join(&tool.id);
            std::fs::create_dir(&stage).unwrap();
            let file = stage.join("download");
            download_file(
                &http,
                &Url::parse(&artifact.url).unwrap(),
                Some(&artifact.sha256),
                &file,
                Duration::ZERO,
                &cancel,
                &mut |_| {},
            )
            .unwrap();
            assert_eq!(std::fs::metadata(&file).unwrap().len(), artifact.bytes);
            unpack(&file, artifact, &stage, &cancel).unwrap();
            validate(&tool.id, &stage.join(&artifact.programs[0]), &cancel).unwrap();
            eprintln!(
                "{} {}: download/hash/extraction/version passed",
                tool.name, tool.version
            );
        }
    }
}
