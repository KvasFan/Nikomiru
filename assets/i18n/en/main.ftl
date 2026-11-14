# SPDX-License-Identifier: GPL-3.0-only
# Nikomiru — English interface strings (SET-004).

app-title = Nikomiru
open-file = Open file…
file-filter-video = Video files
file-filter-all = All files
drop-hint = Drop a video here, or press Ctrl+O to open one

play = Play
pause = Pause
fullscreen = Fullscreen
exit-fullscreen = Exit fullscreen
volume = Volume
mute = Mute
unmute = Unmute
speed = Speed
speed-up = Faster
speed-down = Slower
position = Position
time-switch = Click to show the time left or the time played
video-area = Video
audio-track = Audio track
track-unknown-language = Unknown language

toast-volume = Volume { $value }%
toast-speed = Speed { $value }×
toast-screenshot = Screenshot saved: { $name }
toast-screenshot-failed = Cannot save the screenshot ({ $reason }).
card-button = Anki card
word-status-unknown = new
word-status-learning = learning
word-status-known = known
word-status-ignored = ignore
card-overwrite = Overwrite
card-open-in-anki = Open in Anki
card-add-anyway = Add anyway
toast-card-added = Card added to { $deck }.
toast-card-overwritten = Card overwritten in { $deck }.
toast-card-kept = { $count ->
    [one] Card kept for later: Settings → Anki sends it.
   *[other] Card kept for later ({ $count } kept): Settings → Anki sends them.
}
toast-later-sent = { $sent ->
    [one] 1 card sent to Anki.
   *[other] { $sent } cards sent to Anki.
}
toast-later-some-failed = Sent to Anki: { $sent }. Still kept: { $failed } ({ $reason }).
link-missing = The video of this card cannot be found:
link-locate = Locate the file…
anki-open = Anki settings
anki-save = Save
anki-connection = Connection
anki-url = AnkiConnect address
anki-key = API key
anki-show-key = Show
anki-test = Test the connection
anki-create-lapis = Create Lapis deck
anki-note = Note
anki-deck = Deck
anki-note-type = Note type
anki-fields = Fields
anki-fields-hint = Test the connection to see the fields of the note type.
anki-insert = Insert a placeholder
anki-lapis-defaults = Lapis defaults
anki-duplicates = Duplicates
anki-scope-deck = In the deck
anki-scope-collection = In the whole collection
anki-tags = Tags (separated by spaces)
anki-media = Media
anki-pad-head = Audio before the line (ms)
anki-pad-tail = Audio after the line (ms)
anki-audio-format = Audio format
anki-audio-quality = Audio quality
anki-audio-q0 = Mono 64 kbit/s
anki-audio-q1 = Stereo 128 kbit/s
anki-audio-q2 = Stereo 192 kbit/s
anki-image-format = Image format
anki-image-quality = Image size
anki-image-q0 = 720 px
anki-image-q1 = 1000 px
anki-image-q2 = 2000 px
anki-image-q3 = Original size
anki-image-webp-animated = Animated WebP of the sentence
anki-image-avif-animated = Animated AVIF of the sentence (smaller, slower)
anki-image-webm = WebM video of the sentence, with sound
anki-image-a0 = 480 px, 10 frames/s
anki-image-a1 = 640 px, 12 frames/s
anki-image-a2 = 854 px, 15 frames/s
anki-image-a3 = 1280 px, 15 frames/s
anki-connected = Connected: AnkiConnect { $version }, { $decks } decks, { $models } note types.
anki-lapis-created = The Lapis note type and deck are now in Anki.
anki-lapis-exists = Anki already has the Lapis note type and deck.
settings-saved = Settings saved.
settings-title = Settings
settings-close = Close
settings-player = Player
settings-services = Subtitle services
settings-dictionaries = Dictionaries
settings-audio = Word audio
settings-anki = Anki
settings-general = General
settings-subtitles = Subtitles
settings-lookup = Lookup
settings-shortcuts = Shortcuts
settings-advanced = Advanced
settings-about = About
settings-save = Save
settings-reset = Reset
settings-reset-hint = Puts the settings of this tab back to their defaults; press Save to keep them.
settings-not-saved = config.toml could not be read, so it was not written: these settings last until Nikomiru closes. Fix the file and save again.
settings-interface = Interface
settings-language = Language
settings-language-system = As the system
settings-theme = Theme
settings-theme-system = As the desktop
settings-theme-dark = Dark
settings-theme-light = Light
settings-keys = Keys and passwords
settings-keyring = Keep them in the system keyring
settings-keyring-hint = API keys, passwords and server tokens go to the keyring of your desktop (GNOME Keyring, KWallet) instead of config.toml, when you save. They leave the file only once the keyring holds them. A locked keyring may ask for its password when Nikomiru starts.
settings-accent = Accent color
settings-scale = Interface scale
settings-playback = Playback
settings-seek = Seek step
settings-fit = Picture
settings-fit-contain = Whole, with bars
settings-fit-cover = Fill the window, edges cut
settings-fit-stretch = Stretch to the window
settings-auto-next = Play the next episode of a series after a countdown
settings-gap-speed = Speed between subtitle lines
settings-gap-speed-off = Off
settings-gap-speed-hint = Speed through gaps of at least three seconds. Dialogue and its edges keep your playback speed. Requires text subtitles.
settings-sub-size = Size
settings-sub-languages = Preferred languages
settings-sub-obscure = Main subtitle
settings-sub-secondary-obscure = Secondary subtitle
settings-sub-secondary-position = Secondary subtitle position
settings-position-top = At the top of the video
settings-position-bottom = Above the main subtitle
settings-sub-hint = The size is in points for a video 1080 pixels high: it grows and shrinks with the video, not with the interface scale. The languages are codes such as ja or jpn, in order of preference.
settings-sub-ass = Keep the styles of ASS subtitles (fonts, colors, positions)
settings-sub-read-graphic = Read image subtitles of Blu-rays (PGS) and DVDs (VobSub) as text
settings-sub-read-graphic-hint = When the reading model of the manga is installed, an image track chosen by itself is read on this computer, so its words can be clicked. The images show until the text is ready; the text is kept for the next time.
settings-sub-furigana = Readings over the kanji, from the dictionaries
settings-sub-mark-mined = Underline the words already made into cards
settings-sub-words-hint = The subtitles are split into words with the dictionaries you have, so a reading can be wrong where a word is cut wrong.
settings-sub-word-colors = Colour the words by what you know of them
settings-sub-word-colors-hint = Words not known yet are underlined in blue, words being learned in yellow, including in books and web pages; a subtitle line with a single new word is marked +1 in the list. In a dictionary popup, mark each word new, learning, known or ignored. A word you made a card of counts as learning.
settings-sub-furigana-unknown-only = Readings only over the words not known
settings-sub-known-lines = Lines with no new word
settings-sub-pause-new-only = Pause at the end of a line only when it has a new word
settings-sub-pitch-colors = Colour the pitch accent of the words
settings-sub-pitch-colors-hint = A line over each word, from your pitch dictionaries: blue heiban, red atamadaka, orange nakadaka, green odaka, purple kifuku.
settings-known-words = Known words from Anki and word lists
settings-known-decks = Decks whose cards are known words
settings-known-field = Field with the word
settings-known-hint = Decks separated by commas. A word whose card has an interval of 21 days or more is known, one whose card was studied is being learned; new and suspended cards are left out. They are read each time Nikomiru starts with the colours on, and kept for when Anki is closed. A status you set in a popup wins.
settings-known-read = Read them from Anki now
words-import = Import a word list…
words-export = Export the words…
words-filter = Word lists
words-list-hint = A list has one word per line, known with any reading, or "word,reading,status" lines, where the status is unknown, learning, known or ignored. The export writes the second kind.
toast-known-no-decks = Choose the decks whose cards are known words first, in Settings → Subtitles
toast-known-failed = The known words could not be read from Anki: { $reason }
toast-known-read = From Anki: { $known } words known, { $learning } being learned
toast-words-imported = { $count } words imported from the list
toast-words-import-failed = The word list could not be read: { $reason }
toast-words-exported = { $count } words written to { $path }
toast-words-export-failed = The words could not be written: { $reason }
settings-sub-reveal = Show blurred or hidden subtitles under the pointer
settings-sub-look = Look
settings-sub-font = Font
settings-sub-font-default = The interface's
settings-sub-furigana-size = Furigana size
settings-sub-text-color = Text color
settings-sub-bold = Bold
settings-sub-outline = Outline
settings-sub-shadow = Shadow
settings-sub-box = Box behind the text
settings-sub-position = Main subtitle
settings-sub-edge-bottom = At the bottom
settings-sub-edge-top = At the top
settings-sub-margin = Distance from the edge
settings-sub-look-hint = Distances and the outline are in pixels of a video 1080 pixels high. A transparent shadow or box is not drawn. Subtitles drawn with their ASS styles keep their own colors.
settings-sub-list = Subtitle list
settings-sub-list-text = Text size
settings-sub-list-width = Width
settings-lookup-results = Results in a popup
settings-popup-width = Largest popup width
settings-popup-height = Largest popup height
settings-lookup-hover = Look words up on hover, without Shift
settings-lookup-resume = Resume playback when the pointer leaves the popup
settings-ffmpeg = ffmpeg program
settings-ffmpeg-hint = empty: ffmpeg from PATH
settings-next-start = A change of the ffmpeg program takes effect from the next start of Nikomiru.
settings-folders = Files and folders
settings-config-file = Settings file
settings-logs = Logs
settings-copy = Copy
settings-mpv-hint = The mpv options are in the Player tab.
about-tagline = A video player for learning Japanese, with clickable subtitles, dictionaries and Anki cards.
about-license = License:
about-credits = Credits
about-derived = Nikomiru is a Rust rewrite of Fushi and derives from GPL-3.0 code: Fushi, Yomitan (deinflection tables), hoshidicts and the Lapis note type, whose templates ship unchanged. The logo and the icons are original works of the project, under the same license.
about-third-party = Third-party software
about-third-party-file = Licenses
about-third-party-missing = The licenses of the third-party software (THIRD_PARTY_LICENSES.html) come with the packages; this build does not have them. cargo xtask about makes the file.
shortcuts-hint = Keys are physical: a shortcut stays on the same key whatever the keyboard layout. Single keys do nothing while you type in a field.
shortcuts-global = Everywhere
shortcuts-player = Player
shortcuts-popup = Dictionary popup
shortcuts-add = Add a key to { $action }
shortcuts-remove = Remove { $key } from { $action }
shortcuts-default = Default keys for { $action }
shortcuts-press = Press a key… (Esc gives up)
shortcuts-taken = { $key } is already used by { $actions }.
shortcuts-conflict = These keys are used twice: { $keys }. Change them before saving.
action-play-pause = Play/pause
action-seek-back = Seek back
action-seek-forward = Seek forward
action-volume-up = Volume up
action-volume-down = Volume down
action-mute = Mute
action-speed-up = Faster
action-speed-down = Slower
action-speed-reset = Normal speed
action-hold-speed = 2× while held
action-frame-prev = Previous frame
action-frame-next = Next frame
action-screenshot = Save the frame
action-screenshot-subtitles = Save the frame with its subtitles
action-fullscreen = Fullscreen
action-chapter-prev = Previous chapter
action-chapter-next = Next chapter
action-back = Back
action-open-file = Open a file
action-sub-prev = Previous subtitle
action-sub-next = Next subtitle
action-replay-cue = Replay the subtitle
action-replay-prev-cue = Replay the previous subtitle
action-sub-list = Subtitle list
action-sub-search = Search the subtitles
action-sub-blur = Blur the subtitles
action-sub-hide = Hide the subtitles
action-sub-toggle = Subtitles on/off
action-sub-auto-pause = Pause at the end of each line
action-sub-delay-minus = Subtitles 100 ms earlier
action-sub-delay-plus = Subtitles 100 ms later
action-sub-align-prev = Align the previous subtitle here
action-sub-align-next = Align the next subtitle here
action-popup-next = Next entry
action-popup-prev = Previous entry
action-create-card = Create the card
mining-advanced-title = Card for { $word }
mining-sentence = Sentence
mining-no-sentence = No subtitle line: the card has no sentence.
mining-add-previous = Add the line before
mining-drop-previous = Remove the first line
mining-add-next = Add the line after
mining-drop-next = Remove the last line
mining-audio-section = Sentence audio
mining-audio = Put the sentence audio on the card
mining-start = Start
mining-end = End
mining-preview = ▶ Listen
mining-picture = Picture
mining-picture-click = The frame where you clicked
mining-picture-start = The frame at the start of the sentence
mining-picture-none = No picture
mining-create = Create the card
mining-cancel = Cancel
action-create-card-advanced = Create the card by hand
action-lookup-subtitle = Look up the subtitle shown
action-lookup-prev = Look up from the word before
action-lookup-next = Look up from the word after
action-settings = Settings
anki-config-not-saved = config.toml could not be read, so it was not written: these settings last until Nikomiru closes. Fix the file and save again.
anki-deck-missing = Anki has no deck { $name }: choose another, or create it.
anki-note-type-missing = Anki has no note type { $name }: choose another, or create Lapis.
toast-muted = Muted
toast-unmuted = Sound on

