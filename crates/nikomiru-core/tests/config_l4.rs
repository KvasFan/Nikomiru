// SPDX-License-Identifier: GPL-3.0-only

//! L4: `config.toml` written by a process killed in the middle of its saves.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use nikomiru_core::Config;
use nikomiru_core::config::General;

const NAME: &str = "a_kill_during_a_save_leaves_a_whole_file";

// T-SET-001: the write is atomic
#[test]
#[ignore = "L4: kills a process of its own"]
fn a_kill_during_a_save_leaves_a_whole_file() {
    // the child: saves for ever, a large file each time
    if let Some(path) = std::env::var_os("NIKOMIRU_SAVE_LOOP") {
        let path = PathBuf::from(path);
        let mut c = Config::default();
        c.anki.tags = vec!["x".repeat(4096); 256];
        let mut i: u16 = 0;
        loop {
            i = (i + 1) % 100;
            c.general.ui_scale = 75 + i;
            c.save(&path).unwrap();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save(&path).unwrap();
    for wait in [40, 90, 150, 230, 310] {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--include-ignored", "--nocapture"])
            .env("NIKOMIRU_SAVE_LOOP", &path)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(wait));
        child.kill().unwrap(); // SIGKILL
        child.wait().unwrap();
        let (c, invalid) = Config::load(&path).unwrap();
        assert!(invalid.is_empty(), "{invalid:?}");
        assert!(General::SCALES.contains(&c.general.ui_scale));
    }
}
