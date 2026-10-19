# ADR 0003 — UI toolkit: Qt 6 Quick instead of Dioxus

| Field | Value |
|---|---|
| Status | **Proposed** — awaiting the check on the developer's real session and the maintainer's approval |
| Date | 2026-10-19 |
| Requirements | NFR-PERF-03, SPECIFICATION §1.1, §1.2 (objective 1: "the quality of mpv"), §4, §5 |
| Supersedes (if accepted) | The UI part of SPECIFICATION §1.1/§4/§5 and the composition of [ADR 0001](0001-video-composition.md) |
| Spike | S1q — code in [`spikes/s1q-qt/`](../../spikes/s1q-qt/) |

## Context

The revision of ADR 0001 showed that the Dioxus design (GtkGLArea under a transparent WebKitWebView in a GtkOverlay) recomposes the whole window on the GTK main thread for every video frame. At 4K under CPU load it falls to 14 renders/s and 912 dropped frames in 20 s, while mpv alone drops none. The maintainer proposed Qt 6. Qt Quick renders its scene graph on a dedicated render thread and composites every item on the GPU; KDE's mpv player Haruna is built on it.

## What the spike does

- **Rust + Qt through [cxx-qt](https://github.com/KDAB/cxx-qt) 0.10** (MIT OR Apache-2.0): `QGuiApplication` and `QQmlApplicationEngine` from Rust, a Rust `QObject` (`Probe`) exposed to QML for the measurements and for splitting text into graphemes (`unicode-segmentation`).
- **`MpvItem`**: a `QQuickFramebufferObject` in C++ (about 250 lines, written for the spike after the pattern of mpv's Qt Quick example and KDE's MpvQt). mpv renders into the item's FBO through the libmpv render API, **on Qt Quick's render thread**. The update callback only schedules `update()`; events arrive through the wakeup callback on the GUI thread, and `time-pos` is throttled to 30 Hz (ARC-004). The native Wayland or X11 display is passed for zero-copy hwdec.
- **QML overlay** identical to S1: a control bar with buttons and click/key counters, and a fake subtitle in which every grapheme is a clickable `Text` that changes 30 times per second.
- Same measurements as S1: dropped frames, renders and new frames per second, `render()` CPU time, process CPU, the GPU of every DRM descriptor, RSS, 50 open/close cycles (through a `Loader`).

## Results

Virtual KWin, RX 7900 XTX, **under the same heavy CPU load as the ADR 0001 revision** (load average 13–35 on 16 threads, unrelated `cargo test --release` builds at nice −4).

| Configuration | Output | Duration | Renders/s | Dropped | Process CPU | hwdec |
|---|---|---|---|---|---|---|
| mpv alone (`vo=gpu-next`), for reference | 3840×2160 | 22 s | 60 | 0 | — | vulkan |
| S1 (Dioxus/GTK), for reference | 3840×2160 | 20 s | 14.4 | **912** | 31% (+ WebKit) | vaapi |
| **S1q, Wayland, H.264** | 3840×2160 | 20 s | **60.0** | **2** (first second) | **13.8%** | vaapi |
| **S1q, Wayland, HEVC 10-bit** | 3840×2160 | 60 s | **60.0** | **1** | **15%** | vaapi |
| S1q, X11 (XWayland, GLX), HEVC 10-bit / H.264 | 3840×2160 | 60 s | 59.9 / 60.0 | 2 / 2 | 19% / 16% | vulkan-copy |
| **S1q, X11 with `QT_XCB_GL_INTEGRATION=xcb_egl`**, HEVC 10-bit | 3840×2160 | 30 s | **59.9** | **0** | **10%** | **vaapi** (zero copy) |
| **S1q, integrated GPU Radeon 610M** (`DRI_PRIME`), H.264 / HEVC 10-bit | 1920×1080 | 30 s | **60.0** | **0 / 0** | 10% / 13% | vaapi |

- **50 opens and closes** (a new `MpvItem`, mpv core and render context each time): no crash, RSS 279 MiB after cycle 5 and 138 MiB at the end.
- **Memory**: 156 MiB RSS while playing 4K. The Dioxus design used ~270 MiB plus ~250 MiB for WebKit's processes.
- **Licenses**: the spike's dependency graph (cxx-qt, cxx, cxx-qt-lib, qt-build-utils…) passes the current `deny.toml` with no new exception. Qt itself is LGPL-3.0 (class A), used as a system library (SPECIFICATION §3.3).
- On the integrated GPU the process also opens the dedicated GPU's device (probably VA-API through the compositor's device), so the result shows that the iGPU can render and composite, not a pure iGPU-only run.

