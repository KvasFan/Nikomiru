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

## 5. Carte Anki

1. Installa in Anki il componente aggiuntivo [AnkiConnect](https://ankiweb.net/shared/info/2055492159) (codice `2055492159`) e tieni Anki aperto.
2. In Nikomiru, Impostazioni → Anki: **Prova la connessione**, poi **Crea mazzo Lapis**. Se preferisci, scegli un tuo mazzo e un tuo tipo di nota e associa i campi.
3. In un popup, **+** su una voce (o Ctrl+Invio) crea la carta. Contiene:
   - la parola e i suoi significati;
   - la frase con la parola in grassetto;
   - il fotogramma (oppure, in Impostazioni → Anki → Formato immagine, tutta la battuta come immagine animata) e l'audio della battuta;
   - un link che riapre la scena.
4. **✓** vuol dire che Anki ha già la parola: dal suo menu apri la carta in Anki, oppure ne aggiungi un'altra comunque.
5. **Shift+clic su +** (o Ctrl+Shift+Invio) crea la carta a mano. Puoi:
   - aggiungere le righe prima e dopo;
   - spostare la clip e ascoltarla;
   - scegliere l'immagine, oppure togliere l'audio.

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
- Nella pagina di una serie puoi rinominarla, unirla a un'altra, spostare o togliere episodi, o eliminarla. I tuoi file non vengono mai toccati.
- **Identifica online** chiede ad AniList (o a MyAnimeList) titoli, episodi e copertina della serie. Se più serie corrispondono, scegli tu. Jimaku poi trova i sottotitoli dal suo id AniList.
- **Statistiche** mostra il tempo guardato e i caratteri letti:
  - per giorno, per serie e per sessione;
  - un calendario dell'anno;
  - un'esportazione CSV.

### Jellyfin e Plex

Aggiungi un server in Impostazioni → Server multimediali: per Jellyfin indirizzo, utente e password (la password non viene conservata); per Plex accedi su plex.tv dal browser. Ogni server ha una scheda nella pagina della libreria. I video partono dal file originale, con gli stessi sottotitoli, ricerche e carte di un file locale, e posizione e stato di visione tornano al server. Nikomiru si collega a un server solo mentre la sua scheda è aperta o uno dei suoi video è in riproduzione.

## 8. Impostazioni

⚙ nella barra, oppure Ctrl+, apre le impostazioni. Le schede sono in colonna a sinistra.

| Scheda | Cosa contiene |
|---|---|
| Generale | Lingua, tema (chiaro, scuro o come il desktop), colore d'accento, scala dell'interfaccia, passo dei salti, episodio successivo |
| Player | Le opzioni di mpv come controlli, preset `mpv.conf`, importazione ed esportazione |
| Sottotitoli | Dimensione, lingue preferite, sfocatura o nascondere finché non ci passi sopra, il sottotitolo secondario, le letture sopra i kanji, le parole minate sottolineate |
| Ricerca | Risultati in un popup, dimensione del popup, passaggio del puntatore |
| Scorciatoie | Tutti i tasti, che puoi cambiare. I tasti sono fisici: restano al loro posto con qualunque disposizione della tastiera |
| Avanzate | Il programma ffmpeg, dove sono il file delle impostazioni e i log |

Ogni scheda ha **Salva** e **Ripristina**. Puoi modificare anche `~/.config/nikomiru/config.toml` a mano: Nikomiru conserva i tuoi commenti.

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
| Ctrl+O | Apri un file |
| Ctrl+, | Impostazioni |
| Esc | Indietro |

## 10. Se qualcosa non va

- I log sono in `~/.local/state/nikomiru/logs/`. Non contengono mai le tue chiavi.
- Se le impostazioni non si salvano, controlla che `~/.config/nikomiru` sia tua. Se è stata creata con `sudo`, riprendila con `sudo chown -R $USER: ~/.config/nikomiru`.
- Se un'impostazione in `config.toml` ha un valore del tipo sbagliato, Nikomiru usa quello predefinito e la nomina in un avviso all'avvio.
- Problemi e idee: <https://github.com/KvasFan/nikomiru/issues>.
