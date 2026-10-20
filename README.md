# Nikomiru

Video player for **learning languages by watching anime and TV series**. You click a word in the subtitles, look it up in your Yomitan dictionaries and create an Anki card with the sentence, a screenshot and the audio of the line.

Nikomiru is a Rust rewrite of [Fushi](https://github.com/hajisensai/Fushi) (GPL-3.0). It starts from the video + dictionary + Anki core: until 1.0 the focus is video with clickable subtitles; books, manga and online resources come after. The interface is [egui](https://github.com/emilk/egui), all in Rust; playback is [mpv](https://mpv.io/) through [libmpv2](https://crates.io/crates/libmpv2), which draws the video into the window under the interface ([ADR 0006](docs/adr/0006-toolkit-comparison.md)).

> **Status: 0.0.0, a development snapshot.** M0 (foundations and spikes) and M2 (subtitles) are done; M1 (minimal player) waits for its manual checklist; M3 (dictionaries and lookup) is in progress; the interface moved from Qt Quick to egui (D-13). Anki comes in M4. See [ROADMAP](docs/ROADMAP.md#m3--dictionaries-and-lookup).
>
> Nikomiru was called Anikoto until 2026-10-19 (decision D-11).

## Documents

| Document | Contents |
|---|---|
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | Purpose, licenses, architecture, functional and non-functional requirements, decisions and risks |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones M0–M8 up to 1.0, exit criteria, post-1.0 backlog |
| [docs/TEST.md](docs/TEST.md) | Test strategy, fixtures, conformance with Fushi, case catalog, CI |
| [docs/adr/](docs/adr/) | Architecture decisions with the spike measurements |

## Trying it

```sh
cargo run --release -p nikomiru-app -- path/to/video.mkv
```

What works in 0.0.0: opening a file (argument, drag & drop also on Wayland, `Ctrl+O` with the desktop's file dialog), playback with keyboard and mouse controls (seek, speed, volume, audio track, chapters, frame step, fullscreen), resume from the last position, single instance, screensaver inhibition; subtitles drawn by Nikomiru (SRT, ASS, VTT, embedded or next to the video, encodings, the subtitle list, blur and hide, delay); a click on a word opens its dictionary popup, from Yomitan dictionaries imported on the dictionaries page (or with `--import-dictionary`); translated messages (English, Italian). Logs are in `~/.local/state/nikomiru/logs/`.

## Development

You need Rust through `rustup`: the version is pinned in `rust-toolchain.toml`. You also need the license checking tools:

```sh
cargo install --locked cargo-deny
cargo install --locked cargo-about --features cli
```

Tasks run with `cargo xtask`:

| Command | What it does |
|---|---|
| `cargo xtask ci` | fmt, clippy `-D warnings`, architecture checks, licenses (`cargo deny`), traceability, tests |
| `cargo xtask ci --full` | as above, plus fixtures, L4/L5 tests, the release build, the GUI end-to-end tests (L6, in an off-screen `kwin_wayland` on a private `dbus-daemon`) and `THIRD_PARTY_LICENSES.html` |
| `cargo xtask fixtures [--large] [--perf]` | generates test media and dictionaries in `target/fixtures/` (requires `ffmpeg`) |
| `cargo xtask trace` | requirements ↔ tests ↔ milestones matrix in `target/trace.md` |
| `cargo xtask about` | generates `target/THIRD_PARTY_LICENSES.html` |

The spikes are separate workspaces in [`spikes/`](spikes/), with the measurements in the [ADRs](docs/adr/).

## Platforms

Linux (Wayland and X11) as the primary platform. Windows and macOS after 1.0.

Expected system dependencies: libmpv ≥ 0.37, FFmpeg ≥ 6, OpenGL with EGL (Mesa or NVIDIA's driver), the Wayland or X11 client libraries and libxkbcommon, fontconfig, OpenSSL 3 (for HTTPS, through native-tls) and a font with Japanese glyphs (for example Noto Sans CJK). Building also needs the development files of libmpv and OpenSSL, and `pkg-config`.

## License

[GPL-3.0-only](LICENSE). Nikomiru derives from GPL-3.0 code (Fushi, Yomitan, hoshidicts, Lapis).

The allowed dependencies are MIT, Apache-2.0, GPL and LGPL. The only exceptions are a few permissive transitive dependencies (the Unicode-3.0 ICU family, `foldhash` and `libloading`), documented in [SPECIFICATION §3](docs/SPECIFICATION.md#3-licenses). The check is automatic, with `cargo deny`.
