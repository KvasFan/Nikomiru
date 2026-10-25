// SPDX-License-Identifier: GPL-3.0-only

//! `cargo xtask install [--prefix DIR]` (APP-002): the release build of
//! `nikomiru` in `DIR/bin` and its desktop entry in `DIR/share/applications`,
//! registered for `nikomiru://` links, so that the links in Anki cards open
//! Nikomiru. For builds from source, until the packages of 1.0 (D-5).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util;

/// The desktop entry with `bin` as its program: the session's `PATH` may
/// not hold `DIR/bin`.
pub fn installed_entry(entry: &str, bin: &Path) -> String {
    let program = quote(&bin.to_string_lossy());
    entry
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("Exec={program} %U")
            } else if line.starts_with("TryExec=") {
                format!("TryExec={}", bin.display())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// A program path as the desktop entry specification wants it in `Exec`:
/// quoted when it holds a reserved character, with `" ` $ \` escaped; then
/// written as a string value, whose escapes are undone first when the file
/// is read (so a `$` in quotes is `\\$` in the file).
fn quote(path: &str) -> String {
    let reserved = |c: char| c.is_whitespace() || "\"'\\><~|&;$*?#()`".contains(c);
    let quoted = if path.contains(reserved) {
        let mut out = String::from("\"");
        for c in path.chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    } else {
        path.to_string()
    };
    let mut value = String::with_capacity(quoted.len());
    for c in quoted.chars() {
        match c {
            '\\' => value.push_str("\\\\"),
            '\n' => value.push_str("\\n"),
            '\t' => value.push_str("\\t"),
            '\r' => value.push_str("\\r"),
            c => value.push(c),
        }
    }
    value
}

fn prefix(args: &[String]) -> Result<PathBuf> {
    match args {
        [] => {
            let home = std::env::var_os("HOME").context("HOME is not set")?;
            Ok(PathBuf::from(home).join(".local"))
        }
        [flag, dir] if flag == "--prefix" => Ok(PathBuf::from(dir)),
        _ => bail!("usage: cargo xtask install [--prefix DIR]"),
    }
}

/// Runs a desktop tool if it is installed; says so if it is not.
fn optional(program: &str, args: &[&str]) {
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("warning: {program} {} failed ({status})", args.join(" ")),
        Err(_) => eprintln!("note: {program} is not installed: skipped"),
    }
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let prefix = prefix(args)?;
    util::run(util::cargo().args(["build", "--release", "--locked", "-p", "nikomiru-app"]))?;
    let bin_dir = prefix.join("bin");
    let apps = prefix.join("share/applications");
    std::fs::create_dir_all(&bin_dir)?;
    std::fs::create_dir_all(&apps)?;
    let bin = bin_dir.join("nikomiru");
    // a running copy keeps its file: the new one replaces the name
    let staged = bin_dir.join(".nikomiru.new");
    std::fs::copy(root.join("target/release/nikomiru"), &staged)
        .context("copying target/release/nikomiru")?;
    std::fs::rename(&staged, &bin)?;
    let entry = std::fs::read_to_string(root.join("assets/nikomiru.desktop"))?;
    let desktop = apps.join("nikomiru.desktop");
    std::fs::write(&desktop, installed_entry(&entry, &bin))?;
    eprintln!("installed {} and {}", bin.display(), desktop.display());
    optional("update-desktop-database", &[&apps.to_string_lossy()]);
    optional(
        "xdg-mime",
        &["default", "nikomiru.desktop", "x-scheme-handler/nikomiru"],
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_entry_names_the_program() {
        let entry = "[Desktop Entry]\nName=Nikomiru\nExec=nikomiru %U\nTryExec=nikomiru\n";
        assert_eq!(
            installed_entry(entry, Path::new("/home/u/.local/bin/nikomiru")),
            "[Desktop Entry]\nName=Nikomiru\nExec=/home/u/.local/bin/nikomiru %U\nTryExec=/home/u/.local/bin/nikomiru\n"
        );
        assert_eq!(
            installed_entry("Exec=nikomiru %U", Path::new("/home/my user/bin/nikomiru")),
            "Exec=\"/home/my user/bin/nikomiru\" %U\n"
        );
        // the quoting escapes, then the string escapes: `\\$` for `$`, four
        // backslashes for one
        assert_eq!(quote(r#"/a"$b"#), r#""/a\\"\\$b""#);
        assert_eq!(quote(r"/a b\c`"), r#""/a b\\\\c\\`""#);
        assert_eq!(quote("/a\tb"), r#""/a\tb""#);
    }

    #[test]
    fn the_prefix() {
        assert_eq!(
            prefix(&["--prefix".into(), "/opt/n".into()]).unwrap(),
            PathBuf::from("/opt/n")
        );
        assert!(prefix(&["--wat".into()]).is_err());
    }
}
