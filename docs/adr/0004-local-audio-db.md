# ADR 0004 — The local audio database

| Field | Value |
|---|---|
| Status | Proposed (2026-10-25): S3 met the speed criterion with `turso` but not the license one; Nikomiru reads the file itself. Waiting for the maintainer, who asked for "the best scenario" |
| Date | 2026-10-25 |
| Requirements | AUD-003, AUD-004, NFR-SEC-02 |
| Milestone | M5 (spike S3, decision D-2) |

## Context

AUD-003 reads the word audio of the Local Audio Server for Yomichan: one SQLite file, `android.db`, with `entries(expression, reading, source, speaker, display, file)` (the words and the sub-source of each audio: `nhk16`, `forvo`, `jpod`, …) and `android(file, source, data)` (the audio itself, a few kilobytes of MP3 or Opus per row). The real file ships with an index on `entries(expression, reading)` and one on `android(file, source)` (Fushi, `local_audio_db.dart`, BUG-1667). A lookup is: the rows of the exact pair, else of the expression alone; the sub-source preferred; its blob.

SQLite's own library is public domain, not in the license list (SPECIFICATION §3.6). D-2 approved reading the file with `turso` (MIT), a Rust rewrite of SQLite, if spike S3 showed that a lookup takes p95 ≤ 20 ms on a real `android.db` and that its dependency graph stays within Class A/B; if not, the decision is reopened between (b) an exception for the system SQLite library and (c) dropping AUD-003.

## The spike

No real `android.db` was at hand, and its audio is third-party content. `spikes/s3-local-audio/make_db.py` (now `xtask/scripts/android_db_large.py`; the spike is in the tag `v1.0.0`) writes a synthetic one with Python's `sqlite3` (SQLite itself): the same schema and indexes, 300 000 entries over four sub-sources (forvo with up to four speakers per word), 2.2 GB of random "audio" of 2–12 KB per row. 2000 lookups, a tenth of them for absent words and a third with a reading the file does not have (so the expression alone is asked), each including the blob of the file chosen. NVMe disk; "cold" means the file dropped from the page cache first (`posix_fadvise(DONTNEED)`).

| Reader | open | p50 warm | p95 warm | p95 cold | max cold |
|---|---|---|---|---|---|
| `turso` 0.8.3 (read-only, async API driven by a polling `block_on`) | 0.5–1.1 ms | 33 µs | 51 µs | 0.86–0.98 ms | 3–5.5 ms |
| Nikomiru's reader (below) | < 1 ms | — | 72 µs | 1.13 ms | — |

Both found the same 1712 words of 2000. The speed criterion holds by four orders of magnitude.

The license criterion does not. `cargo deny check licenses` with the project's `deny.toml` rejects, in `turso`'s graph: `bindgen` (BSD-3-Clause, a build dependency of `turso_sdk_kit`), `sha1_smol` (BSD-3-Clause, through `uuid`), `subtle` (BSD-3-Clause, through `aes-gcm` for encryption) and six ICU crates outside the current exception (`icu_collator`, `icu_locale` and their data and fallback, Unicode-3.0). Depending on `turso_core` directly, without its default features, drops `bindgen` and `sha1_smol`; `aes-gcm` (hence `subtle`) and the ICU collation are not optional. The graph is also large: a full database engine, pre-1.0 and changing every few weeks, for two lookups through an index.

## Decision

1. **Nikomiru reads the file itself** (`nikomiru-dict::sqlite`): as much of the SQLite file format, which is documented and public domain (<https://www.sqlite.org/fileformat2.html>), as these lookups need. The 100-byte header (page size, reserved bytes, UTF-8 text only), the schema table, table and index b-trees (interior and leaf pages, cell pointers, varints), records (every serial type), payloads that spill to overflow pages, and the BINARY order of values. It never writes: no journal, no WAL, no locks taken. Around 500 lines, no new dependency, GPL-3.0-only like the rest.
2. **Untrusted input.** The file is the user's: every read is bounded (page numbers within the file, cells within their page, payloads at most 64 MiB, overflow chains no longer than their payload needs, b-trees at most 40 levels and as many pages as the file has); a malformed file is an error, never a panic or an endless walk. Tested on files cut short and on 300 files with random bytes changed.
3. **`nikomiru-dict::local_audio`** finds the tables and their columns from the schema's SQL, and an index by its columns whatever its name (the real file's `idx_expr_reading`, Fushi's `idx_entries_expr_read`); without one, a lookup reads the whole table (correct, slower). It refuses a file without the two tables or without any audio. Sub-sources are ordered by the source's `priority`, by default `nhk16`, `forvo`, `jpod`, then the others.
4. **Tests against SQLite itself.** `cargo xtask fixtures` writes small databases with Python's `sqlite3` (`xtask/scripts/android_db.py`): 1 KiB pages so that the b-trees are several levels deep and the blobs run over overflow chains, the same rows without indexes, one without audio, one of another kind.
5. **D-2** becomes option (d), Nikomiru's own reader, instead of (a) `turso`; (b) and (c) are not needed.

## Consequences

- AUD-003 works with no new dependency and no exception to the license policy.
- The reader covers what the Local Audio Server writes and what SQLite writes by default. Not covered, refused with an error: UTF-16 text, and anything written only to a `-wal` file that has not been checkpointed (the server's file is a plain database). `WITHOUT ROWID` tables are not needed by the format and not read.
- If AUD-003 one day needs real SQL (joins, other tables), this decision is reopened: `turso` remains a candidate once its graph fits the policy.
- The measurements are on a synthetic file: the maintainer, or a user, can run the spike on a real `android.db` with the commands of `spikes/s3-local-audio/README.md`.
