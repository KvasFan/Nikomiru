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
time-switch = Clic per mostrare il tempo rimanente o quello trascorso
video-area = Video
audio-track = Traccia audio
track-unknown-language = Lingua sconosciuta

toast-volume = Volume { $value }%
toast-speed = Velocità { $value }×
toast-screenshot = Screenshot salvato: { $name }
toast-screenshot-failed = Impossibile salvare lo screenshot ({ $reason }).
card-button = Carta Anki
word-status-unknown = nuova
word-status-learning = in studio
word-status-known = nota
word-status-ignored = ignora
card-overwrite = Sovrascrivi
card-open-in-anki = Apri in Anki
card-add-anyway = Aggiungi comunque
toast-card-added = Carta aggiunta a { $deck }.
toast-card-overwritten = Carta sovrascritta in { $deck }.
toast-card-kept = { $count ->
    [one] Carta tenuta per dopo: la manda Impostazioni → Anki.
   *[other] Carta tenuta per dopo ({ $count } tenute): le manda Impostazioni → Anki.
}
toast-later-sent = { $sent ->
    [one] 1 carta mandata ad Anki.
   *[other] { $sent } carte mandate ad Anki.
}
toast-later-some-failed = Mandate ad Anki: { $sent }. Ancora tenute: { $failed } ({ $reason }).
link-missing = Il video di questa carta non si trova:
link-locate = Individua il file…
anki-open = Impostazioni di Anki
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
anki-image-webp-animated = WebP animato della frase
anki-image-avif-animated = AVIF animato della frase (più piccolo, più lento)
anki-image-webm = Video WebM della frase, con audio
anki-image-a0 = 480 px, 10 fotogrammi/s
anki-image-a1 = 640 px, 12 fotogrammi/s
anki-image-a2 = 854 px, 15 fotogrammi/s
anki-image-a3 = 1280 px, 15 fotogrammi/s
anki-connected = Connesso: AnkiConnect { $version }, { $decks } mazzi, { $models } tipi di nota.
anki-lapis-created = Il tipo di nota e il mazzo Lapis ora sono in Anki.
anki-lapis-exists = Anki ha già il tipo di nota e il mazzo Lapis.
settings-saved = Impostazioni salvate.
settings-title = Impostazioni
settings-close = Chiudi
settings-player = Player
settings-services = Servizi sottotitoli
settings-dictionaries = Dizionari
settings-audio = Audio delle parole
settings-anki = Anki
settings-general = Generale
settings-subtitles = Sottotitoli
settings-lookup = Ricerca
settings-shortcuts = Scorciatoie
settings-advanced = Avanzate
settings-about = Informazioni
settings-save = Salva
settings-reset = Ripristina
settings-reset-hint = Riporta ai valori predefiniti le impostazioni di questa scheda; premi Salva per conservarle.
settings-not-saved = config.toml non si è potuto leggere, quindi non è stato scritto: queste impostazioni valgono fino alla chiusura di Nikomiru. Correggi il file e salva di nuovo.
settings-interface = Interfaccia
settings-language = Lingua
settings-language-system = Come il sistema
settings-theme = Tema
settings-theme-system = Come il desktop
settings-theme-dark = Scuro
settings-theme-light = Chiaro
settings-keys = Chiavi e password
settings-keyring = Conservale nel portachiavi di sistema
settings-keyring-hint = Chiavi API, password e token dei server vanno nel portachiavi del desktop (GNOME Keyring, KWallet) invece che in config.toml, al salvataggio. Lasciano il file solo quando il portachiavi le ha davvero. Un portachiavi bloccato può chiedere la sua password all'avvio di Nikomiru.
settings-accent = Colore d'accento
settings-scale = Scala dell'interfaccia
settings-playback = Riproduzione
settings-seek = Passo dei salti
settings-fit = Immagine
settings-fit-contain = Intera, con le bande
settings-fit-cover = Riempi la finestra, bordi tagliati
settings-fit-stretch = Stira sulla finestra
settings-auto-next = Riproduci l'episodio successivo di una serie dopo un conto alla rovescia
settings-gap-speed = Velocità tra le battute
settings-gap-speed-off = Disattivata
settings-gap-speed-hint = Accelera le pause di almeno tre secondi. I dialoghi e i loro margini mantengono la velocità scelta. Richiede sottotitoli di testo.
settings-sub-size = Dimensione
settings-sub-languages = Lingue preferite
settings-sub-obscure = Sottotitolo principale
settings-sub-secondary-obscure = Sottotitolo secondario
settings-sub-secondary-position = Posizione del sottotitolo secondario
settings-position-top = In alto nel video
settings-position-bottom = Sopra il sottotitolo principale
settings-sub-hint = La dimensione è in punti per un video alto 1080 pixel: cresce e cala con il video, non con la scala dell'interfaccia. Le lingue sono codici come ja o jpn, in ordine di preferenza.
settings-sub-ass = Mantieni gli stili dei sottotitoli ASS (caratteri, colori, posizioni)
settings-sub-read-graphic = Leggi come testo i sottotitoli a immagini dei Blu-ray (PGS) e dei DVD (VobSub)
settings-sub-read-graphic-hint = Se il modello di lettura dei manga è installato, una traccia a immagini scelta da sola viene letta su questo computer, così le sue parole si possono cliccare. Le immagini restano finché il testo non è pronto; il testo si conserva per la volta dopo.
settings-sub-furigana = Letture sopra i kanji, dai dizionari
settings-sub-mark-mined = Sottolinea le parole già trasformate in carte
settings-sub-words-hint = I sottotitoli sono divisi in parole con i dizionari che hai: dove una parola è tagliata male anche la lettura può essere sbagliata.
settings-sub-word-colors = Colora le parole secondo quanto le conosci
settings-sub-word-colors-hint = Le parole non ancora note sono sottolineate in blu, quelle in studio in giallo, anche nei libri e nelle pagine web; una riga dei sottotitoli con una sola parola nuova è segnata +1 nella lista. Nel popup del dizionario segna ogni parola come nuova, in studio, nota o da ignorare. Una parola di cui hai fatto una carta conta come in studio.
settings-sub-furigana-unknown-only = Letture solo sopra le parole non note
settings-sub-known-lines = Righe senza parole nuove
settings-sub-pause-new-only = Pausa a fine riga solo se ha una parola nuova
settings-sub-pitch-colors = Colora il pitch accent delle parole
settings-sub-pitch-colors-hint = Una linea sopra ogni parola, dai dizionari di pitch installati: blu heiban, rosso atamadaka, arancione nakadaka, verde odaka, viola kifuku.
settings-known-words = Parole note da Anki e liste di parole
settings-known-decks = Mazzi le cui carte sono parole note
settings-known-field = Campo con la parola
settings-known-hint = Mazzi separati da virgole. Una parola la cui carta ha un intervallo di 21 giorni o più è nota, una la cui carta è stata studiata è in studio; le carte nuove e sospese non contano. Vengono lette a ogni avvio di Nikomiru con i colori attivi, e conservate per quando Anki è chiuso. Uno stato che scegli in un popup vince.
settings-known-read = Leggile ora da Anki
words-import = Importa una lista di parole…
words-export = Esporta le parole…
words-filter = Liste di parole
words-list-hint = Una lista ha una parola per riga, nota con qualunque lettura, oppure righe "parola,lettura,stato", dove lo stato è unknown, learning, known o ignored. L'esportazione scrive il secondo tipo.
toast-known-no-decks = Scegli prima i mazzi le cui carte sono parole note, in Impostazioni → Sottotitoli
toast-known-failed = Non è stato possibile leggere le parole note da Anki: { $reason }
toast-known-read = Da Anki: { $known } parole note, { $learning } in studio
toast-words-imported = { $count } parole importate dalla lista
toast-words-import-failed = Non è stato possibile leggere la lista di parole: { $reason }
toast-words-exported = { $count } parole scritte in { $path }
toast-words-export-failed = Non è stato possibile scrivere le parole: { $reason }
settings-sub-reveal = Mostra sotto il puntatore i sottotitoli sfocati o nascosti
settings-sub-look = Aspetto
settings-sub-font = Font
settings-sub-font-default = Quello dell'interfaccia
settings-sub-furigana-size = Dimensione della furigana
settings-sub-text-color = Colore del testo
settings-sub-bold = Grassetto
settings-sub-outline = Contorno
settings-sub-shadow = Ombra
settings-sub-box = Riquadro dietro il testo
settings-sub-position = Sottotitolo principale
settings-sub-edge-bottom = In basso
settings-sub-edge-top = In alto
settings-sub-margin = Distanza dal bordo
settings-sub-look-hint = Distanze e contorno sono in pixel di un video alto 1080 pixel. Un'ombra o un riquadro trasparenti non vengono disegnati. I sottotitoli disegnati con gli stili ASS mantengono i loro colori.
settings-sub-list = Lista dei sottotitoli
settings-sub-list-text = Dimensione del testo
settings-sub-list-width = Larghezza
settings-lookup-results = Risultati in un popup
settings-popup-width = Larghezza massima del popup
settings-popup-height = Altezza massima del popup
settings-lookup-hover = Cerca le parole al passaggio del puntatore, senza Shift
settings-lookup-resume = Riprendi la riproduzione quando il puntatore lascia il popup
settings-ffmpeg = Programma ffmpeg
settings-ffmpeg-hint = vuoto: ffmpeg dal PATH
settings-next-start = I cambiamenti ai percorsi degli strumenti valgono dalle operazioni successive, senza riavviare Nikomiru.
settings-folders = File e cartelle
settings-config-file = File delle impostazioni
settings-logs = Log
settings-copy = Copia
settings-mpv-hint = Le opzioni di mpv sono nella scheda Player.
about-tagline = Un lettore video per imparare il giapponese, con sottotitoli cliccabili, dizionari e carte Anki.
about-license = Licenza:
about-credits = Riconoscimenti
about-derived = Nikomiru è una riscrittura in Rust di Fushi e deriva da codice GPL-3.0: Fushi, Yomitan (tabelle di deflessione), hoshidicts e il tipo di nota Lapis, i cui modelli sono inclusi senza modifiche. Il logo e le icone sono opere originali del progetto, con la stessa licenza.
about-third-party = Software di terze parti
about-third-party-file = Licenze
about-third-party-missing = Le licenze del software di terze parti (THIRD_PARTY_LICENSES.html) sono incluse nei pacchetti; questa build non le ha. cargo xtask about crea il file.
shortcuts-hint = I tasti sono fisici: una scorciatoia resta sullo stesso tasto con qualunque disposizione della tastiera. I tasti singoli non fanno nulla mentre scrivi in un campo.
shortcuts-global = Ovunque
shortcuts-player = Player
shortcuts-popup = Popup del dizionario
shortcuts-add = Aggiungi un tasto a { $action }
shortcuts-remove = Togli { $key } da { $action }
shortcuts-default = Tasti predefiniti per { $action }
shortcuts-press = Premi un tasto… (Esc annulla)
shortcuts-taken = { $key } è già usato da { $actions }.
shortcuts-conflict = Questi tasti sono usati due volte: { $keys }. Cambiali prima di salvare.
action-play-pause = Riproduci/pausa
action-seek-back = Salta indietro
action-seek-forward = Salta avanti
action-volume-up = Alza il volume
action-volume-down = Abbassa il volume
action-mute = Muto
action-speed-up = Più veloce
action-speed-down = Più lento
action-speed-reset = Velocità normale
action-hold-speed = 2× tenendo premuto
action-frame-prev = Fotogramma precedente
action-frame-next = Fotogramma successivo
action-screenshot = Salva il fotogramma
action-screenshot-subtitles = Salva il fotogramma con i sottotitoli
action-fullscreen = Schermo intero
action-chapter-prev = Capitolo precedente
action-chapter-next = Capitolo successivo
action-back = Indietro
action-open-file = Apri un file
action-sub-prev = Sottotitolo precedente
action-sub-next = Sottotitolo successivo
action-replay-cue = Ripeti il sottotitolo
action-replay-prev-cue = Ripeti il sottotitolo precedente
action-sub-list = Elenco dei sottotitoli
action-sub-search = Cerca nei sottotitoli
action-sub-blur = Sfoca i sottotitoli
action-sub-hide = Nascondi i sottotitoli
action-sub-toggle = Sottotitoli sì/no
action-sub-auto-pause = Pausa alla fine di ogni battuta
action-sub-delay-minus = Sottotitoli 100 ms prima
action-sub-delay-plus = Sottotitoli 100 ms dopo
action-sub-align-prev = Allinea qui il sottotitolo precedente
action-sub-align-next = Allinea qui il sottotitolo successivo
action-popup-next = Voce successiva
action-popup-prev = Voce precedente
action-create-card = Crea la carta
mining-advanced-title = Carta per { $word }
mining-sentence = Frase
mining-no-sentence = Nessuna riga di sottotitoli: la carta non ha una frase.
mining-add-previous = Aggiungi la riga prima
mining-drop-previous = Togli la prima riga
mining-add-next = Aggiungi la riga dopo
mining-drop-next = Togli l'ultima riga
mining-audio-section = Audio della frase
mining-audio = Metti l'audio della frase nella carta
mining-start = Inizio
mining-end = Fine
mining-preview = ▶ Ascolta
mining-picture = Immagine
mining-picture-click = Il fotogramma dove hai cliccato
mining-picture-start = Il fotogramma all'inizio della frase
mining-picture-none = Nessuna immagine
mining-create = Crea la carta
mining-cancel = Annulla
action-create-card-advanced = Crea la carta a mano
action-lookup-subtitle = Cerca nel sottotitolo mostrato
action-lookup-prev = Cerca dalla parola prima
action-lookup-next = Cerca dalla parola dopo
action-settings = Impostazioni
anki-config-not-saved = config.toml non si è potuto leggere, quindi non è stato scritto: queste impostazioni valgono fino alla chiusura di Nikomiru. Correggi il file e salva di nuovo.
anki-deck-missing = Anki non ha il mazzo { $name }: scegline un altro o crealo.
anki-note-type-missing = Anki non ha il tipo di nota { $name }: scegline un altro o crea Lapis.
toast-muted = Audio disattivato
toast-unmuted = Audio attivato

