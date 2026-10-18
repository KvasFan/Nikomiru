# Anikoto — Piano di test

| Campo | Valore |
|---|---|
| Stato | Bozza v0.1 |
| Data | 2026-10-19 |
| Documenti collegati | [SPECIFICHE.md](SPECIFICHE.md) · [ROADMAP.md](ROADMAP.md) |

## 1. Principi

1. **Ogni requisito ha almeno un test.** Il test porta l'ID del requisito (`T-SUB-012` verifica `SUB-012`). La copertura la controlla `cargo xtask trace` (§8).
2. **Logica pura senza display.** Testo, sottotitoli, dizionari, Anki e libreria si testano senza GTK, webview né mpv (SPECIFICHE ARC-010). È qui che si concentra la maggior parte dei test.
3. **Deterministici e offline.** La suite predefinita non usa la rete, non dipende dall'orologio reale (orologio iniettabile in `anikoto-core`) e non dipende da file fuori dal repository. I test con rete, dati reali o Anki reale sono separati e opzionali (§2, livelli L5–L7).
4. **Fushi è l'oracolo.** Dove esiste un test Fushi, i suoi vettori di input e output si portano come test di conformità (§5). Ogni bug Fushi citato in SPECIFICHE diventa un test di regressione.
5. **Ogni bug di Anikoto produce un test** prima della correzione (§7).
6. **Gli strumenti di test rispettano la politica di licenze** (SPECIFICHE §3.5), anche se non vengono distribuiti.

---

## 2. Livelli

| Livello | Cosa | Dove | Quando gira |
|---|---|---|---|
| L1 Unit | Funzioni e tipi, casi puntuali | `#[cfg(test)]` nel crate | Ogni push |
| L2 Property | Invarianti con input generati (`proptest`) | `#[cfg(test)]`, `tests/` | Ogni push (256 casi); di notte 100k casi |
| L3 Snapshot | Output strutturati confrontati con file approvati (`insta`): JSON del popup, HTML dei campi Anki, argv di ffmpeg, componenti Dioxus resi con `dioxus-ssr` | `tests/snapshots/` | Ogni push |
| L4 Integrazione | Crate contro dipendenze reali locali: ffmpeg, filesystem, redb, AnkiConnect finto (`wiremock`) | `crates/*/tests/` | Ogni PR |
| L5 Integrazione mpv | libmpv headless (`vo=null`, `ao=null`) sulle fixture generate | `crates/anikoto-player/tests/` | Ogni PR |
| L6 GUI end-to-end | App vera pilotata dal **test harness** (§2.1), sotto Xvfb (X11) e Weston headless (Wayland), con Mesa llvmpipe | `tests/e2e/` | Ogni PR (smoke); di notte (completo) |
| L7 Esterni | Anki desktop reale con AnkiConnect; servizi online reali (AniList, Jimaku); dati reali dell'utente (JMdict, …) | `#[ignore]` + `cargo xtask test --anki-e2e / --live / --realdata` | Di notte o a mano |
| L8 Performance | Benchmark `criterion` e metriche dal harness, con soglie (§6.15) | `benches/`, `tests/e2e/perf_*` | Di notte |
| L9 Manuale | Checklist esplorative di milestone e di rilascio (§9) | questo documento | Fine milestone, rilascio |

### 2.1 Test harness della GUI

- Feature Cargo `test-harness` di `anikoto-app`, **mai attiva nelle build di rilascio** (verificato da T-SEC-004).
- Avvio: `anikoto --test-harness <socket>`. Protocollo: righe JSON su un socket Unix.
- Comandi:
  - `open {path}`, `wait {event, timeout_ms}`, `key {code, mods}`;
  - `click_grapheme {layer, cue_index, grapheme_index}`, `hover_grapheme {…}`, `click {selector}`;
  - `state {}`: stato del player, popup aperti, cue visibili, toast;
  - `mpv_get {property}`: solo lettura, per le metriche;
  - `quit {}`.
- Eventi: `library-ready`, `file-loaded`, `first-frame`, `cue-shown {index, time_pos}`, `popup-rendered {depth}`, `card-created {note_id}`, `error {code}`.
- Il harness interagisce attraverso gli stessi gestori di eventi della UI: non scorciatoie interne che saltano la logica da testare.

---

## 3. Strumenti

| Strumento | Licenza | Uso |
|---|---|---|
| `cargo test`, `cargo-nextest` | MIT/Apache · Apache-2.0 OR MIT | Esecuzione dei test |
| `proptest` | MIT OR Apache-2.0 | L2 e fuzzing strutturato |
| `insta`, `cargo-insta` | Apache-2.0 | L3 |
| `dioxus-ssr` | MIT OR Apache-2.0 | Snapshot HTML dei componenti |
| `wiremock` | MIT/Apache-2.0 | AnkiConnect finto, servizi online finti |
| `tempfile` | MIT OR Apache-2.0 | Directory XDG isolate per ogni test |
| `criterion` | Apache-2.0 OR MIT | L8 |
| `cargo-llvm-cov` | Apache-2.0 OR MIT | Copertura (NFR-MAINT-02) |
| `cargo-deny`, `cargo-about` | MIT OR Apache-2.0 | Licenze (T-LIC-*) |
| Xvfb, Weston, Mesa llvmpipe | MIT/X11 | Display headless per L6 |
| ffmpeg, ffprobe, libmpv | LGPL/GPL | Generazione e verifica delle fixture |
| Anki desktop + AnkiConnect | AGPL-3.0 / GPL | **Solo come programmi esterni** in L7: non si collegano né si distribuiscono |

**Escluso**: `cargo-fuzz`/`libfuzzer-sys`, perché la licenza include NCSA (fuori lista). Il fuzzing guidato dalla copertura con `afl` (AFL++, Apache-2.0) si può adottare solo dopo un controllo `cargo deny`. Fino ad allora il fuzzing si fa con `proptest` su byte arbitrari e input strutturati (T-FUZZ-*).

---

## 4. Fixture

Tutte le fixture sono **create dal progetto** (GPL-3.0) oppure **generate**. Nessun dizionario, video o sottotitolo di terzi entra nel repository.

### 4.1 Media generati (`cargo xtask fixtures`)

Si generano con ffmpeg `lavfi` in `target/fixtures/`, con flag `+bitexact` e senza metadati. I test verificano proprietà (durate, tracce, toni), non byte esatti.

| File | Contenuto | Usato da |
|---|---|---|
| `av_basic.mkv` | 12 s, 1280×720@30 `testsrc2`, video `mpeg4` (encoder nativo). **Audio 1** `jpn`: silenzio con toni da 1 kHz in [2,0–2,5], [5,0–5,5], [8,0–8,5] s. **Audio 2** `eng`: toni da 660 Hz agli stessi istanti. **Sottotitolo 0** SRT `jpn` con cue allineati ai toni (`食べました`, `日本語を勉強しています。`, `ラーメンが好き`). **Sottotitolo 1** ASS `jpn` con stili, `\an8` e `\pos`. 3 capitoli | PLY, SUB, ANK, STA |
| `av_noaudio.mp4` | 5 s di solo video | ANK-035, PLY-012 |
| `av_vfr.mkv` | Frame rate variabile (`setpts`) | PLY-001 (frame step) |
| `av_long.mkv` | 10 min, 320×180, toni ogni 30 s | STA-001, PLY-007, LIB-006 |
| `av_hd60_h264.mkv`, `av_hd60_hevc10.mkv` | 5 min 1920×1080@60 `testsrc2` (`libx264`, `libx265` 10-bit; solo di notte, in cache) | T-PERF-03 |
| `-strano: nome & spazi ü 日本.mkv` | Copia di `av_basic.mkv` | EXT-002, T-SEC-005 |
| `http:finto.mkv` | Copia di `av_basic.mkv` | EXT-002 (prefisso `file:`) |
| `corrupt_*.mkv` | `av_basic.mkv` troncato o con byte casuali | PLY-012, NFR-REL-02 |

