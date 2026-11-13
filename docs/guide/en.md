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

Besides Yomitan's ZIP files, the import takes StarDict (choose the `.ifo`), DSL (`.dsl`, `.dsl.dz`) and MDict (`.mdx`) dictionaries. Their definitions show as plain text, without pictures. An MDict locked to a user cannot be read.

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
4. **✓** means Anki already has the word: its menu opens the card in Anki, or adds another anyway.
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

- **Add folder…** imports your series. Episodes are grouped by their names (seasons, episode numbers), or by folder.
- **Continue watching** shows what you started.
- When an episode ends, the next one starts after a 5-second countdown.
- A series remembers its subtitle, offset and audio language.
- With word colours on, each episode shows `80% · 25%`: known word tokens and subtitle lines without a new word. Local embedded text subtitles count too, using the same saved track and language priorities as playback. `…` means counting; graphic subtitles, remote videos and subtitles turned off have no score. Reopen the series to refresh changed subtitle sources.
- On a series' page you can rename it, merge it, move or remove episodes, or delete it. Your files are never touched.
- **Identify online** asks AniList (or MyAnimeList) for the series' titles, episodes and cover. If several series match, you choose. Jimaku then finds its subtitles by its AniList id.
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

Add a CBZ or a folder of pictures on the Books page. "Two pages" shows them side by side, the first on the right. The left arrow, Page Down, Space and the wheel go forward, the right arrow and Page Up back.

To read a balloon, drag across it. The first time, the page offers a reading model of 111 MB: it comes from Hugging Face once, only if you ask, and then runs on your computer, so pages go nowhere. The text appears above the page: click a word to look it up, and a card has the text and the part of the page. The model reads one balloon or line at a time, and takes a few seconds.

### Audiobooks

"Open an audiobook…" on the Books page plays a sound file. Put its text beside it with the same name, as `.lrc`, `.srt`, `.vtt` or `.ass`: Nikomiru does not make it. The subtitle list opens to read along; click a word to look it up, and a card has the line and its sound.

With the audiobook playing, open its book on the Books page and press **Read along**: the page follows what you hear, the paragraph marked, and **Listen from here** plays from the paragraph at the top of the page.

### Jellyfin and Plex

Add a server in Settings → Media servers: for Jellyfin the address, user and password (the password is not kept); for Plex, sign in on plex.tv in your browser. Each server gets a tab on the library page. Its videos play from the original file, with the same subtitles, lookups and cards as a local one, and your position and watched state go back to the server. Nikomiru connects to a server only while its tab is open or one of its videos plays.

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