error-file = Cannot play “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = The settings file { $path } is not valid at line { $line } ({ $reason }). The defaults are in use until you fix it.
error-config-read = The settings file { $path } cannot be read ({ $reason }). The defaults are in use.
error-config-write = The settings cannot be saved to { $path } ({ $reason }). Check that the folder is writable.
error-config-dir = The settings folder { $path } cannot be written ({ $reason }): the settings will not be saved. If it was made with sudo, give it back to yourself: sudo chown -R $USER: { $path }
error-config-serialize = The settings cannot be saved ({ $reason }).
error-config-values = Some settings in { $path } have a value of the wrong kind and use their default: { $keys }. Saving the settings writes the defaults over them.
error-paths = HOME is not set: Nikomiru cannot find its folders.
error-keyring = The system keyring cannot be read ({ $reason }): keys and passwords are missing until it can. Nothing was deleted.
error-keyring-kept = The system keyring cannot be written ({ $reason }): keys and passwords stay in config.toml for now.

# Library (nikomiru-library LibraryError)
error-library = The library cannot be used ({ $reason }): positions and history are not saved.
error-library-record = A library record is damaged ({ $reason }).
error-library-io = The library file { $path } cannot be used ({ $reason }).
error-library-read-only = The library was written by a newer version of Nikomiru: it is open read-only.
error-library-schema = The library file is damaged: it has no valid version. Nikomiru leaves it as it is.
error-library-unavailable = The library cannot be used: see the log.

