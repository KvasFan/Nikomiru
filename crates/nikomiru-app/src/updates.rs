// SPDX-License-Identifier: GPL-3.0-only

//! Installing a verified update (UPD-003, UPD-004): who owns this install,
//! and the swap of a portable install's folder for a release checked to run.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use nikomiru_core::Cancel;

/// Who replaces this program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    /// Flatpak: its own updates.
    Flatpak,
    /// A distribution package (or anything under the system's folders).
    Package,
    /// A folder the user cannot write.
    ReadOnly,
    /// A portable install: this folder may be replaced.
    Portable(PathBuf),
}

/// The ownership of the install whose program is `exe`.
pub fn ownership_of(exe: &Path, flatpak: bool) -> Ownership {
    if flatpak {
        return Ownership::Flatpak;
    }
    let system = [
        "/usr/", "/opt/", "/app/", "/bin/", "/sbin/", "/nix/", "/snap/",
    ];
    let text = exe.to_string_lossy();
    if system.iter().any(|p| text.starts_with(p)) {
        return Ownership::Package;
    }
    let Some(root) = exe.parent() else {
        return Ownership::ReadOnly;
    };
    let probe = root.join(".nikomiru-update-probe");
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    let parent_writable = root.parent().is_some_and(|p| {
        let probe = p.join(".nikomiru-update-probe");
        let ok = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    });
    if writable && parent_writable {
        Ownership::Portable(root.to_path_buf())
    } else {
        Ownership::ReadOnly
    }
}

/// This program's ownership.
pub fn ownership() -> Ownership {
    #[cfg(feature = "test-harness")]
    if let Some(root) = std::env::var_os("NIKOMIRU_TEST_UPDATE_ROOT") {
        return Ownership::Portable(PathBuf::from(root));
    }
    let flatpak = std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists();
    match std::env::current_exe() {
        Ok(exe) => ownership_of(&exe, flatpak),
        Err(_) => Ownership::ReadOnly,
    }
}

/// Entries at most.
const MAX_ENTRIES: usize = 20_000;
/// Unpacked bytes at most.
const MAX_UNPACKED: u64 = 4 << 30;

fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A relative path made of plain names only.
fn safe(path: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return Err("unsafe name in the archive".into()),
        }
    }
    if out.as_os_str().is_empty() || out.to_string_lossy().contains('\\') {
        return Err("unsafe name in the archive".into());
    }
    Ok(out)
}

fn write_file(
    source: &mut dyn Read,
    target: &Path,
    executable: bool,
    budget: &mut u64,
) -> Result<(), String> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let mut out = std::fs::File::create(target).map_err(fail)?;
    let copied = std::io::copy(&mut source.take(*budget + 1), &mut out).map_err(fail)?;
    if copied > *budget {
        return Err("the archive is too large".into());
    }
    *budget -= copied;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755)).map_err(fail)?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

/// Extracts `archive` (`tar.xz` or `zip`) into `stage`, keeping safe relative
/// names only; links and devices are refused.
pub fn extract(archive: &Path, kind: &str, stage: &Path, cancel: &Cancel) -> Result<(), String> {
    std::fs::create_dir_all(stage).map_err(fail)?;
    let mut budget = MAX_UNPACKED;
    match kind {
        "zip" => {
            let mut zip =
                zip::ZipArchive::new(std::fs::File::open(archive).map_err(fail)?).map_err(fail)?;
            if zip.len() > MAX_ENTRIES {
                return Err("the archive is too large".into());
            }
            for i in 0..zip.len() {
                if cancel.is_cancelled() {
                    return Err("cancelled".into());
                }
                let mut entry = zip.by_index(i).map_err(fail)?;
                let name = safe(Path::new(entry.name().map_err(fail)?.trim_end_matches('/')))?;
                let mode = entry.unix_mode().unwrap_or(0);
                let kind = mode & 0o170000;
                if kind != 0 && kind != 0o100000 && kind != 0o040000 {
                    return Err("links are not installed".into());
                }
                if entry.is_dir() {
                    std::fs::create_dir_all(stage.join(&name)).map_err(fail)?;
                } else {
                    write_file(
                        &mut entry,
                        &stage.join(&name),
                        mode & 0o111 != 0,
                        &mut budget,
                    )?;
                }
            }
        }
        "tar.xz" => {
            let file = std::fs::File::open(archive).map_err(fail)?;
            let decoder = lzma_rust2::XzReader::new_mem_limit(file, false, 128 << 10);
            let mut tar = tar::Archive::new(decoder);
            for (i, entry) in tar.entries().map_err(fail)?.enumerate() {
                if i >= MAX_ENTRIES || cancel.is_cancelled() {
                    return Err("the archive is too large or the install was stopped".into());
                }
                let mut entry = entry.map_err(fail)?;
                let name = safe(&entry.path().map_err(fail)?)?;
                let kind = entry.header().entry_type();
                if kind.is_dir() {
                    std::fs::create_dir_all(stage.join(&name)).map_err(fail)?;
                } else if kind.is_file() {
                    let executable = entry.header().mode().is_ok_and(|m| m & 0o111 != 0);
                    write_file(&mut entry, &stage.join(&name), executable, &mut budget)?;
                } else {
                    return Err("links are not installed".into());
                }
            }
        }
        _ => return Err("unknown archive".into()),
    }
    Ok(())
}

