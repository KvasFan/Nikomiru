# Security review for 1.0

| Field | Value |
|---|---|
| Version reviewed | 0.4.0 plus the M8 branch (2026-10-26) |
| Asked by | TEST §6.7: a review of every point where external data enters, and of every `unsafe` block |
| Automatic tests | T-SEC-001…T-SEC-008, and the tests named below |

The automatic tests check what they can. This review reads the code at the points the tests cannot fully cover, records what each point bounds and why, and fixes what it found.

## 1. Findings

Two findings, both fixed in this review:

| # | Where | What | Fix |
|---|---|---|---|
| 1 | `nikomiru-ui::catalog::Cover::decode` | Covers were decoded with `image::load_from_memory` and the crate's default limits, up to 512 MiB per image. Since 0.4.0 a cover can come from the network (AniList, Jikan, MET-001). | Covers now go through the decoder of the dictionaries' images (`popup_view::decode_image`): PNG, JPEG, GIF, WebP or BMP only, recognized by their bytes; at most 4 MiB of input, 8192 px a side and 128 MiB of decoder memory; scaled down beyond 1280 px. Tested by `catalog::tests::covers_are_decoded_within_limits`. |
| 2 | `nikomiru-app::subtitles` | A local subtitle file was read whole, whatever its size. | At most 64 MiB, the bound of a downloaded subtitle. A larger file is refused with a message (`error-subtitle-too-large`). |

No other finding. The dependencies have no open RustSec advisory: `cargo deny check advisories` passes. Its three ignored entries are crates that are not maintained, reached only through AccessKit's AT-SPI stack; none is a vulnerability.

## 2. Where external data enters

| Data | Where it is read | Bounds | Tests |
|---|---|---|---|
| Dictionary ZIP files | `nikomiru-dict` import | DIC-011:<br>• no absolute paths, `..` or links;<br>• entries ≤ 512 MiB, ratio ≤ 200:1 above 1 MiB, total ≤ 8 GiB;<br>• ZIP64 and Unicode paths read with checks. | T-DIC-011, T-SEC-002, the import conformance cases of TEST §5.3 |
| Dictionary JSON | `nikomiru-dict` import | ≤ 10 million entries; a glossary ≤ 10 MiB; malformed rows skipped or refused (fushidicts cases) | conformance, T-DIC-* |
| Dictionary content shown | `popup_view` | A typed view model from an allowlist (NFR-SEC-01): no HTML is parsed for display; links are lookups or `http(s)` opened only on click | T-SEC-001 (L1, L3, L6) |
| Dictionary CSS, and the user's | `nikomiru-dict::css` | A subset of properties and selectors (DIC-035), lengths bounded, the rest reported and dropped | T-DIC-035 |
| Dictionary images, covers | `popup_view::decode_image` | Format by the bytes; size, sides and decoder memory bounded (see finding 1) | T-POP-010, `covers_are_decoded_within_limits` |
| Subtitle files | `nikomiru-subs` parsers, through `subtitles.rs` | ≤ 64 MiB (finding 2); decoded by BOM, UTF-8 or `iconv` on a fixed list of encodings; the parsers never index past their input | T-SUB-*, T-SUB-005 |
| Embedded subtitle tracks | ffmpeg, as a child process | Output files only in the cache; the arguments are built in a vector, never through a shell; a path is passed as `file:…` | T-EXT-002, T-SEC-005 |
| HTTP answers | `nikomiru-online::http` | HTTPS only (HTTP only to loopback). Answers ≤ 4 MiB, files ≤ 64 MiB. No redirects for the APIs. A key goes only to its own host. Proxy only from the settings (SET-007). | T-MET-*, T-SET-007 |
| Online subtitle archives | `nikomiru-online::archive` | ZIP only, by name only, ≤ 64 MiB unpacked, ≤ 1000 entries; RAR and 7z listed but never opened | T-MET-003 |
| Word audio | `nikomiru-online::audio` | HTTPS or loopback; ≤ 8 MiB; an unreachable host is paused for 45 s; played by an mpv core of its own | T-AUD-* |
| `android.db` (Local Audio Server) | `nikomiru-dict::sqlite` | Nikomiru's own read-only reader, every read bounded: pages in the file, cells in their page, payloads ≤ 64 MiB, b-trees ≤ 40 levels | ADR 0004; files cut short and 300 randomly damaged files |
| `nikomiru://` links | `nikomiru-anki::link` | Strict parse: unknown or duplicate parameters, absolute paths and `file:` refused; ≤ 16 KiB. A video is found only through the library, never by a path in the link. | T-ANK-051, T-SEC-007 |
| Second launch | `nikomiru-app::instance` | Socket in `$XDG_RUNTIME_DIR` (mode 0700), lock file 0600; a request is one line of ≤ 1 MiB; only paths and links, handled as if from the command line | T-APP-001 |
| AnkiConnect answers | `nikomiru-anki` | JSON read leniently; checks without being asked only on loopback; media of a refused card taken back | T-ANK-* |
| `mpv.conf` imported | `nikomiru-player::options` | Each line previewed. Options that run code, load other files or write files are refused (`script…`, `load-…`, `input-…`, `include`, `config-dir`, `log-file`, `stream-record`…); profiles in sections are not supported. | T-PLY-016 |
| `config.toml` | `nikomiru-core::config` | A wrong value takes its default; parse messages hide quoted values, which may be keys | T-SET-001, T-ERR-001 |
| Files dropped or chosen | the drag-and-drop source and the portal | Paths only; nothing runs | T-LIB-001 |