### 4.2 Sottotitoli (versionati in `tests/fixtures/subs/`)

`basic.srt` · `bom_crlf.srt` · `tags_braces.srt` (`{\an8}`, `<i>`, `<font>`) · `out_of_order.srt` · `negative_duration.srt` · `overlap.srt` · `shift_jis.srt` · `euc_jp.srt` · `utf16le_bom.srt` · `gb18030.srt` · `cp1252.srt` · `settings.vtt` · `notes_styles.vtt` · `ruby.vtt` · `styles.ass` · `no_playres.ass` · `drawing.ass` (`\p1`) · `layers.ass` · `karaoke.ass` · `malformed_*.{srt,vtt,ass}`.

### 4.3 Dizionari

- `tests/fixtures/dicts/mini-ja/` (sorgente JSON scritto a mano; lo zip lo crea `xtask`):
  - circa 80 termini scelti per i casi di §5 e §6.5: verbi di tutte le classi (v1, v5k/k-s/g/s/t/n/b/m/r/u, vk, vs-i), aggettivi i e na, sostantivi composti (食べ物, 日本語), ため, 取り扱い/取扱い, varianti in kana;
  - `tag_bank`, contenuto strutturato (tabella, ruby, immagine `img/a.png` da 2×2 px, link `?query=`), un rinvio `formOf`, un termine con `<script>` e attributi `on*` (per T-SEC-001).
- `mini-freq` (rank-based) · `mini-freq-occ` (`frequencyMode: occurrence-based`, valori stringa) · `mini-pitch` (pitch con `nasal`/`devoice` + ipa) · `mini-kanji` · `mini-norules` (nessuna regola in nessuna voce).
- ZIP malevoli, generati: `evil-traversal.zip` (`../x`) · `evil-absolute.zip` (`/etc/x`) · `evil-symlink.zip` · `evil-bomb.zip` (rapporto > 1000:1) · `evil-huge-entry.zip` (dimensione dichiarata > 512 MiB) · `evil-badjson.zip` · `wrapped-dir.zip` (legittimo, dentro una cartella: si accetta) · `unicode-path.zip` (campo extra 0x7075) · `zip64.zip`.
- Dizionario sintetico grande (`xtask fixtures --large`): 300k termini, circa 150 MB JSON, PRNG con seed fisso. Varianti: 5 term + 3 freq + 1 pitch per T-PERF-04.

### 4.4 Anki

`tests/fixtures/anki/*.json`: risposte AnkiConnect v6 scritte a mano per ogni azione di ANK-005, compresi gli errori (duplicato, azione non supportata, chiave API errata).

### 4.5 Dati reali (opzionali, L7)

Variabile `ANIKOTO_REALDATA=/percorso`, con file forniti dall'utente: `jmdict_english.zip`, `jmnedict.zip`, un dizionario di frequenza, uno di accento, `android.db`, un episodio reale con ASS. Se la variabile manca, i test `#[ignore = "realdata"]` vengono saltati.

---

## 5. Conformità con Fushi

### 5.1 Sorgenti dei vettori

| Area | File Fushi (commit `ac8034e`) | Test Anikoto |
|---|---|---|
| Scansione delle parole | `native/fushidicts/tests/word_scan_test.cpp`, `fushi/test/lookup/apostrophe_word_scan_bug2056_test.dart`, `reader_cue_gap_word_scan_bug3453_test.dart` | T-LKP-001…004 |
| Preprocessori | `native/fushidicts/tests/text_processor_test.cpp` | T-LKP-010…012 |
| Deinflessione e popup | `native/fushidicts/tests/popup_json_deinflection_test.cpp`, `simple_dict_deinflection_test.cpp`, `en_inflection_lookup_test.cpp`, `fushi/test/dictionary/deinflection_grammar_tags_test.dart` | T-LKP-020…024 |
| Ortografia mista, Kansai | `native/fushidicts/tests/ja_mixed_orthography_lookup_test.cpp`, `ja_kansai_ta_boundary_test.cpp` | T-LKP-031, T-LKP-033 |
| Frequenze e accento | `native/fushidicts/tests/freq_pitch_import_query_test.cpp`, `freq_pitch_merge_query_test.cpp`, `lookup_freq_pitch_enrichment_test.cpp`, `ipa_import_query_test.cpp` | T-LKP-035, T-LKP-040…043 |
| Coreano (futuro) | `native/fushidicts/tests/korean_hangul_lookup_test.cpp` | T-LKP-012 |
| Robustezza dell'import | `native/fushidicts/tests/import_failure_boundary_test.cpp`, `import_memory_bound_test.cpp`, `hash_truncated_table_guard_test.cpp` | T-DIC-011…014 |
| Estrazione della frase | `fushi/test/lookup/sentence_extraction_test.dart` | T-TXT-007 |
| Conteggio dei caratteri | `fushi/test/stats/study_char_count_test.dart`, `study_char_count_parity_test.dart` | T-STA-003 |
| Copertura per intervalli | `fushi/test/stats/interval_coverage_test.dart` | T-STA-001 |
| Parser dei sottotitoli | `fushi/test/media/audiobook/srt_parser_test.dart`, `vtt_parser_test.dart`, `ass_parser_test.dart`, `subtitle_markup_test.dart` | T-SUB-001…004 |
| Nomi file e ripresa | `fushi/test/media/video/video_filename_parser_test.dart`, `video_episode_start_policy_test.dart` | T-LIB-003, T-LIB-007 |
| AnkiConnect | `packages/fushi_anki/test/ankiconnect_service_test.dart` | T-ANK-001…005 |
| Link di ritorno | `packages/fushi_anki/test/card_source_link_test.dart`, `fushi/test/anki/card_source_link_uid_test.dart` | T-ANK-050, T-ANK-051 |
| Lapis | `fushi/test/anki/lapis_preset_test.dart` | T-ANK-012, T-ANK-013 |

### 5.2 Procedura

1. Per ogni file della tabella si estraggono i casi (input, configurazione, output atteso) in `tests/conformance/<area>.json`. Ogni caso ha un campo `provenance`: file Fushi, nome del test, commit.
2. Un test Rust per area legge il JSON ed esegue tutti i casi. Un caso che fallisce indica il suo `provenance` nel messaggio.
3. I casi che dipendono da comportamenti **volutamente diversi** (SPECIFICHE §6.6.8) hanno `"divergence": "<ID requisito>"` e un output atteso aggiornato.
4. I dizionari dei test C++ di Fushi sono costruiti in codice: si riscrivono come fixture JSON in `tests/fixtures/dicts/conformance/`.

---

## 6. Catalogo dei casi

