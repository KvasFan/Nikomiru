// SPDX-License-Identifier: GPL-3.0-only

//! L4: discovery of ffmpeg and ffprobe with fake executables in temporary
//! folders (EXT-001).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use nikomiru_media::tools::{AudioFormat, Origin, Search, ToolsError, probe};

const ALL_ENCODERS: &str =
    "aac libopus libmp3lame mjpeg libwebp libwebp_anim libaom-av1 libvpx-vp9";

/// A fake `ffmpeg` (and `ffprobe`) in `dir` that reports `version` and `encoders`.
fn fake(dir: &Path, version: &str, encoders: &str, with_ffprobe: bool) {
    std::fs::create_dir_all(dir).unwrap();
    let table: String = encoders
        .split_whitespace()
        .map(|e| format!(" A....D {e:<20} fake {e}\n"))
        .collect();
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in\n\
         *-version*) echo 'ffmpeg version {version} Copyright (c) fake' ;;\n\
         *-encoders*) printf 'Encoders:\\n A..... = Audio\\n ------\\n%s' '{table}' ;;\n\
         esac\n"
    );
    let write = |name: &str| {
        let p = dir.join(name);
        std::fs::write(&p, &script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    write("ffmpeg");
    if with_ffprobe {
        write("ffprobe");
    }
}

fn search(setting: &Path, env: Option<&Path>, path: &[&Path]) -> Search {
    Search {
        setting: setting.to_string_lossy().into_owned(),
        env: env.map(OsString::from),
        path: Some(std::env::join_paths(path).unwrap()),
    }
}

// T-EXT-001
#[test]
fn the_setting_wins_then_the_variable_then_path() {
    let root = tempfile::tempdir().unwrap();
    let (set, env, bin) = (
        root.path().join("set"),
        root.path().join("env"),
        root.path().join("bin"),
    );
    fake(&set, "9.0.1-setting", ALL_ENCODERS, true);
    fake(&env, "9.0.2-env", ALL_ENCODERS, true);
    fake(&bin, "9.0.3-path", ALL_ENCODERS, true);
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();

    // the setting may name the executable or its folder
    let t = probe(&search(&set.join("ffmpeg"), Some(&env), &[&bin])).unwrap();
    assert_eq!(
        (t.origin, t.version.as_str()),
        (Origin::Setting, "9.0.1-setting")
    );
    assert_eq!(t.ffprobe, set.join("ffprobe"));
    let t = probe(&search(&set, Some(&env), &[&bin])).unwrap();
    assert_eq!(t.origin, Origin::Setting);

    let t = probe(&search(Path::new(""), Some(&env), &[&bin])).unwrap();
    assert_eq!(
        (t.origin, t.version.as_str()),
        (Origin::Environment, "9.0.2-env")
    );

    // a configured folder without ffmpeg is skipped
    let t = probe(&search(&empty, Some(&empty), &[&empty, &bin])).unwrap();
    assert_eq!((t.origin, t.version.as_str()), (Origin::Path, "9.0.3-path"));
    assert!(t.missing_encoders().is_empty());

    assert!(matches!(
        probe(&search(Path::new(""), None, &[&empty])),
        Err(ToolsError::NotFound)
    ));
}

// T-EXT-001
#[test]
fn without_aac_the_audio_falls_back_to_opus() {
    let root = tempfile::tempdir().unwrap();
    fake(root.path(), "9.0.2", "libopus libmp3lame mjpeg", true);
    let t = probe(&search(root.path(), None, &[])).unwrap();
    assert!(!t.has_encoder("aac"));
    assert_eq!(
        t.audio_format(AudioFormat::AacM4a),
        Some(AudioFormat::OpusOgg)
    );
    let missing: Vec<_> = t.missing_encoders().into_iter().map(|m| m.format).collect();
    assert_eq!(
        missing,
        ["aac-m4a", "webp", "webp-animated", "avif-animated", "webm"]
    );
}

#[test]
fn old_versions_and_a_missing_ffprobe_are_errors() {
    let root = tempfile::tempdir().unwrap();
    let (old, lone) = (root.path().join("old"), root.path().join("lone"));
    fake(&old, "5.1.6-0+deb12u1", ALL_ENCODERS, true);
    fake(&lone, "9.0.2", ALL_ENCODERS, false);
    assert!(matches!(
        probe(&search(&old, None, &[])),
        Err(ToolsError::TooOld { version }) if version == "5.1.6-0+deb12u1"
    ));
    assert!(matches!(
        probe(&search(&lone, None, &[])),
        Err(ToolsError::NoFfprobe { .. })
    ));
}

#[test]
fn the_real_ffmpeg_is_found_in_path() {
    // the fixtures already need a system ffmpeg (TEST §4)
    let t = probe(&Search {
        setting: String::new(),
        env: None,
        path: std::env::var_os("PATH"),
    })
    .unwrap();
    assert!(
        t.version
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == 'n' || c == 'N')
    );
    assert!(t.has_encoder("aac"), "{:?}", t.encoders);
}
