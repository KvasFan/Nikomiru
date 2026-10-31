# Nikomiru — Roadmap

| Field | Value |
|---|---|
| Status | Draft v0.1 |
| Date | 2026-10-19 |
| Related documents | [SPECIFICATION.md](SPECIFICATION.md) · [TEST.md](TEST.md) |

The roadmap splits the work into **milestones** (M0…M8). Each one has a goal, the requirements it covers (IDs from [SPECIFICATION.md](SPECIFICATION.md)), the deliverables and verifiable **exit criteria** (IDs from [TEST.md](TEST.md)).

There are no dates: a milestone is closed when all its exit criteria pass, not when a deadline expires. The sizes (S/M/L/XL) only give an idea of the relative effort.

Possible status of a milestone: `not started` · `in progress` · `done`. When a milestone becomes `done`, `cargo xtask trace` requires every one of its requirements to have at least one test (see TEST §8).

## Overview

```
M0 Foundations + spikes ──► M1 Player ──► M2 Subtitles ──► M4 Anki mining ══► 0.1.0 (MVP)
        │                                   ▲                   ▲
        └──► M3 Dictionaries and lookup ────┴───────────────────┘
                (pure logic: can proceed in parallel with M1/M2)

0.1.0 ◄── M4b Online subtitles + mpv presets (D-14, D-15)
      ──► M5 Library + word audio (0.2.0) ──► M6 Statistics, settings, i18n (0.3.0)
      ──► M7 Online metadata (0.4.0) ──► M8 Stabilization ══► 1.0.0
                                         ▲
      M9 Words in the subtitles (D-17) ──┤ (merged while M8's manual
      M10 Moving pictures (D-18) ────────┘  checks wait: they ship with 1.0.0)

1.0.0 ──► M11 Jellyfin, Plex (1.1.0) ──► M12 Backup, WebDAV (1.2.0)
      ──► M13 Advanced ASS, screenshots (1.3.0) ──► M14 Clipboard, texthooker (1.4.0)
      ──► M15 MDict, StarDict, DSL (1.5.0) ──► M16 Books, manga, audiobooks (2.0.0)
      ──► the rest of the backlog
      (Ports, from spike S6, run alongside: D-16)
```