# Player (nikomiru-player PlayerError, PlaybackError)
error-player = The player cannot do this ({ $reason }).
error-player-thread = The player cannot start ({ $reason }).
error-player-path = The player cannot use this path: { $path }.
error-video-output = The video cannot be shown: { $reason }
playback-loading-failed = the file is missing, unreadable or damaged
playback-audio-output = no audio output is available
playback-video-output = the video output cannot start
playback-nothing-to-play = the file has no audio or video
playback-unknown-format = the file format is not recognized
playback-unsupported = the file is not supported
playback-other = mpv error { $code }

# FFmpeg (nikomiru-media ToolsError)
error-ffmpeg-missing = FFmpeg was not found. Install it, or set its path in Settings → Advanced: until then, subtitle extraction and card media are off.
error-ffprobe-missing = FFmpeg was found at { $path }, but ffprobe is not next to it.
error-ffmpeg-run = { $path } cannot be run ({ $reason }).
error-ffmpeg-timeout = { $path } did not answer within { $seconds } seconds.
error-ffmpeg-old = FFmpeg { $version } is too old: version { $minimum } or newer is needed.

# External processes (nikomiru-media JobError)
error-job-spawn = { $program } cannot be started ({ $reason }).
error-job-timeout = { $program } did not finish within { $seconds } seconds and was stopped.
error-job-cancelled = { $program } was cancelled.
error-job-failed = { $program } failed: { $reason }

# Dictionaries (nikomiru-dict ImportError, StoreError)
error-dict-io = { $path } cannot be used ({ $reason }).
error-dict-zip = The file is not a valid zip archive ({ $reason }).
error-dict-foreign = { $file } cannot be read ({ $reason }).
error-dict-unsafe = The dictionary contains an unsafe file ({ $entry }): it was not imported.
error-dict-no-index = This is not a Yomitan dictionary: index.json is missing.
error-dict-format = { $file } is not in the Yomitan format ({ $reason }).
error-dict-too-many = The dictionary has more than 10 million entries.
error-dict-glossary = The definition of { $expression } is larger than 10 MiB.
error-dict-empty = The dictionary contains nothing Nikomiru can use.
error-dict-storage-full = The disk is full: free some space and import the dictionary again.
error-dict-cancelled = Import cancelled.
error-dict-store = The dictionary database cannot be used ({ $reason }).
error-dict-same-revision = { $title } revision { $revision } is already installed.
error-dict-unknown = The dictionary no longer exists.

# Anki (nikomiru-anki AnkiError, ANK-004)
error-anki-bad-url = { $url } is not an AnkiConnect address: it starts with http:// or https://.
error-anki-refused = AnkiConnect is not responding on { $url }: is Anki open?
error-anki-timeout = No reply from { $url } in time: the port is probably used by another program.
error-anki-http = AnkiConnect on { $url } could not answer ({ $reason }).
error-anki-protocol = What answers on { $url } is not AnkiConnect ({ $reason }).
error-anki-wrong-key = AnkiConnect refused the API key: check it in the Anki settings.
error-anki-unsupported = This AnkiConnect is too old for { $action }: update it in Anki.
error-anki-no-collection = Anki has no profile open.
error-anki-busy = Anki is busy: try again in a moment.
error-anki-editor-open = Close Anki's Browse, Add and Edit windows, then try again.
error-anki-model-missing = Anki has no note type { $name }: refresh the Anki settings.
error-anki-deck-missing = Anki has no deck { $name }: refresh the Anki settings.
error-anki-duplicate = The card already exists in Anki.
error-anki-empty-note = The first field of the card is empty.
error-anki-commit-unknown = Anki did not confirm { $action }: check in Anki whether the card was made before trying again.
error-anki-lapis-missing = Anki still has no Lapis note type or deck: check Anki, then try again.
error-anki = AnkiConnect: { $reason }
error-link-invalid = This is not a Nikomiru link to a scene.
error-anki-unavailable = Cards cannot be made: the Anki worker did not start.
error-mining-dictionaries-changed = The dictionaries changed since the popup opened: look the word up again.
error-mining-media = The card's media cannot be made ({ $reason }).
error-later-keep = The card cannot be kept for later ({ $reason }).
error-later-damaged = The card kept for later cannot be read ({ $reason }).
error-mining-no-audio = No card: the sentence audio could not be cut (the video needs an audio track).
error-mining-mapping = No field of the note type has a template: check the field mapping in the Anki settings.
error-mining-note-gone = The last card is no longer in Anki.
error-mining-other-note-type = The last card is a { $name } note, not of the chosen note type: it was not overwritten.
error-mining-overwrite-lost = Anki did not keep the new fields: close the card in Anki's browser and try again.
error-mining-overwrite-unknown = Anki did not confirm the overwrite of the last card: check it in Anki before trying again.
error-mining-not-found = Anki has no card for this word.

