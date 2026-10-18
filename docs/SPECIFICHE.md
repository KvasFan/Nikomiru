# Anikoto — Specifiche

| Campo | Valore |
|---|---|
| Stato | v0.2 — decisioni D-1…D-6 approvate il 2026-10-19 (§10.1) |
| Data | 2026-10-19 |
| Riferimento | Fushi, commit `ac8034e8bbc25e877e6d419334a38951bc477105` (2026-10-18), <https://github.com/hajisensai/Fushi> |
| Documenti collegati | [ROADMAP.md](ROADMAP.md) · [TEST.md](TEST.md) |

## 0. Convenzioni

- **DEVE / NON DEVE**: requisito obbligatorio. **DOVREBBE**: raccomandato; per derogare serve una motivazione scritta nella PR. **PUÒ**: facoltativo.
- Ogni requisito ha un identificativo stabile (`PLY-012`). Gli ID non si riusano: un requisito eliminato resta nella tabella, barrato e con la motivazione.
- I test che verificano un requisito usano lo stesso prefisso (`T-PLY-012`, vedi [TEST.md](TEST.md)).
- "Fushi:" seguito da un percorso indica il file di Fushi da consultare durante il porting. I percorsi partono dalla radice del repository Fushi.
- Il **milestone** di ogni requisito (M0…M8, oppure "post-1.0") è definito in [ROADMAP.md](ROADMAP.md).

---

## 1. Scopo

### 1.1 Cos'è Anikoto

Anikoto è un player video desktop per **imparare le lingue guardando anime e serie** (*immersion learning*). Mentre guardi, puoi cliccare una parola nei sottotitoli: Anikoto la cerca nei dizionari che hai importato (formato Yomitan) e, con un clic, crea una carta Anki con la parola, la frase, uno screenshot e l'audio della battuta.

Anikoto è una **riscrittura in Rust di un sottoinsieme di Fushi**. Fushi è un'app Flutter/Dart GPL-3.0 che copre lettura EPUB, manga, audiolibri, video, visual novel e molto altro. Anikoto ne prende il nucleo "video + dizionario + Anki" e lo reimplementa con:

