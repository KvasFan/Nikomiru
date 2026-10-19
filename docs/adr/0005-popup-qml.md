# ADR 0005 — Dictionary popup in QML

| Field | Value |
|---|---|
| Status | Accepted |
| Date | 2026-10-19 |
| Requirements | POP-001…POP-013, DIC-034, DIC-035, NFR-SEC-01, NFR-PERF-06 |
| Spike | S4 — done directly in `nikomiru-dict` (`structured`, `css`, `style`) and `nikomiru-ui` (`popup`, `popup_view`, the popup QML files) |

## Context

[ADR 0003](0003-qt-quick-ui.md) chose Qt Quick and left one question to spike S4: can the dictionary popup show Yomitan structured content (ruby, lists, tables, images, per-dictionary styles) as QML items, without HTML and without a web view, fast enough for NFR-PERF-06 (click → drawn popup ≤ 100 ms p95)?

## Decision

1. **Rust builds a view model, QML only draws it.** `nikomiru_dict::structured::render` turns a glossary into *blocks* (box, list, table, details, form-of pairs) and flat *inline runs* (text with its inherited style, ruby, link, image, line break). Only the tags of POP-009 survive; `script`, `style`, `iframe` and the like are dropped with their content; deep or huge trees are cut. QML lays the runs out in a `Flow` of `Text` items with `textFormat: Text.PlainText`, so nothing from a dictionary is ever read as markup (NFR-SEC-01). Nested blocks load `GlossaryBlocks.qml` again through a `Loader`, so a level exists only when the data has it.
2. **Tables**: Rust places each cell in the grid (row and column, with `rowSpan`/`colSpan`, as HTML does) and measures the width its longest line needs (`naturalEm`); QML uses a `GridLayout` inside a horizontally scrollable `Flickable`. A wrapping `Text` has no natural width of its own, so without the measurement the columns collapse.
3. **The CSS subset of DIC-035**: from inline `style` objects and from `styles.css` (plus the user's CSS) Nikomiru keeps colour, background colour, font size (relative), weight, style, underline/strike-through, vertical alignment (sub/super), text alignment, margins, paddings, border colour/width/radius and the list marker. Values are checked (colours, lengths, keywords). In `styles.css` only rules whose selectors are a single compound of a tag, `gloss-sc-<tag>` classes and `data-sc-*` attributes apply (what Yomitan dictionaries use), by specificity and order; combinators, pseudo-classes and at-rules are ignored. A dictionary's sheet applies only to its own glossaries.
4. **Images (POP-010) are `data:` URLs**, made by Rust from the dictionary's `media` table: only image types (png, jpeg, gif, webp, svg, bmp, avif), at most 1 MiB each; a missing, unknown or larger image shows its alternative text. A `QQuickImageProvider` would need a C++ subclass (cxx-qt 0.10 cannot subclass it with a `requestImage` override), and ARC-011 allows C++ only for the video item. The paths are checked in Rust (relative, no `..`), so QML never receives an address from a dictionary.
5. **The popup logic is plain Rust** (`nikomiru_ui::popup`): the stack of nested popups (at most 8), pause and resume around them, and the placement (above the cue if it fits, otherwise below, never over it, always inside the window). The lookup runs on a worker thread of the app (`nikomiru_app::dictionaries`), which also renders the view model; the Qt thread only parses JSON.

## Results

- **T-PERF-06**, L6 test `a_click_on_a_subtitle_word_opens_the_dictionary_popup` (off-screen KWin, `mini-ja` + `mini-freq` + `mini-pitch`, 30 clicks on the same cue): click → first frame with the popup **median 17 ms, p95 21 ms** (limit 100 ms). The lookup itself is under 2 ms (ADR 0002, M3 section).
- The screenshots of the spike (headword with furigana, trace chips, frequency, pitch accent graph, collapsible dictionary blocks, ruby, a table, a redirect label kept next to its definition) look as intended at 1600×900.

## Consequences

- POP-010 is implemented with `data:` URLs instead of an image provider; the requirement's intent (images only from the dictionary, never from the network) holds. SPECIFICATION POP-010 is updated.
- Dictionaries with large images (illustrated dictionaries) show them up to 1 MiB each; larger ones need the image provider, which would mean allowing a small C++ class in `nikomiru-ui` (an ARC-011 exception) or a cxx-qt version that can subclass it.
- Inline content wraps per run: a long styled span next to a ruby starts on its own line instead of continuing the line. Readable, not identical to a browser; to revisit if real dictionaries look poor.
