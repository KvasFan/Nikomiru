# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- External subtitles of a real Jellyfin are offered: it names SubRip `subrip`, which gave a file no one read as a subtitle.

### Added

- Books (M16, BOK-001…BOK-008): a Books page reads EPUB files, with furigana, lookups by click, cards of the sentence, and each book's place kept.
- StarDict, DSL and MDict dictionaries (M15, DIC-004): imported like Yomitan's, their definitions as plain text.
- Chapters show (PLY-019): marks on the progress bar, the current one next to the time, the one under the pointer, and a menu to go to one. They could only be reached with PageUp and PageDown.
- With no word audio source, the popup offers the way to add one (AUD-009), instead of nothing.
- Texthooker (M14, HOK-001…HOK-005): a page with the lines of a game from Textractor's or LunaTranslator's WebSocket, or the clipboard, clickable as subtitles, with cards of text only.
- Advanced ASS (M13, SUB-019), with "respect ASS styles" on: fades, moves, rotations, scaling, clips, transforms and karaoke, every character still clickable where it is drawn.
- `Shift+S` saves the frame with its subtitles (PLY-015).
- Backup and sync (M12, BAK-001…BAK-010): Settings → Backup writes a backup to a file and restores one at the next start; a WebDAV folder keeps progress, statistics and cards the same on your computers, merged record by record, by hand or at start and close.
- Media servers (M11, SRV-001…SRV-013): Jellyfin and Plex in Settings → Media servers, a tab for each on the library page, their videos played with the same subtitles, lookups and cards as a file. Direct play only; the position and watched state go back to the server. Tokens stay inside Nikomiru: mpv and ffmpeg read through a relay on this computer.

### Fixed

- A card has the tag of its series (ANK-027): it was written in the specification, and never made.

### Removed

- The spikes (`spikes/`): their decisions are in the ADRs and their code in the tag `v1.0.0`. The large synthetic audio database of spike S3 is now made by `xtask/scripts/android_db_large.py`.

### Changed

- SPECIFICATION: backup and WebDAV sync of M12 (BAK-001…BAK-010, D-21), with their tests in TEST.
- SPECIFICATION: the media servers of M11 (SRV-001…SRV-013, D-20), with their tests in TEST.
- ROADMAP: the milestones after 1.0, in the maintainer's order (D-19): M11 Jellyfin and Plex, M12 backup and WebDAV sync, M13 advanced ASS and screenshots with subtitles, M14 clipboard lookup and texthooker, M15 MDict, StarDict and DSL, M16 books, manga and audiobooks.

## [1.0.0] - 2026-10-30

Packages, the words in the subtitles and moving pictures (M8, M9, M10).

### Added

- Moving pictures (M10, D-18): a card's picture can be its sentence as an animated WebP or AVIF (ANK-042), chosen in Settings → Anki → Image format.
- Thumbnails over the progress bar: the pointer shows the time and the frame there (PLY-014).
- First steps on the library page: import a dictionary, add a folder, make a first card, each with what does it, until done or hidden (LIB-013).

- Words in the subtitles (M9, D-17): the subtitles are split into words with your dictionaries (LKP-060). Words you already made into cards are underlined, in the subtitle list too (LKP-061), and readings can go over the kanji (LKP-051, off by default): both are in Settings → Subtitles. ← → move a keyboard lookup word by word.

- An Arch Linux package: `packaging/aur/PKGBUILD` (D-5), with the icon, the desktop entry and the third-party licenses where Settings → About finds them.
- The user guide, in English and Italian (`docs/guide/`).
- A Flatpak (`packaging/flatpak`, D-5): the freedesktop 26.08 runtime with its FFmpeg, libmpv built in; `cargo xtask flatpak` makes a bundle.
- The desktop entry has an icon: the logo, installed by `cargo xtask install` too.

### Security

- Covers are decoded within the limits of the dictionaries' images (format by the bytes, size, sides and memory bounded): since 0.4.0 one can come from the network.
- A subtitle file larger than 64 MiB is refused instead of read whole.

### Fixed

