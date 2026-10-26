# Accessibility review for 1.0

| Field | Value |
|---|---|
| Version reviewed | 0.4.0 plus the M8 branch (2026-10-26) |
| Asked by | ROADMAP M8 (the accessibility review), NFR-A11Y-01 |
| Automatic tests | T-A11Y-001, T-A11Y-002, T-E2E-A11Y-001, T-SET-003, T-SET-005 |

NFR-A11Y-01 asks for three things: the whole interface usable from the keyboard with the focus always visible, AA contrast in the default themes, and roles and names on the controls (AT-SPI on Linux). This review says how each is checked, and what is left.

## 1. Keyboard

T-E2E-A11Y-001 walks the main path with keys alone: library → episode → lookup → card → settings → back.

- **Over the video**, the shortcuts take the keys. Every action has one, and the keys can be changed in Settings → Shortcuts (KEY-001).
- **Looking a word up** works from the keyboard as well as with the mouse:
  - Enter looks up the subtitle shown, and ← → move the lookup by one word (by one character until the line is split into words);
  - Tab and Shift+Tab move between the popup's entries;
  - Ctrl+Enter makes the card, and Ctrl+Shift+Enter opens the dialog that makes it by hand.
- **On a page or a dialog** (the library, the settings, the dialogs), only the global shortcuts run. Tab, the arrows, Enter and Space drive the page's controls, and Esc goes back from anywhere.
- **The library** gives the keyboard to its first card, and a series' page gives it to the first episode.
- **Drag-and-drop alternatives**: dragging a dictionary into place also has ▲ ▼ buttons, and dropping a file on the window also has Ctrl+O.

## 2. Focus

- egui draws the focus of its own widgets.
- The library's cards are custom widgets, so they draw their own focus: a 2 px ring in the accent color.

## 3. Contrast and size

- **Contrast (T-SET-003)** is computed from the palette. Every pair of text and background is at least 4.5:1, in the light and dark themes, with several accents, and over a black or a white video. An accent used as text is lightened or darkened until it reaches 4.5:1.
- **Interface scale (T-SET-005)** goes from 75% to 200%, and the subtitles keep their own size.
- **Language** can be changed without a restart.

## 4. Screen readers

AccessKit publishes the interface on AT-SPI.

- **T-A11Y-001** starts an AT-SPI bus and registry in the off-screen desktop, and reads the tree with the project's own D-Bus client. Every button, slider, combo box, check box and radio button of the bar has a name.
- **T-A11Y-002** reads the tree on the dictionary popup, on each of the eleven tabs of the settings, on the library and on its statistics. Every control is named, and each page shows a control it must have.
- **The video surface** is named "Video".
- **Subtitle lines and the popup's text** are labels, so a screen reader reads them.

## 5. What is left

| Item | Why | Alternative today |
|---|---|---|
| The day-by-day values of the statistics' chart and calendar show only on hover | They are drawn shapes, with no focus | The totals (today, 7 days, all), the time of each series and the list of sessions are text |
| A screen reader is not told when a cue changes | AccessKit has no live regions in egui 0.37 | The subtitle list (L) reads every line |
| The colors of the dictionaries' own CSS are theirs | A dictionary may choose colors that read badly | Each dictionary's CSS can be overridden on its page (DIC-035) |
| No high-contrast theme | — | The light and dark themes are AA; the accent can be chosen |

## 6. To repeat before each release

- The L6 suite: T-A11Y-001, T-A11Y-002, T-E2E-A11Y-001.
- A manual tour with Orca on GNOME. The automatic tests read the tree a screen reader reads, but not how Orca speaks it.