Formato: **ID** · cosa si verifica · dati e risultato atteso · livello. Ogni sezione elenca i casi principali; ogni requisito della milestone DEVE avere almeno il suo `T-<ID>`.

### 6.1 Architettura e licenze

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-LIC-001 | `cargo deny check licenses bans sources advisories` | Uscita 0. Le eccezioni di classe B coincidono **esattamente** con SPECIFICHE §3.2: `xtask` confronta la tabella in markdown con `deny.toml` | L4 |
| T-LIC-002 | Canarino: crate temporaneo che dipende da `xxhash-rust` (BSL-1.0) | `cargo deny` fallisce | di notte |
| T-LIC-003 | `cargo about generate` | `THIRD_PARTY_LICENSES.html` generato, con tutti i crate del grafo | L4 |
| T-ARC-010 | Regole di dipendenza dei crate (`cargo metadata`) | Nessun crate di logica dipende da `dioxus`, `gtk` o `libmpv2` | L4 |
| T-ARC-011 | `#![forbid(unsafe_code)]` presente nei crate richiesti; ogni `unsafe` ha `// SAFETY:` | Controllo di `xtask` | L4 |
| T-ARC-020 | Intestazione `SPDX-License-Identifier: GPL-3.0-only` in ogni `.rs` | `xtask check-headers` | L4 |
| T-ARC-001 | API del player | Dopo `file-loaded` arriva almeno un evento per ogni proprietà osservata di ARC-001 | L5 |
| T-ARC-002 | LoadToken: eventi di un caricamento precedente | Si scartano e non cambiano lo stato (sorgente di eventi finta) | L1 |
| T-ARC-003 | Nessuna lettura sincrona dal thread UI | `PlayerHandle` non espone getter sincroni (verifica a compile time con un test `trybuild`-like in `xtask`); lint: nessuna chiamata a `get_property` fuori da `anikoto-player` | L4 |
| T-ARC-004 | Limitazione di `time-pos` | Con 240 aggiornamenti/s, la UI ne riceve ≤ 30/s; l'ultimo valore arriva sempre | L1 |
| T-ERR-001 | Errori tradotti | Ogni variante degli errori pubblici ha una chiave Fluent in `it` ed `en` (test che enumera le varianti) | L1 |
| T-ERR-002 | Niente `unwrap`/`expect` su input esterni | Clippy `unwrap_used`, `expect_used` = errore nei crate di libreria; T-FUZZ-* senza panic | L4 |
| T-EXT-001 | Ricerca di ffmpeg ed encoder | Ordine impostazione → `ANIKOTO_FFMPEG` → `PATH` (script finti in directory temporanee); un `-encoders` finto senza `aac` → formato AAC disattivato con avviso, ripiego su Opus | L4 |
| T-EXT-002 | Argomenti come vettore e prefisso `file:` | Snapshot dell'argv; nessuna invocazione passa da `sh -c` (vedi anche T-SEC-005) | L3 |
| T-EXT-003 | Timeout e cancellazione | ffmpeg finto che dorme 120 s → ucciso al timeout; con la cancellazione il processo termina entro 1 s e i file parziali vengono rimossi | L4 |
| T-PRV-001 | Nessuna rete non richiesta | Suite L6 sotto `strace -f -e trace=connect`: nessuna `connect()` verso indirizzi non loopback se l'utente non ha avviato funzioni di rete | L6 |
| T-PRV-002 | Log puliti | Flussi con testo sentinella nei sottotitoli e una chiave API sentinella: nessuna delle due compare nei file di log | L4 |
| T-MAINT-001 | Qualità del codice | `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `#![deny(missing_docs)]` nei crate di libreria | CI |
| T-MAINT-002 | Copertura | `cargo llvm-cov` ≥ 80% delle righe per `anikoto-text`, `anikoto-subs`, `anikoto-dict`, `anikoto-anki` | CI |
| T-PORT-001 | Wayland e X11 | Lo smoke L6 passa sia sotto Weston headless sia sotto Xvfb | L6 |
| T-PORT-002 | Build offline | Dopo `cargo fetch`, `cargo build --offline --workspace` va a buon fine | CI |

### 6.2 Testo (`anikoto-text`)

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-LKP-011-a | Katakana → hiragana | `カタカナ` → `かたかな`; `ラーメン` → `らあめん`; `ヵヶ` invariati | L1 |
| T-LKP-011-b | NFKC | `ｶﾀｶﾅ` → `カタカナ`; `１２３` → `123` | L1 |
| T-LKP-011-c | Collasso | Le varianti di `すっっごい` includono `すっごい` e **non** `すごい` (BUG-1777) | L1 |
| T-LKP-011-d | Segni di iterazione | `時々` → `時時`; `こゝろ` → `こころ` | L1 |
| T-LKP-011-e | Cifre e numerali | `３` → `三` | L1 |
| T-LKP-010 | Varianti deduplicate con passi minimi | Proprietà: per ogni input, ogni variante compare una sola volta, con il minimo numero di passi; l'input originale ha 0 passi | L2 |
| T-TXT-001 | Andata e ritorno hiragana → katakana → hiragana | Proprietà: identità sulle stringhe di solo hiragana (esclusi i casi documentati) | L2 |
| T-TXT-007 | Estrazione della frase | Vettori Fushi (§5.1). Es. `彼は「こんにちは」と言った。次の文。` con il clic su `言` → `彼は「こんにちは」と言った。` | L1 |
| T-LKP-050 | Furigana | (`食べ物`, `たべもの`) → `[食\|た][べ][物\|もの]`; (`お茶`, `おちゃ`) → `[お][茶\|ちゃ]`; (`日本`, `にっぽん`) → `[日本\|にっぽん]` | L1 |
| T-ANK-022 | `{furigana-plain}` | `食[た]べ 物[もの]` | L1 |
| T-STA-003 | Conteggio dei caratteri | `日本語abc 123です` → 7 (3 + 1 + 1 + 2); `가나다` → 3; casi con apostrofi e segni combinanti dai vettori Fushi | L1 |