error-file = Impossibile riprodurre “{ $file }”: { $reason }.

# Settings (nikomiru-core ConfigError, PathsError)
error-config = Il file delle impostazioni { $path } non è valido alla riga { $line } ({ $reason }). Finché non lo correggi si usano i valori predefiniti.
error-config-read = Il file delle impostazioni { $path } non si può leggere ({ $reason }). Si usano i valori predefiniti.
error-config-write = Le impostazioni non si possono salvare in { $path } ({ $reason }). Controlla che la cartella sia scrivibile.
error-config-dir = Nella cartella delle impostazioni { $path } non si può scrivere ({ $reason }): le impostazioni non verranno salvate. Se è stata creata con sudo, riprendila: sudo chown -R $USER: { $path }
error-config-serialize = Le impostazioni non si possono salvare ({ $reason }).
error-config-values = Alcune impostazioni in { $path } hanno un valore del tipo sbagliato e usano quello predefinito: { $keys }. Salvando le impostazioni vengono sostituite dai valori predefiniti.
error-paths = HOME non è impostata: Nikomiru non trova le sue cartelle.
error-keyring = Il portachiavi di sistema non si può leggere ({ $reason }): chiavi e password mancano finché non torna disponibile. Non è stato cancellato nulla.
error-keyring-kept = Il portachiavi di sistema non si può scrivere ({ $reason }): per ora chiavi e password restano in config.toml.

