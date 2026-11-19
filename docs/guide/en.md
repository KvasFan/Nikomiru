# Nikomiru user guide

Nikomiru is a video player for learning Japanese. Click any word of a subtitle to look it up in your Yomitan dictionaries, then make an Anki card from it in one click.

*Italiano: [guida in italiano](it.md).*

## 1. Install

### Arch Linux

The PKGBUILD is in the repository, in `packaging/aur`. Once Nikomiru is on the AUR, `yay -S nikomiru` will do the same.

```sh
git clone https://github.com/KvasFan/nikomiru.git
cd nikomiru/packaging/aur
makepkg -si
```

### Flatpak

A bundle built with `cargo xtask flatpak` installs with `flatpak install --user nikomiru.flatpak`; on Flathub later. The Flatpak brings its own libmpv and uses the FFmpeg of the freedesktop runtime.

### From source

You need:

- Rust through `rustup`;
- libmpv ≥ 0.37 and FFmpeg ≥ 6;
- OpenGL with EGL, and the Wayland or X11 client libraries;
- libxkbcommon, fontconfig and OpenSSL 3;
- a font with Japanese glyphs, for example Noto Sans CJK.

Building also needs the development files of libmpv and OpenSSL, and `pkg-config`.

```sh
git clone https://github.com/KvasFan/nikomiru.git
cd nikomiru
cargo xtask install      # into ~/.local, with the menu entry and the icon
```

## 2. First start

Start Nikomiru from the menu, or with `nikomiru path/to/episode.mkv`. Without a file it opens on the **Library**, which starts with the **first steps**: a dictionary, a folder of videos and a first card, each with a button to it. They go once done, or with **Hide**.

- **Open a video**: Ctrl+O, drag it onto the window, or open it from the library.
- **Subtitles**: Nikomiru loads the ones next to the video by itself, even when their name differs (`Subs/` folders, `Show 02.ja.srt`). It also loads the subtitle tracks inside the video. To choose, use the 字 menu in the bar.
- **Back**: Esc, or "← Library" in the bar.
- **Find a scene**: the pointer over the progress bar shows the time and the frame there.

## 3. Dictionaries

Nikomiru reads the same ZIP files as Yomitan: terms (JMdict, Jitendex…), kanji, frequencies and pitch accent. It ships with none.

1. Open the dictionaries: 📚 in the bar, or Settings → Dictionaries.
2. Choose **Import…**, or drag the ZIP onto the window.
3. Each type has its own list, ordered by priority; drag a dictionary by its handle to change its place.
4. **CSS** lets you restyle a dictionary's entries. Only a safe subset of CSS applies.

Besides Yomitan's ZIP files, the import takes StarDict (choose the `.ifo`), DSL (`.dsl`, `.dsl.dz`) and MDict (`.mdx`) dictionaries. StarDict and DSL definitions show as plain text, without pictures. MDict definitions keep their bold, italics, lists, tables and ruby, and links to other words open a lookup; scripts, styles and other markup are dropped (M114 development). Keep the `.mdd` file(s) beside the `.mdx` (`name.mdd`, `name.1.mdd`…): the pictures and sounds the definitions use are imported with it. A sound link (▶) plays only when you click it, and cards keep the pictures and the sounds Anki plays (MP3, Ogg, WAV, M4A, FLAC). Without the `.mdd`, pictures show their alternative text. An MDict locked to a user cannot be read.

Graphic subtitles read as text now appear progressively: OCR starts near the current playback position, and unread lines keep their pictures. Text already read can be looked up while work continues. Whole-track comprehension, gap acceleration and condensed-audio export wait for completion; changing or disabling subtitles cancels the earlier reading.

To read an external VobSub pair, open or drop its `.idx` or `.sub` file while a video is loaded. Keep the same-stem companion beside it; extension case does not matter. The preferred language is read with the local model and index timestamps. Reopening the video restores this explicit choice. External pairs have no picture fallback while text is pending; missing or damaged companions report an error.

Embedded DVB bitmap subtitles also offer Read as text. FFmpeg converts their pictures locally for the installed model; teletext is excluded.

## 4. Looking words up

- **Click** a character of a subtitle: the popup shows the longest word that starts there, its readings, meanings, pitch accent and frequencies.
- **Shift + hover** looks words up without clicking. In Settings → Lookup you can make plain hover work too.
- **Keyboard**:
  - Enter looks up the subtitle shown;
  - ← → move the lookup to the next or previous word, then by character past the last one;
  - Tab and Shift+Tab move between the entries;
  - Esc closes the popup.
- **Select text** in a definition to look it up in a nested popup.
- **Replay** plays the line again; **Go to the sentence** jumps to its start.
- **♪** plays the word itself, if you set up a word audio source in Settings → Word audio.
- **Words you already mined** are underlined in green in the subtitles and in the subtitle list. In Settings → Subtitles you can turn that off, or turn on readings over the kanji. Both come from your dictionaries, which split each line into words: where a word is cut wrong, its reading can be wrong too.

To hear a word, Nikomiru needs a source of pronunciations: none comes with it. Without one, the popup shows `♪?`, which opens Settings → Word audio: add a local Local Audio Server `android.db`, or an address with `{term}` and `{reading}`.

## 5. Anki cards