/// The program's name inside an install.
fn program() -> &'static str {
    if cfg!(windows) {
        "nikomiru.exe"
    } else {
        "nikomiru"
    }
}

/// Whether `stage`'s program says it is `release` (`nikomiru <release>`).
pub fn runs_as(stage: &Path, release: &str) -> Result<(), String> {
    let exe = stage.join(program());
    let mut child = std::process::Command::new(&exe)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("the new program does not start: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(fail)? {
            Some(_) => break,
            None if started.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the new program does not answer".into());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut out = String::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(4096).read_to_string(&mut out);
    }
    let want = format!("nikomiru {release}");
    if out.lines().next().is_some_and(|l| l.trim() == want) {
        Ok(())
    } else {
        Err(format!(
            "the new program says {:?}, not {want:?}",
            out.trim()
        ))
    }
}

/// Unpacks `archive` into `stage`. Release archives keep everything in one
/// top folder (`Nikomiru-<release>/`): when the program is not at the top but
/// that single folder is all there is, its content becomes the stage.
fn unpack(archive: &Path, kind: &str, stage: &Path, cancel: &Cancel) -> Result<(), String> {
    let unpacked = stage.with_extension("unpack");
    let _ = std::fs::remove_dir_all(&unpacked);
    let result = extract(archive, kind, &unpacked, cancel).and_then(|()| {
        let entries: Vec<_> = std::fs::read_dir(&unpacked)
            .map_err(fail)?
            .collect::<Result<_, _>>()
            .map_err(fail)?;
        let content = match entries.as_slice() {
            [only] if !unpacked.join(program()).exists() && only.path().is_dir() => only.path(),
            _ => unpacked.clone(),
        };
        std::fs::rename(&content, stage).map_err(fail)
    });
    let _ = std::fs::remove_dir_all(&unpacked);
    result
}

/// Installs `archive` over the portable install at `root` (UPD-004): into
/// `<root>.update`, checked to run as `release`, then swapped in, the old
/// folder kept as `<root>.previous`. Any failure leaves `root` as it was.
pub fn install(
    archive: &Path,
    kind: &str,
    root: &Path,
    release: &str,
    cancel: &Cancel,
) -> Result<(), String> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("no install folder")?;
    let stage = root.with_file_name(format!("{name}.update"));
    let previous = root.with_file_name(format!("{name}.previous"));
    let _ = std::fs::remove_dir_all(&stage);
    let checked = unpack(archive, kind, &stage, cancel).and_then(|()| runs_as(&stage, release));
    if let Err(e) = checked {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&previous);
    std::fs::rename(root, &previous).map_err(|e| {
        let _ = std::fs::remove_dir_all(&stage);
        format!("the install cannot be moved: {e}")
    })?;
    if let Err(e) = std::fs::rename(&stage, root) {
        let _ = std::fs::rename(&previous, root);
        let _ = std::fs::remove_dir_all(&stage);
        return Err(format!("the update cannot be put in place: {e}"));
    }
    Ok(())
}