# Library (nikomiru-library LibraryError)
error-library = La libreria non si può usare ({ $reason }): posizioni e cronologia non vengono salvate.
error-library-record = Un record della libreria è danneggiato ({ $reason }).
error-library-io = Il file della libreria { $path } non si può usare ({ $reason }).
error-library-read-only = La libreria è stata scritta da una versione più recente di Nikomiru: è aperta in sola lettura.
error-library-schema = Il file della libreria è danneggiato: non ha una versione valida. Nikomiru lo lascia com'è.
error-library-unavailable = La libreria non si può usare: vedi il log.

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
error-dict-foreign = { $file } non si può leggere ({ $reason }).
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
error-later-keep = Non si può tenere la carta per dopo ({ $reason }).
error-later-damaged = Non si può leggere la carta tenuta per dopo ({ $reason }).
error-mining-no-audio = Nessuna carta: non si è potuto tagliare l'audio della frase (al video serve una traccia audio).
error-mining-mapping = Nessun campo del tipo di nota ha un modello: controlla la corrispondenza dei campi nelle impostazioni di Anki.
error-mining-note-gone = L'ultima carta non è più in Anki.
error-mining-other-note-type = L'ultima carta è una nota { $name }, non del tipo scelto: non è stata sovrascritta.
error-mining-overwrite-lost = Anki non ha tenuto i nuovi campi: chiudi la carta nel browser di Anki e riprova.
error-mining-overwrite-unknown = Anki non ha confermato la sovrascrittura dell'ultima carta: controllala in Anki prima di riprovare.
error-mining-not-found = Anki non ha carte per questa parola.

# Sottotitoli online (nikomiru-online OnlineError, ADR 0008)
error-online-no-key = { $provider } non ha una chiave API: aggiungila in Impostazioni → Servizi sottotitoli.
error-online-bad-url = { $url } non è un indirizzo che Nikomiru usa: solo HTTPS.
error-online-unreachable = { $host } non è raggiungibile ({ $reason }).
error-online-timeout = Nessuna risposta da { $host } in tempo.
error-online-unauthorized = { $provider } ha rifiutato la chiave API o l'accesso: controllali in Impostazioni → Servizi sottotitoli.
error-online-rate-limited = Troppe richieste a { $provider }: riprova tra { $seconds } s.
error-online-quota = I download di oggi da { $provider } sono finiti.
error-online-http = { $provider } non ha potuto rispondere (HTTP { $status }).
error-online-gone = { $provider } non ce l'ha più.
error-book-io = Non è stato possibile aprire il libro ({ $reason }).
error-book-format = Questo non è un libro EPUB ({ $reason }).
error-book-too-large = Questo libro è troppo grande.
error-book-no-text = Questo libro non ha testo: forse è fatto di immagini.
error-book-cbz = Questo non è un archivio CBZ ({ $reason }).
error-book-no-pages = Questo archivio non contiene immagini.
error-ocr-model = Il modello di lettura non funziona ({ $reason }).
error-ocr-picture = Questa pagina non si può leggere ({ $reason }).
error-ocr-small = La selezione è troppo piccola.
error-backup-io = Non è stato possibile scrivere o leggere il backup: { $reason }
error-backup-library = Non è stato possibile leggere la libreria: { $reason }
error-backup-damaged = Questo non è un backup di Nikomiru, oppure è danneggiato.
error-backup-newer = Questo backup viene da una versione più recente di Nikomiru. Aggiorna prima Nikomiru.
error-backup-unsafe = Questo backup non è sicuro da ripristinare.
error-online-protocol = { $provider } ha dato una risposta inattesa ({ $reason }).
error-online-too-large = Il file è più grande di { $limit } MiB.
error-online-archive = L'archivio non si può usare ({ $reason }).
error-online-unusable = { $name } non si può usare: solo file SRT, ASS, SSA, WebVTT e ZIP.
error-online-io = Non si possono salvare i sottotitoli ({ $reason }).
error-online-cancelled = Interrotto.

# Single instance (nikomiru-app InstanceError)
error-instance-io = { $path } non si può usare ({ $reason }): potrebbero aprirsi più finestre di Nikomiru.
error-instance-no-answer = Nikomiru è già aperto ma non risponde. Chiudilo e riprova.

