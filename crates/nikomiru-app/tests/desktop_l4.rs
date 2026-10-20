// SPDX-License-Identifier: GPL-3.0-only

//! L4: the desktop entry (T-APP-002): it opens `nikomiru://` links and the
//! video types, and `desktop-file-validate` accepts it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

fn entry() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/nikomiru.desktop"),
    )
    .unwrap()
}

fn key<'a>(text: &'a str, name: &str) -> &'a str {
    text.lines()
        .find_map(|l| l.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_default()
}

// T-APP-002
#[test]
fn links_and_videos_open_with_nikomiru() {
    let text = entry();
    assert_eq!(key(&text, "Exec"), "nikomiru %U");
    let types: Vec<&str> = key(&text, "MimeType")
        .split(';')
        .filter(|t| !t.is_empty())
        .collect();
    assert!(types.contains(&"x-scheme-handler/nikomiru"), "{types:?}");
    for video in [
        "video/mp4",
        "video/x-matroska",
        "video/webm",
        "video/x-msvideo",
    ] {
        assert!(types.contains(&video), "{video}");
    }
    assert_eq!(
        key(&text, "StartupWMClass"),
        "nikomiru",
        "the app id of the window"
    );
}

// T-APP-002
#[test]
#[ignore = "L4: needs desktop-file-validate (desktop-file-utils)"]
fn desktop_file_validate_accepts_it() {
    let out = std::process::Command::new("desktop-file-validate")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/nikomiru.desktop"))
        .output()
        .expect("desktop-file-validate");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success() && text.trim().is_empty(), "{text}");
}
