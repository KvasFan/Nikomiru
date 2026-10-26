// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask flatpak-sources` (D-5): the crates of `Cargo.lock` as
//! flatpak-builder sources, so that the Flatpak builds offline. Each crate is
//! an archive from crates.io checked by the checksum of the lock file,
//! unpacked in `cargo/vendor`, with the `.cargo-checksum.json` that cargo
//! wants of a vendored crate; then the `cargo/config.toml` that points cargo
//! there. Next to them, `THIRD_PARTY_LICENSES.html` for Settings → About:
//! the SDK has no `cargo about`. `cargo xtask ci` checks that both still
//! match `Cargo.lock`.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// Where the sources go, next to the manifest, and the licenses.
const OUT: &str = "packaging/flatpak/cargo-sources.json";
const LICENSES: &str = "packaging/flatpak/THIRD_PARTY_LICENSES.html";

/// The sources of the crates of `lock`, the text of a `Cargo.lock`.
pub fn sources(lock: &str) -> Result<Vec<Value>> {
    let lock: toml::Table = toml::from_str(lock).context("Cargo.lock")?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .context("Cargo.lock has no packages")?;
    let mut out = Vec::new();
    for p in packages {
        let text = |k: &str| p.get(k).and_then(toml::Value::as_str);
        // the workspace's own crates have no source
        let (Some(name), Some(version), Some(source), Some(sum)) = (
            text("name"),
            text("version"),
            text("source"),
            text("checksum"),
        ) else {
            continue;
        };
        if !source.starts_with("registry+") {
            anyhow::bail!("{name} {version} comes from {source}: only crates.io is vendored");
        }
        let dest = format!("cargo/vendor/{name}-{version}");
        out.push(json!({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": format!("https://static.crates.io/crates/{name}/{name}-{version}.crate"),
            "sha256": sum,
            "dest": dest,
        }));
        out.push(json!({
            "type": "inline",
            "contents": json!({ "package": sum, "files": {} }).to_string(),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        }));
    }
    out.push(json!({
        "type": "inline",
        "contents": "[source.vendored-sources]\ndirectory = \"cargo/vendor\"\n\n\
                     [source.crates-io]\nreplace-with = \"vendored-sources\"\n",
        "dest": "cargo",
        "dest-filename": "config.toml",
    }));
    Ok(out)
}

/// The manifest, its app id, and where the build goes.
const MANIFEST: &str = "packaging/flatpak/io.github.kvasfan.Nikomiru.yaml";
const APP_ID: &str = "io.github.kvasfan.Nikomiru";

/// `manifest` with Nikomiru's source replaced by the repository at `root`, at
/// `commit`: the lines between the two `nikomiru source` comments.
pub fn local_manifest(manifest: &str, root: &Path, commit: &str) -> Result<String> {
    let start = manifest
        .find("      # nikomiru source")
        .context("the manifest has no `# nikomiru source` line")?;
    let end = manifest
        .find("      # end of nikomiru source")
        .context("the manifest has no `# end of nikomiru source` line")?;
    Ok(format!(
        "{}      - type: git\n        path: {}\n        commit: {commit}\n{}",
        &manifest[..start],
        root.display(),
        &manifest[end..]
    ))
}

/// `cargo xtask flatpak`: the Flatpak of the last commit, built with
/// `org.flatpak.Builder` into `target/flatpak`, and its bundle
/// `target/nikomiru.flatpak`.
pub fn build(root: &Path) -> Result<()> {
    run(root)?;
    let commit = crate::util::output(
        std::process::Command::new("git")
            .args(["-C"])
            .arg(root)
            .args(["rev-parse", "HEAD"]),
    )?;
    let dir = root.join("target/flatpak");
    std::fs::create_dir_all(&dir)?;
    let manifest = std::fs::read_to_string(root.join(MANIFEST))?;
    let local = dir.join(format!("{APP_ID}.yaml"));
    std::fs::write(&local, local_manifest(&manifest, root, commit.trim())?)?;
    std::fs::copy(root.join(OUT), dir.join("cargo-sources.json"))?;
    std::fs::copy(root.join(LICENSES), dir.join("THIRD_PARTY_LICENSES.html"))?;
    let repo = dir.join("repo");
    crate::util::run(
        std::process::Command::new("flatpak")
            .args([
                "run",
                "org.flatpak.Builder",
                "--user",
                "--force-clean",
                "--disable-rofiles-fuse",
            ])
            .arg(format!("--state-dir={}", dir.join("state").display()))
            .arg(format!("--repo={}", repo.display()))
            .arg(dir.join("build"))
            .arg(&local),
    )?;
    let bundle = root.join("target/nikomiru.flatpak");
    crate::util::run(
        std::process::Command::new("flatpak")
            .args([
                "build-bundle",
                "--runtime-repo=https://flathub.org/repo/flathub.flatpakrepo",
            ])
            .arg(&repo)
            .arg(&bundle)
            .arg(APP_ID),
    )?;
    eprintln!("built {}", bundle.display());
    Ok(())
}