## Findings for the implementation

1. **Load the file only after the render context exists.** With `vo=libmpv`, a `loadfile` issued before `mpv_render_context_create` leaves the file without video output: `frame-drop-count` and `hwdec-current` become "property unavailable", and the item renders nothing. The render context is created on the render thread when the first FBO is created, so loading waits for a queued "renderer ready" signal.
2. **The item/renderer lifetime needs care.** Qt destroys the renderer on the render thread after the item. The update callback and the renderer reach the item through a shared, mutex-protected link that the item clears in its destructor. Without it, the stress test crashes.
3. `QQuickWindow::setGraphicsApi(OpenGL)` before the first window, `setlocale(LC_NUMERIC, "C")` after `QGuiApplication`, the native display in the render context, and `QT_XCB_GL_INTEGRATION=xcb_egl` on X11 (zero-copy VA-API, fixing the X11 weakness of ADR 0001).
4. cxx-qt does not cover `QQuickFramebufferObject` renderers. The video item stays in C++: one small file, our own code (GPL-3.0-only), with `unsafe` limited to the Rust side of the FFI (ARC-011).
5. Without a terminal, Qt 6 logs to journald. The tests set `QT_FORCE_STDERR_LOGGING=1`.

## Consequences if accepted

The specification changes in these places, in a dedicated revision approved by the maintainer:

- **Stack (§1.1, §4)**: Qt ≥ 6.5 (Quick, QML; fractional scaling on Wayland) and cxx-qt replace `dioxus`, `gtk`/`glib`/`gdk` and WebKitGTK. The class B table (§3.2) is checked again: the exceptions forced by Dioxus desktop (`slotmap`, `const_format`, `konst`, `subtle`, the `image` crates, `base16`/D-8, probably `libloading`) disappear; the `icu_*` family stays through `url`/`idna` (reqwest).
- **Architecture (§5)**: `anikoto-ui` becomes QML plus Rust `QObject`s; the video item lives in `anikoto-player` (C++ plus the Rust bridge). The composition of §5.2 (GtkOverlay) and its fallbacks are replaced by Qt Quick on both Wayland and X11.
- **Dictionary popup (POP-009, POP-010, DIC-035)**: Yomitan structured content is rendered as QML items, which is natural for ruby, lists, tables and images, without HTML and therefore without scripts. Per-dictionary CSS (`@scope`) cannot be applied as it is: either a safe subset of CSS properties is mapped onto QML, or the popup uses a web view (QtWebEngine, heavy). This needs a short spike before M3.
- **Tests (TEST)**: the `dioxus-ssr` snapshots become QML tests (Qt Quick Test, item-tree snapshots); the GUI harness keeps its protocol.
- **Packaging (D-5)**: the Flatpak runtime becomes KDE instead of GNOME.
- **i18n (SET-004)**: Fluent stays in Rust; the strings reach QML through a Rust `QObject`.

## Still to verify

- [ ] The developer's real session (KDE Wayland, 4K at 120 Hz, fractional scale 2.7, where S1 dropped 2456 frames in 60 s): `spike-s1q-qt target/fixtures/av_hd60_h264.mkv --measure 60`, plus clicks, keys, resizing, fullscreen and sharpness.
- [ ] A short popup spike (structured content with ruby, a table and an image as QML items) before rewriting POP-009 and DIC-035.