### 6.3 Sottotitoli (`anikoto-subs`, `anikoto-media`)

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-SUB-001 | SRT | `basic`, `bom_crlf`, `tags_braces` (testo senza `{…}`, markup corsivo conservato), `out_of_order` (ordinati per start) | L1 |
| T-SUB-002 | VTT | `settings.vtt` (line/position/align nel markup), `notes_styles.vtt` (NOTE e STYLE ignorati), `ruby.vtt` (`plain_text` senza `<rt>`) | L1 |
| T-SUB-003 | ASS | `no_playres.ass` → 384×288; `\N` → a capo; righe `\p1` escluse dal testo cliccabile; ordinamento per Layer | L1 |
| T-SUB-004 | Separazione testo / markup | `plain_text` non contiene mai tag né override (proprietà su tutti i file in fixture) | L2 |
| T-SUB-005 | Codifiche | Ogni file in `shift_jis`, `euc_jp`, `utf16le_bom`, `gb18030`, `cp1252` produce il testo atteso; la codifica forzata ha la precedenza | L4 |
| T-SUB-006 | File esterni | `Ep01.mkv` ↔ `Ep01.ja.srt` sì, `Ep01.jpn.ass` sì, `Ep011.srt` no, `ep01.SRT` sì | L1 |
| T-SUB-007 | Estrazione delle tracce incorporate (`av_basic.mkv`) | 2 tracce estratte in una sola invocazione; 3 cue con il testo atteso; seconda richiesta servita dalla cache (nessun processo ffmpeg avviato) | L4 |
| T-SUB-008 | Selezione automatica | Le 4 regole nell'ordine di SPECIFICHE | L1 |
| T-SUB-011 | Offset | `delay_ms = +500`, cue [1000, 2000]: attivo a `pos` = 1600, non a 1400. Saltare a quel cue porta il player a 1000 + 500 − 180 = **1320** | L1 |
| T-SUB-012 | Navigazione | Cue [1000–1100], [1150–2000], [5000–6000], `delay_ms = 0`. Da `pos` = 5500 (dentro il terzo), "precedente" → max(1150 − 180, 1000) = **1000** (mai prima dell'inizio del cue che precede l'obiettivo). Da 1200, "successivo" → 5000 − 180 = **4820**. Senza sottotitoli → seek di ±3 s. Da tastiera, con il cue precedente a > 10 s → seek di tempo | L1 |
| T-SUB-013 | Ripetizione del cue dal popup | Pausa a `end` del cue (± un fotogramma), con `av_basic.mkv` in L5 | L5 |
| T-SUB-014 | Blur | Sigma = max(12, 0,45 · font_size); assente in pausa e con il popup aperto (macchina a stati) | L1/L3 |
| T-SUB-015 | Lista che segue la riproduzione | Nei vuoti tra cue resta sul cue precedente; la ricerca filtra senza perdere la posizione | L1 |
| T-SUB-016 | Allineamento al playhead | `pos` = 4900, successivo [5000–6000] → `delay_ms` diminuisce di 100 | L1 |
| T-FUZZ-001…003 | Parser SRT / VTT / ASS su byte arbitrari | Nessun panic; per ogni cue prodotto `start ≤ end` | L2 |

### 6.4 Dizionari: import e gestione

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-DIC-001 | Import di `mini-ja` | I conteggi di termini, meta, kanji, tag e media coincidono con il sorgente; `styles.css` conservato | L4 |
| T-DIC-002 | Tag bank e `sequence` | Il tag `v5k` ha categoria e note; `sequence` conservato | L4 |
| T-DIC-003 | `mini-freq-occ` | Valori stringa letti; ordinamento invertito (più alto = meglio) | L4 |
| T-DIC-010 | Annullamento | Annullare a metà → nessun dizionario registrato, nessun file residuo | L4 |
| T-DIC-011 | ZIP malevoli (§4.3) | Ognuno è rifiutato con il suo errore specifico; nessun file scritto fuori da `dicts/<id>.importing`. `wrapped-dir`, `unicode-path` e `zip64` si accettano | L4 |
| T-DIC-012 | Limiti | Un glossario > 10 MiB fa fallire l'import in modo pulito | L4 |
| T-DIC-013 | Crash durante l'import | Processo figlio ucciso con SIGKILL a metà → al riavvio nessun dizionario parziale; la cartella `.importing` viene cancellata | L4 |
| T-DIC-014 | Disco pieno | Filesystem tmpfs piccolo → `ERR_STORAGE_FULL`, pulizia completa | L4 (Linux) |
| T-DIC-015 | Rilevamento del tipo | Vettori: term, kanji, freq, pitch, term+kanji | L1 |
| T-DIC-016 | Stessa revisione / revisione nuova | Rifiuto / proposta di aggiornamento con sostituzione atomica | L4 |
| T-DIC-021 | Eliminazione | Il file è chiuso e cancellato; il registro è aggiornato; la cache del lookup è invalidata | L4 |
| T-DIC-032 | Dizionario disabilitato | Non viene interrogato (store spia) e non consuma il budget | L1 |
| T-DIC-035 | CSS con `@scope` | Il CSS del dizionario A non cambia lo stile di B (snapshot L3 e verifica L6) | L3/L6 |

### 6.5 Lookup

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-LKP-001 | Partenza della scansione | Clic su `l` in `hello world` → `hello world`; clic su `語` in `日本語を` → `語を` | L1 |
| T-LKP-002 | Delimitatori e spazi | Si ferma a `。` e `「`; `listen to` resta unito (BUG-1773); `don't` resta unito (BUG-2056) | L1 |
| T-LKP-004 | Candidati | 16 code point al massimo; nessun taglio dentro una parola latina; nessun prefisso che finisce con uno spazio | L1/L2 |
| T-LKP-020…024 | Deinflessione (`mini-ja` + tabelle reali `ja.json`) | `食べました` → `食べる`; `行かなかった` → `行く`; `読んでいる` → `読む` con traccia mostrata `-て « -いる`; `書いた` → `書く`; `高くない` → `高い`; profondità mai > 10 (proprietà) | L1/L4 |
| T-LKP-023 | Regole vuote | `mini-norules`: `食べました` trova `食べる` (BUG-2543) | L4 |
| T-LKP-030 | Corrispondenza più lunga | `食べ物を食べる` dall'inizio → primo risultato `食べ物` | L4 |
| T-LKP-031 | Ortografia mista | `取扱い` trova `取り扱い` (vettori di `ja_mixed_orthography_lookup_test.cpp`) | L4 |
| T-LKP-032 | Rinvii | `formOf` seguito di un livello; una voce con etichetta di rinvio e definizione vera resta (BUG-2566) | L4 |
| T-LKP-033 | Unione ed eccezione Kansai | `ためになる` → `ため` e non `た` + trasformazione (BUG-3212) | L4 |
| T-LKP-034 | Ordinamento | Vettori per ognuno degli 8 criteri, isolati con fixture minime; la priorità del dizionario decide solo a parità dei criteri 1–5 | L1 |
| T-LKP-035 | Budget | `max_results = 2` con 3 headword e 10 glossari → 2 gruppi completi (BUG-1472) | L1 |
| T-LKP-036 | Raggruppamento | Stessa (espressione, lettura) da 2 dizionari → 1 gruppo, glossari nell'ordine dei dizionari; frequenze e accenti deduplicati | L1 |
| T-LKP-037 | Kanji | `食` → scheda con on/kun, significati, tratti | L4 |
| T-LKP-038 | Cache | Disabilitare un dizionario invalida i risultati in cache | L1 |
| T-LKP-040 | Frequenza armonica | [100, 400] → 160; [50] → 50; nessun valore → 9999999; valori ≤ 0 ignorati | L1 |
| T-LKP-041 | More e accento | `きょう` → [`きょ`, `う`]; nasale e devoicing di `mini-pitch` presenti nel modello di rendering | L1 |
| T-LKP-042 | Categorie | `はし`: pos 0 → heiban; pos 1 → atamadaka; `おとうと` (4 more): pos 4 → odaka, pos 2 → nakadaka; verbo con pos 2 → kifuku | L1 |
| T-LKP-043 | Deduplicazione degli accenti | 2 dizionari con payload identico → 1 riga "accento · 2" | L1 |
| T-LKP-060 | Segmentazione | `日本語を勉強します` → [`日本語`, `を`, `勉強`, `します`] con `mini-ja` | L4 |
| T-FUZZ-004 | Deinflettore e lookup su stringhe Unicode arbitrarie | Nessun panic; tempo per query ≤ 50 ms | L2 |

