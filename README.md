# Anikoto

Player video per **imparare le lingue guardando anime e serie**. Clicchi una parola nei sottotitoli, la cerchi nei tuoi dizionari Yomitan e crei una carta Anki con la frase, uno screenshot e l'audio della battuta.

Anikoto è una riscrittura in Rust, limitata al nucleo video + dizionario + Anki, di [Fushi](https://github.com/hajisensai/Fushi) (GPL-3.0). Usa [Dioxus](https://github.com/DioxusLabs/dioxus) per l'interfaccia e [libmpv2](https://crates.io/crates/libmpv2) per la riproduzione.

> **Stato: progettazione.** Non c'è ancora codice. Il lavoro parte dalla milestone M0 della roadmap.

## Documenti

| Documento | Contenuto |
|---|---|
| [docs/SPECIFICHE.md](docs/SPECIFICHE.md) | Scopo, licenze, architettura, requisiti funzionali e non funzionali, decisioni aperte |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestone M0–M8 fino alla 1.0, criteri di uscita, backlog dopo la 1.0 |
| [docs/TEST.md](docs/TEST.md) | Strategia di test, fixture, conformità con Fushi, catalogo dei casi, CI |

## Piattaforme

Linux (Wayland e X11) come piattaforma primaria. Windows e macOS dopo la 1.0.

Dipendenze di sistema previste: libmpv ≥ 0.37, FFmpeg ≥ 6, GTK 3.24, WebKitGTK 4.1 ≥ 2.44.

## Licenza

[GPL-3.0-only](LICENSE). Anikoto deriva da codice GPL-3.0 (Fushi, Yomitan, hoshidicts, Lapis).

Le dipendenze ammesse sono MIT, Apache-2.0, GPL e LGPL. Fanno eccezione alcune dipendenze transitive permissive, imposte da Dioxus e documentate in [SPECIFICHE §3](docs/SPECIFICHE.md#3-licenze). Il controllo è automatico, con `cargo deny`.
