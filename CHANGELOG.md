# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Order study vocabulary by frequency, expression or reading, including exported selections (KNW-028, M37).

- Filter vocabulary by exact word status and card presence across study views (KNW-027, M36).

- Search vocabulary, encounter history and subtitle lists consistently across kana, width and case variants (KNW-026, M35).

- Reuse complete book and episode/series analysis counts across restarts in bounded, source/dictionary-validated private caches (KNW-025, M34).

- Order books and series by comprehension and filter scored material by a minimum known-word percentage (KNW-024, M33).

- Inspect and clear per-episode encounter history, with persistent sync resets preventing old-device resurrection (KNW-023, M32).

- Export shown or selected study vocabulary as a local UTF-8 TSV snapshot (KNW-022, M31).

- Study the combined vocabulary of an analysed series with episode coverage and shared bulk statuses (KNW-021, M30).

- Browse and bulk-mark vocabulary from completed whole-book analyses (KNW-020, M29).

- Cancel whole-book and series comprehension analyses, stopping their own workers and discarding stale replies (KNW-019, M28).

### Added

- DVB bitmap subtitle OCR (SUB-032, M27): Read as text uses the same local model and progressive pipeline as Blu-ray/DVD subtitles. FFmpeg converts the bitmap stream with an explicit matching palette, preserving the source images until text is ready. Teletext is excluded.

- External VobSub pairs (SUB-031, M26): open or drop `.idx` or `.sub` on the current video. The local model reads the preferred language with the index palette and timestamps; both source signatures invalidate the cache. Missing/damaged companions fail without caching a partial result.

- Progressive graphic subtitle OCR (SUB-030, M25): nearby pictures are read first in bounded batches. Their text becomes clickable while later pictures are still being read; unread lines keep their pictures. Lookups survive expanded timelines, and only completed text is cached or offered as a full comprehension/audio-export source. Disabling/replacing subtitles cancels the old work.

## [2.33.0] - 2026-11-13

The new words met while watching.

### Added

- Historical new words met per episode in Statistics (KNW-018, M21): with word colours enabled, distinct unknown headwords from active main text during playback are kept once per episode. Paused seeking and passive analysis do not count, and later status edits preserve history. Encounters survive restarts, backups and WebDAV sync; devices merge by word and earliest encounter time.

## [2.32.0] - 2026-11-13

Comprehension on the series cards.

### Added

- Series comprehension on Library cards (KNW-017, M21): Analyse/Refresh counts local subtitle sources without opening or playing the series. Scores sum word occurrences and lines across episodes and show analysed/total coverage and failed sources. Retained counts follow shared statuses; language, tool and dictionary changes invalidate them.

## [2.31.0] - 2026-11-13

Comprehension across a whole book before opening it.

### Added

- Whole-book comprehension from the Books list (KNW-016, M21): with word colours enabled, Analyse counts every text chapter of a local EPUB or saved web page without opening it or changing reading progress. Refresh rereads the source; retained counts follow shared word statuses. Analysis runs on its own worker, skips images and manga, and reports source errors or size limits without a partial score.

## [2.30.0] - 2026-11-12

Choose an episode by its embedded subtitles too.

### Added

- Episode comprehension scores before playback now include embedded text subtitles in local videos (KNW-008, M21). Selection follows the player's saved track and language priorities, including a preferred-language embedded track ahead of another language's external file. Graphic subtitles and remote videos stay unscored.
- Probing and extraction run on the episode counter's worker, with a private cache and cancellation at shutdown. Reopening a series rechecks its subtitle sources; language/tool setting changes invalidate earlier results, and stale replies cannot replace the new scores.

## [2.29.0] - 2026-11-12

The words of the chapter you are reading.

### Added

- Word status underlines in EPUB and web pages, in horizontal and vertical writing (KNW-002, M20). Settings → Subtitles → Colour the words applies to the reader too, using the same statuses as subtitles and Anki.
- The current chapter's comprehension score and Words view (KNW-014, KNW-015, M21): known word tokens, paragraphs with no new word, and headwords by frequency. Search or narrow to new/learning words, select several and change their status together. Returning to Read keeps your position. Very long chapters remain readable without a partial score; manga image chapters are excluded.

