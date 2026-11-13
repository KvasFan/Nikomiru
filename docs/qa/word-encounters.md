# New words met per episode — validation, 2026-11-13

2.33.0 completes the remaining implementation of M21 with historical word
encounters (KNW-018, D-62). Source releases are prepared locally.

## Automated checks

- `cargo xtask ci`: **847 tests passed**, 71 opt-in tests skipped. Formatting,
  Linux/Windows clippy, architecture, headers, translations, licensing,
  requirement traceability and Flatpak sources passed.
- `cargo test -p nikomiru-app --features test-harness --locked --test e2e encounters:: -- --include-ignored --test-threads 1`:
  **1 passed**, with the actual player and dictionary. Both subtitle lines
  are segmented in advance; paused seeking creates no encounters, then the
  played word counts once despite repetitions/replay. A later known status
  preserves history. Restart with new text and colours off preserves the
  earlier single encounter without adding the new word.
- The backup restore regression records encounters before and after the
  snapshot and confirms that restoring it recovers exactly the saved word
  and timestamp.
- Database tests cover distinct readings/media, earliest timestamps, bounded
  latest-episode statistics and reopen persistence. Actual libraries
  export/merge/apply encounter history without duplicates; gzip JSON round
  trips, including old device states missing the new field.
- The sync property generator now includes encounter sets, exercising the
  existing commutativity/idempotence/associativity tests on the new state.
- Collector tests exclude paused/pending/known/learning/ignored text and
  deduplicate per media visit; the 50,000-key memory bound resets on a new uid.
  The statistics component checks the separate episode history label.

- Final full GUI regression:
  `cargo test -p nikomiru-app --features test-harness --locked --test e2e -- --include-ignored --skip l7_ --test-threads 1`:
  **92 passed, 0 failed**, 5 external/live checks filtered out (322 s).
  This includes all new book/series/history cases, passive-analysis checks,
  playback, X11, accessibility, privacy, Anki, backup/sync, media servers,
  reader modes and web videos, plus the current optimized release's startup.

## Data and behavior

Only dictionary headwords in active main text during loaded, unpaused
playback are counted, with word colours enabled. Future segmentation,
secondary text, reader/list browsing and passive library analyses do not
create encounter history. Unsplit lines and earlier playback are not
estimated retrospectively. A later status change never reduces historical
counts. Counts are distinct by headword plus reading, once per media uid.

An additive `word_encounters` table leaves existing encodings and the schema
version unchanged. Writes use the library writer; full database backups
already contain this table. The default-empty device-state map remains
compatible with older JSON files. Merging unions keys and keeps the earliest
first-encounter time, so repeated sync cannot inflate statistics.

No dependency or configuration field was added. Real two-device WebDAV
validation and a manual comparison with Japanese material remain open.

The optimized 2.33.0 build passed `cargo build --release --locked -p nikomiru-app`;
its executable reports `nikomiru 2.33.0`. The binary, matching source archive
and SHA-256 checksums are retained in `target/releases/2.33.0/`.
