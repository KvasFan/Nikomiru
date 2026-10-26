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

Start Nikomiru from the menu, or with `nikomiru path/to/episode.mkv`. Without a file it opens on the **Library**.

- **Open a video**: Ctrl+O, drag it onto the window, or open it from the library.
- **Subtitles**: Nikomiru loads the ones next to the video by itself, even when their name differs (`Subs/` folders, `Show 02.ja.srt`). It also loads the subtitle tracks inside the video. To choose, use the 字 menu in the bar.
- **Back**: Esc, or "← Library" in the bar.

## 3. Dictionaries

Nikomiru reads the same ZIP files as Yomitan: terms (JMdict, Jitendex…), kanji, frequencies and pitch accent. It ships with none.

1. Open the dictionaries: 📚 in the bar, or Settings → Dictionaries.
2. Choose **Import…**, or drag the ZIP onto the window.
3. Each type has its own list, ordered by priority; drag a dictionary by its handle to change its place.
4. **CSS** lets you restyle a dictionary's entries. Only a safe subset of CSS applies.

## 4. Looking words up

- **Click** a character of a subtitle: the popup shows the longest word that starts there, its readings, meanings, pitch accent and frequencies.
- **Shift + hover** looks words up without clicking. In Settings → Lookup you can make plain hover work too.
- **Keyboard**:
  - Enter looks up the subtitle shown;
  - ← → move the lookup by one character;
  - Tab and Shift+Tab move between the entries;
  - Esc closes the popup.
- **Select text** in a definition to look it up in a nested popup.
- **Replay** plays the line again; **Go to the sentence** jumps to its start.
- **♪** plays the word itself, if you set up a word audio source in Settings → Word audio.

## 5. Anki cards

1. Install the [AnkiConnect](https://ankiweb.net/shared/info/2055492159) add-on in Anki (code `2055492159`) and keep Anki open.
2. In Nikomiru, Settings → Anki: **Test the connection**, then **Create Lapis deck**. If you prefer, choose your own deck and note type and map their fields.
3. In a popup, **+** on an entry (or Ctrl+Enter) makes the card. It includes:
   - the word and its meanings;
   - the sentence with the word in bold;
   - the frame of the video and the audio of the line;
   - a link that reopens the scene.
4. **✓** means Anki already has the word: its menu opens the card in Anki, or adds another anyway.
5. **Shift+click on +** (or Ctrl+Shift+Enter) makes the card by hand. You can:
   - add the lines before and after;
   - move the clip and listen to it;
   - choose the picture, or leave the audio out.

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
- On a series' page you can rename it, merge it, move or remove episodes, or delete it. Your files are never touched.
- **Identify online** asks AniList (or MyAnimeList) for the series' titles, episodes and cover. If several series match, you choose. Jimaku then finds its subtitles by its AniList id.
- **Statistics** shows the time watched and the characters read:
  - by day, by series and by session;
  - a calendar of the year;
  - a CSV export.

## 8. Settings

⚙ in the bar, or Ctrl+, opens the settings. The tabs run down the left side.

| Tab | What it holds |
|---|---|
| General | Language, theme (light, dark or as the desktop), accent color, interface scale, seek step, next episode |
| Player | mpv's options as controls, `mpv.conf` presets, import and export |
| Subtitles | Size, preferred languages, blur or hide until hovered, the secondary subtitle |
| Lookup | Results in a popup, the popup's size, hover |
| Shortcuts | Every key, which you can change. Keys are physical: they stay put whatever the keyboard layout |
| Advanced | The ffmpeg program, where the settings file and the logs are |

Each tab has **Save** and **Reset**. `~/.config/nikomiru/config.toml` can also be edited by hand: Nikomiru keeps your comments.

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
| Ctrl+O | Open a file |
| Ctrl+, | Settings |
| Esc | Back |

## 10. When something goes wrong

- The logs are in `~/.local/state/nikomiru/logs/`. They never contain your keys.
- If the settings cannot be saved, check that `~/.config/nikomiru` belongs to you. If it was created with `sudo`, give it back with `sudo chown -R $USER: ~/.config/nikomiru`.
- If a setting in `config.toml` has a value of the wrong kind, Nikomiru uses the default for it and names it in a notice at startup.
- Problems and ideas: <https://github.com/KvasFan/nikomiru/issues>.
