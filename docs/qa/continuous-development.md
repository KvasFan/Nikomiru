# Continuous development — 2026-11-13

Development continued through three consecutive local releases instead of
stopping after the whole-book increment. M21's remaining implementation is
complete; real-material comparisons and a real two-device encounter sync
remain manual exit criteria.

| Version | Increment | Validation |
|---|---|---|
| 2.31.0 | Whole-book comprehension, KNW-016 | 837 workspace tests, 90 GUI tests; optimized build |
| 2.32.0 | Weighted series-card comprehension and source coverage, KNW-017 | 842 workspace tests; model/component tests and actual subtitle GUI case; optimized build |
| 2.33.0 | Historical new words encountered per episode, KNW-018 | 847 workspace tests, 92 GUI tests; backup restore and sync compatibility/property tests; optimized build |

Each optimized executable was checked with `--version`. Matching source
archives are retained beside the executables, licenses and SHA-256 checksums
under `target/releases/2.31.0/`, `2.32.0/` and `2.33.0/`. Archive manifests and
required feature files were checked against each version. This initial
validation did not push, tag, publish or install the artifacts.

The subsequent release request targets the cumulative **v2.33.0** GitHub
release, including the increments prepared as 2.27.0 through 2.33.0. Only
2.33.0 is published from this batch; earlier local archives remain development
checkpoints. The release carries a Linux executable bundle, matching source
archive and portable SHA-256 checksums.

Detailed behavior and limits are in [whole-book-scores.md](whole-book-scores.md),
[series-scores.md](series-scores.md) and [word-encounters.md](word-encounters.md).

Final GUI regression: **92 passed, 0 failed**, 5 external/live checks
filtered out, in 322 s. Final traceability passed with 328 requirements and
1,067 markers. `git diff --check` is clean. Windows compilation was checked;
runtime testing on Windows remains manual.