### Fixed

- Auto-pause keeps the current subtitle's stop briefly when an end-position event arrives before the player's pause event, preventing the stop from being advanced too early (AK-BUG-007).

## [2.28.0] - 2026-11-12

Videos from web pages, with clickable subtitles.

### Added

- Videos from web addresses (PLY-023, M24): Open web video on the Library page or player bar, a dropped URL or a URL on the command line. The user's yt-dlp resolves the media and downloads uploaded or automatic text subtitles in the preferred languages; the subtitles can be clicked and mined, including videos with separate audio streams. The original page is kept for resume and the card's source link. Choose the executable in Settings → Advanced, `NIKOMIRU_YT_DLP` or PATH. On-demand HTTP media is supported; live, DRM and segmented-only streams are not.

## [2.27.0] - 2026-11-12

Listening between study sessions.

### Added

- Condensed audio (AUD-010, M23): export an episode from its subtitle menu, an episode from the library or a whole series, as Opus or MP3. The lines keep the card audio's padding, nearby lines are joined and every line gets a chapter. The current episode uses the current subtitle offset and audio track; library exports also read embedded text tracks. Exports can be cancelled, report failures per episode and never overwrite an existing file.
- Speed through pauses between subtitle lines (PLY-022, M23): Settings → General → Playback offers 2–4× for gaps of at least three seconds, off by default. Dialogue keeps the chosen playback speed, as do its edges; pause, replay and popups restore it, and the temporary speed is never saved for an episode.

### Fixed

- Gap speed restores earlier before the next subtitle line, allowing for audio queued at the faster rate (AK-BUG-006).

## [2.26.0] - 2026-11-10

The pitch accent on the words.

### Added

- "Colour the pitch accent of the words" in Settings → Subtitles (LKP-062): from your pitch dictionaries, a line over each word of the subtitles in the colour of its accent, as Migaku shows it. Heiban is blue, atamadaka red, nakadaka orange, odaka green and kifuku purple.

## [2.25.0] - 2026-11-10

Help only where a word is new.

### Added

- With the words coloured, Settings → Subtitles has two new choices:
  - "Lines with no new word" (KNW-011): blur or hide the main subtitle while every word of the line is known, so it shows only where a word is new; the pointer still reveals it.
  - "Pause at the end of a line only when it has a new word" (KNW-012), with the pause at the end of each line on.
- The subtitle list's lines can be narrowed to those with a new word, or with just one (KNW-013).

## [2.24.0] - 2026-11-10

The words of an episode, and the words you know.

### Added

- A Words tab in the subtitle list, with the words coloured (KNW-009): every word of the episode with how often it comes, what you know of it and whether it has a card, the most frequent first.
  - Narrow it with the search or to the words you don't know yet.
  - Tick some, or all those shown, and mark them known, learning, new or ignored at once.
- Words in the Statistics tab (KNW-010): how many words you set known, learning and ignored, how many Anki says you know, and the words known over the last 30 days.

## [2.23.0] - 2026-11-10

How much of an episode you will understand, before you play it.

### Added

- The episodes of a series show their comprehension score on the Library page, with the words coloured (KNW-008): `80% · 25%` is the share of the words you know and the share of the lines with no new word, written out in full on hover.
- The score is counted from the subtitle file the episode would play with: the one you chose, else the one in your language next to it or downloaded for it. Episodes that would play an embedded track have none yet.
- The words are counted once, away from the player, and the scores follow every status you set.

## [2.22.0] - 2026-11-09

How much of an episode you understand.

### Added

- The comprehension score of the subtitle you are watching, as Migaku shows it (KNW-007). With the words coloured, the subtitle list's header says:
  - the share of the words you know, each counted every time it comes;
  - the share of the lines with no new word.
- Ignored words are left out of it, and a word you are learning does not count as known. The score follows each status you set.
- The scores of episodes before you play them, on the library's pages, come in 2.23.0.

## [2.21.0] - 2026-11-09

The words you know, from Anki.

### Added

