# Anikoto — Roadmap

| Campo | Valore |
|---|---|
| Stato | Bozza v0.1 |
| Data | 2026-10-19 |
| Documenti collegati | [SPECIFICHE.md](SPECIFICHE.md) · [TEST.md](TEST.md) |

La roadmap divide il lavoro in **milestone** (M0…M8). Ognuna ha un obiettivo, i requisiti che copre (ID di [SPECIFICHE.md](SPECIFICHE.md)), i deliverable e i **criteri di uscita** verificabili (ID di [TEST.md](TEST.md)).

Non ci sono date: una milestone è chiusa quando tutti i criteri di uscita passano, non quando scade un termine. Le dimensioni (S/M/L/XL) danno solo un'idea dello sforzo relativo.

Stato possibile di una milestone: `da iniziare` · `in corso` · `completata`. Quando una milestone diventa `completata`, `cargo xtask trace` impone che ogni suo requisito abbia almeno un test (vedi TEST §8).

## Panoramica

```
M0 Fondamenta + spike ──► M1 Player ──► M2 Sottotitoli ──► M4 Mining Anki ══► 0.1.0 (MVP)
        │                                   ▲                   ▲
        └──► M3 Dizionari e lookup ─────────┴───────────────────┘
                (logica pura: può procedere in parallelo a M1/M2)

0.1.0 ──► M5 Libreria + audio parole (0.2.0) ──► M6 Statistiche, impostazioni, i18n (0.3.0)
      ──► M7 Online: metadati e sottotitoli (0.4.0) ──► M8 Stabilizzazione ══► 1.0.0
```

| Milestone | Titolo | Dim. | Release | Stato |
|---|---|---|---|---|
| M0 | Fondamenta e spike | M | — | da iniziare |
| M1 | Player minimo | M | — | da iniziare |
| M2 | Sottotitoli propri | L | — | da iniziare |
| M3 | Dizionari e lookup | XL | — | da iniziare |
| M4 | Mining verso Anki | L | **0.1.0** (MVP alpha) | da iniziare |
| M5 | Libreria e audio delle parole | L | 0.2.0 | da iniziare |
| M6 | Statistiche, impostazioni, i18n, mining avanzato | M | 0.3.0 | da iniziare |
| M7 | Metadati e sottotitoli online | M | 0.4.0 | da iniziare |
| M8 | Stabilizzazione e pacchetti | M | **1.0.0** | da iniziare |

---

## M0 — Fondamenta e spike

**Obiettivo**: preparare il workspace e la CI, e **eliminare i due rischi tecnici maggiori prima di scrivere funzioni**.

**Requisiti**: ARC-010, ARC-011, NFR-MAINT-01; politica di licenze (SPECIFICHE §3, §3.5).

**Deliverable**

1. Workspace come in SPECIFICHE §5.1: crate vuoti con `lib.rs`, intestazioni SPDX `GPL-3.0-only`, `#![forbid(unsafe_code)]` dove richiesto.
2. `rust-toolchain.toml`, `[workspace.dependencies]` centralizzate, `Cargo.lock` versionato, `clippy.toml` (`unwrap_used`, `expect_used` come errore nelle librerie).
3. `deny.toml` con classe A, eccezioni di classe B per i soli crate elencati in SPECIFICHE §3.2, `bans` (niente versioni duplicate di crate pesanti senza motivo) e `sources` (solo crates.io).
4. `about.toml` e generazione di `THIRD_PARTY_LICENSES.html`.
5. `xtask` con i comandi:
   - `ci`: fmt, clippy, deny, test;
   - `fixtures`: genera i media e i dizionari di test (TEST §4);
   - `check-headers`: verifica le intestazioni SPDX;
   - `trace`: matrice requisiti ↔ test;
   - `about`.