- A long session no longer grows in memory (T-REL-003): each dictionary popup no longer leaves a layer behind in the interface, the GL fence mpv 0.41 makes at every frame drawn is deleted, and the memory freed is given back to the system. Two hours of playback with lookups: 101 → 122 MiB, where it was 110 → 195.

## [0.4.0] - 2026-10-26

Series identified online (M7).

### Added

- Series identified online (MET-001, MET-002, M7): "Identify online" on a series' page asks AniList (or MyAnimeList through Jikan, chosen in Settings → Subtitle services) for its titles, episodes and cover. A result with exactly the series' title is taken at once, otherwise you choose among the results; "Forget" drops it. Nothing goes online unless asked.
- Jimaku finds the subtitles of an identified series by its AniList id first, then by title (MET-003).

## [0.3.0] - 2026-10-25

Statistics, the full settings, the interface strings checked, the advanced mining and the keyboard (M6).

### Added

- Statistics (STA-001…STA-005): the time of a video watched is counted once (seeks and pauses are not), the characters of a cue once it has been played 1.5 s (the units of study of every script, STA-003), lookups and cards; sessions split by hour and by a pause over 2 min, in the local time. The Library page has a Statistics tab: today, 7 days and all time, a chart by day over 7, 30 or 365 days, the year as a calendar, the time by series, the sessions (each can be deleted) and an export to CSV.
- The full settings (SET-001…SET-006): one page with eleven tabs down its side, General, Player, Subtitles, Subtitle services, Dictionaries (📚 opens it), Lookup, Word audio, Anki, Shortcuts, Advanced and About. A tab's Save applies its settings at once; Reset puts them back to their defaults, keeping keys, logins and word audio sources.
- Light, dark or the desktop's theme, followed when the desktop changes (the settings portal), and an accent color (SET-003). The pages and the dictionary popups follow it; the bar, the subtitles and the toasts on the video stay dark.
- The interface language and scale change without a restart (SET-004, SET-005); the subtitles keep their size on the screen.
- A shortcut editor (KEY-001…KEY-004): a key is added by pressing it, refused when another action has it, removed with ×; an action goes back to its default keys with ↺.
- A card made by hand (ANK-040, ANK-041): Shift+click on the card button, or Ctrl+Shift+Enter, opens a dialog where the lines before and after join the sentence, the clip's start and end can be changed and listened to, the picture is the frame of the click, the one at the start of the sentence or none, and the sentence audio can be left out.
- The whole way with the keyboard (NFR-A11Y-01): Enter looks up the first word of the subtitle shown and ← → move the lookup a character; Ctrl+, opens the settings; on a page or a dialog the keys drive its controls (Tab, the arrows, Enter, Space) and only the global shortcuts run, Esc included; the library gives the keyboard to its first card, with the focus drawn.

### Changed

- `config.toml` keeps the comments, order and layout of a file edited by hand: saving changes only the values that changed. A value of the wrong kind takes its default and is named in a notice, instead of the whole file being set aside (SET-001).
- A global shortcut can no longer share its key with an action of another context (KEY-003).
- Back (Esc, Alt+←) is a global shortcut: it closes a page or a dialog wherever the keyboard is.

### Fixed

- The settings tabs General to About cover the popups: keys typed there no longer reach a popup under them.
- The next episode started by the countdown plays, instead of staying paused on its first frame.
- Going back to the library saves the position first, so the video being watched is in "Continue watching" at its place.

## [0.2.0] - 2026-10-25

The library and word audio (M5).

### Added