- Known words from your Anki decks, as Migaku and asbplayer take them (KNW-005). Name the decks in Settings → Subtitles, with the field that holds the word (`Expression` by default):
  - a word whose card has an interval of 21 days or more is known, one whose card was studied is being learned;
  - they are read each time Nikomiru starts with the colours on, or with "Read them from Anki now", and kept for when Anki is closed;
  - a status you set in a popup wins.
- Word lists imported and exported (KNW-006):
  - a list has one word per line, or `word,reading,status` lines;
  - the export writes your statuses in the second form, which reads back the same.

## [2.20.0] - 2026-11-09

Words you know, and words you don't.

### Added

- Words coloured by what you know of them, as Migaku and asbplayer do (KNW-001…KNW-004, D-52). Turn it on in Settings → Subtitles, "Colour the words by what you know of them":
  - words not known yet are underlined in blue, words being learned in yellow, in the subtitles and in their list;
  - in a dictionary popup, mark each word new, learning, known or ignored; a word you made a card of counts as learning;
  - a line with a single new word (i+1) is marked `+1` in the subtitle list;
  - the statuses are kept, and synced between devices by the WebDAV sync;
  - readings can be shown only over the words not known (`[subtitles] furigana_unknown_only`).

## [2.19.0] - 2026-11-09

DVD subtitles you can click.

### Added

- DVD image subtitles (VobSub) read as text on this computer, as Blu-ray ones are since 2.18.0, so their words can be clicked, listed and made into cards (SUB-029, D-50):
  - "read as text" beside the track in the subtitle menu, and a Japanese VobSub track chosen by itself is read when the model is installed (`[subtitles] read_graphic`);
  - the track's own colours are used, from its `.idx` header.

## [2.18.1] - 2026-11-09

### Fixed

- The setup assistant tries a dictionary again when GitHub answers a passing error such as HTTP 504, as KANJIDIC and Kanjium did at a first setup; three tries in all (AK-BUG-005).

## [2.18.0] - 2026-11-09

Blu-ray subtitles you can click.

### Added

- Blu-ray image subtitles (PGS) read as text on this computer by the manga's reading model, so their words can be clicked, listed and made into cards like those of a text track (SUB-028, D-49):
  - "read as text" beside the track in the subtitle menu; a Japanese PGS track chosen by itself is read when the model is installed (`[subtitles] read_graphic`, on by default);
  - the images stay until the text is ready, and the text is kept for the next time;
  - without the model, choosing "read as text" downloads it once (117 MB);
  - furigana above a line is left out, and two lines side by side are read apart.

### Changed

- The text read from a manga page writes ASCII in its full-width forms, as manga-ocr does (`!` → `！`).

## [2.17.1] - 2026-11-09

### Fixed

- The setup assistant opens from Settings → General and from the library's first steps: it opened under those pages, so the button seemed to do nothing (AK-BUG-004).

## [2.17.0] - 2026-11-08

Ready from the first start.

### Added

- The setup assistant, at the first start and from Settings → General or the library's first steps (INS-001…INS-006, D-48), as Fushi's welcome wizard:
  - free dictionaries downloaded from where their makers publish them: Jitendex, KANJIDIC, Kanjium's pitch accents, JPDB frequencies, JMnedict;
  - word audio from Fushi's recommended pack, taken by range: only the library chosen (Japanese, 6.3 GB; English, 1.3 GB) out of the 10.2 GB pack, checked, resumed after a stop or a restart, and added to the word audio sources;
  - cards: Anki, with the Lapis note type found or made in one click and the steps to install AnkiConnect when Anki does not answer, or Nikomiru's review.

## [2.16.0] - 2026-11-08

Subtitles where you want them.

### Added

- Subtitle menu → Move the subtitles: drag the main subtitle up or down; dropped, it keeps that edge and distance, as in Settings → Subtitles → Look (SUB-027, D-47).

### Fixed

- The pause at the end of each line: the next line plays with its sound again, and so does the video after a replay from the popup has stopped (AK-BUG-003).

## [2.15.0] - 2026-11-08

One line at a time.

### Added

- Pause at the end of each line (Shift+P, or the subtitle menu): playback stops on the last frame of each line, and Space plays the next one (SUB-026, D-46).