/// The text of [`OUT`] for the workspace's `Cargo.lock`, and its crates.
fn sources_text(root: &Path) -> Result<(String, usize)> {
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).context("reading Cargo.lock")?;
    let sources = sources(&lock)?;
    Ok((
        serde_json::to_string_pretty(&sources)? + "\n",
        (sources.len() - 1) / 2,
    ))
}

/// Writes [`OUT`] from the workspace's `Cargo.lock`, and [`LICENSES`].
pub fn run(root: &Path) -> Result<()> {
    let (text, crates) = sources_text(root)?;
    let out = root.join(OUT);
    std::fs::write(&out, text).with_context(|| format!("writing {}", out.display()))?;
    eprintln!("{crates} crates in {OUT}");
    let html = crate::licenses::about(root)?;
    std::fs::copy(&html, root.join(LICENSES)).with_context(|| format!("writing {LICENSES}"))?;
    eprintln!("wrote {LICENSES}");
    Ok(())
}

/// [`OUT`] matches `Cargo.lock`: a dependency changed without
/// `cargo xtask flatpak-sources` would break the offline Flatpak build.
pub fn check(root: &Path) -> Result<()> {
    let (text, _) = sources_text(root)?;
    let written = std::fs::read_to_string(root.join(OUT)).unwrap_or_default();
    if text != written {
        anyhow::bail!("{OUT} does not match Cargo.lock: run `cargo xtask flatpak-sources`");
    }
    eprintln!("ok: {OUT} matches Cargo.lock");
    Ok(())
}

/// [`LICENSES`] is the one generated now at `generated`.
pub fn check_licenses(root: &Path, generated: &Path) -> Result<()> {
    let now = std::fs::read(generated).with_context(|| generated.display().to_string())?;
    if std::fs::read(root.join(LICENSES)).unwrap_or_default() != now {
        anyhow::bail!("{LICENSES} is out of date: run `cargo xtask flatpak-sources`");
    }
    eprintln!("ok: {LICENSES} is up to date");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crate_becomes_an_archive_and_its_checksum() {
        let lock = r#"
version = 4

[[package]]
name = "nikomiru-core"
version = "0.4.0"

[[package]]
name = "toml"
version = "1.1.8"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"
"#;
        let s = sources(lock).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[0]["url"],
            "https://static.crates.io/crates/toml/toml-1.1.8.crate"
        );
        assert_eq!(s[0]["sha256"], "abc");
        assert_eq!(s[1]["dest-filename"], ".cargo-checksum.json");
        assert_eq!(s[2]["dest-filename"], "config.toml");
        let git = lock.replace(
            "registry+https://github.com/rust-lang/crates.io-index",
            "git+https://x",
        );
        assert!(sources(&git).is_err());
    }

    #[test]
    fn the_local_build_takes_the_working_tree() {
        let manifest = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(MANIFEST),
        )
        .unwrap();
        let local = local_manifest(&manifest, Path::new("/src/nikomiru"), "abc123").unwrap();
        assert!(
            local.contains(
                "      - type: git\n        path: /src/nikomiru\n        commit: abc123\n"
            )
        );
        assert!(!local.contains("url: https://github.com/KvasFan/nikomiru.git"));
        assert!(local.contains("      - cargo-sources.json"));
    }
}