6. Pipeline CI (servizio da scegliere quando si pubblica il repository) che chiama solo `cargo xtask ci` e `cargo xtask ci --full`.
7. **Spike S1 — composizione video** (SPECIFICHE §5.2): finestra Dioxus con GtkOverlay(GLArea mpv + webview trasparente), un pulsante HTML sopra il video, un sottotitolo HTML finto che si aggiorna a ogni `time-pos`.
8. **Spike S2 — storage dei dizionari**: import di un dizionario sintetico da 300k voci in redb e lookup con un deinflettore minimo.
9. Per ogni spike, un ADR in `docs/adr/` (`0001-composizione-video.md`, `0002-storage-dizionari.md`) con misure, decisione e ripiego scelto. Il codice degli spike non entra in `crates/`: si riscrive pulito nelle milestone successive.

**Criteri di uscita**

- [ ] `cargo xtask ci` verde sul workspace vuoto (T-LIC-001, T-ARC-010, T-ARC-011, T-ARC-020).
- [ ] S1 rispetta **tutti** questi punti su RIF, sia con Wayland (GNOME o KDE) sia con X11:
  - 1080p60 H.264 e HEVC 10-bit per 5 minuti con hwdec, 0 fotogrammi persi e CPU ≤ 25% di un core (T-PERF-03);
  - clic e tasti arrivano alla webview sopra il video;
  - ridimensionamento, schermo intero e HiDPI 2× senza artefatti;
  - 50 aperture e chiusure del player senza crescita della RSS oltre il 10%.

  Se S1 fallisce, l'ADR sceglie un ripiego (B/C/D di SPECIFICHE §5.2.3) e si aggiorna questa roadmap prima di passare a M1.
- [ ] S2: import ≤ 60 s e ≤ 1 GiB di RAM (T-PERF-05); lookup p95 ≤ 10 ms (T-PERF-04). Se fallisce, l'ADR adotta il formato su misura (DIC-020).
- [x] D-1 (deroghe di classe B) e D-3 (GPL-3.0-only) approvate dal maintainer (2026-10-19).

---

## M1 — Player minimo

**Obiettivo**: aprire un file e guardarlo con comandi completi da tastiera e mouse.

**Requisiti**: LIB-001, LIB-006 (salvataggio della posizione per singolo file), PLY-001…PLY-010, PLY-012, PLY-013, ARC-001…ARC-004, KEY-001…KEY-004 (scorciatoie M1 di SPECIFICHE §6.3), APP-001, EXT-001, ERR-001, ERR-002, PRV-001, PRV-002, DAT-001…DAT-003.

**Deliverable**

