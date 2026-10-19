# QA record — M1, Player (TEST §9.1)

Manual checklist at the end of M1. Each item is recorded with its result
(**pass**, **fail** with a note, or **n/a** with the reason).

## Environment

| | |
|---|---|
| Tester | maintainer |
| Date | 2026-10-19 |
| Commit | `594a882` |
| Distribution | CachyOS, kernel 7.2.10-1-cachyos |
| Desktop | KDE Plasma 6.8.0, KWin 6.8.0; Wayland session, X11 session for the X11 run |
| Displays | 4K at 120 Hz, fractional scale 2.7 |
| GPU and driver | AMD Radeon RX 7900 XTX (radeonsi, Mesa 26.2.4), AMD Radeon 610M integrated |
| mpv / libmpv | 0.41.0 / libmpv 2.5.0 |
| FFmpeg | n9.0.2 |
| Qt | 6.12.0 |
| Build | `cargo run --release -p nikomiru-app -- <file>` |

## Items

| Item | Wayland | X11 | Notes |
|---|---|---|---|
| Opening from the dialog, from drag & drop and from the command line, with Unicode file names and spaces | pass | | Fixture: `target/fixtures/-weird: name & spaces ü 日本.mkv` |
| Fullscreen on a secondary monitor; exit with Esc | pass | | First run: Esc did not leave fullscreen. Retest with commit `1e8a0a4` (and `--debug` key logging): Esc works. F → Esc is now also an L6 regression test |
| HiDPI 200%: sharp video, sharp overlay, no misaligned edges | pass | | Tested at the session's 270% |
| hwdec active (check `hwdec-current`) on Intel, AMD and NVIDIA | pass | | Only AMD is available here: Intel and NVIDIA are n/a. `hwdec-current` is in the log with `--debug` (`[vd] Using hardware decoding (vaapi)`) |
| System suspend and resume during playback | pending | | Not possible at the time of the test |
| Audio on PipeWire with a device change during playback | pass | |  |

## Automated counterparts

Already green in `cargo xtask ci --full`: T-LIB-001 (command line and drag &
drop, L6), T-E2E-001 (open, play, seek, close, resume, L6), T-APP-001 (single
instance, L6), T-PLY-013 (screensaver portal, L4).