## [2.14.0] - 2026-11-08

Fansubs in their own fonts.

### Added

- The fonts attached to an MKV file are used by its ASS subtitles when ASS styles are respected, as fansubs expect (SUB-025, D-45).

## [2.13.0] - 2026-11-08

The subtitles in the font you like.

### Added

- Settings → Subtitles → Look → Font: any installed font with Japanese characters for the subtitles; with ASS styles respected, the font of each style when it is installed (SUB-024, D-44).

## [2.12.0] - 2026-11-08

Cards that wait for Anki.

### Added

- Settings → Anki → Cards go to → Anki, later: cards are made at once, with their audio and picture, and kept on this computer; Send to Anki sends them when Anki is open, and the ones it refuses stay with the reason (ANK-044, D-43).

## [2.11.0] - 2026-11-08

Subtitles that look the way you want.

### Added

- Settings → Subtitles: the furigana's size, 30 to 80 % of the text (LKP-052).
- Settings → Subtitles → Look: the text's color, bold, the outline's color and width, the shadow, a box behind the text, the main subtitle at the bottom or the top and its distance from the edge, with a preview of a cue as it will look (SUB-021, D-42).
- Blurred or hidden subtitles can stay so under the pointer: a pause and an open popup still show them (SUB-022).
- The subtitle list's text size and width (SUB-023).
- A click on the time in the control bar shows the time left, and is remembered (PLY-020).
- Settings → General → Picture: the whole picture with bars, the window filled with the edges cut, or the picture stretched to it; the subtitles follow (PLY-021). `panscan` and `keepaspect` in the mpv options are now left to it.

## [2.10.1] - 2026-11-03

### Fixed

- Review: the answer of a card no longer hides itself again when the tab finishes reading just after Show the answer (AK-BUG-002).
- A use-after-free that async-executor 1.14.0, used for the desktop portals and the keyring, could hit when it shut down: 1.14.1 fixes it, and 1.14.0 was withdrawn from crates.io.

## [2.10.0] - 2026-11-03

A new look.

### Changed

- The interface takes after GNOME's Adwaita, in the dark and in the light theme: graphite surfaces, flat buttons, rounded corners, soft shadows, and each page's main action (Save, Apply, Show the answer, Open) filled with the accent (SET-009, D-40).
- The default accent is GNOME's blue. A settings file that still has the blue of before takes the new one; an accent you chose stays.

## [2.9.0] - 2026-11-02

An audiobook and its book, together.

### Added

- Books → Read along: with an audiobook playing, its EPUB follows what you hear, the paragraph marked, across chapters; Listen from here plays from the paragraph at the top of the page (ABK-005, D-39). The lines are found in the book by their text, so the audiobook's own timed text will do.

## [2.8.0] - 2026-11-02

Every card of the review in sight.

### Added

- Review → Cards: the list of every card, with a search on word, reading and sentence, when each comes back and its answers; Start over makes a card new again, and Delete removes it, each clicked twice (REV-009, D-38).

## [2.7.0] - 2026-11-02

How the review goes.

### Added

- The review's statistics on the Statistics tab: today's answers, the share remembered in review over the last 30 days, new, learning, young and mature cards, the answers of each day and the cards due in the next week (REV-008). Every answer is noted for them, in backups and in the WebDAV sync, and Undo takes it out again (REV-007, D-37).

## [2.6.0] - 2026-11-02

Web pages, read like books.

### Added

- Books → Web page: an address typed there opens as a book of one chapter, with lookups, cards, its place and the statistics; HTML files of this computer open too, in any Japanese encoding (WEB-001…WEB-003, D-36). Only that page is fetched, when asked: no pictures, nothing it links to.

## [2.5.0] - 2026-11-02

The review on every computer.

### Added

- The review's cards, with their schedule, sound and pictures, in the WebDAV sync (REV-006, D-35). A file that does not hold what its name says is refused; a card deleted on one computer goes from the others.

## [2.4.0] - 2026-11-02

Cards that stay in Nikomiru, for those without Anki.

### Added

