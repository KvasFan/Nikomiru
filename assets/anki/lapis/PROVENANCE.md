# Lapis note type

The card templates and the styling of the Lapis note type, copied byte for
byte from the upstream release (ANK-010, [ADR 0007](../../../docs/adr/0007-mining-to-anki.md)).
"Create Lapis deck" sends them to Anki with `createModel`.

| | |
|---|---|
| Upstream | <https://github.com/donkuri/lapis> |
| Release | tag `1.7.0`, 2026-01-20 |
| Commit | `38238d0b1c072fe7d7cd121e1e4c838a959f8d6b` |
| Licence | GPL-3.0 (the upstream `LICENSE` is the GPLv3 text, the same as Nikomiru's; no version clause) |
| Authors | Ruri (bewizible), kuri (donkuri) and itokatsu, with the contributors listed in the upstream README; Lapis reuses code by Rudnam (JP-study), and `styling.css` holds code for 明鏡 第三版 and 大辞泉 二版 by kiwakiwaa (credited inside the file) |
| Changes | none |

| File | Upstream path | Bytes | SHA-256 |
|---|---|---|---|
| `front.html` | `src/front.html` | 2585 | `7bb9993df961d35e7f49b3edec9f7b0f43987f114f3cfdc627a10219c62aef49` |
| `back.html` | `src/back.html` | 24860 | `1d92182a7626b8a14fd80bc82ee07e71a7a387533be3a4124d25fe8b3090a137` |
| `styling.css` | `src/styling.css` | 26070 | `51590e7545d43896cb15e494db49b611a9c6e7bbf05c8e0debbf1f98f7ae8920` |

The note type Nikomiru creates is named `Lapis`, with the 22 fields of
`build/anki_fields.yaml` in that order and one card template named `Mining`,
as in the official package (`build/genapkg.py`). The official package's example
note and its media are not copied.

## Updating

1. Download the three files of the new release tag from
   `https://raw.githubusercontent.com/donkuri/lapis/<tag>/src/`.
2. Compare `build/anki_fields.yaml` with `lapis::FIELDS`: a new or renamed field
   changes the note type and the default mapping (ANK-012).
3. Replace the files, update this table and the hashes pinned by the T-ANK-010
   test in `crates/nikomiru-anki/src/lapis.rs`.
4. Never edit the files by hand: a Nikomiru change goes in a separate file,
   documented here.