# Subtitles (M2)
subtitles = Sottotitoli
subtitles-off = Nessuno
subtitle-open-file = Apri file di sottotitoli…
subtitle-read-as-text = leggi come testo
subtitle-read-as-text-fetch = leggi come testo (scarica una volta un modello di { $size } MB)
subtitle-search-online = Cerca online…
subtitle-align = Allinea con l'audio
subtitle-secondary = Sottotitolo secondario
subtitle-secondary-shown = Visibile
subtitle-secondary-blur = Sfocato finché non ci passi sopra
subtitle-secondary-hidden = Nascosto finché non ci passi sopra
toast-sub-aligning = Allineo i sottotitoli con l'audio…
toast-sub-aligned = Sottotitoli allineati con l'audio: { $value } ms (corrispondenza { $confidence } %)
toast-sub-align-unsure = Non allineati: l'audio non corrisponde chiaramente ai sottotitoli (migliore { $value } ms, corrispondenza { $confidence } %)
toast-sub-align-none = Niente da allineare: nessun sottotitolo, o nessun suono
subtitle-encoding = Codifica
encoding-auto = Automatica
subtitle-list-short = Lista
subtitle-list = Elenco dei sottotitoli
subtitle-search = Cerca nei sottotitoli
subtitle-list-close = Chiudi l'elenco dei sottotitoli
subtitle-score = Parole note: { $known }% · righe senza parole nuove: { $clear }%
subtitle-score-counting = Conto delle parole… { $done } righe su { $of }
subtitle-list-lines = Righe
subtitle-list-words = Parole
subtitle-lines-all = Tutte le righe
subtitle-lines-new = Con una parola nuova
subtitle-lines-plus-one = Una sola parola nuova (i+1)
subtitle-words-new-only = Solo parole non note
subtitle-words-all = Tutte quelle mostrate
subtitle-words-ticked = { $count } selezionate:
subtitle-words-card = Ne è stata fatta una carta
library-score-counting = Conto delle parole dei sottotitoli…
file-filter-subtitles = File di sottotitoli
toast-sub-loaded = Sottotitoli: { $name }
toast-sub-loaded-encoding = Sottotitoli: { $name } ({ $encoding })
toast-sub-reading = Lettura come testo delle immagini di { $name }: le parole si potranno cliccare alla fine
toast-sub-read-fetching = Scaricamento del modello di lettura ({ $size } MB, una volta sola) per leggere le immagini di { $name }
toast-sub-delay = Ritardo dei sottotitoli { $value } ms
toast-sub-blurred = Sottotitoli sfocati
toast-sub-hidden = Sottotitoli nascosti
toast-sub-visible = Sottotitoli visibili
subtitles-disable = Sottotitoli disattivati
subtitles-auto-pause = Pausa alla fine di ogni battuta
subtitles-move = Sposta i sottotitoli
subtitles-move-hint = Trascina il sottotitolo in su o in giù · Esc per finire
toast-sub-off = Sottotitoli disattivati (V per rimostrarli)
toast-sub-on = Sottotitoli attivi
toast-auto-pause-on = Pausa alla fine di ogni battuta: sì
toast-auto-pause-off = Pausa alla fine di ogni battuta: no
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
error-subtitle-too-large = { $path } è troppo grande per un file di sottotitoli (oltre 64 MiB)
error-subtitle-encoding = la codifica di { $path } non è riconosciuta: sceglila in Sottotitoli → Codifica
error-subtitle-empty = il file non ha righe di sottotitoli
error-subtitle-bitmap = i sottotitoli a immagini li disegna il player
error-subtitle-read-model = il modello di lettura non è installato
error-subtitle-read-failed = non è stato possibile leggere le immagini ({ $reason })
error-subtitle-read-cancelled = la lettura è stata interrotta

# Shown by the desktop while the screensaver is inhibited (PLY-013).
inhibit-reason = Riproduzione di un video

# Popup del dizionario
popup-loading = Ricerca…
popup-no-results = Nessun dizionario contiene questa parola. Importa i dizionari in Impostazioni → Dizionari (📚).
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
dict-import-filter = Dizionari (Yomitan, StarDict, DSL, MDict)
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
online-services = Servizi e chiavi…
services-list = Servizi
services-list-hint = Si cerca solo con i servizi selezionati; tra due file ugualmente adatti vince il servizio più in alto.
services-up = Sposta { $name } su
services-down = Sposta { $name } giù
services-jimaku-about = Sottotitoli giapponesi di anime e drama, caricati dalla comunità; serve una chiave API gratuita.
services-os-about = Sottotitoli di film e serie in molte lingue, trovati anche dall'impronta del file; serve una chiave API, l'accesso è facoltativo.
services-auto-section = Sottotitoli automatici
services-network = Rete
services-metadata = Metadati delle serie
services-metadata-hint = Dove "Identifica online" nella libreria chiede i titoli, gli episodi e la copertina di una serie. Nessuno dei due richiede una chiave. Jimaku poi trova i sottotitoli di una serie identificata dal suo id AniList.
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
online-no-providers = Nessun servizio di sottotitoli è pronto: scegline uno e aggiungi la sua chiave API in Impostazioni → Servizi sottotitoli.

# Pagina delle impostazioni del player (PLY-016, PLY-018)
player-apply = Applica
player-apply-note = Applica salva le impostazioni e imposta subito le opzioni in mpv; il profilo, il decoder hardware e un'opzione grezza tolta valgono dal file successivo o dal prossimo avvio.
player-presets = Preset (mpv.conf)
player-preset-list = Preset salvati
player-load = Carica…
player-import = Importa mpv.conf…
player-export = Esporta…
player-preset-name = Nome
player-save-preset = Salva come preset
player-filter-conf = Configurazione di mpv
player-preview = Caricamento di { $source }: non cambia nulla finché non lo applichi.
player-line-control = un controllo
player-line-raw = un'opzione grezza
player-preview-apply = Applica
player-preview-cancel = Annulla
player-rendering = Resa
player-audio = Audio
player-advanced = Avanzate
player-reset = Ripristina
player-default = (predefinito)
player-conf = mpv.conf in uso
player-conf-hint = Le opzioni che Nikomiru dà a mpv, come in mpv.conf: modifica il testo o i controlli qui sotto, poi Applica.
player-user-conf = È stato trovato il tuo mpv.conf: Applica lo usa anche in Nikomiru, Annulla tiene i valori predefiniti di mpv.
player-raw-ignored = Riga { $line }: { $name } non viene applicata.
player-raw-invalid = Riga { $line }: { $error }
player-ignored-fixed = la imposta Nikomiru
player-ignored-blocked = non consentita: esegue codice, carica altri file o scrive file
player-ignored-no-effect = nessun effetto in Nikomiru
player-ignored-section = i profili in sezioni non sono supportati
player-ignored-fit = lo decide l'impostazione Immagine in Generale
player-later = Salvato. Queste valgono dal file successivo o dal prossimo avvio: { $names }
player-refused = mpv ha rifiutato: { $names }
player-read-error = Non si può leggere { $path } ({ $reason }).
player-write-error = Non si può scrivere { $path } ({ $reason }).
player-bad-name = Il nome di un preset ha solo lettere, cifre, spazi, - _ e .
player-preset-saved = Preset { $name } salvato.
player-exported = Esportato in { $path }.