# Online subtitles (nikomiru-online OnlineError, ADR 0008)
error-online-no-key = { $provider } has no API key: add it in Settings → Subtitle services.
error-online-bad-url = { $url } is not an address Nikomiru uses: only HTTPS.
error-online-unreachable = { $host } cannot be reached ({ $reason }).
error-online-timeout = No answer from { $host } in time.
error-online-unauthorized = { $provider } refused the API key or the login: check them in Settings → Subtitle services.
error-online-rate-limited = Too many requests to { $provider }: try again in { $seconds } s.
error-online-quota = The { $provider } downloads of today are used up.
error-online-http = { $provider } could not answer (HTTP { $status }).
error-online-gone = { $provider } no longer has this.
error-book-io = The book cannot be opened ({ $reason }).
error-book-format = This is not an EPUB book ({ $reason }).
error-book-too-large = This book is too large.
error-book-no-text = This book has no text: it may be made of pictures.
error-book-cbz = This is not a CBZ archive ({ $reason }).
error-book-no-pages = This archive has no pictures.
error-ocr-model = The reading model cannot run ({ $reason }).
error-ocr-picture = This page cannot be read ({ $reason }).
error-ocr-small = The selection is too small.
error-backup-io = The backup could not be written or read: { $reason }
error-backup-library = The library could not be read: { $reason }
error-backup-damaged = This is not a backup of Nikomiru, or it is damaged.
error-backup-newer = A newer version of Nikomiru made this backup. Update Nikomiru first.
error-backup-unsafe = This backup is not safe to restore.
error-online-protocol = { $provider } gave an unexpected answer ({ $reason }).
error-online-too-large = The file is larger than { $limit } MiB.
error-online-archive = The archive cannot be used ({ $reason }).
error-online-unusable = { $name } cannot be used: only SRT, ASS, SSA, WebVTT and ZIP files.
error-online-io = The subtitles cannot be saved ({ $reason }).
error-online-cancelled = Stopped.

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } cannot be used ({ $reason }): more Nikomiru windows may open.
error-instance-no-answer = Nikomiru is already running but does not answer. Close it and try again.

# Subtitles (M2)
subtitles = Subtitles
subtitles-off = Off
subtitle-open-file = Open subtitle file…
subtitle-read-as-text = read as text
subtitle-read-as-text-fetch = read as text (downloads a model of { $size } MB once)
subtitle-search-online = Search online…
subtitle-align = Align with the audio
subtitle-secondary = Secondary subtitle
subtitle-secondary-shown = Shown
subtitle-secondary-blur = Blurred until hovered
subtitle-secondary-hidden = Hidden until hovered
toast-sub-aligning = Aligning the subtitles with the audio…
toast-sub-aligned = Subtitles aligned with the audio: { $value } ms (match { $confidence } %)
toast-sub-align-unsure = Not aligned: the audio does not match the subtitles clearly (best { $value } ms, match { $confidence } %)
toast-sub-align-none = Nothing to align: no subtitles, or no sound
subtitle-encoding = Encoding
encoding-auto = Automatic
subtitle-list-short = List
subtitle-list = Subtitle list
subtitle-search = Search the subtitles
subtitle-list-close = Close the subtitle list
subtitle-score = Words known: { $known }% · lines with no new word: { $clear }%
subtitle-score-counting = Counting the words… { $done } of { $of } lines
subtitle-list-lines = Lines
subtitle-list-words = Words
subtitle-lines-all = All lines
subtitle-lines-new = With a new word
subtitle-lines-plus-one = One new word (i+1)
subtitle-words-new-only = Only words not known
subtitle-words-all = All shown
subtitle-words-ticked = { $count } ticked:
subtitle-words-card = A card was made of it
library-score-counting = Counting the words of the subtitles…
file-filter-subtitles = Subtitle files
toast-sub-loaded = Subtitles: { $name }
toast-sub-loaded-encoding = Subtitles: { $name } ({ $encoding })
toast-sub-reading = Reading the images of { $name } as text: the words can be clicked when it is done
toast-sub-read-fetching = Downloading the reading model ({ $size } MB, once) to read the images of { $name }
toast-sub-delay = Subtitle offset { $value } ms
toast-sub-blurred = Subtitles blurred
toast-sub-hidden = Subtitles hidden
toast-sub-visible = Subtitles visible
subtitles-disable = Subtitles off
subtitles-auto-pause = Pause at the end of each line
subtitles-move = Move the subtitles
subtitles-move-hint = Drag the subtitle up or down · Esc when done
toast-sub-off = Subtitles off (V to show them again)
toast-sub-on = Subtitles on
toast-auto-pause-on = Pause at the end of each line: on
toast-auto-pause-off = Pause at the end of each line: off
encoding-utf-8 = Unicode — UTF-8
encoding-utf-16le = Unicode — UTF-16 (little-endian)
encoding-utf-16be = Unicode — UTF-16 (big-endian)
encoding-cp932 = Japanese — Shift_JIS (CP932)
encoding-euc-jp = Japanese — EUC-JP
encoding-gb18030 = Chinese, simplified — GB18030
encoding-big5 = Chinese, traditional — Big5
encoding-cp1252 = Western European — Windows-1252
encoding-iso-8859-1 = Western European — ISO-8859-1
error-subtitles = The subtitles cannot be shown: { $reason }
error-subtitle-read = { $path } cannot be read ({ $reason })
error-subtitle-too-large = { $path } is too large for a subtitle file (over 64 MiB)
error-subtitle-encoding = the encoding of { $path } is not recognized: choose it in Subtitles → Encoding
error-subtitle-empty = the file has no subtitle lines
error-subtitle-bitmap = image subtitles are drawn by the player
error-subtitle-read-model = the reading model is not installed
error-subtitle-read-failed = the images could not be read ({ $reason })
error-subtitle-read-cancelled = the reading was stopped

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Playing a video

