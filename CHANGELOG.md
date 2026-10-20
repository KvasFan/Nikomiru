# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed (rename)

- **The project is now called Nikomiru** (D-11): "Anikoto" was already taken. Crates `nikomiru-*`, binary `nikomiru`, URL scheme `nikomiru://`, XDG folders `nikomiru`, environment variables `NIKOMIRU_*`, GitHub repository `nikomiru`.

### Added

- Specification (`docs/SPECIFICATION.md`), roadmap (`docs/ROADMAP.md`) and test plan (`docs/TEST.md`) for the Rust rewrite of Fushi.
- GPL-3.0-only license.
- Cargo workspace with the 11 crates of SPECIFICATION §5.1 (still empty), Rust 1.99.1 pinned in `rust-toolchain.toml`, shared lints (`unwrap_used` and `expect_used` forbidden, ERR-002).
- License policy enforced: `deny.toml` (Class A, Class B exceptions of SPECIFICATION §3.2, ban on the alternatives rejected in §3.6), `about.toml` and the `THIRD_PARTY_LICENSES.html` template.
- `cargo xtask` with the commands `ci [--full]`, `fixtures [--large] [--perf]`, `check-headers`, `check-licenses [--canary]`, `trace` and `about`.
- Spikes S1 (video composition) and S2 (dictionary storage) in `spikes/`, with the measurements and the decisions in `docs/adr/`.
- Spike S1q (Qt 6 Quick + cxx-qt + mpv render on the scene graph thread) and ADR 0003: Qt Quick as the UI toolkit instead of Dioxus.
- Spike S5 (the video overlay and the dictionary popup with iced and with egui, measured against S1q's Qt Quick) and ADR 0006.

### Changed

- **D-13: the UI toolkit is egui instead of Qt 6 Quick** (ADR 0006, accepted after spike S5): under heavy load at 4K HEVC 10-bit egui keeps the video like Qt (0–2 dropped frames in 20 s) with about half of its CPU, draws a whole popup entry in one frame, and is all Rust; iced was rejected (no zero-copy video, dropped frames under load, a CC0 crate, no accessibility). Class B exception `libloading` (ISC) in SPECIFICATION §3.2, `deny.toml` and `about.toml`. The port is ROADMAP M3 work track 5; ADR 0003 and the QML part of ADR 0005 are superseded.
- Decisions D-1…D-6 approved and recorded in `docs/SPECIFICATION.md` §10.1.
- SPECIFICATION §5.1: ARC-010 and ARC-011 written as definitions, so that `cargo xtask trace` reads them.
- TEST: test marker rule (§8), L7 test names (`l7_…`), off-screen compositor for L6 (headless Weston or virtual KWin).
- SPECIFICATION §5.2: composition strategy (a) adopted, with the constraints found by spike S1 (`with_disable_dma_buf_on_wayland(false)`, `LC_NUMERIC=C`, native display for hwdec, mpv shutdown order); risk R-1 updated.
- SPECIFICATION §10.1: decisions D-7 (5 Class B procedural macros found in the real graph), D-8 (CC0 exception for `base16`, needed by the Dioxus `router` feature) and D-9 (English for documents, comments and commit messages) approved; §3.2 table and `deny.toml` updated.
- Documents and code comments translated to English (D-9); `docs/SPECIFICHE.md` renamed to `docs/SPECIFICATION.md`.
- ROADMAP: the X11 T-PERF-03 measurement moved from M0 to M1; M4 (release 0.1.0) now includes restyling the repository page after storytold/cadcraft.
- **D-10: the UI toolkit is Qt 6 Quick (QML) through cxx-qt instead of Dioxus** (ADR 0003, accepted after the developer's 4K/120 Hz session showed no dropped frames). SPECIFICATION v0.5, TEST and ROADMAP revised: architecture (§5), class B table reduced to the ICU/Unicode family and `foldhash`, XDG paths implemented in-house (no `directories`, no MPL-2.0 exception), popup and styling as QML items (spike S4 before M3), Flatpak with the KDE runtime. ADR 0001 superseded.
- M0 done: all exit criteria met (the X11 measurement on a real Xorg session moved to M1). SPECIFICATION §5.2.2: CUDA interop only with NVIDIA's proprietary driver (no more `Cannot load libcuda.so.1`), libmpv messages routed to the log.
- `cargo xtask`: dependency rules for cxx-qt/libmpv2 and a guard against the retired toolkit, `unsafe` allowed in cxx-qt bridge modules, C++ only in `nikomiru-player/cpp/`, SPDX headers also on `.cpp`, `.h` and `.qml` files.
- SPECIFICATION: new requirements PLY-016 (save and load mpv option presets in `mpv.conf` format, M6), PLY-017 (options that raw options and presets can never set, M1) and PLY-018 (render and audio settings in the UI, M6), with tests T-PLY-016…018.
