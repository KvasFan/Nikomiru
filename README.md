# Anikoto

Video player for **learning languages by watching anime and TV series**. You click a word in the subtitles, look it up in your Yomitan dictionaries and create an Anki card with the sentence, a screenshot and the audio of the line.

Anikoto is a Rust rewrite of [Fushi](https://github.com/hajisensai/Fushi) (GPL-3.0), limited to the video + dictionary + Anki core. It uses [Dioxus](https://github.com/DioxusLabs/dioxus) for the interface and [libmpv2](https://crates.io/crates/libmpv2) for playback.

> **Status: milestone M0 in progress** (foundations and spikes). The workspace and the tools are in place; the crates are still empty. See [ROADMAP](docs/ROADMAP.md#m0--foundations-and-spikes).

## Documents

| Document | Contents |
|---|---|
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | Purpose, licenses, architecture, functional and non-functional requirements, decisions and risks |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones M0–M8 up to 1.0, exit criteria, post-1.0 backlog |
| [docs/TEST.md](docs/TEST.md) | Test strategy, fixtures, conformance with Fushi, case catalog, CI |
| [docs/adr/](docs/adr/) | Architecture decisions with the spike measurements |

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
| `cargo xtask ci --full` | as above, plus fixtures, L4/L5 tests and `THIRD_PARTY_LICENSES.html` |
| `cargo xtask fixtures [--large] [--perf]` | generates test media and dictionaries in `target/fixtures/` (requires `ffmpeg`) |
| `cargo xtask trace` | requirements ↔ tests ↔ milestones matrix in `target/trace.md` |
| `cargo xtask about` | generates `target/THIRD_PARTY_LICENSES.html` |

The M0 spikes are separate workspaces in [`spikes/`](spikes/), with the measurements in the [ADRs](docs/adr/).

## Platforms

Linux (Wayland and X11) as the primary platform. Windows and macOS after 1.0.

Expected system dependencies: libmpv ≥ 0.37, FFmpeg ≥ 6, GTK 3.24, WebKitGTK 4.1 ≥ 2.44.

## License

[GPL-3.0-only](LICENSE). Anikoto derives from GPL-3.0 code (Fushi, Yomitan, hoshidicts, Lapis).

The allowed dependencies are MIT, Apache-2.0, GPL and LGPL. The exceptions are some permissive transitive dependencies, required by Dioxus and documented in [SPECIFICATION §3](docs/SPECIFICATION.md#3-licenses). The check is automatic, with `cargo deny`.
