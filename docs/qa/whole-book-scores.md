# Whole-book comprehension — validation, 2026-11-13

2.31.0 completes the Books list's whole-book analysis (KNW-016, M21,
D-60). Analyse counts every text chapter of a local EPUB or saved HTML
without opening the reader. Refresh rereads that source; shared word
statuses rescore retained counts. The source release is prepared locally, without publication.

## Automated checks

- `cargo xtask ci`: **837 workspace tests passed**, 71 opt-in tests skipped.
  Formatting, clippy on Linux and Windows, architecture/source headers,
  interface strings in English, Italian and Japanese, licensing,
  requirement traceability and Flatpak sources passed.
- `cargo test -p nikomiru-app --test book_scores_l4 --locked`: **3 passed**.
  Actual dictionary segmentation counts repeated words across chapters;
  refresh reads changed text; a changed book identity, missing file,
  oversized source or cancellation never returns a partial count.
- `cargo test -p nikomiru-app --features test-harness --locked --test e2e book_scores:: -- --include-ignored --test-threads 1`:
  **2 passed** against the actual dictionary worker in a private off-screen
  KWin. A generated EPUB's whole-book count differs from the saved chapter's
  count, follows imported statuses and preserves reading progress. A saved
  web page refreshes from disk; limit/error outcomes remove the earlier
  score. Analysis alone creates no study segments or last-read timestamp.
- Pure tests cover text/ruby parity with ordinary reading, skipped image
  bytes, image-only/manga exclusions, damaged EPUBs, actual HTML read bounds
  independently of metadata, cancellation and aggregate decompressed EPUB
  limits. UI state tests cover one requested job, four retained entries,
  status rescoring, eviction and stale dictionary/path/request rejection.
  The component test checks that the list action never opens the reader,
  is hidden with colours off and is disabled during analysis.
- Full GUI regression:
  `cargo test -p nikomiru-app --features test-harness --locked --test e2e -- --include-ignored --skip l7_ --test-threads 1`:
  **90 passed, 0 failed**, 5 external/live checks filtered out (331 s).
  This includes the two whole-book cases, reader colours/chapter words,
  external/embedded episode scores, mining, X11 playback, accessibility,
  privacy checks, media servers, sync, audiobooks and web videos.

## Scope and remaining checks

Word colours must be enabled. Analysis uses only local EPUB/HTML text and
does not fetch a saved page again or load images. A dedicated worker keeps
the work separate from subtitle segmentation, episode scores and popup
lookups. Shutdown cancellation stops reading/counting and drops queued work.

Bounds are 32 MiB per source file, 4 MiB per decompressed text/metadata entry
or HTML read, 16 MiB of decompressed EPUB text/metadata in total, 512 spine
entries, 2 MiB of clean text, 8,192 nonempty paragraphs and 64 KiB per
paragraph. Exceeding a bound produces no partial score and does not change
normal reading limits. Four analysis results are retained in memory;
disabling colours, changing dictionaries or closing the app discards them.

A real Japanese novel still needs a manual comparison with a hand count.
Windows compilation passes; Windows runtime behavior remains a manual check.
Series-card scores and new words met per episode remain on M21's roadmap.

No Rust dependency, database schema or persisted configuration field was added.

The optimized 2.31.0 build passed `cargo build --release --locked -p nikomiru-app`;
its executable reports `nikomiru 2.31.0`. The binary, matching source archive
and SHA-256 checksums are retained in `target/releases/2.31.0/`.
