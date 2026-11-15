# Guida di Nikomiru

Nikomiru è un lettore video per imparare il giapponese. Clicchi una parola dei sottotitoli per cercarla nei tuoi dizionari Yomitan, e con un altro clic ne fai una carta Anki.

*English: [user guide in English](en.md).*

## 1. Installazione

### Arch Linux

Il PKGBUILD è nel repository, in `packaging/aur`. Quando Nikomiru sarà su AUR, `yay -S nikomiru` farà lo stesso.

```sh
git clone https://github.com/KvasFan/nikomiru.git
cd nikomiru/packaging/aur
makepkg -si
```

### Flatpak

Un bundle creato con `cargo xtask flatpak` si installa con `flatpak install --user nikomiru.flatpak`; più avanti su Flathub. Il Flatpak porta la sua libmpv e usa l'FFmpeg del runtime freedesktop.

### Dal sorgente

Servono:

- Rust tramite `rustup`;
- libmpv ≥ 0.37 e FFmpeg ≥ 6;
- OpenGL con EGL e le librerie client di Wayland o X11;
- libxkbcommon, fontconfig e OpenSSL 3;
- un carattere con i glifi giapponesi, per esempio Noto Sans CJK.

Per compilare servono anche i file di sviluppo di libmpv e OpenSSL, e `pkg-config`.

```sh
git clone https://github.com/KvasFan/nikomiru.git
cd nikomiru
cargo xtask install      # in ~/.local, con la voce di menu e l'icona
```

## 2. Primo avvio

Avvia Nikomiru dal menu, oppure con `nikomiru percorso/episodio.mkv`. Senza un file si apre sulla **Libreria**, che comincia con i **primi passi**: un dizionario, una cartella di video e una prima carta, ognuno con un pulsante che porta lì. Spariscono una volta fatti, o con **Nascondi**.

- **Aprire un video**: Ctrl+O, trascinandolo sulla finestra, o dalla libreria.
- **Sottotitoli**: Nikomiru carica da solo quelli accanto al video, anche se hanno un altro nome (cartelle `Subs/`, `Serie 02.ja.srt`). Carica anche le tracce di sottotitoli dentro il video. Per scegliere c'è il menu 字 nella barra.
- **Indietro**: Esc, oppure "← Libreria" nella barra.
- **Trovare una scena**: il puntatore sopra la barra di avanzamento mostra l'orario e il fotogramma in quel punto.

## 3. Dizionari

Nikomiru legge gli stessi file ZIP di Yomitan: termini (JMdict, Jitendex…), kanji, frequenze e accento tonale. Non ne include nessuno.

1. Apri i dizionari: 📚 nella barra, oppure Impostazioni → Dizionari.
2. Scegli **Importa…**, oppure trascina lo ZIP sulla finestra.
3. Ogni tipo ha il suo elenco, ordinato per priorità; per spostare un dizionario trascinalo dalla sua maniglia.
4. **CSS** ti permette di cambiare lo stile delle voci di un dizionario. Si applica solo un sottoinsieme sicuro del CSS.

Oltre ai file ZIP di Yomitan, l'import accetta dizionari StarDict (scegli il `.ifo`), DSL (`.dsl`, `.dsl.dz`) e MDict (`.mdx`). Le definizioni appaiono come testo semplice, senza immagini. Un MDict legato a un utente non si può leggere.

I sottotitoli a immagini letti come testo ora compaiono progressivamente: la lettura parte vicino alla posizione corrente e le righe non ancora lette mantengono le immagini. Puoi cercare le parole del testo già pronto mentre il lavoro continua. Comprensione dell’intera traccia, accelerazione delle pause ed esportazione dell’audio condensato attendono il completamento; cambiare o disattivare i sottotitoli annulla la lettura precedente.

Per leggere una coppia VobSub esterna, apri o trascina il file `.idx` o `.sub` mentre è caricato un video. Mantieni accanto il file compagno con lo stesso nome; le maiuscole delle estensioni non contano. Il modello locale legge la lingua preferita con i timestamp dell’indice. Riaprire il video ripristina questa scelta esplicita. Le coppie esterne non mostrano immagini mentre il testo è in preparazione; file compagni mancanti o danneggiati producono un errore.

Anche i sottotitoli bitmap DVB incorporati offrono Leggi come testo. FFmpeg converte le immagini in locale per il modello installato; il televideo è escluso.

## 4. Cercare le parole

- **Clic** su un carattere di un sottotitolo: il popup mostra la parola più lunga che comincia lì, con letture, significati, accento tonale e frequenze.
- **Shift + passaggio del puntatore** cerca senza cliccare. In Impostazioni → Ricerca puoi far funzionare anche il passaggio semplice.
- **Tastiera**:
  - Invio cerca nel sottotitolo mostrato;
  - ← → spostano la ricerca alla parola dopo o prima, e di un carattere oltre l'ultima;
  - Tab e Shift+Tab passano da una voce all'altra;
  - Esc chiude il popup.
- **Seleziona del testo** in una definizione per cercarlo in un popup annidato.
- **Riascolta** ripete la battuta; **Vai alla frase** salta al suo inizio.
- **♪** riproduce la parola stessa, se hai impostato una fonte in Impostazioni → Audio delle parole.
- **Le parole che hai già minato** sono sottolineate in verde nei sottotitoli e nella lista dei sottotitoli. In Impostazioni → Sottotitoli puoi spegnere la sottolineatura, o accendere le letture sopra i kanji. Entrambe vengono dai tuoi dizionari, che dividono ogni battuta in parole: dove una parola è tagliata male, anche la sua lettura può essere sbagliata.

