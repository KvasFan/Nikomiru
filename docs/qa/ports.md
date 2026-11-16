# Ports — per-platform status (D-16, M117)

Each platform is tracked on its own. A cross build, Wine or an emulator is evidence of progress, never of a release: a platform is released only after its native checks below pass on the real platform. iOS is not part of D-16. The maintainer confirmed on 2026-11-15 that only Linux is checked for now.

## Linux (reference)

Released and checked every block by `cargo xtask ci --full` (Wayland through an off-screen KWin, X11 smoke test through Xwayland). Touch gestures (PORT-003) are checked there with injected touch events.

## Windows

| Item | State | Evidence |
|---|---|---|
| Build | done | `x86_64-pc-windows-gnu` cross build from Linux; `cargo xtask ci` checks every crate for Windows with clippy (ADR 0009). |
| Windows layers (dialogs, theme, screen kept on, keyring, single instance, encodings, time, folders) | written; partly tried under Wine | ADR 0009 table: keyring, single instance, encodings, local time and folders tested under Wine 11.21. |
| Package | built, not tried natively | `cargo xtask windows --libmpv … --ffmpeg …` ZIP with `SOURCES.txt`. |
| Touch gestures | implemented | Shared egui code (PORT-003); not tried on a Windows tablet. |
| Platform tool installation (INS-007) | open | The setup catalogue lists Linux builds only. |
| Native checks: start, video with clickable subtitles, lookup, card | **open** | Needs a real Windows machine. |
| Source offer for libmpv/ffmpeg in a published ZIP | **open** | Maintainer's choice (ADR 0009). |

## Android

| Item | State | Evidence |
|---|---|---|
| Input model (tap looks up, swipe turns/seeks) | implemented on the shared code | PORT-003, tested on Linux with injected touches. |
| Build (winit Android backend, GLES, mpv-android's libmpv) | **open** | Needs the NDK, which the maintainer installs (D-16). |
| Files through the document picker | **open** | Not started. |
| AnkiDroid cards (its content provider API instead of AnkiConnect) | **open** | Not started; cannot be built or tested without Android. |
| Native checks on a device | **open** | |

## macOS

| Item | State | Evidence |
|---|---|---|
| Build | **open** | The Apple SDK may be used only on Apple hardware (ADR 0009): needs a Mac or a macOS build machine. |
| Touch/trackpad | shared code | PORT-003 swipes come from touch events; trackpad swipes arrive as scrolling and keep scrolling. |
| Native checks | **open** | |
