// SPDX-License-Identifier: GPL-3.0-only

//! L5: the presets Nikomiru saves are read by a real mpv (T-PLY-016), with
//! every value each control offers (T-PLY-018).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nikomiru_player::settings::{CONTROLS, Kind, OptionSet};

/// Every value of every control, spread over as many presets as the longest
/// list needs.
fn presets() -> Vec<OptionSet> {
    let longest = CONTROLS
        .iter()
        .map(|c| match c.kind {
            Kind::Choice(values) => values.len(),
            Kind::Flag | Kind::Integer(..) => 2,
            Kind::Text => 1,
        })
        .max()
        .unwrap();
    (0..longest)
        .map(|i| {
            let mut set = OptionSet::default();
            for c in CONTROLS {
                let value = match c.kind {
                    Kind::Choice(values) => values[i.min(values.len() - 1)].to_string(),
                    Kind::Flag => ["yes", "no"][i % 2].to_string(),
                    Kind::Integer(min, max) => [min, max][i % 2].to_string(),
                    Kind::Text => "auto".to_string(),
                };
                set.values.insert(c.key.to_string(), value);
            }
            set.raw = "video-zoom=0.25".into();
            set
        })
        .collect()
}

// T-PLY-016, T-PLY-018
#[test]
#[ignore = "L5: needs mpv"]
fn mpv_reads_the_saved_presets() {
    let dir = tempfile::tempdir().unwrap();
    for (i, set) in presets().iter().enumerate() {
        let path = dir.path().join(format!("p{i}.conf"));
        std::fs::write(&path, set.to_mpv_conf("Nikomiru test")).unwrap();
        let out = std::process::Command::new("mpv")
            .arg("--no-config")
            .arg(format!("--include={}", path.display()))
            .args([
                "--idle=no",
                "--vo=null",
                "--ao=null",
                "--msg-level=all=warn",
            ])
            .output()
            .unwrap();
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success() && said.trim().is_empty(),
            "{}:\n{}\n{said}",
            path.display(),
            std::fs::read_to_string(&path).unwrap()
        );
    }
}