- The Library ("← Library" in the bar, Esc over the video, and at start without a file; LIB-002…LIB-012): add a folder, and its videos are grouped into series by their names (Anitomy-style, the episode numbers completed from the files beside them, season folders as seasons), or by folder. "Continue watching", the series with a cover made from their first episode and how many are watched, the episodes of a series with their state. A scan again keeps progress and choices, and so do the user's edits: rename, merge, split, move or remove episodes, delete a series; the files are never touched.
- The next episode starts by itself after a 5 s countdown that Esc or "Cancel" stops (LIB-009); an episode opened again near its end, or as the next one, starts from the beginning (LIB-007); one played to 90% is watched (LIB-008).
- A series remembers its subtitle, offset and audio language for all its episodes (LIB-010).
- Word audio (AUD-001…AUD-008): ♪ next to the card button of an entry plays the word from the sources set in Settings → Word audio, URL templates with `{term}` and `{reading}` that answer with a Yomitan list or the audio itself (a local audio server for Yomitan, for instance); a right click lists what every source has. The first entry plays by itself when a popup opens. No source is built in. The audio plays on an mpv of its own and is kept in a cache of 500 MiB.
- A source of word audio can also be the `android.db` of the Local Audio Server for Yomichan (AUD-003), chosen with "Database…", with its sub-sources in the order wanted (`nhk16`, `forvo`, `jpod` by default). Nikomiru reads the SQLite file itself, read-only, with no library ([ADR 0004](docs/adr/0004-local-audio-db.md), spike S3).

### Changed

- `library.redb` has schema 2: series and imported folders; the videos of 0.1 are migrated, with a backup (`library.redb.bak-v1`).

## [0.1.0] - 2026-10-25

The first release: M0–M4 and M4b (D-15). A video player with clickable Japanese subtitles, Yomitan dictionaries in a popup and one-click Anki cards, online subtitles and mpv presets.

### Changed (rename)

- **The project is now called Nikomiru** (D-11): "Anikoto" was already taken. Crates `nikomiru-*`, binary `nikomiru`, URL scheme `nikomiru://`, XDG folders `nikomiru`, environment variables `NIKOMIRU_*`, GitHub repository `nikomiru`.

### Added

