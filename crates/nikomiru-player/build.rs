// SPDX-License-Identifier: GPL-3.0-only

//! Builds the Qt Quick video item (C++, ADR 0003) and exports it to the crates
//! that depend on nikomiru-player.

use cxx_qt_build::CxxQtBuilder;

fn main() {
    let mpv_includes = pkg_config("--cflags-only-I", "-I");
    let builder = CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("OpenGL")
        .crate_include_root(Some("cpp".into()))
        .file("src/bridge.rs")
        .cpp_file("cpp/videoitem.h")
        .cpp_file("cpp/videoitem.cpp");
    // SAFETY: the callback only adds libmpv's include directories.
    let builder = unsafe {
        builder.cc_builder(move |cc| {
            for dir in &mpv_includes {
                cc.include(dir);
            }
        })
    };
    builder.build().export();
}

/// Values of a pkg-config flag for libmpv, without the prefix.
fn pkg_config(flag: &str, prefix: &str) -> Vec<String> {
    std::process::Command::new("pkg-config")
        .args([flag, "mpv"])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .filter_map(|t| t.strip_prefix(prefix).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
