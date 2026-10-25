# SPDX-License-Identifier: GPL-3.0-only
# Nikomiru — Italian interface strings (SET-004).

app-title = Nikomiru
open-file = Apri file…
file-filter-video = File video
file-filter-all = Tutti i file
drop-hint = Trascina qui un video, oppure premi Ctrl+O per aprirne uno

play = Riproduci
pause = Pausa
fullscreen = Schermo intero
exit-fullscreen = Esci dallo schermo intero
volume = Volume
mute = Disattiva l'audio
unmute = Riattiva l'audio
speed = Velocità
speed-up = Più veloce
speed-down = Più lento
position = Posizione
audio-track = Traccia audio
no-audio = Nessun audio
track-unknown-language = Lingua sconosciuta
chapter-previous = Capitolo precedente
chapter-next = Capitolo successivo

toast-volume = Volume { $value }%
toast-speed = Velocità { $value }×
toast-screenshot = Screenshot salvato: { $name }
toast-screenshot-failed = Impossibile salvare lo screenshot ({ $reason }).
card-button = Carta Anki
card-overwrite = Sovrascrivi
card-open-in-anki = Apri in Anki
card-add-anyway = Aggiungi comunque
toast-card-added = Carta aggiunta a { $deck }.
toast-card-overwritten = Carta sovrascritta in { $deck }.
link-missing = Il video di questa carta non si trova:
link-locate = Individua il file…
anki-open = Impostazioni di Anki
anki-page-title = Anki
anki-close = Chiudi
anki-save = Salva
anki-connection = Connessione
anki-url = Indirizzo di AnkiConnect
anki-key = Chiave API
anki-show-key = Mostra
anki-test = Prova la connessione
anki-create-lapis = Crea mazzo Lapis
anki-note = Nota
anki-deck = Mazzo
anki-note-type = Tipo di nota
anki-fields = Campi
anki-fields-hint = Prova la connessione per vedere i campi del tipo di nota.
anki-insert = Inserisci un segnaposto
anki-lapis-defaults = Valori predefiniti di Lapis
anki-duplicates = Duplicati
anki-scope-deck = Nel mazzo
anki-scope-collection = In tutta la collezione
anki-tags = Etichette (separate da spazi)
anki-media = Media
anki-pad-head = Audio prima della battuta (ms)
anki-pad-tail = Audio dopo la battuta (ms)
anki-audio-format = Formato audio
anki-audio-quality = Qualità audio
anki-audio-q0 = Mono 64 kbit/s
anki-audio-q1 = Stereo 128 kbit/s
anki-audio-q2 = Stereo 192 kbit/s
anki-image-format = Formato immagine
anki-image-quality = Dimensione immagine
anki-image-q0 = 720 px
anki-image-q1 = 1000 px
anki-image-q2 = 2000 px
anki-image-q3 = Dimensione originale
anki-connected = Connesso: AnkiConnect { $version }, { $decks } mazzi, { $models } tipi di nota.
anki-lapis-created = Il tipo di nota e il mazzo Lapis ora sono in Anki.
anki-lapis-exists = Anki ha già il tipo di nota e il mazzo Lapis.
settings-saved = Impostazioni salvate.
anki-config-not-saved = config.toml non si è potuto leggere, quindi non è stato scritto: queste impostazioni valgono fino alla chiusura di Nikomiru. Correggi il file e salva di nuovo.
anki-deck-missing = Anki non ha il mazzo { $name }: scegline un altro o crealo.
anki-note-type-missing = Anki non ha il tipo di nota { $name }: scegline un altro o crea Lapis.
toast-muted = Audio disattivato
toast-unmuted = Audio attivato
toast-paused = In pausa
toast-playing = In riproduzione

error-file = Impossibile riprodurre “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = Il file delle impostazioni { $path } non è valido alla riga { $line } ({ $reason }). Finché non lo correggi si usano i valori predefiniti.
error-config-read = Il file delle impostazioni { $path } non si può leggere ({ $reason }). Si usano i valori predefiniti.
error-config-write = Le impostazioni non si possono salvare in { $path } ({ $reason }). Controlla che la cartella sia scrivibile.
error-config-serialize = Le impostazioni non si possono salvare ({ $reason }).
error-paths = HOME non è impostata: Nikomiru non trova le sue cartelle.

# Library (nikomiru-library LibraryError)
error-library = La libreria non si può usare ({ $reason }): posizioni e cronologia non vengono salvate.
error-library-record = Un record della libreria è danneggiato ({ $reason }).
error-library-io = Il file della libreria { $path } non si può usare ({ $reason }).
error-library-read-only = La libreria è stata scritta da una versione più recente di Nikomiru: è aperta in sola lettura.
error-library-schema = Il file della libreria è danneggiato: non ha una versione valida. Nikomiru lo lascia com'è.

