// SPDX-License-Identifier: GPL-3.0-only

//! Release files made on this machine, without a CI service:
//!
//! - `cargo xtask linux`: the optimized program for Linux in
//!   `target/dist/`, as `.tar.gz` and as `.tar.xz` (the kind the signed
//!   updates install, UPD-001), named after the glibc and OpenSSL it needs;
//! - `cargo xtask update-manifest`: a channel's update manifest for release
//!   archives (UPD-001), written for the maintainer to sign with minisign.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::util;

const REPOSITORY: &str = "https://github.com/KvasFan/Nikomiru";

fn version(root: &Path) -> Result<String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let manifest: toml::Table = toml::from_str(&text)?;
    manifest["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_string)
        .context("no workspace version")
}

/// The newest `GLIBC_x.y` a program asks for, from `objdump -T`.
fn glibc_of(symbols: &str) -> Option<String> {
    symbols
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .filter_map(|w| w.strip_prefix("GLIBC_"))
        .filter_map(|v| {
            let parts: Vec<u32> = v
                .split('.')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            Some((parts, v.to_string()))
        })
        .max()
        .map(|(_, v)| v)
}

/// The major version of `libssl.so.N` among a program's needed libraries,
/// from `readelf -d`.
fn openssl_of(dynamic: &str) -> Option<String> {
    dynamic
        .lines()
        .filter(|l| l.contains("(NEEDED)"))
        .find_map(|l| l.split("libssl.so.").nth(1))
        .map(|rest| rest.chars().take_while(char::is_ascii_digit).collect())
}

fn readme(version: &str, glibc: &str, openssl: &str) -> String {
    format!(
        "Nikomiru {version} — native Linux x86_64\n\
         \n\
         Run ./nikomiru --version to verify the executable.\n\
         \n\
         This native build requires glibc {glibc}, OpenSSL {openssl} (libssl.so.{openssl}/libcrypto.so.{openssl}),\n\
         libmpv.so.2, the desktop/OpenGL libraries documented by the matching source,\n\
         and FFmpeg/ffprobe for extraction and media exports. yt-dlp is optional and\n\
         is supplied by the user. These external programs/libraries are not bundled\n\
         in this native archive.\n\
         \n\
         The folder can be kept anywhere you can write: Settings → About can then\n\
         install signed updates into it.\n\
         \n\
         LICENSE and THIRD_PARTY_LICENSES.html cover the application and Rust graph.\n\
         Sources and validation: {REPOSITORY}/releases/tag/v{version}\n"
    )
}

/// `cargo xtask linux`.
pub fn linux(root: &Path) -> Result<()> {
    for tool in ["objdump", "readelf", "tar", "xz"] {
        if !util::have_program(tool) {
            bail!("{tool} is needed");
        }
    }
    util::run(
        util::cargo()
            .env("RUSTFLAGS", util::release_rustflags(root))
            .args(["build", "--release", "--locked", "-p", "nikomiru-app"]),
    )?;
    let program = root.join("target/release/nikomiru");
    let version = version(root)?;
    let says = util::output(std::process::Command::new(&program).arg("--version"))?;
    if says.trim() != format!("nikomiru {version}") {
        bail!("the program says {:?}, not nikomiru {version}", says.trim());
    }
    let glibc = glibc_of(&util::output(
        std::process::Command::new("objdump")
            .arg("-T")
            .arg(&program),
    )?)
    .context("no glibc version in the program")?;
    let openssl = openssl_of(&util::output(
        std::process::Command::new("readelf")
            .arg("-d")
            .arg(&program),
    )?)
    .context("the program does not link libssl")?;
    let licenses = crate::licenses::about(root)?;
    let dist = root.join("target/dist");
    let folder = format!("nikomiru-{version}");
    let stage = dist.join("stage");
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    let inside = stage.join(&folder);
    std::fs::create_dir_all(&inside)?;
    std::fs::copy(&program, inside.join("nikomiru"))?;
    std::fs::copy(root.join("LICENSE"), inside.join("LICENSE"))?;
    std::fs::copy(&licenses, inside.join("THIRD_PARTY_LICENSES.html"))?;
    std::fs::write(
        inside.join("README.txt"),
        readme(&version, &glibc, &openssl),
    )?;
    let name = format!("nikomiru-{version}-linux-x86_64-glibc{glibc}-openssl{openssl}");
    for (flag, ext) in [("-z", "tar.gz"), ("-J", "tar.xz")] {
        let out = dist.join(format!("{name}.{ext}"));
        util::run(
            std::process::Command::new("tar")
                .arg("-C")
                .arg(&stage)
                .args(["--owner=0", "--group=0", "--sort=name", flag, "-cf"])
                .arg(&out)
                .arg(&folder),
        )?;
        eprintln!("wrote {}", out.display());
    }
    std::fs::remove_dir_all(&stage)?;
    Ok(())
}

/// An artifact of the manifest, from its file name.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    platform: String,
    kind: &'static str,
    name: String,
}

/// The platform and kind of a release archive's name, or why it has none.
fn entry_of(name: &str) -> Result<Entry> {
    let kind = if name.ends_with(".tar.xz") {
        "tar.xz"
    } else if name.ends_with(".zip") {
        "zip"
    } else {
        bail!("{name}: signed updates install only .tar.xz and .zip archives");
    };
    let platform = [
        "linux-x86_64",
        "linux-aarch64",
        "windows-x86_64",
        "windows-aarch64",
    ]
    .into_iter()
    .find(|p| name.contains(&format!("-{p}")))
    .with_context(|| format!("{name}: no platform in the name"))?;
    Ok(Entry {
        platform: platform.into(),
        kind,
        name: name.into(),
    })
}