# Library page (LIB-002…LIB-012)
library-title = Libreria
library-close = Chiudi
library-add-folder = Aggiungi cartella…
library-continue = Continua a guardare
library-left = mancano { $time }
library-series = Serie
first-steps-title = Primi passi
first-steps-hide = Nascondi
first-steps-dictionary = Importa un dizionario Yomitan (per esempio JMdict) per cercare le parole.
first-steps-dictionary-button = Dizionari…
first-steps-folder = Aggiungi la cartella delle tue serie e dei tuoi film.
first-steps-card = Crea la tua prima carta Anki: Anki aperto con AnkiConnect, poi + in un popup.
first-steps-card-button = Impostazioni Anki…
library-empty = Ancora nessun video: aggiungi una cartella con le tue serie e i tuoi film.
library-watched = { $watched } di { $total } visti
library-folders = Cartelle
library-grouping = Raggruppamento
library-by-series = Per serie
library-by-folder = Per cartella
library-rescan = Analizza di nuovo
library-back = ← Libreria
library-name = Nome
library-rename = Rinomina
library-merge-into = Unisci a
library-merge = Unisci
library-delete = Togli dalla libreria (i file restano)
library-episodes = Episodi ({ $count })
library-play = Riproduci
library-move-to = Sposta in
library-move = Sposta
library-split-name = Nuova serie dagli episodi spuntati
library-split = Crea
library-remove = Togli gli episodi spuntati dalla libreria (i file restano)
library-scanning = Cerco video in { $path }…
library-imported = { $added } video nuovi, { $series } serie nuove.
library-no-folder = { $path } non è una cartella leggibile.
library-identify = Identifica online
library-identify-hint = Chiede ad AniList (o a MyAnimeList, scelto in Impostazioni → Servizi sottotitoli) i titoli, gli episodi e la copertina di questa serie. Altrimenti nulla va online.
library-identifying = Chiedo…
library-identify-choose = { $source } non ha una serie con esattamente questo titolo: quale è?
library-identify-pick = Questa
library-identify-this = Questa: { $title }
library-identify-none-of-these = Nessuna di queste
library-identify-none = { $source } non ha trovato nulla per questo titolo: rinomina la serie e riprova.
library-identified = Identificata come { $title }.
library-identify-again = Identifica di nuovo
library-forget = Dimentica
library-online-page = Apri su
library-online-episodes = { $count ->
    [one] { $count } episodio
   *[other] { $count } episodi
}

# Next episode (LIB-009)
next-up = Prossimo episodio fra { $seconds } s
next-up-play = Riproduci ora
next-up-cancel = Annulla

# Word audio (AUD-001…AUD-008)
word-audio-button = Audio della parola
word-audio-hint = Riproduci la parola; clic destro: tutte le fonti
word-audio-loading = Chiedo alle fonti…
word-audio-none = Nessun audio per questa parola.
word-audio-unavailable = L'audio delle parole non si può riprodurre: vedi il log.

# Word audio tab (AUD-001, AUD-005, AUD-006)
audio-save = Salva
audio-sources = Fonti, nell'ordine in cui vengono chieste
audio-sources-hint = Una fonte è un indirizzo con {"{"}term{"}"} e {"{"}reading{"}"}, come un server audio locale per Yomitan, che risponde con una lista Yomitan di file audio o con l'audio stesso (solo HTTPS, o HTTP su questo computer); oppure l'android.db del Local Audio Server for Yomichan, letto dal disco. Nessuna è predefinita.
audio-source-n = Fonte { $n }
audio-name = Nome
audio-url = Indirizzo
audio-up = Sposta su la fonte { $n }
audio-down = Sposta giù la fonte { $n }
audio-remove = Togli la fonte { $n }
audio-add = Aggiungi una fonte
audio-playing = Riproduzione
audio-autoplay = Riproduci la parola da sola quando si apre un popup
audio-volume = Volume
audio-database = Scegli il database della fonte { $n }
audio-database-button = Database…
audio-database-filter = Database di audio
audio-priority = Sotto-fonti, in ordine
word-audio-database = Il database audio { $path } non si può usare ({ $reason }).

# Statistics (STA-005)
stats-title = Statistiche
stats-export = Esporta CSV…
stats-csv-filter = File CSV
stats-exported = Le sessioni sono in { $path }.
stats-under-minute = < 1 min
stats-minutes = { $m } min
stats-hours = { $h } h { $m } min
stats-today = Oggi: { $time } · { $chars } caratteri
stats-week = 7 giorni: { $time } · { $chars } caratteri
stats-all = In tutto: { $time } · { $chars } caratteri
stats-chart = Tempo guardato per giorno
stats-range = { $n } giorni
stats-day = { $date }: { $time } · { $chars } caratteri
stats-calendar = L'anno
stats-series = Per serie
stats-time-chars = { $time } · { $chars } caratteri
stats-sessions = Sessioni
stats-none = Ancora nessuna sessione: vengono registrate mentre guardi.
stats-session = { $time } · { $chars } caratteri · { $lookups } { $lookups ->
    [one] ricerca
   *[other] ricerche
} · { $cards } { $cards ->
    [one] carta
   *[other] carte
}
stats-delete = Cancella
stats-delete-session = Cancella la sessione del { $date }, { $name }

settings-servers = Server multimediali
servers-added = Server aggiunti
servers-none = Ancora nessun server. Nikomiru si collega a un server solo dopo che lo aggiungi.
servers-add-jellyfin = Aggiungi un server Jellyfin
servers-name = Nome
servers-address = Indirizzo
servers-address-hint = http://192.168.1.10:8096
servers-user = Utente
servers-password = Password
servers-unencrypted = Consenti una connessione non cifrata
servers-unencrypted-warning = Password e token viaggeranno in chiaro. Serve solo per un indirizzo http:// fuori dalla tua rete.
servers-log-in = Accedi
servers-add-plex = Aggiungi server Plex
servers-plex-hint = Accedi su plex.tv dal browser: nessuna password passa da Nikomiru.
servers-sign-in-plex = Accedi con Plex
servers-cancel = Annulla
servers-plex-stopped = L'accesso a Plex si è fermato prima di finire.
servers-check = Librerie
servers-remove = Rimuovi
servers-added-count = { $count ->
    [one] Un server aggiunto.
   *[other] { $count } server aggiunti.
}
servers-this-server = Questo server
servers-search = Cerca in questa libreria
servers-empty = Qui non c'è niente.
servers-more = Altri
servers-watched = Visto
servers-started = Iniziato