# Player (nikomiru-player PlayerError, PlaybackError)
error-player = Il player non può farlo ({ $reason }).
error-player-thread = Il player non riesce ad avviarsi ({ $reason }).
error-player-path = Il player non può usare questo percorso: { $path }.
error-video-output = Il video non può essere mostrato: { $reason }
playback-loading-failed = il file manca, non è leggibile o è danneggiato
playback-audio-output = non c'è un'uscita audio disponibile
playback-video-output = l'uscita video non riesce ad avviarsi
playback-nothing-to-play = il file non ha né audio né video
playback-unknown-format = il formato del file non è riconosciuto
playback-unsupported = il file non è supportato
playback-other = errore di mpv { $code }

# FFmpeg (nikomiru-media ToolsError)
error-ffmpeg-missing = FFmpeg non è stato trovato. Installalo o indica il suo percorso in Impostazioni → Avanzate: fino ad allora l'estrazione dei sottotitoli e i media delle carte sono disattivati.
error-ffprobe-missing = FFmpeg è in { $path }, ma ffprobe non è accanto.
error-ffmpeg-run = { $path } non si può eseguire ({ $reason }).
error-ffmpeg-timeout = { $path } non ha risposto entro { $seconds } secondi.
error-ffmpeg-old = FFmpeg { $version } è troppo vecchio: serve la versione { $minimum } o successiva.

# External processes (nikomiru-media JobError)
error-job-spawn = { $program } non si può avviare ({ $reason }).
error-job-timeout = { $program } non ha finito entro { $seconds } secondi ed è stato fermato.
error-job-cancelled = { $program } è stato annullato.
error-job-failed = { $program } non è riuscito: { $reason }

# Dictionaries (nikomiru-dict ImportError, StoreError)
error-dict-io = { $path } non si può usare ({ $reason }).
error-dict-zip = Il file non è un archivio zip valido ({ $reason }).
error-dict-unsafe = Il dizionario contiene un file non sicuro ({ $entry }): non è stato importato.
error-dict-no-index = Questo non è un dizionario Yomitan: manca index.json.
error-dict-format = { $file } non è nel formato Yomitan ({ $reason }).
error-dict-too-many = Il dizionario ha più di 10 milioni di voci.
error-dict-glossary = La definizione di { $expression } supera i 10 MiB.
error-dict-empty = Il dizionario non contiene nulla che Nikomiru possa usare.
error-dict-storage-full = Il disco è pieno: libera spazio e importa di nuovo il dizionario.
error-dict-cancelled = Importazione annullata.
error-dict-store = Il database del dizionario non si può usare ({ $reason }).
error-dict-same-revision = { $title } revisione { $revision } è già installato.
error-dict-unknown = Il dizionario non esiste più.

# Anki (nikomiru-anki AnkiError, ANK-004)
error-anki-bad-url = { $url } non è un indirizzo di AnkiConnect: inizia con http:// o https://.
error-anki-refused = AnkiConnect non risponde su { $url }: Anki è aperto?
error-anki-timeout = Nessuna risposta da { $url } in tempo: probabilmente la porta è usata da un altro programma.
error-anki-http = AnkiConnect su { $url } non ha potuto rispondere ({ $reason }).
error-anki-protocol = Su { $url } non risponde AnkiConnect ({ $reason }).
error-anki-wrong-key = AnkiConnect ha rifiutato la chiave API: controllala nelle impostazioni di Anki.
error-anki-unsupported = Questo AnkiConnect è troppo vecchio per { $action }: aggiornalo in Anki.
error-anki-no-collection = Anki non ha nessun profilo aperto.
error-anki-busy = Anki è occupato: riprova tra un momento.
error-anki-editor-open = Chiudi le finestre Sfoglia, Aggiungi e Modifica di Anki, poi riprova.
error-anki-model-missing = Anki non ha il tipo di nota { $name }: aggiorna le impostazioni di Anki.
error-anki-deck-missing = Anki non ha il mazzo { $name }: aggiorna le impostazioni di Anki.
error-anki-duplicate = La carta esiste già in Anki.
error-anki-empty-note = Il primo campo della carta è vuoto.
error-anki-commit-unknown = Anki non ha confermato { $action }: controlla in Anki se la carta è stata creata prima di riprovare.
error-anki-lapis-missing = Anki non ha ancora il tipo di nota o il mazzo Lapis: controlla Anki e riprova.
error-anki = AnkiConnect: { $reason }
error-link-invalid = Questo non è un link di Nikomiru a una scena.
error-anki-unavailable = Non si possono creare carte: il worker di Anki non è partito.
error-mining-dictionaries-changed = I dizionari sono cambiati da quando si è aperto il popup: cerca di nuovo la parola.
error-mining-media = Non si possono creare i media della carta ({ $reason }).
error-mining-no-audio = Nessuna carta: non si è potuto tagliare l'audio della frase (al video serve una traccia audio).
error-mining-mapping = Nessun campo del tipo di nota ha un modello: controlla la corrispondenza dei campi nelle impostazioni di Anki.
error-mining-note-gone = L'ultima carta non è più in Anki.
error-mining-other-note-type = L'ultima carta è una nota { $name }, non del tipo scelto: non è stata sovrascritta.
error-mining-overwrite-lost = Anki non ha tenuto i nuovi campi: chiudi la carta nel browser di Anki e riprova.
error-mining-overwrite-unknown = Anki non ha confermato la sovrascrittura dell'ultima carta: controllala in Anki prima di riprovare.
error-mining-not-found = Anki non ha carte per questa parola.

