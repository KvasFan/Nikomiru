# Series-card comprehension — validation, 2026-11-13

2.32.0 adds explicit series analysis on Library cards (KNW-017, M21, D-61).
Source releases are prepared locally, without publication.

## Automated checks

- `cargo xtask ci`: **842 tests passed**, 71 opt-in tests skipped; formatting,
  Linux/Windows clippy, architecture, headers, all three interface languages,
  licensing, traceability and Flatpak sources passed.
- `cargo test -p nikomiru-app --features test-harness --locked --test e2e series_scores:: -- --include-ignored --test-threads 1`:
  **1 passed**, using actual external and embedded subtitle sources and the
  dictionary worker. The aggregate matches episode totals, shows 2/4 usable
  sources (Off and missing text excluded), follows imported statuses and
  refreshes modified text without opening/playing or changing the open view.
  Analysis creates no study segments.
- Four model tests check weighted percentages, coverage/error counts,
  one in-flight episode, rescoring, refresh, duplicate/stale callbacks,
  dictionary/language/tool/removal/disabled-colour invalidation, four retained
  results, empty/error sources and the 256-episode limit.
- The component test checks the accessible Analyse action, independent of
  opening its card, hidden without colours and disabled during an analysis.

## Scope

Scores aggregate word occurrences and lines rather than averaging episode
percentages. Coverage and failed sources are always visible. The existing
KNW-008 resolver follows playback choices; no background OCR or remote
probing is introduced. Normal episode-list jobs precede each next analysis
job on the worker. Scans and library edits discard retained series analyses.

Real mixed-container series and runtime behavior on Windows remain manual
checks. No dependency, persisted field or database schema change was added.

The optimized 2.32.0 build passed `cargo build --release --locked -p nikomiru-app`;
its executable reports `nikomiru 2.32.0`. The binary, matching source archive
and SHA-256 checksums are retained in `target/releases/2.32.0/`.
