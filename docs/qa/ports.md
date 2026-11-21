# Ports — per-platform status (D-16, M117)

Each platform is tracked on its own. A cross build, Wine or an emulator is evidence of progress, never of a release: a platform is released only after its native checks below pass on the real platform. iOS is not part of D-16. The maintainer confirmed on 2026-11-15 that only Linux is checked for now.

## Linux (reference)

Released and checked every block by `cargo xtask ci --full` (Wayland through an off-screen KWin, X11 smoke test through Xwayland). Touch gestures (PORT-003) are checked there with injected touch events.

## Windows

| Item | State | Evidence |
|---|---|---|
| Build | done | `x86_64-pc-windows-gnu` cross build from Linux; `cargo xtask ci` checks every crate for Windows with clippy (ADR 0009). |
| Windows layers (dialogs, theme, screen kept on, keyring, single instance, encodings, time, folders) | written; partly tried under Wine | ADR 0009 table: keyring, single instance, encodings, local time and folders tested under Wine 11.21. |
| Package | published, not tried natively | `cargo xtask windows --libmpv … --ffmpeg …` makes the ZIP with `SOURCES.txt` and, with `wixl` (msitools), an MSI for all users under Program Files with a Start menu shortcut and major upgrades. Both published with 2.120.0; the MSI installs, runs `--version` and uninstalls under Wine 11.21. |
| Touch gestures | implemented | Shared egui code (PORT-003); not tried on a Windows tablet. |
| Platform tool installation (INS-007) | catalogued, not tried natively | `assets/setup/tools.json` pins Windows x86_64/aarch64 FFmpeg, yt-dlp and Deno assets (T-INS-007 checks every platform); the Windows ZIP/MSI already carry FFmpeg/ffprobe. |
| Native checks: start, video with clickable subtitles, lookup, card | **open** | Needs a real Windows machine. |
| Source offer for libmpv/ffmpeg in a published ZIP | written offer by link | `SOURCES.txt` names each archive, its SHA-256 and the upstream release with the sources; the maintainer asked to publish the ZIP and the installer on 2026-11-17. |

## Android

| Item | State | Evidence |
|---|---|---|
| Input model (tap looks up, swipe turns/seeks) | implemented on the shared code | PORT-003, tested on Linux with injected touches. |
| Build (winit Android backend, GLES, mpv-android's libmpv) | done (preview, 2.180.0) | `cargo xtask android` builds a signed APK for arm64-v8a and x86_64 with NDK r29 and libmpv-android 1.1.0 (mpv 0.42); `cargo xtask ci` checks it with clippy (T-PORT-006). |
| Package | published as a preview, emulator only | The APK of 2.180.0 installs, plays a video with its subtitles, keeps the screen on and comes back from the background on an Android 15 emulator (x86_64, SwiftShader GLES 3). |
| Files | the app's own chooser | The system's document picker needs Java to answer; the chooser lists the shared storage once "all files access" is given in the system settings (asked for on the first file dialog). |
| Phone layout | done (2.181.0), emulator only | Upright phones draw the desktop pages smaller (at least 560 points across) and the system bars are kept out; below 720 points the player's bar keeps play, the time and the subtitles and puts the rest in a "⋯" menu, and the settings fill the screen with the tabs in a list (PLY-024). A few symbols are still missing from Android's fonts. |
| FFmpeg's command line | done (2.181.0), emulator only | `packaging/android/ffmpeg.sh` builds FFmpeg 8.1.3 with LAME, Opus and libwebp; the APK carries `libffmpeg.so`/`libffprobe.so` and the libraries renamed `*_nikomiru.so`. Embedded subtitles, thumbnails, sentence audio (AAC) and pictures (JPEG) checked on the emulator. |
| Keyring | done (2.181.0), emulator only | The Android Keystore seals the keys (MET-005): a key in the file moved to `keyring.json` at the start and came back after a restart. |
| AnkiDroid cards (its content provider API instead of AnkiConnect) | done (2.181.0), emulator only | ANK-055 with AnkiDroid 2.25.3: the permission asked and given, the Lapis note type and deck created, two cards mined from a video with their audio and picture in AnkiDroid's media. |
| Native checks on a real phone | **open** | Only the emulator so far. |

## macOS

| Item | State | Evidence |
|---|---|---|
| Build | **open** | The Apple SDK may be used only on Apple hardware (ADR 0009): needs a Mac or a macOS build machine. |
| Touch/trackpad | shared code | PORT-003 swipes come from touch events; trackpad swipes arrive as scrolling and keep scrolling. |
| Native checks | **open** | |
