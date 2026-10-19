# QA record — M2, Subtitles (TEST §9.2)

Manual checklist at the end of M2, with at least three real files. Each item is
recorded as **pass**, **fail** (with a note) or **n/a** (with the reason).
Environment as in [m1-player.md](m1-player.md).

| | |
|---|---|
| Tester | — |
| Date | — |
| Commit | — |
| Build | `cargo run --release -p nikomiru-app -- <file>` |

## Files

| # | File (not committed) | What it exercises |
|---|---|---|
| 1 | — | A real SRT in Shift_JIS next to the video (`<video>.ja.srt`) |
| 2 | — | A fansub ASS with styles and karaoke |
| 3 | — | An MKV with two embedded text tracks |

## Items

| Item | Result | Notes |
|---|---|---|
| The Shift_JIS SRT loads automatically; the toast and Subtitles → Encoding show `CP932`; forcing another encoding reloads it | | |
| The fansub ASS stays readable (styles and karaoke effects not supported are ignored, the text is complete); with `respect_ass_styles = true` colours, fonts and `\pos` signs follow the file | | |
| The MKV's Japanese track is chosen automatically; the menu switches to the other track and to Off | | |
| A click on a character pauses and highlights exactly that character, in the overlay and in the list (L) | | |
| Z/X move the subtitles by 100 ms (toast); Ctrl+Shift+←/→ align a subtitle that is 2 s out of sync | | |
| Ctrl+←/→ jump to the previous/next line, R replays the current one | | |
| B blurs during playback, the text shows on mouse hover and when paused; H hides it | | |
| The list follows playback and stays on the previous line in the gaps; Ctrl+F filters without losing the position | | |

## Automated counterparts

Green in `cargo xtask ci --full`:
- T-SUB-001…008 and T-SUB-011…016;
- the 67 Fushi conformance cases;
- T-SUB-009 (L6), T-SUB-013 (L5) and T-PERF-07 (L6).