- Nikomiru's own review: Settings → Anki → Cards go to → Nikomiru's review keeps the cards in the library, and the Library page's Review tab brings them back on a schedule worked out with FSRS-5, written here (REV-001…REV-005, D-34). Anki is not needed; it stays the default.

## [2.3.0] - 2026-11-02

Keys and passwords in the system keyring, for those who want them there.

### Added

- Settings → General → Keep them in the system keyring: API keys, passwords and server tokens go to GNOME Keyring, KWallet or the like instead of `config.toml` (MET-005, D-33). Off by default, and off nothing talks to the keyring. A key leaves the file only once the keyring gives it back.

## [2.2.0] - 2026-11-02

The interface in Japanese.

### Added

- Japanese interface: Settings → General → Language, or by itself with a Japanese system locale (SET-004, D-32).

### Fixed

- A short message at the top of the window wraps only when it has to, not at the width of the one before (AK-BUG-001).

## [2.1.0] - 2026-11-02

Video cards, and what 2.0.0 left out of manga.

### Added

- A card's picture can be its sentence as a WebM video with sound (ANK-043, D-31): Settings → Anki → image format.
- Manga from a folder of pictures, and two pages side by side, read from the right, with the cover and wide pictures alone (MNG-001, MNG-002).

## [2.0.0] - 2026-11-01

Manga, with which the direction of D-12 is complete: books, audiobooks and manga read with the same dictionary and cards.

### Added

- Manga (M16, MNG-001…MNG-006): CBZ files on the Books page, one page at a time, with the place kept. A drag across a balloon reads it on this computer; the text is clicked like a book's, and a card has the text and the part of the page.
- The reading model, 111 MB, is fetched from Hugging Face only when asked for, at pinned revisions and checked by SHA-256 (D-30). Pages are never sent anywhere.

### Fixed

- A picture of the same name in another book showed the picture of the book read before (BOK-010).

## [1.7.0] - 2026-11-01

Audiobooks.

### Added

- Audiobooks (M16, ABK-001…ABK-004): sound files open from the Books page or like any file, with their text from a subtitle file or LRC beside them. The subtitle list opens to read along, words are looked up by click, and a card has the line and its sound.
- LRC files are read like the other subtitles (SUB-020).

## [1.6.0] - 2026-11-01

Vertical writing and pictures in books, and frames of videos the GPU decodes.

### Added

- Vertical writing in books (BOK-009): columns from the right, pages, furigana beside the text. A book that says it reads from the right opens that way, and "Vertical" chooses for all.
- Pictures in books (BOK-010).
- The word looked up is marked in books and in the texthooker.
- How far a book was read goes through the WebDAV sync.
- A "List" button in the bar opens the subtitle list, and ⚙ on the Library page opens the settings.

### Fixed

- `S` and the picture of a card work on videos the GPU decodes: mpv 0.41 cannot read those frames back, so `S` saved nothing and cards had no picture. ffmpeg now takes the same moment from the file.

## [1.5.0] - 2026-11-01

Media servers, backup and sync, advanced ASS, a texthooker, more dictionary formats and books (M11–M15, and the books of M16). They ship together (D-26).

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

### Changed

- SPECIFICATION: backup and WebDAV sync of M12 (BAK-001…BAK-010, D-21), with their tests in TEST.
- SPECIFICATION: the media servers of M11 (SRV-001…SRV-013, D-20), with their tests in TEST.
- ROADMAP: the milestones after 1.0, in the maintainer's order (D-19): M11 Jellyfin and Plex, M12 backup and WebDAV sync, M13 advanced ASS and screenshots with subtitles, M14 clipboard lookup and texthooker, M15 MDict, StarDict and DSL, M16 books, manga and audiobooks.

### Fixed

- A subtitle file dropped on a video that has just opened stays: the automatic choice could come after it and replace it.
- External subtitles of a real Jellyfin are offered: it names SubRip `subrip`, which gave a file no one read as a subtitle.
- A card has the tag of its series (ANK-027): it was written in the specification, and never made.

### Removed

- The spikes (`spikes/`): their decisions are in the ADRs and their code in the tag `v1.0.0`. The large synthetic audio database of spike S3 is now made by `xtask/scripts/android_db_large.py`.

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