# Dictionary popup
popup-loading = Looking up…
popup-no-results = No dictionary has this word. Import dictionaries in Settings → Dictionaries (📚).
popup-pitch-accent = pitch accent
popup-replay = Replay
popup-goto = Go to the sentence
# The name of the popup's border for screen readers: dragging it resizes the popup.
popup-resize = Resize the popup
kanji-strokes = strokes: { $count }
kanji-radical = radical: { $radical }

# Dictionaries page
dict-page-title = Dictionaries
dict-import = Import…
dict-import-filter = Dictionaries (Yomitan, StarDict, DSL, MDict)
dict-empty = No dictionaries yet. Import a Yomitan dictionary (.zip), for example JMdict or Jitendex, a frequency dictionary or a pitch accent dictionary, or drop it on the window.
dict-importing = Importing
dict-cancel = Cancel
dict-entries = entries
dict-update-offer = Another revision of this dictionary is installed. Update it? Its settings are kept.
dict-update = Update
dict-keep = Keep the installed one
dict-revision = revision
dict-enabled = Used for lookups
dict-move-up = Move up (higher priority)
dict-move-down = Move down
dict-rename = Rename (empty restores the title)
dict-delete = Delete
dict-delete-confirm = Delete this dictionary? Its files are removed.
dict-collapse = Block in the popup
dict-collapse-automatic = Automatic
dict-collapse-expanded = Expanded
dict-collapse-collapsed = Collapsed
dict-type-terms = terms
dict-type-kanji = kanji
dict-type-frequency = frequencies
dict-type-pitch = pitch accents
dict-size-mb = { $whole }.{ $tenth } MB
dict-size-kb = { $size } kB
dict-imported = { $title } imported.
dict-same-revision = { $title } is already installed with this revision.
dict-import-busy = Wait for the import in progress to finish.
dict-css-saved = The style of { $title } is saved.
dict-group-terms = Terms
dict-group-kanji = Kanji
dict-group-frequency = Frequencies
dict-group-pitch = Pitch accent
dict-reference = { $name }, listed under { $group }
dict-language = language { $code }
dict-drag = Drag to change the order
dict-css = Your style for this dictionary (CSS)
dict-css-intro = Your CSS for this dictionary's definitions, applied after its own styles.css. Only colours, font size, weight and style, margins, padding, borders and backgrounds are used, on the selectors Yomitan dictionaries use (a tag, gloss-sc-… classes, data-sc-… attributes).
dict-css-field = CSS of { $title }
dict-css-save = Save
dict-css-revert = Revert
dict-css-clear = Clear
dict-css-unsaved = Not saved yet
dict-css-rules = Rules applied: { $count }
dict-css-dropped = Properties left out: { $list }
dict-css-ignored = Rules ignored: { $list }
error-dict-unavailable = The dictionaries are not available: the library could not be opened.
error-file-dialog = The file dialog is not available: drop the file on the window, or give it on the command line.

# Online subtitles page (MET-009, ADR 0008)
online-page-title = Online subtitles
online-close = Close
online-search-section = Search
online-title = Title
online-season = Season
online-episode = Episode
online-search = Search
online-use = Use
online-episode-n = episode { $n }
online-archive = archive
online-unusable = cannot be used (RAR or 7z)
online-machine = machine translated
online-services = Services and keys…
services-list = Services
services-list-hint = Only the services ticked are searched; between two files that fit as well, the service higher up wins.
services-up = Move { $name } up
services-down = Move { $name } down
services-jimaku-about = Japanese subtitles of anime and dramas, uploaded by the community; needs a free API key.
services-os-about = Subtitles of films and series in many languages, found also by the file's fingerprint; needs an API key, the login is optional.
services-auto-section = Automatic subtitles
services-network = Network
services-metadata = Series metadata
services-metadata-hint = Where "Identify online" in the library asks for the titles, the episodes and the cover of a series. Neither needs a key. Jimaku then finds the subtitles of an identified series by its AniList id.
online-auto = Search and load subtitles automatically when a video has none in the preferred language
online-jimaku-key = Jimaku API key
online-os-key = OpenSubtitles API key
online-os-user = OpenSubtitles user name (optional)
online-os-password = OpenSubtitles password (optional)
online-proxy = Proxy (optional)
online-show-keys = Show the keys and the password
online-keys-hint = The Jimaku key is in the account page of jimaku.cc; the OpenSubtitles key is a consumer key made on opensubtitles.com (with an account, more downloads a day).
online-save = Save
online-no-video = Open a video first.
online-downloading = Downloading { $name }…
online-nothing-found = No subtitles found: try another title, or no episode.
online-loaded = Subtitles from { $provider }: { $name }
online-auto-none = No online subtitles found for this video.
online-no-providers = No subtitle service is ready: choose one and add its API key in Settings → Subtitle services.

# Player settings page (PLY-016, PLY-018)
player-apply = Apply
player-apply-note = Apply saves the settings and sets the options in mpv now; the profile, the hardware decoder and a removed raw option take effect from the next file or the next start.
player-presets = Presets (mpv.conf)
player-preset-list = Saved presets
player-load = Load…
player-import = Import mpv.conf…
player-export = Export…
player-preset-name = Name
player-save-preset = Save as preset
player-filter-conf = mpv configuration
player-preview = Loading { $source }: nothing changes until you apply it.
player-line-control = a control
player-line-raw = a raw option
player-preview-apply = Apply
player-preview-cancel = Cancel
player-rendering = Rendering
player-audio = Audio
player-advanced = Advanced
player-reset = Reset
player-default = (default)
player-conf = mpv.conf in use
player-conf-hint = The options Nikomiru gives mpv, as in mpv.conf: edit the text or the controls below, then Apply.
player-user-conf = Your mpv.conf was found: Apply uses it in Nikomiru too, Cancel keeps mpv's defaults.
player-raw-ignored = Line { $line }: { $name } is not applied.
player-raw-invalid = Line { $line }: { $error }
player-ignored-fixed = Nikomiru sets it itself
player-ignored-blocked = not allowed: it runs code, loads other files or writes files
player-ignored-no-effect = no effect in Nikomiru
player-ignored-section = profiles in sections are not supported
player-ignored-fit = set by the picture setting in General
player-later = Saved. These take effect from the next file or the next start: { $names }
player-refused = mpv refused: { $names }
player-read-error = { $path } cannot be read ({ $reason }).
player-write-error = { $path } cannot be written ({ $reason }).
player-bad-name = A preset name is letters, digits, spaces, - _ and . only.
player-preset-saved = Preset { $name } saved.
player-exported = Exported to { $path }.