- **UI**: [Dioxus](https://github.com/DioxusLabs/dioxus) 0.7 (desktop, webview WebKitGTK su Linux);
- **riproduzione**: [libmpv2](https://crates.io/crates/libmpv2) 6.0 (binding Rust di libmpv, API di rendering OpenGL);
- **licenze**: solo GNU (GPL/LGPL), Apache-2.0 e MIT, con le eccezioni obbligate documentate al §3.

### 1.2 Obiettivi

1. Riprodurre file video locali con la qualità di mpv: decodifica hardware, tutti i codec e i container supportati da mpv.
2. Mostrare i sottotitoli testuali in un overlay **cliccabile carattere per carattere**, sopra il video.
3. Fare lookup nei dizionari Yomitan con deinflessione, frequenze e accento tonale, con qualità almeno pari a Fushi.
4. Creare carte Anki via AnkiConnect (nota Lapis pronta all'uso) con frase, screenshot, audio della frase e link di ritorno alla scena.
5. Gestire una libreria di serie ed episodi, con progressi e statistiche di visione.
6. Funzionare offline. La rete si usa solo per funzioni chieste dall'utente (metadati, sottotitoli online, audio delle parole). Nessuna telemetria.

### 1.3 Non-obiettivi (fuori ambito fino a nuova decisione)

| Funzione di Fushi | Decisione | Motivo |
|---|---|---|
| Lettore EPUB, manga + OCR, PDF, audiolibri (Sasayaki) | post-1.0, da rivalutare | Non sono video; ognuno è grande quanto l'intero MVP |
| Hook di testo e voce per galgame (Windows) | escluso | Solo Windows; dipende da LunaHook e MinHook |
| Torrent, estensioni Mihon/Aniyomi, LNReader, centro download | escluso | Fuori dallo scopo; runtime Kotlin/APK |
| AI/LLM, ASR (generazione sottotitoli), TTS, modelli ONNX | escluso | Alcuni modelli hanno licenze CC BY-NC; peso enorme |
| Google Lens OCR, decifrazione AACS Blu-ray | escluso | Rischi legali e di termini di servizio |
| Sync cloud, Interconnect, P2P iroh, server headless, CLI | post-1.0 (solo backup/sync WebDAV) | Complessità; non serve all'MVP |
| SRS integrato (FSRS) | post-1.0 | `fsrs` è BSD-3-Clause: servirebbe una deroga (§3) |
| Classifica, feedback in-app, telemetria di utilizzo | escluso | Privacy; nessun servizio da gestire |
| Android, iOS | escluso | Dioxus mobile + libmpv non sono un obiettivo |
| Windows, macOS | post-1.0 | Linux prima; l'architettura non deve precluderli (§5.2.4) |

### 1.4 Piattaforme

- **Primaria**: Linux x86_64, sia **Wayland** sia **X11**, con GTK 3.24 e WebKitGTK 4.1 (≥ 2.44) installati.
- **libmpv**: API ≥ 2.2 (mpv ≥ 0.37). Riferimento di sviluppo: mpv 0.41 (API 2.5).
- **FFmpeg**: ≥ 6.0 come eseguibile di sistema (`ffmpeg`, `ffprobe`). Riferimento di sviluppo: 9.0.
- Secondarie (post-1.0): Windows 10+, macOS 13+ su Apple Silicon.

### 1.5 Lingue

- Lingua di studio primaria: **giapponese**. Tutti gli algoritmi DEVONO comunque essere guidati da dati (tabelle di trasformazione Yomitan e preprocessori per lingua), in modo che altre lingue si aggiungano senza riscrivere codice.
- Lingue dell'interfaccia: **italiano** e **inglese** alla 1.0. Giapponese DOVREBBE seguire.

---

## 2. Rapporto con Fushi

### 2.1 Fushi in numeri (al commit di riferimento)

- App: circa 1.860 file Dart, circa 900k righe in `fushi/lib`; pacchetti a parte (`fushi_engine` da solo: circa 182k righe).
- Test: circa 4.270 file `*_test.dart`, circa 1M di righe. Circa 930 sono "guard test" che verificano regole architetturali leggendo il sorgente.
- Database Drift/SQLite: 98 tabelle, schema v118.
- Motore dizionari nativo in C++23 (`native/fushidicts`, fork di hoshidicts).
- 17 lingue di interfaccia, circa 8.400 chiavi.

**Conseguenza**: Anikoto non è un port 1:1. È un port **selettivo e guidato dai requisiti** di questo documento. Fushi è l'implementazione di riferimento per comportamenti, algoritmi, casi limite e bug noti.

### 2.2 Regole di porting

1. Prima di implementare un requisito che cita un file Fushi, si legge quel file e i suoi test. Ogni bug Fushi citato (`BUG-xxxx`) diventa un test di regressione in Anikoto.
2. Codice tradotto da Fushi, Yomitan, hoshidicts, Lapis o Tsubasa è un'**opera derivata GPL-3.0**. Il modulo Rust che lo contiene DEVE avere un commento iniziale che indichi il progetto e i file d'origine, con il commit.
3. Si portano **i comportamenti, non le strutture**. Non si replicano i workaround legati a Flutter, Dart o Windows (isolate, FFI, IME di Windows).
4. Dove Fushi ha lacune note (vedi §6.6.8), Anikoto PUÒ migliorare, ma DEVE documentare la differenza qui e coprirla con un test.

### 2.3 Mappa dei moduli

| Area Fushi | Riferimento | Crate Anikoto | Milestone |
|---|---|---|---|
| Player video, config mpv | `fushi/lib/src/media/video/video_player_controller.dart`, `video_mpv_config.dart` | `anikoto-player` | M1 |
| Sottotitoli (parser, markup, overlay) | `packages/fushi_audio/lib/src/parsers/`, `fushi/lib/src/media/video/video_subtitle_overlay.dart` | `anikoto-subs`, `anikoto-ui` | M2 |
| Motore dizionari | `native/fushidicts/fushidicts_src/` | `anikoto-dict` | M3 |
| Preprocessori e kana | `native/fushidicts/fushidicts_src/text_processor/` | `anikoto-text` | M3 |
| Popup di lookup | `fushi/assets/popup/popup.js`, `selection.js` | `anikoto-ui` | M3 |
| Anki / Lapis / composer | `packages/fushi_anki/lib/src/` | `anikoto-anki` | M4 |
| Mining video (ffmpeg) | `fushi/lib/src/mining/immersion_mining_engine.dart`, `packages/fushi_engine/lib/utils/misc/desktop_audio_clipper.dart` | `anikoto-media` | M4 |
| Libreria, parser nomi file | `packages/fushi_engine/lib/media/video/video_library_import.dart`, `video_filename_parser.dart` | `anikoto-library` | M5 |
| Audio delle parole | `fushi/lib/src/utils/misc/word_audio_resolver.dart`, `local_audio_db.dart` | `anikoto-dict` (modulo `audio`) | M5 |
| Statistiche | `fushi/lib/src/media/video/video_watch_tracker.dart`, `fushi/lib/src/stats/` | `anikoto-library` (modulo `stats`) | M6 |
| Scorciatoie | `fushi/lib/src/shortcuts/shortcut_defaults.dart` | `anikoto-ui` | M1–M6 |
| Sottotitoli online, metadati | `packages/fushi_engine/lib/media/video/jimaku_client.dart`, `metadata/` | `anikoto-online` | M7 |

---

## 3. Licenze

### 3.1 Licenza di Anikoto

Anikoto è distribuito sotto **GPL-3.0-only** (file `LICENSE`).

- È obbligatorio: Anikoto deriva da Fushi (GPL-3.0), dalle tabelle Yomitan (GPL-3.0), da hoshidicts (GPL-3.0) e dai template Lapis (GPL-3.0).
- Si usa "only" e non "or-later" perché Fushi non concede esplicitamente le versioni successive. Si potrà passare a "or-later" solo se tutte le fonti derivate lo permettono.
- Ogni file sorgente DEVE iniziare con l'intestazione SPDX `// SPDX-License-Identifier: GPL-3.0-only`.

### 3.2 Politica per le dipendenze Rust

**Classe A — sempre ammesse** (dipendenze dirette e transitive):

| Licenza SPDX | Note |
|---|---|
| `MIT` | |
| `Apache-2.0`, `Apache-2.0 WITH LLVM-exception` | Compatibile con GPL-3.0 |
| `GPL-2.0-or-later`, `GPL-3.0-only`, `GPL-3.0-or-later` | `GPL-2.0-only` **non** è ammessa: è incompatibile con GPL-3.0 |
| `LGPL-2.1-only`, `LGPL-2.1-or-later`, `LGPL-3.0-only`, `LGPL-3.0-or-later` | Es. `libmpv2` (LGPL-2.1) |

Le espressioni con `OR` (es. `MIT OR Apache-2.0`, `Unlicense OR MIT`, `MIT OR Zlib OR Apache-2.0`) sono ammesse se almeno un ramo è di classe A: Anikoto sceglie quel ramo.

**Classe B — deroghe obbligate, ammesse solo per i crate elencati.**

Una verifica reale (`cargo tree` su Linux con `dioxus` 0.7.10 desktop, `libmpv2` 6.0.0, `reqwest` con native-tls) mostra che **Dioxus desktop stesso** richiede crate con licenze fuori dalla classe A. Non sono evitabili senza rinunciare a Dioxus, che è un vincolo di progetto. Sono tutte licenze permissive compatibili con GPL-3.0.

| Licenza | Crate (versione verificata) | Chi li richiede |
|---|---|---|
| `Unicode-3.0` | `icu_*` 2.3, `litemap`, `tinystr`, `writeable`, `yoke`, `zerofrom`, `zerotrie`, `zerovec`, `potential_utf`; `unicode-ident` (`(MIT OR Apache-2.0) AND Unicode-3.0`) | `url` → `idna` (tao, wry, reqwest); `proc-macro2` (ogni macro, incluso `rsx!`) |
| `Zlib` | `slotmap`, `const_format`, `konst` | `dioxus-core` |
| `Zlib` | `foldhash` | `hashbrown` (usato quasi ovunque) |
| `Zlib` | `zlib-rs` | `flate2` ← `png`/`image` ← `dioxus-desktop`, `tray-icon` |
| `BSD-3-Clause` | `subtle` | `dioxus-desktop` |
| `BSD-2-Clause`, `BSD-3-Clause` | `rav1e`, `ravif`, `av1-grain`, `v_frame`, `avif-serialize`, `exr`, `lebe` | `image` con feature di default ← `dioxus-desktop` |
| `ISC` | `libloading` 0.7 / 0.8 | `subsecond` (hot-patch Dioxus), `libappindicator-sys` |
| `MPL-2.0` | `option-ext` | `dirs-sys` ← `dirs` ← `wry`, `tray-icon` |

Regole per la classe B:

1. Le deroghe valgono **solo** per i crate elencati, tramite `[[licenses.exceptions]]` in `deny.toml`. Un nuovo crate con una di queste licenze fa fallire la CI.
2. Ogni nuova deroga richiede: una motivazione ("chi la richiede e perché non è evitabile"), una riga in questa tabella e l'approvazione esplicita del maintainer.
3. Nessuna dipendenza **diretta** di Anikoto può essere di classe B, salvo `icu_normalizer` (vedi §4): è già nel grafo tramite `idna`, quindi non aggiunge nessuna licenza nuova.
4. Ad ogni aggiornamento di Dioxus si DEVE provare a ridurre la classe B, per esempio con una PR a monte per rendere opzionali le feature di `image`.

**Vietate** (la CI fallisce):

- Tutte le licenze non elencate sopra.
- In particolare: `AGPL-*` (Anki rslib), `CC-BY-NC-*`, `CC0-1.0` (es. `notify`), `BSL-1.0` (es. `xxhash-rust`), `GPL-2.0-only`, `OpenSSL`, `CECILL-*`, e qualsiasi crate senza licenza dichiarata.

### 3.3 Librerie di sistema e programmi esterni

Si caricano a runtime oppure si invocano come processi. Non vengono distribuiti con Anikoto, salvo nei pacchetti post-1.0.

| Componente | Licenza | Uso |
|---|---|---|
| libmpv (mpv) | GPL-2.0-or-later (oppure LGPL-2.1-or-later, secondo la build) | Riproduzione |
| FFmpeg / FFprobe (eseguibili) | LGPL-2.1-or-later o GPL-2.0-or-later | Estrazione sottotitoli, clip audio, immagini |
| GTK 3, GLib, WebKitGTK | LGPL-2.1-or-later (WebKit: anche BSD-2 per alcune parti) | UI tramite tao/wry |
| glibc (`iconv`) | LGPL-2.1-or-later | Conversione delle codifiche dei sottotitoli (§6.4) |
| OpenSSL 3 | Apache-2.0 | TLS tramite `native-tls` |
| Mesa / libEGL / libGL | MIT | Contesto OpenGL per il rendering mpv |

### 3.4 Dati e risorse

| Risorsa | Licenza | Regola |
|---|---|---|
| Tabelle di trasformazione Yomitan (JSON generati da Fushi `fushi/assets/transforms/`) | GPL-3.0 | Copiate in `assets/transforms/` con file `PROVENANCE.md` (versione Yomitan e commit Fushi) |
| Template della nota Lapis (donkuri/lapis) | GPL-3.0 | Copiati in `assets/anki/lapis/` con la versione fissata |
| Dizionari (JMdict, JMnedict, KANJIDIC, dizionari di frequenza, ecc.) | Varie (es. CC BY-SA 4.0) | **NON** si distribuiscono. Li importa l'utente. I test usano dizionari scritti a mano (TEST §4.3) |
| Font | — | **NON** si distribuiscono font: si usano quelli di sistema (es. Noto CJK). Un font OFL non è nella lista ammessa |
| Icone e logo | GPL-3.0-only | Opere originali del progetto; nessuna licenza Creative Commons, per coerenza con la politica |

### 3.5 Come si applica la politica

- `deny.toml` ([cargo-deny](https://github.com/EmbarkStudios/cargo-deny), MIT/Apache) codifica §3.2: `[licenses] allow = [classe A]` e `[[licenses.exceptions]]` per la classe B. Gli stessi controlli coprono anche `bans`, `advisories` e `sources`.
- `cargo deny check` è un **gate obbligatorio** della CI (T-LIC-001).
- `cargo about` (MIT/Apache) genera `THIRD_PARTY_LICENSES.html`. Il file si include in ogni pacchetto e si mostra nella schermata "Informazioni" (requisito di attribuzione di MIT/BSD/ISC/Zlib).
- Gli strumenti di sviluppo (cargo-deny, cargo-about, cargo-nextest) non vengono distribuiti, ma DEVONO rispettare comunque la classe A. **Niente `just`** (CC0): i task si scrivono come `cargo xtask` in Rust.

### 3.6 Alternative scelte per restare nella politica

| Esigenza | Scelta scartata (licenza) | Scelta adottata |
|---|---|---|
| Database locale | SQLite (pubblico dominio: fuori lista) | `redb` (MIT OR Apache-2.0) |
| Compressione dei glossari | zstd C (BSD-3 / GPL-2.0-only) | `lz4_flex` (MIT) |
| Hash veloce | `xxhash-rust` (BSL-1.0) | `twox-hash` (MIT) se serve; altrimenti l'hasher di `redb` |
| Rilevamento della codifica dei sottotitoli | `encoding_rs` (`… AND BSD-3-Clause`) | `iconv` di glibc tramite FFI (LGPL) + euristiche proprie |
| TLS | `rustls` + `ring` (ISC) | `native-tls` → OpenSSL (Apache-2.0) |
| Parser sottotitoli | `subparse` (MPL-2.0) | Parser propri (porting da Fushi) |
| Parser nomi file anime | `anitomy` (MPL-2.0) | Parser proprio (porting di `video_filename_parser.dart`) |
| Osservazione del file system | `notify` (CC0-1.0) | Rescan manuale o periodico; inotify via `libc` se servirà |
| Task runner | `just` (CC0-1.0) | `cargo xtask` |
| Normalizzazione Unicode | `unicode-normalization` (aggiunge un AND Unicode-3.0) | `icu_normalizer` (già presente nel grafo) |

---

## 4. Stack tecnologico

Versioni verificate su crates.io il 2026-10-19. Il lockfile (`Cargo.lock`) è versionato.

| Ambito | Crate | Versione | Licenza |
|---|---|---|---|
| Toolchain | Rust stable, edition 2024, fissata in `rust-toolchain.toml` | ≥ 1.85 (sviluppo: 1.99) | — |
| UI | `dioxus` (feature `desktop`, `router`) | 0.7.10 | MIT OR Apache-2.0 |
| Composizione GTK | `gtk`, `glib`, `gdk` (la stessa versione usata da `tao`/`wry`) | 0.18 | MIT |
| Video | `libmpv2` (feature `render`) | 6.0.0 | LGPL-2.1 |
| Async | `tokio` | 1.x | MIT |
| Storage | `redb` | 4.x | MIT OR Apache-2.0 |
| Serializzazione | `serde`, `serde_json`, `toml`, `postcard` | 1.x | MIT OR Apache-2.0 |
| Compressione | `lz4_flex` | 0.14 | MIT |
| ZIP | `zip` (`default-features = false`, `deflate-flate2`; `flate2` con backend `miniz_oxide`) | 9.x | MIT |
| HTTP | `reqwest` (`default-features = false`, `json`, `native-tls`) | 0.13 | MIT OR Apache-2.0 |
| Unicode | `icu_normalizer`, `unicode-segmentation` | 2.3 / 1.13 | Unicode-3.0 (classe B) / MIT OR Apache-2.0 |
| Errori | `thiserror` (librerie), `anyhow` (solo binario e xtask) | 2.x / 1.x | MIT OR Apache-2.0 |
| Log | `tracing`, `tracing-subscriber`, `tracing-appender` | 0.1 / 0.3 | MIT |
| Percorsi XDG | `directories` | 6.x | MIT OR Apache-2.0 |
| Dialoghi file | `rfd` (portale XDG) | 0.17 | MIT |
| i18n | `fluent-bundle`, `unic-langid` | 0.16 | Apache-2.0 OR MIT |
| Identificativi e hash | `uuid`, `sha2`, `md4` (ed2k, M7) | — | MIT OR Apache-2.0 |
| HTML dei dizionari (solo post-1.0, MDict) | `ammonia` | 4.x | MIT OR Apache-2.0 |
| Test | `proptest`, `insta`, `criterion`, `wiremock`, `tempfile` | — | MIT/Apache |

Ogni nuovo crate richiede in PR: la licenza (verificata da `cargo deny`), una motivazione e l'alternativa "scrivere il codice noi" valutata.

---

## 5. Architettura

### 5.1 Workspace

```
anikoto/
├── Cargo.toml              # workspace, [workspace.dependencies] centralizzate
├── rust-toolchain.toml
├── deny.toml  about.toml
├── xtask/                  # cargo xtask: ci, fixtures, about, bench
├── crates/
│   ├── anikoto-core/       # tipi condivisi, config, percorsi, errori, orologio
│   ├── anikoto-text/       # normalizzazione, kana, preprocessori, frasi, furigana, conteggio caratteri
│   ├── anikoto-dict/       # import Yomitan, storage redb, deinflessione, lookup, audio parole
│   ├── anikoto-subs/       # parser SRT/VTT/ASS, indice dei cue, offset, codifiche
│   ├── anikoto-media/      # runner ffmpeg/ffprobe: estrazione sottotitoli, clip, frame
│   ├── anikoto-player/     # wrapper libmpv2: init, proprietà, comandi, eventi, render GL
│   ├── anikoto-anki/       # client AnkiConnect, composer note, Lapis, link di ritorno
│   ├── anikoto-library/    # libreria, nomi file, progressi, statistiche
│   ├── anikoto-online/     # (M7) AniList/Jikan, Jimaku, OpenSubtitles
│   ├── anikoto-ui/         # componenti Dioxus (popup, overlay sottotitoli, controlli, impostazioni)
│   └── anikoto-app/        # binario: avvio Dioxus, compositore GTK+mpv, collegamenti
├── assets/                 # transforms/, anki/lapis/, i18n/*.ftl, icone
├── tests/fixtures/         # piccoli file versionati; i media si generano con xtask
└── docs/
```

**Regole di dipendenza** (ARC-010; le verifica T-ARC-010 con `cargo metadata`):

- `anikoto-core` non dipende da nessun crate del workspace.
- `anikoto-text`, `anikoto-subs`, `anikoto-dict`, `anikoto-anki` e `anikoto-library` **NON DEVONO** dipendere da `dioxus`, `gtk` o `libmpv2`. Restano logica pura, testabile senza display.
- Solo `anikoto-player` dipende da `libmpv2`. Solo `anikoto-app` dipende da `gtk`. Solo `anikoto-ui` e `anikoto-app` dipendono da `dioxus`.
- ARC-011: `unsafe` è ammesso solo in `anikoto-player` (callback GL) e in `anikoto-app::linux` (FFI GTK/EGL e iconv). Ogni blocco DEVE avere un commento `// SAFETY:`. Gli altri crate dichiarano `#![forbid(unsafe_code)]`.

### 5.2 Video e UI nella stessa finestra (Linux)

Questo è il **rischio tecnico principale** (R-1). Dioxus desktop disegna l'intera UI in una webview (WebKitGTK), mentre mpv deve disegnare il video con OpenGL. I due livelli vanno composti nella stessa finestra.

#### 5.2.1 Soluzione scelta: GtkOverlay

```
GtkWindow (creata da tao)
└── GtkBox "default_vbox" (tao)
    └── GtkOverlay                      ← inserito da Anikoto
        ├── GtkGLArea                   (figlio principale: video, API render di libmpv)
        └── WebKitWebView (wry/Dioxus)  (overlay: tutta la UI, sfondo trasparente)
```

- La webview sta **sopra** e riceve tutti gli eventi di mouse e tastiera. La GLArea non riceve input: non serve far passare gli eventi da un livello all'altro.
- Nelle pagine senza video (libreria, impostazioni) la GLArea è nascosta e la webview ha uno sfondo opaco.
- Nel player il `body` della webview è trasparente (`Config::with_background_color((0,0,0,0))` più CSS). Sono opachi solo i controlli, i sottotitoli e i popup.

**Inserimento nel widget tree.** Dioxus 0.7.10 su Linux crea la webview con `webview.build_gtk(window.default_vbox())` (vedi `dioxus-desktop/src/webview.rs`). Due strategie, da validare nello spike S1:

- **(a) Riparentatura a runtime**: al primo render si recupera la `WebKitWebView` dal vbox, la si rimuove, si crea il `GtkOverlay` con la GLArea e si reinserisce la webview come overlay.
- **(b) Patch minima di `dioxus-desktop`**: un hook `Config::with_gtk_container(...)` che restituisce il contenitore in cui costruire la webview. La patch si mantiene in `third_party/dioxus-desktop` (MIT/Apache) e si propone a monte.

#### 5.2.2 Rendering mpv

1. Su `realize` della GLArea: `make_current()`, poi `mpv.create_render_context([ApiType(OpenGl), InitParams{get_proc_address}])`.
   - `get_proc_address` usa `eglGetProcAddress` su Wayland ed EGL, oppure `glXGetProcAddressARB` su X11/GLX, tramite FFI diretta verso Mesa (MIT). Non si usano crate di loader.
2. La callback di update di mpv (chiamata da un thread qualsiasi) **NON DEVE** chiamare API di mpv. Esegue solo `glib::MainContext::default().invoke(|| gl_area.queue_render())`.
3. Sul segnale `render`:
   - si legge `GL_DRAW_FRAMEBUFFER_BINDING`;
   - si chiama `render_context.render(fbo, w·scale, h·scale, flip=true)`, dove `scale` è lo scale factor HiDPI;
   - si chiama `report_swap()`.
4. Su `unrealize`: il render context si distrugge **prima** del contesto GL e prima di `Mpv`. L'ordine è garantito dai lifetime di `libmpv2::render::RenderContext<'a>`.

#### 5.2.3 Strategie di ripiego (se S1 fallisce i criteri)

- **B**: solo X11/XWayland. mpv disegna con `wid` in una finestra figlia nativa; la UI Dioxus va in una finestra trasparente separata che segue la geometria del player.
- **C**: `dioxus-native` (renderer Blitz su wgpu), quando sarà maturo. La texture mpv si comporrebbe direttamente nella scena.
- **D**: player in una finestra separata, con mpv che disegna da solo. Si accetta solo come ripiego temporaneo, perché perde i sottotitoli cliccabili sopra il video.

#### 5.2.4 Altre piattaforme (post-1.0)

Il compositore DEVE stare dietro un trait (`VideoSurface`) in `anikoto-app`, con un'implementazione per piattaforma:

- Windows: WebView2 trasparente con `build_as_child` sopra una finestra HWND figlia con `wid`;
- macOS: NSView con contesto OpenGL o Metal.

### 5.3 Thread ed eventi

```
 ┌───────────── thread UI GTK (main) ─────────────┐        ┌──── thread mpv-events ────┐
 │ Dioxus VirtualDom + Signals                    │        │ loop: mpv.wait_event(-1)  │
 │ GLArea.render → render_context.render()        │◄───────┤ PropertyChange / EndFile  │
 │ comandi UI → PlayerHandle (mpsc)               ├───────►│ → PlayerEvent (canale)    │
 └────────────────────────────────────────────────┘        └───────────────────────────┘
          ▲                    │ tokio runtime (I/O, ffmpeg, HTTP, import dizionari)
          └────────────────────┘
```

- **ARC-001**: `anikoto-player` espone `PlayerHandle` (comandi asincroni) e uno stream di `PlayerEvent`. Le proprietà osservate (`observe_property`) sono: `time-pos`, `duration`, `pause`, `speed`, `volume`, `mute`, `track-list`, `aid`, `sid`, `chapter-list`, `eof-reached`, `seeking`, `paused-for-cache`, `video-params`, `demuxer-cache-state`.
- **ARC-002**: ogni caricamento ha un `LoadToken` crescente. Gli eventi e i risultati asincroni con un token vecchio si scartano. È l'equivalente dei bug Fushi "ricontrollare player e loadToken dopo ogni await".
- **ARC-003**: nessuna lettura sincrona di proprietà mpv sul thread UI durante la riproduzione (Fushi BUG-3507). Lo stato si legge dagli eventi.
- **ARC-004**: gli aggiornamenti di `time-pos` verso la UI si limitano a ≤ 30 Hz. La sincronizzazione dei cue usa il valore più recente con ricerca binaria. Il timer a 125 ms di Fushi non serve.

### 5.4 Persistenza

| Cosa | Dove | Formato |
|---|---|---|
| Impostazioni | `$XDG_CONFIG_HOME/anikoto/config.toml` | TOML leggibile; scrittura atomica (file temporaneo + rename); permessi 0600 |
| Libreria, progressi, statistiche | `$XDG_DATA_HOME/anikoto/library.redb` | redb; valori `postcard`; tabella `meta.schema_version` |
| Dizionari | `$XDG_DATA_HOME/anikoto/dicts/<id>/dict.redb` | Un file per dizionario (§6.5.3) |
| Cache (sottotitoli estratti, miniature, audio parole) | `$XDG_CACHE_HOME/anikoto/` | File; si può cancellare in qualsiasi momento |
| Log | `$XDG_STATE_HOME/anikoto/logs/` | Testo, rotazione giornaliera, 7 file |

- **DAT-001**: le migrazioni di schema vanno solo in avanti, sono numerate e sono idempotenti. Prima di migrare si copia il file in `library.redb.bak-v<N>`.
- **DAT-002**: se il file è stato scritto da una versione più recente di Anikoto (schema maggiore di quello supportato), lo si apre in sola lettura con un avviso. Non lo si modifica mai.
- **DAT-003**: l'identità di un video è `uid = "video/" + sanitize(nome file senza estensione)`, come in Fushi. In più si salvano `size` e `mtime` per riconoscere un file spostato.

### 5.5 Processi esterni

- **EXT-001**: ffmpeg e ffprobe si cercano in quest'ordine: impostazione `paths.ffmpeg`, variabile `ANIKOTO_FFMPEG`, `PATH`. All'avvio si verifica la versione e la presenza degli encoder necessari (`ffmpeg -hide_banner -encoders`). Se ne manca uno, la funzione relativa si disattiva con un messaggio chiaro.
- **EXT-002**: gli argomenti si passano sempre come vettore, mai attraverso una shell. I percorsi di input si prefissano con `file:` per neutralizzare nomi che iniziano con `-` o contengono `:`.
- **EXT-003**: ogni processo ha un timeout (predefinito 60 s; per l'estrazione dei sottotitoli scala con la durata) ed è cancellabile. Alla cancellazione si uccide il processo e si rimuovono i file parziali.

### 5.6 Errori, log e privacy

- **ERR-001**: le librerie definiscono errori tipizzati con `thiserror`. La UI mostra messaggi tradotti e azionabili ("AnkiConnect non risponde su 127.0.0.1:8765: Anki è aperto?").
- **ERR-002**: nessun `panic!`/`unwrap()` su input esterno (file, rete, dizionari). Clippy con `unwrap_used` e `expect_used` vale come errore nei crate di libreria.
- **PRV-001**: zero telemetria. Senza un'azione dell'utente o una funzione che l'utente ha attivato, l'unica connessione permessa è verso AnkiConnect su loopback.
- **PRV-002**: i log non contengono il testo dei sottotitoli né le chiavi API. I percorsi sono ammessi.

### 5.7 Istanza singola e schema URL

- **APP-001**: una sola istanza per utente. Un secondo avvio passa gli argomenti (file da aprire, URL `anikoto://`) all'istanza attiva tramite il socket Unix `$XDG_RUNTIME_DIR/anikoto.sock`, poi termina.
- **APP-002**: il file `.desktop` registra `MimeType=x-scheme-handler/anikoto;` e i tipi video, con `Exec=anikoto %U`.

---

## 6. Requisiti funzionali

### 6.1 Libreria video (LIB)

| ID | Requisito | Fushi |
|---|---|---|
| LIB-001 | DEVE aprire un singolo file video: dal dialogo file, con drag & drop sulla finestra o come argomento da riga di comando. Estensioni proposte nel dialogo: mp4 mkv avi mov webm m4v ts m2ts mts flv wmv mpg mpeg ogv; il dialogo DEVE permettere anche "tutti i file" | `kVideoExtensions` |
| LIB-002 | DEVE importare una cartella in modo ricorsivo, in due modalità: **per serie** (predefinita) e **per cartella** | `video_library_import.dart` |
| LIB-003 | Per serie: il nome file si analizza in stile Anitomy (serie, stagione, episodio, gruppo, risoluzione). Se manca la stagione, la si ricava dalla cartella (`Season 2`, `S02`, `第2季`, numero finale). Se un nome non si analizza, il numero di episodio si deduce confrontandolo con i file vicini | `video_filename_parser.dart` |
| LIB-004 | Cambiare modalità o rifare la scansione NON DEVE perdere progressi, sottotitoli scelti o metadati. Una scansione NON DEVE ricreare le serie che l'utente ha eliminato | spec `2026-09-07-video-library-workflow.md` |
| LIB-005 | La home mostra "Continua a guardare" (ultimi episodi iniziati e non finiti) e la griglia delle serie con copertina. La copertina è il fotogramma al 10% del primo episodio (ffmpeg, in cache) finché non ci sono metadati | |
| LIB-006 | Avanzamento dell'episodio: si salva al massimo una volta al secondo, solo dopo che il file è aperto e il seek di ripresa è avvenuto. All'uscita e al cambio episodio si salva subito | `video_player_controller.dart` |
| LIB-007 | Regola di ripresa: si riparte da 0 se l'avanzamento salvato è ≥ 90% o se restano meno di 3 s. "Episodio precedente" e l'avanzamento automatico partono sempre da 0 | `video_episode_start_policy.dart` |
| LIB-008 | `completed_at` si imposta la prima volta che l'avanzamento raggiunge il 90%. La griglia DEVE mostrare lo stato visto / in corso / nuovo | |
| LIB-009 | A fine episodio DEVE poter partire il successivo (impostazione `player.auto_next`, predefinita sì), con un conto alla rovescia annullabile di 5 s | |
| LIB-010 | Per ogni serie si ricordano traccia audio, sottotitolo scelto (per lingua o titolo della traccia) e offset dei sottotitoli. Si applicano a tutti gli episodi, che però possono avere valori propri | `VideoBooks.delayMs` |
| LIB-011 | DEVE essere possibile rinominare, unire e dividere serie, e spostare episodi da una serie all'altra | |
| LIB-012 | Rimuovere un elemento dalla libreria NON DEVE cancellare il file video dal disco | |

### 6.2 Player (PLY)

| ID | Requisito | Fushi |
|---|---|---|
| PLY-001 | Comandi: play/pausa, seek ±N s (predefinito 3, intervallo 1–30), seek assoluto dalla barra, velocità 0.25–4.0 (passo 0.1), volume 0–150 (passo 5), muto, fotogramma avanti/indietro, capitoli precedente/successivo, schermo intero | `video_player_shortcuts.dart` |
| PLY-002 | "Tieni premuto per accelerare": velocità 2.0× finché il tasto è premuto (predefinito `E`) | |
| PLY-003 | Selezione della traccia audio dall'elenco `track-list` (lingua, titolo, codec, canali), salvata per serie | |
| PLY-004 | Barra dei controlli in sovraimpressione: si nasconde dopo 2 s senza movimento del mouse durante la riproduzione (nasconde anche il cursore) e riappare al movimento. Resta visibile in pausa, con il mouse sopra i sottotitoli e con un popup aperto | |
| PLY-005 | Opzioni mpv applicate **prima** di `initialize` (fisse): `vo=libmpv`, `config=no`, `input-default-bindings=no`, `input-vo-keyboard=no`, `osc=no`, `terminal=no`, `idle=yes`, `keep-open=yes`, `ytdl=no`, `sub-auto=no` | `video_mpv_config.dart` |
| PLY-006 | Opzioni configurabili, impostate prima di `loadfile`: `hwdec` (`auto-safe` predefinito, `auto-copy-safe`, `no`), profilo qualità (`default` o `high-quality`), `deband`, `interpolation` (con `video-sync=display-resample` e `tscale=oversample`), `audio-channels` (predefinito `auto-safe`; preset Fushi `7.1,5.1,stereo` come opzione), `audio-pitch-correction=yes`, `tone-mapping=auto`. In più un campo "opzioni mpv grezze" (`chiave=valore` per riga) applicato per ultimo, con validazione ed errore per riga | `video_mpv_config.dart` (BUG-798, BUG-1545) |
| PLY-007 | Ripresa: la posizione si imposta con la proprietà `start` prima di `loadfile` e si azzera (`start=none`) dopo `file-loaded`. NON si fa un seek dopo il caricamento, perché `loadfile` lo sovrascrive | BUG-1288 |
| PLY-008 | Dopo `file-loaded`: si adotta l'`aid` effettivo; si disattiva il rendering mpv per le tracce testuali (`sid=no`, `sub-visibility=no`); si applicano volume e velocità salvati | `video_player_controller.dart` `load()` |
| PLY-009 | Il player e il render context si **riusano** tra un episodio e l'altro: si ricrea solo il contenuto con `loadfile replace` | BUG-120 |
| PLY-010 | La velocità usa solo la proprietà `speed`, mai il filtro `scaletempo` | TODO-116 |
| PLY-011 | Screenshot (`S`): il fotogramma senza sottotitoli finisce nella cartella `Immagini/Anikoto` (`$XDG_PICTURES_DIR/Anikoto`) | |
| PLY-012 | Errori di riproduzione (file mancante, codec non supportato, `end-file` con errore) DEVONO mostrare un messaggio comprensibile e non bloccare l'app | |
| PLY-013 | Inibizione del salvaschermo durante la riproduzione, tramite il portale XDG `Inhibit`, o tramite `stop-screensaver` di mpv se il portale non c'è | |
| PLY-014 | Miniature sulla barra di avanzamento: post-1.0 | |
| PLY-015 | Screenshot con i sottotitoli dell'overlay (`Shift+S`): post-1.0, perché richiede di comporre l'overlay sul fotogramma | |

### 6.3 Scorciatoie (KEY)

| ID | Requisito |
|---|---|
| KEY-001 | Tutte le azioni sono rimappabili; le assegnazioni si salvano in `config.toml` (`[shortcuts]`). |
| KEY-002 | I tasti si riconoscono dal **codice fisico** (`KeyboardEvent.code`), non dal carattere prodotto: le scorciatoie restano uguali con qualsiasi layout e metodo di input (Fushi BUG-936). |
| KEY-003 | Due azioni nello stesso contesto (`globale`, `libreria`, `player`, `popup`) non possono avere lo stesso tasto: l'editor segnala il conflitto e non salva. |
| KEY-004 | Con un campo di testo attivo, le scorciatoie a tasto singolo sono disattivate. |

Assegnazioni predefinite del player, da Fushi `shortcut_defaults.dart`:

| Azione | Tasti | Milestone |
|---|---|---|
| Play/pausa | Space, P, MediaPlayPause | M1 |
| Seek indietro / avanti | ←, A / →, D | M1 |
| Sottotitolo precedente / successivo | Ctrl+← / Ctrl+→ | M2 |
| Ripeti il cue corrente / precedente | R / Shift+R | M2 |
| Volume + / − ; muto | ↑, 0 / ↓, 9 ; M | M1 |
| Velocità + / − / normale | ], = / [, - / Backspace | M1 |
| Tieni premuto per velocità 2× | E | M1 |
| Fotogramma precedente / successivo | , / . | M1 |
| Screenshot | S | M1 |
| Schermo intero | F, F11 | M1 |
| Lista sottotitoli / cerca | L / Ctrl+F | M2 |
| Blur sottotitolo / nascondi sottotitolo | B / H | M2 |
| Offset sottotitolo −100 / +100 ms | Z / X | M2 |
| Allinea l'inizio del cue precedente / successivo alla posizione | Ctrl+Shift+← / Ctrl+Shift+→ | M2 |
| Capitolo precedente / successivo | PageUp / PageDown | M1 |
| Cursore di carattere per il lookup da tastiera | Enter | M3 |
| Crea carta dalla voce del popup | Ctrl+Enter | M4 |
| Indietro (chiude popup → esce dallo schermo intero → torna alla libreria) | Esc, Alt+← | M1 |

### 6.4 Sottotitoli (SUB)

**Principio (da Fushi):** mpv **non** disegna i sottotitoli testuali. Anikoto li analizza in cue e li disegna nell'overlay Dioxus. È questo che rende possibile il clic sul singolo carattere, il blur, gli stili e il mining. Solo le tracce grafiche (PGS, VobSub, DVB) le disegna mpv (`sub-visibility=yes`), senza lookup.

| ID | Requisito | Fushi |
|---|---|---|
| SUB-001 | Parser **SRT**: BOM, CRLF/LF, indici mancanti o fuori ordine, `{...}` rimossi (BUG-105), tag `<i> <b> <u> <font color>` convertiti in markup, timestamp con `,` o `.` | `srt_parser.dart` |
| SUB-002 | Parser **WebVTT**: header, cue settings (`line`, `position`, `align`), tag `<c>`, `<v>`, `<ruby>`, `<rt>`, cue con identificativo, blocchi `NOTE` e `STYLE` ignorati | `vtt_parser.dart` |
| SUB-003 | Parser **ASS/SSA** (base per la 1.0): `[Script Info]` (PlayResX/Y, predefinito 384×288), `[V4+ Styles]`, `[Events]` (Dialogue con Layer, Start, End, Style, Text), override `\N \n \h \an1-9 \pos \b \i \u \s \c \1c \fs \fn`. Gli altri override si ignorano senza errore. Le righe con disegni `\p1` si escludono dal testo cliccabile | `ass_parser.dart`, `subtitle_markup.dart` |
| SUB-004 | Ogni cue ha `start_ms`, `end_ms`, `plain_text` (usato per lookup, mining e conteggi) e un `markup` separato, usato solo per disegnare | |
| SUB-005 | Codifica: BOM UTF-8/UTF-16 → altrimenti validazione UTF-8 → altrimenti prova Shift_JIS (CP932), EUC-JP, GB18030, Big5, CP1252 tramite `iconv`, scegliendo la prima che decodifica senza errori e con il punteggio più alto di caratteri plausibili. La codifica rilevata si mostra e si può forzare | `text_file_io.dart` |
| SUB-006 | File esterni accanto al video: stesso nome base con `.srt .ass .ssa .vtt`, anche con suffisso di lingua (`.ja.srt`, `.jpn.ass`, `.jp.srt`). Si caricano automaticamente secondo la lingua preferita (predefinita `ja`) | |
| SUB-007 | Tracce incorporate testuali: si estraggono con `ffmpeg -y -i file:<video> -map 0:<ff-index> -c:s <srt\|ass\|webvtt> <cache>`. Più tracce si estraggono in un solo passaggio; se il passaggio fallisce, si riprova traccia per traccia. La cache usa come chiave (percorso, size, mtime, ff-index) | `video_subtitle_source.dart` |
| SUB-008 | Selezione automatica del sottotitolo: (1) la scelta salvata per la serie; (2) un file esterno nella lingua preferita; (3) una traccia incorporata nella lingua preferita (`lang` = ja/jpn, oppure titolo che contiene "Japanese" o "日本語"); (4) nessuno | |
| SUB-009 | Overlay: più cue attivi insieme sono ammessi (ordinati per Layer e poi per ordine nel file). Testo diviso in **grafemi** (`unicode-segmentation`), ogni grafema è uno `span` con l'offset del carattere nel `plain_text` | `video_subtitle_overlay.dart` |
| SUB-010 | Stile predefinito: testo bianco, peso 400, ombra morbida, nessun box, 36 px riferiti a un'altezza video di 1080 (scala in proporzione), 75 px dal fondo; si alza quando la barra dei controlli è visibile. Le opzioni di stile (font, dimensione, contorno, ombra, box, posizione) si possono modificare. "Rispetta gli stili ASS" (predefinito no) usa colori, font e posizione dell'ASS | `video_subtitle_style.dart` |
| SUB-011 | Offset: `delay_ms` positivo = il sottotitolo appare dopo. Il tempo del cue confrontato è `pos − delay_ms`; seek e mining applicano l'inverso. Z/X = ∓100 ms; l'offset è visibile in un toast | |
| SUB-012 | Navigazione: "precedente/successivo" porta a `cue.start − 180 ms`, mai prima dell'inizio del cue precedente, tenendo conto di `delay_ms` (BUG-259). L'obiettivo si calcola dalla posizione corrente con ricerca *floor*, non dall'indice in cache. Senza sottotitoli si ricade sul seek ±N s. Con la tastiera, se il cue precedente è lontano più di 10 s, si fa un seek di tempo (BUG-3339) | `video_player_controller.dart` |
| SUB-013 | "Ripeti il cue" porta all'inizio del cue corrente e, se attivato dal popup, mette in pausa alla fine del cue | `replayCue` |
| SUB-014 | Blur / nascosto: modalità per il sottotitolo principale (`nessuna`, `blur`, `nascosto`). Sigma del blur = max(12, font_size·0,45). Il testo si rivela con il mouse sopra. Il blur vale solo durante la riproduzione e senza popup aperti (BUG-2198, BUG-2235) | |
| SUB-015 | Pannello lista sottotitoli (`L`): elenco di tutti i cue con tempo, ricerca testuale (`Ctrl+F`), scorrimento automatico che segue la riproduzione (nei vuoti tra cue resta sul cue precedente), clic sul tempo per saltare, clic su una parola per il lookup | `video_subtitle_jump_panel.dart` |
| SUB-016 | Allinea l'inizio del cue precedente o successivo alla posizione corrente (Ctrl+Shift+←/→), aggiornando `delay_ms` | |
| SUB-017 | Sottotitolo **secondario** (es. traduzione): secondo livello con offset proprio (predefinito: segue quello principale), modalità blur e posizione propri. Milestone M7 | |
| SUB-018 | Allineamento automatico con la forma d'onda: envelope RMS con ffmpeg `astats` e cross-correlazione (bin da 100 ms, ricerca ±15 s, confidenza minima 0,15). Milestone M7 | `subtitle_auto_align.dart`, `audio_energy_probe.dart` |
| SUB-019 | ASS avanzato (`\t`, `\move`, `\fad`, `\clip`, karaoke `\k`, rotazioni): post-1.0 | |

### 6.5 Dizionari: import e gestione (DIC)

#### 6.5.1 Formati

| ID | Requisito | Fushi |
|---|---|---|
| DIC-001 | DEVE importare dizionari **Yomitan/Yomichan** (zip con `index.json`, formato 3; il formato 1/2 è DOVREBBE): `term_bank_*`, `term_meta_bank_*` (`freq`, `pitch`, `ipa`), `kanji_bank_*`, `kanji_meta_bank_*` (`freq`), `tag_bank_*`, `styles.css`, file media | `importer.cpp` |
| DIC-002 | A differenza di Fushi, DEVE importare i `tag_bank_*` (categoria, ordine, note, punteggio) e usarli per i chip e per l'ordinamento. DEVE conservare `sequence` e `score` | lacuna Fushi |
| DIC-003 | Frequenze: valori come numero, `{value, displayValue?}` e `{reading, frequency}`. A differenza di Fushi, DEVE accettare anche valori stringa (leggendo l'intero iniziale) e rispettare `frequencyMode` di `index.json` (`rank-based` predefinito; `occurrence-based` = più alto è meglio) | lacuna Fushi |
| DIC-004 | MDict, StarDict, DSL, Migaku: post-1.0 | |

#### 6.5.2 Pipeline di import

| ID | Requisito |
|---|---|
| DIC-010 | L'import gira in un task in background con avanzamento (file e voci), annullabile; la UI resta reattiva. |
| DIC-011 | Sicurezza dello ZIP: si rifiutano percorsi assoluti, componenti `..`, link simbolici, entry > 512 MiB non compresse, rapporto di compressione > 200:1 su entry > 1 MiB, totale non compresso > 8 GiB. Si toglie una sola cartella contenitore e si ignorano `__MACOSX` e `.DS_Store` (BUG-2053). Si supporta il campo extra Info-ZIP per percorsi Unicode (BUG-3277) e ZIP64. |
| DIC-012 | Limiti: 10 M voci per dizionario, glossario ≤ 10 MiB per voce. Se si supera un limite, l'import fallisce in modo pulito. |
| DIC-013 | L'import scrive in `dicts/<id>.importing/` e, solo a successo completato, fa un rename atomico a `dicts/<id>/` e registra il dizionario. Dopo un errore o un crash non resta niente di parziale; i residui `.importing` si eliminano all'avvio. |
| DIC-014 | Disco pieno: messaggio specifico (`ERR_STORAGE_FULL`), con pulizia dei file parziali. |
| DIC-015 | Si rileva il **tipo**: term se ci sono term bank; altrimenti kanji se ci sono kanji bank; altrimenti si guarda la modalità del primo meta (`freq` → frequency, `pitch`/`ipa` → pitch). Un dizionario term con kanji bank conta per entrambi. |
| DIC-016 | Si rifiuta un dizionario con lo stesso titolo e la stessa revisione di uno già importato. Con una revisione diversa si propone l'aggiornamento (sostituzione atomica). |

#### 6.5.3 Storage (un file redb per dizionario)

| Tabella | Chiave → valore | Note |
|---|---|---|
| `info` | `&str` → `&[u8]` | `index.json`, `styles.css`, versione schema, conteggi |
| `terms` | `u64` (id) → `TermRecord` (postcard) | espressione, lettura, tag di definizione, regole, tag del termine, punteggio, sequence, glossario JSON compresso con lz4 |
| `term_by_expression` | multimap `&str` → `u64` | |
| `term_by_reading` | multimap `&str` → `u64` | Solo se la lettura è diversa dall'espressione |
| `term_meta` | multimap `&str` → `MetaRecord` | freq / pitch / ipa |
| `kanji` | `&str` (carattere) → `KanjiRecord` | |
| `kanji_meta` | multimap `&str` → `MetaRecord` | |
| `tags` | `&str` → `TagRecord` | |
| `media` | `&str` (percorso normalizzato: `\` → `/`, senza `/` iniziale) → `&[u8]` | |

- **DIC-020**: lo storage sta dietro un trait `DictStore`; l'implementazione redb è la prima. Lo spike S2 deve rispettare i limiti di NFR-PERF-04/05; se non ci riesce, si passa a un formato su misura (mmap + tabella hash + blob, come fushidicts) dietro lo stesso trait.
- **DIC-021**: un file per dizionario: eliminare un dizionario = chiuderlo e cancellare la sua cartella (Fushi BUG-1756: i lock mmap su Windows). Il registro (ordine, abilitato, lingua, alias) sta in `library.redb`.

#### 6.5.4 Gestione

| ID | Requisito |
|---|---|
| DIC-030 | Elenco dei dizionari per tipo (termini, frequenze, accento, kanji): titolo, revisione, data di import, numero di voci, dimensione. |
| DIC-031 | Riordino con trascinamento: l'ordine decide la posizione dei glossari nel popup e la priorità dei dizionari di frequenza. |
| DIC-032 | Ogni dizionario si può abilitare o disabilitare. Uno disabilitato **non viene interrogato** (Fushi BUG-3454/2997: escluderlo prima del troncamento, non al momento del rendering). |
| DIC-033 | Eliminazione con conferma; rinomina solo per la visualizzazione (`display_name`). |
| DIC-034 | Stato di comprimere o espandere il dizionario nel popup: `espanso` / `compresso` / `automatico` (si espandono solo i primi N). |
| DIC-035 | CSS per dizionario: quello del dizionario (`styles.css`) più quello dell'utente, entrambi limitati con `@scope ([data-dictionary="<id>"])` (WebKitGTK ≥ 2.44). |

### 6.6 Lookup (LKP)

L'algoritmo è un port di quello di Yomitan, come lo implementa fushidicts. Fushi: `native/fushidicts/fushidicts_src/lookup.cpp`, `deinflector.cpp`, `text_processor/text_processor.cpp`, `scan/word_scan.cpp`.

#### 6.6.1 Scansione del testo

| ID | Requisito |
|---|---|
| LKP-001 | Il lookup parte dal grafema cliccato. Per le scritture con spazi (latino, cirillico, ecc.) si torna all'inizio della parola. |
| LKP-002 | Il testo della query va dal punto di partenza alla fine del cue (nei sottotitoli) o fino a 400 caratteri (nel testo libero), e si ferma ai delimitatori ``。、！？…‥「」『』（）()【】〈〉《》〔〕｛｝{}［］[]・：；:;，,.─\n\r"'“”‘’«»‹›``. Uno spazio singolo non ferma la scansione (BUG-1773); un apostrofo dentro una parola nemmeno (BUG-2056). |
| LKP-003 | Normalizzazione prima della ricerca: tronca a 2000 caratteri, `\n` → spazio, emoji → spazio, toglie la punteggiatura all'inizio e alla fine, sostituisce i surrogati isolati. |
| LKP-004 | Candidati: prefissi dei primi **16** code point, dal più lungo al più corto. Non si taglia mai tra due lettere di scritture con spazi; si scartano i prefissi che finiscono con uno spazio; per il CJK ogni code point è un punto di taglio valido. |

#### 6.6.2 Preprocessori (varianti del testo)

| ID | Requisito |
|---|---|
| LKP-010 | Ogni preprocessore genera un insieme di varianti. Il prodotto cartesiano si deduplica tenendo il numero minimo di "passi" (un passo per ogni preprocessore che cambia il testo). |
| LKP-011 | Catena per il giapponese, in quest'ordine: NFKC → kana (invariato / katakana→hiragana, con `ー` sostituito dalla vocale del kana precedente e ヵヶ conservati / hiragana→katakana) → collasso di っっ e ーー ripetuti (**mai** il collasso totale, BUG-1777) → ASCII → larghezza piena → varianti di kanji (tabella di 2122 voci) → espansione dei segni di iterazione 々ゝゞヽヾ → cifre a larghezza piena → numerali kanji. |
| LKP-012 | Catene per le altre lingue (apostrofi, minuscole, rimozione dei segni diacritici e combinanti, harakat, niqqud, jamo coreani **per ultimi**, BUG-2148): si attivano solo per la **lingua di studio** configurata, oppure per la lingua dichiarata da ciascun dizionario. È un miglioramento rispetto a Fushi, che applica tutto a tutte le lingue. |

#### 6.6.3 Deinflessione

| ID | Requisito |
|---|---|
| LKP-020 | Dati: le tabelle Yomitan in `assets/transforms/<lang>.json` (schema `conditions`, `transforms`, `rules` di tipo suffix/prefix/wholeWord, `conditionsIn`/`conditionsOut`; `phrasalVerbs` solo per l'inglese). |
| LKP-021 | Condizioni come maschere di bit, fino a 64 per lingua; le `subConditions` si espandono fino al punto fisso. |
| LKP-022 | L'algoritmo è ricorsivo, con profondità massima 10. Emette sempre `(testo, condizioni, traccia)`. Una regola si applica se `condizioni == 0` oppure `condizioni & conditionsIn ≠ 0`. Per i suffissi si prova dalla lunghezza massima fino a 1; per `wholeWord` il prefisso residuo deve essere vuoto. |
| LKP-023 | Filtro per parte del discorso: con condizioni 0 si tengono tutti i termini. Altrimenti si tiene un termine solo se le regole dei suoi `rules` intersecano le condizioni. `*` = tutte. Un dizionario senza regole in nessuna voce tratta le regole vuote come `*` (BUG-2543). |
| LKP-024 | La traccia si conserva nell'ordine di rimozione e si mostra al contrario (`-て « -いる « -た`), con nomi e descrizioni tradotti. |

#### 6.6.4 Ricerca, unione, ordinamento

| ID | Requisito |
|---|---|
| LKP-030 | Per ogni forma deinflessa, si interrogano tutti i dizionari term abilitati, in ordine, per espressione o lettura uguale. |
| LKP-031 | Ortografia mista kanji+kana (BUG-3261): ogni gruppo di kanji (al massimo 3 gruppi) si sostituisce con le letture trovate per quel gruppo (al massimo 16 per gruppo, 64 combinazioni), e si interroga l'indice delle letture. Un risultato si accetta solo se ogni gruppo di kanji dell'headword compare letteralmente nella query o è scritto per intero nel kana corrispondente. |
| LKP-032 | Rinvii (redirect): un glossario fatto solo di `[formOf, [regole…]]` (≤ 1 KiB) si sostituisce con un lookup di un livello del lemma, senza filtro per parte del discorso; le regole si aggiungono alla traccia. I record che hanno un'etichetta di rinvio **e** una definizione vera si conservano (BUG-2853, BUG-2566). |
| LKP-033 | Unione per (espressione, lettura): vince il `matched` più lungo in code point; a parità vince chi ha meno passi di preprocessore; eccezione Kansai -た → ため/たび (BUG-3212). I glossari si accodano nell'ordine dei dizionari; il punteggio è il massimo. |
| LKP-034 | Ordinamento, in quest'ordine: (1) `matched` più lungo; (2) meno passi di preprocessore; (3) traccia di deinflessione più corta; (4) headword uguale alla forma deinflessa; (5) frequenza: per ogni dizionario di frequenza, nell'ordine utente, il minimo valore ≥ 0, crescente (i mancanti vanno ultimi); (6) **priorità del dizionario** (miglioramento: Fushi non ce l'ha); (7) punteggio decrescente; (8) espressione uguale alla lettura. |
| LKP-035 | Il budget `max_results` (predefinito 10) conta i **gruppi di headword**, non le righe di glossario (BUG-1472). Le frequenze si arricchiscono dopo la deduplicazione, l'accento dopo il troncamento (BUG-1304). |
| LKP-036 | Raggruppamento: chiave `espressione\n(lettura o espressione)`. Dentro il gruppo, glossari ordinati per dizionario. Frequenze deduplicate per (dizionario, valori); accenti per (dizionario, posizioni, pattern, trascrizioni). `best_length` = `matched` massimo, usato per evidenziare il testo. |
| LKP-037 | Lookup dei kanji: se la query è esattamente un carattere Han, oppure con un clic su un kanji dell'headword nel popup. Risultato: carattere, onyomi, kunyomi, significati, tratti, radicale, statistiche, frequenze. |
| LKP-038 | Cache LRU dei risultati (5000 voci, chiave = primi 16 code point + l'insieme di dizionari attivi); si invalida quando cambia un dizionario. |

#### 6.6.5 Frequenze e accento

| ID | Requisito |
|---|---|
| LKP-040 | Frequenze: un chip per dizionario `Dizionario: v1, v2` (preferisce `displayValue`). Valore armonico = `floor(n / Σ 1/vᵢ)` sul primo valore > 0 di ogni dizionario; senza valori = `9999999`. |
| LKP-041 | Accento: modo `pitch` (`position` intero o pattern, `nasal`, `devoice`) e modo `ipa`. Divisione in more (i kana piccoli si attaccano alla mora precedente), alto/basso per mora come `isMoraPitchHigh` di Yomitan, linea del downstep ed etichetta `[n]`. A differenza di Fushi, mostra anche nasale e devoicing. |
| LKP-042 | Categoria dell'accento: heiban / atamadaka / nakadaka / odaka / kifuku (`v*` e `adj-i` = kifuku quando la posizione è > 0). |
| LKP-043 | Deduplicazione degli accenti tra dizionari (predefinita sì): payload identici diventano una riga con l'indicazione "accento · N". |

#### 6.6.6 Furigana

| ID | Requisito |
|---|---|
| LKP-050 | `distribute_furigana(espressione, lettura)`, come `distributeFurigana`/`segmentizeFurigana` di Yomitan: divide in gruppi kana e kanji e allinea la lettura; se l'allineamento è ambiguo, la lettura va sull'intera espressione. |
| LKP-051 | Il furigana automatico sul testo dei sottotitoli è post-1.0; per farlo serve la segmentazione greedy di LKP-060. |

#### 6.6.7 Segmentazione

| ID | Requisito |
|---|---|
| LKP-060 | Segmentazione delle parole senza tokenizzatore morfologico: longest-match greedy con `lookup(max_results = 1)` (come Fushi `japanese_language.dart`). Si usa per la "parola successiva/precedente" con il cursore da tastiera e, in futuro, per evidenziare le parole già minate. Nessun dizionario MeCab/IPADIC (licenze). |

#### 6.6.8 Differenze volute rispetto a Fushi

Tag bank importati (DIC-002) · frequenze stringa e `frequencyMode` (DIC-003) · preprocessori per lingua (LKP-012) · priorità del dizionario nell'ordinamento (LKP-034) · nasale e devoicing mostrati (LKP-041). Ognuna ha un test dedicato in [TEST.md](TEST.md).

### 6.7 Popup del dizionario (POP)

| ID | Requisito | Fushi |
|---|---|---|
| POP-001 | Clic su un carattere del sottotitolo → se il video era in riproduzione si mette in pausa e si ricorda `paused_for_lookup` → si apre il popup senza aspettare che la pausa sia confermata. | `video_fushi_page.dart` |
| POP-002 | Il testo trovato (`best_length` grafemi) si evidenzia nel sottotitolo. | |
| POP-003 | Hover con Shift (soglia di movimento 8 px; non si ripete sullo stesso carattere). Opzione `lookup.hover_without_modifier` (predefinita no). Il lookup via hover mette in pausa come il clic. Se il puntatore esce da sottotitolo e popup per 320 ms, il popup si chiude e la riproduzione riprende (`lookup.resume_on_leave`, predefinita sì). | |
| POP-004 | Quando tutti i popup sono chiusi, la riproduzione riprende **solo** se l'ha messa in pausa il lookup (BUG-072). | |
| POP-005 | Con un popup aperto, un clic su un altro carattere del sottotitolo o della lista sostituisce la voce e resta in pausa. | |
| POP-006 | Posizione: sopra il sottotitolo se c'è spazio, altrimenti sotto. Non copre mai la parola cercata e si restringe per stare nella finestra. Dimensioni massime configurabili; ridimensionabile trascinando il bordo. | `dictionary_popup_layer.dart` |
| POP-007 | Intestazione di ogni voce: headword con furigana (LKP-050), pulsante audio ♪ (AUD), pulsante crea carta (+ / ✓ / ✓↩, ANK-030), chip della traccia di deinflessione con descrizione al passaggio del mouse, riga delle frequenze, riga dell'accento, chip dei tag del termine. | `popup.js` |
| POP-008 | Un blocco `<details>` per dizionario, con stato compresso secondo DIC-034. I tag di definizione appaiono come chip: i tag numerici si nascondono, il tag di parte del discorso ripetuto su sensi consecutivi si omette. | |
| POP-009 | **Contenuto strutturato** Yomitan: si converte in elementi Dioxus da una lista di tag ammessi (`br ruby rt rp table thead tbody tfoot tr td th span div ol ul li details summary img a`), con classe `gloss-sc-<tag>`, attributi `data-sc-*`, `lang`, e una lista di proprietà `style` ammesse. **Nessun HTML grezzo** dai dizionari Yomitan e nessuno script. Tabelle con contenitore scorrevole. | `renderStructuredContent` |
| POP-010 | Immagini dei dizionari: protocollo personalizzato `anikoto-dict://<id>/<percorso>` servito dalla tabella `media`; rispetta `width`, `height`, `sizeUnits`, `imageRendering`, `collapsed`. | |
| POP-011 | Link interni (`?query=` o testo del link senza `<rt>`, BUG-2456) → nuovo lookup in un **popup annidato**. Link esterni → browser di sistema (`xdg-open`). | |
| POP-012 | Popup annidati: si seleziona il testo dentro un glossario e si apre un popup figlio. Esc o Indietro chiudono l'ultimo; un clic sul genitore chiude i figli; un clic fuori chiude tutto. Profondità massima 8. | |
| POP-013 | Voci kanji (LKP-037) mostrate con una scheda dedicata. | |
| POP-014 | Navigazione da tastiera: Tab/Shift+Tab tra le voci, Enter = audio, Ctrl+Enter = crea carta, Esc = chiudi. Alt+rotellina = voce successiva o precedente. | |
| POP-015 | "Ripeti la frase" e "vai alla frase" dal popup (SUB-013). | |

### 6.8 Audio delle parole (AUD)

| ID | Requisito | Fushi |
|---|---|---|
| AUD-001 | Sorgenti audio in un elenco ordinato e configurabile; ognuna si può abilitare o disabilitare. | `audio_source_config.dart` |
| AUD-002 | Sorgente **URL template** con `{term}` e `{reading}` codificati come URI. La risposta può essere JSON Yomitan `{type:"audioSourceList", audioSources:[{name,url}]}` oppure, a differenza di Fushi, audio diretto (`Content-Type: audio/*`). | `word_audio_resolver.dart` |
| AUD-003 | Sorgente **database locale** nel formato "Local Audio Server for Yomichan" (`entries(expression, reading, file, source)`, `android(file, source, data)`): prima la coppia esatta, poi solo l'espressione, con priorità per sotto-sorgente (nhk16, forvo, jpod, …). Il formato è SQLite: si legge con `turso` (MIT), da validare nello spike S3 (D-2). | `local_audio_db.dart` |
| AUD-004 | Risoluzione: per la riproduzione basta il primo risultato valido in ordine; il menu (clic destro su ♪) interroga tutte le sorgenti in parallelo e le mostra tutte. Timeout: 8 s per la connessione, 10 s per la risposta. Una risposta 4xx = "nessun audio"; un errore di connessione mette l'host in pausa per 45 s. | |
| AUD-005 | La riproduzione usa una **seconda istanza libmpv solo audio** (`vid=no`, `video=no`), indipendente dal player. Non si usa l'audio di WebKit/GStreamer. Volume configurabile. | |
| AUD-006 | Opzione "riproduci automaticamente all'apertura del popup", predefinita sì: non si sovrappone al dialogo perché il lookup mette già in pausa il video (POP-001). Si può disattivare. | |
| AUD-007 | Non ci sono URL predefiniti di JapanesePod101 o Forvo (termini di servizio). L'utente aggiunge i propri template. | |
| AUD-008 | Cache su disco dell'audio scaricato (chiave = URL), limitata a 500 MiB con politica LRU. | |

### 6.9 Mining verso Anki (ANK)

#### 6.9.1 AnkiConnect

| ID | Requisito | Fushi |
|---|---|---|
| ANK-001 | Endpoint predefinito `http://127.0.0.1:8765`, configurabile (host, porta, HTTPS, chiave API). Corpo della richiesta: `{"action", "version": 6, "params"?, "key"?}`; `key` solo se non vuota. Timeout: 10 s per richiesta, 5 s per la connessione. | `ankiconnect_service.dart` |
| ANK-002 | Risposta: un HTTP diverso da 200, un corpo non JSON o un corpo senza `result`/`error` → errore di protocollo. Un `error` non nullo → errore. Per `addNote`, il testo `cannot create note because it is a duplicate` → errore di duplicato dedicato. | |
| ANK-003 | Retry: un solo nuovo tentativo, se la connessione in pool era scaduta. `addNote` e `createModel` **non sono idempotenti**: si ritentano solo se è fallita la fase di connessione. Altrimenti l'errore è "esito sconosciuto, controlla in Anki", senza ripetere. | |
| ANK-004 | Codici di errore mostrati all'utente: connessione rifiutata, timeout (porta probabilmente occupata da un altro programma), errore HTTP, mapping dei campi non valido, primo campo vuoto, Lapis mancante. | |
| ANK-005 | Azioni usate: `version`, `deckNames`, `modelNames`, `modelFieldNames`, `addNote`, `canAddNotesWithErrorDetail`, `findNotes`, `notesInfo`, `updateNoteFields`, `addTags`, `storeMediaFile`, `getMediaFilesNames`, `deleteMediaFile`, `createModel`, `createDeck`, `guiBrowse`, `multi`. | |
| ANK-006 | Aggiorna configurazione: si chiamano `version`, `deckNames`, `modelNames` e `modelFieldNames` per il modello selezionato. La selezione si conserva per nome. | |

#### 6.9.2 Nota Lapis

| ID | Requisito |
|---|---|
| ANK-010 | Template Lapis vendorizzati (front, back, CSS) in `assets/anki/lapis/`, con versione fissata (Fushi usa la v1.7.0). Campi, in ordine: Expression, ExpressionFurigana, ExpressionReading, ExpressionAudio, SelectionText, MainDefinition, DefinitionPicture, Sentence, SentenceFurigana, SentenceAudio, Picture, Glossary, Hint, IsWordAndSentenceCard, IsClickCard, IsSentenceCard, IsAudioCard, PitchPosition, PitchCategories, Frequency, FreqSort, MiscInfo. |
| ANK-011 | "Crea mazzo Lapis": `createModel` se non esiste un modello con nome "Lapis"; `createDeck("Lapis")`; poi si aggiorna la configurazione e si verifica che entrambi esistano; si selezionano e si applica il mapping predefinito. È idempotente. Esito: `creato` / `già esistente` / `fallito`. |
| ANK-012 | Mapping predefinito di Lapis: Expression `{expression}` · ExpressionFurigana `{furigana-plain}` · ExpressionReading `{reading}` · ExpressionAudio `{audio}` · MainDefinition `{glossary-first}` · Sentence `{sentence}` · SentenceAudio `{sentence-audio}` · Picture `{card-image}` · Glossary `{glossary}` · PitchPosition `{pitch-accent-positions}` · PitchCategories `{pitch-accent-categories}` · Frequency `{frequencies}` · FreqSort `{frequency-harmonic-rank}` · MiscInfo `{source-link} {clip-timestamp}` · IsWordAndSentenceCard `x`. |
| ANK-013 | Riconoscimento Lapis: il nome contiene "lapis" (senza distinguere maiuscole), oppure i campi includono Expression, MainDefinition e Sentence. |

#### 6.9.3 Composizione dei campi

| ID | Requisito |
|---|---|
| ANK-020 | Ogni campo della nota ha un template con segnaposto `{nome}` (regex `\{[^}]*\}`); un segnaposto sconosciuto diventa una stringa vuota. Il mapping si salva per tipo di nota. |
| ANK-021 | Segnaposto da implementare: `{expression}` `{reading}` `{furigana-plain}` `{glossary}` `{glossary-first}` `{single-glossary-<dizionario>}` `{sentence}` `{cue-sentence}` `{frequencies}` `{frequency-harmonic-rank}` `{pitch-accent-positions}` `{pitch-accent-categories}` `{phonetic-transcriptions}` `{audio}` `{sentence-audio}` `{card-image}` `{document-title}` `{source-link}` `{clip-timestamp}` `{tags}`. Il formato HTML di ognuno deve coincidere con quello di Fushi `anki_note_composer.dart` e `popup.js` (vettori di test in T-ANK-02x). |
| ANK-022 | `{furigana-plain}`: un gruppo di kanji diventa ` 漢字[かんじ]`, un gruppo di kana resta uguale; il risultato si rifila (es. `食[た]べ 物[もの]`). |
| ANK-023 | `{glossary}`: `<div style="text-align: left;" class="yomitan-glossary"><ol>`, poi un `<li data-dictionary="D"><i>(tag, D)</i> <span>…</span></li>` per ogni senso. L'etichetta `(tag, D)` compare solo sul primo senso di ogni dizionario, i tag di parte del discorso ripetuti si tolgono, i dizionari disabilitati e i rinvii si saltano. Il CSS del dizionario si aggiunge come `<style>` limitato a `.yomitan-glossary [data-dictionary="D"]`. |
| ANK-024 | `{sentence}`: il cue (o i cue uniti, ANK-041) con la parola cercata racchiusa in `<b>`, alla posizione esatta dell'offset del clic (oppure alla prima occorrenza, se l'offset non è valido). |
| ANK-025 | `{frequency-harmonic-rank}` come LKP-040. `{frequencies}`: `<ul style="text-align: left;"><li>D: valore</li>…</ul>`. |
| ANK-026 | Campi vuoti: alla creazione si omettono. Il primo campo del tipo di nota non può essere vuoto (errore dedicato). Si inviano solo i campi che esistono davvero nel tipo di nota. |
| ANK-027 | Tag: tag dell'utente + `anikoto` + `video` + titolo della serie ripulito (spazi → `_`). Deduplicati, nell'ordine. |

#### 6.9.4 Flusso di mining dal video

| ID | Requisito |
|---|---|
| ANK-030 | Stato del pulsante nel popup: `+` (si può aggiungere), `✓` (esiste già: il clic apre un menu con sovrascrivi / apri in Anki / aggiungi comunque), `✓↩` (ultima carta di questa sessione, sovrascrivibile). Il controllo usa `canAddNotesWithErrorDetail` sul primo campo, con le stesse `duplicateScope` e `duplicateScopeOptions` di `addNote`; se AnkiConnect è vecchio, si ripiega su `findNotes`. Dopo un errore di trasporto c'è una pausa di 30 s per tutto il processo. |
| ANK-031 | Il cue di riferimento si sceglie in quest'ordine: quello da cui è partito il lookup, quello corrente, quello trovato per posizione. |
| ANK-032 | Intervallo audio: `[start − pad_testa, end + pad_coda]` (predefiniti 120 ms e 200 ms, massimo 1000 ms ciascuno), limitato dai cue adiacenti, poi spostato di `+delay_ms`. Durata massima 30 s. |
| ANK-033 | Audio della frase: `ffmpeg -y -ss S -t D -i file:IN -vn -sn -map_chapters -1 -map 0:<ff-index della traccia audio attiva> -c:a aac -ac 1 -b:a 64k out.m4a`. Livelli di qualità: mono 64k (predefinito), stereo 128k, stereo 192k. Formati alternativi: Opus/OGG, MP3. |
| ANK-034 | Immagine: il fotogramma esatto del momento del clic, catturato da mpv (`screenshot-to-file <tmp>.png video`), poi ridimensionato e codificato con ffmpeg. Livelli: lato lungo 720 px / q80, **1000 px / q90 (predefinito)**, 2000 px / q95, originale. Formati: JPEG (predefinito), WebP. Modalità alternativa: fotogramma all'inizio del cue. |
| ANK-035 | Audio ed immagine si estraggono in parallelo. Se manca l'audio, la carta si annulla con un messaggio (come Fushi). |
| ANK-036 | Media su Anki: nome per contenuto `anikoto_<sha256[0..16]>.<ext>`. Se l'host è loopback si usa `storeMediaFile` con `path`; altrimenti `data` in base64. Se `addNote` fallisce, si cancellano i media caricati in quel tentativo. |
| ANK-037 | Sovrascrivere l'ultima carta: `notesInfo`, rifiuto se il tipo di nota è diverso, poi `updateNoteFields` con **tutti** i campi (quelli non mappati diventano `''`) e `addTags`. Ambito predefinito: solo l'ultima carta minata nella sessione. |
| ANK-038 | "Apri in Anki": `guiBrowse` con `nid:<id>`; per una parola, `findNotes` sul primo campo e poi `guiBrowse` con gli id trovati. |
| ANK-039 | Ogni carta creata si registra in `mined_cards` (id nota, espressione, lettura, frase, uid del video, start/end, `source_id`, data) per le statistiche e per evidenziare "già minato" in futuro. |
| ANK-040 | Dialogo "mining avanzato" (Shift+clic su +): modifica di start/end con anteprima, scelta del fotogramma, esclusione di audio o immagine. Milestone M6. |
| ANK-041 | Aggiungere i cue precedenti o successivi alla frase (unendo testo e intervallo). Milestone M6. |
| ANK-042 | Clip video WebM e copertine animate (AVIF/WebP/GIF): post-1.0. |

#### 6.9.5 Ritorno alla scena

| ID | Requisito |
|---|---|
| ANK-050 | `{source-link}` = `<a href="anikoto://source?v=1&kind=video&uid=…&ep=…&startMs=…&endMs=…&sourceId=<uuid-v4>">Titolo</a>` (titolo con escape HTML). |
| ANK-051 | Il parsing è rigoroso: parametri sconosciuti o duplicati, percorsi assoluti e uid `file:` si rifiutano; lunghezza massima 16 KiB. |
| ANK-052 | Aprendo il link (APP-001/002), si apre il video, si va a `startMs` in pausa e si evidenzia il cue. Se il file non si trova (uid sconosciuto o file spostato), si mostra un messaggio con l'opzione "cerca il file". |
| ANK-053 | `{clip-timestamp}` = `HH:MM:SS - HH:MM:SS`, solo se `end > start`. |

### 6.10 Statistiche (STA)

| ID | Requisito | Fushi |
|---|---|---|
| STA-001 | Il tempo di visione conta ogni intervallo del video **una sola volta** (copertura per intervalli). Campionamento ogni 1 s; un salto maggiore di `tempo_reale × velocità × 1,25 + 500 ms` è un seek e non conta. La copertura si salva per video: il totale non supera mai la durata (BUG-2108). | `video_watch_tracker.dart`, `interval_coverage.dart` |
| STA-002 | I caratteri dei sottotitoli si contano una volta per cue, dopo almeno `min(1500 ms, durata del cue)` di riproduzione effettiva in quel cue. I cue già contati in sessioni precedenti non si contano di nuovo (BUG-1763). | |
| STA-003 | Regola di conteggio dei caratteri: Han, kana, Hangul, thai e scritture simili = 1 per code point; una sequenza di lettere latine o cifre = 1; segni combinanti e apostrofi = 0. | `study_char_count.dart` |
| STA-004 | Sessioni (`study_segments`): orologio con tick di 60 s, una pausa > 120 s chiude la sessione, segmenti divisi a ogni ora. Per ogni segmento: media, inizio, fine, data, durata, caratteri, lookup, carte. | `study_clock.dart` |
| STA-005 | UI: calendario con mappa di calore, grafico per intervallo (7/30/365 giorni), totale per serie, elenco delle sessioni con eliminazione. Export CSV. | `video_statistics_page.dart` |

### 6.11 Impostazioni, aspetto, lingue (SET)

| ID | Requisito |
|---|---|
| SET-001 | Pagina impostazioni con sezioni: Generale, Player, Sottotitoli, Dizionari, Lookup, Audio parole, Anki, Scorciatoie, Avanzate (opzioni mpv, percorsi ffmpeg, log), Informazioni (versione, licenze, `THIRD_PARTY_LICENSES`). |
| SET-002 | Le modifiche si applicano subito, senza riavviare, salvo dove indicato (es. `hwdec` vale dal file successivo). |
| SET-003 | Tema chiaro / scuro / come il sistema (`prefers-color-scheme` e impostazione GTK), con colore di accento configurabile. Token di design come variabili CSS in un solo foglio di stile. |
| SET-004 | i18n con Fluent (`assets/i18n/<locale>/*.ftl`). Nessuna stringa visibile scritta nel codice. Lingua dell'interfaccia: sistema (`LANG`) con ripiego su inglese; si può cambiare dalle impostazioni. |
| SET-005 | Scala dell'interfaccia 75–200% (zoom della webview), indipendente dalla dimensione dei sottotitoli. |
| SET-006 | Ripristino delle impostazioni predefinite per sezione. |
| SET-007 | Proxy HTTP(S) configurabile; si usa per tutte le richieste di rete tranne AnkiConnect su loopback. |

### 6.12 Sottotitoli online e metadati (MET) — milestone M7

| ID | Requisito | Fushi |
|---|---|---|
| MET-001 | Metadati da **AniList** (GraphQL, senza chiave): titolo romaji/nativo/inglese, copertina, episodi, `anilist_id`. Identificazione dal titolo della serie ottenuto da LIB-003; la si accetta in automatico solo con una corrispondenza esatta e unica, altrimenti si chiede all'utente di scegliere. | — (Fushi usa Jikan/TMDB) |
| MET-002 | **Jikan v4** (MAL) come fonte alternativa: limite di 1 richiesta/s, cache, pausa dopo un 429. | `mal_video_metadata_provider.dart` |
| MET-003 | **Jimaku**: `https://jimaku.cc/api`, header `Authorization: <chiave>`. `GET /entries/search?anilist_id=…` (con ripiego su `query=`), `GET /entries/{id}/files?episode=n`, download, archivi zip estratti con le regole di DIC-011. Si deduce episodio e lingua dal nome del file. | `jimaku_client.dart` |
| MET-004 | **OpenSubtitles** (chiave dell'utente): `moviehash` + `moviebytesize`, poi `query` + `season_number`/`episode_number`; file ≤ 64 MiB. | `open_subtitles_client.dart` |
| MET-005 | Le chiavi API si salvano nella config con permessi 0600 (il keyring è nel backlog della ROADMAP). | |
| MET-006 | Identificazione dei file con AniDB ed2k: post-1.0 (serve registrare un client AniDB). | `anidb_ed2k.dart` |

---

## 7. Requisiti non funzionali (NFR)

**Hardware di riferimento** (RIF): x86_64, 4 core / 8 thread (2020 o successivo), GPU integrata Intel o AMD con VA-API, 16 GB di RAM, SSD NVMe, display 1920×1080 a 60 Hz, sessione Wayland (GNOME o KDE).

| ID | Requisito | Misura (vedi TEST) |
|---|---|---|
| NFR-PERF-01 | Avvio a freddo fino alla libreria visibile ≤ 1,5 s su RIF | T-PERF-01 |
| NFR-PERF-02 | Apertura di un file locale fino al primo fotogramma ≤ 1,0 s (h264/hevc, file mkv da 1 GB) | T-PERF-02 |
| NFR-PERF-03 | Riproduzione 1080p60 H.264 e HEVC 10-bit con overlay visibile (sottotitoli + controlli): **0 fotogrammi persi** in 5 minuti (`frame-drop-count`, `vo-delayed-frame-count`), CPU del processo ≤ 25% di un core con hwdec attivo | T-PERF-03 (criterio di uscita dello spike S1) |
| NFR-PERF-04 | Lookup nel motore (scansione di 16 code point, 5 dizionari term + 3 di frequenza + 1 di accento) p95 ≤ 10 ms, p99 ≤ 25 ms | T-PERF-04 |
| NFR-PERF-05 | Import di un dizionario di 300k voci / 150 MB JSON ≤ 60 s, picco di RAM ≤ 1 GiB | T-PERF-05 |
| NFR-PERF-06 | Dal clic sul carattere al popup disegnato ≤ 100 ms p95 | T-PERF-06 |
| NFR-PERF-07 | Un cue appare entro ≤ 50 ms dal suo `start` effettivo | T-PERF-07 |
| NFR-PERF-08 | Carta creata (audio 5 s + immagine) ≤ 3 s dal clic, con Anki locale | T-PERF-08 |
| NFR-PERF-09 | RAM (RSS) a riposo con la libreria aperta ≤ 400 MB, WebKit incluso | T-PERF-09 |
| NFR-REL-01 | Nessun crash né perdita di dati se si chiude l'app durante: import, scrittura della config, salvataggio del progresso, mining | T-REL-001, T-REL-002 |
| NFR-REL-02 | Un file video, un sottotitolo o un dizionario corrotto o malevolo non fa crashare l'app (fuzzing dei parser) | T-FUZZ-* |
| NFR-SEC-01 | Contenuti dei dizionari ed estratti dei sottotitoli trattati come dati non fidati: nessuna esecuzione di script, nessun HTML grezzo (POP-009), CSP della webview restrittiva e compatibile con Dioxus | T-SEC-* |
| NFR-SEC-02 | Nessun file scritto fuori dalle directory XDG di Anikoto e da quelle scelte dall'utente (screenshot, export) | T-SEC-03 |
| NFR-A11Y-01 | Tutta l'interfaccia si usa da tastiera, con focus sempre visibile; contrasto AA nei temi predefiniti; attributi ARIA sui controlli del player | T-A11Y-* |
| NFR-PORT-01 | Funziona su Wayland e X11; la build non richiede rete a parte `cargo fetch` | T-PORT-001, T-PORT-002 |
| NFR-MAINT-01 | `cargo clippy --workspace --all-targets -- -D warnings` e `cargo fmt --check` puliti; documentazione `///` su ogni elemento `pub` dei crate di libreria | T-MAINT-001 |
| NFR-MAINT-02 | Copertura delle righe ≥ 80% per `anikoto-text`, `anikoto-subs`, `anikoto-dict`, `anikoto-anki` (misurata con `cargo llvm-cov`, licenza MIT/Apache) | T-MAINT-002 |

---

## 8. Modello dati (library.redb)

| Tabella | Chiave | Valore (campi principali) |
|---|---|---|
| `meta` | `&str` | `schema_version`, `created_with` |
| `series` | `u64` | titolo, titolo nativo, `anilist_id?`, `mal_id?`, copertina, modalità di raggruppamento, aggiunta il, eliminata (tombstone, per LIB-004), preferenze (audio, sottotitolo, `delay_ms`) |
| `videos` | `&str` uid | `series_id?`, percorso, size, mtime, stagione, episodio, titolo, durata, `last_position_ms`, `last_played_at`, `completed_at`, audio scelto, `SubtitleChoice` principale e secondaria, `delay_ms?`, `secondary_delay_ms?`, `speed?`, `volume?` |
| `watch_coverage` | `&str` uid | intervalli `[(start_ms, end_ms)]` uniti |
| `cue_credit` | `&str` uid | bitset degli indici dei cue già contati + hash del file di sottotitoli |
| `study_segments` | `u128` uuid | uid del media, inizio, fine, `date_key`, ora, durata, caratteri, lookup, carte |
| `mined_cards` | `u64` id nota | espressione, lettura, frase, uid del media, start, end, `source_id`, data |
| `dictionaries` | `&str` id | titolo, revisione, tipo, ordine, abilitato, lingua, `display_name`, stato di compressione, CSS utente, conteggi |
| `subtitle_cache` | chiave di SUB-007 | percorso del file estratto, formato, codifica |

`SubtitleChoice = File(path) | Embedded { ff_index, lang, title } | Online { provider, id, path } | Off`

---

## 9. Configurazione (estratto di `config.toml`)

```toml
[general]
ui_language = "system"      # "it", "en", …
theme = "system"            # "light" | "dark" | "system"
study_language = "ja"

[player]
hwdec = "auto-safe"
quality_profile = "default" # "default" | "high-quality"
seek_seconds = 3
auto_next = true
raw_mpv_options = ""

[subtitles]
preferred_languages = ["ja", "jpn"]
font_size = 36
respect_ass_styles = false
obscure = "none"            # "none" | "blur" | "hidden"

[lookup]
max_results = 10
hover_without_modifier = false
resume_on_leave = true

[anki]
url = "http://127.0.0.1:8765"
api_key = ""
deck = "Lapis"
note_type = "Lapis"
duplicate_scope = "deck"    # "deck" | "collection"
audio_pad_head_ms = 120
audio_pad_tail_ms = 200
audio_format = "aac-m4a"    # "aac-m4a" | "opus-ogg" | "mp3"
image_format = "jpeg"       # "jpeg" | "webp"
image_quality_level = 1     # 0..3
tags = []

[anki.field_mapping.Lapis]
Expression = "{expression}"
Sentence = "{sentence}"
# …

[paths]
ffmpeg = ""                 # vuoto = cerca in PATH
```

---

## 10. Decisioni e rischi

### 10.1 Decisioni del maintainer

Approvate dal maintainer il 2026-10-19. Una decisione si riapre solo con una modifica esplicita di questa tabella.

| ID | Decisione | Esito |
|---|---|---|
| D-1 | Deroghe di classe B (§3.2), necessarie per usare Dioxus desktop | **Approvata**: elenco chiuso; la CI blocca ogni aggiunta |
| D-2 | Lettura del DB audio locale (SQLite, pubblico dominio) | **Approvata** l'opzione (a): `turso` (MIT, riscrittura di SQLite in Rust), da validare nello spike S3. Se S3 fallisce, la decisione si riapre tra (b) deroga per la libreria SQLite di sistema e (c) rinuncia ad AUD-003 |
| D-3 | GPL-3.0-only oppure GPL-3.0-or-later | **Approvata**: GPL-3.0-only (§3.1) |
| D-4 | Formato audio predefinito delle carte | **Approvata**: AAC in M4A, compatibile con Anki desktop, AnkiDroid e AnkiMobile |
| D-5 | Pacchetti per la 1.0 | **Approvata**: PKGBUILD (AUR) + Flatpak (runtime GNOME, con libmpv e ffmpeg inclusi nel manifest) |
| D-6 | Nome dell'app, della cartella e del repository | **Approvata**: l'app si chiama **Anikoto**; la cartella locale (oggi "Amikoto") e il repository si rinominano in `anikoto` |

### 10.2 Rischi

| ID | Rischio | Prob. | Impatto | Mitigazione |
|---|---|---|---|---|
| R-1 | La composizione webview trasparente + GLArea (§5.2) non rende in modo fluido, specie su Wayland con DMA-BUF disattivato in WebKit (`with_disable_dma_buf_on_wayland`, predefinito in Dioxus) | Media | Alto | Spike S1 in M0 con criteri misurabili; ripieghi B/C/D |
| R-2 | tao/wry/Dioxus restano su GTK3 (gtk-rs per GTK3 non è più mantenuto) oppure migrano a GTK4 e rompono la riparentatura | Media | Medio | Isolare in `anikoto-app::linux::compositor`; fissare le versioni; test di smoke a ogni aggiornamento |
| R-3 | Una nuova versione di Dioxus aggiunge dipendenze con licenze nuove | Media | Medio | `cargo deny` nella CI; aggiornamenti solo con PR dedicata |
| R-4 | ffmpeg di sistema senza encoder (es. `aac` assente in build ridotte) | Bassa | Medio | EXT-001: verifica all'avvio, ripiego su Opus/MP3, messaggio chiaro |
| R-5 | Prestazioni di redb non sufficienti per il lookup | Bassa | Medio | Spike S2; trait `DictStore` e formato su misura come piano B |
| R-6 | AnkiConnect cambia o rimuove `canAddNotesWithErrorDetail` | Bassa | Basso | Ripiego su `findNotes` (ANK-030) |
| R-7 | Il volume di Fushi spinge ad allargare l'ambito | Alta | Alto | §1.3 vincolante; ogni nuova funzione passa da una modifica di questo documento |

---

## 11. Glossario

| Termine | Significato |
|---|---|
| Cue | Singola battuta di sottotitolo, con un inizio e una fine |
| Lookup | Ricerca nel dizionario a partire da un punto del testo |
| Deinflessione | Riportare una forma flessa (食べました) alla forma del dizionario (食べる) |
| Mining | Creare una carta Anki da una parola incontrata, con il suo contesto |
| Lapis | Tipo di nota Anki molto diffuso per il giapponese (donkuri/lapis) |
| AnkiConnect | Add-on di Anki (codice 2055492159) che espone un'API HTTP locale |
| Structured content | Formato JSON di Yomitan per glossari ricchi (tabelle, ruby, immagini) |
| hwdec | Decodifica video hardware di mpv |
| Spike | Prototipo a tempo limitato per ridurre un rischio tecnico; il codice si butta |
