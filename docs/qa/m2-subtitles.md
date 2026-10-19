# QA record — M2, Subtitles (TEST §9.2)

Manual checklist at the end of M2, with at least three real files. Each item is
recorded as **pass**, **fail** (with a note) or **n/a** (with the reason).
Environment as in [m1-player.md](m1-player.md).

| | |
|---|---|
| Tester | maintainer |
| Date | 2026-10-19 |
| Commit | `594a882` |
| Build | `cargo run --release -p nikomiru-app -- <file>` |

## Files

| # | File (not committed) | What it exercises |
|---|---|---|
| 1 | maintainer's file | A real SRT in Shift_JIS next to the video (`<video>.ja.srt`) |
| 2 | maintainer's file | A fansub ASS with styles and karaoke |
| 3 | maintainer's file | An MKV with two embedded text tracks |

## Items

| Item | Result | Notes |
|---|---|---|
| The Shift_JIS SRT loads automatically; the toast and Subtitles → Encoding show `CP932`; forcing another encoding reloads it | pass | Works, but the menu showed only `CP932` and the meaning of "encoding" was not clear. Changed: readable names ("Japanese — Shift_JIS (CP932)"), "Automatic" shows what was detected, the forced choice is ticked (SUB-005) |
| The fansub ASS stays readable (styles and karaoke effects not supported are ignored, the text is complete); with `respect_ass_styles = true` colours, fonts and `\pos` signs follow the file | pass | |
| The MKV's Japanese track is chosen automatically; the menu switches to the other track and to Off | pass | |
| A click on a character pauses and highlights exactly that character, in the overlay and in the list (L) | pass | |
| Z/X move the subtitles by 100 ms (toast); Ctrl+Shift+←/→ align a subtitle that is 2 s out of sync | pass | |
| Ctrl+←/→ jump to the previous/next line, R replays the current one | pass | |
| B blurs during playback, the text shows on mouse hover and when paused; H hides it | pass | Request: a third option, subtitles off. Added: V turns them off and on (nothing drawn, not even on hover; the track stays chosen), also in the 字 menu (SUB-014) |
| The list follows playback and stays on the previous line in the gaps; Ctrl+F filters without losing the position | pass | |

## Result

All items pass on Wayland; the two notes above became changes in the next commit.

## Automated counterparts

Green in `cargo xtask ci --full`:
- T-SUB-001…008 and T-SUB-011…016;
- the 67 Fushi conformance cases;
- T-SUB-009 (L6), T-SUB-013 (L5) and T-PERF-07 (L6).
