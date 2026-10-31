# Nikomiru — Specification

| Field | Value |
|---|---|
| Status | v0.7 — decisions D-1…D-12 approved on 2026-10-19 and D-13 on 2026-10-20 (§10.1); D-13 replaces Qt 6 Quick with egui; D-11 renames the project to Nikomiru |
| Date | 2026-10-20 |
| Reference | Fushi, commit `ac8034e8bbc25e877e6d419334a38951bc477105` (2026-10-18), <https://github.com/hajisensai/Fushi> |
| Related documents | [ROADMAP.md](ROADMAP.md) · [TEST.md](TEST.md) |

## 0. Conventions

- **MUST / MUST NOT**: mandatory requirement. **SHOULD**: recommended; an exception needs a written justification in the PR. **MAY**: optional.
- Every requirement has a stable identifier (`PLY-012`). IDs are never reused: a removed requirement stays in the table, struck through, with the reason.
- Tests that verify a requirement use the same prefix (`T-PLY-012`, see [TEST.md](TEST.md)).
- "Fushi:" followed by a path names the Fushi file to consult during porting. Paths start from the root of the Fushi repository.
- The **milestone** of each requirement (M0…M9, or "post-1.0") is defined in [ROADMAP.md](ROADMAP.md).
- **Language (D-9)**: documents, code comments, commit messages and developer-tool messages are written in **English**. The user interface is localized through Fluent (SET-004), with Italian and English at 1.0.

---

## 1. Purpose

### 1.1 What Nikomiru is

Nikomiru is a desktop video player for **learning languages by watching anime and series** (*immersion learning*). While you watch, you can click a word in the subtitles: Nikomiru looks it up in the dictionaries you imported (Yomitan format) and, with one click, creates an Anki card with the word, the sentence, a screenshot and the audio of the line.

Nikomiru is a **Rust rewrite of a subset of Fushi**. Fushi is a GPL-3.0 Flutter/Dart app that covers EPUB reading, manga, audiobooks, video, visual novels and much more. Nikomiru starts from its "video + dictionary + Anki" core and reimplements it with:

- **UI**: [egui](https://github.com/emilk/egui) (immediate mode, all Rust) on winit and glutin (OpenGL through EGL), with accessibility through AccessKit;
- **playback**: libmpv through [libmpv2](https://crates.io/crates/libmpv2) 6.0 (core, commands, events) and its OpenGL render API, which draws the video into the window under the interface;
- **licenses**: only GNU (GPL/LGPL), Apache-2.0 and MIT, with the unavoidable exceptions documented in §3.

**Direction (D-12).** Until 1.0 the focus is video with clickable subtitles: a click on a word, in the overlay or in the subtitle list, shows its dictionary explanation, and a second click makes the Anki card. After 1.0 Nikomiru will grow to books (EPUB), manga and online resources. That is why lookup, dictionaries and mining live in crates that know nothing about video (`nikomiru-text`, `nikomiru-dict`, `nikomiru-anki`, §5.1): a book or a manga page must be able to reuse them as they are.

### 1.2 Goals

1. Play local video files with mpv quality: hardware decoding, all codecs and containers supported by mpv.
2. Show text subtitles in an overlay above the video that is **clickable character by character**.
3. Look up words in Yomitan dictionaries with deinflection, frequencies and pitch accent, with quality at least equal to Fushi.
4. Create Anki cards via AnkiConnect (ready-to-use Lapis note) with sentence, screenshot, sentence audio and a link back to the scene.
5. Manage a library of series and episodes, with progress and watch statistics.
6. Work offline. The network is used only for features the user asks for (metadata, online subtitles, word audio). No telemetry.

### 1.3 Non-goals (out of scope until a new decision)

| Fushi feature | Decision | Reason |
|---|---|---|
| EPUB reader, manga + OCR, PDF, audiobooks (Sasayaki), online resources | **planned after 1.0** (D-12) | Each one is as large as the whole MVP; until 1.0 the focus is video. The text, dictionary and Anki crates stay independent of video so they can be reused |
| Text and voice hooks for galgames (Windows) | excluded | Windows only; depends on LunaHook and MinHook |
| Torrent, Mihon/Aniyomi extensions, LNReader, download center | excluded | Out of scope; Kotlin/APK runtime |
| AI/LLM, ASR (subtitle generation), TTS, ONNX models | excluded | Some models have CC BY-NC licenses; huge size |
| Google Lens OCR, AACS Blu-ray decryption | excluded | Legal and terms-of-service risks |
| Cloud sync, Interconnect, iroh P2P, headless server, CLI | post-1.0 (WebDAV backup/sync only) | Complexity; not needed for the MVP |
| Built-in SRS (FSRS) | post-1.0 | `fsrs` is BSD-3-Clause: it would need an exception (§3) |
| Leaderboard, in-app feedback, usage telemetry | excluded | Privacy; no service to run |
| Android | in scope (D-16) | Builds asked by the maintainer; libmpv from mpv-android, AnkiDroid's own API instead of AnkiConnect, touch instead of hover (§5.2.4) |
| iOS | excluded | Not asked; libmpv and the App Store's rules |
| Windows, macOS | in scope (D-16) | Linux first; builds asked by the maintainer, after spike S6 (§5.2.4) |

### 1.4 Platforms

- **Primary**: Linux x86_64, both **Wayland** and **X11**, with OpenGL drivers with EGL installed (Mesa or the proprietary NVIDIA driver), the Wayland and X11 client libraries and libxkbcommon, which winit loads at run time, and fontconfig. Fractional scaling on Wayland comes through winit. On X11 the OpenGL context is EGL too (§5.2.2).
- **libmpv**: API ≥ 2.2 (mpv ≥ 0.37). Development reference: mpv 0.41 (API 2.5).
- **FFmpeg**: ≥ 6.0 as a system executable (`ffmpeg`, `ffprobe`). Development reference: 9.0.
- Secondary (post-1.0): Windows 10+, macOS 13+ on Apple Silicon.

### 1.5 Languages

- Primary study language: **Japanese**. All algorithms MUST still be data-driven (Yomitan transform tables and per-language preprocessors), so that other languages can be added without rewriting code.
- UI languages: **Italian** and **English** at 1.0. Japanese SHOULD follow.

---

## 2. Relationship with Fushi

### 2.1 Fushi in numbers (at the reference commit)

- App: about 1,860 Dart files, about 900k lines in `fushi/lib`; separate packages (`fushi_engine` alone: about 182k lines).
- Tests: about 4,270 `*_test.dart` files, about 1M lines. About 930 are "guard tests" that check architectural rules by reading the source.
- Drift/SQLite database: 98 tables, schema v118.
- Native dictionary engine in C++23 (`native/fushidicts`, a fork of hoshidicts).
- 17 UI languages, about 8,400 keys.

**Consequence**: Nikomiru is not a 1:1 port. It is a **selective, requirement-driven** port based on this document. Fushi is the reference implementation for behaviors, algorithms, edge cases and known bugs.

### 2.2 Porting rules

1. Before implementing a requirement that cites a Fushi file, read that file and its tests. Every cited Fushi bug (`BUG-xxxx`) becomes a regression test in Nikomiru.
2. Code translated from Fushi, Yomitan, hoshidicts, Lapis or Tsubasa is a **GPL-3.0 derivative work**. The Rust module that contains it MUST have a header comment that names the project and the source files, with the commit.
3. Port **behaviors, not structures**. Do not replicate workarounds tied to Flutter, Dart or Windows (isolates, FFI, Windows IME).
4. Where Fushi has known gaps (see §6.6.8), Nikomiru MAY improve, but MUST document the difference here and cover it with a test.

### 2.3 Module map

| Fushi area | Reference | Nikomiru crate | Milestone |
|---|---|---|---|
| Video player, mpv config | `fushi/lib/src/media/video/video_player_controller.dart`, `video_mpv_config.dart` | `nikomiru-player` | M1 |
| Subtitles (parser, markup, overlay) | `packages/fushi_audio/lib/src/parsers/`, `fushi/lib/src/media/video/video_subtitle_overlay.dart` | `nikomiru-subs`, `nikomiru-ui` (egui) | M2 |
| Dictionary engine | `native/fushidicts/fushidicts_src/` | `nikomiru-dict` | M3 |
| Preprocessors and kana | `native/fushidicts/fushidicts_src/text_processor/` | `nikomiru-text` | M3 |
| Lookup popup | `fushi/assets/popup/popup.js`, `selection.js` | `nikomiru-ui` (egui) | M3 |
| Anki / Lapis / composer | `packages/fushi_anki/lib/src/` | `nikomiru-anki` | M4 |
| Video mining (ffmpeg) | `fushi/lib/src/mining/immersion_mining_engine.dart`, `packages/fushi_engine/lib/utils/misc/desktop_audio_clipper.dart` | `nikomiru-media` | M4 |
| Library, file name parser | `packages/fushi_engine/lib/media/video/video_library_import.dart`, `video_filename_parser.dart` | `nikomiru-library` | M5 |
| Word audio | `fushi/lib/src/utils/misc/word_audio_resolver.dart`, `local_audio_db.dart` | `nikomiru-dict` (`audio` module) | M5 |
| Statistics | `fushi/lib/src/media/video/video_watch_tracker.dart`, `fushi/lib/src/stats/` | `nikomiru-library` (`stats` module) | M6 |
| Shortcuts | `fushi/lib/src/shortcuts/shortcut_defaults.dart` | `nikomiru-ui` | M1–M6 |
| Online subtitles, metadata | `packages/fushi_engine/lib/media/video/jimaku_client.dart`, `metadata/` | `nikomiru-online` | M7 |

---

## 3. Licenses

### 3.1 Nikomiru license

Nikomiru is distributed under **GPL-3.0-only** (`LICENSE` file).

- This is mandatory: Nikomiru derives from Fushi (GPL-3.0), the Yomitan tables (GPL-3.0), hoshidicts (GPL-3.0) and the Lapis templates (GPL-3.0).
- "only" is used instead of "or-later" because Fushi does not explicitly grant later versions. A move to "or-later" is possible only if all derived sources allow it.
- Every source file MUST start with the SPDX header `// SPDX-License-Identifier: GPL-3.0-only`.

### 3.2 Rust dependency policy

**Class A — always allowed** (direct and transitive dependencies):

| SPDX license | Notes |
|---|---|
| `MIT` | |
| `Apache-2.0`, `Apache-2.0 WITH LLVM-exception` | Compatible with GPL-3.0 |
| `GPL-2.0-or-later`, `GPL-3.0-only`, `GPL-3.0-or-later` | `GPL-2.0-only` is **not** allowed: it is incompatible with GPL-3.0 |
| `LGPL-2.1-only`, `LGPL-2.1-or-later`, `LGPL-3.0-only`, `LGPL-3.0-or-later` | E.g. `libmpv2` (LGPL-2.1) |

Expressions with `OR` (e.g. `MIT OR Apache-2.0`, `Unlicense OR MIT`, `MIT OR Zlib OR Apache-2.0`) are allowed if at least one branch is class A: Nikomiru picks that branch.

**Class B — unavoidable exceptions, allowed only for the listed crates.**

A real check with `cargo deny` on the whole §4 stack (D-13: egui 0.37 on winit and glutin, `libmpv2` 6.0.0, `reqwest` with native-tls) shows a few unavoidable crates outside class A: the Unicode-3.0 ICU family (`url` → `idna`, needed by reqwest, and the direct `icu_normalizer`), `unicode-ident` (every procedural macro), `foldhash` (Zlib, the default hasher of `hashbrown` in egui's text stack and in AccessKit) and `libloading` (ISC: glutin and winit open the system's graphics and windowing libraries at run time, [ADR 0006](adr/0006-toolkit-comparison.md)). All of them are permissive licenses compatible with GPL-3.0. The longer list forced by Dioxus desktop (D-1, D-7, D-8) went with D-10; cxx and cxx-qt went with D-13.

| License | Crate (verified version) | Required by |
|---|---|---|
| `Unicode-3.0` | `icu_*` 2.3, `litemap`, `tinystr`, `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`, `zerotrie`, `zerovec`, `zerovec-derive`, `potential_utf`; `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`) | `url` → `idna` (reqwest); `icu_normalizer` (direct, §4); `proc-macro2` (every procedural macro: serde_derive, thiserror, zbus through AccessKit) |
| `Zlib` | `foldhash` | `hashbrown`'s default hasher (fontique, AccessKit, epaint's `vello_cpu`) and `glifo` (epaint) |
| `ISC` | `libloading` | `glutin` and the libraries winit loads at run time (libEGL, libwayland, Xlib): egui's window and OpenGL context (D-13) |

Rules for class B:

1. The exceptions apply **only** to the listed crates, through `[[licenses.exceptions]]` in `deny.toml`. A new crate with one of these licenses makes CI fail.
2. Every new exception requires: a justification ("what requires it and why it cannot be avoided"), a row in this table and explicit approval from the maintainer.
3. No **direct** dependency of Nikomiru may be class B, except `icu_normalizer` (see §4): it is already in the graph through `idna`, so it adds no new license.
4. On every update of egui, winit, glutin or reqwest we MUST try to shrink class B.

**Forbidden** (CI fails):

- All licenses not listed above.
- In particular: `AGPL-*` (Anki rslib), `CC-BY-NC-*`, `CC0-1.0` (e.g. `notify`; forbidden with no exception, D-8 is obsolete), `BSL-1.0` (e.g. `xxhash-rust`), `GPL-2.0-only`, `OpenSSL`, `CECILL-*`, and any crate without a declared license.

### 3.3 System libraries and external programs

They are loaded at runtime or invoked as processes. They are not distributed with Nikomiru, except in the post-1.0 packages.

| Component | License | Use |
|---|---|---|
| libmpv (mpv) | GPL-2.0-or-later (or LGPL-2.1-or-later, depending on the build) | Playback |
| FFmpeg / FFprobe (executables) | LGPL-2.1-or-later or GPL-2.0-or-later | Subtitle extraction, audio clips, images |
| libwayland-client, libxkbcommon, libX11 and its extensions (Xcursor, Xi, Xrandr) | MIT / X11 | Window and input (winit, glutin), loaded at run time |
| fontconfig | MIT-style | Finding the system fonts (fontique) |
| glibc (`iconv`) | LGPL-2.1-or-later | Subtitle encoding conversion (§6.4) |
| OpenSSL 3 | Apache-2.0 | TLS through `native-tls` |
| Mesa / libEGL / libGL | MIT | OpenGL context for egui and for mpv's rendering |

### 3.4 Data and resources

| Resource | License | Rule |
|---|---|---|
| Yomitan transform tables (JSON generated from Fushi `fushi/assets/transforms/`) | GPL-3.0 | Copied into `assets/transforms/` with a `PROVENANCE.md` file (Yomitan version and Fushi commit) |
| Lapis note templates (donkuri/lapis) | GPL-3.0 | Copied byte for byte into `assets/anki/lapis/` from a release tag, with a `PROVENANCE.md` (version, commit, hashes, authors); credited in the README and on the About screen, since `THIRD_PARTY_LICENSES.html` lists crates only |
| Dictionaries (JMdict, JMnedict, KANJIDIC, frequency dictionaries, etc.) | Various (e.g. CC BY-SA 4.0) | **NOT** distributed. The user imports them. Tests use hand-written dictionaries (TEST §4.3) |
| Fonts | — | **NO** fonts are distributed: system fonts are used (e.g. Noto CJK). An OFL font is not in the allowed list |
| Icons and logo | GPL-3.0-only | Original works of the project; no Creative Commons license, for consistency with the policy |

### 3.5 How the policy is enforced

