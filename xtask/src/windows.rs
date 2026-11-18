// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask windows`: Nikomiru for Windows as a ZIP (ADR 0009), built
//! from Linux. The program in release, libmpv and ffmpeg beside it (EXT-001
//! finds them there), the licenses, and `SOURCES.txt`, which names the
//! archives they came from, their SHA-256 and where their source code is, as
//! the GPL of libmpv and ffmpeg asks of whoever passes them on. With
//! `wixl` (msitools) installed, the same files also become an MSI installer
//! for all users under Program Files, with a Start menu shortcut.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::util;

/// The Windows target of ADR 0009.
const TARGET: &str = "x86_64-pc-windows-gnu";

/// The MSI's upgrade code: the same for every release, so that installing a
/// new one replaces the old.
const UPGRADE_CODE: &str = "5C0E7F4B-5D7A-4E61-9C55-0A6C2E3B9F10";

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
        util::release_rustflags(root),
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
    let notes = sources(&version, &libmpv, &ffmpeg);
    zip.start_file(format!("{folder}/SOURCES.txt"), options)?;
    std::io::Write::write_all(&mut zip, notes.as_bytes())?;
    zip.finish()?;
    eprintln!("wrote {}", out.display());
    if util::have_program("wixl") {
        msi(root, &work, &version, &files, &notes)?;
    } else {
        eprintln!("wixl (msitools) is not installed: no MSI");
    }
    Ok(())
}

/// The MSI of the same files, built with `wixl`.
fn msi(
    root: &Path,
    work: &Path,
    version: &str,
    files: &[(PathBuf, &str)],
    notes: &str,
) -> Result<()> {
    let stage = work.join("msi");
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;
    let mut names = Vec::new();
    for (source, name) in files {
        std::fs::copy(source, stage.join(name)).with_context(|| source.display().to_string())?;
        names.push(name.to_string());
    }
    std::fs::write(stage.join("SOURCES.txt"), notes)?;
    names.push("SOURCES.txt".into());
    std::fs::copy(
        root.join("assets/brand/nikomiru.ico"),
        stage.join("nikomiru.ico"),
    )?;
    let wxs = stage.join("nikomiru.wxs");
    std::fs::write(&wxs, wxs_text(version, &names))?;
    let out = work.join(format!("nikomiru-{version}-windows-x86_64.msi"));
    util::run(
        std::process::Command::new("wixl")
            .current_dir(&stage)
            .args(["-a", "x64", "-o"])
            .arg(&out)
            .arg("nikomiru.wxs"),
    )?;
    eprintln!("wrote {}", out.display());
    Ok(())
}

/// The WiX source of the MSI: every file in one folder under Program Files,
/// `nikomiru.exe` with a Start menu shortcut, and a major upgrade, so that a
/// newer release replaces an older one and an older one is refused.
fn wxs_text(version: &str, names: &[String]) -> String {
    let mut components = String::new();
    let mut refs = String::new();
    for (i, name) in names.iter().enumerate() {
        let shortcut = if name == "nikomiru.exe" {
            "\n              <Shortcut Id=\"StartMenu\" Directory=\"ProgramMenuFolder\" Name=\"Nikomiru\" WorkingDirectory=\"INSTALLDIR\" Icon=\"nikomiru.ico\" Advertise=\"yes\"/>\n            "
        } else {
            ""
        };
        components.push_str(&format!(
            "          <Component Id=\"C{i}\" Guid=\"*\" Win64=\"yes\">\n            <File Id=\"F{i}\" Source=\"{name}\" KeyPath=\"yes\">{shortcut}</File>\n          </Component>\n"
        ));
        refs.push_str(&format!("      <ComponentRef Id=\"C{i}\"/>\n"));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name="Nikomiru" Language="1033" Version="{version}" Manufacturer="Nikomiru" UpgradeCode="{UPGRADE_CODE}">
    <Package InstallerVersion="500" Compressed="yes" InstallScope="perMachine" Description="Nikomiru {version}"/>
    <MajorUpgrade DowngradeErrorMessage="A newer Nikomiru is already installed."/>
    <Media Id="1" Cabinet="nikomiru.cab" EmbedCab="yes"/>
    <Icon Id="nikomiru.ico" SourceFile="nikomiru.ico"/>
    <Property Id="ARPPRODUCTICON" Value="nikomiru.ico"/>
    <Property Id="ARPURLINFOABOUT" Value="https://github.com/KvasFan/Nikomiru"/>
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLDIR" Name="Nikomiru">
{components}        </Directory>
      </Directory>
      <Directory Id="ProgramMenuFolder"/>
    </Directory>
    <Feature Id="Main" Level="1">
{refs}    </Feature>
  </Product>
</Wix>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_msi_source_lists_every_file_once() {
        let names: Vec<String> = ["nikomiru.exe", "libmpv-2.dll", "SOURCES.txt"]
            .map(String::from)
            .to_vec();
        let text = wxs_text("2.120.0", &names);
        assert!(text.contains("Version=\"2.120.0\""));
        assert!(text.contains(UPGRADE_CODE));
        for name in &names {
            assert_eq!(text.matches(&format!("Source=\"{name}\"")).count(), 1);
        }
        assert_eq!(text.matches("<Shortcut ").count(), 1);
        assert_eq!(text.matches("<ComponentRef ").count(), names.len());
    }
}
