# Spikes

Time-boxed prototypes that reduce technical risks before the features are written (ROADMAP M0, and S5 in M3). **The code is thrown away:** later milestones rewrite it cleanly in `crates/`. Each spike is a standalone Cargo workspace, excluded from the main one, with its own `Cargo.lock`.

| Spike | Question | Outcome |
|---|---|---|
| [`s1-composition/`](s1-composition/) | mpv video (GtkGLArea) and Dioxus UI (transparent webview) in the same window, with a `GtkOverlay` | [ADR 0001](../docs/adr/0001-video-composition.md) — superseded by S1q (D-10) |
| [`s1q-qt/`](s1q-qt/) | The same, with Qt 6 Quick instead of Dioxus: mpv in a `QQuickFramebufferObject` rendered on Qt Quick's render thread, QML overlay, Rust through cxx-qt | [ADR 0003](../docs/adr/0003-qt-quick-ui.md) — accepted (D-10), then superseded by S5 (D-13) |
| [`s2-storage/`](s2-storage/) | Can redb handle dictionary import and lookup within NFR-PERF-04/05? | [ADR 0002](../docs/adr/0002-dictionary-storage.md) |
| [`s5-toolkits/`](s5-toolkits/) | The video overlay and the dictionary popup with iced and with egui (eframe), measured against S1q's Qt Quick | [ADR 0006](../docs/adr/0006-toolkit-comparison.md) — egui accepted (D-13) |

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

S1q needs Qt ≥ 6.5 with its development files, `qmake6`, a C++ compiler and libmpv:

```sh
cd spikes/s1q-qt && QMAKE=qmake6 cargo build --release && cd ../..
spikes/s1q-qt/target/release/spike-s1q-qt target/fixtures/av_hd60_h264.mkv --measure 60   # on the real session
spikes/s1q-qt/run-virtual.sh "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 20   # virtual KWin, 4K
```

S5 builds two binaries from one workspace (a shared crate, an iced one and an egui one), and compares them with S1q's binary (build it first, as above) in the same virtual KWin. The real 2160p clip is taken from `$S5_2160P` (default `target/fixtures/av_2160p60_hevc10.mkv`; the command that makes it is in ADR 0006 and in `measure-all.sh`). The popup measurement needs a file of popup contents, one `popups_json` array per line, as the Qt app made them with real dictionaries (not part of the repository):

```sh
cd spikes/s5-toolkits && cargo build --release && cd ../..
spikes/s5-toolkits/measure-all.sh target/spike-s5 [POPUPS_FILE] [REPETITIONS]   # one JSON report per run
spikes/s5-toolkits/run-virtual.sh --size 1920x1080 spikes/s5-toolkits/target/release/spike-s5-egui "$PWD/target/fixtures/av_hd60_h264.mkv" --fullscreen --measure 20
```

Keep the machine otherwise idle while measuring: each report records the 1-minute load average at the start and the end of its window.

The iced binary runs wgpu on OpenGL (`WGPU_BACKEND=gl`, set by itself) and decodes with a copy back to memory (see ADR 0006).