settings-backup = Backup
backup-file = Backup su file
backup-file-hint = Impostazioni, libreria, avanzamento, statistiche e sottotitoli scaricati in un solo file.
backup-dictionaries = Includi i dizionari (grandi)
backup-secrets = Includi chiavi e password
backup-secrets-warning = Il file le conterrà in chiaro: conservalo in un posto sicuro.
backup-make = Crea backup…
backup-restore = Ripristina…
backup-filter = Backup di Nikomiru
backup-restore-confirm = Un backup del { $date }, creato da Nikomiru { $version }. Sostituisce impostazioni, libreria e statistiche al prossimo avvio; ciò che c'è ora resta in before-restore.
backup-restore-yes = Ripristina al prossimo avvio
backup-done = Backup scritto.
backup-restored = Il backup è pronto: chiudi e riapri Nikomiru per ripristinarlo.
sync-title = Sincronizzazione WebDAV
sync-hint = Avanzamento, statistiche e carte restano uguali sui tuoi computer, tramite una cartella su un tuo server (Nextcloud e simili: usa una password per app). Impostazioni, serie e dizionari restano su ogni computer. I file sul server non sono cifrati.
sync-automatic = Sincronizza all'avvio e alla chiusura
sync-now = Sincronizza ora
sync-saved = Salvato.
sync-checked = La cartella è scrivibile.
sync-done = Sincronizzato con { $devices } altri dispositivi; { $skipped } saltati.

chapters = Capitoli
chapter-n = Capitolo { $n }
word-audio-setup = Nessuna fonte per la pronuncia: aggiungine una nelle impostazioni
hooker-title = Texthooker
hooker-connected = Connesso
hooker-waiting = In attesa dell'hooker…
hooker-clear = Svuota
hooker-clipboard = Leggi anche gli appunti
hooker-hint = Qui compaiono le righe dal WebSocket di Textractor o LunaTranslator: clicca una parola per cercarla e crea una carta dal popup, con la frase e senza immagine né audio. Ctrl+V aggiunge ciò che hai copiato. Leggere gli appunti in background richiede wl-paste o xclip e non funziona su GNOME Wayland; si tiene solo il testo giapponese.
hooker-no-clipboard = Nessun programma per leggere gli appunti: installa wl-paste o xclip.

books-title = Libri
books-add = Aggiungi un libro o un manga…
books-folder = Aggiungi una cartella di pagine…
books-none = Ancora nessun libro. Aggiungi un EPUB, un CBZ o una pagina HTML: il file resta dov'è. Oppure apri una pagina web dal suo indirizzo.
books-audiobook = Apri un audiolibro…
books-audio-filter = File audio
books-filter = Libri EPUB, manga CBZ e pagine HTML
books-remove = Rimuovi { $title }
books-back = ← Libri
books-vertical = Verticale
books-next = Capitolo successivo
books-read = Leggi
books-score-analyse = Analizza
books-score-refresh = Aggiorna
books-score-analyse-title = Analizza la comprensione di { $title }
books-score-counting = Analisi del libro…
books-score-summary = Comprensione: { $known }% · { $clear }%
books-score-detail = { $known }% delle occorrenze di parole note · { $clear }% dei paragrafi senza parole nuove, nell'intero libro
books-score-no-text = Nessun testo analizzabile
books-score-too-long = Troppo grande per l'analisi del libro
books-score-failed = Analisi non disponibile
books-score-wait = È in corso l'analisi di un altro libro
books-score-source-changed = Il file del libro è cambiato. Aprilo per aggiornare la scheda in libreria, poi ripeti l'analisi.
books-words = Parole
books-words-search = Cerca una parola o lettura
books-chapter-score = Capitolo: { $known }% parole note · { $clear }% paragrafi senza parole nuove
books-chapter-counting = Conteggio del capitolo: { $done } di { $of } paragrafi
books-chapter-too-long = Il capitolo è troppo lungo per il conteggio. Puoi continuare a leggere.
books-previous = Capitolo precedente
books-next-page = Pagina successiva
books-previous-page = Pagina precedente
manga-double = Due pagine
manga-page = Pagina { $number }
manga-bad-page = Questa pagina non si può mostrare.
manga-offer = Per leggere il testo di una pagina, Nikomiru ha bisogno di un modello di lettura da { $size } MB, scaricato una volta sola da Hugging Face. Funziona su questo computer: le pagine non vanno da nessuna parte.
manga-credit = Modello: manga-ocr di kha-white (Apache-2.0), addestrato sul dataset Manga109-s.
manga-fetch = Scarica il modello
manga-fetching = Scaricamento del modello: { $percent } %
manga-hint = Trascina su un fumetto per leggerlo.
manga-reading = Lettura…
manga-nothing = Lì non è stato trovato testo.