Per sentire una parola, Nikomiru ha bisogno di una fonte di pronunce: non ne include nessuna. Senza fonte il popup mostra `♪?`, che apre Impostazioni → Audio delle parole: aggiungi un `android.db` di Local Audio Server, oppure un indirizzo con `{term}` e `{reading}`.

## 5. Carte Anki

1. Installa in Anki il componente aggiuntivo [AnkiConnect](https://ankiweb.net/shared/info/2055492159) (codice `2055492159`) e tieni Anki aperto.
2. In Nikomiru, Impostazioni → Anki: **Prova la connessione**, poi **Crea mazzo Lapis**. Se preferisci, scegli un tuo mazzo e un tuo tipo di nota e associa i campi.
3. In un popup, **+** su una voce (o Ctrl+Invio) crea la carta. Contiene:
   - la parola e i suoi significati;
   - la frase con la parola in grassetto;
   - il fotogramma (oppure, in Impostazioni → Anki → Formato immagine, tutta la battuta come immagine animata o video WebM con audio) e l'audio della battuta;
   - un link che riapre la scena.
4. **✓** vuol dire che Anki ha già la parola: dal suo menu apri la carta in Anki, oppure ne aggiungi un'altra comunque.
5. **Shift+clic su +** (o Ctrl+Shift+Invio) crea la carta a mano. Puoi:
   - aggiungere le righe prima e dopo;
   - spostare la clip e ascoltarla;
   - scegliere l'immagine, oppure togliere l'audio.

### Senza Anki: il ripasso di Nikomiru

In Impostazioni → Anki, **Le carte vanno in → ripasso di Nikomiru** tiene le carte in Nikomiru: Anki non serve. Il **+** di un popup le crea lì, con la frase, il suo audio, il fotogramma e i significati.

Tornano nella scheda **Ripasso** della pagina Libreria, secondo un calendario calcolato con FSRS:

1. Il fronte mostra la parola e la sua frase. Pensa al significato, poi **Mostra la risposta** (Spazio o Invio): compare la lettura e parte la frase.
2. Rispondi con **Di nuovo**, **Difficile**, **Bene** o **Facile** (tasti 1–4). Ogni pulsante dice quando la carta tornerà.
3. **Annulla** (Ctrl+Z) ritira l'ultima risposta. **Elimina**, cliccato due volte, toglie la carta.

**Carte**, in cima alla scheda, elenca tutte le carte con una ricerca: quando torna ognuna, le sue risposte, e **Ricomincia** per impararne una da capo.

La scheda Statistiche mostra come va il ripasso: le risposte di oggi, la quota ricordata negli ultimi 30 giorni, le carte giovani e mature, e le carte in scadenza nella prossima settimana.

La quota di carte da ricordare quando tornano (90 % di base) e le carte nuove al giorno (20) si impostano nella stessa scheda. Il ripasso è nei backup e nella sincronizzazione WebDAV, con audio e immagini.

## 6. Sottotitoli online

Nulla va online se non lo chiedi.

1. In Impostazioni → Servizi sottotitoli, spunta Jimaku e/o OpenSubtitles e incolla le loro chiavi. Quella di Jimaku è gratuita; per OpenSubtitles serve una chiave consumer.
2. Nel menu 字, **Cerca online…** trova i sottotitoli dell'episodio. **Usa** ne scarica uno e lo carica.
3. Se vuoi, i **sottotitoli automatici** li cercano da soli per un video che non ne ha in giapponese.
4. **Allinea con l'audio** (menu 字) corregge i sottotitoli in anticipo o in ritardo.

## 7. Libreria

- **Aggiungi cartella…** importa le tue serie. Gli episodi vengono raggruppati per nome (stagioni, numeri di episodio), oppure per cartella.
- **Continua a guardare** mostra quello che hai iniziato.
- Alla fine di un episodio, il successivo parte dopo un conto alla rovescia di 5 secondi.
- Una serie ricorda il sottotitolo, il ritardo e la lingua dell'audio.
- Con i colori delle parole attivi, ogni episodio mostra `80% · 25%`: occorrenze di parole note e righe senza parole nuove. Contano anche i sottotitoli di testo incorporati nei video locali, con la stessa traccia salvata e priorità di lingua della riproduzione. `…` indica il conteggio; sottotitoli a immagini, video remoti e sottotitoli disattivati non hanno punteggio. Riapri la serie per aggiornare le sorgenti dei sottotitoli modificate.
- Nella pagina di una serie puoi rinominarla, unirla a un'altra, spostare o togliere episodi, o eliminarla. I tuoi file non vengono mai toccati.
- **Identifica online** chiede ad AniList (o a MyAnimeList) titoli, episodi e copertina della serie. Se più serie corrispondono, scegli tu. Jimaku poi trova i sottotitoli dal suo id AniList.
- **Statistiche** mostra il tempo guardato e i caratteri letti:
  - per giorno, per serie e per sessione;
  - un calendario dell'anno;
  - un'esportazione CSV.

Con i colori delle parole attivi, **Analizza** sotto la scheda di una serie ne calcola la comprensione senza aprirla o riprodurla. Il punteggio combina le occorrenze delle parole e le righe degli episodi con testo utilizzabile; il numero di episodi analizzati sul totale indica la copertura, compreso il numero di analisi non riuscite. **Aggiorna** rilegge la serie e le sorgenti. Vengono conservati fino a quattro risultati; cambiare lo stato delle parole li aggiorna subito. Le serie con più di 256 episodi non hanno un punteggio complessivo.

Con i colori delle parole attivi, Statistiche mostra anche **Parole nuove incontrate per episodio**: i lemmi distinti che erano nuovi quando li hai incontrati nel sottotitolo principale attivo durante la riproduzione. Ripetizioni e riascolti contano una volta per episodio; gli spostamenti in pausa e le analisi dalla libreria non contano. Lo storico resta quando una parola diventa nota, sopravvive al riavvio e segue backup e sincronizzazione WebDAV. La visione precedente e le righe non segmentate mentre erano attive non vengono stimate in seguito. Sono mostrati i 200 episodi con gli incontri più recenti.

### Texthooker

Per visual novel e giochi: "Texthooker" nella pagina della libreria mostra le righe che Textractor o LunaTranslator inviano sul loro WebSocket (`ws://127.0.0.1:6677` di default). Clicca una parola per cercarla; una carta creata da lì ha la parola e la frase, senza immagine né audio. Ctrl+V aggiunge ciò che hai copiato. Leggere gli appunti da solo richiede `wl-paste` o `xclip` e non funziona su GNOME Wayland.

### Libri

"Libri" nella pagina della libreria legge file EPUB: scegline uno, e il file resta dov'è. Clicca una parola per cercarla e crea una carta dal popup, con la frase e senza immagine né audio. Un libro si riapre dove l'hai lasciato. "Verticale" passa alla scrittura verticale, con ← e → o la rotella per girare pagina; un libro che si legge da destra si apre così.

Con **Colora le parole secondo quanto le conosci** attivo in Impostazioni → Sottotitoli, libri e pagine web sottolineano le parole nuove in blu e quelle in studio in giallo. Il punteggio del capitolo mostra le parole note e i paragrafi senza parole nuove; le parole ignorate sono escluse. **Parole** elenca i lemmi del capitolo per frequenza: cerca per parola o lettura, filtra quelle nuove/in studio, selezionane alcune o tutte quelle mostrate e scegli lo stato. **Leggi** riporta alla posizione di lettura. I capitoli molto lunghi restano leggibili senza punteggio o elenco del capitolo. I paragrafi eccezionalmente lunghi non vengono colorati.

Nell'elenco Libri, **Analizza** mostra la comprensione di tutti i capitoli di testo senza aprire il libro o cambiare la posizione di lettura. I colori delle parole devono essere attivi. **Aggiorna** rilegge il file locale; cambiare lo stato delle parole aggiorna subito un punteggio conservato. Vengono conservate fino a quattro analisi finché non chiudi l'app, disattivi i colori delle parole o cambi i dizionari. Per le pagine web salvate viene usato il testo locale. Manga, libri di sole immagini e libri oltre i limiti di analisi non hanno punteggio; possono essere aperti come al solito.

### Pagine web

Nella pagina Libri, scrivi l'indirizzo di una pagina accanto a **Pagina web** e premi **Apri**: un articolo di NHK News Web Easy, un capitolo di un romanzo online, un'opera di Aozora Bunko. Nikomiru scarica solo quella pagina, niente di ciò a cui rimanda e nessuna immagine, e ne legge il testo come quello di un libro, senza menu e piè di pagina, con i furigana. Resta nell'elenco, con il sito come autore, e si riapre senza rete. Anche un file HTML salvato sul tuo computer si apre con "Aggiungi un libro o un manga…", in qualsiasi codifica giapponese.

### Manga

Aggiungi un CBZ o una cartella di immagini nella pagina dei libri. "Due pagine" le mostra affiancate, la prima a destra. Freccia sinistra, Pagina giù, Spazio e rotellina vanno avanti, freccia destra e Pagina su indietro.

Per leggere un fumetto, trascina sopra di esso. La prima volta la pagina propone un modello di lettura da 111 MB: arriva da Hugging Face una volta sola, solo se lo chiedi, e poi funziona sul tuo computer, quindi le pagine non vanno da nessuna parte. Il testo compare sopra la pagina: clicca una parola per cercarla, e una carta ha il testo e la parte di pagina. Il modello legge un fumetto o una riga alla volta e impiega qualche secondo.

### Audiolibri

"Apri un audiolibro…" nella pagina dei libri riproduce un file audio. Mettigli accanto il testo con lo stesso nome, come `.lrc`, `.srt`, `.vtt` o `.ass`: Nikomiru non lo genera. La lista dei sottotitoli si apre per seguire la lettura; clicca una parola per cercarla, e una carta ha la riga e il suo audio.

Con l'audiolibro in riproduzione, apri il suo libro nella pagina dei libri e premi **Leggi ascoltando**: la pagina segue ciò che senti, con il paragrafo evidenziato, e **Ascolta da qui** riproduce dal paragrafo in cima alla pagina.

### Jellyfin e Plex

Aggiungi un server in Impostazioni → Server multimediali: per Jellyfin indirizzo, utente e password (la password non viene conservata); per Plex accedi su plex.tv dal browser. Ogni server ha una scheda nella pagina della libreria. I video partono dal file originale, con gli stessi sottotitoli, ricerche e carte di un file locale, e posizione e stato di visione tornano al server. Nikomiru si collega a un server solo mentre la sua scheda è aperta o uno dei suoi video è in riproduzione.

**Apri video dal web**, nella Libreria o nella barra del player, accetta l'indirizzo della pagina di un video. Puoi anche trascinare l'indirizzo o passarlo dalla riga di comando. Installa `yt-dlp`; se non è nel PATH, imposta l'eseguibile in Impostazioni → Avanzate oppure con `NIKOMIRU_YT_DLP`. Le lingue preferite si applicano ai sottotitoli caricati dall'autore e a quelli automatici. Ricerca e carte funzionano come per i file locali; i collegamenti delle carte riaprono la pagina originale alla scena. Sono supportati video su richiesta disponibili come file HTTP, anche con audio separato; dirette, DRM e flussi disponibili solo a segmenti restano esclusi.

## 8. Impostazioni

⚙ nella barra, oppure Ctrl+, apre le impostazioni. Le schede sono in colonna a sinistra.

| Scheda | Cosa contiene |
|---|---|
| Generale | Lingua (inglese, italiano o giapponese), tema (chiaro, scuro o come il desktop), colore d'accento, scala dell'interfaccia, passo dei salti, episodio successivo, chiavi e password nel portachiavi di sistema |
| Player | Le opzioni di mpv come controlli, preset `mpv.conf`, importazione ed esportazione |
| Sottotitoli | Dimensione, lingue preferite, sfocatura o nascondere finché non ci passi sopra, il sottotitolo secondario, le letture sopra i kanji, le parole minate sottolineate |
| Ricerca | Risultati in un popup, dimensione del popup, passaggio del puntatore |
| Scorciatoie | Tutti i tasti, che puoi cambiare. I tasti sono fisici: restano al loro posto con qualunque disposizione della tastiera |
| Avanzate | Il programma ffmpeg, dove sono il file delle impostazioni e i log |

Ogni scheda ha **Salva** e **Ripristina**. Puoi modificare anche `~/.config/nikomiru/config.toml` a mano: Nikomiru conserva i tuoi commenti.

Generale → Riproduzione → **Velocità tra le battute** accelera a 2–4× le pause di almeno tre secondi e torna alla velocità scelta prima della battuta successiva. Richiede sottotitoli di testo ed è disattivata inizialmente. Pausa, popup del dizionario e replay mantengono la velocità scelta; quella temporanea delle pause non viene salvata per l'episodio.

Il menu dei sottotitoli e i menu di episodi e serie nella libreria offrono **Esporta audio condensato**: scegli Opus o MP3 e una cartella. Ogni episodio diventa un file con le battute dei sottotitoli, con i margini impostati per le carte; le pause sotto un secondo vengono mantenute. I capitoli seguono le battute. I file esistenti vengono preservati e dal menu puoi annullare l'esportazione. Per le serie vengono segnalati separatamente gli episodi senza sottotitoli di testo o audio utilizzabili.

### Backup e sincronizzazione

Impostazioni → Backup scrive impostazioni, libreria, statistiche e sottotitoli scaricati in un solo file, con i dizionari se li spunti. Chiavi e password restano fuori, a meno che non spunti anche quelle. Per ripristinare scegli un backup e riapri Nikomiru: ciò che c'era resta in `before-restore`.

Con una cartella WebDAV (Nextcloud e simili, con una password per app), "Sincronizza ora" mantiene uguali avanzamento, statistiche e carte, anche quelle del ripasso, sui tuoi computer. Impostazioni, serie e dizionari restano su ogni computer, e i file sul server non sono cifrati. Un video è lo stesso su due computer quando il suo file ha lo stesso nome.

## 9. Tastiera

Tutta l'app si usa da tastiera. Nelle pagine e nei dialoghi, Tab, frecce, Invio e Spazio comandano i controlli. Ogni scorciatoia è elencata, e si può cambiare, in Impostazioni → Scorciatoie.

| Tasto | Azione |
|---|---|
| Spazio, P | Riproduci/pausa |
| ← → / A D | Salta indietro / avanti |
| Ctrl+← Ctrl+→ | Sottotitolo precedente / successivo |
| R / Shift+R | Ripeti la battuta / la precedente |
| Invio | Cerca nel sottotitolo mostrato |
| Ctrl+Invio | Crea la carta |
| L / Ctrl+F | Elenco dei sottotitoli / cerca nell'elenco |
| B / H / V | Sfoca / nascondi / disattiva i sottotitoli |
| Z / X | Sottotitoli 100 ms prima / dopo |
| F, F11 | Schermo intero |
| S | Salva il fotogramma |
| Shift+S | Salva il fotogramma con i sottotitoli |
| Ctrl+O | Apri un file |
| Ctrl+, | Impostazioni |
| Esc | Indietro |

## 10. Se qualcosa non va

- I log sono in `~/.local/state/nikomiru/logs/`. Non contengono mai le tue chiavi.
- Se le impostazioni non si salvano, controlla che `~/.config/nikomiru` sia tua. Se è stata creata con `sudo`, riprendila con `sudo chown -R $USER: ~/.config/nikomiru`.
- Se un'impostazione in `config.toml` ha un valore del tipo sbagliato, Nikomiru usa quello predefinito e la nomina in un avviso all'avvio.
- Problemi e idee: <https://github.com/KvasFan/nikomiru/issues>.

Durante l’analisi di un libro intero o di una serie, Annulla analisi interrompe il lavoro e scarta il risultato incompleto. Puoi analizzare subito un altro elemento; riproduzione e punteggi degli episodi continuano.

Dopo l’analisi dell’intero libro, Parole da studiare apre il vocabolario di tutti i capitoli. Cerca una parola o lettura, filtra le parole nuove/in apprendimento, seleziona righe o tutte quelle visibili e assegna loro uno stato insieme. Il punteggio del libro segue questi cambiamenti condivisi. Aggiornare l’analisi chiude il vecchio elenco.

Anche le schede delle serie offrono Parole da studiare dopo l’analisi. Le frequenze sommano gli episodi conteggiati. La copertura resta visibile: gli episodi esclusi o falliti non contribuiscono parole. Cambiare uno stato aggiorna il punteggio della serie e le altre viste di studio.

Negli elenchi delle analisi, Esporta visibili salva i risultati di ricerca/filtro; Esporta selezionate salva tutte le parole spuntate, anche quelle nascoste. Il TSV UTF-8 locale include parola, lettura, stato, occorrenze e presenza di una scheda. L’esportazione fotografa l’elenco al clic, con un limite di 16 MiB; non modifica gli stati delle parole.

Statistiche → Esamina cronologia mostra le ultime 1.000 parole incontrate storicamente in un episodio, con il primo incontro in millisecondi Unix. Cerca parola o lettura. Cancella cronologia episodio rimuove quella cronologia e sincronizza un reset persistente, così i vecchi file dei dispositivi non la ripristinano. Stati delle parole e progressi restano conservati. Le parole già deduplicate nella visita di riproduzione corrente restano escluse fino a una nuova visita.

Con i colori delle parole attivi, Libri e serie locali offrono Ordine per comprensione: originale, più conosciuti o meno conosciuti. Solo analizzati limita l’elenco ai punteggi completati, con una percentuale minima di parole conosciute. Gli elementi senza punteggio vanno in fondo; cambiare stati aggiorna l’ordine. Questi controlli non avviano analisi e non modificano l’ordine salvato nella libreria.

Le analisi completate di libri interi ed episodi/serie possono riusare conteggi locali dopo il riavvio. Verificano contenuto della sorgente e impostazioni dei dizionari, poi applicano gli stati correnti delle parole. Le cache dei conteggi sono limitate complessivamente a 64 MiB (32 MiB per libri e 32 MiB per episodi); risultati danneggiati, eccessivi o annullati vengono ignorati. Le analisi restano esplicite.

La ricerca in vocabolario, parole storiche e lista sottotitoli accetta varianti kana e di larghezza e ignora maiuscole/minuscole. Separa i termini con spazi per richiederli tutti; le parole possono corrispondere alla forma o alla lettura. Cambia solo il filtro, mantenendo grafia originale e bersagli del dizionario.

Gli elenchi di vocabolario offrono anche filtri per stato esatto e presenza di una scheda. Si combinano con ricerca e Solo parole non conosciute. Le parole spuntate nascoste restano selezionate per azioni in blocco o Esporta selezionate; Esporta visibili segue tutti i filtri correnti.

Ordine del vocabolario offre frequenza crescente/decrescente, parola e lettura. Le letture mancanti usano la parola; l’ordine lessicale confronta le stringhe Unicode originali. Le parole selezionate restano tali cambiando ordine e le esportazioni seguono l’ordine scelto.

Copia visibili e Copia selezionate inseriscono negli appunti le stesse colonne TSV del vocabolario. Sono disponibili in tutte le viste delle parole. Copia selezionate include le parole spuntate nascoste, conserva la selezione e segue l’ordine scelto. Valgono gli stessi limiti di 16 MiB e le protezioni per i fogli di calcolo.

Esamina cronologia → Esporta cronologia completa episodio scrive tutte le parole registrate per quell’episodio, oltre il limite di 1.000 righe visualizzate, con i millisecondi Unix del primo incontro originale. Il worker legge l’episodio scelto al momento del salvataggio. Il TSV atomico, limitato a 16 MiB, non modifica cronologia o stati delle parole.

In una serie aperta, gli episodi con punteggio completato offrono Parole da studiare prima della riproduzione. La vista usa il conteggio dei sottotitoli già disponibile e tutti i controlli condivisi del vocabolario; cambiare stati la aggiorna subito. Riaprire le sorgenti o cambiare dizionari/impostazioni chiude gli elenchi superati.

Durante la lettura di una traccia grafica, il menu dei sottotitoli principali e la lista mostrano preparazione o immagini elaborate/totali. Annulla lettura interrompe quella richiesta e rimuove il testo parziale: le tracce incorporate tornano alle immagini, le coppie esterne vengono disattivate. La riproduzione continua. Annullare la lettura non annulla un download del modello richiesto separatamente.

Il menu principale e la lista sottotitoli offrono Esporta testo corrente come SRT quando l’intera traccia testuale è pronta, compreso l’OCR completato. Il clic acquisisce testo semplice e offset corrente. Gli inizi negativi vengono portati a zero; intervalli interamente negativi o vuoti vengono omessi. Stili e font non vengono esportati; la destinazione viene scritta atomicamente con un limite di 16 MiB.

Statistiche mostra numero di file e byte dei conteggi di analisi persistenti. Aggiorna utilizzo cache legge una nuova fotografia; Svuota cache conteggi rimuove i file riconosciuti di libri/episodi e i temporanei di scritture interrotte. Analisi correnti, cronologia, dizionari e cache sottotitoli/OCR restano disponibili. Il lavoro completato in seguito può riempire di nuovo la cache.

Libri e serie locali offrono Esporta riepiloghi analizzati per le analisi completate conservate. Il TSV acquisisce totali e percentuali correnti di parole/unità testuali, più copertura analizzati/totali/falliti delle serie. Include tutte le analisi completate di quella vista, indipendentemente dai filtri visibili; percentuali non definite e copertura dei libri restano vuote. Cambiamenti successivi agli stati non riscrivono la fotografia salvata.

Nell’assistente di configurazione seleziona **Strumenti di supporto** per installare FFmpeg/ffprobe, yt-dlp (con la dipendenza Deno) o il modello OCR locale. Gli strumenti locali funzionanti vengono riutilizzati. L’assistente sceglie il pacchetto Linux/Windows x86_64 o ARM64, verifica lo SHA-256 e salva i percorsi, subito utilizzabili. I download iniziano solo dopo **Installa**, usano il proxy configurato e possono essere interrotti. Gli strumenti risiedono nella cartella dati privata di Nikomiru e non richiedono privilegi di amministratore. Le librerie di riproduzione e grafica sono fornite dal pacchetto o dal sistema operativo.

## Strumenti di studio dei sottotitoli e vocabolario

Nella lista dei sottotitoli spunta le singole righe o usa Seleziona righe visibili.
Le spunte nascoste restano selezionate cambiando ricerca e filtri temporali,
lessicali o di velocità; una nuova versione della traccia primaria le azzera.
Gli strumenti di studio copiano trascrizioni TSV con i tempi ed esportano SRT
selezionati, WebVTT visibili/selezionati, trascrizioni bilingui o SRT bilingui.
L’uscita bilingue richiede due tracce complete: abbina le sovrapposizioni positive
considerando i due offset e conserva le righe primarie senza traduzione. Il
salvataggio usa la fotografia di testo e tempi acquisita al clic. L’OCR parziale
non può esportare un file di sottotitoli completo.

Dal/Fino al tempo corrente restringono lo studio senza spostare la riproduzione.
I limiti opzionali di lettura contano i caratteri Unicode non bianchi al secondo.
Le statistiche contano righe/caratteri e uniscono il tempo di dialogo sovrapposto
senza contarlo due volte. Riga corrispondente precedente/successiva navigano
fra le righe filtrate conservando la pausa. Ogni azzeramento rimuove solo il
proprio limite.

Le viste del vocabolario combinano ricerca esatta/parziale per parola o lettura,
stato, presenza di scheda e limiti di occorrenze. La copertura distingue parole
diverse da occorrenze; i totali selezionati includono le spunte nascoste. Il
pulsante del dizionario apre una ricerca senza associarla a una frase estranea.
Nella scheda Parole dei sottotitoli, Esempi cerca le identità lessicali segmentate,
comprese le flessioni, e mostra le righe originali. Azzera il filtro degli esempi
per tornare alla lista intera; sorgenti o analisi sostituite invalidano gli esempi.

Esporta il dizionario di frequenza visibile/selezionato per ottenere uno ZIP
Yomitan importabile, con conteggi esatti e letture distinte. L’importazione resta
esplicita. I filtri riutilizzabili memorizzano/applicano/dimenticano una sola
combinazione di ricerca e filtri fra viste nella sessione corrente: non includono
le selezioni e non sopravvivono al riavvio.

## Altri strumenti di studio (sorgenti 2.80.0)

I preset salvati conservano ricerca, corrispondenza esatta, scrittura, lunghezza della parola, stato, schede, occorrenze e ordine del vocabolario. Apri Preset di studio salvati, inserisci un nome e salva. Premi un nome per applicarlo oppure elimina il preset. Puoi salvarne sedici; le selezioni restano nella vista corrente. Attendi l’esito del salvataggio prima di chiudere. Sono ancora disponibili anche i preset della sola sessione.

Gli strumenti dei sottotitoli aggiungono limiti di durata in millisecondi. La modalità di ascolto della lista sottotitoli nasconde il testo finché riveli una riga o quelle visibili. Cambiando sorgente il testo viene nascosto nuovamente. Copia ed esportazione contengono comunque il testo originale. Maiusc-clic sulla casella seleziona o deseleziona un intervallo nell’ordine delle righe visibili; l’inversione conserva le selezioni nascoste.

Vocabolario di queste righe raccoglie le parole delle righe visibili o selezionate dopo l’analisi completa, comprese le forme flesse ricondotte al dizionario. Aggiorna stati e schede senza modificare il punteggio dei sottotitoli. Puoi tornare al vocabolario completo; cambiare sorgente o dizionari annulla la cattura. I gruppi di studio selezionano le prossime parole non ancora spuntate nell’ordine corrente: venti per impostazione iniziale, da una a mille. Deselezionare o invertire le parole visibili conserva le selezioni nascoste.

Con una traccia secondaria completa puoi esportare una scheda HTML bilingue autonoma tramite la finestra di salvataggio. Aprila localmente e rivela ogni traduzione quando desideri. L’esportazione rispetta i filtri e le righe scelte.

La ripetizione riproduce la battuta corrente due, tre o cinque volte e poi resta in pausa, sospendendo pausa automatica e accelerazione degli intervalli. Ferma ripetizione, i controlli manuali di riproduzione, posizione, tempi o velocità e il cambio della sorgente principale annullano l’esercizio. La sezione mostra le ripetizioni rimanenti.

## Filtri estesi e code di studio (sorgenti 2.90.0)

I filtri sul testo combinano limiti inclusivi di caratteri Unicode, escludendo gli spazi, battute su una o più righe e Solo la prima occorrenza. Le righe vuote non contano nella struttura. Le ripetizioni vengono riconosciute uniformando gli spazi, conservando maiuscole e punteggiatura: resta la prima battuta originale dopo gli altri filtri. Le selezioni nascoste restano spuntate. Esportazioni visibili, statistiche e navigazione condividono lo stesso ambito.

I filtri di lettura distinguono parole con una lettura separata non vuota da quelle senza. L’obiettivo di copertura conserva le parole più frequenti necessarie per raggiungere dall’1 al 100 per cento delle occorrenze attualmente filtrate. Le parole a pari frequenza sul limite sono tutte incluse: il risultato può superare la percentuale richiesta. L’ordine di visualizzazione viene applicato dopo. Entrambi i tipi di preset del vocabolario conservano queste scelte.

I preset dei sottotitoli salvano ricerca, limiti temporali, velocità di lettura, durata, caratteri, struttura, ripetizioni e contesto. Premi un nome per applicarlo oppure eliminalo esplicitamente. Puoi salvarne sedici e riutilizzarli dopo il riavvio, attendendo l’esito del salvataggio. Selezioni, testo nascosto, righe rivelate, parola degli esempi e identità della sorgente restano esclusi.

Le schede HTML del vocabolario esportano parole visibili o selezionate tramite la finestra di salvataggio. Mostra lettura rivela la pronuncia; occorrenze, stati e presenza di schede restano visibili. Sono documenti autonomi consultabili senza rete e senza risorse esterne. L’esportazione delle selezionate include anche le parole nascoste dai filtri.

Gli esempi di una parola possono mostrare una o due battute originali precedenti e successive. Le corrispondenze esatte hanno un contorno e un nome accessibile; gli altri filtri si applicano comunque al contesto ampliato. La parola cercata rimane la stessa.

Con la lista aperta, Alt+Su/Giù naviga fra le battute corrispondenti, Alt+R rivela quella corrente nella modalità di ascolto e Alt+T ne cambia la selezione. Puoi configurare i tasti nelle scorciatoie. Digitazione, pagine sovrapposte, lista chiusa o ambiti scaduti impediscono queste azioni. La navigazione conserva la pausa.

La ripetizione può acquisire una coda delle battute visibili o selezionate, fino a mille battute uniche nell’ordine della sorgente. Scegli due, tre o cinque passaggi per battuta: al termine resta in pausa. Tutti gli intervalli devono essere completi, validi e compresi nella durata del video prima dell’avvio. Sono mostrati passaggi e battute rimanenti. Cambiare i filtri non riscrive la coda acquisita; controlli manuali, cambio dei tempi o della sorgente principale e Ferma ripetizione annullano l’intera coda.

## Esercizi guidati e filtri portabili (sorgenti 2.100.0)

La ricerca può corrispondere all’intera battuta normalizzata. I filtri sul testo offrono ordine della sorgente, durata crescente/decrescente e testo più breve. L’evidenziazione mantiene la battuta corrente originale; la navigazione precedente/successiva segue ancora i tempi. Gli intervalli di selezione seguono l’ordine visualizzato, esportazioni e code di pratica quello della sorgente. Ricerca esatta e ordine vengono conservati nei preset.

Inserisci intervallo di studio accetta secondi non negativi, M:SS oppure H:MM:SS, con massimo tre cifre decimali e punto o virgola. Un campo vuoto rimuove quel limite. Applica verifica entrambi i campi e rifiuta una fine precedente o uguale all’inizio. Le bozze non modificano i filtri finché non vengono applicate. Gli strumenti scorrono internamente, lasciando spazio alle battute; copia ed esportazione hanno una sezione richiudibile.

Il pulsante di traduzione accanto alla battuta apre il testo secondario sovrapposto in una finestra di sola lettura, considerando entrambi i ritardi. L’indizio non rivela il testo principale nascosto e non cambia la riproduzione. Cambiare sorgente o ritardo chiude l’anteprima precedente. Il testo interattivo è limitato a 64 KiB.

Esercita le parole a partire dalle letture nasconde i lemmi del vocabolario anche nei nomi accessibili. Mostra una parola o tutte quelle visibili; dizionario ed esempi si abilitano dopo la rivelazione. Le letture distinte forniscono indizi. Cambiare sorgente o conteggi nasconde nuovamente le parole; aggiornare solo lo stato conserva le rivelazioni. Copia ed esportazione esplicite salvano ancora i testi originali. La modalità resta nella vista e non entra nei preset.

Annulla stato parole ripristina una modifica singola o multipla nelle viste di studio, dopo aver verificato dizionario e valori correnti attesi. Le selezioni rimangono. La cattura conserva al massimo diecimila identità e otto MiB di testo. Modifiche incompatibili successive rendono l’azione non disponibile; la cronologia degli incontri non viene riscritta.

La ripetizione offre una pausa di sessione da zero a diecimila millisecondi tra passaggi o battute. L’interfaccia resta utilizzabile e mostra l’attesa; arresto, comandi manuali e cambio sorgente annullano anche i riavvii programmati. Un’attesa già avviata conserva la scadenza acquisita quando cambi l’impostazione. Salta battuta corrente avanza subito nella coda e mette in pausa se non restano battute. Ricomincia battuta corrente ripristina il numero originale di passaggi e conserva il resto della coda, anche dopo il completamento.

Nel vocabolario dei sottotitoli, la sezione HTML/esportazione salva in TSV gli esempi delle parole visibili o selezionate. Lemma e lettura analizzati corrispondono esattamente; ogni battuta produce una riga con tempi effettivi in millisecondi e testo originale. Servono analisi complete; i limiti sono mille parole, centomila corrispondenze e sedici MiB. Richieste scadute o incomplete non scrivono un file parziale.

I pannelli dei preset importano ed esportano entrambe le raccolte dei filtri in JSON. Il file versionato, limitato a un MiB, contiene solo filtri, senza percorsi, servizi o segreti. L’importazione verifica l’intero file e unisce per nome, sostituendo i nomi corrispondenti e mantenendo gli altri; ciascuna raccolta conserva il limite di sedici voci. Filtri ignoti, dati invalidi o capacità superata non modificano la configurazione. Una configurazione in sola lettura rifiuta l’importazione; l’esito del salvataggio usa il normale sistema di configurazione.

## Ripasso finito del vocabolario (sorgenti 2.110.0)

Apri Lettura e copertura in una vista del vocabolario per avviare il ripasso delle parole mostrate o selezionate, comprese le selezioni nascoste dai filtri. La sessione acquisisce fino a mille identità uniche parola/lettura nell’ordine mostrato. I cambiamenti successivi alla fonte o ai filtri non la riscrivono. La lettura distinta funge da indizio; altrimenti un messaggio generico mantiene nascosta la parola. Rivela la risposta, poi scegli Ancora o Bene; Salta passa oltre senza valutazione. Ancora rimette la parola in coda per un massimo di tre tentativi. Le valutazioni appartengono alla sessione e non modificano gli stati globali di apprendimento.

Spazio rivela, 1 valuta Ancora e 2 valuta Bene quando il ripasso ha la priorità. Puoi modificarli in Scorciatoie → Richiamo del vocabolario. I campi di testo e le impostazioni mantengono i propri comandi; i popup del dizionario hanno la precedenza. Indietro chiude prima il popup, poi il ripasso. La ricerca nel dizionario si abilita dopo aver rivelato la risposta. Avanzare, riavviare o sostituire la sessione chiude i vecchi risultati.

Acquisisci indizi dalle frasi usa facoltativamente l’analisi completa dei sottotitoli principali correnti e copia fino a tre brevi esempi distinti con corrispondenza esatta della parola analizzata. Gli indizi si rivelano separatamente dalla risposta. Mantengono il titolo della fonte, non spostano la riproduzione e non seguono i cambiamenti successivi dei sottotitoli. I riavvii in ordine originale, inverso o casuale conservano parole e indizi, azzerando i progressi con una nuova identità di sessione.

Salva progressi del ripasso crea un file JSON con versione. Apri ripasso salvato è disponibile nella sessione e nella Libreria, anche senza un video aperto. Il file viene validato interamente ricostruendo le valutazioni prima di sostituire la sessione; un file non valido lascia intatti i progressi aperti. Alla ripresa risposte e indizi sono nascosti. Il file contiene parole, ordine, indizi e valutazioni, escludendo gli stati globali e i percorsi dei video. Esporta risultati del ripasso salva un’istantanea TSV indipendente con una riga per parola, tentativi, voti Ancora, ultima valutazione e completamento.

Al termine puoi avviare una nuova sessione delle parole valutate Ancora, comprese quelle poi valutate Bene, oppure delle parole saltate. Le categorie vuote sono disabilitate. Registra questo riepilogo aggiunge esplicitamente conteggi e titolo della fonte alla cronologia locale, una volta per identità di sessione. La cronologia conserva gli ultimi cento riepiloghi dopo il riavvio e consente rimozione, cancellazione e un nuovo tentativo in caso di errore di salvataggio. Non contiene le parole acquisite e non aumenta le statistiche del tempo di studio. Le sessioni attive si riprendono aprendo il relativo JSON salvato.

## TXT, Markdown e testo incollato (sviluppo M102)

Libri → Aggiungi accetta file UTF-8 `.txt`, `.md` e `.markdown`, fino a quattro MiB. Il titolo deriva dal nome del file. Il testo semplice conserva letteralmente parentesi angolari ed entità, con ogni riga non vuota come paragrafo; sono gestiti i fine riga Windows e il BOM UTF-8 facoltativo. Markdown usa la struttura CommonMark per titoli, paragrafi, elenchi, citazioni, collegamenti e codice. Collegamenti e immagini non si aprono e non vengono scaricati. I furigana inline sono conservati; l’altro HTML resta testo visibile inerte. Stili avanzati, tabelle e immagini Markdown non vengono renderizzati.

Libri → Incolla testo apre l’editor. Inserisci un titolo di massimo 256 caratteri, incolla il testo, scegli eventualmente Markdown e premi Leggi testo. Il documento viene conservato localmente e può essere riaperto dalla lista dopo il riavvio. La bozza rimane disponibile se il salvataggio fallisce. Titolo, formato e contenuto identici riusano la stessa fonte; contenuti diversi mantengono posizioni indipendenti anche con titolo e lunghezza uguali. Spostare un file senza cambiarne nome e contenuto conserva la sua identità.

Dizionario, stati delle parole, analisi del vocabolario e creazione delle carte usano lo stesso lettore degli EPUB. I documenti sono limitati a 8.192 paragrafi non vuoti e 64 KiB per paragrafo. L’analisi completa mantiene i suoi limiti più restrittivi. I testi incollati sono inclusi nei backup locali; i contenuti delle fonti non vengono caricati dalla sincronizzazione WebDAV. Rimuovere un libro lascia il file della fonte, come per gli altri libri.

## Manga Mokuro (sviluppo M103)

Libri → Aggiungi accetta file JSON `.mokuro` 0.2.x con immagini locali in una cartella omonima accanto al file, oppure accanto al manifest se la cartella non esiste. Seleziona un riquadro nella pagina: il testo originale diventa cliccabile e il ritaglio JPEG esatto è disponibile per le carte, senza OCR o installazione del modello. Ordine e posizione delle pagine sono conservati. Immagini mancanti, percorsi non sicuri e coordinate non valide rifiutano l’importazione prima di sostituire un libro. Piccoli sconfinamenti dei riquadri OCR vengono ritagliati. I lettori HTML eseguibili precedenti e gli URL di immagini remote non vengono usati.