# Library page (LIB-002…LIB-012)
library-title = Library
library-close = Close
library-add-folder = Add folder…
library-continue = Continue watching
library-left = { $time } left
library-series = Series
first-steps-title = First steps
first-steps-hide = Hide
first-steps-dictionary = Import a Yomitan dictionary (JMdict, for example) to look words up.
first-steps-dictionary-button = Dictionaries…
first-steps-folder = Add the folder of your series and films.
first-steps-card = Make your first Anki card: Anki open with AnkiConnect, then + in a popup.
first-steps-card-button = Anki settings…
library-empty = No videos yet: add a folder with your series and films.
library-watched = { $watched } of { $total } watched
library-folders = Folders
library-grouping = Grouping
library-by-series = By series
library-by-folder = By folder
library-rescan = Scan again
library-back = ← Library
library-name = Name
library-rename = Rename
library-merge-into = Merge into
library-merge = Merge
library-delete = Delete from the library (the files stay)
library-episodes = Episodes ({ $count })
library-play = Play
library-move-to = Move to
library-move = Move
library-split-name = New series from the ticked episodes
library-split = Make it
library-remove = Remove the ticked episodes from the library (the files stay)
library-scanning = Looking for videos in { $path }…
library-imported = { $added } new videos, { $series } new series.
library-no-folder = { $path } is not a folder that can be read.
library-identify = Identify online
library-identify-hint = Asks AniList (or MyAnimeList, chosen in Settings → Subtitle services) for the titles, the episodes and the cover of this series. Nothing goes online otherwise.
library-identifying = Asking…
library-identify-choose = { $source } has no series with exactly this title: which one is it?
library-identify-pick = This one
library-identify-this = This one: { $title }
library-identify-none-of-these = None of these
library-identify-none = { $source } found nothing for this title: rename the series and try again.
library-identified = Identified as { $title }.
library-identify-again = Identify again
library-forget = Forget
library-online-page = Open on
library-online-episodes = { $count ->
    [one] { $count } episode
   *[other] { $count } episodes
}

# Next episode (LIB-009)
next-up = Next episode in { $seconds } s
next-up-play = Play now
next-up-cancel = Cancel

# Word audio (AUD-001…AUD-008)
word-audio-button = Word audio
word-audio-hint = Play the word; right click: every source
word-audio-loading = Asking the sources…
word-audio-none = No audio for this word.
word-audio-unavailable = Word audio cannot play: see the log.

# Word audio tab (AUD-001, AUD-005, AUD-006)
audio-save = Save
audio-sources = Sources, in the order they are asked
audio-sources-hint = A source is an address with {"{"}term{"}"} and {"{"}reading{"}"}, such as a local audio server for Yomitan, which answers with a Yomitan list of audio files or with the audio itself (only HTTPS, or HTTP on this computer); or the android.db of the Local Audio Server for Yomichan, read from disk. None is built in.
audio-source-n = Source { $n }
audio-name = Name
audio-url = Address
audio-up = Move source { $n } up
audio-down = Move source { $n } down
audio-remove = Remove source { $n }
audio-add = Add a source
audio-playing = Playing
audio-autoplay = Play the word by itself when a popup opens
audio-volume = Volume
audio-database = Choose the database of source { $n }
audio-database-button = Database…
audio-database-filter = Databases of audio
audio-priority = Sub-sources, in order
word-audio-database = The audio database { $path } cannot be used ({ $reason }).

# Statistics (STA-005)
stats-title = Statistics
stats-export = Export CSV…
stats-csv-filter = CSV files
stats-exported = The sessions are in { $path }.
stats-under-minute = < 1 min
stats-minutes = { $m } min
stats-hours = { $h } h { $m } min
stats-today = Today: { $time } · { $chars } characters
stats-week = 7 days: { $time } · { $chars } characters
stats-all = In all: { $time } · { $chars } characters
stats-chart = Time watched by day
stats-range = { $n } days
stats-day = { $date }: { $time } · { $chars } characters
stats-calendar = The year
stats-series = By series
stats-time-chars = { $time } · { $chars } characters
stats-sessions = Sessions
stats-none = No session yet: they are recorded while you watch.
stats-session = { $time } · { $chars } characters · { $lookups } { $lookups ->
    [one] lookup
   *[other] lookups
} · { $cards } { $cards ->
    [one] card
   *[other] cards
}
stats-delete = Delete
stats-delete-session = Delete the session of { $date }, { $name }

settings-servers = Media servers
servers-added = Servers added
servers-none = No server yet. Nikomiru connects to one only after you add it.
servers-add-jellyfin = Add a Jellyfin server
servers-name = Name
servers-address = Address
servers-address-hint = http://192.168.1.10:8096
servers-user = User
servers-password = Password
servers-unencrypted = Allow an unencrypted connection
servers-unencrypted-warning = Your password and token will travel in clear. Only needed for an http:// address outside your own network.
servers-log-in = Log in
servers-add-plex = Add Plex servers
servers-plex-hint = You sign in on plex.tv in your browser: no password goes through Nikomiru.
servers-sign-in-plex = Sign in with Plex
servers-cancel = Cancel
servers-plex-stopped = The Plex sign-in stopped before it was done.
servers-check = Libraries
servers-remove = Remove
servers-added-count = { $count ->
    [one] One server added.
   *[other] { $count } servers added.
}
servers-this-server = This server
servers-search = Search this library
servers-empty = Nothing here.
servers-more = More
servers-watched = Watched
servers-started = Started

settings-backup = Backup
backup-file = Backup to a file
backup-file-hint = Your settings, library, progress, statistics and downloaded subtitles in one file.
backup-dictionaries = Include the dictionaries (large)
backup-secrets = Include keys and passwords
backup-secrets-warning = The file will hold them in clear: keep it somewhere safe.
backup-make = Back up…
backup-restore = Restore…
backup-filter = Nikomiru backups
backup-restore-confirm = A backup of { $date }, made by Nikomiru { $version }. It replaces your settings, library and statistics at the next start; what is there now is kept in before-restore.
backup-restore-yes = Restore at the next start
backup-done = Backup written.
backup-restored = The backup is ready: close and reopen Nikomiru to restore it.
sync-title = Sync with WebDAV
sync-hint = Progress, statistics and cards stay the same on your computers, through a folder on a server of yours (Nextcloud and the like: use an app password). Settings, series and dictionaries stay on each computer. Files on the server are not encrypted.
sync-automatic = Sync at start and close
sync-now = Sync now
sync-saved = Saved.
sync-checked = The folder can be written.
sync-done = Synced with { $devices } other devices; { $skipped } skipped.