### 6.6 Popup

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-POP-001/004/005 | Macchina a stati pausa/ripresa | (in riproduzione) clic → pausa e `paused_for_lookup`; chiusura → ripresa. (in pausa) clic → nessun cambio; chiusura → resta in pausa. Clic su un'altra parola con il popup aperto → sostituzione, resta in pausa. Annidato: ripresa solo quando la pila è vuota | L1 |
| T-POP-002 | Evidenziazione | Snapshot: esattamente `best_length` grafemi marcati a partire da quello cliccato, anche con grafemi composti (emoji, combinanti) | L3 |
| T-POP-003 | Hover | Shift + movimento ≥ 8 px → lookup; nessun nuovo lookup sullo stesso carattere; uscita per 320 ms → chiusura e ripresa (orologio finto) | L1 |
| T-POP-006 | Posizionamento | Proprietà: per ogni rettangolo della parola e ogni finestra ≥ 320×240, il popup non copre la parola e sta nella finestra | L2 |
| T-POP-007 | Intestazione | Snapshot della voce `食べました` → headword `食べる` con furigana, chip della traccia, frequenze, accento, tag | L3 |
| T-POP-008 | Blocchi per dizionario | Tag numerici nascosti; tag di parte del discorso ripetuto su sensi consecutivi omesso; stato compresso rispettato | L3 |
| T-POP-009 | Contenuto strutturato | Snapshot `dioxus-ssr` per tabella, ruby, lista, details. Tag non ammessi (`script`, `iframe`, `style`) e attributi `on*` eliminati; proprietà `style` non ammesse eliminate | L3 |
| T-POP-010 | Immagini | `anikoto-dict://<id>/img/a.png` → byte corretti e `Content-Type: image/png`; un percorso inesistente → 404; `..` → 400 | L4 |
| T-POP-011 | Link interni | Il testo del link esclude `<rt>` (BUG-2456); un link esterno apre `xdg-open` (spia) | L1 |
| T-POP-012 | Annidamento | Profondità 8 al massimo; Esc chiude solo l'ultimo; un clic sul genitore chiude i figli | L1 |
| T-POP-013 | Scheda kanji | Snapshot per `食` da `mini-kanji` | L3 |
| T-POP-014 | Tastiera | Tab, Enter, Ctrl+Enter, Esc, Alt+rotellina (harness) | L6 |
| T-POP-015 | Ripeti e vai alla frase | Dal popup: "ripeti" riparte dall'inizio del cue e si ferma alla fine; "vai" chiude il popup e porta il player all'inizio del cue (harness) | L6 |

### 6.7 Sicurezza

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-SEC-001 | Voce di `mini-ja` con `<script>` e `onerror=` | Nessuno script eseguito: la variabile globale sentinella resta indefinita (harness) | L6 |
| T-SEC-002 | ZIP malevoli | Vedi T-DIC-011 | L4 |
| T-SEC-003 | Scritture su disco | Durante tutta la suite L6, nessun file creato fuori dalle directory XDG isolate e da quelle di export del test (controllo con inotify sul `$HOME` di prova) | L6 |
| T-SEC-004 | Build di rilascio | `cargo tree -e features --release -p anikoto-app` senza `test-harness`; il binario non contiene il simbolo del harness | L4 |
| T-SEC-005 | Iniezione di argomenti in ffmpeg | `-strano: nome & spazi ü 日本.mkv` e `http:finto.mkv` vengono trattati come file locali; argv catturato con snapshot | L4 |
| T-SEC-006 | CSP della webview | L'header o meta CSP è presente; un `<img src="https://…">` iniettato in un glossario non genera richieste di rete (proxy spia) | L6 |
| T-SEC-007 | Link `anikoto://` ostili | Percorsi assoluti, `file:`, parametri duplicati o sconosciuti, > 16 KiB → rifiutati senza effetti (vedi T-ANK-051) | L1 |
| T-SEC-008 | Permessi della config | `config.toml` creato con permessi 0600 | L4 |

**Revisione di sicurezza in M8**: oltre ai test, una revisione manuale di ogni punto in cui entrano dati esterni (zip, JSON dei dizionari, sottotitoli, risposte HTTP, link `anikoto://`, argomenti dalla seconda istanza) e di ogni blocco `unsafe`. Gli esiti vanno in `docs/security-review-1.0.md`.

### 6.8 Player, scorciatoie, istanza singola

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-PLY-001 | Comandi su `av_basic.mkv` | play/pausa; seek ±3 s (`time-pos` atteso ± 0,1 s); velocità limitata a 0,25–4,0; volume limitato a 0–150; frame step su `av_vfr.mkv` | L5 |
| T-PLY-002 | Tieni premuto per accelerare | keydown `E` → velocità 2,0; le ripetizioni automatiche del tasto si ignorano; keyup → torna alla velocità precedente | L1 |
| T-PLY-003 | Tracce audio | 2 tracce elencate con lingua; il cambio di traccia aggiorna `aid` | L5 |
| T-PLY-004 | Barra dei controlli | Macchina a stati con orologio finto: si nasconde dopo 2 s senza movimento durante la riproduzione; resta visibile in pausa, con il mouse sui sottotitoli e con un popup aperto | L1 |
| T-PLY-005 | Opzioni fisse | Dopo l'inizializzazione, ogni opzione di PLY-005 ha il valore atteso (`mpv_get`) | L5 |
| T-PLY-006 | Opzioni mpv grezze | `deband=yes` applicato; `chiave senza uguale` → errore alla riga 1; l'ordine di applicazione è rispettato (le opzioni grezze vincono) | L1/L5 |
| T-PLY-007 | Ripresa | `start = 5` → dopo `file-loaded`, `time-pos` ∈ [4,9; 5,5] e `start` torna `none` | L5 |
| T-PLY-008 | Dopo il caricamento | `sid = no`, `sub-visibility = no`; `aid` adottato | L5 |
| T-PLY-009 | Riuso | Due `load` consecutivi usano la stessa istanza `Mpv` (contatore di istanze) | L5 |
| T-PLY-010 | Velocità | Dopo un cambio di velocità, `af` non contiene `scaletempo` | L5 |
| T-PLY-011 | Screenshot | `S` → file PNG/JPEG in `$XDG_PICTURES_DIR/Anikoto` con le dimensioni del video, senza sottotitoli | L5 |
| T-PLY-012 | Errori | File inesistente, `corrupt_*.mkv` → errore tipizzato; il player resta utilizzabile per il file successivo | L5 |
| T-PLY-013 | Salvaschermo | Durante la riproduzione l'inibizione è attiva; in pausa per > 1 min viene rilasciata (portale spia) | L4 |
| T-KEY-001…004 | Scorciatoie | Rimappatura salvata e ricaricata; associazione per `code` (con un layout AZERTY simulato `KeyQ` resta "Q fisico"); conflitto nello stesso contesto rifiutato; tasti singoli disattivati in un campo di testo | L1/L6 |
| T-APP-002 | File `.desktop` | `desktop-file-validate` senza errori; `MimeType` contiene `x-scheme-handler/anikoto` e i tipi video; `Exec=anikoto %U` | L4 |
| T-APP-001 | Istanza singola | Il secondo avvio con un file → il file si apre nella prima istanza; il secondo processo esce con 0 | L6 |
| T-E2E-001 | Flusso M1 | apri → riproduci → seek → chiudi → riapri → riprende dalla posizione salvata | L6 |

