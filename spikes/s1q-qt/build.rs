// SPDX-License-Identifier: GPL-3.0-only

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let mpv = pkg_config_flags("mpv");
    let builder =
        CxxQtBuilder::new_qml_module(QmlModule::new("Anikoto.Spike").qml_file("qml/main.qml"))
            .qt_module("Quick")
            .qt_module("OpenGL")
            .file("src/probe.rs")
            .cpp_file("cpp/mpvitem.h")
            .cpp_file("cpp/mpvitem.cpp");
    // SAFETY: the callback only adds include paths for libmpv's headers.
    let builder = unsafe {
        builder.cc_builder(move |cc| {
            for dir in &mpv.0 {
                cc.include(dir);
            }
        })
    };
    builder.build();
    for lib in mpv.1 {
        println!("cargo:rustc-link-lib={lib}");
    }
}

/// Include directories and libraries of a pkg-config package.
fn pkg_config_flags(pkg: &str) -> (Vec<String>, Vec<String>) {
    let out = std::process::Command::new("pkg-config")
        .args(["--cflags-only-I", "--libs-only-l", pkg])
        .output()
        .expect("pkg-config");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut inc = Vec::new();
    let mut libs = Vec::new();
    for t in text.split_whitespace() {
        if let Some(i) = t.strip_prefix("-I") {
            inc.push(i.to_string());
        } else if let Some(l) = t.strip_prefix("-l") {
            libs.push(l.to_string());
        }
    }
    (inc, libs)
}