- `anikoto-player`: `PlayerHandle`, `PlayerEvent`, opzioni mpv (PLY-005/006), ripresa con `start` (PLY-007), riuso tra un file e l'altro (PLY-009), parser delle opzioni mpv grezze.
- `anikoto-app`: compositore Linux definitivo (dall'ADR 0001), istanza singola via socket, apertura da argomento o drag & drop.
- `anikoto-ui`: pagina player con barra dei controlli (avanzamento, tempo, volume, velocità, tracce audio, schermo intero) e toast.
- `config.toml` minimo, con scrittura atomica.
- `library.redb` con la sola tabella `videos`.

**Criteri di uscita**

- [ ] T-PLY-001…T-PLY-013, T-KEY-001…T-KEY-004, T-APP-001 verdi.
- [ ] GUI harness: apre una fixture, play/pausa/seek da tastiera, chiude e riapre e riprende dalla posizione salvata (T-E2E-001).
- [ ] Checklist manuale TEST §9 sezione "Player" completata su Wayland e X11.

---

## M2 — Sottotitoli propri

**Obiettivo**: sottotitoli testuali disegnati da Anikoto, cliccabili per grafema, sincronizzati e navigabili.

**Requisiti**: SUB-001…SUB-016, EXT-002, EXT-003, scorciatoie M2.

**Deliverable**

- `anikoto-subs`:
  - parser SRT, VTT e ASS di base;
  - modello `Cue { start_ms, end_ms, plain_text, markup }`;
  - indice per ricerca binaria;
  - offset;
  - rilevamento della codifica tramite iconv (FFI in `anikoto-app::linux` o in un piccolo modulo `unsafe` dedicato, ARC-011).
- `anikoto-media`: runner ffmpeg e ffprobe con timeout e cancellazione (EXT-002/003); estrazione delle tracce incorporate con cache.
- `anikoto-ui`:
  - overlay a grafemi;
  - stile predefinito e "rispetta ASS";
  - blur e nascondi;
  - pannello lista con ricerca;
  - navigazione prev/next/replay;
  - Z/X e allineamento al playhead.

**Criteri di uscita**

- [ ] T-SUB-001…T-SUB-016 verdi, compresi i vettori di conformità con Fushi (TEST §5) per i parser.
- [ ] T-FUZZ-001…T-FUZZ-003 (parser) senza panic per 10 minuti ciascuno.
- [ ] T-PERF-07: cue visibile entro ≤ 50 ms dal suo start.
- [ ] Checklist manuale "Sottotitoli" con almeno 3 file reali: SRT Shift_JIS, ASS di fansub con stili, MKV con 2 tracce incorporate.

---

## M3 — Dizionari e lookup

**Obiettivo**: importare dizionari Yomitan e fare lookup con qualità pari a Fushi, popup compreso.

**Requisiti**: DIC-001…DIC-003, DIC-010…DIC-016, DIC-020, DIC-021, DIC-030…DIC-035, LKP-001…LKP-050, LKP-060, POP-001…POP-015, NFR-SEC-01.

**Tracce di lavoro** (`anikoto-text` e `anikoto-dict` non dipendono dalla UI e possono iniziare in parallelo a M1):

1. `anikoto-text`: preprocessori (LKP-010…012), conversione kana, segni di iterazione, varianti dei kanji (tabella da Fushi), estrazione della frase, furigana (LKP-050), conteggio dei caratteri (STA-003, serve in M6).
2. `anikoto-dict`:
   - import Yomitan sicuro (DIC-011), atomico (DIC-013) e annullabile;
   - `DictStore` su redb;
   - deinflettore sulle tabelle `assets/transforms`;
   - scansione, unione, ordinamento e raggruppamento;
   - frequenze, accento, kanji;
   - cache LRU.
3. `anikoto-ui`:
   - gestione dei dizionari;
   - popup (contenuto strutturato con lista di tag ammessi, CSS con `@scope`, immagini via `anikoto-dict://`);
   - popup annidati;
   - macchina a stati pausa/ripresa (POP-001/004/005);
   - hover con Shift.

**Criteri di uscita**

- [ ] T-DIC-*, T-LKP-*, T-POP-* verdi, compresi **tutti** i vettori di conformità portati dai test di fushidicts (TEST §5.1).
- [ ] T-PERF-04, T-PERF-05, T-PERF-06 entro le soglie.
- [ ] T-SEC-001 (nessuno script eseguito dal contenuto dei dizionari) e T-SEC-002 (ZIP malevoli) verdi.
- [ ] Prova manuale con JMdict (inglese), JMnedict, un dizionario di frequenza (es. JPDB) e uno di accento (es. NHK) importati dall'utente: le 50 parole della lista di TEST §9.3 danno lo stesso headword in Anikoto e in Fushi.

---

## M4 — Mining verso Anki → release 0.1.0 (MVP)

**Obiettivo**: dal popup, una carta Lapis completa in un clic.

**Requisiti**: ANK-001…ANK-006, ANK-010…ANK-013, ANK-020…ANK-027, ANK-030…ANK-039, ANK-050…ANK-053, APP-002, PLY-011 (screenshot semplice), scorciatoia Ctrl+Enter.

**Deliverable**

- `anikoto-anki`: client AnkiConnect (retry e duplicati come da specifica), composer dei campi, template Lapis vendorizzati (`assets/anki/lapis/` con `PROVENANCE.md`), link `anikoto://`.
- `anikoto-media`: audio della frase e immagine (screenshot mpv → ffmpeg).
- UI: impostazioni Anki (connessione, mazzo, tipo di nota, editor del mapping, "Crea mazzo Lapis", test di connessione), stati del pulsante + / ✓ / ✓↩, menu della carta esistente.
- `.desktop` con `x-scheme-handler/anikoto`.

**Criteri di uscita**

- [ ] T-ANK-* verdi su mock (wiremock).
- [ ] T-E2E-ANKI-001 (Anki reale in Xvfb con AnkiConnect, eseguito di notte o a mano) verde.
- [ ] T-PERF-08 ≤ 3 s.
- [ ] Checklist manuale "Mining": 10 carte create da 2 episodi reali, controllate in Anki desktop e dopo la sincronizzazione su AnkiDroid (audio, immagine, grassetto nella frase, link di ritorno che riapre la scena).
- [ ] **Release 0.1.0**: tag, CHANGELOG, `THIRD_PARTY_LICENSES.html`, istruzioni di build.

---

## M5 — Libreria e audio delle parole → 0.2.0

**Requisiti**: LIB-002…LIB-005, LIB-007…LIB-012, AUD-001…AUD-008, spike **S3** (decisione D-2).

**Deliverable**

- `anikoto-library`: import di cartelle, parser dei nomi file (porting di `video_filename_parser.dart` con i suoi vettori), serie ed episodi, tombstone, preferenze per serie, avanzamento automatico.
- Home: "Continua a guardare", griglia delle serie, dettaglio della serie, copertine generate.
- Audio delle parole: template URL, risposta JSON Yomitan o audio diretto, seconda istanza libmpv solo audio, cache LRU.
- **Spike S3**: lettura read-only di un `android.db` reale con `turso` (MIT). Criteri: query (expression, reading) p95 ≤ 20 ms; nessuna licenza fuori classe A/B nel grafo. L'ADR `0003-db-audio-locale.md` conferma D-2, oppure la riapre se i criteri non sono rispettati.

**Criteri di uscita**

- [ ] T-LIB-*, T-AUD-* verdi.
- [ ] Import di una libreria reale di ≥ 200 episodi in ≤ 30 s (senza metadati online), con i raggruppamenti corretti controllati a mano.
- [ ] **Release 0.2.0**.

---

## M6 — Statistiche, impostazioni complete, i18n, mining avanzato → 0.3.0

**Requisiti**: STA-001…STA-005, SET-001…SET-007, ANK-040, ANK-041, NFR-A11Y-01.

**Deliverable**

- Tracker della visione (copertura per intervalli, crediti per cue), orologio delle sessioni, pagina statistiche, export CSV.
- Pagina impostazioni completa, editor delle scorciatoie con controllo dei conflitti, temi chiaro/scuro/sistema, scala dell'interfaccia.
- Fluent con `it` ed `en` completi.
- Dialogo di mining avanzato; frasi multiple.

**Criteri di uscita**

- [ ] T-STA-*, T-SET-*, T-I18N-*, T-A11Y-* verdi.
- [ ] Giro completo dell'app con la sola tastiera (T-E2E-A11Y-001).
- [ ] **Release 0.3.0**.

---

## M7 — Metadati e sottotitoli online → 0.4.0

**Requisiti**: MET-001…MET-005, SUB-017, SUB-018.

**Deliverable**

- Crate `anikoto-online`: client AniList (GraphQL), Jikan con limite di frequenza, Jimaku, OpenSubtitles. Tutti dietro un trait e con test su server finto.
- UI: identificazione della serie (automatica se univoca, altrimenti scelta dell'utente), ricerca e download di sottotitoli per episodio.
- Sottotitolo secondario; allineamento automatico con la forma d'onda.

**Criteri di uscita**

- [ ] T-MET-* verdi su server finti; T-MET-LIVE-* (rete reale, solo di notte e con chiavi) verdi.
- [ ] Allineamento: su 5 coppie reali (sottotitolo scaricato + traccia incorporata) l'errore residuo è ≤ 150 ms.
- [ ] **Release 0.4.0**.

---

## M8 — Stabilizzazione e pacchetti → 1.0.0

**Requisiti**: tutti gli NFR non assegnati ad altre milestone (NFR-PERF-*, NFR-REL-*, NFR-SEC-02, NFR-PORT-01, NFR-MAINT-02), D-5.

**Deliverable**

- Pacchetti: PKGBUILD (AUR) e Flatpak (manifest con libmpv e ffmpeg, runtime GNOME), come deciso in D-5.
- Guida utente in `docs/guida/` (italiano e inglese): installazione, primo avvio, import dei dizionari, configurazione di Anki con AnkiConnect (codice 2055492159).
- Revisione di sicurezza (TEST §6.7) e dell'accessibilità.
- Ottimizzazione finché tutti i T-PERF passano su RIF.
- Pulizia del backlog dei bug P1/P2.

**Criteri di uscita**

- [ ] Tutti i test automatici verdi; nessun bug aperto di priorità P1.
- [ ] Checklist manuale di rilascio TEST §9 completa su: GNOME Wayland, KDE Wayland, X11 (Xfce o i3), GPU Intel, AMD e NVIDIA (driver proprietario).
- [ ] `THIRD_PARTY_LICENSES.html` incluso nei pacchetti e visibile in "Informazioni".
- [ ] **Release 1.0.0**.

---

## Dopo la 1.0 (backlog, senza ordine vincolante)

Ogni voce richiede una modifica di SPECIFICHE.md prima di iniziare.

| Voce | Note su licenze e dipendenze |
|---|---|
| Porting Windows e macOS (SPECIFICHE §5.2.4) | WebView2 / WKWebView; libmpv e ffmpeg da distribuire (LGPL/GPL, obblighi sul sorgente) |
| Clip video WebM e copertine animate (ANK-042) | Encoder libvpx (BSD) e SVT-AV1 (BSD) **dentro ffmpeg di sistema**: sono processi esterni, non crate |
| ASS avanzato (SUB-019), screenshot con sottotitoli (PLY-015), miniature sulla barra (PLY-014) | |
| Furigana automatico sui sottotitoli (LKP-051), evidenziazione delle parole già minate | Usa LKP-060 e `mined_cards` |
| Lookup dagli appunti e texthooker WebSocket (visual novel con Textractor/LunaTranslator esterni) | `tokio-tungstenite` (MIT) |
| Import MDict, StarDict, DSL (DIC-004) | `ammonia` (MIT/Apache) per l'HTML |
| Lettore EPUB (con il formato di sync ttu) e audiolibri | Grande; da rivalutare come progetto a parte |
| SRS integrato | `fsrs` è BSD-3-Clause: o una deroga, o un'implementazione propria di FSRS in GPL dall'algoritmo pubblicato |
| Backup/ripristino e sync WebDAV | |
| Navigazione di Jellyfin e Plex | |
| Identificazione AniDB ed2k (MET-006) | Registrare un client AniDB; `md4`/`ed2k` (MIT/Apache) |
| Chiavi API nel keyring (MET-005) | `oo7` o `secret-service` (verificare le licenze) |
| Interfaccia in giapponese | Fluent `ja` |

---

## Definizione di "fatto" (vale per ogni PR)

1. I requisiti toccati sono citati nel titolo o nella descrizione della PR.
2. Ogni requisito implementato ha almeno un test marcato con il suo ID (`// T-XXX-NNN`).
3. `cargo xtask ci` è verde (fmt, clippy `-D warnings`, deny, test, intestazioni, trace).
4. Il codice portato da Fushi o da altri progetti GPL ha l'intestazione di provenienza (SPECIFICHE §2.2).
5. Le stringhe visibili sono in Fluent, per `it` ed `en`.
6. Ogni bug corretto ha un test di regressione `AK-BUG-NNN` (TEST §7).
7. CHANGELOG aggiornato nella sezione `[Unreleased]`.
8. Se cambia un comportamento specificato, SPECIFICHE.md è aggiornato **nella stessa PR**.

## Versioni

- SemVer. Fino alla 1.0, `0.MINOR.PATCH`: MINOR per ogni milestone, PATCH per le correzioni.
- Lo schema di `library.redb` si può cambiare solo con una migrazione (DAT-001), anche prima della 1.0.
- CHANGELOG nel formato *Keep a Changelog*.