## 3. `unsafe`

`unsafe` is allowed only where the architecture checks permit it (ARC-011): the player crate (the mpv FFI) and three parts of the app crate (`src/linux`, `src/window.rs`). Every block has a `SAFETY` comment, and `cargo xtask check-headers` refuses `unsafe` anywhere else.

| File | Blocks | What | Review |
|---|---|---|---|
| `nikomiru-player/src/render.rs` | 9 | mpv's render context: creation, update callback, render, report swap, free | Sound. The callbacks' contexts are boxed and freed only after `mpv_render_context_free`, so mpv calls neither afterwards. The caller's contract (a current GL context, the display outliving the renderer, the renderer dropped before the core) is written on `VideoRenderer::new` and kept by `window.rs` and `main::run`. |
| `nikomiru-player/src/player.rs` | 1 | `mpv_request_log_messages`, not exposed by libmpv2 | Sound: a live handle, a NUL-terminated level that mpv copies. |
| `nikomiru-app/src/window.rs` | 9 | glutin display, configurations, context, surface; glow's loader; plain GL state calls | Sound. The handles come from the event loop and outlive the window (field order); GL calls happen with the context current on the UI thread. |
| `nikomiru-app/src/linux/mod.rs` | 12 | Environment cleanup, `setlocale`, the signal self-pipe, `iconv`, `localtime_r`, and two test-only signals | Sound. `remove_var` and `setlocale` run before any thread starts. The signal handler only calls `write`, which is async-signal-safe. `iconv` reads at most its input, and writes at most `out_left` into a buffer of 4 bytes per input byte (UTF-8's worst case) plus slack; a failure is "not this encoding". `localtime_r` is reentrant, and its result is read only on success. |
| `nikomiru-app/src/linux/dnd.rs` | 1 | The Wayland display borrowed from winit for the drag-and-drop source | Sound: the backend is a guest that never closes the display, and is dropped before the event loop ends. |

## 4. What stays outside Nikomiru's hands

- **libmpv and FFmpeg** parse every video and audio file, in process (libmpv) and as children (ffmpeg, ffprobe). Their safety is the system's, kept up to date by the distribution; the Flatpak will carry its own copies, to update with each release.
- **The `image` crate's decoders** handle untrusted images within the limits above.
- **The TLS stack**: `reqwest` with `native-tls`, so the system's OpenSSL.
- **The system's portals and AT-SPI bus** are trusted as the desktop is.

## 5. To repeat before each release

- `cargo deny check advisories`, and a fresh look at its ignored entries (they go once AccessKit moves to zbus 6).
- This review, for every new place where external data enters. Since this review, the only new one is the online metadata of 0.4.0.