1. Install the [AnkiConnect](https://ankiweb.net/shared/info/2055492159) add-on in Anki (code `2055492159`) and keep Anki open.
2. In Nikomiru, Settings → Anki: **Test the connection**, then **Create Lapis deck**. If you prefer, choose your own deck and note type and map their fields.
3. In a popup, **+** on an entry (or Ctrl+Enter) makes the card. It includes:
   - the word and its meanings;
   - the sentence with the word in bold;
   - the frame of the video (or, in Settings → Anki → Image format, the whole line as an animated picture or a WebM video with sound) and the audio of the line;
   - a link that reopens the scene.
4. **✓** means Anki already has the word: its menu opens the card in Anki, or adds another anyway. On the card you just made, **✓↩**, the menu can also replace it or **Undo** it: the card is deleted from Anki.
5. **Shift+click on +** (or Ctrl+Shift+Enter) makes the card by hand. You can:
   - add the lines before and after;
   - move the clip and listen to it;
   - choose the picture, or leave the audio out.

### Without Anki: Nikomiru's review

In Settings → Anki, **Cards go to → Nikomiru's review** keeps the cards in Nikomiru: Anki is not needed. The **+** of a popup then makes them there, with the sentence, its audio, the frame and the meanings.

They come back on the **Review** tab of the Library page, on a schedule worked out with FSRS:

1. The front shows the word and its sentence. Think of the meaning, then **Show the answer** (Space or Enter): the reading appears and the sentence plays.
2. Answer with **Again**, **Hard**, **Good** or **Easy** (keys 1–4). Each button says when the card comes back.
3. **Undo** (Ctrl+Z) takes the last answer back. **Delete**, clicked twice, removes the card.

**Cards**, at the top of the tab, lists every card with a search: when each comes back, its answers, and **Start over** to learn one again from the beginning.

The Statistics tab shows how the review goes: today's answers, the share remembered in the last 30 days, young and mature cards, and the cards due in the next week.

The share of cards to remember when they come back (90 % by default) and the new cards a day (20) are set in the same tab. The review is in backups and in the WebDAV sync, with its sound and pictures.

## 6. Online subtitles

Nothing goes online unless you ask for it.

1. In Settings → Subtitle services, tick Jimaku and/or OpenSubtitles and paste their keys. Jimaku's key is free; OpenSubtitles needs a consumer key.
2. In the 字 menu, **Search online…** finds subtitles for the episode. **Use** downloads one and loads it.
3. Optionally, **automatic subtitles** fetch them by themselves for a video that has none in Japanese.
4. **Align with the audio** (字 menu) fixes subtitles that are early or late.

## 7. Library

The main page has a row of tabs:

- **Collection**: your imported series and films, and what to continue.
- **Videos**: the videos you opened on their own (files, web videos, media servers), the last first, with their progress; **✕** takes one out of the history. Above them, **Saved links**: type a video page's address and a name and press **Save link**, then **Play** it whenever you like. A web video of the history is saved with one click.
- **Books**, **Manga** and **Websites**: what you added of each kind, with how far you read. **Read** (or **Continue**) opens it; closing the reader brings you back to the tab. Each tab has its own buttons under the tabs: add a book, paste text or an audiobook; add a manga or a folder of pages; type a web page's address.
- **Statistics**, the **Review** once you use it, and each media server.

The **Tools** menu holds the Texthooker, the phrase notebook, the recall history and files, podcasts, annotations, the search through your books and the list of every book.

- **Add folder…** imports your series. Episodes are grouped by their names (seasons, episode numbers), or by folder.
- **Continue watching** shows what you started.
- When an episode ends, the next one starts after a 5-second countdown.
- A series remembers its subtitle, offset and audio language.
- With word colours on, each episode shows `80% · 25%`: known word tokens and subtitle lines without a new word. Local embedded text subtitles count too, using the same saved track and language priorities as playback. `…` means counting; graphic subtitles, remote videos and subtitles turned off have no score. Reopen the series to refresh changed subtitle sources.
- On a series' page you can rename it, merge it, move or remove episodes, or delete it. Your files are never touched.
- Tick episodes to **Mark watched** or **Mark unwatched (from the start)**; the mark follows the sync to your other computers.
- **Identify online** asks AniList (or MyAnimeList) for the series' titles, episodes and cover. If several series match, you choose. Jimaku then finds its subtitles by its AniList id.
- **Identify by file (AniDB)** works even when folder names say nothing. It sends the size and ed2k hash of one episode file to AniDB with your AniDB account (Settings → Subtitle services), then looks for AniDB's title on AniList or MyAnimeList. AniDB accepts only registered clients: until Nikomiru's registration is built in, register a UDP client at anidb.net and enter its name and version there. AniDB is not asked while a proxy is set, because its UDP API cannot go through one.
- **Statistics** shows the time watched and the characters read:
  - by day, by series and by session;
  - a calendar of the year;
  - a CSV export.

With word colours enabled, **Analyse** below a series card counts comprehension without opening or playing it. The score combines word occurrences and lines across the episodes with usable text; the analysed/total episode count shows its coverage, including the number that failed. **Refresh** rereads the series and sources. At most four series results are retained; changing word statuses rescores them immediately. Series above 256 episodes have no aggregate score.

With word colours enabled, Statistics also shows **New words met by episode**: distinct headwords that were new when encountered in the active main subtitle during playback. Repetitions and replay count once per episode; paused seeking and analyses from the library do not count. The history stays when a word becomes known, survives a restart and travels with backups and WebDAV sync. Earlier playback and lines not segmented while active are not estimated afterwards. The 200 most recently encountered episodes are shown.

### Texthooker

For visual novels and games: "Texthooker" on the Library page shows the lines Textractor or LunaTranslator send over their WebSocket (`ws://127.0.0.1:6677` by default). Click a word to look it up; a card from there has the word and the sentence, with no picture or audio. Ctrl+V adds what you copied. Reading the clipboard by itself needs `wl-paste` or `xclip`, and does not work on GNOME Wayland.

### Books

"Books" on the Library page reads EPUB files: choose one, and its file stays where it is. Click a word to look it up, and make a card from the popup, with the sentence and no picture or audio. A book reopens where you left it. "Vertical" switches to vertical writing, with ← and → or the wheel to turn the page; a book that reads from the right opens that way.

With **Colour the words by what you know of them** on in Settings → Subtitles, books and web pages underline new words in blue and learning words in yellow. The chapter's score shows known word tokens and paragraphs without a new word; ignored words are left out. **Words** lists the chapter's headwords by frequency: search by word or reading, filter to new/learning words, tick several or all shown, then choose their status. **Read** returns to your reading position. Very long chapters remain readable without a chapter score or word list. Exceptionally long paragraphs are not coloured.

In the Books list, **Analyse** shows comprehension across every text chapter without opening the book or changing your reading position. Word colours must be enabled. **Refresh** reads the local file again; changing word statuses updates a retained score immediately. Up to four analyses are kept until you close the app, turn off word colours or change dictionaries. Saved web pages use their local text. Manga, image-only books and books above the analysis limits have no score; they can still be opened as usual.

### Web pages

On the Books page, type a page's address next to **Web page** and press **Open**: an article of NHK News Web Easy, a chapter of a web novel, a work of Aozora Bunko. Nikomiru fetches that page only, nothing it links to and no pictures, and reads its text like a book's, menus and footers left out, furigana kept. It stays in the list, with its site as the author, and reopens without the network. An HTML file saved on your computer opens with "Add a book or manga…" too, in any Japanese encoding.

### Manga

Add a CBZ or a folder of pictures from the Manga tab. "Two pages" shows them side by side, the first on the right. The left arrow, Page Down, Space and the wheel go forward, the right arrow and Page Up back.

To read a balloon, drag a box around it. The first time, the page offers a reading model of 111 MB: it comes from Hugging Face once, only if you ask, and then runs on your computer, so pages go nowhere. The balloon is read and its first word opens in the dictionary at once; the whole text appears above the page, where you click the other words. A card has the text and the part of the page. The model reads one balloon or line at a time, and takes a second or two.

With the text detector too (90 MB more, offered on the page and fetched with the model in the setup), Nikomiru reads the page you are on and the next one in the background: the line over the page says when it is done, and a click on a word looks it up at once, as in a Mokuro volume. The text read is written over each balloon, at about the size of the drawing's, and a click on a character looks its word up; **Text on the page** turns it off to see the drawing. How many pages are read ahead, and the memory their text may take, are set in Settings → General → Manga. To keep the dictionary off the text, drag it by **⠿ Move** where you want it: in manga it opens there from then on, also after a restart; ↺ puts it back next to the word. Drag a box over any text it missed. A page read is kept, so a manga opened again is ready at once.

Hold Ctrl and turn the wheel (or pinch on a touchpad) to zoom: in a book the text grows or shrinks and stays so; in a manga the page is enlarged about the pointer. An enlarged page is moved with the wheel (Shift for sideways) or by dragging with the middle button; the arrows still turn pages, and a new page opens at its top right.

A Mokuro volume already has its text: a click on a word looks it up directly, as in subtitles.

### Audiobooks

"Open an audiobook…" on the Books page plays a sound file. Put its text beside it with the same name, as `.lrc`, `.srt`, `.vtt` or `.ass`: Nikomiru does not make it. The subtitle list opens to read along; click a word to look it up, and a card has the line and its sound.

With the audiobook playing, open its book on the Books page and press **Read along**: the page follows what you hear, the paragraph marked, and **Listen from here** plays from the paragraph at the top of the page.

### Jellyfin, Emby and Plex

Add a server in Settings → Media servers: for Jellyfin or Emby (choose which) the address, user and password (the password is not kept); for Plex, sign in on plex.tv in your browser. Each server gets a tab on the library page. Its videos play from the original file, with the same subtitles, lookups and cards as a local one, and your position and watched state go back to the server. Nikomiru connects to a server only while its tab is open or one of its videos plays. An Emby is checked to be one before logging in; a file it cannot play directly shows an error instead of being converted (M116 development).

**Open web video** on the Library page or the player bar accepts a video page URL. You can also drop the URL or pass it on the command line. Install `yt-dlp` yourself; if it is not on PATH, set its executable in Settings → Advanced (or `NIKOMIRU_YT_DLP`). The preferred subtitle languages apply to uploaded and automatic subtitles. Web videos use the same dictionary popups and cards, and their source links reopen the original page at the scene. On-demand direct HTTP media is supported, including separate audio/video streams; live, DRM and segmented-only streams are not.

## 8. Settings

⚙ in the bar, or Ctrl+, opens the settings. The tabs run down the left side.

| Tab | What it holds |
|---|---|
| General | Language (English, Italian or Japanese), theme (light, dark or as the desktop), accent color, interface scale, seek step, next episode, keys and passwords in the system keyring |
| Player | mpv's options as controls, `mpv.conf` presets, import and export |
| Subtitles | Size, preferred languages, blur or hide until hovered, the secondary subtitle, readings over the kanji, mined words underlined |
| Lookup | Results in a popup, the popup's size, hover |
| Shortcuts | Every key, which you can change. Keys are physical: they stay put whatever the keyboard layout |
| Advanced | The ffmpeg program, where the settings file and the logs are |

Each tab has **Save** and **Reset**. `~/.config/nikomiru/config.toml` can also be edited by hand: Nikomiru keeps your comments.

General → Playback → **Speed between subtitle lines** speeds through pauses of at least three seconds at 2–4×, returning to your chosen speed before the next line. It needs a text subtitle and is off by default. Pause, dictionary popups and replays keep your chosen speed; the faster gap speed is never saved for an episode.

The subtitle menu and the library's episode/series menus offer **Export condensed audio**: choose Opus or MP3 and a folder. Each episode gets a file containing only the subtitle lines, with the padding configured for cards; gaps under a second are kept. Chapters follow the lines. Existing files are kept, and the menu can cancel a running export. Series exports report episodes without usable text subtitles or audio separately.

### Backup and sync

Settings → Backup writes your settings, library, statistics and downloaded subtitles to one file, with the dictionaries if you tick them. Keys and passwords stay out unless you tick them too. To restore, choose a backup and reopen Nikomiru: what was there stays in `before-restore`.

With a WebDAV folder (Nextcloud and the like, with an app password), "Sync now" keeps progress, statistics and cards, the review's too, the same on your computers. Settings, series and dictionaries stay on each computer, and the files on the server are not encrypted. A video is the same on two computers when its file has the same name.

## 9. Keyboard

The whole app works from the keyboard. On pages and dialogs, Tab, the arrows, Enter and Space drive the controls. Every shortcut is listed, and can be changed, in Settings → Shortcuts.

**Export shortcuts…** saves every action's keys as a small JSON file (physical keys, so the same keys on any keyboard layout); **Import shortcuts…** reads one and shows what would change, actions it does not know (ignored), keys it does not recognise and conflicts. Nothing changes until **Apply**, which is available only without invalid keys or conflicts and saves at once; **Cancel** drops the preview. **Reset** still restores the defaults. Typing in a text field still never triggers single-key shortcuts. (M113 development)

| Key | Action |
|---|---|
| Space, P | Play/pause |
| ← → / A D | Seek back / forward |
| Ctrl+← Ctrl+→ | Previous / next subtitle |
| R / Shift+R | Replay the line / the previous one |
| Enter | Look up the subtitle shown |
| Ctrl+Enter | Create the card |
| L / Ctrl+F | Subtitle list / search it |
| B / H / V | Blur / hide / turn off the subtitles |
| Z / X | Subtitles 100 ms earlier / later |
| F, F11 | Fullscreen |
| S | Save the frame |
| Shift+S | Save the frame with its subtitles |
| Shift+T / G / Y / O | Translate / explain / shadow the line; read text from the picture |
| Ctrl+O | Open a file |
| Ctrl+, | Settings |
| Esc | Back |

## 10. When something goes wrong

- The logs are in `~/.local/state/nikomiru/logs/`. They never contain your keys.
- If the settings cannot be saved, check that `~/.config/nikomiru` belongs to you. If it was created with `sudo`, give it back with `sudo chown -R $USER: ~/.config/nikomiru`.
- If a setting in `config.toml` has a value of the wrong kind, Nikomiru uses the default for it and names it in a notice at startup.
- Problems and ideas: <https://github.com/KvasFan/nikomiru/issues>.

While a whole-book or series analysis is running, Cancel analysis stops it and discards its incomplete result. You can immediately analyse another item; playback and episode-list scores continue.

After a whole-book analysis finishes, Study words opens the vocabulary of every chapter. Search by word or reading, filter unknown/learning words, select rows or All shown, and choose a status for them together. The whole-book score follows those shared changes. Refreshing the analysis closes the old list.

Series cards also offer Study words after analysis. Frequencies sum across the counted episodes. Coverage remains visible: excluded or failed episodes do not contribute words. Changing a status updates the series score and other study views.

In either analysis vocabulary list, Export shown saves the current search/filter results; Export selected saves all ticked words, even hidden ones. The local UTF-8 TSV includes expression, reading, status, count and card. The export is a snapshot taken when clicked, limited to 16 MiB; it does not change your word statuses.

Statistics → Inspect history shows the latest 1,000 historically encountered words for an episode, with their first encounter time in Unix milliseconds. Search a word or reading. Clear episode history removes that episode’s history and syncs a persistent reset, so old device files cannot bring it back. Word statuses and progress are kept. Words already deduplicated in the current playback visit remain suppressed until a new visit.

With word colours enabled, Books and local series offer Comprehension order: original, most known or least known. Only scored can restrict the list to completed scores, with a minimum known-word percentage. Unscored items sort last; changing statuses updates the order. These controls do not start analyses or change the stored library order.

Completed whole-book and episode/series analyses can reuse local count caches after restart. They validate source content and dictionary settings, then apply current word statuses. Counts are limited to 64 MiB combined (32 MiB each for books and episodes); corrupt, oversized or cancelled results are ignored. Analyses remain explicit.

Search in vocabulary, historical words and subtitle lists accepts kana-script and width variants and ignores letter case. Separate terms with whitespace to require all of them; words can match either expression or reading. This changes filtering only, keeping the original spelling and lookup targets.

Vocabulary lists also offer exact status and card-presence filters. They combine with the search and Only words not known. Hidden ticked words stay selected for bulk actions or Export selected; Export shown follows all current filters.

Vocabulary order offers most/least frequent, expression and reading order. Missing readings sort by expression; lexical order uses the original Unicode strings. Selected words stay selected when you change order, and exports follow the selected order.

Copy shown and Copy selected put the same TSV vocabulary columns on the clipboard. All vocabulary views support this. Copy selected includes hidden ticked words, keeps the selection and follows your chosen order. The same 16 MiB and spreadsheet-safety rules apply.

Inspect history → Export full episode history writes every recorded word for that episode, beyond the 1,000 display limit, with its original first-encounter Unix milliseconds. The worker reads the selected episode at save time. The 16 MiB atomic TSV export leaves history and word statuses unchanged.

In an open series, episodes with a completed score offer Study words before playback. The browser uses the retained subtitle count and all shared vocabulary controls; status changes update it immediately. Reopening sources or changing dictionaries/settings closes stale lists.

While a graphic track is being read, the primary subtitle menu and subtitle list show preparation or processed/total pictures. Cancel reading stops that request and removes partial text: embedded tracks return to pictures, external pairs turn off. Playback continues. Cancelling reading does not cancel a separately requested model download.

The primary subtitle menu and list offer Export current text as SRT after the whole text track is ready, including completed OCR. It freezes plain text and the current delay when clicked. Negative starts are clipped to zero; fully negative or empty intervals are omitted. Styles/fonts are not exported, and the destination is written atomically with a 16 MiB limit.

Statistics shows the file count and byte usage of persisted analysis counts. Refresh cache usage reads a new snapshot; Clear count cache removes recognised book/episode count files and interrupted write temporaries. Current analyses, history, dictionaries and subtitle/OCR caches remain available. Work completed afterwards can refill the cache.

Books and local series offer Export analysed summaries for their retained completed analyses. The TSV captures current token/text-unit totals and percentages, plus series analysed/total/failed coverage. It includes all completed analyses of that view, independently of visible filters; undefined percentages and book coverage fields stay blank. Later status changes do not rewrite the saved snapshot.

Select **Support tools** in the setup assistant to install FFmpeg/ffprobe, yt-dlp (including its Deno dependency), or the local OCR model. Working local tools are reused. The assistant chooses the Linux/Windows x86_64 or ARM64 package, checks its SHA-256, and saves paths for immediate use. Downloads only begin after **Install**, use the configured proxy, and can be stopped. Tools live in Nikomiru’s private data directory; administrator access is not required. Playback/graphics libraries remain part of the application package or operating system.

## Subtitle study tools and vocabulary

In the subtitle list, tick individual lines or use Select shown. Hidden ticks
remain selected across search, time-window, reading-speed and word filters;
loading/replacing the primary timeline clears them. Study tools can copy timed
TSV transcripts, export selected SRT or shown/selected WebVTT, and export paired
primary/secondary transcripts or bilingual SRT. Both tracks must be complete for
bilingual output; pairing uses positive time overlap with each track’s delay and
retains untranslated primary lines. Save snapshots keep their captured text and
times. Incomplete OCR cannot export a complete subtitle file.

From/Until current time restrict the study window without seeking. Optional
reading-speed bounds use non-whitespace Unicode characters per second. Scope
statistics count lines/characters and merge overlapping dialogue time rather
than counting it twice. Previous/Next matching line seek within these filters
while preserving pause. Reset controls remove only their own bounds.

Vocabulary views combine exact/contains headword/reading search with status,
card and occurrence-count limits. Coverage distinguishes unique words from
occurrence tokens; selected totals include hidden ticks. The dictionary button
opens a text-only lookup without changing the study view. In the current
subtitle Words tab, Examples finds segmented headword/reading occurrences,
including inflections, and opens their original lines; clear the examples
filter to return to the full list. Replacing sources or analyses invalidates
old examples.

Export a frequency dictionary from shown/selected vocabulary for an importable
Yomitan ZIP with exact occurrence counts and separate readings. Import it
explicitly if desired; export never installs it. Reusable study filters can
remember/apply/forget one bounded filter preset across views for the current
application session. Presets retain no selections and do not survive restart.

## More subtitle and vocabulary practice (2.80.0 source)

Named study presets persist the shared vocabulary search, exact mode, script, Unicode headword length, status, card, occurrence and ordering filters. Open Named study presets, enter a name and save; click a saved name to apply or delete it explicitly. There are at most sixteen presets. Selections stay in each view. Session presets remain available separately. Wait for the save result before closing the app.

Subtitle Study tools add inclusive duration bounds in milliseconds. The subtitle list’s listening mode hides caption text until you reveal a line or the shown scope; source changes hide revealed text again. Copy/export actions still contain the original text. Shift-click a row checkbox to select/deselect a range in the currently shown order; invert shown selection preserves hidden ticks.

Vocabulary from these lines captures shown or selected cue vocabulary, including inflected dictionary headwords, once segmentation is complete. It updates word statuses and card marks without changing the subtitle score. Return to full subtitle vocabulary clears the capture; changing the source or dictionaries invalidates it. Vocabulary study batches select the next unticked words in the current order (twenty by default, from one to one thousand); clear/invert shown keeps hidden selections.

With a complete secondary track, HTML study sheets export a standalone bilingual document through the native save dialog. Open it locally and expand each translation when ready. Current filters and selected original cue indices determine the snapshot.

Sentence repetition plays the active current cue two, three or five times, then stays paused. It suspends automatic line pausing and gap speed while active. Use Stop repetition or manual playback, seek, timing or speed controls to cancel; changing the primary source also cancels it. Remaining passes are shown in the section.

## Extended study scopes and queues (2.90.0 source)

Caption text filters combine inclusive non-whitespace Unicode character bounds with single/multiline structure and First occurrence only. Empty lines do not count toward multiline structure. Duplicate matching collapses whitespace but preserves case and punctuation, keeping the first original cue after other filters. Hidden ticks remain selected. Shown exports, statistics and matching navigation share this scope.

Vocabulary reading filters distinguish words with a separate nonempty reading from words without one. An optional occurrence coverage target keeps the most frequent currently matching words required to reach 1–100 percent of tokens. Boundary frequency ties are included, so the result can exceed the requested percentage. The chosen display order applies afterwards. Session and persistent vocabulary presets include these options.

Saved subtitle study presets store search, time/speed/duration/text limits, structure, duplicate and context choices. Apply a name or explicitly delete it. Up to sixteen presets persist through restart, with asynchronous save results. Selections, hidden-text/reveal state, exact word identities and source identities stay outside the preset.

Vocabulary HTML study sheets save shown or selected words through the native dialog. Readings stay behind a Reveal reading control; occurrence counts, current statuses and card marks remain visible. These are standalone offline snapshots with escaped text and no external assets. Hidden selected words remain included in selected exports.

Word examples can include one or two original neighbouring cues on each side. Exact hits keep an outline and an accessible label, while other filters still apply to the expanded scope. Context never changes the captured word identity.

With the subtitle list open, Alt+Up/Down navigates matching starts, Alt+R reveals the current shown line in listening mode, and Alt+T toggles its selection. Configure these in Shortcuts. Typing focus, covering pages, closed lists and stale scopes prevent study actions. Navigation preserves pause.

Sentence repetition can capture shown or selected cues as a finite queue of up to one thousand unique original cues. Choose two, three or five passes per line; the queue follows source order and pauses after the last line. All intervals must be complete, valid and within the media duration before playback begins. Remaining passes and lines are shown. Filter changes do not rewrite a running snapshot; manual controls, primary timing/source changes and Stop repetition cancel the whole queue.

## Guided practice and portable filters (2.100.0 source)

Caption search can match the entire normalised caption. Text filters also offer source order, shortest/longest duration and shortest text. Highlighting keeps the original current cue; previous/next matching navigation still follows time. Range selections follow display order, while subtitle exports and practice queues keep source order. Exact mode and display order persist in named subtitle presets.

Enter study time bounds accepts nonnegative seconds, M:SS or H:MM:SS, with up to three decimal digits using a point or comma. Leave either field empty to remove that bound. Apply validates both fields together and rejects an end at/before the start. Draft edits do not affect the scope until applied. Study tools scroll internally so subtitle rows remain available; copy/export controls have their own expandable section.

The translation button beside a cue opens its complete overlapping secondary text in a read-only window, using each track’s delay. This hint leaves primary listening text hidden and does not change playback. Changing a source or delay closes the old preview. Interactive translation text is bounded to 64 KiB.

Practice recalling words from readings hides headwords in vocabulary lists, including accessible control names. Reveal one word or all shown; dictionary/example actions enable after reveal. Distinct readings supply clues. Source/count changes hide words again; status-only updates keep reveals. Explicit copying/exporting still saves the original words. This mode stays local to the view and is excluded from filter presets.

Undo word status restores one previous single/bulk change across study views. It checks the dictionary generation and all expected current values before restoring, and preserves selections. A snapshot holds at most ten thousand identities and eight MiB of identity text. A conflicting later status/dictionary change makes the old action unavailable; undo does not rewrite encounter history.

Sentence repetition offers a session-only pause of zero to ten thousand milliseconds between passes/cues. The interface stays responsive, shows the wait, and Stop repetition/manual/source controls cancel delayed resumes. A running wait retains its captured deadline when the setting changes. Skip current line advances the captured queue immediately, pausing when no cue remains. Restart current line restores that cue’s original finite count and retains the rest of the queue, including after completion.

In subtitle vocabulary, the HTML/export section can save examples of shown or selected words as TSV. Exact segmented expression/reading identities produce one row per matching original cue, with effective milliseconds and caption text. Reports require a complete segmentation cache and are bounded to one thousand words, one hundred thousand matches and sixteen MiB; stale/incomplete requests do not write a partial file.

Named preset panels can import/export both vocabulary and subtitle filter collections as JSON. The one-MiB versioned file contains only filters, excluding paths, services and secrets. Import validates the complete file and merges by name, replacing matching names while retaining others; each collection remains limited to sixteen entries. Invalid/unknown filters or excess capacity leave configuration untouched. Read-only configuration rejects import, and save results use the normal configuration writer.

## Finite vocabulary recall (2.110.0 source)

Open Reading and coverage in a vocabulary view to start recall from shown words or selected words, including hidden selections. A round captures up to one thousand unique expression/reading identities in the current display order. Source/filter changes do not rewrite it. Distinct readings are clues; otherwise a generic clue keeps the headword hidden. Reveal the answer, then choose Again or Good; Skip advances without a self-rating. Again returns a card to the queue for at most three attempts. These ratings belong to this round and do not change global known/learning statuses.

Space reveals, 1 rates Again and 2 rates Good while recall has priority. Configure these in Shortcuts → Recall. Typing fields and covering settings retain their controls; dictionary popups take priority. Back closes a popup before the recall round. Explicit dictionary lookup enables only after revealing the answer. Advancing, restarting or replacing a round clears old lookup results.

Capture sentence hints optionally uses the complete current primary subtitle analysis to copy up to three distinct short exact segmented examples per card. Reveal hints separately from the answer. Captured hints keep their source title and do not seek, change playback or follow later source changes. Original, reverse and shuffled restart choices preserve captured cards/hints and reset all progress under a new round identity.

Save recall progress writes a versioned JSON file. Open saved recall is available both in the round and in the Library, including before opening media. The complete bounded file is validated by replaying ratings before replacing a round; invalid files leave existing progress intact. Resumed answers and hints start concealed. The file contains the captured words, order, hints and ratings, excluding global statuses and media paths. Export recall results saves an independent TSV snapshot with one row per captured word, attempts, Again votes, final rating and completion state.

After completion, start a fresh round from words rated Again (including those later rated Good) or from skipped words. Empty categories are disabled. Record this summary explicitly adds aggregate counts and the source title to local recall history, once per round identity. History keeps the latest one hundred summaries across restarts and offers removal, clear and retry after save errors. It contains no captured word text and does not add to learning-time statistics. Active rounds resume only by opening their saved JSON file.

## TXT, Markdown and pasted text (M102 development)

Books → Add accepts UTF-8 `.txt`, `.md` and `.markdown` files, up to four MiB. The filename supplies the title. Plain text keeps angle brackets/entities literally, with each nonempty line a paragraph; Windows line endings and an optional UTF-8 BOM are handled. Markdown uses CommonMark structure for headings, paragraphs, lists, quotes, links and code. Links/images never open or download. Inline ruby is retained; other HTML is visible inert text. Rich Markdown styling, tables and images are not rendered.

Books → Paste text opens an explicit editor. Enter a title (up to 256 characters), paste the text, optionally choose Markdown, and press Read text. The source is kept locally and reopens from the Books list after restart. The draft remains available if saving fails. Identical title/format/content reuses the source; different content keeps its own reading position even with the same title and length. Moving a file without changing its name/content also retains its identity.

Lookup, word status marks, vocabulary/comprehension and card creation use the same reader as EPUB. Documents are limited to 8,192 nonempty paragraphs and 64 KiB per paragraph. Whole-book analysis keeps its existing stricter text budgets. Pasted sources are included in local backups; source bodies are not uploaded by WebDAV sync. Removing a book leaves its source file, as with other books.

## Line tools: speech, translation, explanation, shadowing and text from the picture (M121…M125 development)

Open **Line tools** from the subtitle menu (CC). The panel holds the line on screen; click a character to look it up. Each tool runs only when you ask, on that line:

- **Generate from the audio (Whisper)**, in the subtitle menu, reads the episode's speech on this computer and makes subtitles from it, for episodes without any. Install **whisper.cpp** and the **Speech model** (about 575 MB) in the setup assistant first. The menu shows the progress and can stop it. The result is kept with the episode's subtitles as `….whisper.ja.srt` and works like any subtitle file. Nothing is sent anywhere. Settings → Advanced → Speech chooses another program or model, the spoken language (`ja` by default, or `auto`) and the threads.
- **Translate** (Shift+T) sends the line to DeepL with your API key (Settings → Subtitle services → Translations and explanations) and shows it in the interface's language. DeepL API Free keys end in `:fx`.
- **Explain** (G) sends the line and the lines around it to Anthropic's Claude with your API key. It shows each part's reading, meaning and grammar, then a translation. Another Claude model can be named in the same settings.
- **Shadow** (Y) pauses and records you saying the line, for about one and a half times its length. With whisper.cpp installed, it shows what it heard and how close that is to the line; **Play my recording** plays it back, and R replays the original. The microphone is an ffmpeg input in Settings → Advanced (`pulse:default` on Linux by default).
- **Read from the picture** (O) pauses; drag over text in the video, such as burned-in subtitles or a sign, and the OCR model reads it into the panel. It is also added to the Texthooker's lines, so lookups and cards work as for hooked text. Esc gives up.

Only the line you ask about, and for an explanation its two neighbours, leaves the computer, and only to the service on the button.

## Updates (M119 development)

Settings → About → **Updates** looks for a new version only when you click **Check for updates**; Nikomiru never checks, downloads or installs by itself. Choose the **Stable** or **Beta** channel. A release is accepted only if its channel's list is signed with the key built into Nikomiru and is newer than both the running version and any version already seen, so an old list cannot be replayed. For a portable install (a folder you can write), **Download** fetches the release (resuming if interrupted, then checked by size and SHA-256) and **Install** unpacks it beside the current folder, checks that the new program runs as that version, and swaps the folders, keeping the old one as `.previous`; the new version starts next time. Flatpak and distribution packages are updated by their own tools and are never replaced. Builds without a signing key (for now, all of them: the signed feed is not published yet) say so and point to the release page.

## Palettes and video shaders (M118 development)

Settings → General → **Palette** offers Default, **OLED black** (pure black surfaces, always dark) and **High contrast** (black on white or white on black, with every main text at 7:1 or more). The accent colour still applies, made readable.

**Video shaders** can sharpen and upscale anime with [Anime4K](https://github.com/bloc97/Anime4K) (MIT license, by bloc97). Click **Download Anime4K shaders** once (0.8 MB, a fixed release checked before use), then choose **Anime4K fast** or **Anime4K quality** (for stronger GPUs); **Off** is the default rendering. Nothing is downloaded during playback. If your own mpv options set `glsl-shaders`, they are used and the presets stay off. If the video cannot use the shaders, the preset is turned off and the reason shown. **Remove Anime4K shaders** deletes them.

## Touch (M117 development)

On a touchscreen a tap does what a click does: it looks a word up, plays or pauses, and presses buttons. Swipe one finger quickly left or right over the video to seek back or forward by the seek step. In the reader's Pages and Sentence modes, swipe left for the next page or sentence and right for the previous one; in vertical writing and manga, which read from right to left, swipe right to go on. Vertical drags keep scrolling. The Windows, Android and macOS ports are tracked in `docs/qa/ports.md`; only Linux is released for now.

## Study languages (M115 development)

Settings → General → **Study language** chooses the language you study: Japanese (the default) or one of the other languages with deinflection rules (Arabic, German, Greek, English, Esperanto, Spanish, Basque, French, Irish, Ancient Greek, Georgian, Korean, Latin, Old Irish, Albanian, Tagalog, Yiddish). Each language has its own settings: **Words** splits text by the longest dictionary match (best for Japanese) or into **whole words between spaces** (best for languages written with spaces); **Readings** shows or hides readings over words; and its **dictionaries** are chosen automatically (those that declare the language, and those that declare none) or by hand. Saving applies everything at once.

Word statuses, known words read from Anki, phrases, cloze exercises and review cards are kept separately for each language: marking a word known in English changes nothing in Japanese, and the Review tab shows the cards made while that language was studied. Statuses and phrases from before this version belong to Japanese. Review statistics still count every language.

## Podcasts and lyric lessons (M112 development)

Books → **Podcasts** adds a podcast by its RSS feed address (HTTPS) with **Add or refresh**. Nothing is fetched or downloaded unless you ask: **Refresh** reads the feed again, **Download** saves one episode at a time for offline listening and **Play** opens it like an audiobook, with the subtitle list, word lookup, line replay and cards with the line's sound. If a download stops (Stop, a network failure or quitting), **Resume download** continues from where it stopped when the server allows it, otherwise it starts again. Episodes up to 2 GiB are accepted.

When the feed offers a transcript in SRT or WebVTT (`podcast:transcript`), it is downloaded with the episode and opens with it. Other transcript types (HTML, JSON) are reported as unsupported. **Use timed text file…** takes your own SRT, WebVTT, ASS or LRC file for an episode instead. For songs, keep an `.lrc` file beside the audio file (same name) and open the song: its lines can be looked up, replayed and mined the same way. Nikomiru never transcribes speech.

Feed and episode records are in local backups; downloaded files are not (download them again). **Remove with downloads** deletes a feed and its files; a refresh keeps downloaded episodes that the feed no longer lists. Web pages of feeds and episodes open in your browser only when you click them. Podcasts are not synced by WebDAV.

## Annotations, library search and reading modes (M111 development)

In an open text book, **Add bookmark**, **Highlight** and **Add note** start a draft; nothing is saved until **Save**. A highlight or note starts on the word you last looked up; choose **Word**, **Sentence** or **Paragraph** to cover more. A bookmark marks the paragraph at the top of the page and needs a name. Highlights and notes are drawn behind their text. **Annotations** lists every book's annotations (or only this book's) with a filter, explicit editing/deletion, **Open in the book** and a TSV export of the shown rows. If a book's text changes, its annotations are kept but marked as changed and are never shown at a shifted place. Annotations stay on this computer: they are in local backups but not merged by WebDAV. They never change word statuses, phrases or reading progress.

**Search books** finds literal text in every added book (manga excluded), ignoring case and Unicode composition. Up to 200 results (20 per book) show their book, chapter and context; opening one checks that the text is still there before moving the reader. The text is read once per session and read again when a file changes; changed or missing books are listed as not searched. Searching does not move any reading position.

For horizontal writing, choose **Scroll** (the default), **Pages** or **Sentence** in the reader bar. Pages shows whole paragraphs a page at a time; Sentence shows one larger sentence of its paragraph. Turn with the Previous/Next buttons or Page Up/Page Down. Lookup, cards, status underlines and read-along work in every mode; the choice is saved. Vertical writing and manga keep their own pages.

## Additional local documents (M110 development)

Books → Add also accepts `.htmlz`, `.rtf`, `.docx` and text-layer `.pdf` files. Each uses the usual word lookup, cards, phrases, sentence indexing, analysis and saved reading position. Original files stay where they are; their contents are not copied into backups or WebDAV. Moving a file with the same name/content retains its identity; replacing its content creates a different source. Failed imports keep the current book open.

- **HTMLZ** reads the root `index.html`/`index.htm`, or the only unambiguous HTML document. Paragraphs and ruby remain; scripts, styles, images and external resources are ignored.
- **RTF** reads Unicode and explicitly declared supported legacy code pages, including Japanese. Paragraphs, line breaks, tabs and field results remain. Formatting, pictures, objects and field instructions are ignored; fields are never evaluated.
- **DOCX** reads ordinary non-macro WordprocessingML body paragraphs, runs and tables in document order, including inserted text and basic headings. Deleted text, drawings and field instructions are ignored. It does not reproduce Word page layout or follow external relationships.
- **PDF** requires an installed Poppler `pdftotext`. Set its executable/folder in Settings → Advanced, or explicitly select Poppler in Setup to reuse a discovered installation. On supported native Linux distributions, Setup can install Poppler and its CMap data through the system package manager, requesting normal administrator authentication. This uses OS repositories/network settings; missing-package installation is disabled with a custom app proxy or in a sandbox. Stop prevents later setup jobs while the OS completes its current package transaction. Reading never installs anything. PDFs retain page order, including blank pages, and extract text only: scanned/image-only files, files requiring a password and invalid files produce an error. Complex visual reading order and PDF OCR are not provided.

Archives/PDFs are limited to 16 MiB; RTF and selected archive documents to 4 MiB. Imports allow at most 2,048 archive entries, 64 MiB of declared uncompressed archive content, 8,192 paragraphs, 4 MiB of clean text and 64 KiB per paragraph. PDF extraction additionally allows at most 512 pages and 30 seconds. Whole-book analysis retains its stricter limits. Unsafe archive paths, malformed structures and unsupported encodings are rejected. Native Windows/macOS/Android and sandboxed helper packaging checks remain in the platform work.

## Mokuro manga (M103 development)

Books → Add accepts maintained `.mokuro` 0.2.x JSON with local images in a same-stem folder beside it (or alongside the manifest if there is no volume folder). Select a text region on the page: its original text becomes clickable and its exact JPEG crop is available for cards, without OCR or model installation. Original page order and progress are retained. Missing images, unsafe paths and invalid coordinates reject the import before replacing a book. Modest OCR edge overruns are clipped. Executable legacy HTML viewers and remote image URLs are not used.

### Phrase notebook

Open **Phrase notebook** from Library or Books. **Capture current source** creates an editable draft from the reader selection (or current paragraph) or current primary caption. Enter an exact phrase contained in the original sentence, add its meaning and choose its own learning status, then click **Save phrase**. **New phrase** also works without a source. Drafts are not saved automatically; duplicates and failed writes keep the draft available for correction.

Search matches phrase, meaning, sentence and source title. Edit or delete a record explicitly; **Open source** returns to its original reader paragraph or paused video scene if still available. **Export shown phrases** freezes the displayed records as UTF-8 TSV and neutralises spreadsheet formulas. Phrase statuses do not change the word colours, comprehension counts or review schedule. Local phrases are retained in ordinary backups; this notebook is not merged through WebDAV yet.

### Cloze exercises

In the phrase notebook, choose **Preview cloze**, inspect the masked sentence and confirm **Create exercise**. The exercise retains its own snapshot, including meaning and source. Editing or deleting the notebook phrase later does not rewrite the exercise.

Open **Cloze practice**, filter the exercises and practice the first 100 shown using typed answers or choices. Typed answers compare Unicode NFC and ignore outer whitespace; case, punctuation and Japanese readings remain distinct. Choices need another distinct answer in the same study language. Answers and meanings appear after submission; advance, skip, undo the previous step or return to the exercise list explicitly. Other source pages are hidden during the exercise.

Finite practice never changes review schedules or phrase/word statuses. **Enrol in scheduled review** explicitly adds an exercise's single FSRS schedule. **Review due exercises** uses the existing review settings, with four explicit ratings after feedback and one-step schedule undo. Due exercises, frozen sources and schedules survive restart and ordinary backups. Deleting an exercise also deletes its schedule; it leaves the notebook phrase untouched.

### Sentence bank and exact examples

Open **Sentence bank** from the phrase notebook, or **Examples across sources** from a dictionary entry. Explicitly index the current book or the complete loaded primary text track. Indexing captures plain sentences and their original reader byte locations or video intervals, using the installed dictionaries to identify expressions and readings. A new index replaces that source's previous snapshot atomically. **Cancel indexing** cancels preparation; the final atomic commit has no cancel button.

Search an exact expression and reading, for example `食べる` / `たべる`: an inflected sentence such as `食べました。` can match without containing the dictionary expression literally. The browser shows at most 50 saved examples, their original matched spans and provenance. Results remain available offline. **Open source** checks the original reader context before moving to it; a changed or missing file leaves the saved example available. **Export shown examples** exports only that frozen result list as a formula-neutralised TSV. Deleting a source snapshot deletes its index, not the book, captions or learning states. Ordinary backups retain the bank; WebDAV merge is not added.

**Import owned sentences** accepts a local UTF-8 JSON pack, at most 8 MiB, in the current study language:

```json
{"version":1,"title":"My examples","entries":[{"text":"食べました。","language":"ja"}]}
```

Entries may additionally contain an existing library `source` reference with the same fields as exported phrase locations; paths, URLs and executable links are refused. Imported text is plain data; no pack metadata or provider is fetched. The bank keeps at most 256 source/pack batches, 10,000 sentences and 32 MiB of serialized data. Individual sentences are at most 16 KiB with 256 recognised words. Dictionaries must be installed before indexing.

### Previewed batch mining

Open **Batch mining** from the phrase notebook, capture a dictionary entry, add a saved sentence-bank example, or select up to 20 current-primary vocabulary entries. Captured contexts remain frozen; choose among at most five per word and explicitly enable audio or picture extraction. **Preview** checks the complete text mapping, deck, note type and tags without extracting media or adding notes. Inspect the fields before **Start**. Changed settings, dictionaries or media require a new preview. Live Anki mappings need an identity-based first field for duplicate checks.

Cards run one at a time. Stop finishes the current card and cancels the remaining items; continue never-sent items explicitly. Duplicate words are skipped. Retry a certain failure explicitly with its original operation identity. An unknown Anki outcome stops the batch: **Reconcile** performs a read-only exact-tag search and adopts a uniquely found note. A missing or ambiguous result stays unresolved and cannot be resent automatically. Restart restores the journal but never resumes exports. Resolved journals may be deleted explicitly; unresolved records are protected.

Ordinary database backups retain journals, not prepared pending-card media. Cards kept for later use the existing pending exporter, retaining their operation tags and uncertainty protection. Bounds are 20 cards, 8 MiB of captured content, 100 retained journals and 32 MiB of journal data; remove resolved history explicitly when full.

### Mining profiles and composer fields

In Settings → Anki, enter a profile name and choose **Save new profile**. A profile captures destination, deck, note type, tags and that note type's templates; it contains no connection URL, API key or media settings. Apply or delete profiles explicitly, then **Save** the settings to retain the changes. Applying checks existing Anki decks/models without creating missing ones. Up to 16 names are retained across restart and ordinary configuration backups. Existing templates remain available.

In **Advanced mining**, expand **Override and pin fields**. Enable a named field and enter literal text to replace its template for this card; HTML and braces remain literal text. Fields not enabled keep their ordinary templates/media. **Pin field** explicitly reuses that override for subsequent advanced cards during this run. Unpin stops future reuse while keeping this draft's override; disable the field to restore the template. Quick/batch mining never uses these pins. Anki/pending destinations support overrides; local review keeps its existing fixed schema. Each value is limited to 16 KiB, with 128 fields/256 KiB total; unknown fields and an empty first field fail before live uploads. Pending cards retain overrides through export/restart.

Expand **Choose dictionary senses** to include only selected dictionary definition records. All records start selected; deselect explicitly. A record may contain several related meanings. Empty selections, entries exceeding 128 records and changed source fingerprints are refused. Only selected records supply glossary text/images. Every new composer starts with fresh sense and media choices. Profile quick actions explicitly close the current draft and clear all pins. Changing the reader/video/lookup while editing cannot replace the captured source silently: reopen the composer if its source has changed.

### Guided pause lookup and held translation

Settings → Lookup → **Look up when paused** defaults to Off. Choose **First unknown word** to use the first active primary text cue and your installed dictionary identities/statuses. Known, learning and ignored words, punctuation and text without a dictionary entry are skipped. The lookup waits for existing segmentation and opens once per transition into pause. Closing this popup leaves playback paused. Resuming, changing the source/dictionaries, opening another popup or covering the player cancels a pending lookup. Disabled and partial OCR tracks do not start this rule. No word status changes.

**First kanji** is an independent rule, with no unknown-word fallback: it queries the original first kanji character. The separate **Look up first kanji** player shortcut is initially unbound and can be assigned in Shortcuts. Settings changes alone do not open another lookup.

Hold physical **T** (configurable as **Hold to reveal secondary translation**) to reveal the secondary text track already selected in the subtitle menu. Release it to restore the current obscuring rules. It does not pause, seek, change the primary track, translate or download anything. Repeats never latch the reveal; changing modifiers before release is safe. Focus loss, text input, covering pages, popups and source changes clear the hold. With multiple assigned keys, the first pressed key owns the hold until released. Saved obscuring preferences remain unchanged; nothing is held after restart.
