# ADR 0002 — Dictionary storage

| Field | Value |
|---|---|
| Status | Accepted |
| Date | 2026-10-19 |
| Requirements | DIC-013, DIC-020, NFR-PERF-04, NFR-PERF-05 |
| Risk | R-5 (redb performance not sufficient for lookup) |
| Spike | S2 — code in [`spikes/s2-storage/`](../../spikes/s2-storage/) |

## Context

SPECIFICATION §6.5.3 plans one redb file per dictionary, behind the `DictStore` trait (DIC-020). Spike S2 must tell whether redb meets the limits of NFR-PERF-04 (lookup) and NFR-PERF-05 (import) before `nikomiru-dict` is written in M3. If it does not, the custom format is adopted (mmap + hash table + blob, like fushidicts).

## Method

- **Data**: `cargo xtask fixtures --large` generates, with a fixed seed, 5 Yomitan term dictionaries of ~300k entries (147 MB of JSON each, average glossaries of 440 bytes, 10% as structured content), 3 frequency dictionaries in the three formats of DIC-003 (150k entries) and 1 pitch accent dictionary (200k entries). All of them derive from a shared lexicon of 400k pseudo-Japanese words: as in real dictionaries, the same word appears in several dictionaries.
- **Queries**: 10,000 sentences made of 2–4 words from the lexicon, inflected (-ます, -ない, -た, -て, -ている, -ば forms, …) and joined by particles. Lookup starts from the beginning of the sentence.
- **Import**: schema of SPECIFICATION §6.5.3 (`terms`, `term_by_expression`, `term_by_reading`, `term_meta`, `info`), records in postcard, JSON glossary compressed with lz4. A single write transaction per dictionary (DIC-013). Indexes are written sorted by key. One process per import, so `VmHWM` measures only that import.
- **Lookup** (5 term + 3 freq + 1 pitch, as in T-PERF-04): prefixes of the first 16 code points, longest first (LKP-004); kana variants (unchanged, katakana→hiragana, hiragana→katakana); deinflector with 137 suffix rules and bitmask conditions, recursive up to depth 10 (structure of LKP-021/022); search by expression and by reading in each dictionary; part-of-speech filter (LKP-023); merge by (expression, reading); frequencies after deduplication; sorting; truncation to 10 groups; then pitch accent and decompression of the glossaries of the remaining groups (LKP-035). Each query opens one read transaction per dictionary. Two passes over the same 10k queries.
- **Machine**: AMD Ryzen 7 7800X3D (8 cores), 61 GiB of RAM, Samsung 970 EVO NVMe in a USB enclosure, btrfs on LUKS; Rust 1.99.1, redb 4.4.0, release profile.

## Results

### Import (NFR-PERF-05: ≤ 60 s, peak RAM ≤ 1 GiB)

| Dictionary | Entries | JSON | Time | Peak RSS | redb file |
|---|---|---|---|---|---|
| synth-term-1…5 | 299,477–300,000 | 147 MB | **2.3 s** | **83–86 MiB** | 269 MB; **160 MB** after `compact()` (+0.5 s) |
| synth-freq-1…3 | 150k meta | 4–15 MB | 0.2–0.3 s | 27–31 MiB | 8 MB |
| synth-pitch | 200k meta | 15 MB | 0.3 s | 36 MiB | 8 MB |

### Lookup (NFR-PERF-04: p95 ≤ 10 ms, p99 ≤ 25 ms)

Average results per query: 9.4 groups, 37 glossaries; all 10k queries have results.

| Condition | redb cache per dictionary | p50 | p95 | p99 | max | Process RSS |
|---|---|---|---|---|---|---|
| OS page cache dropped (`posix_fadvise DONTNEED`), 1st pass | 16 MiB | 0.41 ms | **4.74 ms** | **13.9 ms** | 416 ms (first query) | — |
| Warm cache, 2nd pass | 16 MiB | 0.34 ms | **2.36 ms** | **3.59 ms** | 5.5 ms | 116 MiB |
| Freshly written files, 1st pass | 64 MiB | 0.32 ms | 1.69 ms | 2.51 ms | 11.4 ms | 364 MiB |
| Warm cache, 2nd pass | 64 MiB | — | 1.64 ms | 2.31 ms | — | 364 MiB |

