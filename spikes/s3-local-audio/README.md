# Spike S3: the local audio database

Can a database of the Local Audio Server for Yomichan (`android.db`, SQLite) be read for AUD-003 within the criteria of D-2: a lookup by (expression, reading) with p95 ≤ 20 ms, and no license outside Class A/B in the dependency graph? Outcome: [ADR 0004](../../docs/adr/0004-local-audio-db.md).

No real `android.db` was at hand, and its audio is third-party content: `make_db.py` writes a synthetic one with the same schema and indexes, 300 000 entries and 2.2 GB of random "audio".

```sh
python3 spikes/s3-local-audio/make_db.py target/spike-s3/android.db 300000
cargo build --release --manifest-path spikes/s3-local-audio/Cargo.toml
spikes/s3-local-audio/target/release/spike-s3-local-audio target/spike-s3/android.db target/spike-s3/android.db.words
# Nikomiru's own reader, on the same database:
cargo test --release -p nikomiru-dict --test local_audio_l4 -- --include-ignored --nocapture the_spike_database
```
