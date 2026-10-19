// SPDX-License-Identifier: GPL-3.0-only

//! Spike S1q (ROADMAP M0): the Qt 6 alternative to S1, proposed by the
//! maintainer after the 4K results of ADR 0001. Qt Quick composites mpv's video
//! (QQuickFramebufferObject, rendered on the scene graph's render thread) and a
//! QML overlay on the GPU. Rust drives Qt through cxx-qt.
//!
//! Usage:
//!   spike-s1q-qt <video> [--fullscreen] [--measure SECONDS | --stress CYCLES]
//! Environment: S1Q_MPV_OPTIONS="key=value,…" (extra mpv options),
//! S1Q_STATIC_SUB=1 (frozen subtitle), S1Q_NO_OVERLAY=1 (video only).

mod probe;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

unsafe extern "C" {
    /// Registers `MpvItem` with QML and selects the OpenGL scene graph (cpp/mpvitem.cpp).
    fn s1q_register();
}

fn main() {
    let mut app = QGuiApplication::new();
    // SAFETY: called once on the main thread, after QGuiApplication exists and
    // before any window is created.
    unsafe { s1q_register() };
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Anikoto/Spike/qml/main.qml"));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
