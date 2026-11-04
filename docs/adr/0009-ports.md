# ADR 0009 — Ports to Windows, macOS and Android

| Field | Value |
|---|---|
| Status | Accepted (2026-11-03) |
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
4. **The result.** `nikomiru.exe` builds and links (debug, 325 MB with its symbols; `libmpv-2.dll` beside it, 122 MB with ffmpeg inside). Under wine 11.21, on an off-screen KWin with Xwayland, it opens its window through WGL and plays a video with libmpv: picture, sound through WASAPI, the tracks read; settings, library and logs in the Windows folders. The subtitles need ffmpeg (EXT-001), now also looked for beside the program; that run, and a real Windows, are still to do. The Linux build is unchanged: on the spike's branch every step of `cargo xtask ci --full` is green, and 68 of its 69 GUI tests; the other is the timing of T-PERF-06, p95 100.3 ms against 100 ms while the machine was busy, which run alone measures as on `main` (median 50 ms on both).
5. **Folders.** `Paths` read only `HOME` and XDG, which wine does not pass and Windows does not use: on Windows they now come from `%APPDATA%` (settings) and `%LOCALAPPDATA%` (data, cache and logs, each in a folder of its own), with the pictures in the profile.

## Decision

### Order

1. **Windows first.** It builds from Linux today; what is left is the Windows side of each layer below, then a check on a real Windows.
2. **Android second.** The largest: the input model changes (a tap looks up, a long press selects; no hover), files come through the document picker, cards through AnkiDroid's API (AnkiConnect is a desktop add-on), libmpv from mpv-android, GLES through winit's Android backend. The NDK is the maintainer's to install (D-16).
3. **macOS last.** The Apple SDK's license allows a build only on Apple hardware: a Mac or a macOS build machine (a GitHub runner, sparingly, D-16). OpenGL is deprecated there but still works through CGL; a Metal path is a later question.

### The Windows layers

| Layer | Linux | Windows | State on 2026-11-04 |
|---|---|---|---|
| File dialogs, links | XDG portal over D-Bus | the common dialogs (`GetOpenFileNameW`, `GetSaveFileNameW`), the folder browser (`SHBrowseForFolderW`), `ShellExecuteW`; no new crate | written, not tried |
| Screensaver | portal `Inhibit` | `SetThreadExecutionState`, from a thread of its own | written, not tried |
| Light or dark | portal `color-scheme` | the registry's `AppsUseLightTheme`, read every 2 s | written, not tried |
| Keyring | Secret Service | generic credentials of the Credential Manager, `nikomiru:<key>` | tested under wine |
| Single instance | Unix socket and lock file | a named mutex decides who is first, a named pipe carries the request; both named after the user's folder | tested under wine |
| Termination | signals through a self-pipe | none: a GUI program gets no signals, closing the window saves as on Linux | done |
| Subtitle encodings | glibc `iconv` | Windows's code pages (`MultiByteToWideChar`): CP932, EUC-JP (20932), GB18030, Big5, CP1252 and the names web pages use; `encoding_rs` would bring BSD-3-Clause | tested under wine |
| Local time | `localtime_r` | `SystemTimeToTzSpecificLocalTime` | tested under wine |
| Drag and drop | Wayland data device | winit's own `DroppedFile`, which the shell already took on X11 | done, not tried |
| Folders | XDG | `%APPDATA%\Nikomiru` for the settings, `%LOCALAPPDATA%\Nikomiru\{data,cache,state}`, the profile's Pictures | tested under wine |
| Resolved paths | `canonicalize` | `canonicalize` gives `\\?\C:\…`, which neither compares nor reads well: `nikomiru_core::fs::canonical` makes it ordinary | tested under wine |
| ffmpeg | the system's | shipped beside the program, searched there after `PATH` | done |
| Packages | AUR, Flatpak | a ZIP with libmpv and ffmpeg, and their LGPL/GPL source obligations | to do |

Every unit test of every crate passes for Windows under wine 11.21, but those that use Unix permissions or symbolic links. What is left: the package, and the checks on a real Windows (the dialogs, the theme, the screen kept on, a video with clickable subtitles and a card).

## Consequences

- The platform layer is one module, `platform`, with a file per system; what the interface needs from the desktop stays behind traits, as before.
- Code shared by all systems keeps away from `std::os::unix`: a Windows `cargo check` of the library crates belongs in the CI once the port is accepted, so the crates stay portable.
- The Windows build needs libmpv's import library at link time (`-L native=<folder>`) and its DLL at run time: neither is in the repository.
- Until the maintainer accepts this record, the Windows stubs are not released and promise nothing.