/// The manifest's text (`nikomiru-update` version 1).
fn manifest_text(
    channel: &str,
    release: &str,
    published: &str,
    base: &str,
    artifacts: &[(Entry, String, u64)],
) -> Result<String> {
    let mut seen = std::collections::BTreeSet::new();
    for (e, _, _) in artifacts {
        if !seen.insert(&e.platform) {
            bail!("two archives for {}", e.platform);
        }
    }
    let list: Vec<_> = artifacts
        .iter()
        .map(|(e, sha256, bytes)| {
            serde_json::json!({
                "platform": e.platform,
                "kind": e.kind,
                "url": format!("{base}/{}", e.name),
                "sha256": sha256,
                "bytes": bytes,
            })
        })
        .collect();
    let value = serde_json::json!({
        "format": "nikomiru-update",
        "version": 1,
        "channel": channel,
        "release": release,
        "published": published,
        "notes_url": format!("{REPOSITORY}/releases/tag/v{release}"),
        "artifacts": list,
    });
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

const MANIFEST_USAGE: &str =
    "cargo xtask update-manifest [--channel stable|beta] <archive.tar.xz|archive.zip>…";

/// `cargo xtask update-manifest`.
pub fn update_manifest(root: &Path, args: &[String]) -> Result<()> {
    let mut channel = "stable".to_string();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--channel" {
            channel = it.next().context(MANIFEST_USAGE)?.clone();
        } else {
            files.push(PathBuf::from(a));
        }
    }
    if !["stable", "beta"].contains(&channel.as_str()) || files.is_empty() {
        bail!("usage: {MANIFEST_USAGE}");
    }
    let release = version(root)?;
    let base = format!("{REPOSITORY}/releases/download/v{release}");
    let mut artifacts = Vec::new();
    for file in &files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .with_context(|| file.display().to_string())?;
        if !name.contains(&format!("-{release}-")) {
            bail!("{name} is not an archive of {release}");
        }
        let bytes = std::fs::read(file).with_context(|| file.display().to_string())?;
        let sha256: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        artifacts.push((entry_of(name)?, sha256, bytes.len() as u64));
    }
    let published = util::output(std::process::Command::new("date").args(["-u", "+%Y-%m-%d"]))?;
    let text = manifest_text(&channel, &release, published.trim(), &base, &artifacts)?;
    let dist = root.join("target/dist");
    std::fs::create_dir_all(&dist)?;
    let out = dist.join(format!("{channel}.json"));
    std::fs::write(&out, text)?;
    eprintln!(
        "wrote {}\nsign it with:\n  minisign -S -s ~/.minisign/nikomiru.key -m {} -t \"nikomiru {channel} {release}\"\n\
         then upload {channel}.json and {channel}.json.minisig to the release tagged `updates`",
        out.display(),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_needs_of_a_program_are_read() {
        let symbols = "0000 DF *UND* GLIBC_2.34 pthread_create\n\
                       0000 DF *UND* GLIBC_2.44 foo\n0000 DF *UND* GLIBC_2.9 bar\n";
        assert_eq!(glibc_of(symbols).as_deref(), Some("2.44"));
        let dynamic = " 0x01 (NEEDED) Shared library: [libmpv.so.2]\n \
                       0x01 (NEEDED) Shared library: [libssl.so.4]\n";
        assert_eq!(openssl_of(dynamic).as_deref(), Some("4"));
        assert_eq!(
            openssl_of(" 0x01 (NEEDED) Shared library: [libc.so.6]"),
            None
        );
    }

    #[test]
    fn manifests_list_the_archives_the_updater_installs() {
        let linux = entry_of("nikomiru-3.0.0-linux-x86_64-glibc2.44-openssl4.tar.xz").unwrap();
        assert_eq!(
            (linux.platform.as_str(), linux.kind),
            ("linux-x86_64", "tar.xz")
        );
        let windows = entry_of("nikomiru-3.0.0-windows-x86_64.zip").unwrap();
        assert_eq!(
            (windows.platform.as_str(), windows.kind),
            ("windows-x86_64", "zip")
        );
        assert!(entry_of("nikomiru-3.0.0-windows-x86_64.msi").is_err());
        assert!(entry_of("nikomiru-3.0.0-linux-x86_64.tar.gz").is_err());
        let text = manifest_text(
            "stable",
            "3.0.0",
            "2026-11-18",
            "https://example.org/v3.0.0",
            &[(windows, "ab".repeat(32), 10)],
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["format"], "nikomiru-update");
        assert_eq!(v["channel"], "stable");
        assert_eq!(
            v["artifacts"][0]["url"],
            "https://example.org/v3.0.0/nikomiru-3.0.0-windows-x86_64.zip"
        );
        assert_eq!(v["notes_url"], format!("{REPOSITORY}/releases/tag/v3.0.0"));
        let twice = [
            (entry_of("a-linux-x86_64.zip").unwrap(), String::new(), 1),
            (entry_of("b-linux-x86_64.zip").unwrap(), String::new(), 1),
        ];
        assert!(manifest_text("stable", "3.0.0", "d", "u", &twice).is_err());
    }
}