- `deny.toml` ([cargo-deny](https://github.com/EmbarkStudios/cargo-deny), MIT/Apache) encodes §3.2: `[licenses] allow = [class A]` and `[[licenses.exceptions]]` for class B. The same checks also cover `bans`, `advisories` and `sources`.
- `cargo deny check` is a **mandatory CI gate** (T-LIC-001).
- `cargo about` (MIT/Apache) generates `THIRD_PARTY_LICENSES.html`. The file is included in every package and shown in the "About" screen (attribution requirement of MIT/BSD/ISC/Zlib).
- Development tools (cargo-deny, cargo-about, cargo-nextest) are not distributed, but they MUST still comply with class A. **No `just`** (CC0): tasks are written as `cargo xtask` in Rust.

### 3.6 Alternatives chosen to stay within the policy

| Need | Rejected choice (license) | Adopted choice |
|---|---|---|
| Local database | SQLite (public domain: not in the list) | `redb` (MIT OR Apache-2.0) |
| Reading the local audio database (AUD-003) | SQLite (public domain); `turso` (BSD-3-Clause `subtle`, Unicode-3.0 ICU collation in its graph, S3) | Own read-only reader of the SQLite file format ([ADR 0004](adr/0004-local-audio-db.md)) |
| Glossary compression | zstd C (BSD-3 / GPL-2.0-only) | `lz4_flex` (MIT) |
| Fast hash | `xxhash-rust` (BSL-1.0) | `twox-hash` (MIT) if needed; otherwise the `redb` hasher |
| Subtitle encoding detection | `encoding_rs` (`… AND BSD-3-Clause`) | glibc `iconv` via FFI (LGPL) + own heuristics |
| TLS | `rustls` + `ring` (ISC) | `native-tls` → OpenSSL (Apache-2.0) |
| Subtitle parsers | `subparse` (MPL-2.0) | Own parsers (ported from Fushi) |
| Anime file name parser | `anitomy` (MPL-2.0) | Own parser (port of `video_filename_parser.dart`) |
| File system watching | `notify` (CC0-1.0) | Manual or periodic rescan; inotify via `libc` if ever needed |
| Task runner | `just` (CC0-1.0) | `cargo xtask` |
| Unicode normalization | `unicode-normalization` (adds an AND Unicode-3.0) | `icu_normalizer` (already in the graph) |
| XDG paths | `directories` (pulls `option-ext`, MPL-2.0) | Own implementation in `nikomiru-core` (XDG Base Directory variables and `user-dirs.dirs`) |

---

## 4. Technology stack

Versions verified on crates.io on 2026-10-19. The lockfile (`Cargo.lock`) is committed.

| Area | Crate | Version | License |
|---|---|---|---|
| Toolchain | Rust stable, edition 2024, pinned in `rust-toolchain.toml` | ≥ 1.95: egui 0.37 (also let chains, `File::try_lock`) (development: 1.99) | — |
| UI | `egui` (no default fonts), `egui-winit` (AccessKit, clipboard; no `links`), `egui_glow`, `winit` (no Adwaita decorations), `glutin` and `glutin-winit` (EGL first), `glow`, `raw-window-handle` | 0.37 / 0.30 / 0.32 / 0.5 / 0.17 / 0.6 | MIT OR Apache-2.0; class B: `libloading` (§3.2) |
| Fonts | `fontique` (system fonts through fontconfig, §3.4), `egui_system_font_provider` | 0.11 / 0.37 | Apache-2.0 OR MIT |
| Video | `libmpv2` (feature `render`) for the core, `libmpv2-sys` for the render context (§5.2) | 6.0.0 / 4.0.1 | LGPL-2.1 |
| Dictionary images | `image` (png, jpeg, gif, webp, bmp), decoded on the lookup thread (POP-010) | 0.25 | MIT OR Apache-2.0 |
| Wayland drag and drop | `wayland-client`, `wayland-backend` (already in the graph through winit): a data device on winit's connection, which has none | 0.31 / 0.3 | MIT |
| Async | `tokio` | 1.x | MIT |
| Storage | `redb` | 4.x | MIT OR Apache-2.0 |
| Serialization | `serde`, `serde_json`, `toml`, `postcard` | 1.x | MIT OR Apache-2.0 |
| Compression | `lz4_flex` | 0.14 | MIT |
| ZIP | `zip` (`default-features = false`, `deflate-flate2`; `flate2` with `miniz_oxide` backend) | 9.x | MIT |
| HTTP | `reqwest` (`default-features = false`, `blocking`, `native-tls`: the workers are threads, so the blocking client, with no proxy for AnkiConnect, [ADR 0007](adr/0007-mining-to-anki.md)) | 0.13 | MIT OR Apache-2.0 |
| Unicode | `icu_normalizer`, `unicode-segmentation` | 2.3 / 1.13 | Unicode-3.0 (class B) / MIT OR Apache-2.0 |
| Errors | `thiserror` (libraries), `anyhow` (binary and xtask only) | 2.x / 1.x | MIT OR Apache-2.0 |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | 0.1 / 0.3 | MIT |
| XDG paths | Own implementation in `nikomiru-core` (§3.6), no crate | — | — |
| System calls | `libc`, only in `nikomiru-app::linux` (`setlocale` for libmpv, termination signals, iconv) | 0.2 | MIT OR Apache-2.0 |
| D-Bus (portals) | Own minimal client in `nikomiru-app::dbus` (PLY-013), no crate | — | — |
| File dialogs, external links | The XDG portal (`FileChooser`, `OpenURI`) through the own D-Bus client, no crate; `xdg-open` as the fallback for links | — | — |
| i18n | `fluent-bundle`, `unic-langid` | 0.16 | Apache-2.0 OR MIT |
| Identifiers and hashes | `uuid`, `sha2`, `md4` (ed2k, M7) | — | MIT OR Apache-2.0 |
| Dictionary HTML (post-1.0 only, MDict): sanitized to the POP-009 allowlist, then converted to the popup's view model | `ammonia` | 4.x | MIT OR Apache-2.0 |
| Tests | `proptest`, `insta`, `criterion`, `wiremock` (online services, M7; AnkiConnect has its own fake), `tempfile`, `egui_kittest` (UI tests through AccessKit, no renderer) | — | MIT/Apache |

Every new crate requires, in the PR: the license (verified by `cargo deny`), a justification and an assessment of the "write the code ourselves" alternative.

---

## 5. Architecture

### 5.1 Workspace

```
nikomiru/
├── Cargo.toml              # workspace, centralized [workspace.dependencies]
├── rust-toolchain.toml
├── deny.toml  about.toml
├── xtask/                  # cargo xtask: ci, fixtures, about, bench
├── crates/
│   ├── nikomiru-core/       # shared types, config, paths, errors, clock
│   ├── nikomiru-text/       # normalization, kana, preprocessors, sentences, furigana, character count, file names
│   ├── nikomiru-dict/       # Yomitan import, redb storage, deinflection, lookup, word audio
│   ├── nikomiru-subs/       # SRT/VTT/ASS parsers, cue index, offset, encodings
│   ├── nikomiru-media/      # ffmpeg/ffprobe runner: subtitle extraction, clips, frames
│   ├── nikomiru-player/     # libmpv core (libmpv2): init, properties, commands, events;
│   │                       # the OpenGL render context of the video
│   ├── nikomiru-anki/       # AnkiConnect client, note composer, Lapis, return link
│   ├── nikomiru-library/    # library: folders, series, progress, statistics, cards, dictionary registry
│   ├── nikomiru-online/     # Jimaku, OpenSubtitles (M4b); AniList/Jikan (M7)
│   ├── nikomiru-ui/         # egui interface: player page, subtitle overlay, popup, pages, controls
│   └── nikomiru-app/        # binary: winit event loop, glutin (EGL) window, egui_glow, wiring, single instance
├── assets/                 # transforms/, anki/lapis/, i18n/*.ftl, icons
├── tests/fixtures/         # small committed files; media are generated with xtask
└── docs/
```

**ARC-010 — Dependency rules** (verified by T-ARC-010 with `cargo metadata`):

- `nikomiru-core` depends on no workspace crate.
- `nikomiru-text`, `nikomiru-subs`, `nikomiru-dict`, `nikomiru-anki` and `nikomiru-library` **MUST NOT** depend on the UI toolkit (`egui` and its crates, `winit`, `glutin`, `glow`, `accesskit`) or on `libmpv2`. They stay pure logic, testable without a display.
- Only `nikomiru-player` depends on `libmpv2` (and `libmpv2-sys`). Only `nikomiru-ui` and `nikomiru-app` depend on the UI toolkit. No crate depends on a retired toolkit (Dioxus, D-10; Qt through cxx-qt, D-13).
- **ARC-011**: `unsafe` is allowed only in `nikomiru-player` (mpv FFI and the render context), in `nikomiru-app::linux` (environment, `setlocale`, signals, iconv, the Wayland connection for drag and drop) and in `nikomiru-app::window` (the OpenGL context through glutin, the GL state around mpv's render). There is no C++. Every `unsafe` block MUST have a `// SAFETY:` comment on the line above it. `nikomiru-app` declares `#![deny(unsafe_code)]`, with `#[allow(unsafe_code)]` only on those modules; every other crate except `nikomiru-player` declares `#![forbid(unsafe_code)]`.

### 5.2 Video and UI in the same window (egui)

This was the **main technical risk** (R-1). The Dioxus design (a transparent WebKitGTK webview over a `GtkGLArea`) recomposed the whole window on the GTK main thread for every video frame and dropped frames at 4K under CPU load ([ADR 0001](adr/0001-video-composition.md)). D-10 moved to Qt Quick, which rendered mpv into a scene-graph item on its render thread ([ADR 0003](adr/0003-qt-quick-ui.md)). With D-13 the UI is egui: mpv draws each video frame into the window's framebuffer, and egui paints the interface over it in the same frame ([ADR 0006](adr/0006-toolkit-comparison.md)).

#### 5.2.1 Composition

```
winit window + glutin surface (OpenGL through EGL; GLX only as a fallback)
└── each frame, on the UI thread
    1. egui lays out the interface      (controls, subtitles, popups, pages)
    2. mpv draws the video              (libmpv render API, framebuffer 0)
    3. egui paints over it              (egui_glow, no clear)
    4. buffer swap, then mpv's report_swap
```

- The OpenGL context MUST be created through EGL when it is available (glutin, on X11 too), before the first frame.
- The video fills the window and the interface is painted over it. Pages without video (library, settings) do not call mpv's render.
- Input goes to egui; a click no element takes goes to the player page (play/pause, or closing the popups; a double click toggles fullscreen). Keys go to the shortcuts first (KEY-002); egui gets them only when a text field has the keyboard (KEY-004).
- The UI thread renders the video, so it MUST NOT wait for the mpv core (`<mpv/render.h>`): the player's commands are queued to a command thread (§5.3), and long work stays on worker threads, because on this thread it delays video frames.

Measured in ADR 0006 (spike S5): at 4K HEVC 10-bit under heavy CPU load, 60 renders per second and 0–2 dropped frames in 20 s, with about half of Qt Quick's process CPU. ADR 0003 keeps the Qt figures.

#### 5.2.2 mpv rendering

1. The mpv core is owned by `nikomiru-player` (Rust, `libmpv2`): options (PLY-005), properties, commands and events. Its `render` module holds the render context; the app hands it only the GL context's `get_proc_address`, the framebuffer and the native display.
2. The render context is created once, as soon as the window's GL context is current and before the first frame: `mpv_render_context_create` with the OpenGL API, `get_proc_address` from glutin's display, and the native Wayland or X11 display, which mpv needs for zero-copy hwdec interop (VA-API on EGL).
3. `loadfile` MUST wait until the render context exists: with `vo=libmpv`, a file loaded earlier has no video output. A file opened earlier (command line, second launch) waits in the session until then.
4. The mpv update callback (called from any thread) **MUST NOT** call mpv APIs. It only wakes the event loop for a redraw.
5. Each frame resets the GL state mpv expects (no blending, scissor, program, vertex array or texture bound), calls `mpv_render_context_render` on framebuffer 0 at the window's physical size with `FLIP_Y`, lets egui paint without clearing, swaps the buffers and calls `mpv_render_context_report_swap`.
6. Every `mpv_render_*` call runs on the UI thread, with the window's GL context current.
7. Shutdown, in this order: the render context is freed with the GL context current; then egui's painter, the session (which saves the position, joins its workers and closes the library) and the window; then, once the command thread has run what it had queued, the `quit` command, which sends `MPV_EVENT_SHUTDOWN` to the event-thread client; then the join of the `mpv-events` thread; finally the core is destroyed. The lifetimes of `libmpv2::render::RenderContext<'a>` are not enough to guarantee this: `'a` is only a `PhantomData`, and the `Drop` of `Mpv` calls `mpv_destroy`, which does not stop the core while another client exists (ADR 0001). This is why the render context is driven through `libmpv2-sys`.
8. `LC_NUMERIC` MUST be `"C"` before mpv is created, or libmpv refuses to start (`mpv_create` returns NULL); on X11 winit sets `LC_CTYPE` when its event loop starts. `nikomiru-app` calls `setlocale(LC_NUMERIC, "C")` right after creating the event loop, before the player.
9. On X11 the GL context is EGL too (glutin prefers EGL), so mpv gets zero-copy VA-API. If EGL is not available, glutin falls back to GLX, where `hwdec=auto-safe` uses a copy mode (`vulkan-copy` or `vaapi-copy`).
10. `gpu-hwdec-interop` is `vaapi` unless NVIDIA's proprietary driver is loaded (`/proc/driver/nvidia/version` exists), in which case mpv's automatic choice, CUDA included, is kept. It is set when the core is created, because the render context probes the interops as soon as it exists, before any file is loaded. On AMD and Intel the CUDA interop is useless, and probing it prints `Cannot load libcuda.so.1` on stderr (seen on the developer's session, 2026-10-19).
11. libmpv's messages go to Nikomiru's log (`mpv_request_log_messages`, level `warn` by default, `v` with debug logging), never to stderr; PRV-002 applies to them as well.

#### 5.2.3 Fallback

- Without OpenGL (neither EGL nor GLX) there is no window: the app reports the error and exits. Option D (mpv in its own window) is withdrawn: egui needs OpenGL too, so it would keep the video without the interface.
- The GtkOverlay design of ADR 0001 and its fallbacks B (separate transparent window over a `wid` child window) and C (`dioxus-native`) are withdrawn (D-10).

#### 5.2.4 Other platforms (D-16)

The maintainer asked for builds for macOS, Windows and Android (D-16). Linux stays the reference platform. Spike S6 (ROADMAP, milestone "Ports") finds what each port takes before any is promised for a release.

- **Windows**: the same integration, with glutin on WGL. The platform layers to replace are the D-Bus portals (file dialogs, screensaver, links, the color scheme), iconv, `localtime_r`, the single-instance socket and the XDG folders. libmpv and ffmpeg ship with the program (their LGPL/GPL source obligations apply). It is built from Linux with the `x86_64-pc-windows-gnu` target, then checked on a real Windows.
- **macOS**: OpenGL is deprecated (glutin still supports CGL); a Metal path may be needed, to be evaluated. The Apple SDK's license allows it only on Apple hardware, so a build needs a Mac or a macOS build machine.
- **Android**: winit's Android backend with GLES; libmpv as built by mpv-android; files through the system's document picker; cards through AnkiDroid's API (AnkiConnect is a desktop add-on); touch: a tap looks up, a long press selects. A port of this size changes the input model of the popup and of the subtitles.

### 5.3 Threads and events

```
 ┌──────────── UI thread (main) ────────────┐   queued     ┌──── mpv-commands thread ────┐
 │ winit event loop, egui, the session      ├─────────────►│ commands, in order          │
 │ each frame: layout, mpv render, paint,   │              └─────────────────────────────┘
 │ swap                                     │   messages   ┌──── mpv-events thread ──────┐
 │                                          │◄─────────────┤ wait_event → PlayerEvent    │
 └──────────────────────▲───────────────────┘   (proxy)    └─────────────────────────────┘
                        │ messages (proxy)
 ┌──────────────────────┴──────────────────────────────────────────────────────────────┐
 │ workers: subtitles (ffmpeg), lookup, import, library writes, anki (mining), portal,   │
 │ single instance, signals, test harness                                                │
 └───────────────────────────────────────────────────────────────────────────────────────┘
```

- The **UI thread** runs the winit event loop, egui and the session (the UI state), and renders the video in every frame (§5.2.2). It never waits for the mpv core.
- The **`mpv-commands` thread** runs the player's commands in order with the synchronous libmpv API: `PlayerHandle` queues them and never waits for them. Their errors are logged; a refused `loadfile` becomes an `EndFile` error of its load. A command with a reply (the screenshots of PLY-011 and ANK-034) sends its result back on a channel, which only a worker waits on.
- The **`mpv-events` thread** waits for mpv events and turns them into `PlayerEvent`s stamped with their load's token (ARC-002).
- **Workers**, each on its own thread: subtitle loading and extraction, dictionary lookups and imports, library writes, the `anki` worker (AnkiConnect requests one at a time, and the mining of a card: the frame the session asked for at the click, ffmpeg jobs in parallel, composition, upload; on exit it finishes the job it is on and drops the queued ones; [ADR 0007](adr/0007-mining-to-anki.md)), the screensaver portal, the single-instance socket, the termination signals. Their results reach the UI thread as messages through the event loop's proxy, which also wakes it.

- **ARC-001**: `nikomiru-player` exposes `PlayerHandle` (asynchronous commands) and a stream of `PlayerEvent`. The observed properties (`observe_property`) are: `time-pos`, `duration`, `pause`, `speed`, `volume`, `mute`, `track-list`, `aid`, `sid`, `chapter-list`, `eof-reached`, `seeking`, `paused-for-cache`, `video-params`, `demuxer-cache-state`.
- **ARC-002**: every load has an increasing `LoadToken`. Events and asynchronous results with an old token are discarded. This is the equivalent of the Fushi bugs "re-check player and loadToken after every await".
- **ARC-003**: no synchronous read of mpv properties on the GUI thread during playback (Fushi BUG-3507). State is read from events.
- **ARC-004**: `time-pos` updates to the UI are limited to ≤ 30 Hz. Cue synchronization uses the most recent value with a binary search. Fushi's 125 ms timer is not needed.

### 5.4 Persistence

| What | Where | Format |
|---|---|---|
| Settings | `$XDG_CONFIG_HOME/nikomiru/config.toml` | Human-readable TOML; atomic write (temporary file + rename); permissions 0600 |
| Library, progress, statistics | `$XDG_DATA_HOME/nikomiru/library.redb` | redb; `postcard` values; `meta.schema_version` table |
| Dictionaries | `$XDG_DATA_HOME/nikomiru/dicts/<id>/dict.redb` | One file per dictionary (§6.5.3) |
| Cache (extracted subtitles, thumbnails, word audio) | `$XDG_CACHE_HOME/nikomiru/` | Files; can be deleted at any time |
| Logs | `$XDG_STATE_HOME/nikomiru/logs/` | Text, daily rotation, 7 files |

- **DAT-001**: schema migrations go forward only, are numbered and are idempotent. Before migrating, the file is copied to `library.redb.bak-v<N>`.
- **DAT-002**: if the file was written by a newer version of Nikomiru (schema higher than the supported one), it is opened read-only with a warning. It is never modified.
- **DAT-003**: the identity of a video is `uid = "video/" + sanitize(file name without extension)`, as in Fushi. In addition, `size` and `mtime` are stored to recognize a moved file.

### 5.5 External processes

- **EXT-001**: ffmpeg and ffprobe are searched in this order: the `paths.ffmpeg` setting, the `NIKOMIRU_FFMPEG` variable, `PATH`. At startup the version and the presence of the required encoders are checked (`ffmpeg -hide_banner -encoders`). If one is missing, the related feature is disabled with a clear message.
- **EXT-002**: arguments are always passed as a vector, never through a shell. Input paths are prefixed with `file:` to neutralize names that start with `-` or contain `:`.
- **EXT-003**: every process has a timeout (default 60 s; for subtitle extraction it scales with the duration) and can be cancelled. On cancellation the process is killed and partial files are removed.

### 5.6 Errors, logs and privacy

- **ERR-001**: libraries define typed errors with `thiserror`. The UI shows translated, actionable messages ("AnkiConnect is not responding on 127.0.0.1:8765: is Anki open?").
- **ERR-002**: no `panic!`/`unwrap()` on external input (files, network, dictionaries). Clippy with `unwrap_used` and `expect_used` counts as an error in library crates.
- **PRV-001**: zero telemetry. Without a user action or a feature the user has enabled, the only allowed connection is to AnkiConnect on loopback.
- **PRV-002**: logs contain neither subtitle text nor API keys, nor a user name or password in a URL. Paths are allowed.

### 5.7 Single instance and URL scheme

- **APP-001**: only one instance per user. A second launch passes its arguments (files to open, `nikomiru://` URLs) to the running instance through the Unix socket `$XDG_RUNTIME_DIR/nikomiru.sock`, then exits.
- **APP-002**: the `.desktop` file (`assets/nikomiru.desktop`) registers `MimeType=x-scheme-handler/nikomiru;` and the video types, with `Exec=nikomiru %U`. Until the packages of 1.0, `cargo xtask install` installs it with the binary for the user and registers the scheme.

---

## 6. Functional requirements

### 6.1 Video library (LIB)

| ID | Requirement | Fushi |
|---|---|---|
| LIB-001 | MUST open a single video file: from the file dialog, by drag & drop onto the window or as a command-line argument. Extensions offered in the dialog: mp4 mkv avi mov webm m4v ts m2ts mts flv wmv mpg mpeg ogv; the dialog MUST also allow "all files" | `kVideoExtensions` |
| LIB-002 | MUST import a folder recursively, in two modes: **by series** (default) and **by folder** | `video_library_import.dart` |
| LIB-003 | By series: the file name is parsed Anitomy-style (series, season, episode, group, resolution). If the season is missing, it is derived from the folder (`Season 2`, `S02`, `第2季`, trailing number). If a name cannot be parsed, the episode number is inferred by comparing it with neighboring files. A name that is only an episode (`01.mkv`, `第01集.mp4`) takes the series of its folder, or of the folder above a season folder (`Show/Season 2/01.mkv` is season 2 of `Show`; Fushi keeps `Season 2` as a series) | `video_filename_parser.dart` |
| LIB-004 | Changing mode or rescanning MUST NOT lose progress, chosen subtitles or metadata. A scan MUST NOT recreate series the user has deleted | spec `2026-09-07-video-library-workflow.md` |
| LIB-005 | The home page shows "Continue watching" (latest episodes started and not finished) and the series grid with covers. The cover is the frame at 10% of the first episode (ffmpeg, cached) until metadata are available | |
| LIB-006 | Episode progress: saved at most once per second, only after the file is open and the resume seek has happened. On exit and on episode change it is saved immediately | `video_player_controller.dart` |
| LIB-007 | Resume rule: restart from 0 if the saved progress is ≥ 90% or if less than 3 s remain. "Previous episode" and auto-advance always start from 0 | `video_episode_start_policy.dart` |
| LIB-008 | `completed_at` is set the first time progress reaches 90%. The grid MUST show the watched / in progress / new state | |
| LIB-009 | At the end of an episode the next one MUST be able to start (setting `player.auto_next`, default yes), with a cancellable 5 s countdown | |
| LIB-010 | For each series the audio track, the chosen subtitle (by language or track title) and the subtitle offset are remembered. They apply to all episodes, which can however have their own values | `VideoBooks.delayMs` |
| LIB-011 | It MUST be possible to rename, merge and split series, and to move episodes from one series to another | |
| LIB-012 | Removing an item from the library MUST NOT delete the video file from disk | |
| LIB-013 | **First steps**: until they are done, the library's home page starts with three steps, each marked done once it is: import a dictionary (a button opens Settings → Dictionaries), add a folder of videos (the "Add folder…" of the page), make a first card (a button opens Settings → Anki, the line says that Anki must be open with AnkiConnect). Done is read from what lasts: a dictionary installed, a series or folder in the library, a card in `mined_cards`. "Hide" saves `[general] first_steps = false`. M8 (TEST §9.5) | |

### 6.2 Player (PLY)

| ID | Requirement | Fushi |
|---|---|---|
| PLY-001 | Commands: play/pause, seek ±N s (default 3, range 1–30), absolute seek from the bar, speed 0.25–4.0 (step 0.1), volume 0–150 (step 5), mute, frame step forward/back, previous/next chapter, fullscreen | `video_player_shortcuts.dart` |
| PLY-002 | "Hold to speed up": speed 2.0× while the key is held (default `E`) | |
| PLY-003 | Audio track selection from the `track-list` list (language, title, codec, channels), saved per series | |
| PLY-004 | Overlaid control bar: it hides after 2 s without mouse movement during playback (it also hides the cursor) and reappears on movement. It stays visible while paused, with the mouse over the subtitles and with a popup open | |
| PLY-005 | mpv options applied **before** `initialize` (fixed): `vo=libmpv`, `config=no`, `input-default-bindings=no`, `input-vo-keyboard=no`, `osc=no`, `terminal=no`, `idle=yes`, `keep-open=yes`, `ytdl=no`, `sub-auto=no` | `video_mpv_config.dart` |
| PLY-006 | Configurable options, set before `loadfile`: `hwdec` (`auto-safe` default, `auto-copy-safe`, `no`), quality profile (`default` or `high-quality`), `deband`, `interpolation` (with `video-sync=display-resample` and `tscale=oversample`), `audio-channels` (default `auto-safe`; Fushi preset `7.1,5.1,stereo` as an option), `audio-pitch-correction=yes`, `tone-mapping=auto`. In addition, a "raw mpv options" field (`key=value` per line) applied last, with validation and a per-line error | `video_mpv_config.dart` (BUG-798, BUG-1545) |
| PLY-007 | Resume: the position is set with the `start` property before `loadfile` and reset (`start=none`) after `file-loaded`. NO seek is done after loading, because `loadfile` overwrites it | BUG-1288 |
| PLY-008 | After `file-loaded`: adopt the actual `aid`; disable mpv rendering for text tracks (`sid=no`, `sub-visibility=no`); apply the saved volume and speed | `video_player_controller.dart` `load()` |
| PLY-009 | The player and the render context are **reused** from one episode to the next: only the content is recreated with `loadfile replace` | BUG-120 |
| PLY-010 | Speed uses only the `speed` property, never the `scaletempo` filter | TODO-116 |
| PLY-011 | Screenshot (`S`): the frame without subtitles goes into the `Pictures/Nikomiru` folder (`$XDG_PICTURES_DIR/Nikomiru`) as `<file name> <HH-MM-SS.mmm>.png` (a taken name gets ` (2)`, ` (3)`…, reserved at once so two screenshots never share one), and a message names the file | |
| PLY-012 | Playback errors (missing file, unsupported codec, `end-file` with an error) MUST show an understandable message and must not block the app | |
| PLY-013 | Screensaver inhibition during playback, through the XDG `Inhibit` portal (flag *idle*), kept for one minute after a pause. Nikomiru talks to the portal with its own minimal D-Bus client (`nikomiru-app::dbus`): `zbus` would add about 45 crates for three fixed calls. If the portal is not available, a warning is logged: mpv's `stop-screensaver` has no effect under the render API, because mpv owns no window | |
| PLY-014 | Thumbnails on the progress bar (M10, D-18): over the bar the pointer shows the time there and the frame, 144 px high, made by ffmpeg (`-ss` before `-i`, without sound or subtitles) on a worker thread, one at a time; a wish that comes while one is made replaces the one waiting. The bar is cut into at most 300 steps of at least 2 s; each step is made once per file, the last 120 kept. A stream has the time only. About 0.15 s a frame for 1080p HEVC 10-bit | |
| PLY-015 | Screenshot with the overlay subtitles (`Shift+S`): post-1.0, because it requires compositing the overlay onto the frame | |
| PLY-016 | **mpv option presets in `mpv.conf` format.** The mpv options set from the UI (PLY-006, PLY-018) and the raw options form one *option set*. In Settings → Player (⚙ in the bar) the user MUST be able to: (a) **save** the option set as a named preset in `$XDG_CONFIG_HOME/nikomiru/mpv/<name>.conf`, or **export** it to any file chosen in the portal's save dialog; (b) **load** an `mpv.conf` file (a preset, or any file such as the user's own `~/.config/mpv/mpv.conf`, chosen in the portal's file dialog); (c) switch between saved presets; (d) see and edit the option set **in use**: the page opens on it, as `mpv.conf` text (only what differs from the defaults) kept in step with the controls, with the preset last loaded or saved selected. While Nikomiru sets no mpv option, the user's own `$XDG_CONFIG_HOME/mpv/mpv.conf`, which mpv reads and Nikomiru does not (`config=no`, PLY-005), is offered in the preview, and the import dialog starts in its folder. Loading replaces the whole option set: options that the UI exposes fill its controls, the others go to the raw options. Accepted syntax: `key=value`, `key` (flag = `yes`), `no-key`, `--key=value`, quoted values, `#` comments, blank lines, `profile=` with a built-in profile (`fast`, `high-quality`, `gpu-hq`). Before applying, a preview lists every line as a control, a raw option, ignored (with the reason) or invalid (with what is wrong); nothing changes until the user confirms, and the values mpv then refuses are named on the page. User-defined `[sections]`, `include=` and conditional profiles are ignored with a warning. Saving writes a header comment (Nikomiru version, date) and only the options that differ from the defaults, so the file also works in mpv | |
| PLY-017 | **Options that the raw options and the presets can never set** (PLY-006, PLY-016): they are rejected with a reason and never reach mpv. (1) The fixed options of PLY-005. (2) Options that run code, load other configuration, open sockets or write files outside the XDG folders (NFR-SEC-02): `script*`, `scripts`, `load-*`, `include`, `config-dir`, `use-filedir-conf`, `input-*`, `log-file`, `dump-stats`, `stream-dump`, `stream-record`, `record-file`, `watch-later-*`, `write-filename-in-watch-later-config`, `screenshot-*` (Nikomiru owns screenshots, PLY-011), `ytdl*`. (3) Options with no effect under the libmpv render API (`gpu-api`, `gpu-context`, `vo-*`) are ignored with a warning, and so are `sub-*` options for text subtitles, which Nikomiru draws itself (§6.4). The list is a single table in `nikomiru-player`, covered by tests | |
| PLY-018 | **Render and audio settings in the UI** (Settings → Player, groups Rendering, Audio and Advanced): profile (`default`, `fast`, `high-quality`), `scale`, `cscale`, `dscale`, `deband` (with `deband-iterations` and `deband-threshold`), `dither-depth`, `interpolation` (with `tscale` and `video-sync`), `tone-mapping`, `target-peak`, `hdr-compute-peak`, `deinterlace`, `brightness`, `contrast`, `saturation`, `gamma`, `hwdec`; audio: `audio-channels`, `audio-pitch-correction`, `volume-max`, `audio-normalize-downmix`. Each control writes the same mpv option that a preset contains (PLY-016); "default" leaves the option unset. Applying the page sets the options in mpv at once, without reloading the file (SET-002), and an option the user unset goes back to the value mpv had when it started; the profile, `hwdec` and a removed raw option take effect from the next file or the next start, and the page names them. Each group has a reset to the defaults (SET-006) | |

### 6.3 Shortcuts (KEY)

| ID | Requirement |
|---|---|
| KEY-001 | All actions can be remapped in Settings → Shortcuts (a key removed with ×, one added by pressing it, Esc gives up; an action back to its defaults); the bindings are saved in `config.toml` (`[shortcuts]`), only for the actions whose keys differ from the defaults. |
| KEY-002 | Keys are recognized by their **physical key**, i.e. the scan code (winit's physical key, as a Linux evdev code), not by the produced character: shortcuts stay the same with any layout and input method (Fushi BUG-936). |
| KEY-003 | Two actions in the same context (`global`, `library`, `player`, `popup`) cannot have the same key, nor can a global action and any other: the editor refuses the key, naming the action that has it, and a file with conflicts is not saved. |
| KEY-004 | While a text field is focused, single-key shortcuts are disabled. |
| KEY-005 | A held key repeats only the actions that step (seek, volume, speed up and down, frame, chapter, cue, subtitle offset, popup entry); the others run once per press. |

Default player bindings, from Fushi `shortcut_defaults.dart`:

| Action | Keys | Milestone |
|---|---|---|
| Play/pause | Space, P, MediaPlayPause | M1 |
| Seek back / forward | ←, A / →, D | M1 |
| Previous / next subtitle | Ctrl+← / Ctrl+→ | M2 |
| Replay current / previous cue | R / Shift+R | M2 |
| Volume + / − ; mute | ↑, 0 / ↓, 9 ; M | M1 |
| Speed + / − / normal | ], = / [, - / Backspace | M1 |
| Hold for 2× speed | E | M1 |
| Previous / next frame | , / . | M1 |
| Screenshot | S | M4 |
| Fullscreen | F, F11 | M1 |
| Subtitle list / search | L / Ctrl+F | M2 |
| Blur subtitle / hide subtitle | B / H | M2 |
| Subtitles off / on (the track stays chosen) | V | M2 |
| Subtitle offset −100 / +100 ms | Z / X | M2 |
| Align the start of the previous / next cue to the position | Ctrl+Shift+← / Ctrl+Shift+→ | M2 |
| Previous / next chapter | PageUp / PageDown | M1 |
| Look up the first word of the subtitle shown; with the popup open, the lookup a character earlier / later | Enter; ← / → | M6 |
| Create card from the popup entry; the "Advanced mining" dialog | Ctrl+Enter; Ctrl+Shift+Enter | M4, M6 |
| Back (closes a dialog or a page → a popup → leaves fullscreen → returns to the library), everywhere | Esc, Alt+← | M1 |
| Settings, everywhere | Ctrl+, | M6 |

While a page or a dialog covers the video (the library, the settings, the dialogs), only the shortcuts of the global context run: the other keys drive the page's controls (Tab, the arrows, Enter, Space), with the focus always drawn (NFR-A11Y-01). The library gives the keyboard to its first card, and a series to its first episode.

### 6.4 Subtitles (SUB)

**Principle (from Fushi):** mpv does **not** draw text subtitles. Nikomiru parses them into cues and draws them in its own overlay (egui). This is what makes clicking a single character, blur, styles and mining possible. Only graphic tracks (PGS, VobSub, DVB) are drawn by mpv (`sub-visibility=yes`), without lookup.

| ID | Requirement | Fushi |
|---|---|---|
| SUB-001 | **SRT** parser: BOM, CRLF/LF, missing or out-of-order indices, `{...}` removed (BUG-105), `<i> <b> <u> <font color>` tags converted to markup, timestamps with `,` or `.` | `srt_parser.dart` |
| SUB-002 | **WebVTT** parser: header, cue settings (`line`, `position`, `align`), `<c>`, `<v>`, `<ruby>`, `<rt>` tags, cues with an identifier, `NOTE` and `STYLE` blocks ignored | `vtt_parser.dart` |
| SUB-003 | **ASS/SSA** parser (basic for 1.0): `[Script Info]` (PlayResX/Y, default 384×288), `[V4+ Styles]`, `[Events]` (Dialogue with Layer, Start, End, Style, Text), overrides `\N \n \h \an1-9 \pos \b \i \u \s \c \1c \fs \fn \r` (`\n` is a line break only with `WrapStyle: 2`, a space otherwise, as in libass; a missing PlayResX or PlayResY is derived from the other at 4:3, as in libass). Other overrides are ignored without error. Lines with `\p1` drawings are excluded from the clickable text | `ass_parser.dart`, `subtitle_markup.dart` |
| SUB-004 | Every cue has `start_ms`, `end_ms`, `plain_text` (used for lookup, mining and counts) and a separate `markup`, used only for drawing. In `plain_text` the lines of a cue are separated by `\n` (Fushi joins them with a space and keeps the break positions apart): lookup treats `\n` as a boundary, and mining (M4) joins the lines as the note type needs | |
| SUB-005 | Encoding: UTF-8/UTF-16 BOM → otherwise UTF-8 validation → otherwise try Shift_JIS (CP932), EUC-JP, GB18030, Big5, CP1252 through `iconv`, choosing the first one that decodes without errors and with the highest score of plausible characters. The detected encoding is shown with a readable name ("Japanese — Shift_JIS (CP932)") and can be forced | `text_file_io.dart` |
| SUB-006 | External files next to the video: same base name with `.srt .ass .ssa .vtt`, also with a language suffix (`.ja.srt`, `.jpn.ass`, `.jp.srt`), and the subtitles downloaded for it (MET-008). When no file has its base name, the files that belong to it by another name: those of a folder named after the video inside `Subs/`, `Sub/` or `Subtitles/` (any case of the first letter); otherwise the files of the video's episode (MET-010 parser) in its folder and in those subtitle folders; for a video without an episode, all of them when the folder holds no other video. Their language comes from a suffix or from the name (`Japanese.srt`, `[JP]`, MET-003). They are loaded automatically according to the preferred language (default `ja`), when the video opens | |
| SUB-007 | Embedded text tracks: extracted with `ffmpeg -y -i file:<video> -map 0:<ff-index> -c:s <srt\|ass\|webvtt> <cache>`. Several tracks are extracted in a single pass; if the pass fails, it is retried track by track. The cache key is (path, size, mtime, ff-index) | `video_subtitle_source.dart` |
| SUB-008 | Automatic subtitle selection: (1) the choice saved for the series; (2) an external file in the preferred language; (3) an embedded track in the preferred language (`lang` = ja/jpn, or a title that contains "Japanese" or "日本語"); (4) none | |
| SUB-009 | Overlay: several cues active at once are allowed (sorted by Layer and then by order in the file). Text is split into **graphemes** (`unicode-segmentation`); each grapheme is a hit target of the laid-out text carrying its byte offset in `plain_text` | `video_subtitle_overlay.dart` |
| SUB-010 | Default style: white text, weight 400, soft shadow, no box, 36 px relative to a video height of 1080 (scales proportionally), 75 px from the bottom; it moves up when the control bar is visible. The style options (font, size, outline, shadow, box, position) can be changed. "Respect ASS styles" (default no) uses the ASS colors, font and position | `video_subtitle_style.dart` |
| SUB-011 | Offset: positive `delay_ms` = the subtitle appears later. The compared cue time is `pos − delay_ms`; seek and mining apply the inverse. Z/X = ∓100 ms; the offset is shown in a toast | |
| SUB-012 | Navigation: "previous/next" goes to `cue.start − 180 ms`, never before the start of the previous cue, taking `delay_ms` into account (BUG-259). The target is computed from the current position with a *floor* search, not from the cached index. Without subtitles it falls back to the ±N s seek. With the keyboard, if the previous cue is more than 10 s away, a time seek is done (BUG-3339) | `video_player_controller.dart` |
| SUB-013 | "Replay cue" goes to the start of the current cue and, if triggered from the popup, pauses at the end of the cue | `replayCue` |
| SUB-014 | Blur / hidden: modes for the main subtitle (`none`, `blur`, `hidden`). Blur is a soft blur of the text, with sigma = max(12, font_size·0.45). The text is revealed on mouse hover. Blur applies only during playback and with no popup open (BUG-2198, BUG-2235). **V** turns the subtitles off and on: nothing is drawn, not even on hover, and the chosen track stays for when they come back (maintainer request after the M2 checklist) | |
| SUB-015 | Subtitle list panel (`L`): list of all cues with time, text search (`Ctrl+F`), automatic scrolling that follows playback (in gaps between cues it stays on the previous cue), click on the time to jump, click on a word for lookup | `video_subtitle_jump_panel.dart` |
| SUB-016 | Align the start of the previous or next cue to the current position (Ctrl+Shift+←/→), updating `delay_ms` | |
| SUB-017 | **Secondary** subtitle (e.g. a translation), chosen among the text sources in the subtitle menu: a second layer at 85 % of the size, at the top of the video or above the main subtitle at the bottom (`[subtitles] secondary_position`), never clicked (no lookups), with its own obscuring mode (`secondary_obscure`, revealed with the main one on hover); it follows the main offset. Its load neither cancels nor is cancelled by the main one's. Milestone M4b (D-14) | |
| SUB-018 | **Automatic alignment with the audio** ("Align with the audio" in the subtitle menu, and by itself after downloaded subtitles load, MET-007): the first 20 minutes of the active audio track decoded by ffmpeg to 8 kHz mono PCM, its RMS level every 100 ms; the bins in the loud part of the track's own range (≥ 35 %) and the cues become activity series, and the offset is the shift within ±15 s with the best Pearson correlation. It is applied as the subtitle offset (SUB-011) only when the correlation is ≥ 0.15 and at least 0.05 above the best one more than 1 s away; otherwise the menu says it is not sure and nothing changes. A new load cancels it. Milestone M4b (D-14) | `subtitle_auto_align.dart`, `audio_energy_probe.dart` |
| SUB-019 | Advanced ASS (`\t`, `\move`, `\fad`, `\clip`, karaoke `\k`, rotations): post-1.0 | |

### 6.5 Dictionaries: import and management (DIC)

#### 6.5.1 Formats

| ID | Requirement | Fushi |
|---|---|---|
| DIC-001 | MUST import **Yomitan/Yomichan** dictionaries (zip with `index.json`, format 3; format 1/2 is SHOULD): `term_bank_*`, `term_meta_bank_*` (`freq`, `pitch`, `ipa`), `kanji_bank_*`, `kanji_meta_bank_*` (`freq`), `tag_bank_*`, `styles.css`, media files | `importer.cpp` |
| DIC-002 | Unlike Fushi, MUST import the `tag_bank_*` files (category, order, notes, score) and use them for chips and for sorting. MUST keep `sequence` and `score` | Fushi gap |
| DIC-003 | Frequencies: values as a number, `{value, displayValue?}` and `{reading, frequency}` (also written `{reading, value}`, which Fushi reads too). Unlike Fushi, MUST also accept string values (reading the leading integer) and respect `frequencyMode` from `index.json` (`rank-based` default; `occurrence-based` = higher is better) | Fushi gap |
| DIC-004 | MDict, StarDict, DSL, Migaku: post-1.0 | |

#### 6.5.2 Import pipeline

| ID | Requirement |
|---|---|
| DIC-010 | Import runs in a background task with progress (files and entries) and can be cancelled; the UI stays responsive. |
| DIC-011 | ZIP safety: reject absolute paths, `..` components, symbolic links, entries > 512 MiB uncompressed, compression ratio > 200:1 on entries > 1 MiB, total uncompressed > 8 GiB. Strip the wrapper folder that holds the whole archive, and the one nested in it, until `index.json` is at the root; ignore `__MACOSX` and `.DS_Store` (BUG-2053). Support the Info-ZIP extra field for Unicode paths (BUG-3277) and ZIP64: the field (0x7075) names the entry only when the UTF-8 flag (bit 11) is clear, its version is 1, its CRC-32 is the raw name's and its name is UTF-8 and not empty; otherwise the raw name, UTF-8 or CP437. An archive whose central directory is malformed (an extra field overrunning its block) is refused as a whole, where Fushi takes the raw name. |
| DIC-012 | Limits: 10 M entries per dictionary, glossary ≤ 10 MiB per entry. If a limit is exceeded, the import fails cleanly. |
| DIC-013 | Import writes to `dicts/<id>.importing/` and, only on complete success, does an atomic rename to `dicts/<id>/` and registers the dictionary. After an error or a crash nothing partial remains; leftover `.importing` folders are deleted at startup. |
| DIC-014 | Disk full: specific message (`ERR_STORAGE_FULL`), with cleanup of partial files. |
| DIC-015 | The **type** is detected: term if there are term banks; otherwise kanji if there are kanji banks; otherwise the mode of the first meta is checked (`freq` → frequency, `pitch`/`ipa` → pitch). A term dictionary with kanji banks counts as both. |
| DIC-016 | A dictionary with the same title and the same revision as one already imported is rejected. With a different revision, an update is offered (atomic replacement). |

#### 6.5.3 Storage (one redb file per dictionary)

| Table | Key → value | Notes |
|---|---|---|
| `info` | `&str` → `&[u8]` | `index.json`, `styles.css`, schema version, counts |
| `terms` | `u64` (id) → `TermRecord` (postcard) | expression, reading, definition tags, rules, term tags, score, sequence, JSON glossary compressed with lz4 |
| `term_by_expression` | multimap `&str` → `u64` | |
| `term_by_reading` | multimap `&str` → `u64` | Only if the reading differs from the expression |
| `term_meta` | multimap `&str` → `MetaRecord` | freq / pitch / ipa |
| `kanji` | `&str` (character) → `KanjiRecord` | |
| `kanji_meta` | multimap `&str` → `MetaRecord` | |
| `tags` | `&str` → `TagRecord` | |
| `media` | `&str` (normalized path: `\` → `/`, no leading `/`) → `&[u8]` | |

- **DIC-020**: storage sits behind a `DictStore` trait; the redb implementation is the first one. Spike S2 must meet the limits of NFR-PERF-04/05; if it cannot, we switch to a custom format (mmap + hash table + blob, like fushidicts) behind the same trait.
- **DIC-021**: one file per dictionary: deleting a dictionary = closing it and deleting its folder (Fushi BUG-1756: mmap locks on Windows). The registry (order, enabled, language, aliases) lives in `library.redb`.

#### 6.5.4 Management

| ID | Requirement |
|---|---|
| DIC-030 | List of dictionaries by type (terms, kanji, frequencies, pitch accent): title, revision, declared language, import date, number of entries, size. A dictionary with several types (DIC-015) has its row under the first of them in this order; under each of its other types a reference shows where it ranks. |
| DIC-031 | Reordering by drag and drop: the order decides the position of the glossaries in the popup and the priority of the frequency dictionaries. The order is one for every type: a dictionary dropped in its type's list goes just above the row under the drop marker (or just below the list's last row), and every other dictionary keeps its place. ▲/▼ buttons do the same one row at a time, for the keyboard and screen readers. |
| DIC-032 | Every dictionary can be enabled or disabled. A disabled one is **not queried** (Fushi BUG-3454/2997: exclude it before truncation, not at render time). |
| DIC-033 | Deletion with confirmation; renaming for display only (`display_name`). |
| DIC-034 | Collapse/expand state of the dictionary in the popup: `expanded` / `collapsed` / `automatic` (only the first N are expanded). |
| DIC-035 | Per-dictionary styling: a safe subset of CSS properties taken from the dictionary's `styles.css` and from the user's CSS (color, font size, weight and style, margins, padding, borders, background), applied only to that dictionary's popup content. The subset is the one of spike S4 ([ADR 0005](adr/0005-popup-qml.md)). The user's CSS of a term dictionary is written on the dictionaries page, in a code field with Save, Revert and Clear, and applied after its `styles.css` through the same subset; the page says what the subset keeps of it: the rules applied, the properties left out and the rules ignored. |

### 6.6 Lookup (LKP)

The algorithm is a port of Yomitan's, as implemented by fushidicts. Fushi: `native/fushidicts/fushidicts_src/lookup.cpp`, `deinflector.cpp`, `text_processor/text_processor.cpp`, `scan/word_scan.cpp`.

#### 6.6.1 Text scanning

| ID | Requirement |
|---|---|
| LKP-001 | Lookup starts from the clicked grapheme. For scripts with spaces (Latin, Cyrillic, etc.) it moves back to the start of the word, without crossing an apostrophe (`l’homme` clicked in `homme` gives `homme`, as in Fushi). |
| LKP-002 | The query text goes from the starting point to the end of the cue (in subtitles) or up to 400 characters (in free text), and stops at the delimiters ``。、！？…‥「」『』（）()【】〈〉《》〔〕｛｝{}［］[]・：；:;，,.─\n\r"'“”‘’«»‹›``. A single space does not stop the scan (BUG-1773) unless whitespace or a delimiter follows it, so the query never ends with a space (`rock 'n' roll` gives `rock`); an apostrophe inside a word does not stop it either (BUG-2056). |
| LKP-003 | Normalization before the search: truncate to 2000 characters, `\n` → space, emoji → space, strip punctuation at the start and end, replace lone surrogates. |
| LKP-004 | Candidates: prefixes of the first **16** code points, from longest to shortest. Never cut between two letters of scripts with spaces; discard prefixes that end with a space; for CJK every code point is a valid cut point. |

#### 6.6.2 Preprocessors (text variants)

| ID | Requirement |
|---|---|
| LKP-010 | Every preprocessor generates a set of variants. The Cartesian product is deduplicated, keeping the minimum number of "steps" (one step for each preprocessor that changes the text). |
| LKP-011 | Chain for Japanese, in this order: NFKC → kana (unchanged / katakana→hiragana, with `ー` replaced by the vowel of the previous kana and ヵヶ kept / hiragana→katakana) → collapse of repeated っっ and ーー (**never** full collapse, BUG-1777) → ASCII → full width → kanji variants (table of 2122 entries) → expansion of the iteration marks 々ゝゞヽヾ → full-width digits → kanji numerals. |
| LKP-012 | Chains for other languages (apostrophes, lowercase, removal of diacritics and combining marks, harakat, niqqud, Korean jamo **last**, BUG-2148): they are enabled only for the configured **study language**, or for the language declared by each dictionary. This is an improvement over Fushi, which applies everything to all languages. |

#### 6.6.3 Deinflection

| ID | Requirement |
|---|---|
| LKP-020 | Data: the Yomitan tables in `assets/transforms/<lang>.json` (schema `conditions`, `transforms`, `rules` of type suffix/prefix/wholeWord, `conditionsIn`/`conditionsOut`; `phrasalVerbs` for English only). |
| LKP-021 | Conditions as bit masks, up to 64 per language; `subConditions` are expanded to a fixed point. |
| LKP-022 | The algorithm is recursive, with a maximum depth of 10. It always emits `(text, conditions, trace)`. A rule applies if `conditions == 0` or `conditions & conditionsIn ≠ 0`. For suffixes, lengths are tried from the maximum down to 1; for `wholeWord` the remaining prefix must be empty. |
| LKP-023 | Part-of-speech filter: with conditions 0 all terms are kept. Otherwise a term is kept only if the rules in its `rules` intersect the conditions. `*` = all. A dictionary with no rules in any entry treats empty rules as `*` (BUG-2543). |
| LKP-024 | The trace is kept in removal order and shown reversed (`-て « -いる « -た`), with translated names and descriptions. |

#### 6.6.4 Search, merging, sorting

| ID | Requirement |
|---|---|
| LKP-030 | For each deinflected form, all enabled term dictionaries are queried, in order, for an equal expression or reading. |
| LKP-031 | Mixed kanji+kana spelling (BUG-3261): each kanji group (at most 3 groups) is replaced with the readings found for that group (at most 16 per group, 64 combinations), and the reading index is queried. A result is accepted only if every kanji group of the headword appears literally in the query or is spelled out in full in the corresponding kana. |
| LKP-032 | Redirects: a glossary made only of `[formOf, [rules…]]` (≤ 1 KiB) is replaced with a one-level lookup of the lemma, without the part-of-speech filter; the rules are appended to the trace. Records that have a redirect label **and** a real definition are kept (BUG-2853, BUG-2566). |
| LKP-033 | Merging by (expression, reading): the longest `matched` in code points wins; on a tie, the one with fewer preprocessor steps wins; Kansai exception -た → ため/たび (BUG-3212). Glossaries are appended in dictionary order; the score is the maximum. |
| LKP-034 | Sorting, in this order: (1) longest `matched`; (2) fewer preprocessor steps; (3) shorter deinflection trace; (4) headword equal to the deinflected form; (5) frequency: for each frequency dictionary, in user order, the minimum value ≥ 0, ascending (missing ones go last); (6) **dictionary priority** (improvement: Fushi does not have it); (7) score descending; (8) expression equal to the reading. |
| LKP-035 | The `max_results` budget (default 10) counts **headword groups**, not glossary rows (BUG-1472). Frequencies are added after deduplication, pitch accent after truncation (BUG-1304). |
| LKP-036 | Grouping: key `expression\n(reading or expression)`. Inside a group, glossaries are sorted by dictionary. Frequencies deduplicated by (dictionary, values); pitch accents by (dictionary, positions, pattern, transcriptions). `best_length` = maximum `matched`, used to highlight the text. |
| LKP-037 | Kanji lookup: if the query is exactly one Han character, or by clicking a kanji of the headword in the popup. Result: character, onyomi, kunyomi, meanings, strokes, radical, statistics, frequencies. |
| LKP-038 | LRU cache of results (5000 entries, key = first 16 code points + the set of active dictionaries); it is invalidated when a dictionary changes. |

#### 6.6.5 Frequencies and pitch accent

| ID | Requirement |
|---|---|
| LKP-040 | Frequencies: one chip per dictionary `Dictionary: v1, v2` (prefers `displayValue`). Harmonic value = `floor(n / Σ 1/vᵢ)` over the first value > 0 of each dictionary; no values = `9999999`. |
| LKP-041 | Pitch accent: `pitch` mode (`position` integer or pattern, `nasal`, `devoice`) and `ipa` mode. Split into morae (small kana attach to the previous mora), high/low per mora like Yomitan's `isMoraPitchHigh`, downstep line and `[n]` label. Unlike Fushi, it also shows nasal and devoicing. |
| LKP-042 | Pitch accent category: heiban / atamadaka / nakadaka / odaka / kifuku (`v*` and `adj-i` = kifuku when the position is > 0). |
| LKP-043 | Deduplication of pitch accents across dictionaries (default yes): identical payloads become one row with the label "pitch accent · N". |

#### 6.6.6 Furigana

| ID | Requirement |
|---|---|
| LKP-050 | `distribute_furigana(expression, reading)`, like Yomitan's `distributeFurigana`/`segmentizeFurigana`: splits into kana and kanji groups and aligns the reading; if the alignment is ambiguous, the reading goes over the whole expression. |
| LKP-051 | Automatic furigana on subtitle text (`[subtitles] furigana`, default off; M9, D-17): each word found by the segmentation of LKP-060 gets the reading of its headword over its kanji, distributed over the stem the word shares with the headword (Yomitan's `distributeFuriganaInflected`: `食べた` under `食べる`/`たべる` → `た` over `食`). Drawn as WebVTT ruby, at half size; a cue with ruby of its own keeps it. Not in the subtitle list, whose rows' heights are measured once for thousands of cues. A word cut wrong can get a wrong reading, and the settings say so. |

#### 6.6.7 Segmentation

| ID | Requirement |
|---|---|
| LKP-060 | Word segmentation without a morphological tokenizer: greedy longest-match with `lookup(max_results = 1)` (like Fushi `japanese_language.dart`); spaces and punctuation are not looked up, and text no dictionary knows stays in one piece. No MeCab/IPADIC dictionary (licenses). It runs on a thread of its own, so a click never waits behind it, and its lookups bypass the cache of LKP-038. The cues shown and the next three are split ahead, once per text; a change of the dictionaries splits them again. Used by the furigana (LKP-051), the mined words (LKP-061), and to move the keyboard lookup (← →) word by word once a cue is split (grapheme by grapheme before, and where no word follows, which reaches the words inside a compound). M9 (D-17). |
| LKP-061 | The words already made into cards (`mined_cards`, ANK-039) are underlined in the subtitles and in the subtitle list (SUB-015; the rows in view are split as they come into view) (`[subtitles] mark_mined`, default on), matched by headword, so an inflected form is marked too; a card just made marks its word at once. Cards made outside Nikomiru are not known. M9 (D-17). |

#### 6.6.8 Intended differences from Fushi

Imported tag banks (DIC-002) · string frequencies and `frequencyMode` (DIC-003) · per-language preprocessors (LKP-012) · dictionary priority in sorting (LKP-034) · nasal and devoicing shown (LKP-041). Each one has a dedicated test in [TEST.md](TEST.md).

### 6.7 Dictionary popup (POP)

| ID | Requirement | Fushi |
|---|---|---|
| POP-001 | Click on a subtitle character → if the video was playing it is paused and `paused_for_lookup` is remembered → the popup opens without waiting for the pause to be confirmed. | `video_fushi_page.dart` |
| POP-002 | The matched text (`best_length` graphemes) is highlighted in the subtitle. | |
| POP-003 | Hover with Shift (movement threshold 8 px; not repeated on the same character). Option `lookup.hover_without_modifier` (default no). Hover lookup pauses like a click. If the pointer leaves the subtitle and the popup for 320 ms, the popup closes and playback resumes (`lookup.resume_on_leave`, default yes). | |
| POP-004 | When all popups are closed, playback resumes **only** if the lookup paused it (BUG-072). | |
| POP-005 | With a popup open, a click on another character of the subtitle or the list replaces the entry and stays paused. | |
| POP-006 | Position: above the subtitle if there is room, otherwise below. It never covers the looked-up word and shrinks to fit in the window. Configurable maximum size (`[popup]`, §9; 560 × 500 points by default): a popup is that wide and as tall as its content up to that height. Resizable by dragging the border: the edge away from the word, the side edges and the corners between them (the edge next to the word has no grip: moving it would cover the word). The dragged edge follows the pointer while the popup keeps its side of the word, stays centred on it and inside the window; the size it gets, within 240 × 120 and 4096 × 4096 points, holds for the next popups until the app closes; Settings → Lookup sets the size the popups start from. | `dictionary_popup_layer.dart` |
| POP-007 | Header of each entry: headword with furigana (LKP-050), ♪ audio button (AUD), create-card button (+ / ✓ / ✓↩, ANK-030), deinflection trace chips with a description on hover, frequency row, pitch accent row, term tag chips. | `popup.js` |
| POP-008 | One collapsible block per dictionary, with the collapsed state according to DIC-034. Definition tags appear as chips: numeric tags are hidden, a part-of-speech tag repeated on consecutive senses is omitted. | |
| POP-009 | Yomitan **structured content**: converted into the popup's view model (typed blocks and runs, drawn by egui) from an allowlist of tags (`br ruby rt rp table thead tbody tfoot tr td th span div ol ul li details summary img a`). The style hooks of each tag (Yomitan's `gloss-sc-<tag>` class), the `data-sc-*` values, `lang` and an allowlist of `style` properties feed the styling subset of DIC-035. **No raw HTML** from Yomitan dictionaries and no scripts. Tables in a scrollable container. | `renderStructuredContent` |
| POP-010 | Dictionary images, read from the dictionary's `media` table and decoded on the lookup thread: PNG, JPEG, GIF (first frame), WebP and BMP, recognised by their bytes; files up to 4 MiB, decoded within 8192 pixels a side and 128 MiB, then scaled down to 1280 pixels a side; at most 64 MiB of pixels per popup. The UI never loads an address from a dictionary. Respects `width`, `height`, `sizeUnits`, `imageRendering`, `title` (tooltip) and `collapsed` (the alternative text, as a link that shows the image); a missing, unreadable or too large image shows its alternative text. | |
| POP-011 | Internal links (`?query=` or the link text without `<rt>`, BUG-2456) → new lookup in a **nested popup**. External links (`http`/`https` only) → system browser through the OpenURI portal, or `xdg-open` with the URL as its only argument. | |
| POP-012 | Nested popups: select text inside a glossary and a child popup opens. Esc or Back closes the last one; a click on the parent closes the children; a click outside closes everything. Maximum depth 8. | |
| POP-013 | Kanji entries (LKP-037) shown with a dedicated card. | |
| POP-014 | Keyboard navigation: Tab/Shift+Tab between entries, Enter = audio, Ctrl+Enter = create card, Esc = close. Alt+wheel = next or previous entry. While a settings page covers the popups, their keys wait with them: Esc closes the page. | |
| POP-015 | "Replay sentence" and "go to sentence" from the popup (SUB-013). | |

### 6.8 Word audio (AUD)

| ID | Requirement | Fushi |
|---|---|---|
| AUD-001 | Audio sources in an ordered, configurable list; each one can be enabled or disabled. | `audio_source_config.dart` |
| AUD-002 | **URL template** source with `{term}` and `{reading}` URI-encoded (as `encodeURIComponent`; an empty reading is the term). The response can be Yomitan JSON `{type:"audioSourceList", audioSources:[{name,url}]}` (relative addresses resolved against the source's) or, unlike Fushi, direct audio (`Content-Type: audio/*`). Only HTTPS, or HTTP on loopback (a local audio server), as for the online subtitles; at most 8 MiB per file; through `[network] proxy` (SET-007). | `word_audio_resolver.dart` |
| AUD-003 | **Local database** source in the "Local Audio Server for Yomichan" format (`entries(expression, reading, file, source)`, `android(file, source, data)`): first the exact pair, then the expression only, with priority by sub-source (default `nhk16`, `forvo`, `jpod`, then the others; settable per source). The format is SQLite: it is read read-only by Nikomiru's own reader of the file format, with no library ([ADR 0004](adr/0004-local-audio-db.md), D-2 (d)). | `local_audio_db.dart` |
| AUD-004 | Resolution: for playback the first valid result in order is enough; the menu (right click on ♪) queries all sources in parallel and shows them all. Timeouts: 8 s for the connection, 10 s for the response. A 4xx response = "no audio"; a connection error pauses the host for 45 s. | |
| AUD-005 | Playback uses a **second audio-only libmpv instance** (`vid=no`, `video=no`, no configuration or scripts), independent of the player, with the player's audio output (`ao`) if set. Configurable volume. | |
| AUD-006 | Option "play automatically when the popup opens", default yes: it does not overlap the dialogue because the lookup already pauses the video (POP-001). It can be disabled. | |
| AUD-007 | There are no default JapanesePod101 or Forvo URLs (terms of service). The user adds their own templates. | |
| AUD-008 | On-disk cache of downloaded audio (key = URL), limited to 500 MiB with an LRU policy. | |

### 6.9 Mining to Anki (ANK)

#### 6.9.1 AnkiConnect

| ID | Requirement | Fushi |
|---|---|---|
| ANK-001 | AnkiConnect's legacy API, version 6, at one configurable URL (default `http://127.0.0.1:8765`; HTTPS only for a reverse proxy) with an optional API key. Request body: `{"action", "version": 6, "params"?, "key"?}`, also inside every `multi` child; `key` only if not empty. No proxy, no pooled connections (`Connection: close`). Timeouts: 10 s per request, 5 s for the connection ([ADR 0007](adr/0007-mining-to-anki.md)). | `ankiconnect_service.dart` |
| ANK-002 | Response: an HTTP status other than 200, a non-JSON body or a body without `result`/`error` → protocol error. A non-null `error` → error. For `addNote`, the text `cannot create note because it is a duplicate` → dedicated duplicate error. | |
| ANK-003 | Retry by the phase that failed, once: a request that failed to connect is retried whatever the action; a read-only or idempotent action is also retried when the connection closes before the reply, never after a reply timeout. `addNote` and `createModel` (and a `multi` holding one) are **not idempotent**: once sent, a failure is "unknown outcome, check in Anki", without retrying and without deleting the media uploaded. An HTTP 5xx status or a reply that is not AnkiConnect's is the same unknown outcome for them (a reverse proxy may have passed the request on). | |
| ANK-004 | Errors shown to the user, each with its own message: connection refused ("is Anki open?"), timeout (port probably taken by another program), HTTP or protocol error, wrong API key, AnkiConnect too old, no Anki profile open, deck or note type missing, invalid field mapping, empty first field, duplicate, unknown outcome, Lapis missing, an Anki editor window open (AnkiConnect 26.10), audio missing, the dictionaries changed. | |
| ANK-005 | Actions used: `version`, `deckNames`, `modelNames`, `modelNamesAndIds`, `modelFieldNames`, `addNote`, `canAddNotesWithErrorDetail`, `findNotes`, `notesInfo`, `updateNoteFields`, `addTags`, `storeMediaFile`, `getMediaFilesNames`, `deleteMediaFile`, `createModel`, `createDeck`, `guiBrowse`, `multi`. Only documented parameters are sent (AnkiConnect 25.x fails on unknown ones). | |
| ANK-006 | Refresh configuration: call `version`, `deckNames`, `modelNames` and `modelFieldNames` for the selected model. The selection is kept by name. | |

#### 6.9.2 Lapis note

| ID | Requirement |
|---|---|
| ANK-010 | Lapis templates (front, back, CSS) of the upstream tag `1.7.0` copied byte for byte into `assets/anki/lapis/`, with a `PROVENANCE.md` and their SHA-256 pinned by a test; one card template named `Mining`. Fields, in order: Expression, ExpressionFurigana, ExpressionReading, ExpressionAudio, SelectionText, MainDefinition, DefinitionPicture, Sentence, SentenceFurigana, SentenceAudio, Picture, Glossary, Hint, IsWordAndSentenceCard, IsClickCard, IsSentenceCard, IsAudioCard, PitchPosition, PitchCategories, Frequency, FreqSort, MiscInfo. |
| ANK-011 | "Create Lapis deck": `createModel` if no model named "Lapis" exists; `createDeck("Lapis")`; then refresh the configuration and check that both exist; select them and apply the default mapping. It is idempotent. Outcome: `created` (the model or the deck was new) / `already exists` / `failed`. |
| ANK-012 | Default Lapis mapping: Expression `{expression}` · ExpressionFurigana `{furigana-plain}` · ExpressionReading `{reading}` · ExpressionAudio `{audio}` · MainDefinition `{glossary-first}` · Sentence `{sentence}` · SentenceAudio `{sentence-audio}` · Picture `{card-image}` · Glossary `{glossary}` · PitchPosition `{pitch-accent-positions}` · PitchCategories `{pitch-accent-categories}` · Frequency `{frequencies}` · FreqSort `{frequency-harmonic-rank}` · MiscInfo `{source-link} {clip-timestamp}` · IsWordAndSentenceCard `x`. SelectionText stays empty (Fushi fills it with the popup's selection text, which a video card does not have). |
| ANK-013 | Lapis detection: the name contains "lapis" (case-insensitive), or the fields include Expression, MainDefinition and Sentence. |

#### 6.9.3 Field composition

| ID | Requirement |
|---|---|
| ANK-020 | Every note field has a template with `{name}` placeholders (regex `\{[^}]*\}`); an unknown placeholder becomes an empty string. The mapping is saved per note type. |
| ANK-021 | Placeholders to implement: `{expression}` `{reading}` `{furigana-plain}` `{glossary}` `{glossary-first}` `{single-glossary-<dictionary>}` `{sentence}` `{cue-sentence}` `{frequencies}` `{frequency-harmonic-rank}` `{pitch-accent-positions}` `{pitch-accent-categories}` `{phonetic-transcriptions}` `{audio}` `{sentence-audio}` `{card-image}` `{document-title}` `{source-link}` `{clip-timestamp}` `{tags}`. The HTML format of each one matches Fushi `anki_note_composer.dart` and `popup.js` (test vectors in T-ANK-02x), except that **every value from a subtitle, a dictionary, a file name or the user is HTML-escaped**, `` ` `` `$` `\` included (templates paste fields into JavaScript template literals); for a Lapis-like note the fields its scripts read through `text:` (Expression, PitchPosition) get the fullwidth forms of those characters and of `< >`, and tags lose them. `{document-title}` is the file name without extension until series exist (M5); `{tags}` the entry's term tags, comma-separated; `{audio}` is empty until word audio (M5); `{single-glossary-<dictionary>}` matches the title, then the name shown. |
| ANK-022 | `{furigana-plain}`: a kanji group becomes ` 漢字[かんじ]`, a kana group stays the same; the result is trimmed (e.g. `食[た]べ 物[もの]`). |
| ANK-023 | `{glossary}`: `<div style="text-align: left;" class="yomitan-glossary"><ol>`, then one `<li data-dictionary="D"><i>(tag, D)</i> <span>…</span></li>` per sense. The `(tag, D)` label appears only on the first sense of each dictionary, repeated part-of-speech tags are removed, disabled dictionaries and redirects are skipped. Structured content is written as Yomitan writes it (its tags, `data-sc-*` attributes, `gloss-sc-*` classes), through the allowlist of POP-009 and with inline styles limited to the CSS subset of DIC-035; images are uploaded with the card (PNG, JPEG, GIF, WebP, BMP, and SVG and AVIF, which Anki shows, recognised by their bytes: an SVG by its root element). The dictionary CSS and the user's (DIC-035) are added as a `<style>` rewritten from the rules the subset keeps and scoped to `.yomitan-glossary [data-dictionary="D"]`. |
| ANK-024 | `{sentence}`: the cue (or the merged cues, ANK-041), escaped, with its lines joined by `<br>` and the looked-up word wrapped in `<b>` at the exact position of the click offset (or at the first occurrence, if the offset does not hold the word). |
| ANK-025 | `{frequency-harmonic-rank}` as in LKP-040. `{frequencies}`: `<ul style="text-align: left;"><li>D: value</li>…</ul>`. |
| ANK-026 | Empty fields: omitted on creation. The first field of the note type cannot be empty (dedicated error). Only fields that actually exist in the note type are sent. |
| ANK-027 | Tags: user tags + `nikomiru` + `video` + cleaned-up series title (spaces → `_`, from M5, when series exist). Deduplicated, in order. |

#### 6.9.4 Mining flow from video

| ID | Requirement |
|---|---|
| ANK-030 | State of the button in the popup: `+` (can be added, or unknown), `✓` (already exists: the click opens a menu with open in Anki / add anyway), `✓↩` (last card made since Nikomiru started, wins over ✓: the same menu with overwrite first). The check runs when a popup's content arrives, for all its entries in one `canAddNotesWithErrorDetail`, on the first field rendered from its template with the headword's placeholders (`{expression}`, `{reading}`, `{furigana-plain}`; the others empty), with `allowDuplicate: false` and the same `duplicateScope` and `duplicateScopeOptions` as `addNote` (`deck`: `deckName`, `checkChildren` and `checkAllModels` true; `collection`: `checkChildren` false, `checkAllModels` true); only the exact duplicate text means ✓. If AnkiConnect is old (`unsupported action`), it falls back to `findNotes`. After a transport error or an unknown outcome of any request to that AnkiConnect there is a 30 s pause for the whole process; any answer ends it. Only a loopback URL is checked without an action of the user (PRV-001). |
| ANK-031 | The reference cue is chosen in this order: the one the lookup started from, the current one, the one found by position. |
| ANK-032 | Audio interval: `[start − pad_head, end + pad_tail]` (defaults 120 ms and 200 ms, maximum 1000 ms each), bounded by the adjacent cues, then shifted by `+delay_ms`. Maximum duration 30 s: a longer clip keeps its first 30 s. The same clip is used for the link and the timestamp (ANK-050, ANK-053). A clip of no length counts as no cue. |
| ANK-033 | Sentence audio: `ffmpeg -y -ss S -t D -i file:IN -vn -sn -map_chapters -1 -map 0:<ff-index of the active audio track> -c:a aac -ac 1 -b:a 64k out.m4a`. Quality levels (`audio_quality_level`): 0 = mono 64k (default), 1 = stereo 128k, 2 = stereo 192k. Alternative formats: Opus/OGG, MP3. An external audio track is cut from its own file (`IN`), with its ff-index in that file. |
| ANK-034 | Image: the exact frame at the moment of the click, captured by mpv (`screenshot-to-file <tmp>.png video`, a player command with a reply) asked when the card is asked for, before any later resume or seek, then resized (never enlarged) and encoded with ffmpeg. Levels: long side 720 px / q80, **1000 px / q90 (default)**, 2000 px / q95, original. Formats: JPEG (default), WebP. A failed image still makes the card. Alternative mode, with the advanced mining of M6: frame at the start of the cue. |
| ANK-035 | Audio and image are extracted in parallel. If the audio is missing, the card is cancelled with a message (as in Fushi). |
| ANK-036 | Media in Anki: content-based name `nikomiru_<first 16 hex digits of the SHA-256>.<ext>`, uploaded with `storeMediaFile` as base64 `data` (never `path`: a sandboxed Anki cannot read Nikomiru's files); the name AnkiConnect returns goes into the fields. Before uploading, `getMediaFilesNames` tells which names already exist. If `addNote` fails with a known error, the media new in that attempt are deleted; after an unknown outcome nothing is deleted. Temporary files live in `$XDG_CACHE_HOME/nikomiru/mining/`, removed however the attempt ends; at start, the ones older than ten minutes are removed. |
| ANK-037 | Overwriting the last card: `notesInfo`, rejection if the note type is different, then `updateNoteFields` with **all** fields (unmapped ones become `''`), `addTags`, and `notesInfo` again to check the write. Once `updateNoteFields` may have reached Anki, a failure is an unknown outcome: the media stay and the message says to check the card. Scope: only the last card made since Nikomiru started, the one Anki confirmed last even if its popup has closed. `mined_cards` follows. |
| ANK-038 | "Open in Anki": `guiBrowse` with `nid:<id>`; for a word, `findNotes` with Anki's duplicate search (`dupe:<model id>,<first field>` for each note type, from `modelNamesAndIds`, within the deck when the scope is `deck`) and then `guiBrowse` with the ids found. |
| ANK-039 | Every created card is recorded in `mined_cards` (note id, expression, reading, sentence, video uid, start/end, `source_id`, date) for statistics and to highlight "already mined" in the future. |
| ANK-040 | "Advanced mining" dialog (Shift+click on the card button, or Ctrl+Shift+Enter): the start and end of the clip with a preview (played from its start, paused at its end, as "Replay"), the picture (the frame of the click, saved when the dialog opens; the frame at the start of the sentence, taken from the video by ffmpeg; none), the sentence audio left out or not (a video without audio then makes a card). Esc or Cancel closes it; the popups wait under it. Milestone M6. |
| ANK-041 | In that dialog, the cue before or after the sentence added, or its first or last cue taken away again: the text a cue per line, the word still bold at its place; the clip from the first cue to the last, padded, at most 30 s. Milestone M6. |
| ANK-042 | The card's picture can be its sentence, moving (M10, D-18): `image_format = "webp-animated"` (ffmpeg's `libwebp_anim`) or `"avif-animated"` (`libaom-av1`, smaller files, slower), made with ffmpeg from the clip of ANK-032 (its pads included), at most 10 s from its start, without sound, looping. The image level of ANK-034 gives the long side and the frame rate: 480 px / 10 fps, **640 px / 12 fps (default)**, 854 px / 15 fps, 1280 px / 15 fps. Without a sentence (a card made away from any cue) or without the encoder, the still frame in WebP (for WebP) or JPEG (for AVIF). The click's frame or the advanced dialog's choice of frame do not change it; "no picture" still leaves it out. Such a card may take longer than NFR-PERF-08's 3 s: about 1–3 s for 4 s of sentence. |
| ANK-043 | WebM video clips with sound: post-1.0. |

#### 6.9.5 Return to the scene

| ID | Requirement |
|---|---|
| ANK-050 | `{source-link}` = `<a href="nikomiru://source?v=1&kind=video&uid=…&ep=…&startMs=…&endMs=…&sourceId=<uuid-v4>">Title</a>` (title HTML-escaped, the uid percent-encoded). `startMs`/`endMs` are the clip of ANK-032 in video time; `ep` is 0 until series exist (M5). Without the video's uid (no library) it is the title alone, HTML-escaped, as in Fushi. |
| ANK-051 | Parsing is strict: unknown or duplicate parameters, absolute paths and `file:` uids are rejected; maximum length 16 KiB. |
| ANK-052 | Opening the link (APP-001/002) opens the video paused at `startMs`, then, once the subtitles chosen automatically for it are loaded, at the start of the first cue that overlaps the clip, so the cue is on screen (no automatic choice, a failed load or another choice of the user: playback stays where it is). If the file cannot be found (unknown uid or moved file), a message is shown with the option "locate the file" (the portal's file chooser), which saves the new path in the video's record. |
| ANK-053 | `{clip-timestamp}` = `HH:MM:SS - HH:MM:SS` of the clip of ANK-032 (seconds truncated), only if `end > start`. |

### 6.10 Statistics (STA)

| ID | Requirement | Fushi |
|---|---|---|
| STA-001 | Watch time counts each interval of the video **only once** (interval coverage). Sampling every 1 s; a jump larger than `real_time × speed × 1.25 + 500 ms` is a seek and does not count. Coverage is saved per video: the total never exceeds the duration (BUG-2108). | `video_watch_tracker.dart`, `interval_coverage.dart` |
| STA-002 | Subtitle characters are counted once per cue, after at least `min(1500 ms, cue duration)` of actual playback in that cue. Cues already counted in previous sessions are not counted again (BUG-1763). | |
| STA-003 | Character counting rule: Han, kana, Hangul, Thai and similar scripts = 1 per code point; a sequence of Latin letters or digits = 1; combining marks and apostrophes = 0. | `study_char_count.dart` |
| STA-004 | Sessions (`study_segments`): clock with 60 s ticks, a pause > 120 s closes the session, segments split at every hour. For each segment: media, start, end, date, duration, characters, lookups, cards. | `study_clock.dart` |
| STA-005 | UI: calendar with heat map, chart by range (7/30/365 days), total per series, list of sessions with deletion. CSV export. | `video_statistics_page.dart` |

### 6.11 Settings, appearance, languages (SET)

| ID | Requirement |
|---|---|
| SET-001 | Settings page with sections: General, Player, Subtitles, Dictionaries, Lookup, Word audio, Anki, Shortcuts, Advanced (mpv options, ffmpeg paths, logs), About (version, licenses, `THIRD_PARTY_LICENSES`). `config.toml`: a key this version does not know is kept and written back; a value of the wrong kind takes its default and is named in a notice at startup (a file that is not TOML keeps the defaults in use and is never written over); saving changes only the values that changed, so a file edited by hand keeps its comments, order and layout; the write is atomic, with permissions 0600. |
| SET-002 | Changes apply immediately, without restarting, except where noted (e.g. `hwdec` takes effect from the next file, the ffmpeg program from the next start). |
| SET-003 | Light / dark / system theme (the system theme follows the desktop's `color-scheme`, read from the XDG settings portal and followed through its `SettingChanged` signal; dark when the desktop has no preference), with a configurable accent color. Design tokens in a single Rust palette (`view/palette.rs`), applied to egui's visuals: the pages and the dictionary popups follow the theme; what is drawn on the video (the bar, the subtitles, their list, the toasts) stays dark. The accent as text is made lighter or darker until it reads at 4.5:1. |
| SET-004 | i18n with Fluent (`assets/i18n/<locale>/*.ftl`): the interface takes every string from the Fluent bundle. No user-visible string literal in the code. UI language: system (`LANG`) with fallback to English; it can be changed from the settings, at once for the interface and the messages of the workers. |
| SET-005 | UI scale 75–200% (egui's zoom factor, on top of the display's scale), independent of the subtitle size. |
| SET-006 | Reset to default settings per section: the tab's keys go back to their defaults in the page, and are saved with the page. What the user has rather than how Nikomiru behaves stays: the keys and logins of the services, the AnkiConnect key, the word audio sources, the keys `config.toml` has that this version does not know. |
| SET-007 | Configurable HTTP(S) proxy (`[network] proxy`); used for all network requests except AnkiConnect on loopback. The environment's proxy variables are never used ([ADR 0008](adr/0008-online-subtitles.md)). |
| SET-008 | The settings are one page with a tab per section, the tabs down its left side, opened by ⚙ in the bar on the tab shown last (📚 opens the Dictionaries tab, the Anki button the Anki tab); Esc or Close closes it. From 0.1.0: Player (PLY-016, PLY-018), Subtitle services (MET-011) and Anki (ANK-006…ANK-013; also from the Anki button in the bar); from 0.2.0 also Word audio (AUD-001, AUD-005, AUD-006). SET-001 adds the other sections as tabs. |

### 6.12 Online subtitles and metadata (MET) — milestones M4b (subtitles, D-14) and M7 (metadata)

Online subtitles follow [ADR 0008](adr/0008-online-subtitles.md).

| ID | Requirement | Fushi |
|---|---|---|
| MET-001 | Metadata from **AniList** (GraphQL, no key): romaji/native/English title, cover, episodes, `anilist_id`. Identification from the series title obtained by LIB-003, asked with "Identify online" on the series' page (nothing goes online otherwise, PRV-001); it is accepted automatically only with an exact, unique match (letters and digits compared, case aside, against every title of a result), otherwise the user chooses among the results or none. The cover, an image checked by decoding it, becomes the series' own. "Forget" drops what was found. | — (Fushi uses Jikan/TMDB) |
| MET-002 | **Jikan v4** (MAL) as an alternative source, chosen in Settings → Subtitle services (`[online] metadata = "anilist"` or `"jikan"`): limit of 1 request/s, cache, pause after a 429 (no request until it ends). | `mal_video_metadata_provider.dart` |
| MET-003 | **Jimaku**: `https://jimaku.cc/api`, header `Authorization: <key>`, sent only to the Jimaku host. `GET /entries/search?query=<title>&anime=true`, then `anime=false` when nothing is found (and `anilist_id=` first once M7 identifies series), `GET /entries/{id}/files` for the first two entries (all files: season packs carry no episode), download of each file's `url`; zip archives opened with the rules of DIC-011 (names only, ≤ 64 MiB unpacked, ≤ 1000 entries); RAR and 7z listed but not usable. Episode and language are inferred from the file name; a file of unknown language counts as Japanese. | `jimaku_client.dart` |
| MET-004 | **OpenSubtitles** (user's consumer API key, header `Api-Key`, `User-Agent: Nikomiru/<version>`; optional user name and password for `POST /login`): `GET /subtitles` with `moviehash` + `moviebytesize`, then `query` + `season_number`/`episode_number`, always with `languages=<study language>`; `POST /download {file_id}` gives a temporary HTTPS link; files ≤ 64 MiB; machine-translated results marked. | `open_subtitles_client.dart` |
| MET-005 | API keys (and the OpenSubtitles password) are saved in the config with permissions 0600 and never written to a log (the keyring is in the ROADMAP backlog). | |
| MET-006 | File identification with AniDB ed2k: post-1.0 (requires registering an AniDB client). | `anidb_ed2k.dart` |
| MET-007 | **Automatic subtitles** (`[online] auto_subtitles`, default off, PRV-001): when an opened video has no text subtitles in the preferred language (none next to it or downloaded, no embedded text track in that language) and a provider has a key, the best online file (ADR 0008 §9: the preferred language, or unknown on Jimaku; the episode asked for, or a film; never machine-translated) is downloaded and loaded, unless the user chose subtitles meanwhile; a toast names the file and the provider. One attempt per opening. | `subtitle_batch.dart` |
| MET-008 | Downloaded subtitles are kept in `$XDG_DATA_HOME/nikomiru/subtitles/<hash>/<video name>.<language>.<ext>`, `<hash>` being the OpenSubtitles hash of the video, never next to the video (NFR-SEC-02); they are offered like files next to it (SUB-006), so the next opening needs no network. | |
| MET-009 | **Online subtitles page** ("Search online…" in the subtitle menu): title, season and episode filled from the file name and editable, "Search", results (provider, file, language, episode, notes: machine translated, archive, not usable) ranked by ADR 0008 §9, "Use" downloads and loads one; "Services and keys…" opens Settings → Subtitle services (MET-011). HTTP 429 shows when to try again. | |
| MET-011 | **Subtitle services in the settings** (Settings → Subtitle services, SET-008): the services that search for subtitles, ticked or not, in the order preferred (`[online] providers`, default `["jimaku", "opensubtitles"]`; one left out is never asked, MET-007 included); the order breaks the ties of the ranking (ADR 0008 §9). On the same tab: each service's key (Jimaku; OpenSubtitles key, user name, password), the automatic subtitles (MET-007) and the proxy (SET-007), saved atomically with permissions 0600 (MET-005). | |
| MET-010 | File names are parsed Anitomy-style for the search (title, season, episode): bracketed blocks and technical tokens dropped; episode from `SxxEyy`, `第N話`, `EP N`, `#N`, ` - N` or a trailing number of two digits or with a leading zero; season from `SxxEyy`, `Season N`, `S N`, `Nth Season`, `第N期/季`, or the parent folder. LIB-003 (M5) reuses the parser. | `video_filename_parser.dart` |

---

### 6.13 Media servers (SRV) — milestone M11 (D-20)

Jellyfin and Plex: their videos are watched as local ones are, with the same subtitles, lookups and cards. Fushi has no counterpart.

| ID | Requirement |
|---|---|
| SRV-001 | **Servers in the settings** (Settings → Media servers, SET-008): any number of servers, each added, renamed, checked and removed by hand; none by default, and with none nothing changes. Removing one logs out and keeps what the library knows of its videos (progress, statistics, cards). |
| SRV-002 | **Jellyfin login**: address, user and password; `POST /Users/AuthenticateByName` with the `Authorization: MediaBrowser Client=…, Device=…, DeviceId=…, Version=…` header (`DeviceId` random, one per installation). The access token and user id are saved; the password never is. An expired token (401) asks for the password again. |
| SRV-003 | **Plex login**: the plex.tv PIN flow: `POST https://plex.tv/api/v2/pins?strong=true`, the sign-in page opened in the browser (no password goes through Nikomiru), `GET /api/v2/pins/{id}` every 2 s for at most 10 min until the token comes, cancellable; then `GET /api/v2/resources` lists the account's servers and their addresses, tried local first. `X-Plex-Client-Identifier` as `DeviceId` above. The token goes only to plex.tv and that account's servers. |
| SRV-004 | **Secrets and privacy**: tokens are kept in the config like API keys (MET-005) and never appear in a log, a command line, an mpv property or a URL given to another program (PRV-002). No request reaches a server until the user has added it, and then only while its page is open or one of its videos plays (PRV-001). Each request carries only that server's token (a redirect to another host drops it). |
| SRV-005 | **Connections** (exception to ADR 0008 §6, for these servers only): HTTPS always; plain HTTP without asking only to loopback or a private, link-local or CGNAT address (a home network, Tailscale); to any other address only with "Allow an unencrypted connection" ticked on that server, which warns that the password and token travel in clear. Certificates are checked against the system's store. `[network] proxy` applies (SET-007), except to loopback and private addresses. |
| SRV-006 | **Library**: a tab for each server on the library page, with the libraries of type series or movies chosen in its settings (all by default), series → seasons → episodes, and movies. Covers, titles and watched, in-progress and new marks come from the server; requests are paged (100 items); covers are cached in `$XDG_CACHE_HOME/nikomiru/servers/`, at most 500 MiB, oldest out first; the rest is asked again each time the tab opens. Search by title in the tab. Server episodes also appear in "Continue watching" (LIB-005), not in the series grid, and a rescan never touches them (LIB-004). |
| SRV-007 | **Identity**: a server's video has the uid and path `srv:<server id>:<item id>`, so the `videos`, statistics and `mined_cards` records work as for a local file, and the table `server_items` tells where it is. A server that gives a video a new id is a new video: nothing is matched across ids. |
| SRV-008 | **Playback**: only direct play: the original file, never a transcode, from `GET /Videos/{id}/stream?static=true` on Jellyfin and the part's `key` on Plex. mpv and ffmpeg read it through the **relay** (SRV-009). A video the network cannot carry stutters, and that is not handled. Next episode (LIB-009) follows the server's order within the season. Everything that reads the file works on the relay's address: card clips, frames, thumbnails (PLY-014), embedded subtitles, the alignment (SUB-018), with the timeouts of EXT-003 doubled, since the file comes over a network. |
| SRV-009 | **Relay**: a thread pool of `nikomiru-online`, on 127.0.0.1 with a free port, alive only while a server video is open: `GET`/`HEAD` with `Range` under an unguessable path (128 random bits, new for each video) are forwarded with the token added, and nothing else is answered. It stops reading from the server when the client leaves, and takes at most 8 connections at once. Its address, not the token, is what appears in ffmpeg's command line (EXT-002 lets only this address through without `file:`). |
| SRV-010 | **Subtitles**: the server's external subtitles are offered with those next to a file (SUB-006), fetched as text (`/Videos/{id}/{source}/Subtitles/{index}/Stream.<format>` on Jellyfin, the stream's `key` on Plex) and cached like downloads (MET-008, `<hash>` being the uid's SHA-256); embedded ones come through the relay like a file's. Image subtitles stay unsupported as in local files. The online search (MET-009) and MET-007 use the title, season and episode from the server and, on Jellyfin, its AniList id when given; never the file hash (MET-004), so no download of the file's head and tail. |
| SRV-011 | **Progress**: at opening, the later of the two positions (the server's by its `LastPlayedDate` or `lastViewedAt`, the local by `last_played_at`), then LIB-007. Reported to the server on start, every 10 s while playing, on pause and on stop (`/Sessions/Playing*` on Jellyfin, `/:/timeline` on Plex), on the `servers` worker so that playback never waits; a report that fails is dropped, and only the last one is retried, at the next tick. At 90% (LIB-008) the video is marked played on the server. Statistics stay local. |
| SRV-012 | **Cards**: a card from a server's video is made as from a file. Its `{source-link}` (ANK-050) carries the uid: opening it plays the scene from that server; with the server gone or unreachable, a message names it and nothing else happens. |
| SRV-013 | **Errors**: unreachable server, 401, 403, 404 and a video gone each have their own translated message (ERR-001); answers are bounded (`ANSWER_LIMIT`), parsed as untrusted data and never rendered as markup (NFR-SEC-01); titles and covers from a server go through the limits of the covers (M8 security review). A server failing during playback stops at what mpv buffered, with the message, and keeps the position. |

### 6.14 Backup and sync (BAK) — milestone M12 (D-21)

Fushi has no counterpart to follow: its sync goes through other services.

| ID | Requirement |
|---|---|
| BAK-001 | **Backup to a file** (Settings → Backup): a `.nikomiru-backup` file, a ZIP with `manifest.json` (format, Nikomiru's version, schema, date), `config.toml`, `library.redb`, the dictionaries' registry, the mpv presets and the downloaded subtitles. Dictionaries are optional, being large, and off by default. `library.redb` is a copy taken inside one read transaction, so a backup made during playback is consistent. |
| BAK-002 | **Secrets in a backup**: keys, passwords and tokens (MET-005, SRV-004) are left out of `config.toml` unless "Include keys and passwords" is ticked, which warns that the file then holds them in clear. The file is written with permissions 0600 either way. |
| BAK-003 | **Restore**: a chosen backup shows its date, version and contents, and asks for confirmation; it is unpacked beside the data, and the next start of Nikomiru moves what is there into `$XDG_DATA_HOME/nikomiru/before-restore/` (only the latest is kept) and puts the backup in place, before anything opens the files. Keys left out of the backup keep their current values. A backup from a newer schema is refused (DAT-002); one from an older schema is migrated (DAT-001). Paths in the archive are checked: nothing is written outside the data folders (NFR-SEC-02), with sizes bounded. |
| BAK-004 | **WebDAV** (Settings → Backup, `[sync]`): an address, a user and a password (an app password on Nextcloud), kept like the API keys (MET-005). `PROPFIND`, `MKCOL`, `PUT`, `GET` on the client in use, with Basic authentication; addresses follow SRV-005. Nothing connects until the user has set it up (PRV-001); "Check" says whether the folder can be written. |
| BAK-005 | **What syncs**: the progress of videos, the watched time and intervals, cue credit, study sessions and mined cards, by uid (DAT-003: the same file name on two computers is the same video; server videos by their `srv:` uid). Not synced: settings, keys, folders, series, dictionaries, subtitles. Series, grouping and paths belong to a computer. |
| BAK-006 | **One file per device**: each computer writes only `nikomiru/devices/<device id>.sync` on the server, and reads the others', so no two computers ever write the same file and no lock is needed. Each file holds that device's records with a format version, compressed; a file of a newer format is skipped with a message, and one damaged is skipped without touching the library. |
| BAK-007 | **Merge**: per record, never by overwriting a whole file. A video's position: the one played last (`last_played_at`), the further one on a tie. Watched: the earliest `completed_at`. Intervals: the union. Cue credit: the union for the same subtitle file; for two different ones, one of them, the same on every device. Study sessions and mined cards: the union by id, the later version of the same one. Merging twice changes nothing, and the order of the devices changes nothing. |
| BAK-008 | **Deletions**: a study session deleted (STA-005) or a video removed (LIB-012) leaves a tombstone, kept 180 days and synced, so that it does not return from another device. |
| BAK-009 | **When**: by hand with "Sync now"; with "Sync at start and close" (off by default) also then, on the `servers` worker's pattern: a worker of its own, so nothing waits for it, at most 30 s at close. Failures are a message, never a loss: the library is written only after everything was read and merged. |
| BAK-010 | **Errors**: unreachable, refused, full (507), a server that lies about its files: each its own message (ERR-001); answers are untrusted and bounded (64 MiB for a device's file). |

## 7. Non-functional requirements (NFR)

**Reference hardware** (REF): x86_64, 4 cores / 8 threads (2020 or later), integrated Intel or AMD GPU with VA-API, 16 GB of RAM, NVMe SSD, 1920×1080 display at 60 Hz, Wayland session (GNOME or KDE).

| ID | Requirement | Measure (see TEST) |
|---|---|---|
| NFR-PERF-01 | Cold start until the library is visible ≤ 1.5 s on REF | T-PERF-01 |
| NFR-PERF-02 | Opening a local file until the first frame ≤ 1.0 s (h264/hevc, 1 GB mkv file) | T-PERF-02 |
| NFR-PERF-03 | 1080p60 H.264 and 10-bit HEVC playback with the overlay visible (subtitles + controls): **0 dropped frames** in 5 minutes (`frame-drop-count`, `vo-delayed-frame-count`), process CPU ≤ 25% of one core with hwdec active | T-PERF-03 (exit criterion of spike S1) |
| NFR-PERF-04 | Engine lookup (scan of 16 code points, 5 term dictionaries + 3 frequency + 1 pitch accent) p95 ≤ 10 ms, p99 ≤ 25 ms | T-PERF-04 |
| NFR-PERF-05 | Import of a dictionary with 300k entries / 150 MB JSON ≤ 60 s, peak RAM ≤ 1 GiB | T-PERF-05 |
| NFR-PERF-06 | From the click on the character to the drawn popup ≤ 100 ms p95 | T-PERF-06 |
| NFR-PERF-07 | A cue appears within ≤ 50 ms of its actual `start` | T-PERF-07 |
| NFR-PERF-08 | Card created (5 s audio + image) ≤ 3 s from the click, with a local Anki | T-PERF-08 |
| NFR-PERF-09 | Idle RAM (RSS) with the library open ≤ 400 MB | T-PERF-09 |
| NFR-PERF-10 | A line split into words (LKP-060) with the dictionaries of NFR-PERF-04: p95 ≤ 50 ms | T-PERF-10 |
| NFR-REL-01 | No crash and no data loss if the app is closed during: import, config write, progress save, mining | T-REL-001, T-REL-002 |
| NFR-REL-02 | A corrupted or malicious video file, subtitle or dictionary does not crash the app (parser fuzzing) | T-FUZZ-* |
| NFR-SEC-01 | Dictionary contents and subtitle text are untrusted data: the interface lays them out as literal text from a typed view model (allowlisted blocks and runs, POP-009); no markup is ever parsed from them for display, never raw HTML or scripts; the interface never loads a URL from them (images are decoded from the dictionary's own media, POP-010; external links open only `http`/`https` in the browser, POP-011) | T-SEC-* |
| NFR-SEC-02 | No file written outside Nikomiru's XDG directories and those chosen by the user (screenshots, exports) | T-SEC-03 |
| NFR-A11Y-01 | The whole UI is usable from the keyboard, with focus always visible; AA contrast in the default themes; AccessKit roles and names on the controls (AT-SPI on Linux) | T-A11Y-*, T-E2E-A11Y-001 |
| NFR-PORT-01 | Works on Wayland and X11; the build needs no network apart from `cargo fetch` | T-PORT-001, T-PORT-002 |
| NFR-MAINT-01 | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean; `///` documentation on every `pub` item of the library crates | T-MAINT-001 |
| NFR-MAINT-02 | Line coverage ≥ 80% for `nikomiru-text`, `nikomiru-subs`, `nikomiru-dict`, `nikomiru-anki` (measured with `cargo llvm-cov`, MIT/Apache license) | T-MAINT-002 |

---

## 8. Data model (library.redb)

| Table | Key | Value (main fields) |
|---|---|---|
| `meta` | `&str` | `schema_version` (2 from 0.2), `created_with` |
| `folders` | `&str` path | an imported folder (LIB-002): grouping mode, added on, last scan |
| `series` | `u64` | title, grouping key (the series name in lower case, or the folder's path), grouping mode, folder, added on, deleted (tombstone, for LIB-004), `merged_into?` (LIB-011), cover, preferences (audio language, subtitle, `delay_ms`; LIB-010) |
| `series_info` | `u64` series id | what AniList or Jikan said of it (MET-001, M7): source, `anilist_id?`, `mal_id?`, romaji, English and native titles, episodes, year, format, page, identified on; a table of its own, so the `series` records stay as they were |
| `videos` | `&str` uid | `series_id?`, path, size, mtime, season, episode, duration, `last_position_ms`, `last_played_at`, `completed_at`, chosen audio, main and secondary `SubtitleChoice`, `delay_ms?`, `speed?`, `volume?`, `moved_to?` (moved by hand, LIB-011), removed (LIB-012) |
| `watch_coverage` | `&str` uid | merged `[(start_ms, end_ms)]` intervals |
| `cue_credit` | `&str` uid | bitset of the indices of cues already counted + hash of the subtitle file |
| `study_segments` | `u128` uuid | media uid, start, end, `date_key`, hour, duration, characters, lookups, cards |
| `mined_cards` | `u64` note id | expression, reading, sentence, video uid, start, end, `source_id`, date (a new table: the schema version does not change, [ADR 0007](adr/0007-mining-to-anki.md)) |
| `dictionaries` | `&str` id | title, revision, type, order, enabled, language, `display_name`, collapse state, user CSS, counts |
| `subtitle_cache` | key from SUB-007 | path of the extracted file, format, encoding |
| `server_items` | `&str` uid | server id, kind (Jellyfin or Plex), item id, media source or part, series and episode titles (SRV-007; a new table: the schema version does not change, and older versions see these `videos` rows as missing files) |

`SubtitleChoice = File(path) | Embedded { ff_index, lang, title } | Off` (a downloaded file is a `File`, MET-008)

A scan of a folder keeps what the library knows (LIB-004): a file is found again by its path, or, moved, by its name, size and time; a series by its grouping key, so a renamed one keeps its name, a merged one gathers its episodes where it was merged, a deleted one is not made again; an episode moved by hand stays where it was put, a removed one stays out. Changing a folder's grouping hides the series of the other grouping without deleting them, so changing back loses nothing.

---

## 9. Configuration (excerpt of `config.toml`)

```toml
[general]
ui_language = "system"      # "it", "en", …
theme = "system"            # "light" | "dark" | "system" (SET-003)
accent = "#6aa0ff"          # the accent color, #rrggbb (SET-003)
ui_scale = 100              # the interface scale in percent, 75–200 (SET-005)
study_language = "ja"
first_steps = true          # the library's first steps, until done or hidden (LIB-013)

[player]
hwdec = "auto-safe"
quality_profile = "default" # "default" | "high-quality"
seek_seconds = 3
auto_next = true
raw_mpv_options = ""
mpv_preset = ""             # last loaded or saved preset, mpv/<name>.conf (PLY-016)

[player.mpv_options]        # options set from the UI or loaded from a preset (PLY-016, PLY-018)
# deband = "yes"
# scale = "ewa_lanczos"

[subtitles]
preferred_languages = ["ja", "jpn"]
font_size = 36
respect_ass_styles = false
obscure = "none"            # "none" | "blur" | "hidden"
secondary_obscure = "none"  # the same for the secondary subtitle (SUB-017)
secondary_position = "top"  # "top" | "bottom" (above the main subtitle)
furigana = false            # readings over the kanji, from the dictionaries (LKP-051)
mark_mined = true           # underline the words already made into cards (LKP-061)

[lookup]
max_results = 10
hover_without_modifier = false
resume_on_leave = true

[popup]                     # the largest dictionary popup, in points (POP-006)
max_width = 560             # 240–4096; never wider than the window
max_height = 500            # 120–4096; a popup is as tall as its content, up to this

[anki]
url = "http://127.0.0.1:8765"
api_key = ""
deck = "Lapis"
note_type = "Lapis"
duplicate_scope = "deck"    # "deck" | "collection"
audio_pad_head_ms = 120
audio_pad_tail_ms = 200
audio_format = "aac-m4a"    # "aac-m4a" | "opus-ogg" | "mp3"
audio_quality_level = 0     # 0 = mono 64k, 1 = stereo 128k, 2 = stereo 192k
image_format = "jpeg"       # "jpeg" | "webp" | "webp-animated" | "avif-animated" (ANK-042)
image_quality_level = 1     # 0..3
tags = []

[anki.field_mapping.Lapis]
Expression = "{expression}"
Sentence = "{sentence}"
# …

[paths]
ffmpeg = ""                 # empty = search in PATH

[network]
proxy = ""                  # "http://host:port" or "socks5://…"; empty = none (SET-007)
device_id = ""              # random, made for the first media server (SRV-002, SRV-003)

[word_audio]                # AUD-001…AUD-008
autoplay = true             # AUD-006
volume = 100                # AUD-005, 0–100
# [[word_audio.sources]]    # none by default (AUD-007), asked in order (AUD-001)
# name = "Local"
# url = "http://localhost:5050/?term={term}&reading={reading}"   # or the path of an android.db (AUD-003)
# enabled = true
# priority = ["nhk16", "forvo", "jpod"]                          # a database's sub-sources, in order

[online]                    # ADR 0008
providers = ["jimaku", "opensubtitles"]  # the services used, in the order preferred (MET-011)
auto_subtitles = false      # MET-007
jimaku_api_key = ""
opensubtitles_api_key = ""
opensubtitles_username = ""
opensubtitles_password = ""
# jimaku_url, opensubtitles_url: other base URLs, for the tests

[sync]                      # WebDAV (BAK-004), none by default
url = ""
user = ""
password = ""               # like the API keys (MET-005)
allow_unencrypted = false   # SRV-005
automatic = false           # at start and close (BAK-009)

# [[servers]]               # media servers (SRV-001), none by default
# id = "…"                  # random, for the uids (SRV-007)
# kind = "jellyfin"         # or "plex"
# name = "Home"
# url = "http://192.168.1.10:8096"
# other_urls = []           # Plex: the account's other addresses
# token = ""                # like the API keys (MET-005)
# user_id = ""
# libraries = []            # the ones shown; empty = all (SRV-006)
# allow_unencrypted = false # SRV-005
```

---

## 10. Decisions and risks

### 10.1 Maintainer decisions

Decisions D-1…D-12 approved by the maintainer on 2026-10-19 (D-7…D-9 after the M0 spikes, D-10 after spike S1q), D-13 on 2026-10-20 after spike S5, D-14 on 2026-10-25. A decision is reopened only through an explicit change to this table.

| ID | Decision | Outcome |
|---|---|---|
| D-1 | Class B exceptions (§3.2), needed to use Dioxus desktop | **Approved**: closed list; CI blocks any addition. Superseded by D-10: the exceptions forced by Dioxus are gone (§3.2) |
| D-2 | Reading the local audio DB (SQLite, public domain) | **Reopened, (d) proposed** (2026-10-25): spike S3 ([ADR 0004](adr/0004-local-audio-db.md)) found `turso` fast enough (p95 under 1 ms) but its graph outside the license policy (BSD-3-Clause `subtle`, Unicode-3.0 ICU collation, both unconditional). Proposed and implemented: (d) Nikomiru's own read-only reader of the SQLite file format, no dependency, no exception. Was: (a) `turso` (MIT), to be validated in S3; (b) an exception for the system SQLite library; (c) dropping AUD-003 |
| D-3 | GPL-3.0-only or GPL-3.0-or-later | **Approved**: GPL-3.0-only (§3.1) |
| D-4 | Default audio format for cards | **Approved**: AAC in M4A, compatible with Anki desktop, AnkiDroid and AnkiMobile |
| D-5 | Packages for 1.0 | **Approved**: PKGBUILD (AUR) + Flatpak (**freedesktop runtime**, with libmpv and ffmpeg included in the manifest; runtime updated by D-10, then by D-13: the KDE runtime and Qt are no longer needed) |
| D-6 | Name of the app, the folder and the repository | **Approved**, then superseded by D-11: the app was called **Anikoto**, a name that turned out to be taken |
| D-7 | The `cargo deny` check on the real graph (M0, 2026-10-19) finds 5 class B crates missing from the §3.2 table: `yoke-derive`, `zerofrom-derive`, `zerovec-derive` (Unicode-3.0) and `const_format_proc_macros`, `konst_macro_rules` (Zlib). They are procedural macros of the already approved families (`yoke`, `zerofrom`, `zerovec`, `const_format`, `konst`), with the same license and the same requester. `foldhash` and `zlib-rs`, on the other hand, do not appear in the Linux graph | **Approved**: the 5 crates are in the §3.2 table and in `deny.toml`; `foldhash` and `zlib-rs` stay, since they may reappear with other versions or platforms. Superseded by D-10: only the Unicode-3.0 macro crates remain |
| D-8 | The `router` feature of `dioxus` (§4) brings in `base16` 0.2.1, which is **CC0-1.0** (forbidden, §3.2), through `dioxus-router-macro` | **Obsolete (D-10)**: no Dioxus, no `base16`. CC0-1.0 is forbidden with no exception (§3.2) |
| D-9 | Language of documents and code comments | **Approved**: documents and code comments are in English (§0). For consistency, commit messages and developer-tool messages (`cargo xtask`) are in English too. The UI stays localized (SET-004) |
| D-10 | UI toolkit | **Approved**: Qt 6 Quick (QML) through cxx-qt instead of Dioxus ([ADR 0003](adr/0003-qt-quick-ui.md)). On the developer's 4K, 120 Hz, fractional-scale session no frame was dropped, where the Dioxus design dropped 2456 in 60 s; the class B table shrinks to the ICU/Unicode family and `foldhash`. Superseded by D-13: the UI is egui; ADR 0003's findings on mpv (render context before loading, native display, shutdown order) still apply |
| D-11 | Name of the project | **Approved**: the app, the crates (`nikomiru-*`), the binary, the URL scheme (`nikomiru://`), the XDG folders and the GitHub repository (`nikomiru`, created at the first push) are called **Nikomiru**, because "Anikoto" was already taken. Git history before the rename keeps the old name |
| D-12 | Direction after 1.0 | **Approved** (2026-10-19): books (EPUB), manga and online resources are a planned direction after 1.0, not a permanent exclusion. Until 1.0 the focus is video with clickable subtitles and the dictionary explanation of the clicked word, in the overlay and in the subtitle list (SUB-009, SUB-015, M3) |
| D-13 | UI toolkit, after spike S5 (iced and egui against Qt Quick) | **Approved** (2026-10-20): egui 0.37 on winit + glutin (EGL first) + egui_glow instead of Qt 6 Quick ([ADR 0006](adr/0006-toolkit-comparison.md)). Under heavy load at 4K HEVC 10-bit egui drops 0–2 frames in 20 s like Qt, with about half of its CPU; the popup draws a whole entry in one frame; everything is Rust (no QML, C++ or cxx-qt). Class B exception `libloading` (ISC) approved with it (§3.2). The Flatpak of D-5 no longer needs the KDE runtime: the freedesktop one has what egui needs |
| D-14 | Priorities after M4 | **Approved** (2026-10-25): online subtitles (search by hand and automatic, [ADR 0008](adr/0008-online-subtitles.md)), their alignment with the audio (SUB-018), the secondary subtitle (SUB-017) and the mpv option presets in `mpv.conf` format with their settings page (PLY-016, PLY-018) come right after M4, as milestone M4b, before the library (M5) |
| D-15 | Contents of 0.1.0 | **Approved** (2026-10-25): since neither is released, 0.1.0 ships M4 and M4b together, with the maintainer's notes of 2026-10-25: the subtitle services chosen in the settings (MET-011, SET-008), the Player page opening on the `mpv.conf` in use (PLY-016 d), the local subtitles of another name opened with the video (SUB-006). The tag waits for the maintainer's check of the manual exit criteria. The logo in `assets/brand/` is approved too |
| D-16 | Other platforms | **Asked by the maintainer** (2026-10-25, with 0.4.0): builds for macOS, Windows and Android. Recorded as the milestone "Ports" after M7, which starts with spike S6; the release that ships each port follows from S6. Before: Windows and macOS after 1.0, Android excluded (§1.3) |
| D-17 | The next version, while 1.0.0 waits for its manual checks | **Asked by the maintainer** (2026-10-26): work goes on with the next version while M8's manual checks are open. Recorded as M9 "Words in the subtitles": the segmentation of LKP-060 in the subtitles, automatic furigana (LKP-051, until now post-1.0) and the words already mined underlined (LKP-061), on by default except the furigana. Planned for 1.1.0; merged into `main` while M8's manual checks wait, so it ships with 1.0.0 |
| D-18 | After M9 | **Proposed** (2026-10-26, the maintainer asked to go on; to change at will): M10 "Moving pictures", merged into `main` while M8's manual checks wait, so it ships with 1.0.0: the card's picture as the moving sentence (ANK-042) and thumbnails on the progress bar (PLY-014), both made by the system's ffmpeg with no new dependency. WebM clips with sound (ANK-043) stay in the backlog |
| D-19 | Order after 1.0 | **Asked by the maintainer** (2026-10-30): M11 Jellyfin and Plex (1.1.0), M12 backup, restore and WebDAV sync (1.2.0), M13 advanced ASS (SUB-019) and screenshots with subtitles (PLY-015) (1.3.0), M14 clipboard lookup and texthooker (1.4.0), M15 MDict, StarDict and DSL (DIC-004, 1.5.0), M16 books, manga and audiobooks (D-12, 2.0.0); then the rest of the backlog. Each milestone's requirements are written here before its work starts; fixes from the manual checks of 1.0.0 go out as 1.0.x meanwhile. The ports (D-16) run alongside |
| D-20 | Design of M11 | **Approved** (2026-10-31): (a) direct play only, no transcoding; (b) a loopback relay, so that tokens never reach mpv or ffmpeg and one path handles the proxy, the limits and the redirects; (c) a tab per server, not mixed in the series grid, with only "Continue watching" shared; (d) plain HTTP allowed without asking only on private addresses; (e) self-signed certificates not supported in 1.1.0 (use HTTP on a home network, or a real certificate); (f) the later position wins, no merge of the watch history; (g) the server's data is not cached for offline use, except covers. Live checks need a real Jellyfin and the maintainer's Plex: no container runtime on the development machine |
| D-21 | Design of M12 | **Approved** (2026-10-31): (a) one file per device instead of a shared file, so no locks; (b) settings, series and dictionaries are not synced, only what the user did: progress, statistics, cards; (c) a backup leaves keys out unless asked; (d) no encryption of the files on the WebDAV server in 1.2.0: they hold what was watched and studied, on a server of the user's own choosing; (e) by hand by default, automatic only if ticked |

### 10.2 Risks

| ID | Risk | Prob. | Impact | Mitigation |
|---|---|---|---|---|
| R-1 | Video and UI composition in the same window (§5.2) does not render smoothly | Low (D-13) | High | egui composition measured in [ADR 0006](adr/0006-toolkit-comparison.md): at 4K HEVC 10-bit under load 60 renders/s and 0–2 dropped frames, like Qt Quick in [ADR 0003](adr/0003-qt-quick-ui.md) (the Dioxus design of [ADR 0001](adr/0001-video-composition.md) dropped frames at 4K). Residual: video and interface share the UI thread, so long work on it delays frames (§5.2.1); OpenGL is required (§5.2.3); the egui build still has to be measured on the developer's session and on REF |
| R-2 | egui and winit are still 0.x: API changes between versions | Medium | Medium | Pinned versions; the toolkit only in `nikomiru-ui`/`nikomiru-app` (ARC-010); upgrades only through a dedicated PR |
| R-3 | A new version of egui, winit, glutin or reqwest adds dependencies with new licenses | Medium | Medium | `cargo deny` in CI; updates only through a dedicated PR |
| R-4 | System ffmpeg without encoders (e.g. `aac` missing in reduced builds) | Low | Medium | EXT-001: check at startup, fallback to Opus/MP3, clear message |
| R-5 | redb performance not sufficient for lookup | Low | Medium | Spike S2; `DictStore` trait and a custom format as plan B |
| R-6 | AnkiConnect changes or removes `canAddNotesWithErrorDetail` | Low | Low | Fallback to `findNotes` (ANK-030) |
| R-7 | The size of Fushi pushes toward widening the scope | High | High | §1.3 is binding; every new feature goes through a change to this document |
| R-8 | ~~Distributions ship Qt older than 6.5~~ | — | — | Obsolete (D-13): no Qt |
| R-9 | winit's gaps on Linux: no Wayland drag and drop, no window activation on Wayland, the desktop's colour scheme not reported; no client-side decorations without a further class B exception (GNOME shows no title bar) | Medium | Medium | Own Wayland data device (§4); XDG portals through the own D-Bus client; revisit with each winit release |

---

## 11. Glossary

| Term | Meaning |
|---|---|
| Cue | A single subtitle line, with a start and an end |
| Lookup | Dictionary search starting from a point in the text |
| Deinflection | Bringing an inflected form (食べました) back to the dictionary form (食べる) |
| Mining | Creating an Anki card from a word encountered, with its context |
| Lapis | Very common Anki note type for Japanese (donkuri/lapis) |
| AnkiConnect | Anki add-on (code 2055492159) that exposes a local HTTP API |
| Structured content | Yomitan JSON format for rich glossaries (tables, ruby, images) |
| hwdec | mpv hardware video decoding |
| Spike | Time-boxed prototype to reduce a technical risk; the code is thrown away |