chapters = Chapters
chapter-n = Chapter { $n }
word-audio-setup = No pronunciation source yet: add one in the settings
hooker-title = Texthooker
hooker-connected = Connected
hooker-waiting = Waiting for the hooker…
hooker-clear = Clear
hooker-clipboard = Read the clipboard too
hooker-hint = Lines from Textractor's or LunaTranslator's WebSocket appear here: click a word to look it up, and make a card from the popup, with the sentence and no picture or audio. Ctrl+V adds what you copied. Reading the clipboard in the background needs wl-paste or xclip, and does not work on GNOME Wayland; only Japanese text is kept.
hooker-no-clipboard = No program to read the clipboard: install wl-paste or xclip.

books-title = Books
books-add = Add a book or manga…
books-folder = Add a folder of pages…
books-none = No book yet. Add an EPUB, a CBZ or an HTML page: its file stays where it is. Or open a web page by its address.
books-audiobook = Open an audiobook…
books-audio-filter = Audio files
books-filter = EPUB books, CBZ manga and HTML pages
books-remove = Remove { $title }
books-back = ← Books
books-vertical = Vertical
books-next = Next chapter
books-read = Read
books-score-analyse = Analyse
books-score-refresh = Refresh
books-score-analyse-title = Analyse comprehension of { $title }
books-score-counting = Analysing the book…
books-score-summary = Comprehension: { $known }% · { $clear }%
books-score-detail = { $known }% of word tokens known · { $clear }% of paragraphs with no new word, across the whole book
books-score-no-text = No analysable text
books-score-too-long = Too large for book analysis
books-score-failed = Analysis unavailable
books-score-wait = Another book is being analysed
books-score-source-changed = The book file has changed. Open it to refresh its library record, then analyse it again.
books-words = Words
books-words-search = Find a word or reading
books-chapter-score = Chapter: { $known }% known words · { $clear }% paragraphs with no new word
books-chapter-counting = Counting the chapter: { $done } of { $of } paragraphs
books-chapter-too-long = This chapter is too long to count. You can continue reading.
books-previous = Previous chapter
books-next-page = Next page
books-previous-page = Previous page
manga-double = Two pages
manga-page = Page { $number }
manga-bad-page = This page cannot be shown.
manga-offer = To read the text on a page, Nikomiru needs a reading model of { $size } MB, fetched once from Hugging Face. It runs on this computer: pages go nowhere.
manga-credit = Model: manga-ocr by kha-white (Apache-2.0), trained on the Manga109-s dataset.
manga-fetch = Download the model
manga-fetching = Downloading the model: { $percent } %
manga-hint = Drag across a balloon to read it.
manga-reading = Reading…
manga-nothing = No text was found there.

# Review (REV-001…REV-006)
review-title = Review
review-title-due = Review ({ $count })
review-deck = Nikomiru's review
review-in-library = This card is in Nikomiru's review: Library → Review.
error-review-no-library = Cards for the review need the library, which could not be opened.
review-count-new = New: { $count }
review-count-learning = Learning: { $count }
review-count-due = To review: { $count }
review-reload = Reload
review-undo = Undo
review-done = Nothing to review now.
review-next = The next card comes back in { $when }.
review-total = { $count ->
    [one] One card in the review.
   *[other] { $count } cards in the review.
}
review-empty-hint = To make cards here instead of in Anki, choose "Nikomiru's review" in Settings → Anki → Cards go to: the card button of a popup then adds them.
review-show = Show the answer
review-show-keys = Space or Enter; then 1–4 to answer
review-sentence-audio = ▶ Sentence
review-word-audio = ♪ Word
review-scene = Open the scene
review-delete = Delete
review-delete-confirm = Delete? Click again
review-again = Again
review-hard = Hard
review-good = Good
review-easy = Easy
review-minutes = { $n } min
review-hours = { $n } h
review-days = { $n } d
review-months = { $n } mo
review-years = { $n } y
anki-destination = Cards go to
anki-destination-anki = Anki
anki-destination-nikomiru = Nikomiru's review
anki-destination-later = Anki, later
anki-destination-later-hint = Cards are made at once, with their audio and picture, and kept on this computer: Anki need not be open. Send them below when it is.
anki-later = Cards for later ({ $count })
anki-later-none = No card is kept.
anki-later-remove = Remove
anki-later-damaged = A card that cannot be read: remove it.
anki-later-send = Send to Anki
anki-destination-hint = Nikomiru's review keeps the cards in the library and brings them back with FSRS on the Library page's Review tab: Anki is not needed.
review-settings = Review in Nikomiru
review-retention = Share of cards remembered when they come back
review-new-per-day = New cards a day
books-web = Web page
books-web-open = Open
error-book-not-page = This page cannot be read: its text is in an encoding Nikomiru does not know.
stats-words = Words
stats-words-set = Known: { $known } · learning: { $learning } · ignored: { $ignored }
stats-words-anki = From Anki: { $known } known, { $learning } learning
stats-words-days = Words known, the last 30 days
stats-words-day = { $ago ->
    [0] Today
    [1] Yesterday
   *[other] { $ago } days ago
}: { $count } words known
stats-review = Review
stats-review-today = Today: { $count } answers, { $kept } remembered
stats-review-retention = Remembered in review, last 30 days: { $percent } %
stats-review-retention-none = Remembered in review, last 30 days: no answer yet
stats-review-cards = { $new } new · { $learning } learning · { $young } young · { $mature } mature
stats-review-days = Answers a day, the last 30 days
stats-review-day = { $ago ->
    [0] Today
    [1] Yesterday
   *[other] { $ago } days ago
}: { $count } answers
stats-review-ahead = Cards due, the next 7 days
stats-review-due = { $in ->
    [0] Today
    [1] Tomorrow
   *[other] In { $in } days
}: { $count } cards
review-study = Study
review-cards = Cards ({ $count })
review-search = Search
review-no-match = No card holds that.
review-shown = The first { $shown } of { $count } cards: search to find the others.
review-phase-new = New
review-phase-learning = Learning
review-phase-review = In review
review-phase-relearning = Relearning
review-due-now = due now
review-answers = { $reps ->
    [one] 1 answer
   *[other] { $reps } answers
}, { $lapses } forgotten
review-reset = Start over
review-reset-confirm = Start over? Click again
books-along = Read along
books-along-hint = Play an audiobook with its timed text, then come back to its book: the page follows what you hear.
books-along-none = Nothing with timed text is playing: open the audiobook first.
books-along-not-found = No line of the audiobook was found in this book.
books-along-found = { $found } of { $count } lines of the audiobook found in the book.
books-listen = Listen from here

