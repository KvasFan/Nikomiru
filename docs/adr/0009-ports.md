# ADR 0009 — Ports to Windows, macOS and Android

| Field | Value |
|---|---|
| Status | Proposed |
| Date | 2026-11-03 |
| Requirements | §5.2.4, NFR-PORT-01 |
| Milestone | Ports, spike S6 (decision D-16) |

## Context

The maintainer asked for builds for Windows, macOS and Android (D-16). The ROADMAP puts spike S6 first: find what each port takes before any is promised for a release. Nikomiru was written for Linux, but with the platform kept at the edges: the interface (`nikomiru-ui`) sees only traits (`IdleInhibitor`, `SecretStore`, `Clock`, the services), and the desktop layers live in `nikomiru-app`.

## The spike

Windows was tried first, since it can be built from this Linux machine: the `x86_64-pc-windows-gnu` target of Rust 1.99.1, linked with mingw-w64 (GCC 16.2), against libmpv 2 for Windows (the `mpv-dev-x86_64` package of shinchiro's `mpv-winbuild-cmake`, 2026-11-03, the build mpv's own site points to).

1. **The library crates.** `cargo check` for Windows stopped on five uses of Unix-only APIs, each one line of `cfg` to fix:
   - `nikomiru-core` `fs::write_atomic`: the file mode `0o600` and the `fsync` of the folder (Windows cannot open a folder as a file);
   - `nikomiru-dict` `sqlite`: `read_exact_at` (Windows: `seek_read` in a loop);
   - `nikomiru-media` `tools`: the executable bit, and `ffmpeg.exe` rather than `ffmpeg` (`EXE_SUFFIX`);
   - `nikomiru-library` `backup`: the mode `0o600` of a backup;
   - `nikomiru-ui` `url_to_path`: a path from bytes (Windows paths are UTF-16), and the drive of `file:///C:/…`.

   With these, all twelve library crates build for Windows, `nikomiru-player` (libmpv) and `nikomiru-ocr` (tract) included, and their tests pass on Linux as before.
2. **The features.** egui, winit and glutin had `wayland`, `x11`, `egl` and `glx` on for every target, and `wayland-sys` cannot build for Windows. They are now in `nikomiru-app`'s `[target.'cfg(target_os = "linux")'.dependencies]`, with `secret-service`, `wayland-client` and `libc`; Windows takes glutin's `wgl`.
3. **The app.** 48 errors, all in `nikomiru-app`: the `linux` module (signals, iconv, `localtime_r`, `malloc_trim`, the activation token, Wayland drag and drop), D-Bus and the portals (file dialogs, links, the color scheme, the screensaver), the Secret Service keyring, the single-instance socket, the log folder's mode, and the window, whose EGL/GLX path has no Windows form. For the spike:
   - the `linux` module is declared as `platform`, from `linux/mod.rs` on Linux and `windows/mod.rs` on Windows;
   - `inhibit`, `keyring`, `portal` and `instance` take files of `windows/` with the same API, which do nothing and say so in the log; `dbus` and `appearance` exist only on Linux;
   - the window has two paths: on Linux the display first, then a window for its configuration (EGL, else GLX), as before; on Windows the window first, then a WGL display for it.
4. **The result.** `nikomiru.exe` builds and links (debug, 325 MB with its symbols; `libmpv-2.dll` beside it, 122 MB with ffmpeg inside). Starting it under wine with a video was not done: running the downloaded libmpv was left to the maintainer. The Linux build is unchanged: on the spike's branch every step of `cargo xtask ci --full` is green, and 68 of its 69 GUI tests; the other is the timing of T-PERF-06, p95 100.3 ms against 100 ms while the machine was busy, which run alone measures as on `main` (median 50 ms on both).

## Decision (proposed)

### Order

1. **Windows first.** It builds from Linux today; what is left is the Windows side of each layer below, then a check on a real Windows.
2. **Android second.** The largest: the input model changes (a tap looks up, a long press selects; no hover), files come through the document picker, cards through AnkiDroid's API (AnkiConnect is a desktop add-on), libmpv from mpv-android, GLES through winit's Android backend. The NDK is the maintainer's to install (D-16).
3. **macOS last.** The Apple SDK's license allows a build only on Apple hardware: a Mac or a macOS build machine (a GitHub runner, sparingly, D-16). OpenGL is deprecated there but still works through CGL; a Metal path is a later question.

### What the Windows port still needs

| Layer | Linux | Windows | Size |
|---|---|---|---|
| File dialogs, links | XDG portal over D-Bus | the common item dialog, `ShellExecuteW` (or the `rfd` crate, MIT) | M |
| Screensaver | portal `Inhibit` | `SetThreadExecutionState` | S |
| Light or dark | portal `color-scheme` | the registry's `AppsUseLightTheme`, and `WM_SETTINGCHANGE` | S |
| Keyring | Secret Service | Credential Manager | S |
| Single instance | Unix socket and lock file | a named pipe | M |
| Termination | signals through a self-pipe | console control events; the window's close is enough at first | S |
| Subtitle encodings | glibc `iconv` | a decoder for Shift_JIS, EUC-JP, UTF-16 (e.g. `encoding_rs`, its license to check against §3.2) | M |
| Local time | `localtime_r` | `GetTimeZoneInformationForYear`, or `jiff`'s time zones | S |
| Drag and drop | Wayland data device | winit's own `DroppedFile` | S |
| Folders | XDG | `%APPDATA%`, `%LOCALAPPDATA%`, the known Pictures folder (`Paths` reads only `HOME` and XDG today) | S |
| ffmpeg | the system's | shipped beside the program | S |
| Packages | AUR, Flatpak | an installer or a ZIP, with libmpv and ffmpeg; their LGPL/GPL source obligations | M |

S is a day or less, M a few days. With the checks on a real Windows, about two weeks of work.

## Consequences

- The platform layer is one module, `platform`, with a file per system; what the interface needs from the desktop stays behind traits, as before.
- Code shared by all systems keeps away from `std::os::unix`: a Windows `cargo check` of the library crates belongs in the CI once the port is accepted, so the crates stay portable.
- The Windows build needs libmpv's import library at link time (`-L native=<folder>`) and its DLL at run time: neither is in the repository.
- Until the maintainer accepts this record, the Windows stubs are not released and promise nothing.
