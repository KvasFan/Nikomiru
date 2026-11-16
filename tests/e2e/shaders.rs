// SPDX-License-Identifier: GPL-3.0-only

//! M118: palettes and shader presets in the actual app.
use super::support::{Desktop, exit_status, fixture};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::time::Duration;

/// An offline pack with the preset shader names: a harmless pass-through shader.
fn test_pack(path: &std::path::Path) -> (String, u64) {
    let shader = b"//!HOOK MAIN\n//!BIND HOOKED\n//!DESC nikomiru test\nvec4 hook() { return HOOKED_tex(HOOKED_pos); }\n";
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let mut names: Vec<&str> = nikomiru_online::shaders::FAST
        .iter()
        .chain(nikomiru_online::shaders::QUALITY.iter())
        .copied()
        .collect();
    names.sort();
    names.dedup();
    for name in names {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(shader).unwrap();
    }
    zip.finish().unwrap();
    let bytes = std::fs::read(path).unwrap();
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (sha, bytes.len() as u64)
}

// T-THM-001, T-THM-003: a named palette saved; a preset sets mpv's shaders live and
// at the next load, Off clears them, and the user's own mpv shaders win.
#[test]
#[ignore = "L6: needs kwin_wayland, dbus-daemon and `cargo xtask fixtures`"]
fn palettes_and_shader_presets_apply_at_once() {
    let mut desk = Desktop::start();
    desk.write_config("[player]\nmpv_options = { ao = \"null\" }\n");
    let pack = desk.path("pack.zip");
    let (sha, bytes) = test_pack(&pack);
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    assert_eq!(h.state()["shaders"]["installed"], false);
    // no preset can apply without the pack
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[player]\nshader_preset = \"anime4k-fast\"\n[general]\npalette = \"oled\"\n"}));
    let shaders = h.cmd(json!({"cmd":"mpv_get","property":"glsl-shaders"}));
    assert!(shaders.as_str().is_none_or(str::is_empty), "{shaders}");
    assert_eq!(h.state()["shaders"]["palette"], "oled");
    h.cmd(json!({"cmd":"shader-pack-local","path":pack,"sha256":sha,"bytes":bytes}));
    assert_eq!(h.state()["shaders"]["installed"], true);
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[player]\nshader_preset = \"anime4k-quality\"\n"}));
    let set = h.cmd(json!({"cmd":"mpv_get","property":"glsl-shaders"}));
    let set = set.as_str().unwrap_or_default().to_string();
    assert!(set.contains("Anime4K_Restore_CNN_VL.glsl"), "{set}");
    assert_eq!(set.matches(".glsl").count(), 6, "{set}");
    // kept at the next file
    h.open(&fixture("av_long.mkv"));
    h.wait("first-frame");
    let again = h.cmd(json!({"cmd":"mpv_get","property":"glsl-shaders"}));
    assert!(
        again
            .as_str()
            .unwrap_or_default()
            .contains("Anime4K_Upscale_CNN_x2_VL.glsl")
    );
    // Off clears them
    h.cmd(json!({"cmd":"settings","tab":"general","save":"[player]\nshader_preset = \"\"\n"}));
    let off = h.cmd(json!({"cmd":"mpv_get","property":"glsl-shaders"}));
    assert!(off.as_str().is_none_or(str::is_empty), "{off}");
    h.cmd(json!({"cmd":"settings","tab":"close"}));
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
    // the user's own mpv shaders win over a preset
    let own = desk.path("own.glsl");
    std::fs::write(
        &own,
        "//!HOOK MAIN\n//!BIND HOOKED\nvec4 hook() { return HOOKED_tex(HOOKED_pos); }\n",
    )
    .unwrap();
    desk.write_config(&format!(
        "[general]\npalette = \"oled\"\n[player]\nshader_preset = \"anime4k-fast\"\nraw_mpv_options = \"glsl-shaders={}\"\nmpv_options = {{ ao = \"null\" }}\n",
        own.display()
    ));
    let (mut app, mut h) = desk.launch();
    h.open(&fixture("av_basic.mkv"));
    h.wait("first-frame");
    let state = h.state();
    assert_eq!(state["shaders"]["overridden"], true);
    assert_eq!(state["shaders"]["palette"], "oled", "kept");
    let own_set = h.cmd(json!({"cmd":"mpv_get","property":"glsl-shaders"}));
    let own_set = own_set.as_str().unwrap_or_default();
    assert!(
        own_set.contains("own.glsl") && !own_set.contains("Anime4K"),
        "{own_set}"
    );
    h.quit();
    assert!(
        exit_status(&mut app, Duration::from_secs(15)).success(),
        "{}",
        desk.log()
    );
}