# Setup assistant (INS-001…INS-006)
setup-title = Setup assistant
setup-close = Close
setup-open = Set up dictionaries, audio and cards…
setup-open-hint = Downloads free dictionaries and word audio, and sets up how cards are made, as on the first start.
setup-first-steps = Dictionaries, word audio and cards in one place:
setup-intro = Choose what to install. Nothing is downloaded until you press Install; downloads go on while you use Nikomiru, and resume after a restart.
setup-dictionaries = Dictionaries
setup-dictionaries-hint = Downloaded from where their makers publish them, then imported as a file you chose would be.
setup-license-unstated = terms of its source, not stated
setup-audio = Word audio
setup-audio-hint = From Fushi's recommended pack: only the library ticked is downloaded, not the pack's dictionaries. The pack is made by Fushi's author, and the licenses of its recordings are not stated.
setup-audio-japanese = Japanese pronunciations
setup-audio-english = English pronunciations
setup-install = Install
setup-stop = Stop
setup-cards = Cards
setup-cards-anki = Anki
setup-cards-review = Nikomiru's review
setup-anki-answers = Anki answers (AnkiConnect { $version }).
setup-anki-lapis-in-use = Cards go to the Lapis note type.
setup-anki-use-lapis = Use the Lapis note type
setup-anki-away = Anki does not answer.
setup-anki-steps = Install Anki from apps.ankiweb.net, then in Anki: Tools → Add-ons → Get Add-ons…, code 2055492159 (AnkiConnect), and restart Anki. Keep Anki open while you make cards.
setup-anki-retry = Try again
setup-anki-later = Keep cards for later meanwhile
setup-state-installed = installed
setup-state-waiting = waiting
setup-state-downloading = { $percent } % of { $size }
setup-state-importing = importing
setup-state-done = done
setup-finished = The setup assistant has finished.
error-setup-space = Not enough free space: { $need } needed, { $free } free.
error-setup-no-library = The pack has no { $name }.
error-setup-not-audio = The file downloaded is not a word audio database ({ $reason }).

# AUD-010
condense-export = Export condensed audio
condense-folder = Folder for condensed audio
condense-hint = One audio file per episode, with a chapter for each line. Uses the subtitle and audio choices saved for the episode.
condense-running = Exporting condensed audio…
condense-cancel = Cancel audio export
condense-cancelled = Export cancelled. Files written: { $count }; episodes failed: { $failed }.
condense-done = Audio files written: { $count }; episodes failed: { $failed }.
condense-no-subtitles = No usable text subtitles. Choose a text track or read the graphic subtitles as text first.
condense-no-audio = This episode has no audio track to export.
condense-local-only = Series audio export requires a local video file.
condense-failed = Audio export failed: { $reason }

# PLY-023
webvideo-open = Open web video
webvideo-address = Video page address
webvideo-hint = Open a video page with yt-dlp installed on your computer. Its text subtitles can be looked up and made into cards.
webvideo-cancel = Cancel
webvideo-play = Open video
webvideo-opening = Opening web video…
webvideo-unavailable = The web video worker could not start. Local files can still be played.
webvideo-missing = yt-dlp was not found. Install it or set its path in Settings → Advanced.
webvideo-failed = The web video could not be opened. Check the address, its availability and whether yt-dlp is up to date.
webvideo-stream = No supported media stream was found. Live, DRM-protected and segmented-only videos are not supported.
webvideo-invalid = Enter a valid HTTPS video page address.
webvideo-cancelled = Opening the web video was cancelled.
settings-yt-dlp = yt-dlp executable
settings-yt-dlp-hint = Empty: NIKOMIRU_YT_DLP, then PATH

# KNW-017: explicit series comprehension
library-score-analyse = Analyse
library-score-refresh = Refresh
library-score-analyse-title = Analyse comprehension of { $title }
library-score-loading = Reading the episode list…
library-score-counting = Analysing episodes: { $done } / { $total }
library-score-summary = Comprehension: { $known }% · { $clear }%
library-score-detail = { $known }% of word tokens known · { $clear }% of lines with no new word, across the analysed episodes
library-score-coverage = { $scored } / { $total } episodes analysed · { $failed } failed
library-score-unavailable = No analysable subtitles
library-score-too-large = Too many episodes for series analysis

# KNW-018: historical playback encounters
stats-encounters = New words met by episode
stats-encounters-detail = Distinct words that were new when encountered during playback. Later status changes keep this history.
stats-encounters-episode = { $title }: { $count } new words met

condense-reading = Wait for subtitle reading to finish before exporting condensed audio.

error-subtitle-vobsub-index = The VobSub index is damaged or does not identify a readable language track.

analysis-cancel = Cancel analysis

analysis-words = Study words
analysis-words-back = Back to list

analysis-export-shown = Export shown
analysis-export-selected = Export selected
analysis-export-saved = Vocabulary saved to { $path }.
analysis-export-too-large = This vocabulary exceeds the 16 MiB export limit.

encounter-history-open = Inspect history
encounter-history-clear = Clear episode history
encounter-history-coverage = Showing { $shown } of { $total } historical words (first encounter: Unix ms).

analysis-order = Comprehension order
analysis-order-original = Library order
analysis-order-most = Most known first
analysis-order-least = Least known first
analysis-only-scored = Only scored
analysis-minimum-known = Minimum known %

analysis-source-changed = The source or dictionaries changed during analysis. Please refresh.

vocabulary-status-filter = Word status filter
vocabulary-all-statuses = All statuses
vocabulary-card-filter = Card filter
vocabulary-any-card = Any card
vocabulary-with-card = With card
vocabulary-without-card = Without card

vocabulary-order = Vocabulary order
vocabulary-frequency-desc = Most frequent first
vocabulary-frequency-asc = Least frequent first
vocabulary-expression = Expression order
vocabulary-reading = Reading order

vocabulary-copy-shown = Copy shown
vocabulary-copy-selected = Copy selected
vocabulary-copied = Vocabulary copied.