### 6.9 Anki

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-ANK-001 | Forma della richiesta | Snapshot JSON di ogni azione; `key` assente se vuota | L3 |
| T-ANK-002 | Mappatura degli errori | Non-200, non-JSON, `result`/`error` mancanti, messaggio di duplicato → l'errore tipizzato giusto | L4 |
| T-ANK-003 | Retry | `deckNames` con connessione scaduta → ritentato una volta (wiremock conta 2 richieste). `addNote` con risposta persa dopo l'invio → `CommitUnknown`, **1** sola richiesta | L4 |
| T-ANK-011 | "Crea mazzo Lapis" | Modello assente → `createModel` + `createDeck`; modello presente → solo `createDeck`; verifica fallita → `LapisSetupMissing`; seconda esecuzione → `già esistente` | L4 |
| T-ANK-012/013 | Mapping predefinito e riconoscimento di Lapis | Vettori di `lapis_preset_test.dart` | L1 |
| T-ANK-021…025 | Segnaposto | Snapshot HTML per `{glossary}` (etichetta solo sul primo senso, `<style>` limitato), `{frequencies}`, `{pitch-accent-positions}`, `{pitch-accent-categories}`, confrontati con l'output di Fushi su `mini-ja` | L3 |
| T-ANK-024 | `{sentence}` | Offset valido → `<b>` nel punto esatto anche se la parola compare due volte; offset non valido → prima occorrenza | L1 |
| T-ANK-026 | Campi vuoti | Primo campo vuoto → errore dedicato; i campi inesistenti nel tipo di nota non vengono inviati | L1 |
| T-ANK-027 | Tag | `["mio", "anikoto", "video", "Sousou_no_Frieren"]` deduplicati e nell'ordine | L1 |
| T-ANK-030 | Duplicati | `canAddNotesWithErrorDetail` con le stesse opzioni di `addNote`; su "unsupported action" → ripiego su `findNotes`; dopo un errore di trasporto, 30 s di pausa (orologio finto) | L4 |
| T-ANK-032 | Intervallo audio | Cue [10000, 12000], padding 120/200, cue precedente che finisce a 9950, successivo che inizia a 12100, `delay` +300 → **[10250, 12400]** | L1 |
| T-ANK-033 | Audio della frase | Snapshot dell'argv di ffmpeg. Su `av_basic.mkv`, cue 2 con traccia `jpn`: il file prodotto dura D ± 50 ms e contiene il tono da 1 kHz (ffmpeg `astats`); con la traccia `eng` il tono è a 660 Hz (verifica di `ff-index`, Fushi BUG-345) | L3/L4 |
| T-ANK-034 | Immagine | Lato lungo 1000 px, JPEG; livello 3 = risoluzione originale | L4/L5 |
| T-ANK-035 | Senza audio | `av_noaudio.mp4` → mining annullato con un messaggio; nessuna richiesta `addNote` | L4 |
| T-ANK-036 | Media | Nome per contenuto stabile; host loopback → `path`, host remoto → `data` base64; se `addNote` fallisce → `deleteMediaFile` per i soli media caricati in quel tentativo | L4 |
| T-ANK-037 | Sovrascrittura | `updateNoteFields` con tutti i campi (quelli non mappati = `''`); tipo di nota diverso → rifiuto | L4 |
| T-ANK-050/051 | Link di ritorno | Andata e ritorno; vettori di rifiuto di `card_source_link_test.dart` | L1/L2 |
| T-ANK-052 | Apertura del link | `anikoto://source?…` passato alla seconda istanza → video aperto a `startMs`, in pausa, cue evidenziato (harness) | L6 |
| T-E2E-ANKI-001 | Anki reale | Anki in Xvfb con AnkiConnect, profilo vuoto → "Crea mazzo Lapis" → mining da `av_basic.mkv` → `notesInfo` mostra i campi attesi, i file media esistono in `collection.media` | L7 |

