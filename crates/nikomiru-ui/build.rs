// SPDX-License-Identifier: GPL-3.0-only

//! Builds the QML module `Nikomiru.UI` (QML files and Rust QObjects) and exports
//! it to the application crate.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let module = QmlModule::new("Nikomiru.UI").qml_files([
        "qml/Main.qml",
        "qml/ControlBar.qml",
        "qml/Toast.qml",
        "qml/SubtitleOverlay.qml",
        "qml/SubtitleList.qml",
    ]);
    CxxQtBuilder::new_qml_module(module)
        .qt_module("Quick")
        .files(["src/bridge/player.rs"])
        .build()
        .export();
    for f in [
        "../../assets/i18n/en/main.ftl",
        "../../assets/i18n/it/main.ftl",
    ] {
        println!("cargo::rerun-if-changed={f}");
    }
}
