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
audio-track = Audio track
no-audio = No audio
track-unknown-language = Unknown language
chapter-previous = Previous chapter
chapter-next = Next chapter

toast-volume = Volume { $value }%
toast-speed = Speed { $value }×
toast-screenshot = Screenshot saved: { $name }
toast-screenshot-failed = Cannot save the screenshot ({ $reason }).
card-button = Anki card
card-overwrite = Overwrite
card-open-in-anki = Open in Anki
card-add-anyway = Add anyway
toast-card-added = Card added to { $deck }.
toast-card-overwritten = Card overwritten in { $deck }.
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
anki-config-not-saved = config.toml could not be read, so it was not written: these settings last until Nikomiru closes. Fix the file and save again.
anki-deck-missing = Anki has no deck { $name }: choose another, or create it.
anki-note-type-missing = Anki has no note type { $name }: choose another, or create Lapis.
toast-muted = Muted
toast-unmuted = Sound on
toast-paused = Paused
toast-playing = Playing

error-file = Cannot play “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = The settings file { $path } is not valid at line { $line } ({ $reason }). The defaults are in use until you fix it.
error-config-read = The settings file { $path } cannot be read ({ $reason }). The defaults are in use.
error-config-write = The settings cannot be saved to { $path } ({ $reason }). Check that the folder is writable.
error-config-dir = The settings folder { $path } cannot be written ({ $reason }): the settings will not be saved. If it was made with sudo, give it back to yourself: sudo chown -R $USER: { $path }
error-config-serialize = The settings cannot be saved ({ $reason }).
error-config-values = Some settings in { $path } have a value of the wrong kind and use their default: { $keys }. Saving the settings writes the defaults over them.
error-paths = HOME is not set: Nikomiru cannot find its folders.

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
error-online-protocol = { $provider } gave an unexpected answer ({ $reason }).
error-online-too-large = The file is larger than { $limit } MiB.
error-online-archive = The archive cannot be used ({ $reason }).
error-online-unusable = { $name } cannot be used: only SRT, ASS, SSA, WebVTT and ZIP files.
error-online-io = The subtitles cannot be saved ({ $reason }).

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } cannot be used ({ $reason }): more Nikomiru windows may open.
error-instance-no-answer = Nikomiru is already running but does not answer. Close it and try again.

# Subtitles (M2)
subtitles = Subtitles
subtitles-off = Off
subtitle-open-file = Open subtitle file…
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
subtitle-list = Subtitle list
subtitle-search = Search the subtitles
subtitle-list-close = Close the subtitle list
file-filter-subtitles = Subtitle files
toast-sub-loaded = Subtitles: { $name }
toast-sub-loaded-encoding = Subtitles: { $name } ({ $encoding })
toast-sub-delay = Subtitle offset { $value } ms
toast-sub-blurred = Subtitles blurred
toast-sub-hidden = Subtitles hidden
toast-sub-visible = Subtitles visible
subtitles-disable = Subtitles off
toast-sub-off = Subtitles off (V to show them again)
toast-sub-on = Subtitles on
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
error-subtitle-encoding = the encoding of { $path } is not recognized: choose it in Subtitles → Encoding
error-subtitle-empty = the file has no subtitle lines
error-subtitle-bitmap = image subtitles are drawn by the player

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Playing a video

# Dictionary popup
popup-loading = Looking up…
popup-no-results = No dictionary has this word. Import dictionaries from the menu.
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
dict-import-filter = Yomitan dictionaries
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