# Review (REV-001…REV-006)
review-title = Ripasso
review-title-due = Ripasso ({ $count })
review-deck = ripasso di Nikomiru
review-in-library = Questa carta è nel ripasso di Nikomiru: Libreria → Ripasso.
error-review-no-library = Le carte del ripasso hanno bisogno della libreria, che non si è potuta aprire.
review-count-new = Nuove: { $count }
review-count-learning = In apprendimento: { $count }
review-count-due = Da ripassare: { $count }
review-reload = Aggiorna
review-undo = Annulla
review-done = Niente da ripassare per ora.
review-next = La prossima carta torna tra { $when }.
review-total = { $count ->
    [one] Una carta nel ripasso.
   *[other] { $count } carte nel ripasso.
}
review-empty-hint = Per fare le carte qui invece che in Anki, scegli "ripasso di Nikomiru" in Impostazioni → Anki → Le carte vanno in: il pulsante della carta in un popup le aggiunge.
review-show = Mostra la risposta
review-show-keys = Spazio o Invio; poi 1–4 per rispondere
review-sentence-audio = ▶ Frase
review-word-audio = ♪ Parola
review-scene = Apri la scena
review-delete = Elimina
review-delete-confirm = Eliminare? Clicca di nuovo
review-again = Di nuovo
review-hard = Difficile
review-good = Bene
review-easy = Facile
review-minutes = { $n } min
review-hours = { $n } h
review-days = { $n } g
review-months = { $n } mesi
review-years = { $n } anni
anki-destination = Le carte vanno in
anki-destination-anki = Anki
anki-destination-nikomiru = ripasso di Nikomiru
anki-destination-later = Anki, più tardi
anki-destination-later-hint = Le carte si fanno subito, con audio e immagine, e restano su questo computer: Anki può essere chiuso. Mandale qui sotto quando è aperto.
anki-later = Carte per dopo ({ $count })
anki-later-none = Nessuna carta tenuta.
anki-later-remove = Togli
anki-later-damaged = Una carta che non si può leggere: toglila.
anki-later-send = Manda ad Anki
anki-destination-hint = Il ripasso di Nikomiru tiene le carte nella libreria e le ripropone con FSRS nella scheda Ripasso della pagina Libreria: Anki non serve.
review-settings = Ripasso in Nikomiru
review-retention = Quota di carte ricordate quando tornano
review-new-per-day = Carte nuove al giorno
books-web = Pagina web
books-web-open = Apri
error-book-not-page = Questa pagina non si può leggere: il suo testo è in una codifica che Nikomiru non conosce.
stats-words = Parole
stats-words-set = Note: { $known } · in studio: { $learning } · ignorate: { $ignored }
stats-words-anki = Da Anki: { $known } note, { $learning } in studio
stats-words-days = Parole note, ultimi 30 giorni
stats-words-day = { $ago ->
    [0] Oggi
    [1] Ieri
   *[other] { $ago } giorni fa
}: { $count } parole note
stats-review = Ripasso
stats-review-today = Oggi: { $count } risposte, { $kept } ricordate
stats-review-retention = Ricordate in ripasso, ultimi 30 giorni: { $percent } %
stats-review-retention-none = Ricordate in ripasso, ultimi 30 giorni: ancora nessuna risposta
stats-review-cards = { $new } nuove · { $learning } in apprendimento · { $young } giovani · { $mature } mature
stats-review-days = Risposte al giorno, ultimi 30 giorni
stats-review-day = { $ago ->
    [0] Oggi
    [1] Ieri
   *[other] { $ago } giorni fa
}: { $count } risposte
stats-review-ahead = Carte in scadenza, prossimi 7 giorni
stats-review-due = { $in ->
    [0] Oggi
    [1] Domani
   *[other] Tra { $in } giorni
}: { $count } carte
review-study = Studia
review-cards = Carte ({ $count })
review-search = Cerca
review-no-match = Nessuna carta lo contiene.
review-shown = Le prime { $shown } di { $count } carte: cerca per trovare le altre.
review-phase-new = Nuova
review-phase-learning = In apprendimento
review-phase-review = In ripasso
review-phase-relearning = Da riapprendere
review-due-now = da ripassare ora
review-answers = { $reps ->
    [one] 1 risposta
   *[other] { $reps } risposte
}, { $lapses } dimenticate
review-reset = Ricomincia
review-reset-confirm = Ricominciare? Clicca di nuovo
books-along = Leggi ascoltando
books-along-hint = Avvia un audiolibro con il suo testo sincronizzato, poi torna al suo libro: la pagina segue ciò che ascolti.
books-along-none = Non sta suonando niente con testo sincronizzato: apri prima l'audiolibro.
books-along-not-found = Nessuna riga dell'audiolibro si trova in questo libro.
books-along-found = { $found } righe dell'audiolibro su { $count } trovate nel libro.
books-listen = Ascolta da qui

# Setup assistant (INS-001…INS-006)
setup-title = Assistente di configurazione
setup-close = Chiudi
setup-open = Configura dizionari, audio, carte e strumenti…
setup-open-hint = Scarica dizionari liberi, audio delle parole e strumenti di supporto, e scegli come creare le carte.
setup-first-steps = Dizionari, audio delle parole, carte e strumenti in un posto solo:
setup-intro = Scegli cosa installare. Nulla viene scaricato prima di premere Installa; i download continuano mentre usi Nikomiru. I download incompleti degli strumenti ricominciano al nuovo tentativo.
setup-dictionaries = Dizionari
setup-dictionaries-hint = Scaricati da dove li pubblicano i loro autori, poi importati come un file scelto da te.
setup-license-unstated = condizioni della fonte, non dichiarate
setup-audio = Audio delle parole
setup-audio-hint = Dal pacchetto consigliato di Fushi: si scarica solo la libreria spuntata, non i dizionari del pacchetto. Il pacchetto è fatto dall'autore di Fushi e le licenze delle registrazioni non sono dichiarate.
setup-audio-japanese = Pronunce giapponesi
setup-audio-english = Pronunce inglesi
setup-install = Installa
setup-stop = Ferma
setup-cards = Carte
setup-cards-anki = Anki
setup-cards-review = ripasso di Nikomiru
setup-anki-answers = Anki risponde (AnkiConnect { $version }).
setup-anki-lapis-in-use = Le carte usano il tipo di nota Lapis.
setup-anki-use-lapis = Usa il tipo di nota Lapis
setup-anki-away = Anki non risponde.
setup-anki-steps = Installa Anki da apps.ankiweb.net, poi in Anki: Strumenti → Componenti aggiuntivi → Ottieni componenti…, codice 2055492159 (AnkiConnect), e riavvia Anki. Tieni Anki aperto mentre fai le carte.
setup-anki-retry = Riprova
setup-anki-later = Intanto tieni le carte per dopo
setup-state-installed = installato
setup-state-waiting = in attesa
setup-state-downloading = { $percent } % di { $size }
setup-state-importing = importazione
setup-state-done = fatto
setup-finished = L'assistente di configurazione ha finito.
error-setup-space = Spazio libero insufficiente: servono { $need }, liberi { $free }.
error-setup-no-library = Il pacchetto non contiene { $name }.
error-setup-not-audio = Il file scaricato non è un database di audio delle parole ({ $reason }).

# AUD-010
condense-export = Esporta audio condensato
condense-folder = Cartella per l'audio condensato
condense-hint = Un file audio per episodio, con un capitolo per battuta. Usa le scelte di sottotitoli e audio salvate per l'episodio.
condense-running = Esportazione dell'audio condensato…
condense-cancel = Annulla esportazione audio
condense-cancelled = Esportazione annullata. File scritti: { $count }; episodi non riusciti: { $failed }.
condense-done = File audio scritti: { $count }; episodi non riusciti: { $failed }.
condense-no-subtitles = Nessun sottotitolo di testo utilizzabile. Scegli una traccia di testo o leggi prima i sottotitoli grafici come testo.
condense-no-audio = Questo episodio non ha una traccia audio da esportare.
condense-local-only = L'esportazione audio di una serie richiede un video locale.
condense-failed = Esportazione audio non riuscita: { $reason }

# PLY-023
webvideo-open = Apri video dal web
webvideo-address = Indirizzo della pagina del video
webvideo-hint = Apri la pagina di un video con yt-dlp installato sul computer. Puoi cercare le parole dei suoi sottotitoli di testo e creare carte.
webvideo-cancel = Annulla
webvideo-play = Apri video
webvideo-opening = Apertura del video dal web…
webvideo-unavailable = Impossibile avviare il servizio per i video dal web. Puoi continuare a riprodurre i file locali.
webvideo-missing = yt-dlp non trovato. Installalo o impostane il percorso in Impostazioni → Avanzate.
webvideo-failed = Impossibile aprire il video dal web. Controlla l'indirizzo, la disponibilità del video e che yt-dlp sia aggiornato.
webvideo-stream = Nessun flusso multimediale supportato. Dirette, video protetti da DRM e video disponibili solo a segmenti non sono supportati.
webvideo-invalid = Inserisci un indirizzo HTTPS valido della pagina di un video.
webvideo-cancelled = Apertura del video dal web annullata.
settings-yt-dlp = Eseguibile yt-dlp
settings-yt-dlp-hint = Vuoto: NIKOMIRU_YT_DLP, poi PATH

