# M0 spikes

Time-boxed prototypes that reduce technical risks before the features are written (ROADMAP M0). **The code is thrown away:** later milestones rewrite it cleanly in `crates/`. Each spike is a standalone Cargo workspace, excluded from the main one, with its own `Cargo.lock`.

| Spike | Question | Outcome |
|---|---|---|
| [`s1-composition/`](s1-composition/) | mpv video (GtkGLArea) and Dioxus UI (transparent webview) in the same window, with a `GtkOverlay` | [ADR 0001](../docs/adr/0001-video-composition.md) |
| [`s2-storage/`](s2-storage/) | Can redb handle dictionary import and lookup within NFR-PERF-04/05? | [ADR 0002](../docs/adr/0002-dictionary-storage.md) |

## How to run them

First: `cargo xtask fixtures --large --perf` (synthetic dictionaries and 1080p60 video).

```sh
# S2: import and lookup benchmark
cargo build --release --manifest-path spikes/s2-storage/Cargo.toml
spikes/s2-storage/target/release/spike-s2-storage all target/fixtures/dicts/synth target/spike-s2

# S1: interactive window (Space pause, ←/→ seek, F fullscreen)
cargo build --release --manifest-path spikes/s1-composition/Cargo.toml
GDK_BACKEND=wayland spikes/s1-composition/target/release/spike-s1-composition target/fixtures/av_hd60_h264.mkv
```

The automatic S1 measurements (`--measure`, `--stress`, `--exercise`) must run in an off-screen compositor, not in the graphical session in use: a covered window does not receive frame callbacks and does not render (ADR 0001). `run-virtual.sh` does this, with a virtual KWin:

```sh
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 290
GDK_BACKEND=x11     spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 290
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh --scale 2 "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 30
GDK_BACKEND=wayland spikes/s1-composition/run-virtual.sh "$PWD/target/fixtures/av_basic.mkv" --stress 50
```

The JSON report goes to stdout.