/// The app's update worker (UPD-001…UPD-004): every job on a thread of its
/// own, only when asked.
pub struct Updater {
    /// The public key, if this build has one.
    key: Option<String>,
    /// Where the channels are published.
    feed: String,
    /// The data folder: downloads and the newest accepted release.
    data_dir: PathBuf,
    /// The running version.
    current: String,
    translator: nikomiru_ui::i18n::Translator,
}

impl std::fmt::Debug for Updater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updater")
            .field("feed", &self.feed)
            .finish_non_exhaustive()
    }
}

impl Updater {
    /// The worker for this build.
    pub fn new(data_dir: PathBuf, translator: nikomiru_ui::i18n::Translator) -> Self {
        #[allow(unused_mut)]
        let mut updater = Self {
            key: nikomiru_online::updates::PRODUCTION_KEY.map(str::to_string),
            feed: nikomiru_online::updates::FEED.into(),
            data_dir,
            current: env!("CARGO_PKG_VERSION").into(),
            translator,
        };
        // the GUI tests point it at a fake feed with a test key
        #[cfg(feature = "test-harness")]
        {
            if let Ok(key) = std::env::var("NIKOMIRU_TEST_UPDATE_KEY") {
                updater.key = Some(key);
            }
            if let Ok(feed) = std::env::var("NIKOMIRU_TEST_UPDATE_FEED") {
                updater.feed = feed;
            }
        }
        updater
    }

    fn floor_file(data: &Path, channel: &str) -> PathBuf {
        data.join("updates").join(format!("{channel}.accepted"))
    }
}

fn http(
    proxy: &str,
    request: Duration,
) -> Result<nikomiru_online::http::Http, nikomiru_online::OnlineError> {
    nikomiru_online::http::Http::with_timeouts(
        proxy,
        nikomiru_online::http::CONNECT_TIMEOUT,
        request,
    )
}

fn spawn(name: &str, job: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(job) {
        tracing::error!("{name}: {e}");
    }
}

impl nikomiru_ui::updates::UpdateService for Updater {
    fn owner(&self) -> nikomiru_ui::updates::Owner {
        use nikomiru_ui::updates::Owner;
        match ownership() {
            Ownership::Flatpak => Owner::Flatpak,
            Ownership::Package => Owner::Package,
            Ownership::ReadOnly => Owner::ReadOnly,
            Ownership::Portable(_) => Owner::Portable,
        }
    }

    fn configured(&self) -> bool {
        self.key.is_some()
    }

    fn check(
        &self,
        channel: String,
        proxy: String,
        done: nikomiru_ui::updates::Done<Option<nikomiru_ui::updates::Available>>,
    ) {
        let Some(key) = self.key.clone() else {
            return done(Err(self.translator.tr("updates-unconfigured", &[])));
        };
        let (feed, data, current) = (
            self.feed.clone(),
            self.data_dir.clone(),
            self.current.clone(),
        );
        let tr = self.translator.clone();
        spawn("update-check", move || {
            let result = http(&proxy, Duration::from_secs(30))
                .and_then(|h| nikomiru_online::updates::check(&h, &feed, &channel, &key))
                .map_err(|e| tr.tr("updates-failed", &[("reason", e.to_string())]))
                .map(|manifest| {
                    let floor_file = Updater::floor_file(&data, &channel);
                    let floor = std::fs::read_to_string(&floor_file).ok();
                    let floor = floor.as_deref().map(str::trim);
                    // UPD-002: the newest verified release is remembered
                    let newer_than_floor = floor
                        .and_then(nikomiru_online::updates::version_of)
                        .is_none_or(|f| {
                            nikomiru_online::updates::version_of(&manifest.release)
                                .is_some_and(|r| r > f)
                        });
                    if newer_than_floor {
                        let _ = std::fs::create_dir_all(floor_file.parent().unwrap_or(&data));
                        let _ = nikomiru_core::fs::write_atomic(
                            &floor_file,
                            manifest.release.as_bytes(),
                            0o600,
                        );
                    }
                    nikomiru_online::updates::offered(&manifest.release, &current, floor).then(
                        || {
                            let platform = nikomiru_online::setup::tools::platform();
                            nikomiru_ui::updates::Available {
                                artifact: nikomiru_online::updates::artifact(&manifest, &platform)
                                    .cloned(),
                                release: manifest.release,
                                published: manifest.published,
                                notes_url: manifest.notes_url,
                            }
                        },
                    )
                });
            done(result);
        });
    }