# KNW-017: explicit series comprehension
library-score-analyse = Analizza
library-score-refresh = Aggiorna
library-score-analyse-title = Analizza la comprensione di { $title }
library-score-loading = Lettura dell'elenco episodi…
library-score-counting = Analisi episodi: { $done } / { $total }
library-score-summary = Comprensione: { $known }% · { $clear }%
library-score-detail = { $known }% delle occorrenze di parole note · { $clear }% delle righe senza parole nuove, negli episodi analizzati
library-score-coverage = { $scored } / { $total } episodi analizzati · { $failed } non riusciti
library-score-unavailable = Nessun sottotitolo analizzabile
library-score-too-large = Troppi episodi per l'analisi della serie

# KNW-018: historical playback encounters
stats-encounters = Parole nuove incontrate per episodio
stats-encounters-detail = Parole distinte che erano nuove quando le hai incontrate durante la riproduzione. I cambi di stato successivi conservano questo storico.
stats-encounters-episode = { $title }: { $count } parole nuove incontrate

condense-reading = Attendi il completamento della lettura dei sottotitoli prima di esportare l’audio condensato.

error-subtitle-vobsub-index = L’indice VobSub è danneggiato o non identifica una traccia di lingua leggibile.

analysis-cancel = Annulla analisi

analysis-words = Parole da studiare
analysis-words-back = Torna all’elenco

analysis-export-shown = Esporta visibili
analysis-export-selected = Esporta selezionate
analysis-export-saved = Vocabolario salvato in { $path }.
analysis-export-too-large = Il vocabolario supera il limite di esportazione di 16 MiB.

encounter-history-open = Esamina cronologia
encounter-history-clear = Cancella cronologia episodio
encounter-history-coverage = Visualizzate { $shown } di { $total } parole storiche (primo incontro: ms Unix).

analysis-order = Ordine per comprensione
analysis-order-original = Ordine della libreria
analysis-order-most = Più conosciuti prima
analysis-order-least = Meno conosciuti prima
analysis-only-scored = Solo analizzati
analysis-minimum-known = Minimo conosciuto %

analysis-source-changed = La sorgente o i dizionari sono cambiati durante l’analisi. Aggiorna il conteggio.

vocabulary-status-filter = Filtro stato parole
vocabulary-all-statuses = Tutti gli stati
vocabulary-card-filter = Filtro schede
vocabulary-any-card = Qualsiasi scheda
vocabulary-with-card = Con scheda
vocabulary-without-card = Senza scheda

vocabulary-order = Ordine del vocabolario
vocabulary-frequency-desc = Più frequenti prima
vocabulary-frequency-asc = Meno frequenti prima
vocabulary-expression = Ordine per parola
vocabulary-reading = Ordine per lettura

vocabulary-copy-shown = Copia visibili
vocabulary-copy-selected = Copia selezionate
vocabulary-copied = Vocabolario copiato.

encounter-export = Esporta cronologia completa episodio
encounter-export-saved = Cronologia dell’episodio salvata in { $path }.

episode-words-title = Parole da studiare: { $title }

sub-reading-preparing = Preparazione lettura sottotitoli…
sub-reading-progress = Lette { $done } / { $total } immagini
sub-reading-cancel = Annulla lettura

sub-export = Esporta testo corrente come SRT…
sub-export-filter = Sottotitoli SubRip
sub-export-unavailable = Per esportare servono sottotitoli testuali completi e non vuoti.
sub-export-too-large = La traccia supera il limite di esportazione di 16 MiB.
sub-export-saved = Sottotitoli salvati in { $path }.

sub-export-failed = Impossibile salvare i sottotitoli: { $reason }

analysis-cache-title = Cache dei conteggi di analisi
analysis-cache-usage = { $files } file · { $bytes } byte
analysis-cache-unavailable = Utilizzo della cache non disponibile.
analysis-cache-refresh = Aggiorna utilizzo cache
analysis-cache-clear = Svuota cache conteggi
analysis-cache-detail = Fino a 64 MiB di conteggi derivati di libri/episodi. Le analisi correnti restano aperte; il lavoro completato può riempire di nuovo la cache.

study-report-export = Esporta riepiloghi analizzati…
study-report-empty = Non ci sono analisi completate disponibili per questo riepilogo.
study-report-saved = Riepilogo di comprensione salvato in { $path }.
study-report-failed = Impossibile salvare il riepilogo: { $reason }

# Strumenti di supporto privati selezionati (INS-007).
setup-tools = Strumenti di supporto
setup-tools-hint = Seleziona gli strumenti da installare. Quelli già funzionanti vengono riutilizzati. yt-dlp richiede anche FFmpeg e Deno. I download vengono verificati e installati per questa applicazione senza privilegi di amministratore.
setup-runtime-ready = Le librerie di riproduzione, grafica e TLS sono fornite dal pacchetto dell’applicazione o dal sistema operativo.
setup-tools-refresh = Controlla strumenti installati
setup-tool-required = Richiesto dalla selezione
setup-tool-available = Disponibile localmente; verrà riutilizzato
setup-tool-model = Modello OCR locale
setup-tool-checking = Estrazione e verifica…
setup-tool-unsupported = Nessun programma di installazione per questa piattaforma
setup-tool-cancelled = Installazione annullata
setup-tool-invalid = Lo strumento o il piano di installazione non è valido.
setup-tool-missing = Strumento non trovato.
setup-tool-archive = L’archivio è incompleto o contiene elementi non sicuri.
setup-tool-too-large = Il download o i file estratti superano il limite di dimensione.
setup-tool-damaged = La posizione di installazione non può essere sostituita in sicurezza.
setup-tool-space = Spazio libero insufficiente per l’installazione.
setup-tool-dependency = Impossibile installare un componente richiesto.
setup-tool-failed = Installazione non riuscita: { $reason }
settings-deno = Eseguibile o cartella di Deno

# Subtitle study selections (SUB-035).
sub-study-tools = Strumenti di studio
sub-select-shown = Seleziona righe visibili
sub-selection-clear = Svuota selezione righe
sub-export-selected = Esporta righe selezionate come SRT…
sub-select-line = Seleziona riga

sub-copy-shown = Copia trascrizione visibile
sub-copy-selected = Copia trascrizione selezionata
sub-transcript-copied = Trascrizione copiata
