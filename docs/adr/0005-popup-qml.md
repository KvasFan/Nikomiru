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

1. **Rust builds a view model, QML only draws it.** `nikomiru_dict::structured::render` turns a glossary into *blocks* (paragraph, box, list, table, details, form-of pairs) and flat *inline runs* (text with its inherited style, ruby, link, image, line break). Only the tags of POP-009 survive; `script`, `style`, `iframe` and the like are dropped with their content; deep or huge trees are cut. A paragraph without ruby, images or chips is one `Text` with `textFormat: Text.StyledText`, whose markup Rust writes itself from escaped text with a fixed set of tags (`b`, `i`, `u`, `s`, `font` colour and size, `br`, links as an index into the paragraph's targets; never `img`); other inline content is laid out in a `Flow` of `Text` items with `textFormat: Text.PlainText`. Nothing from a dictionary is ever read as markup (NFR-SEC-01). Nested blocks create `GlossaryBlocks.qml` again, so a level exists only when the data has it.
2. **Tables**: Rust places each cell in the grid (row and column, with `rowSpan`/`colSpan`, as HTML does) and measures the width its longest line needs (`naturalEm`); QML uses a `GridLayout` inside a horizontally scrollable `Flickable`. A wrapping `Text` has no natural width of its own, so without the measurement the columns collapse.
3. **The CSS subset of DIC-035**: from inline `style` objects and from `styles.css` (plus the user's CSS) Nikomiru keeps colour, background colour, font size (relative), weight, style, underline/strike-through, vertical alignment (sub/super), text alignment, margins, paddings, border colour/width/radius and the list marker. Values are checked (colours, lengths, keywords). In `styles.css` only rules whose selectors are a single compound of a tag, `gloss-sc-<tag>` classes and `data-sc-*` attributes apply (what Yomitan dictionaries use), by specificity and order; combinators, pseudo-classes and at-rules are ignored. A dictionary's sheet applies only to its own glossaries.
4. **Images (POP-010) are `data:` URLs**, made by Rust from the dictionary's `media` table: only image types (png, jpeg, gif, webp, svg, bmp, avif), at most 1 MiB each; a missing, unknown or larger image shows its alternative text. A `QQuickImageProvider` would need a C++ subclass (cxx-qt 0.10 cannot subclass it with a `requestImage` override), and ARC-011 allows C++ only for the video item. The paths are checked in Rust (relative, no `..`), so QML never receives an address from a dictionary.
5. **The popup logic is plain Rust** (`nikomiru_ui::popup`): the stack of nested popups (at most 8), pause and resume around them, and the placement (above the cue if it fits, otherwise below, never over it, always inside the window). The lookup runs on a worker thread of the app (`nikomiru_app::dictionaries`), which also renders the view model; the Qt thread only parses JSON.

## Results

- **T-PERF-06**, L6 test `a_click_on_a_subtitle_word_opens_the_dictionary_popup` (off-screen KWin, `mini-ja` + `mini-freq` + `mini-pitch`, 30 clicks on the same cue): click → first frame with the popup **median 17 ms, p95 21 ms** (limit 100 ms). The lookup itself is under 2 ms (ADR 0002, M3 section).
- The screenshots of the spike (headword with furigana, trace chips, frequency, pitch accent graph, collapsible dictionary blocks, ruby, a table, a redirect label kept next to its definition) look as intended at 1600×900.

## With a real dictionary: Jitendex (2026-10-03)

Imported from the maintainer's copy: 436,555 terms, 253 media files, 6.1 s and 522 MiB peak (release build).

- **CSS**: Jitendex styles its structured content with nested rules (`li[data-sc-content="sense"] { & ul[…] { list-style-type: none } }`), descendant selectors and left-only borders (`border-style: none none none solid`). The subset now covers nesting (`&`), descendant and child combinators (matched against the chain of ancestors) and borders on some sides; sibling combinators, pseudo-classes and pseudo-elements stay out. Text next to ruby is lowered to the ruby's base line.
- **Size of an entry**: a rich entry (行く) is ~800 blocks and runs, several thousand QML items; created at once they took 90–200 ms. The popup creates a budget of 30 blocks and runs at once (about the first sense, with its example) and the rest in the next frames through asynchronous `Loader`s.

## Big entries faster, furigana in examples (2026-10-20)

With Jitendex and JMdict together a click on 行きたくない took 85 ms to show the popup (release build), and the readings of an example sentence sat over whole spans that wrapped as blocks. Profiling the QML (`qmlprofiler` on a copy of the popup files with real popup contents) showed where the time went:

- **The budget was not shared.** A JavaScript object passed in the property map of `createObject` (or of `Loader.setSource`) reaches the new object as a copy, so every nested level spent its own copy: "30 items at once" was 30 per branch. The budget, and the pool of components below, are now `QtObject`s, passed by reference; the counter is a plain JavaScript field, so spending it re-evaluates no binding. Creating the first part of 行く went from 41–45 ms to 14–17 ms.
- **Repeaters get counts, not arrays.** A Repeater turns an array of objects into a `QVariantList`, deeply, compares it with the previous one, and each delegate turns its `modelData` back into JavaScript: nested content was copied at every level. The content now stays JavaScript and each delegate reads its item by index. Rust sends the popup content as a JSON string, so the panels' own model stays light, and each panel parses its content once. The fixed part of the popup (everything but the glossaries) went from ~20 ms to ~8 ms for 行く.
- **One component per depth.** A nested level used to load `GlossaryBlocks.qml` by URL (`Loader.setSource`, 53 µs); it now creates it from a component made once (20 µs). One per depth, because a component cannot create an object while it is still completing an ancestor; made in the popup layer, because a component made inside a popup becomes unusable when that popup closes.
- **Fewer items.** Boxes that draw nothing (no margin, padding, background or border) and lists whose items have no marker give way to their content; consecutive plain paragraphs merge into one StyledText `Text` (a sense with five glosses: one item instead of about thirty); the horizontal margin of an inline span becomes non-breaking spaces (an inline element has no vertical margin), so Jitendex's "See also" label and footnote marks stay in the paragraph. Jitendex, 行きたくない: 479 → 337 blocks and 334 → 187 runs; JMdict: every glossary line is one `Text`. A list item's own marker is kept (Jitendex's ①, ②…).
- **Furigana as furigana.** A flow with ruby is split into characters (words for Latin text), so an example sentence wraps like Japanese text and each reading stays over its own base.

Results (release build, off-screen KWin at 1600×900, Jitendex and JMdict, machine under load, click → first frame with the content, 3 runs each): 行きたくない 85–87 → 53–64 ms, 食べてしまった 74–77 → 28–40 ms, 猫が好き 47–58 → 22–27 ms, 学校 36 → 20 ms; the whole of 行く is created about 130 ms after the result arrives. Debug build: 行きたくない 84 ms (104–111 before). Colours given as `rgba()` or `#rrggbbaa` are now emitted as Qt reads them (`#aarrggbb`, alpha first): before, a semi-transparent red came out blue.

## Consequences

- POP-010 is implemented with `data:` URLs instead of an image provider; the requirement's intent (images only from the dictionary, never from the network) holds. SPECIFICATION POP-010 is updated.
- Dictionaries with large images (illustrated dictionaries) show them up to 1 MiB each; larger ones need the image provider, which would mean allowing a small C++ class in `nikomiru-ui` (an ARC-011 exception) or a cxx-qt version that can subclass it.
- Inline content with ruby is a flow of characters: it wraps like Japanese text, but without line-breaking rules (a `。` can start a line), and a reading wider than its base widens it, as in a browser.
- QML code that passes shared state between components must use a `QtObject`, never a JavaScript object, and must not give a Repeater an array of objects (see the comments in `GlossaryBlocks.qml`).