- The settings are one page with tabs (⚙ in the bar, SET-008): Player, Subtitle services and Anki; ⚙ opens the tab shown last. On Subtitle services (MET-011) the services that search for subtitles are ticked or not and put in the order preferred (`[online] providers`), with each one's key, the OpenSubtitles login, the automatic subtitles and the proxy; the Online subtitles page keeps only the search and leads there with "Services and keys…".
- The Player settings page opens on the mpv options in use, as `mpv.conf` text to edit in step with the controls (PLY-016); while Nikomiru sets no option, the user's own `~/.config/mpv/mpv.conf` is offered in the preview, and "Import mpv.conf…" starts in its folder.
- A video opens with its local subtitles also when their names differ from its own (SUB-006): the files of its episode next to it or in a `Subs/`, `Sub/` or `Subtitles/` folder, all those of a folder named after it there, and, for a film alone in its folder, every subtitle file beside it. The language is read from the name (`Show 02.ja.srt`, `Japanese.srt`, `[JP]`).
- A secondary subtitle (SUB-017), e.g. the translation under the Japanese line: chosen in the subtitle menu among the text tracks and files, at the top of the video or above the main subtitle, smaller, not clickable, with its own blur mode (shown, blurred or hidden until hovered), following the main subtitle's offset.
- "Align with the audio" in the subtitle menu (SUB-018): the subtitle offset is found by matching the cues with the loud moments of the audio track (the first 20 minutes, ±15 s) and applied only when the match is clear. Downloaded subtitles are aligned by themselves once they load.
- Player settings (⚙ in the bar, PLY-016, PLY-018): the mpv options as controls in three groups (rendering, audio, advanced) with a reset each, the raw options with the lines mpv will not get explained, and presets in `mpv.conf` format: save, load and switch, import any `mpv.conf` through the file dialog with a preview of every line before anything changes, export through the save dialog. Applying sets the options in mpv at once and names those that wait for the next file; the presets work in mpv as they are.
- Online subtitles (ADR 0008, D-14): "Search online…" in the subtitle menu opens a page that searches Jimaku and OpenSubtitles with the title, season and episode read from the file name, ranks the Japanese files of the episode first and loads the one chosen; ZIP season packs are opened and the episode taken out. With automatic subtitles on, a video without Japanese subtitles gets them by itself. Downloaded files are kept in the data folder under the video's hash, so the next opening needs no network. Nothing goes online unless asked for.
- The Anki settings page ("Anki" in the bar): the AnkiConnect address and key with a connection test, "Create Lapis deck", the deck and note type, a template for each field with a placeholder picker (the Lapis defaults for a Lapis note type), duplicates, tags, audio pads, formats and quality levels. Saving writes `config.toml` (0600) off the UI thread, and the next card follows the new settings.
- The link in a card, `nikomiru://source?…`, opens its video paused at the scene's cue, also from a second launch (ANK-052); a video that moved is offered to be located, and the library keeps its new place. `assets/nikomiru.desktop` registers Nikomiru for these links and the video types, and `cargo xtask install` installs it with the release binary for builds from source (APP-002).
- Cards to Anki from the popup (ANK-030…ANK-039): each entry has a card button, `+` when Anki has no card for the word, `✓` when it has one (a menu opens it in Anki or adds another), `✓↩` for the last card made since Nikomiru started (the menu can also overwrite it); Ctrl+Enter is the button of the entry the keyboard is on. A card holds the entry composed from the dictionaries, the cue with the word in bold, the frame of the click and the sentence audio of the track being heard (made in parallel), and the link back to the scene; it is recorded in the library. The `anki` worker runs AnkiConnect's requests one at a time, checks only an AnkiConnect on this computer without being asked, pauses the checks 30 s after a failure, takes back the media of a card Anki refused and keeps those of a card whose outcome is unknown.
- Anki cards are composed from the popup entry (ANK-020…ANK-027): the placeholders of Fushi and Yomitan, with every text escaped; the glossary in the shape the Lapis CSS styles (`data-sc-*` attributes, `gloss-sc-*` classes), reduced to the popup's allowlist, with the dictionary's CSS rewritten from the subset of DIC-035 and scoped, and its images sent with the card. The link back to the scene, `nikomiru://source`, is built and read strictly (ANK-050, ANK-051).
- `S` saves the frame shown, without subtitles, as a PNG in `Pictures/Nikomiru` named after the file and the position, and says where (PLY-011). The player gets commands with a reply: they run in order with the others, and a worker waits for their result, never the UI thread.
- Specification (`docs/SPECIFICATION.md`), roadmap (`docs/ROADMAP.md`) and test plan (`docs/TEST.md`) for the Rust rewrite of Fushi.
- GPL-3.0-only license.
- Cargo workspace with the 11 crates of SPECIFICATION §5.1 (still empty), Rust 1.99.1 pinned in `rust-toolchain.toml`, shared lints (`unwrap_used` and `expect_used` forbidden, ERR-002).
- License policy enforced: `deny.toml` (Class A, Class B exceptions of SPECIFICATION §3.2, ban on the alternatives rejected in §3.6), `about.toml` and the `THIRD_PARTY_LICENSES.html` template.
- `cargo xtask` with the commands `ci [--full]`, `fixtures [--large] [--perf]`, `check-headers`, `check-licenses [--canary]`, `trace` and `about`.
- Spikes S1 (video composition) and S2 (dictionary storage) in `spikes/`, with the measurements and the decisions in `docs/adr/`.
- Spike S1q (Qt 6 Quick + cxx-qt + mpv render on the scene graph thread) and ADR 0003: Qt Quick as the UI toolkit instead of Dioxus.
- Spike S5 (the video overlay and the dictionary popup with iced and with egui, measured against S1q's Qt Quick) and ADR 0006.
- T-SEC-001 tests the hostile entry of `mini-ja` for real: looked up through the real import, lookup and popup content builder, its view model walked against the allowlist, drawn with `egui_kittest`, and opened in the real app, where a spy proxy sees no request. The GUI test harness has `lookup {text}` and `popup_content {level}` for it.
- Conformance with every fushidicts test (TEST §5.3): `tests/conformance/import.json` and its runner (45 import cases: wrapper folders, Unicode Path fields, ZIP64 sentinels, broken deflate streams, lying sizes, media, styles, damaged dictionary files), 14 more lookup cases (kanji stats, glossary round trip, pitch patterns, score and frequency order, non-ASCII titles, malformed kanji rows) and an import that reaches a file size limit halfway. Five import cases are known failures of the importer, listed in TEST §5.3.
- The dictionaries page lists the dictionaries by type (terms, kanji, frequencies, pitch accent) with their language (DIC-030): a dictionary with several types has its row under the first and a reference under the others, where it ranks too; ▲/▼ move it within its type, and so does dragging it by its handle onto the drop marker (DIC-031). A term dictionary has an editor for the user's CSS (DIC-035), applied after its `styles.css` through the same safe subset, which says what the subset keeps: the rules applied, the properties left out, the rules ignored.
- `[popup]` in `config.toml` (POP-006): the largest dictionary popup, 560 × 500 points by default; it was 560 points wide and up to 70% of the window high.
- A dictionary popup is resized by dragging its border (POP-006): the edge away from the word, the side edges and the corners between them. The popup keeps its side of the word, and the next popups take the new size until the app closes.
- Text selected in a glossary opens a nested popup (POP-012): a drag across a paragraph, or across a line of ruby, chips and links (the readings left out), looks the selection up in a child popup next to it, and the selection stays lit while that popup is open. Links still open with a click.

### Changed

- **D-13: the UI toolkit is egui instead of Qt 6 Quick** (ADR 0006, accepted after spike S5): under heavy load at 4K HEVC 10-bit egui keeps the video like Qt (0–2 dropped frames in 20 s) with about half of its CPU, draws a whole popup entry in one frame, and is all Rust; iced was rejected (no zero-copy video, dropped frames under load, a CC0 crate, no accessibility). Class B exception `libloading` (ISC) in SPECIFICATION §3.2, `deny.toml` and `about.toml`. The port is ROADMAP M3 work track 5; ADR 0003 and the QML part of ADR 0005 are superseded.
- **The interface runs on egui** (ROADMAP M3, work track 5): winit, glutin with EGL first, egui-winit and egui_glow; mpv draws the video with the libmpv OpenGL render API into the window under the interface, and the player's commands run on a thread of their own so that the UI thread never waits for mpv. Qt, QML, cxx-qt and the C++ video item are gone. Everything the Qt interface did is there: the control bar (it fades in and out), the subtitle overlay (outline, shadow and blur as copies of the text), the subtitle list (only the rows in view are laid out), the dictionary popups and the dictionaries page; the L6 suite is the same, plus a real drag from another Wayland client.
- The dictionary popup is built from a typed view model on the lookup thread: no generated StyledText any more, colours as RGBA (the whole CSS name table, `hsl()`), list markers in their own system (roman, alpha, CJK), relative font sizes multiplied down the tree, images decoded by their bytes with decoder limits and scaled down. Tables have row and column spans, boxes their partial borders, sub- and superscripts are raised and lowered, a collapsed image opens on click, the keyboard cursor scrolls to entry 0 too, a press anywhere in a parent popup closes its children, and the parent keeps its state while children open.
- The subtitle list picks the character under the pointer (Qt picked the nearest cursor position, so the right half of a character selected the next one).
- Open, the subtitle file and the dictionary import use the desktop's file dialog through the XDG portal (`org.freedesktop.portal.FileChooser`), with a message when there is none; web links in dictionaries open through the `OpenURI` portal, or `xdg-open` without one.
- Drag and drop works on Wayland (winit has none there): the app's own data device takes `text/uri-list` drops, with the copy action only.
- Decisions D-1…D-6 approved and recorded in `docs/SPECIFICATION.md` §10.1.
- SPECIFICATION §5.1: ARC-010 and ARC-011 written as definitions, so that `cargo xtask trace` reads them.
- TEST: test marker rule (§8), L7 test names (`l7_…`), off-screen compositor for L6 (headless Weston or virtual KWin).
- SPECIFICATION §5.2: composition strategy (a) adopted, with the constraints found by spike S1 (`with_disable_dma_buf_on_wayland(false)`, `LC_NUMERIC=C`, native display for hwdec, mpv shutdown order); risk R-1 updated.
- SPECIFICATION §10.1: decisions D-7 (5 Class B procedural macros found in the real graph), D-8 (CC0 exception for `base16`, needed by the Dioxus `router` feature) and D-9 (English for documents, comments and commit messages) approved; §3.2 table and `deny.toml` updated.
- Documents and code comments translated to English (D-9); `docs/SPECIFICHE.md` renamed to `docs/SPECIFICATION.md`.
- ROADMAP: the X11 T-PERF-03 measurement moved from M0 to M1; M4 (release 0.1.0) now includes restyling the repository page after storytold/cadcraft.
- **D-10: the UI toolkit is Qt 6 Quick (QML) through cxx-qt instead of Dioxus** (ADR 0003, accepted after the developer's 4K/120 Hz session showed no dropped frames). SPECIFICATION v0.5, TEST and ROADMAP revised: architecture (§5), class B table reduced to the ICU/Unicode family and `foldhash`, XDG paths implemented in-house (no `directories`, no MPL-2.0 exception), popup and styling as QML items (spike S4 before M3), Flatpak with the KDE runtime. ADR 0001 superseded.
- M0 done: all exit criteria met (the X11 measurement on a real Xorg session moved to M1). SPECIFICATION §5.2.2: CUDA interop only with NVIDIA's proprietary driver (no more `Cannot load libcuda.so.1`), libmpv messages routed to the log.
- `cargo xtask`: dependency rules for cxx-qt/libmpv2 and a guard against the retired toolkit, `unsafe` allowed in cxx-qt bridge modules, C++ only in `nikomiru-player/cpp/`, SPDX headers also on `.cpp`, `.h` and `.qml` files.
- SPECIFICATION: new requirements PLY-016 (save and load mpv option presets in `mpv.conf` format, M6), PLY-017 (options that raw options and presets can never set, M1) and PLY-018 (render and audio settings in the UI, M6), with tests T-PLY-016…018.

### Fixed

- The Anki settings no longer write over a `config.toml` that could not be read (they apply until Nikomiru closes and say so); saves run one at a time; "Create Lapis deck" saves its choice; a deck or note type Anki does not have is reported; a Lapis deck under another letter case counts; reopening the page keeps the saved deck, and its tags field can be emptied.
- Card fields can no longer run script in Anki: the Lapis templates paste fields into JavaScript template literals, so `` ` `` `$` `\` are escaped too, the fields Lapis reads through `text:` inside a script get their fullwidth forms, and tags lose them.
- A dictionary zip whose wrapper folder holds a second one (a wrapped dictionary extracted and zipped again) imports: wrapper folders are stripped until `index.json` is at the root, as in Fushi (BUG-2053).
- The dictionary registry is read and written in one transaction: an edit on the dictionaries page and the registration of an import, on its own thread, no longer overwrite each other (a deleted dictionary could come back). The registration checks the registry again: the same dictionary imported twice at once is registered once, and an update whose old dictionary was deleted meanwhile is added instead of failing.
- An update whose old folder cannot be removed is no longer reported as an error with the old dictionary still in the popups: it is registered, the lookup cache is dropped, and the next start removes the folder.
- A dictionary import stops promptly when cancelled, at every step: large banks are read, parsed and written a stretch at a time, and the final step (indexes, commit, compaction) looks at the cancel too, where it used to ignore it and register the dictionary anyway. With a 300k-entry dictionary in one bank (release build), a cancel used to wait up to 1.1 s inside the bank and was ignored in the final step; it now stops the import within 0.1 s, or 0.45 s when it comes during redb's commit.
- Quitting during a dictionary import cancels it and waits for its thread: the library is closed at exit, where the import's thread used to keep it open and the next start had to repair it.
- The Yomitan import keeps a kanji bank with malformed rows (a number where a string belongs, meanings that are not a list): those rows are skipped, as in Fushi (BUG-2952). An `index.json` that is not valid UTF-8 is read with U+FFFD instead of failing, and a reading-scoped frequency written `{reading, value}` keeps its reading (DIC-003).
