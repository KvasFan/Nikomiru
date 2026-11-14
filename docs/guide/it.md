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