### 6.10 Libreria e audio delle parole

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-LIB-001 | Apertura di un file | Da riga di comando e con drag & drop simulato (`drop {paths}` nel harness); il dialogo si verifica a mano (§9.1) | L6 |
| T-LIB-002 | Import di cartelle | Albero temporaneo con 2 serie, sottocartelle di stagione e file non video: modalità per serie → 2 serie con episodi corretti; per cartella → una collezione per sottocartella più quella radice | L4 |
| T-LIB-003 | Nomi file | Vettori di `video_filename_parser_test.dart`. Es.: `[SubsPlease] Sousou no Frieren - 01 (1080p) [ABCD1234].mkv` → serie `Sousou no Frieren`, ep. 1; `Show.S02E05.1080p.mkv` → s2 e5; `Season 2/03.mkv` → s2 e3; `第2季/03.mkv` → s2 e3 | L1 |
| T-LIB-004 | Nuova scansione | Progressi, sottotitoli e offset conservati; una serie eliminata non viene ricreata; cambio di modalità reversibile senza perdite | L4 |
| T-LIB-005 | Home | "Continua a guardare" ordinato per `last_played_at`, senza episodi completati; copertina generata al 10% della durata e servita dalla cache alla seconda richiesta | L1/L4 |
| T-LIB-006 | Salvataggio del progresso | Con l'orologio finto, ≤ 1 scrittura/s; nessuna scrittura prima del seek di ripresa; scrittura sincrona all'uscita | L1 |
| T-LIB-007 | Regola di ripresa | (90% → 0), (resto 2,9 s → 0), (episodio precedente → 0), (50% → posizione salvata) | L1 |
| T-LIB-008 | Completamento | `completed_at` impostato la prima volta che si supera il 90% e mai sovrascritto; stato visto / in corso / nuovo | L1 |
| T-LIB-009 | Episodio successivo | Il conto alla rovescia di 5 s si annulla con Esc | L6 |
| T-LIB-010 | Preferenze per serie | Un episodio senza valori propri eredita audio, sottotitolo e offset della serie; un valore proprio dell'episodio vince | L1 |
| T-LIB-011 | Modifiche alla libreria | Rinomina, unione, divisione, spostamento di episodi: progressi e preferenze conservati | L4 |
| T-LIB-012 | Rimozione | Il file su disco resta | L4 |
| T-DAT-001/002 | Migrazioni | Un DB v1 di fixture → migrato con backup `.bak-v1`; un DB con schema futuro → aperto in sola lettura | L4 |
| T-DAT-003 | uid | `sanitize` stabile; due file con lo stesso nome in cartelle diverse → ` (2)` | L1 |
| T-AUD-001 | Elenco delle sorgenti | Ordine e abilitazione salvati e riletti; una sorgente disabilitata non viene interrogata | L1 |
| T-AUD-002 | URL template | Risposta JSON Yomitan → prima voce; risposta `audio/mpeg` diretta → riprodotta; 404 → "nessun audio"; connessione rifiutata → pausa di 45 s dell'host | L4 |
| T-AUD-003 | DB locale | `android.db` di fixture (generato da xtask, solo dopo l'ADR 0003) → coppia esatta, poi solo espressione, priorità delle sotto-sorgenti | L4 |
| T-AUD-004 | Menu delle sorgenti | Interrogazione in parallelo, ordine della configurazione, deduplicazione | L4 |
| T-AUD-005 | Istanza audio separata | La riproduzione di una parola non cambia `time-pos`, `pause` né `volume` del player principale | L5 |
| T-AUD-006 | Riproduzione automatica | Attiva → l'audio parte all'apertura del popup; disattiva → solo con ♪ | L1 |
| T-AUD-007 | Nessun URL predefinito di terzi | La configurazione predefinita non contiene sorgenti remote | L1 |
| T-AUD-008 | Cache | Superati 500 MiB, eliminazione LRU | L1 |

### 6.11 Statistiche

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-STA-001 | Copertura | Guardo 0–10 s, seek a 5 s, guardo fino a 15 s → **15 s** totali. Velocità 2,0: un salto di 2,4 s in 1 s di tempo reale non è un seek (soglia 1 × 2 × 1,25 + 0,5 = 3,0 s). Più i vettori di `interval_coverage_test.dart` | L1 |
| T-STA-002 | Crediti per cue | Cue di 1000 ms: 900 ms guardati → nessun credito, 1000 ms → credito; sessione successiva → nessun doppio conteggio | L1 |
| T-STA-004 | Sessioni | Pausa > 120 s → nuova sessione; sessione a cavallo delle 15:00 → 2 segmenti | L1 |
| T-STA-005 | Export CSV | Snapshot | L3 |

### 6.12 Impostazioni, i18n, accessibilità

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-SET-001 | Config | Una chiave sconosciuta viene conservata e segnalata; un valore non valido → predefinito più un avviso; la scrittura è atomica (SIGKILL durante la scrittura → il file precedente resta intatto) | L4 |
| T-SET-002 | Applicazione immediata | Cambio di tema o di dimensione dei sottotitoli senza riavvio (harness) | L6 |
| T-SET-003 | Contrasto | Per ogni coppia testo/sfondo dei token dei temi chiaro e scuro, rapporto ≥ 4,5:1 (calcolato in un unit test dai valori CSS) | L1 |
| T-SET-004 | Lingua | `LANG=it_IT.UTF-8` → italiano; `LANG=de_DE.UTF-8` → ripiego su inglese; la scelta nelle impostazioni vince | L1 |
| T-SET-005 | Scala dell'interfaccia | 75% e 200%: lo zoom della webview cambia, la dimensione dei sottotitoli no (harness) | L6 |
| T-SET-006 | Ripristino per sezione | Solo le chiavi della sezione tornano ai valori predefiniti | L1 |
| T-SET-007 | Proxy | Le richieste HTTP passano dal proxy (wiremock come proxy); AnkiConnect su loopback no | L4 |
| T-I18N-001 | Chiavi | Ogni chiave Fluent esiste in `it` ed `en`; nessuna chiave inutilizzata; stessi argomenti `{ $var }` in tutte le lingue | L1 |
| T-I18N-002 | Nessuna stringa nel codice | Lint `xtask`: nessuna stringa letterale come testo nei componenti `rsx!` (lista di eccezioni esplicita) | L4 |
| T-A11Y-001 | Focus e ARIA | Snapshot dei controlli del player con `aria-label`; focus visibile (classe e stile) | L3 |
| T-E2E-A11Y-001 | Solo tastiera | Libreria → episodio → lookup → mining → impostazioni → indietro, senza mouse (harness) | L6 |

### 6.13 Online (M7)

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-MET-001 | AniList | Risposta GraphQL finta → serie identificata solo se c'è un risultato esatto e unico; altrimenti scelta dell'utente | L4 |
| T-MET-002 | Jikan | Al massimo 1 richiesta/s; dopo un 429, pausa e cache rispettate (orologio finto) | L4 |
| T-MET-003 | Jimaku | Header `Authorization`; ricerca per `anilist_id` e poi per `query`; zip estratto con le regole di DIC-011 | L4 |
| T-MET-004 | OpenSubtitles | Ordine delle query; file > 64 MiB rifiutato | L4 |
| T-MET-005 | Chiavi API | Salvate con permessi 0600 (vedi T-SEC-008); mai nei log (vedi T-PRV-002) | L4 |
| T-MET-LIVE-* | Servizi reali | Smoke test con chiavi da variabili d'ambiente | L7 |

### 6.14 Affidabilità e fuzzing

| ID | Verifica | Atteso | L |
|---|---|---|---|
| T-REL-001 | SIGKILL durante: import, scrittura della config, salvataggio del progresso, mining (processo figlio) | Al riavvio, stato coerente: nessun DB corrotto, nessun dizionario parziale, nessun media orfano registrato | L4 |
| T-REL-002 | Anki chiuso durante il mining | Errore "connessione rifiutata", nessun panic, nessun media orfano | L4 |
| T-FUZZ-005 | JSON dei term bank Yomitan | Nessun panic; errore tipizzato | L2 |
| T-FUZZ-006 | Contenuto strutturato → elementi | Nessun panic; l'output contiene solo tag della lista ammessa | L2 |
| T-FUZZ-007 | Parser dei link `anikoto://` e della config TOML | Nessun panic | L2 |

### 6.15 Prestazioni

Si eseguono su RIF (SPECIFICHE §7) di notte. Esito negativo se la soglia è superata **oppure** se il risultato è peggiore del 15% rispetto alla baseline in `perf-baseline.json`.

| ID | Misura | Metodo | Soglia |
|---|---|---|---|
| T-PERF-01 | Avvio a freddo fino a `library-ready` | Harness, mediana di 10 esecuzioni (cache del disco svuotata con `vmtouch -e` o simile) | ≤ 1,5 s |
| T-PERF-02 | Da `open` a `first-frame` | Harness | ≤ 1,0 s |
| T-PERF-03 | Fotogrammi persi e CPU durante la riproduzione | `frame-drop-count`, `vo-delayed-frame-count` (mpv) e `/proc/<pid>/stat` per 5 min su `av_hd60_*` con overlay visibile | 0 persi; CPU ≤ 25% di un core |
| T-PERF-04 | Lookup nel motore | `criterion`, dizionario sintetico ×5 + freq ×3 + pitch, 10k query da un corpus di frasi | p95 ≤ 10 ms, p99 ≤ 25 ms |
| T-PERF-05 | Import di 300k voci | Tempo totale e `VmHWM` | ≤ 60 s; ≤ 1 GiB |
| T-PERF-06 | Da `click_grapheme` a `popup-rendered` | Harness, 100 clic | p95 ≤ 100 ms |
| T-PERF-07 | Ritardo di visualizzazione dei cue | `cue-shown.time_pos − cue.start` | ≤ 50 ms |
| T-PERF-08 | Mining | Da Ctrl+Enter a `card-created`, AnkiConnect finto, ffmpeg reale | ≤ 3 s |
| T-PERF-09 | RSS a riposo | 30 s dopo `library-ready` | ≤ 400 MB |

---

## 7. Bug e regressioni

- Ogni bug di Anikoto ha una scheda `docs/bugs/AK-BUG-NNN.md` (sintomo, passi per riprodurlo, causa, correzione) e un test `ak_bug_nnn_<descrizione>` marcato `// AK-BUG-NNN`, scritto **prima** della correzione e verificato rosso.
- I bug Fushi citati in SPECIFICHE (`BUG-xxxx`) hanno un test `fushi_bug_xxxx_<descrizione>` marcato `// FUSHI-BUG-xxxx`.
- Priorità: P1 = crash, perdita di dati o carta Anki sbagliata; P2 = funzione principale degradata; P3 = estetica o casi rari. Nessun rilascio con P1 aperti.

---

## 8. Tracciabilità (`cargo xtask trace`)

1. Legge gli ID dei requisiti da SPECIFICHE.md: celle e paragrafi che iniziano con `[A-Z]{2,4}-\d{3}` o `NFR-[A-Z0-9]+-\d{2}`.
2. Legge da ROADMAP.md, per ogni milestone, la riga **Requisiti** (espandendo gli intervalli `PLY-001…PLY-010`) e lo **Stato**.
3. Cerca i marcatori `// T-<ID>[-<suffisso>]` nei test (`crates/**`, `tests/**`, `xtask/**`). Gli NFR corrispondono ai test indicati nella colonna "Misura" di SPECIFICHE §7 (es. `NFR-PERF-04` → `T-PERF-04`, `NFR-REL-02` → `T-FUZZ-*`).
4. Produce `target/trace.md` (requisito → test → milestone) e **fallisce** se:
   - un requisito di una milestone `completata` non ha test;
   - un test cita un requisito che non esiste;
   - un requisito appare in due milestone.

---

## 9. Test manuali

Si registrano in `docs/qa/<versione>.md`: chi, data, ambiente (distribuzione, compositore, GPU, driver, versioni di mpv/ffmpeg/WebKitGTK), esito di ogni voce.

### 9.1 Player (fine M1)

- [ ] Apertura da dialogo, da drag & drop e da riga di comando, con nomi file Unicode e spazi.
- [ ] Schermo intero su un monitor secondario; uscita con Esc.
- [ ] HiDPI 200%: video nitido, overlay nitido, nessun bordo sfasato.
- [ ] hwdec attivo (verificare `hwdec-current`) su Intel, AMD e NVIDIA.
- [ ] Sospensione e ripresa del sistema durante la riproduzione.
- [ ] Audio su PipeWire con cambio del dispositivo durante la riproduzione.

### 9.2 Sottotitoli (fine M2)

- [ ] SRT in Shift_JIS reale, ASS di fansub con stili e karaoke (il testo resta leggibile anche se gli effetti non sono supportati), MKV con 2 tracce incorporate.
- [ ] Offset con Z/X e allineamento con Ctrl+Shift+frecce su un sottotitolo sfasato di 2 s.
- [ ] Blur: illeggibile in riproduzione, si rivela al passaggio del mouse.

### 9.3 Lookup (fine M3): confronto con Fushi

Importa in Anikoto e in Fushi **gli stessi** dizionari (JMdict inglese, un dizionario di frequenza, uno di accento) e confronta il primo headword per ogni query. **L'oracolo è il risultato di Fushi.** La colonna "indicativo" serve solo a orientarsi.

| # | Query | Indicativo | # | Query | Indicativo |
|---|---|---|---|---|---|
| 1 | 食べました | 食べる | 26 | 高くて | 高い |
| 2 | 食べなかった | 食べる | 27 | 美しく | 美しい |
| 3 | 食べられる | 食べる | 28 | 寒そう | 寒い |
| 4 | 食べさせられた | 食べる | 29 | 寝ちゃった | 寝る |
| 5 | 食べたい | 食べる | 30 | 見ない | 見る |
| 6 | 食べて | 食べる | 31 | 見せて | 見せる |
| 7 | 食べよう | 食べる | 32 | 起きろ | 起きる |
| 8 | 食べれば | 食べる | 33 | 言わせる | 言う |
| 9 | 行った | 行く | 34 | 知らなかった | 知る |
| 10 | 行かない | 行く | 35 | 頑張って | 頑張る |
| 11 | 行きましょう | 行く | 36 | 思い出した | 思い出す |
| 12 | 書いている | 書く | 37 | 呼ばれた | 呼ぶ |
| 13 | 泳いだ | 泳ぐ | 38 | 歩いて | 歩く |
| 14 | 話します | 話す | 39 | 静かな | 静か |
| 15 | 待って | 待つ | 40 | 日本語を | 日本語 |
| 16 | 死んだ | 死ぬ | 41 | 食べ物 | 食べ物 |
| 17 | 遊ばない | 遊ぶ | 42 | 時々 | 時々 |
| 18 | 読んだ | 読む | 43 | 綺麗 | 綺麗 |
| 19 | 帰ろう | 帰る | 44 | きれい | 綺麗 |
| 20 | 買わなきゃ | 買う | 45 | ラーメン | ラーメン |
| 21 | 来なかった | 来る | 46 | ｶﾀｶﾅ | カタカナ |
| 22 | しなければ | する | 47 | ありがとうございます | ありがとうございます |
| 23 | 高かった | 高い | 48 | 取扱い | 取り扱い |
| 24 | 高くない | 高い | 49 | ためになる | ため |
| 25 | 高ければ | 高い | 50 | 読んでいた | 読む |

Criterio: 50/50 uguali, oppure ogni differenza spiegata da una divergenza voluta (SPECIFICHE §6.6.8) e annotata.

### 9.4 Mining (fine M4)

- [ ] 10 carte da 2 episodi reali: in Anki desktop audio e immagine si riproducono, la parola è in grassetto nella frase, il furigana è corretto.
- [ ] Dopo la sincronizzazione su AnkiDroid: audio e immagine funzionano.
- [ ] Il link nella carta riapre la scena in Anikoto (con Anikoto chiuso e con Anikoto aperto).
- [ ] Duplicato: il pulsante mostra ✓; "aggiungi comunque" crea la seconda carta; "sovrascrivi" sostituisce l'ultima.
- [ ] Con Anki chiuso: messaggio chiaro, nessun blocco.

### 9.5 Rilascio (M8)

Le sezioni 9.1–9.4 più:

- [ ] Installazione pulita dal pacchetto (AUR e Flatpak) su GNOME Wayland, KDE Wayland e X11.
- [ ] Primo avvio senza configurazione: lingua dal sistema, guida al primo avvio (dizionari, Anki).
- [ ] "Informazioni" mostra la versione e `THIRD_PARTY_LICENSES`.
- [ ] Aggiornamento da una versione precedente: migrazione del DB con backup.
- [ ] Una sessione di 2 ore senza crescita anomala della memoria.

---

## 10. Pipeline CI

| Fase | Quando | Contenuto | Bloccante |
|---|---|---|---|
| Veloce | Ogni push | `cargo xtask ci`: fmt, clippy `-D warnings`, `cargo deny`, `check-headers`, `trace`, L1+L2+L3 (`cargo nextest`), doctest | Sì |
| Completa | Ogni PR | Fase veloce + `cargo xtask fixtures` + L4 + L5 + smoke L6 (Xvfb e Weston headless) + copertura (NFR-MAINT-02) | Sì |
| Notturna | Ogni notte | L2 con 100k casi, fuzzing (T-FUZZ-*) per 10 minuti ciascuno, L6 completo, L8 con baseline, T-LIC-002, L7 dove sono disponibili segreti o dati | Apre una segnalazione se fallisce |
| Rilascio | Tag `v*` | Completa + notturna + checklist §9.5 + `THIRD_PARTY_LICENSES.html` | Sì |

L'immagine CI contiene: Rust stable (versione di `rust-toolchain.toml`), `libmpv-dev` ≥ 0.37, `ffmpeg` ≥ 6 con `libx264`/`libx265` (solo per generare le fixture di prestazione), GTK 3, WebKitGTK 4.1, Xvfb, Weston, Mesa.