## Decision

1. **We adopt redb** as the first implementation of `DictStore` (DIC-020). The limits are met with a margin of 25× on import, 4× on warm p95 and 2× on cold p99. The custom format remains plan B behind the same trait, but there is no need to write it.
2. Guidance for `nikomiru-dict` (M3):
   - import in **a single transaction**, with the multimap indexes written **in key order**;
   - **`compact()` after import**, before the atomic rename of DIC-013: the file shrinks by 40%;
   - **16 MiB redb cache per dictionary**, not the default one (1 GiB). With 64 MiB the p95 improves by only 0.7 ms, but RSS rises by 250 MiB, and NFR-PERF-09 (≤ 400 MB at idle, WebKit included) does not allow that;
   - records read with postcard **without copies** (`Cow` borrowed from the redb bytes) and glossaries decompressed **only after truncation**.

## Spike limits and residual risks

- **The M3 lookup will do more work.** The full LKP-011 chain (NFKC, width, kanji variants, iteration marks, numerals) generates more variants. Mixed orthography (LKP-031) adds up to 64 searches per reading, and the Yomitan tables have more rules. Conservative estimate: 2–3× today's searches, so p95 ~5–7 ms warm, ~10–14 ms cold. The LRU result cache (LKP-038) and memoization of deinflections per prefix are the first countermeasures. T-PERF-04 in M3 will verify this with the real engine.
- **Per-record lz4 compresses short glossaries poorly.** In M3 it is measured on real dictionaries (JMdict, with `NIKOMIRU_REALDATA`), and block compression of records is evaluated if disk space is a problem.
- **Hardware**: this machine is faster than REF (SPECIFICATION §7) in compute and slower in I/O (NVMe over USB and LUKS encryption). The margins make a failure on REF unlikely, but T-PERF-04/05 must be repeated on REF in M3.
- The data is synthetic. Key distribution and glossary length imitate those of JMdict, but do not reproduce them.

## M3: the real engine

T-PERF-04/05 with `nikomiru-dict` (full LKP-011 chain, Yomitan `ja.json` tables, mixed orthography, redirects, the Kansai exception), same data and machine, under a load average of 5–12 from other builds:

| | Import (each term dictionary) | Lookup p50 | p95 | p99 | max |
|---|---|---|---|---|---|
| First version (one read transaction per query, records read for every hit) | 2.7–4.3 s, 205 MiB | 4.5 ms | **44 ms** | **132 ms** | 600 ms |
| Final | 2.9–3.5 s, 261 MiB, 175 MB file | **0.40 ms** | **1.7 ms** | **2.2 ms** | 5 ms |

The residual risk above was real: the engine made up to 3,000 queries per click (mostly mixed-orthography readings that exist nowhere) and single-kana readings hit 700+ records. What fixed it, in order of effect:

1. **Term heads in the indexes**: `term_by_expression` and `term_by_reading` hold id, headword, reading, rules, score and the redirect flag, so a key with hundreds of homophones is one sequential read; the record (glossary, tags) is read only for the groups shown, after truncation.
2. **A Bloom filter per dictionary** (10 bits per key, ~0.8% false positives, stored in the file): a query for a string that is in no index costs a few hashes.
3. **One reader per dictionary per lookup** (one read transaction, tables opened once).
4. **Redirects flagged at import**, so a lookup decompresses only the glossaries that are redirects.
5. **Frequencies read only for groups that can still enter the budget** (keys 1–4 of LKP-034 do not need them).

`cargo xtask bench` repeats T-PERF-04/05.

## How to repeat the measurements

```sh
cargo xtask fixtures --large
cargo build --release --manifest-path spikes/s2-storage/Cargo.toml
spikes/s2-storage/target/release/spike-s2-storage all target/fixtures/dicts/synth target/spike-s2
```

The full report is written to `target/spike-s2/report.json`.