# Sottotitoli online (nikomiru-online OnlineError, ADR 0008)
error-online-no-key = { $provider } non ha una chiave API: aggiungila nella pagina Sottotitoli online.
error-online-bad-url = { $url } non è un indirizzo che Nikomiru usa: solo HTTPS.
error-online-unreachable = { $host } non è raggiungibile ({ $reason }).
error-online-timeout = Nessuna risposta da { $host } in tempo.
error-online-unauthorized = { $provider } ha rifiutato la chiave API o l'accesso: controllali nella pagina Sottotitoli online.
error-online-rate-limited = Troppe richieste a { $provider }: riprova tra { $seconds } s.
error-online-quota = I download di oggi da { $provider } sono finiti.
error-online-http = { $provider } non ha potuto rispondere (HTTP { $status }).
error-online-protocol = { $provider } ha dato una risposta inattesa ({ $reason }).
error-online-too-large = Il file è più grande di { $limit } MiB.
error-online-archive = L'archivio non si può usare ({ $reason }).
error-online-unusable = { $name } non si può usare: solo file SRT, ASS, SSA, WebVTT e ZIP.
error-online-io = Non si possono salvare i sottotitoli ({ $reason }).

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } non si può usare ({ $reason }): potrebbero aprirsi più finestre di Nikomiru.
error-instance-no-answer = Nikomiru è già aperto ma non risponde. Chiudilo e riprova.

# Subtitles (M2)
subtitles = Sottotitoli
subtitles-off = Nessuno
subtitle-open-file = Apri file di sottotitoli…
subtitle-search-online = Cerca online…
subtitle-encoding = Codifica
encoding-auto = Automatica
subtitle-list = Elenco dei sottotitoli
subtitle-search = Cerca nei sottotitoli
subtitle-list-close = Chiudi l'elenco dei sottotitoli
file-filter-subtitles = File di sottotitoli
toast-sub-loaded = Sottotitoli: { $name }
toast-sub-loaded-encoding = Sottotitoli: { $name } ({ $encoding })
toast-sub-delay = Ritardo dei sottotitoli { $value } ms
toast-sub-blurred = Sottotitoli sfocati
toast-sub-hidden = Sottotitoli nascosti
toast-sub-visible = Sottotitoli visibili
subtitles-disable = Sottotitoli disattivati
toast-sub-off = Sottotitoli disattivati (V per rimostrarli)
toast-sub-on = Sottotitoli attivi
encoding-utf-8 = Unicode — UTF-8
encoding-utf-16le = Unicode — UTF-16 (little-endian)
encoding-utf-16be = Unicode — UTF-16 (big-endian)
encoding-cp932 = Giapponese — Shift_JIS (CP932)
encoding-euc-jp = Giapponese — EUC-JP
encoding-gb18030 = Cinese semplificato — GB18030
encoding-big5 = Cinese tradizionale — Big5
encoding-cp1252 = Europa occidentale — Windows-1252
encoding-iso-8859-1 = Europa occidentale — ISO-8859-1
error-subtitles = I sottotitoli non si possono mostrare: { $reason }
error-subtitle-read = { $path } non si può leggere ({ $reason })
error-subtitle-encoding = la codifica di { $path } non è riconosciuta: sceglila in Sottotitoli → Codifica
error-subtitle-empty = il file non ha righe di sottotitoli
error-subtitle-bitmap = i sottotitoli a immagini li disegna il player

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Riproduzione di un video

