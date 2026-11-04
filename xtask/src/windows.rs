// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask windows`: Nikomiru for Windows as a ZIP (ADR 0009), built
//! from Linux. The program in release, libmpv and ffmpeg beside it (EXT-001
//! finds them there), the licenses, and `SOURCES.txt`, which names the
//! archives they came from, their SHA-256 and where their source code is, as
//! the GPL of libmpv and ffmpeg asks of whoever passes them on.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::util;

/// The Windows target of ADR 0009.
const TARGET: &str = "x86_64-pc-windows-gnu";

const USAGE: &str =
    "cargo xtask windows --libmpv <mpv-dev-x86_64-….7z> --ffmpeg <ffmpeg-…-win64-gpl….zip>";

/// An archive given on the command line, unpacked into `dir`.
struct Input {
    archive: PathBuf,
    sha256: String,
    dir: PathBuf,
}

fn unpack(archive: &Path, dir: &Path) -> Result<Input> {
    let bytes = std::fs::read(archive).with_context(|| archive.display().to_string())?;
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::create_dir_all(dir)?;
    util::run(
        std::process::Command::new("bsdtar")
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(dir),
    )?;
    Ok(Input {
        archive: archive.to_path_buf(),
        sha256,
        dir: dir.to_path_buf(),
    })
}

/// The first file called `name` under `dir`.
fn find(dir: &Path, name: &str) -> Result<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                return Ok(path);
            }
        }
    }
    bail!("{name} is not in {}", dir.display())
}

fn version(root: &Path) -> Result<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let manifest: toml::Table = toml::from_str(&text)?;
    manifest["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_string)
        .context("no workspace version")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sources(version: &str, libmpv: &Input, ffmpeg: &Input) -> String {
    format!(
        "Nikomiru {version} for Windows\n\
         \n\
         Nikomiru is free software under the GNU GPL version 3 (LICENSE).\n\
         Its source code: https://github.com/KvasFan/nikomiru/tree/v{version}\n\
         \n\
         libmpv-2.dll is mpv's library, built by mpv-winbuild-cmake, under the\n\
         GNU GPL (it holds FFmpeg built with GPL parts). It comes from\n\
         {} (SHA-256 {}).\n\
         The sources: https://github.com/shinchiro/mpv-winbuild-cmake/releases,\n\
         the release of the same date, which names the commit of mpv and of\n\
         every library it builds (https://github.com/mpv-player/mpv).\n\
         \n\
         ffmpeg.exe and ffprobe.exe are FFmpeg, built by FFmpeg-Builds, under\n\
         the GNU GPL. They come from\n\
         {} (SHA-256 {}).\n\
         The sources: https://github.com/BtbN/FFmpeg-Builds/releases, the\n\
         release of the same name, which names the commit of FFmpeg and of\n\
         every library it builds (https://ffmpeg.org/download.html).\n",
        file_name(&libmpv.archive),
        libmpv.sha256,
        file_name(&ffmpeg.archive),
        ffmpeg.sha256,
    )
}

/// `cargo xtask windows --libmpv <archive> --ffmpeg <archive>`.
pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let (Some(libmpv), Some(ffmpeg)) = (value("--libmpv"), value("--ffmpeg")) else {
        bail!("usage: {USAGE}");
    };
    if !util::have_program("bsdtar") {
        bail!("bsdtar (libarchive) is needed to unpack the archives");
    }
    let work = root.join("target/windows");
    let libmpv = unpack(&libmpv, &work.join("libmpv"))?;
    let ffmpeg = unpack(&ffmpeg, &work.join("ffmpeg"))?;
    let import = find(&libmpv.dir, "libmpv.dll.a")?;
    let link_dir = import.parent().context("libmpv.dll.a has no folder")?;
    let rustflags = format!(
        "{} -L native={}",
        std::env::var("RUSTFLAGS").unwrap_or_default(),
        link_dir.display()
    );
    util::run(util::cargo().env("RUSTFLAGS", rustflags.trim()).args([
        "build",
        "--release",
        "--locked",
        "-p",
        "nikomiru-app",
        "--target",
        TARGET,
        "--target-dir",
        "target/win",
    ]))?;
    let licenses = crate::licenses::about(root)?;
    let version = version(root)?;
    let folder = format!("Nikomiru-{version}");
    let files = [
        (
            root.join("target/win")
                .join(TARGET)
                .join("release/nikomiru.exe"),
            "nikomiru.exe",
        ),
        (find(&libmpv.dir, "libmpv-2.dll")?, "libmpv-2.dll"),
        (find(&ffmpeg.dir, "ffmpeg.exe")?, "ffmpeg.exe"),
        (find(&ffmpeg.dir, "ffprobe.exe")?, "ffprobe.exe"),
        (licenses, "THIRD_PARTY_LICENSES.html"),
        (root.join("LICENSE"), "LICENSE.txt"),
        (root.join("README.md"), "README.md"),
        (root.join("CHANGELOG.md"), "CHANGELOG.md"),
    ];
    let out = work.join(format!("nikomiru-{version}-windows-x86_64.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&out)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    for (source, name) in &files {
        zip.start_file(format!("{folder}/{name}"), options)?;
        let mut file = std::fs::File::open(source).with_context(|| source.display().to_string())?;
        std::io::copy(&mut file, &mut zip)?;
    }
    zip.start_file(format!("{folder}/SOURCES.txt"), options)?;
    std::io::Write::write_all(&mut zip, sources(&version, &libmpv, &ffmpeg).as_bytes())?;
    zip.finish()?;
    eprintln!("wrote {}", out.display());
    Ok(())
}