| Milestone | Title | Size | Release | Status |
|---|---|---|---|---|
| M0 | Foundations and spikes | M | — | done |
| M1 | Minimal player | M | — | in progress |
| M2 | Own subtitle rendering | L | — | done |
| M3 | Dictionaries and lookup | XL | — | in progress |
| M4 | Mining to Anki | L | **0.1.0** (MVP alpha) | released (manual checks open) |
| M4b | Online subtitles and mpv presets (D-14) | M | **0.1.0** with M4 (D-15) | released (manual checks open) |
| M5 | Library and word audio | L | 0.2.0 | released (manual checks open) |
| M6 | Statistics, settings, i18n, advanced mining | M | 0.3.0 | released |
| M7 | Online metadata | M | 0.4.0 | released |
| Ports | macOS, Windows and Android (D-16), from spike S6 | L | after S6 | not started |
| M8 | Stabilization and packages | M | **1.0.0** | released (manual checks open) |
| M9 | Words in the subtitles (D-17) | M | **1.0.0** with M8 | released (maintainer's check open) |
| M10 | Moving pictures (D-18) | M | **1.0.0** with M8 | released (maintainer's check open) |
| M11 | Jellyfin and Plex (D-19) | L | 1.1.0 | not started |
| M12 | Backup, restore and WebDAV sync (D-19) | M | 1.2.0 | not started |
| M13 | Advanced ASS and screenshots with subtitles (D-19) | L | 1.3.0 | not started |
| M14 | Clipboard lookup and texthooker (D-19) | M | 1.4.0 | not started |
| M15 | MDict, StarDict and DSL dictionaries (D-19) | L | 1.5.0 | not started |
| M16 | Books, manga and audiobooks (D-12, D-19) | XL | 2.0.0 | not started |

---

## M0 — Foundations and spikes

**Goal**: set up the workspace and CI, and **remove the two biggest technical risks before writing features**.

**Requirements**: ARC-010, ARC-011, NFR-MAINT-01; license policy (SPECIFICATION §3, §3.5).

**Deliverables**

1. Workspace as in SPECIFICATION §5.1: empty crates with `lib.rs`, `GPL-3.0-only` SPDX headers, `#![forbid(unsafe_code)]` where required.
2. `rust-toolchain.toml`, centralized `[workspace.dependencies]`, `Cargo.lock` under version control, `clippy.toml` (`unwrap_used`, `expect_used` as errors in libraries).
3. `deny.toml` with Class A, Class B exceptions only for the crates listed in SPECIFICATION §3.2, `bans` (no duplicate versions of heavy crates without a reason) and `sources` (crates.io only).
4. `about.toml` and generation of `THIRD_PARTY_LICENSES.html`.
5. `xtask` with the commands:
   - `ci`: fmt, clippy, deny, test;
   - `fixtures`: generates the test media and dictionaries (TEST §4);
   - `check-headers`: checks the SPDX headers;
   - `trace`: requirements ↔ tests matrix;
   - `about`.
6. CI pipeline (service to be chosen when the repository is published) that only calls `cargo xtask ci` and `cargo xtask ci --full`.
7. **Spike S1 — video composition** (SPECIFICATION §5.2): Dioxus window with GtkOverlay(mpv GLArea + transparent webview), an HTML button over the video, a fake HTML subtitle that updates on every `time-pos`. After the 4K revision of ADR 0001, **spike S1q** repeats it with Qt 6 Quick: mpv rendered on Qt Quick's render thread, QML overlay, Rust through cxx-qt (D-10).
8. **Spike S2 — dictionary storage**: import of a synthetic 300k-entry dictionary into redb and lookup with a minimal deinflector.
9. For each spike, an ADR in `docs/adr/` (`0001-video-composition.md`, `0002-dictionary-storage.md`, `0003-qt-quick-ui.md`) with measurements, decision and chosen fallback. The spike code does not go into `crates/`: it is rewritten cleanly in later milestones.

**Exit criteria**

- [x] `cargo xtask ci` green on the empty workspace (T-LIC-001, T-ARC-010, T-ARC-011, T-ARC-020); the canary T-LIC-002 (`cargo xtask check-licenses --canary`) confirms that a forbidden license makes CI fail.
- [x] S1 meets **all** of these points on REF, both with Wayland (GNOME or KDE) and with X11:
  - 1080p60 H.264 and HEVC 10-bit for 5 minutes with hwdec, 0 dropped frames and CPU ≤ 25% of one core (T-PERF-03);
  - clicks and keys reach the UI over the video (with D-10: the QML overlay);
  - resizing, fullscreen and HiDPI 2× without artifacts;
  - 50 player opens and closes without RSS growth above 10%.

  If S1 fails, the ADR picks a fallback (B/C/D of SPECIFICATION §5.2.3) and this roadmap is updated before moving to M1.

  On 2026-10-19 the maintainer moved the X11 T-PERF-03 measurement to the M1 exit criteria, because it cannot be measured reliably with nested XWayland.

  Status as of 2026-10-19: S1 (Dioxus/GTK) met the criteria at 1080p on an idle machine but failed at 4K under CPU load: 14 renders/s and 912 dropped frames in 20 s ([ADR 0001](adr/0001-video-composition.md)). The maintainer chose Qt 6 Quick (D-10), and spike S1q meets the criteria ([ADR 0003](adr/0003-qt-quick-ui.md), accepted). Automatic measurements in a virtual KWin, unless stated otherwise:
  - [x] Wayland, T-PERF-03: at 1080p and at 4K under heavy CPU load, 0–2 dropped frames with H.264 and HEVC 10-bit (VA-API without copies); process CPU 14–15%.
  - [x] The developer's real session (KDE Wayland, 4K, 120 Hz, fractional scale 2.7): no dropped frames (2026-10-19).
  - [x] 50 opens and closes without RSS growth.
  - [x] REF, integrated GPU (Radeon 610M) at 1080p: 0 dropped frames with H.264 and HEVC 10-bit.
  - [x] X11 in the virtual KWin with EGL (`QT_XCB_GL_INTEGRATION=xcb_egl`, zero-copy VA-API): 0 dropped frames. The measurement on a real Xorg session stays in M1.
  - [x] By hand, on the real session: clicks and keys on the overlay, resizing and fullscreen without artifacts, sharpness at fractional scale (confirmed by the developer, 2026-10-19). The only remark, the `Cannot load libcuda.so.1` warning, is fixed by SPECIFICATION §5.2.2 point 10.
- [x] S2: import ≤ 60 s and ≤ 1 GiB of RAM (T-PERF-05); lookup p95 ≤ 10 ms (T-PERF-04). If it fails, the ADR adopts the custom format (DIC-020). — Passed: import of 300k entries in 2.3 s with 86 MiB; lookup p95 2.4 ms warm and 4.7 ms cold. redb is adopted ([ADR 0002](adr/0002-dictionary-storage.md)).
- [x] D-1 (Class B exceptions) and D-3 (GPL-3.0-only) approved by the maintainer (2026-10-19).
- [x] D-7 (5 Class B procedural macros that emerged from the real graph) and D-8 (CC0 exception for `base16`, brought in by the Dioxus `router` feature) approved by the maintainer (2026-10-19), together with D-9 (documents and comments in English). D-10 (Qt 6 Quick, driven from Rust through cxx-qt, instead of Dioxus; [ADR 0003](adr/0003-qt-quick-ui.md)) approved by the maintainer (2026-10-19): it supersedes D-7 and D-8.

**Progress notes** (2026-10-19)

- Deliverable 6 (CI pipeline) waits for the repository to be published: for now CI runs locally with `cargo xtask ci`.
- The automatic S1 and S1q measurements run in an off-screen virtual KWin (`spikes/s1-composition/run-virtual.sh`, `spikes/s1q-qt/run-virtual.sh`): on the session in use, the window opened behind the others and, being covered, did not draw (see ADR 0001).
- D-10 replaced Dioxus with Qt 6 Quick ([ADR 0003](adr/0003-qt-quick-ui.md)). The code of the spikes stayed in `spikes/` until 1.0.0 and is in the tag `v1.0.0`.

---

## M1 — Minimal player

**Goal**: open a file and watch it with full keyboard and mouse controls.

**Requirements**: LIB-001, LIB-006 (position saving per single file), PLY-001…PLY-010, PLY-012, PLY-013, PLY-017, ARC-001…ARC-004, KEY-001…KEY-004 (M1 shortcuts of SPECIFICATION §6.3), APP-001, EXT-001, ERR-001, ERR-002, PRV-001, PRV-002, DAT-001…DAT-003.

**Deliverables**

- `nikomiru-player`: `PlayerHandle`, `PlayerEvent`, mpv options (PLY-005/006), resume with `start` (PLY-007), reuse from one file to the next (PLY-009), parser for raw mpv options with the table of blocked options (PLY-017), the video's render context (first the Qt Quick video item of [ADR 0003](adr/0003-qt-quick-ui.md); with D-13 the libmpv render API driven from the egui frame, and a command thread).
- `nikomiru-app`: the application window (Qt until D-13; now winit + glutin with EGL + egui_glow) instead of the GTK compositor of ADR 0001, single instance via socket, opening from an argument or drag & drop.
- `nikomiru-ui`: player page with control bar (progress, time, volume, speed, audio tracks, fullscreen) and toasts (QML until D-13, now egui).
- Minimal `config.toml`, with atomic writes.
- `library.redb` with only the `videos` table.
- cxx-qt enters the workspace: `deny.toml` turns `unused-license-exception` back to `warn`.

**Exit criteria**

- [x] T-PLY-001…T-PLY-010, T-PLY-012, T-PLY-013, T-KEY-001…T-KEY-004, T-APP-001 green (T-PLY-011 goes with PLY-011 in M4).
- [x] GUI harness: opens a fixture, play/pause/seek from the keyboard, closes, reopens and resumes from the saved position (T-E2E-001).
- [x] T-PRV-001 under `strace` (2026-10-26: a whole session reaches for no address but AnkiConnect's on loopback).
- [ ] Manual checklist TEST §9, "Player" section, completed on Wayland and X11.
- [ ] T-PERF-03 on a real X11 session (Xorg), with EGL (glutin prefers EGL on X11, [ADR 0006](adr/0006-toolkit-comparison.md)): 1080p60 H.264 and HEVC 10-bit for 5 minutes with hwdec, 0 dropped frames and process CPU ≤ 25% of one core (moved from M0, [ADR 0001](adr/0001-video-composition.md)). If it fails, the maintainer decides between a documented tolerance on X11 and moving X11 support after 1.0.

---

## M2 — Own subtitle rendering

**Goal**: text subtitles drawn by Nikomiru, clickable per grapheme, synchronized and navigable.

**Requirements**: SUB-001…SUB-016, EXT-002, EXT-003, M2 shortcuts.

**Deliverables**

- `nikomiru-subs`:
  - basic SRT, VTT and ASS parsers;
  - model `Cue { start_ms, end_ms, plain_text, markup }`;
  - index for binary search;
  - offset;
  - encoding detection via iconv (FFI in `nikomiru-app::linux` or in a small dedicated `unsafe` module, ARC-011).
- `nikomiru-media`: ffmpeg and ffprobe runner with timeout and cancellation (EXT-002/003); extraction of embedded tracks with a cache.
- `nikomiru-ui`:
  - grapheme overlay (QML items; with D-13 drawn by egui);
  - default style and "respect ASS";
  - blur and hide;
  - list panel with search;
  - prev/next/replay navigation;
  - Z/X and alignment to the playhead.

**Exit criteria**

- [x] T-SUB-001…T-SUB-016 green, including the Fushi conformance vectors (TEST §5) for the parsers (67 of 84 cases apply to 1.0; the others are recorded divergences).
- [x] T-FUZZ-001…T-FUZZ-003 (parsers) without panics for 10 minutes each (12 million cases each, 15–18 minutes, 2026-10-19).
- [x] T-PERF-07: cue visible within ≤ 50 ms of its start.
- [x] Manual checklist "Subtitles" with at least 3 real files: Shift_JIS SRT, fansub ASS with styles, MKV with 2 embedded tracks ([docs/qa/m2-subtitles.md](qa/m2-subtitles.md), 2026-10-19).

---

## M3 — Dictionaries and lookup

**Goal**: import Yomitan dictionaries and do lookups with the same quality as Fushi, popup included.

**Requirements**: DIC-001…DIC-003, DIC-010…DIC-016, DIC-020, DIC-021, DIC-030…DIC-035, LKP-001…LKP-050, POP-001…POP-015, NFR-SEC-01.

**Work tracks** (`nikomiru-text` and `nikomiru-dict` do not depend on the UI and can start in parallel with M1):

1. **Spike S4 — dictionary popup in QML**: Yomitan structured content (ruby, table, list, details, image, links) rendered as QML items from the tag allowlist, the CSS subset of DIC-035, and the click-to-popup time (NFR-PERF-06); ADR `0005-popup-qml.md`.
2. `nikomiru-text`: preprocessors (LKP-010…012), kana conversion, iteration marks, kanji variants (table from Fushi), sentence extraction, furigana (LKP-050), character count (STA-003, needed in M6).
3. `nikomiru-dict`:
   - safe (DIC-011), atomic (DIC-013) and cancellable Yomitan import;
   - `DictStore` on redb;
   - deinflector on the `assets/transforms` tables;
   - scanning, merging, sorting and grouping;
   - frequencies, pitch accent, kanji;
   - LRU cache.
4. `nikomiru-ui`:
   - dictionary management;
   - popup (structured content turned into a view model from an allowlist of tags, the style subset of DIC-035, images from the dictionary's media; drawn with QML until D-13, now with egui);
   - nested popups;
   - pause/resume state machine (POP-001/004/005);
   - hover with Shift.
5. **UI on egui** (D-13, [ADR 0006](adr/0006-toolkit-comparison.md)). Qt 6 Quick, QML, cxx-qt and the C++ video item give way to egui 0.37 on winit + glutin (EGL first) + egui-winit + egui_glow. mpv draws the video with the libmpv OpenGL render API into the window's framebuffer, under the interface, on the UI thread; the player's commands move to a command thread so that this thread never waits for the core. The Rust logic (session, subtitles, popup state, dictionaries page, shortcuts, i18n) and the harness protocol (TEST §2.1) stay, and with them the L6 tests; the requirements are those of M1–M3. In order:
   - the window, the video and the control bar (the behaviour of M1);
   - the subtitle overlay and the subtitle list (M2);
   - the popup, with a typed view model and images decoded on the lookup thread, and the dictionaries page (M3);
   - the harness, drag and drop on Wayland, file dialogs through the XDG portal;
   - removal of Qt, cxx-qt and the C++ code; specification, test plan and packages updated (the Flatpak runtime becomes the freedesktop one).

**Status as of 2026-10-20**

- Done: Yomitan import (safe, atomic, cancellable; T-DIC-001…003, 010…015, T-SEC-002), redb storage with term heads in the indexes and a Bloom filter per dictionary, deinflector on the `assets/transforms` tables, lookup (merging, sorting with dictionary priority, budget in groups, redirects, mixed orthography, Kansai exception, frequencies, pitch accent, kanji, LRU cache), registry in `library.redb` (order, enabled, update by revision, deletion, cleanup after a crash), the popup (spike S4, [ADR 0005](adr/0005-popup-qml.md)) with pause/resume, nesting, placement and highlighting, `--import-dictionary` on the command line.
- Conformance: 168 text vectors, 243 lookup vectors and 39 import vectors from Fushi, green except 4 import vectors marked as known failures: Unicode Path fields that are malformed or that APPNOTE says to ignore (bit 11 set, unknown version, a record overrunning its block, an empty name), which the `zip` crate applies or refuses where Fushi keeps the raw name.
- Measured: T-PERF-04 p95 1.7 ms / p99 2.2 ms, T-PERF-05 ≤ 4.4 s and 261 MiB per 300k-entry dictionary, T-PERF-06 p95 21 ms; with Jitendex and JMdict, click → popup 20–64 ms in a release build (ADR 0005).
- Also done: the dictionaries page (import with progress and cancel, update offer, order, enable, rename, popup block state, delete), hover with Shift (POP-003), Tab/Shift+Tab/Alt+wheel between entries (POP-014; Enter and Ctrl+Enter wait for audio in M5 and cards in M4), replay and go to sentence (POP-015), `[lookup]` settings, library writes off the UI thread.
- Done on 2026-10-20: selection inside glossaries opens a nested popup (POP-012), the popup resized by its border with its maximum size in `config.toml` (POP-006), the dictionaries page grouped by type with drag and drop between positions (DIC-030, DIC-031) and the user's CSS per dictionary edited on it (DIC-035), registry edits in one transaction, imports cancelled promptly and waited for at exit, T-SEC-001 on the real popup.
- Done on 2026-10-26: the 4 Unicode Path known failures: the importer reads the names from the central directory by Fushi's rules; three pass, the fourth (an overrunning field) is a declared DIC-011 divergence. T-PERF-04 (p95 9.7 ms, p99 21.6 ms) and T-PERF-05 within their thresholds on 2026-10-26, T-PERF-06 in every `ci --full`.
- Open: the manual test with real dictionaries.
- UI on egui (D-13, 2026-10-20): decided after spike S5, which measured iced and egui against Qt Quick ([ADR 0006](adr/0006-toolkit-comparison.md)). Work track 5 done on 2026-10-20: window, video and control bar; subtitle overlay and list; popups from a typed view model with images decoded on the lookup thread; dictionaries page; file dialogs through the XDG portal; drag and drop on Wayland with the app's own data device (L6 with a real drag from another client); Qt, QML, cxx-qt and the C++ code removed. Still to check on the developer's session: fractional scale, Japanese IME, Orca, T-PERF-03 at 4K under load.

**Exit criteria**

- [x] T-DIC-*, T-LKP-*, T-POP-* green, including **all** the conformance vectors ported from the fushidicts tests (TEST §5.1): no known failure left, the divergences declared (2026-10-26).
- [x] T-PERF-04, T-PERF-05, T-PERF-06 within the thresholds.
- [x] T-SEC-001 (dictionary content produces only allowlisted view-model nodes) and T-SEC-002 (malicious ZIPs) green.
- [x] UI on egui (D-13): the L6 suite green on the egui build; no Qt, cxx-qt or C++ left in the workspace (T-ARC-010, T-ARC-011).
- [ ] UI on egui, by hand: T-PERF-03 at 4K HEVC 10-bit under load, on Wayland and on X11 with EGL; on the developer's session: fractional scale, Japanese IME (fcitx5, ibus), AccessKit with Orca, 50 opens and closes ([ADR 0006](adr/0006-toolkit-comparison.md)).
- [ ] Manual test with JMdict (English), JMnedict, a frequency dictionary (e.g. JPDB) and a pitch accent dictionary (e.g. NHK) imported by the user: the 50 words in the list of TEST §9.3 give the same headword in Nikomiru and in Fushi.

---

## M4 — Mining to Anki → release 0.1.0 (MVP)

**Goal**: from the popup, a complete Lapis card in one click.

**Requirements**: ANK-001…ANK-006, ANK-010…ANK-013, ANK-020…ANK-027, ANK-030…ANK-039, ANK-050…ANK-053, APP-002, PLY-011 (simple screenshot), Ctrl+Enter shortcut.

**Deliverables**

- `nikomiru-anki`: AnkiConnect client (retry and duplicates as in the specification), field composer, vendored Lapis templates (`assets/anki/lapis/` with `PROVENANCE.md`), `nikomiru://` links.
- `nikomiru-media`: sentence audio and image (mpv screenshot → ffmpeg).
- UI: Anki settings (connection, deck, note type, mapping editor, "Create Lapis deck", connection test), button states + / ✓ / ✓↩, menu for the existing card.
- `.desktop` with `x-scheme-handler/nikomiru`.
- **Repository page for the first release**: `README.md` restyled after [storytold/cadcraft](https://github.com/storytold/cadcraft), from top to bottom:
  - centered logo with light and dark variants (`<picture>` with `prefers-color-scheme`), centered `<h1>` title, a bold one-line tagline and a short centered paragraph;
  - one centered row of shields.io badges in `style=flat-square`: written in Rust, runs on Linux (Wayland · X11), license GPL-3.0-only, status alpha (accent colour for the status badge, two colours for the rest);
  - a full-width hero screenshot (player with clickable subtitles and the dictionary popup open) with a `<sub>` caption, then one centered line of section links separated by `·`;
  - "Screenshots": a two-column `<table>` (popup with pitch accent and frequencies, card created in Anki, subtitle list, settings) with `<sub>` captions;
  - "Why Nikomiru": a bullet list with bold lead-ins;
  - "What works today": an honest `| Area | Status |` table that matches the ROADMAP;
  - "Quick start" (build from source, AnkiConnect setup), "Architecture" (crates and SPECIFICATION §5), "Roadmap" (link), "Downloads" (per-format table; PKGBUILD and Flatpak arrive with 1.0, D-5), "License and credits" (GPL-3.0-only, derived from Fushi, Yomitan, hoshidicts and Lapis, `THIRD_PARTY_LICENSES.html`).

  Constraints: the logo and icons are original GPL-3.0-only works in `assets/brand/` (SPECIFICATION §3.4). Screenshots show only project-made media and the project's test dictionaries (`mini-ja`): no anime frames (copyright), no Creative Commons media, no content from third-party dictionaries. They are taken with the GUI harness in an off-screen compositor, so they can be regenerated for every release.

**Status as of 2026-10-21**

- Decided in [ADR 0007](adr/0007-mining-to-anki.md): the split between `nikomiru-anki` and the `anki` worker, AnkiConnect's legacy API over reqwest without proxy or pooled connections, retries by phase, a fake AnkiConnect of our own for the tests, Lapis 1.7.0 from the upstream tag, escaping and a Yomitan-shaped glossary, media always uploaded as base64, "session" as the run of the app, `mined_cards` as a new table.
- Done: the AnkiConnect client and its fake (T-ANK-001…005), Lapis 1.7.0 vendored and "Create Lapis deck" (ANK-010…013), the field composer with the Yomitan-shaped glossary, its images and the scoped CSS of the subset (ANK-020…027), the sentence audio and the card image (ANK-033, ANK-034; the player's commands with a reply, also for `S`, PLY-011), the + / ✓ / ✓↩ buttons with their menu and Ctrl+Enter, the `anki` worker (checks, mining, overwrite, rollback, Open in Anki), `mined_cards`, the `[anki]` settings and their page, `nikomiru://source` links with "Locate the file" (ANK-050…053), `assets/nikomiru.desktop` and `cargo xtask install` (APP-002). Every M4 requirement has its tests.
- Measured: T-PERF-08, Ctrl+Enter → `card-created` against the fake AnkiConnect with the real ffmpeg and mpv, 276 ms (limit 3 s).
- Open: T-E2E-ANKI-001 against a real Anki (`cargo xtask test --anki-e2e`, needs Anki with AnkiConnect, not installed on the development machine), the manual "Mining" checklist with real episodes and AnkiDroid, the tag.
- Release tasks done on 2026-10-25: the workspace at 0.1.0, the CHANGELOG, the repository page restyled with the logo (`assets/brand/`, approved with D-15) and screenshots from the GUI harness (`docs/images/`), `THIRD_PARTY_LICENSES.html` from `cargo xtask ci --full`.

**Exit criteria**

- [x] T-ANK-* green on the fake AnkiConnect ([ADR 0007](adr/0007-mining-to-anki.md)).
- [ ] T-E2E-ANKI-001 (real Anki in Xvfb with AnkiConnect, run nightly or by hand) green.
- [x] T-PERF-08 ≤ 3 s (276 ms against the fake AnkiConnect).
- [ ] Manual checklist "Mining": 10 cards created from 2 real episodes, checked in Anki desktop and after syncing to AnkiDroid (audio, image, bold word in the sentence, return link that reopens the scene).
- [x] **Release 0.1.0** (2026-10-25, tag `v0.1.0`): tag, CHANGELOG, `THIRD_PARTY_LICENSES.html`, build instructions, repository page restyled as described in the deliverables, with 0.1.0 screenshots.

---

## M4b — Online subtitles and mpv presets → 0.1.0 with M4 (D-15)

Decision D-14 (2026-10-25): what the maintainer needs right after mining, brought forward from M6 and M7. D-15 (approved 2026-10-25): neither M4 nor M4b was released, so 0.1.0 ships both.

**Requirements**: MET-003, MET-004, MET-005, MET-007…MET-011, SUB-017, SUB-018, SET-007, SET-008, PLY-016, PLY-018.

**The maintainer's notes for the release (2026-10-25)**

- Choosing the subtitle services belongs to the settings, not to the search page: MET-011 on the Subtitle services tab of the settings (SET-008).
- The `mpv.conf` configuration opens with the one in use loaded: PLY-016 (d).
- A video opens with the matching local subtitles of its folder, also when their names differ from its own: SUB-006.

**Deliverables**

- Crate `nikomiru-online` ([ADR 0008](adr/0008-online-subtitles.md)): Jimaku and OpenSubtitles clients behind one trait, the file-name parser (MET-010), language detection, the OpenSubtitles hash, ZIP archives, the ranking; a fake server for the tests.
- The `online` worker; the Online subtitles page ("Search online…" in the subtitle menu); automatic subtitles (MET-007); downloaded subtitles kept in the data folder and offered like files next to the video (MET-008).
- The settings as one page with tabs (⚙, SET-008): Player, Subtitle services (the services used and their order, keys, automatic subtitles, proxy; MET-011) and Anki.
- Settings → Player: the option set in use as `mpv.conf` text and as render and audio controls (PLY-018), and mpv option presets in `mpv.conf` format: save, load with a per-line preview, switch, import and export through the file dialogs, the user's own `mpv.conf` offered (PLY-016).
- Local subtitles of another name opened with the video: same episode, `Subs/` folders (SUB-006).
- Alignment of the subtitles with the audio (SUB-018): from the subtitle menu, and by itself after downloaded subtitles load.
- The secondary subtitle (SUB-017), e.g. a translation under the Japanese one.

**Exit criteria**

- [x] T-MET-003, T-MET-004, T-MET-007…T-MET-011 green on the fake server; T-PLY-016, T-PLY-018, T-SET-008, T-SUB-006 green (every `ci --full`, 2026-10-26).
- [ ] By hand with real keys (T-MET-LIVE-001): Japanese subtitles found and loaded automatically for 5 real episodes from 3 series.
- [ ] Alignment: on 5 real pairs (downloaded subtitle + embedded track) the residual error is ≤ 150 ms.

---

## M5 — Library and word audio → 0.2.0

**Requirements**: LIB-002…LIB-005, LIB-007…LIB-012, AUD-001…AUD-008, spike **S3** (decision D-2).

**Deliverables**

- `nikomiru-library`: folder import, file name parser (port of `video_filename_parser.dart` with its vectors), series and episodes, tombstones, per-series preferences, automatic advance.
- Home: "Continue watching", series grid, series detail, generated covers.
- Word audio: URL templates, Yomitan JSON response or direct audio, second audio-only libmpv instance, LRU cache.
- **Spike S3**: read-only access to a real `android.db` with `turso` (MIT). Criteria: query (expression, reading) p95 ≤ 20 ms; no license outside Class A/B in the graph. ADR `0004-local-audio-db.md` confirms D-2, or reopens it if the criteria are not met. Done: reopened, own reader proposed.

**Status as of 2026-10-25**

- Done: the library (LIB-002…LIB-012): schema 2 with series and folders, the import by series or by folder with Fushi's grouping vectors, scans that keep progress, choices and the user's edits, the covers, the Library page ("← Library" in the bar, Esc over the video, and at start without a file), the next episode with its countdown, the start rule, the watched state, the series' choices. The file-name parser moved to `nikomiru-text`.
- Done too: word audio from URL templates (AUD-001, AUD-002, AUD-004…AUD-008): sources in the settings (a Word audio tab), ♪ and its menu in the popup, playing by itself, a second audio-only mpv core, an LRU cache.
- Spike S3 done ([ADR 0004](adr/0004-local-audio-db.md), proposed): on a synthetic `android.db` of 300 000 entries `turso` is fast enough but its graph is outside the license policy; Nikomiru reads the SQLite file format itself, and AUD-003 works through it (a source of the Word audio tab can be an `android.db`, with its sub-sources in order).
- Open: the maintainer's decision on ADR 0004 (D-2 (d)); the import of a real library of ≥ 200 episodes by hand.

**Exit criteria**

- [x] T-LIB-*, T-AUD-* green.
- [ ] Import of a real library of ≥ 200 episodes in ≤ 30 s (without online metadata), with the correct groupings checked by hand.
- [x] **Release 0.2.0** (2026-10-25, tag `v0.2.0`, at the maintainer's request; the manual check above and the decision on ADR 0004 follow it).

---

## M6 — Statistics, full settings, i18n, advanced mining → 0.3.0

**Requirements**: STA-001…STA-005, SET-001…SET-006, ANK-040, ANK-041, NFR-A11Y-01.

The proxy setting and the mpv presets with their settings page moved to M4b (D-14).

**Status as of 2026-10-25**

- Done: statistics (STA-001…STA-005), ported from Fushi with its vectors: the watch tracker with interval coverage, per-cue credits, the session clock by local hour, the Statistics tab of the Library page with its chart, calendar, totals by series, sessions and CSV export.
- Done: the full settings (SET-001…SET-006): eleven tabs down the side of one page; `config.toml` keeps its comments and takes a wrong value as its default with a notice; reset per tab; light, dark and system theme (the desktop's portal) with an accent, the tokens checked for contrast; the language and the interface scale changed at once; the shortcut editor with conflict checks (KEY-001…KEY-004).
- Done: the `it`/`en` review, with `cargo xtask check-i18n` in the CI (T-I18N-001, T-I18N-002): every key used, every key asked for present, no untranslated text in the views.
- Done: the advanced mining dialog (ANK-040, ANK-041): Shift+click or Ctrl+Shift+Enter, lines added to the sentence, the clip with a preview, the picture and the audio chosen.
- Done: the keyboard-only tour (NFR-A11Y-01): pages take the keys no global shortcut takes, Enter and ← → look words up, Ctrl+, opens the settings, the library's first card has the keyboard with a drawn focus; T-E2E-A11Y-001 goes from the library to a card and back with keys only, and T-A11Y-001 reads the controls through AT-SPI.
- Released as 0.3.0 on 2026-10-25, after the maintainer's checks.

**Deliverables**

- Watch tracker (interval coverage, per-cue credits), session clock, statistics page, CSV export.
- Full settings page, shortcut editor with conflict checks, light/dark/system themes, interface scale.
- Fluent with complete `it` and `en`.
- Advanced mining dialog; multiple sentences.

**Exit criteria**

- [x] T-STA-*, T-SET-*, T-I18N-*, T-A11Y-* green.
- [x] Full tour of the app with the keyboard only (T-E2E-A11Y-001).
- [x] **Release 0.3.0** (2026-10-25, tag `v0.3.0`, after the maintainer's checks).

---

## M7 — Online metadata → 0.4.0

**Requirements**: MET-001, MET-002.

The online subtitles, their alignment with the audio and the secondary subtitle moved to M4b (D-14).

**Deliverables**

- `nikomiru-online`: AniList client (GraphQL), Jikan with rate limiting, behind a trait and with tests against a fake server; Jimaku searched by `anilist_id` first.
- UI: series identification (automatic if unique, otherwise chosen by the user).

**Status as of 2026-10-26**

- Done: AniList and Jikan clients (`nikomiru-online::metadata`) behind one trait, with their fake routes; the identification rule (exact and unique, otherwise the user's choice); the `series_info` table; "Identify online" on a series' page, the results to choose from, "Forget", the cover downloaded; the source chosen in Settings → Subtitle services; Jimaku asked by `anilist_id` first.
- T-MET-LIVE on 2026-10-26: AniList identified Sousou no Frieren (154587); Jikan's API (`api.jikan.moe`) did not answer at all, while its site did: an outage of the service, to run again.

**Exit criteria**

- [x] T-MET-* green on fake servers; [ ] T-MET-LIVE-* (real network) green: AniList yes, Jikan to run again once its API answers.
- [x] **Release 0.4.0** (2026-10-26, tag `v0.4.0`, at the maintainer's request; Jikan's live check to run again once its API answers).

---

## Ports — macOS, Windows, Android (D-16)

The maintainer asked for these builds on 2026-10-25. Linux stays the reference.

**Spike S6 — what each port takes** (first, before any port is promised for a release)

- Windows: a cross build from Linux (`x86_64-pc-windows-gnu`) that opens a window with egui and plays a video through libmpv; the list of Linux-only layers to put behind traits (portals, iconv, `localtime_r`, the single-instance socket, XDG folders, the fonts) and their Windows counterparts; how libmpv and ffmpeg ship, with their source obligations.
- Android: egui and mpv's render API on GLES through winit's Android backend, with mpv-android's libmpv; AnkiDroid's API for cards; touch for lookups; the size of the work.
- macOS: what a Mac or a macOS build machine must do (the Apple SDK cannot be used elsewhere), CGL or Metal.
- Output: ADR 0009 with the order of the ports and what each costs.

**Exit criteria**

- [ ] ADR 0009 accepted by the maintainer.
- [ ] Each port it keeps: built, started, a video played with clickable subtitles and a card made, on the real platform.

---

## M8 — Stabilization and packages → 1.0.0

**Requirements**: LIB-013; all NFRs not assigned to other milestones (NFR-PERF-01…NFR-PERF-09, NFR-REL-*, NFR-SEC-02, NFR-PORT-01, NFR-MAINT-02), D-5.

**Deliverables**

- Packages: PKGBUILD (AUR) and Flatpak (manifest with libmpv and ffmpeg, freedesktop runtime), as decided in D-5 (runtime updated by D-10 and D-13).
- User guide in `docs/guide/` (Italian and English): installation, first start, dictionary import, Anki setup with AnkiConnect (code 2055492159).
- Security review (TEST §6.7) and accessibility review.
- Optimization until all T-PERF pass on REF.
- Cleanup of the P1/P2 bug backlog.

**Status as of 2026-10-26**

- Done: the PKGBUILD in `packaging/aur` (built and checked with `makepkg` and `namcap`: the program, the desktop entry with its icon, `THIRD_PARTY_LICENSES.html` where Settings → About finds it, the logic's tests in `check()`); the user guide in `docs/guide/` (English and Italian); the desktop entry's icon.
- Done: the security review (`docs/security-review-1.0.md`): every entry point of external data and every `unsafe` block; two findings fixed (covers decoded within limits, subtitle files bounded to 64 MiB); `cargo deny check advisories` clean.
- Done: T-PERF-01 (start → library, median 0.45 s), T-PERF-02 (open → first frame of the 1080p60 files: 0.19 s and 0.06 s), T-PERF-09 (152 MiB at rest) in the L6 suite; T-SEC-003: the app's HOME stays empty through the whole L6 suite.
- Done: NFR-MAINT-02 (`cargo xtask coverage`: 92–97% of the lines of the logic crates), NFR-PORT-01 (the smoke test on X11 through Xwayland; the offline build in `ci --full`).
- Done: the Flatpak (`packaging/flatpak`): freedesktop 26.08, whose FFmpeg has the programs and encoders of the cards; libass, libplacebo and libmpv built in it; the crates as offline sources (`cargo xtask flatpak-sources`); `cargo xtask flatpak` builds a bundle. Built, its libraries resolved, its manifest and AppStream file passing `flatpak-builder-lint`.
- Done: the accessibility review (`docs/accessibility-review-1.0.md`), with T-A11Y-002: every page's controls named for a screen reader.
- Done: the first steps on the library page (LIB-013), which the release checklist expects of a first start; T-REL-003, a two-hour session, as a test to run before a release.
- Done: the Flatpak installs `THIRD_PARTY_LICENSES.html` too, generated with its crate sources; T-PRV-001 under `strace`; T-PERF-03 as one command for REF, `cargo xtask perf-playback` (checked for its mechanics in an off-screen KWin).
- Open: publishing on the AUR and on Flathub (the maintainer's accounts), the Flatpak run on a desktop, T-PERF-03 by hand on REF, the bug backlog (no issue open on 2026-10-26), the manual checklist of TEST §9.

**Exit criteria**

- [ ] All automatic tests green; no open P1 bugs.
- [ ] Manual release checklist TEST §9 complete on: GNOME Wayland, KDE Wayland, X11 (Xfce or i3), Intel, AMD and NVIDIA GPUs (proprietary driver).
- [x] `THIRD_PARTY_LICENSES.html` included in the packages and visible in "About" (2026-10-26: the PKGBUILD and the Flatpak install it where About finds it; `ci` checks the Flatpak's copy).
- [x] **Release 1.0.0** (2026-10-30, tag `v1.0.0`, at the maintainer's request; the manual checklist of TEST §9, in `docs/qa/1.0.0.md`, and the publishing on the AUR and Flathub follow it).

---

## M9 — Words in the subtitles → 1.0.0 with M8

Asked by the maintainer while 1.0.0 waits for its manual checks (D-17).

**Requirements**: LKP-051, LKP-060, LKP-061, NFR-PERF-10.

**Deliverables**

- `nikomiru-dict::segment`: the words of a text by greedy longest match, on a thread of its own in the app; the cues shown and the next three split ahead.
- The overlay: readings over the kanji of each word (off by default), the words already made into cards underlined (on by default), both in Settings → Subtitles.
- ← → move the keyboard lookup word by word.

**Status as of 2026-10-26**

- Done: the segmentation (T-LKP-060; 3 ms per line, p95 8 ms, on the dictionaries of T-PERF-04), the furigana of inflected words (T-LKP-051), the mined words (T-LKP-061), the settings, the keyboard by word, T-E2E-WORDS-001.

**Exit criteria**

- [x] T-LKP-051, T-LKP-060, T-LKP-061, T-PERF-10, T-E2E-WORDS-001 green.
- [ ] The furigana checked by the maintainer on real subtitles and a real dictionary (JMdict).
- [x] Released with 1.0.0 (2026-10-30).

---

## M10 — Moving pictures → 1.0.0 with M8

Proposed after M9, when the maintainer asked to go on (D-18).

**Requirements**: ANK-042, PLY-014.

**Deliverables**

- Cards whose picture is the sentence, moving: animated WebP or AVIF made by ffmpeg from the clip (ANK-042).
- Thumbnails over the progress bar (PLY-014).

**Status as of 2026-10-26**

- Done: the animated picture (T-ANK-042, T-E2E-ANK-042), its formats and levels in Settings → Anki.
- Done: the thumbnails of the progress bar (T-PLY-014, T-E2E-PLY-014): about 0.15 s a frame for 1080p HEVC 10-bit.

**Exit criteria**

- [x] T-ANK-042, T-E2E-ANK-042, T-PLY-014 and T-E2E-PLY-014 green.
- [ ] An animated card checked by the maintainer in Anki desktop and AnkiDroid.
- [x] Released with 1.0.0 (2026-10-30).

---

## After 1.0

The order is the maintainer's (D-19, 2026-10-30): M11 to M16, then the rest of the backlog. Every milestone starts with its requirements written into SPECIFICATION.md (with their tests in TEST.md) and accepted by the maintainer; until then, its deliverables below are a scope, not a design. Fixes found by the manual checks of 1.0.0 go out as 1.0.x patches meanwhile.

## M11 — Jellyfin and Plex → 1.1.0

**Goal**: watch the series of a Jellyfin or Plex server with the same subtitles, lookups and cards as a local file.

**Requirements**: SRV-001…SRV-013 (D-20).

**Deliverables**

- `nikomiru-online`: the Jellyfin and Plex clients and the loopback relay.
- `nikomiru-library`: the table `server_items`, server videos in "Continue watching".
- `nikomiru-ui`: Settings → Media servers, a tab for each server.
- `nikomiru-app`: playback, subtitles, progress and scene links for server videos.

**Status as of 2026-10-31**

- Done on simulated servers: the Jellyfin and Plex clients, the relay, the settings tab, a tab for each server, playback, subtitles, progress, scene links (T-SRV-001…T-SRV-013 except what is listed below).
- Open: the live checks (T-SRV-LIVE-*), on a real Jellyfin and the maintainer's Plex.

**Notes**: the HTTP client in use (`reqwest`, blocking); no new dependency. Test servers simulated as for AniList and Jimaku; the live checks need a real Jellyfin and the maintainer's Plex, since the development machine has no container runtime.

**Exit criteria**

- [x] SRV-001…SRV-013 accepted (D-20); T-SRV-001…T-SRV-013 green on simulated servers.
- [ ] An episode from a real Jellyfin and from a real Plex: played, looked up, made into a card whose link reopens it.
- [ ] **Release 1.1.0**.

---

## M12 — Backup, restore and WebDAV sync → 1.2.0

**Goal**: keep the library, the statistics and the settings safe, and the same on two computers.

**Requirements**: BAK-001…BAK-010 (D-21).

**Deliverables**

- `nikomiru-library`: the consistent copy, the export and merge of records, the tombstones.
- `nikomiru-online`: the WebDAV client.
- `nikomiru-app`: the backup, the restore and the `sync` worker.
- `nikomiru-ui`: Settings → Backup.

**Notes**: WebDAV is HTTP over the client in use, and the archives use `zip`: no new dependency expected. The live check needs a real Nextcloud (a container, once Podman has its network). Cloud sync, iroh P2P and the like stay out (SPECIFICATION §1.3).

**Exit criteria**

- [ ] BAK-001…BAK-010 accepted (D-21); T-BAK-001…T-BAK-010 green, against a simulated WebDAV server.
- [ ] Two computers synced through a real Nextcloud.
- [ ] **Release 1.2.0**.

---

## M13 — Advanced ASS and screenshots with subtitles → 1.3.0

**Goal**: typeset fansubs drawn as their authors meant, and screenshots with what is on screen.

**Deliverables (scope)**

- SUB-019 in Nikomiru's own renderer: `\t`, `\move`, `\fad`, `\clip`, karaoke `\k`, rotations; every drawn character still clickable.
- PLY-015: `Shift+S` saves the frame with the overlay subtitles composited on it.

**Exit criteria**

- [ ] Requirements accepted; golden images for each tag green.
- [ ] Real typeset fansubs checked by hand against mpv's libass.
- [ ] **Release 1.3.0**.

---

## M14 — Clipboard lookup and texthooker → 1.4.0

**Goal**: the dictionary and the cards for visual novels and games, through Textractor or LunaTranslator.

**Deliverables (scope)**

- A texthooker page: the lines read from the clipboard or from a WebSocket (the protocol of Textractor's and LunaTranslator's plugins), clickable as subtitles, with lookups and cards (sentence; picture and audio when given).

**Notes**: `tokio-tungstenite` (MIT) for the WebSocket; the clipboard through egui-winit.

**Exit criteria**

- [ ] Requirements accepted; tests green with a simulated hooker.
- [ ] A real visual novel through Textractor or LunaTranslator.
- [ ] **Release 1.4.0**.

---

## M15 — MDict, StarDict and DSL dictionaries → 1.5.0

**Goal**: the dictionaries that are not in Yomitan's format (DIC-004).

**Deliverables (scope)**

- Import of MDict (`.mdx`, `.mdd`), StarDict and DSL into the same store as the Yomitan dictionaries, their HTML cleaned and shown in the popup.

**Notes**: `ammonia` (MIT/Apache) for the HTML; the formats' readers written in the project unless a crate passes the license policy.

**Exit criteria**

- [ ] Requirements accepted; import tests green on project-made files of each format.
- [ ] Real dictionaries of each format imported by the maintainer.
- [ ] **Release 1.5.0**.

---

## M16 — Books, manga and audiobooks → 2.0.0

**Goal**: the direction of D-12: reading with the same dictionary and cards.

**Deliverables (scope)**, each about as large as the MVP, in this order, each possibly a release of its own:

- Books: EPUB, with the ttu reader's sync format.
- Manga: pages with the text found by OCR, clickable.
- Audiobooks: an audiobook with its text, played in sync.

**Notes**: they reuse the text, dictionary and Anki crates, which know nothing about video. The OCR's engine and its license are decided with the requirements.

**Exit criteria**

- [ ] Requirements of each part accepted; split into milestones of their own if needed.
- [ ] **Release 2.0.0**.

---

## Backlog after M16 (in no binding order)

Every item requires a change to SPECIFICATION.md before work starts.

| Item | Notes on licenses and dependencies |
|---|---|
| WebM video clips with sound (ANK-043) | libvpx (BSD) **inside the system ffmpeg**: an external process, not a crate |
| Online resources (D-12) | |
| Built-in SRS | `fsrs` is BSD-3-Clause: either an exception, or our own GPL implementation of FSRS from the published algorithm |
| AniDB ed2k identification (MET-006) | Register an AniDB client; `md4`/`ed2k` (MIT/Apache) |
| API keys in the keyring (MET-005) | `oo7` or `secret-service` (check the licenses) |
| Japanese interface | Fluent `ja` |

---

## Definition of done (applies to every PR)

1. The affected requirements are cited in the PR title or description.
2. Every implemented requirement has at least one test marked with its ID (`// T-XXX-NNN`).
3. `cargo xtask ci` is green (fmt, clippy `-D warnings`, deny, test, headers, trace).
4. Code ported from Fushi or from other GPL projects has the provenance header (SPECIFICATION §2.2).
5. Visible strings are in Fluent, for `it` and `en`.
6. Every fixed bug has an `AK-BUG-NNN` regression test (TEST §7).
7. CHANGELOG updated in the `[Unreleased]` section.
8. If a specified behavior changes, SPECIFICATION.md is updated **in the same PR**.
9. Documents, code comments and commit messages are in English (D-9).

## Versions

- SemVer. Until 1.0, `0.MINOR.PATCH`: MINOR for each milestone, PATCH for fixes. From 1.0, MINOR for each milestone that adds features, PATCH for fixes, MAJOR for a change that drops what users relied on, and for M16, which makes Nikomiru a reader too (D-19).
- The `library.redb` schema can only change with a migration (DAT-001), even before 1.0.
- CHANGELOG in the *Keep a Changelog* format.