# Popup del dizionario
popup-loading = Ricerca…
popup-no-results = Nessun dizionario contiene questa parola. Importa i dizionari dal menu.
popup-pitch-accent = accento tonale
popup-replay = Riascolta
popup-goto = Vai alla frase
# The name of the popup's border for screen readers: dragging it resizes the popup.
popup-resize = Ridimensiona il popup
kanji-strokes = tratti: { $count }
kanji-radical = radicale: { $radical }

# Pagina dei dizionari
dict-page-title = Dizionari
dict-import = Importa…
dict-import-filter = Dizionari Yomitan
dict-empty = Nessun dizionario. Importa un dizionario Yomitan (.zip), per esempio JMdict o Jitendex, un dizionario di frequenze o di accento tonale, oppure trascinalo sulla finestra.
dict-importing = Importazione
dict-cancel = Annulla
dict-entries = voci
dict-update-offer = È installata un'altra revisione di questo dizionario. Aggiornarla? Le impostazioni restano.
dict-update = Aggiorna
dict-keep = Tieni quella installata
dict-revision = revisione
dict-enabled = Usato nelle ricerche
dict-move-up = Sposta su (priorità più alta)
dict-move-down = Sposta giù
dict-rename = Rinomina (vuoto ripristina il titolo)
dict-delete = Elimina
dict-delete-confirm = Eliminare questo dizionario? I suoi file vengono rimossi.
dict-collapse = Blocco nel popup
dict-collapse-automatic = Automatico
dict-collapse-expanded = Espanso
dict-collapse-collapsed = Compresso
dict-close = Chiudi
dict-type-terms = termini
dict-type-kanji = kanji
dict-type-frequency = frequenze
dict-type-pitch = accenti tonali
dict-size-mb = { $whole },{ $tenth } MB
dict-size-kb = { $size } kB
dict-imported = { $title } importato.
dict-same-revision = { $title } è già installato con questa revisione.
dict-import-busy = Attendi la fine dell'importazione in corso.
dict-css-saved = Lo stile di { $title } è salvato.
dict-group-terms = Termini
dict-group-kanji = Kanji
dict-group-frequency = Frequenze
dict-group-pitch = Accento tonale
dict-reference = { $name }, elencato in { $group }
dict-language = lingua { $code }
dict-drag = Trascina per cambiare l'ordine
dict-css = Il tuo stile per questo dizionario (CSS)
dict-css-intro = Il tuo CSS per le definizioni di questo dizionario, applicato dopo il suo styles.css. Si usano solo colori, dimensione, peso e stile del carattere, margini, spaziature, bordi e sfondi, sui selettori dei dizionari Yomitan (un tag, classi gloss-sc-…, attributi data-sc-…).
dict-css-field = CSS di { $title }
dict-css-save = Salva
dict-css-revert = Ripristina
dict-css-clear = Svuota
dict-css-unsaved = Non ancora salvato
dict-css-rules = Regole applicate: { $count }
dict-css-dropped = Proprietà escluse: { $list }
dict-css-ignored = Regole ignorate: { $list }
error-dict-unavailable = I dizionari non sono disponibili: la libreria non si è aperta.
error-file-dialog = La finestra di scelta dei file non è disponibile: trascina il file sulla finestra o indicalo dalla riga di comando.

# Pagina dei sottotitoli online (MET-009, ADR 0008)
online-page-title = Sottotitoli online
online-close = Chiudi
online-search-section = Ricerca
online-title = Titolo
online-season = Stagione
online-episode = Episodio
online-search = Cerca
online-use = Usa
online-episode-n = episodio { $n }
online-archive = archivio
online-unusable = non utilizzabile (RAR o 7z)
online-machine = tradotto automaticamente
online-settings = Impostazioni
online-auto = Cerca e carica i sottotitoli in automatico quando un video non li ha nella lingua preferita
online-jimaku-key = Chiave API di Jimaku
online-os-key = Chiave API di OpenSubtitles
online-os-user = Nome utente OpenSubtitles (facoltativo)
online-os-password = Password OpenSubtitles (facoltativa)
online-proxy = Proxy (facoltativo)
online-show-keys = Mostra le chiavi e la password
online-keys-hint = La chiave di Jimaku è nella pagina dell'account su jimaku.cc; quella di OpenSubtitles è una chiave consumer creata su opensubtitles.com (con un account, più download al giorno).
online-save = Salva
online-no-video = Prima apri un video.
online-downloading = Scarico { $name }…
online-nothing-found = Nessun sottotitolo trovato: prova un altro titolo, o senza episodio.
online-loaded = Sottotitoli da { $provider }: { $name }
online-auto-none = Nessun sottotitolo online trovato per questo video.
online-no-providers = Nessun fornitore ha una chiave API: aggiungine una nella pagina Sottotitoli online.