    fn download(
        &self,
        available: nikomiru_ui::updates::Available,
        proxy: String,
        progress: Box<dyn Fn(u64, Option<u64>) + Send>,
        done: nikomiru_ui::updates::Done<PathBuf>,
    ) {
        let Some(artifact) = available.artifact.clone() else {
            return done(Err(self.translator.tr("updates-no-artifact", &[])));
        };
        let dest = self
            .data_dir
            .join("updates")
            .join(format!("nikomiru-{}.{}", available.release, artifact.kind));
        let tr = self.translator.clone();
        spawn("update-download", move || {
            let mut last = std::time::Instant::now();
            let result = http(&proxy, Duration::from_secs(6 * 3600))
                .and_then(|h| {
                    nikomiru_online::updates::download(
                        &h,
                        &artifact,
                        &dest,
                        &Cancel::new(),
                        &mut |d, t| {
                            if last.elapsed() > Duration::from_millis(200) {
                                last = std::time::Instant::now();
                                progress(d, t);
                            }
                        },
                    )
                })
                .map(|()| dest)
                .map_err(|e| tr.tr("updates-failed", &[("reason", e.to_string())]));
            done(result);
        });
    }

    fn install(
        &self,
        archive: PathBuf,
        available: nikomiru_ui::updates::Available,
        done: nikomiru_ui::updates::Done<()>,
    ) {
        let Ownership::Portable(root) = ownership() else {
            return done(Err(self.translator.tr("updates-not-portable", &[])));
        };
        let Some(kind) = available.artifact.as_ref().map(|a| a.kind.clone()) else {
            return done(Err(self.translator.tr("updates-no-artifact", &[])));
        };
        let tr = self.translator.clone();
        spawn("update-install", move || {
            let result = install(&archive, &kind, &root, &available.release, &Cancel::new())
                .map_err(|e| tr.tr("updates-failed", &[("reason", e)]));
            if result.is_ok() {
                let _ = std::fs::remove_file(&archive);
            }
            done(result);
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::io::Write;

    use super::*;

    #[cfg(unix)]
    fn release_zip(path: &Path, says: &str, extra: &[(&str, u32)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let exec = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("nikomiru", exec).unwrap();
        zip.write_all(format!("#!/bin/sh\necho 'nikomiru {says}'\n").as_bytes())
            .unwrap();
        zip.add_directory("assets/", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.start_file("assets/notes.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"new").unwrap();
        for (name, mode) in extra {
            if *mode == 0o120777 {
                zip.add_symlink(
                    *name,
                    "/etc/passwd",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            } else {
                zip.start_file(
                    *name,
                    zip::write::SimpleFileOptions::default().unix_permissions(*mode),
                )
                .unwrap();
                zip.write_all(b"x").unwrap();
            }
        }
        zip.finish().unwrap();
    }

    fn portable(dir: &Path) -> PathBuf {
        let root = dir.join("Nikomiru");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("old.txt"), b"old").unwrap();
        root
    }

    // T-UPD-004: a release checked to run replaces the folder, the old one kept.
    #[cfg(unix)]
    #[test]
    fn a_checked_release_is_swapped_in() {
        let dir = tempfile::tempdir().unwrap();
        let root = portable(dir.path());
        let archive = dir.path().join("r.zip");
        release_zip(&archive, "9.0.0", &[]);
        install(&archive, "zip", &root, "9.0.0", &Cancel::new()).unwrap();
        assert_eq!(
            std::fs::read(root.join("assets/notes.txt")).unwrap(),
            b"new"
        );
        assert!(!root.join("old.txt").exists());
        assert!(dir.path().join("Nikomiru.previous/old.txt").exists());
        assert!(!dir.path().join("Nikomiru.update").exists());
    }

    // T-UPD-004: a Linux release archive (.tar.xz) in its top folder.
    #[cfg(unix)]
    #[test]
    fn a_tar_xz_release_is_swapped_in() {
        let dir = tempfile::tempdir().unwrap();
        let root = portable(dir.path());
        let archive = dir.path().join("r.tar.xz");
        let xz = lzma_rust2::XzWriter::new(
            std::fs::File::create(&archive).unwrap(),
            lzma_rust2::XzOptions::with_preset(6),
        )
        .unwrap();
        let mut tar = tar::Builder::new(xz);
        let program = b"#!/bin/sh\necho 'nikomiru 9.0.0'\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(program.len() as u64);
        header.set_mode(0o755);
        tar.append_data(&mut header, "nikomiru-9.0.0/nikomiru", &program[..])
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o644);
        tar.append_data(&mut header, "nikomiru-9.0.0/LICENSE", &b"GPL"[..])
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        install(&archive, "tar.xz", &root, "9.0.0", &Cancel::new()).unwrap();
        assert_eq!(std::fs::read(root.join("LICENSE")).unwrap(), b"GPL");
        assert!(dir.path().join("Nikomiru.previous/old.txt").exists());
    }

    // T-UPD-004: published archives keep everything in one top folder.
    #[cfg(unix)]
    #[test]
    fn a_release_in_its_top_folder_is_swapped_in() {
        let dir = tempfile::tempdir().unwrap();
        let root = portable(dir.path());
        let archive = dir.path().join("r.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        let exec = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.add_directory("Nikomiru-9.0.0/", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.start_file("Nikomiru-9.0.0/nikomiru", exec).unwrap();
        zip.write_all(b"#!/bin/sh\necho 'nikomiru 9.0.0'\n")
            .unwrap();
        zip.start_file(
            "Nikomiru-9.0.0/LICENSE",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"GPL").unwrap();
        zip.finish().unwrap();
        install(&archive, "zip", &root, "9.0.0", &Cancel::new()).unwrap();
        assert_eq!(std::fs::read(root.join("LICENSE")).unwrap(), b"GPL");
        assert!(!root.join("Nikomiru-9.0.0").exists());
        assert!(dir.path().join("Nikomiru.previous/old.txt").exists());
        assert!(!dir.path().join("Nikomiru.unpack").exists());
        assert!(!dir.path().join("Nikomiru.update").exists());
    }

    // T-UPD-004: a program that is not the release, unsafe names or links change nothing.
    #[cfg(unix)]
    #[test]
    fn failed_installs_leave_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = portable(dir.path());
        let archive = dir.path().join("r.zip");
        release_zip(&archive, "8.0.0", &[]);
        assert!(install(&archive, "zip", &root, "9.0.0", &Cancel::new()).is_err());
        release_zip(&archive, "9.0.0", &[("../escape", 0o644)]);
        assert!(install(&archive, "zip", &root, "9.0.0", &Cancel::new()).is_err());
        release_zip(&archive, "9.0.0", &[("link", 0o120777)]);
        assert!(install(&archive, "zip", &root, "9.0.0", &Cancel::new()).is_err());
        assert!(root.join("old.txt").exists(), "untouched");
        assert!(!dir.path().join("escape").exists());
        assert!(!dir.path().join("Nikomiru.update").exists());
    }

    // T-UPD-003: package-owned installs are never replaced.
    #[test]
    fn package_installs_are_not_portable() {
        assert_eq!(
            ownership_of(Path::new("/usr/bin/nikomiru"), false),
            Ownership::Package
        );
        assert_eq!(
            ownership_of(Path::new("/app/bin/nikomiru"), false),
            Ownership::Package
        );
        let dir = tempfile::tempdir().unwrap();
        let root = portable(dir.path());
        assert_eq!(
            ownership_of(&root.join("nikomiru"), true),
            Ownership::Flatpak
        );
        assert_eq!(
            ownership_of(&root.join("nikomiru"), false),
            Ownership::Portable(root.clone())
        );
    }
}
