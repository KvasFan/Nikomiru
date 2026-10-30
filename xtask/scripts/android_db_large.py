# SPDX-License-Identifier: GPL-3.0-only
"""A large synthetic android.db with the schema of the Local Audio Server for
Yomichan, first written for spike S3 and kept for the L4 test of T-AUD-003: `entries(id, expression, reading, source, speaker,
display, file)` and `android(file, source, data)`, with the indexes the real
file ships with. The words are made up and the audio is random bytes: no
third-party content.

    python3 -I xtask/scripts/android_db_large.py target/fixtures/audio-large/android.db 300000

Writes OUT.db and OUT.db.words (the words to query, a tenth of them absent).
"""

import os
import random
import sqlite3
import sys

KANA = [chr(c) for c in range(0x3041, 0x3094)]
KANJI = [chr(c) for c in range(0x4E00, 0x4E00 + 3000)]
# sources and the share of entries each has, roughly as in the real file
SOURCES = [("forvo", 0.55), ("nhk16", 0.15), ("jpod", 0.15), ("shinmeikai8", 0.15)]


def word(rng):
    expression = "".join(rng.choice(KANJI) for _ in range(rng.randint(1, 3)))
    expression += "".join(rng.choice(KANA) for _ in range(rng.randint(0, 2)))
    reading = "".join(rng.choice(KANA) for _ in range(rng.randint(2, 6)))
    return expression, reading


def main():
    out = sys.argv[1]
    total = int(sys.argv[2]) if len(sys.argv) > 2 else 300_000
    rng = random.Random(42)
    if os.path.exists(out):
        os.remove(out)
    db = sqlite3.connect(out)
    db.executescript(
        """
        CREATE TABLE entries (
            id INTEGER PRIMARY KEY, expression TEXT NOT NULL, reading TEXT,
            source TEXT NOT NULL, speaker TEXT, display TEXT, file TEXT NOT NULL);
        CREATE TABLE android (file TEXT NOT NULL, source TEXT NOT NULL, data BLOB NOT NULL);
        """
    )
    words = []
    rows = 0
    n = 0
    while rows < total:
        expression, reading = word(rng)
        words.append((expression, reading))
        # a word is in one source or more; forvo has several speakers
        for source, share in SOURCES:
            if rng.random() > share * 1.6:
                continue
            speakers = rng.randint(1, 4) if source == "forvo" else 1
            for s in range(speakers):
                n += 1
                file = f"{source}/{n}.mp3" if source != "forvo" else f"{expression}/{s}.mp3"
                db.execute(
                    "INSERT INTO entries (expression, reading, source, speaker, display, file) "
                    "VALUES (?, ?, ?, ?, ?, ?)",
                    (expression, reading, source, f"speaker{s}", f"{expression} ({source})", file),
                )
                data = rng.randbytes(rng.randint(2_000, 12_000))
                db.execute("INSERT INTO android (file, source, data) VALUES (?, ?, ?)", (file, source, data))
                rows += 1
        if len(words) % 20_000 == 0:
            db.commit()
            print(f"{rows} entries", file=sys.stderr)
    db.executescript(
        """
        CREATE INDEX idx_expr_reading ON entries (expression, reading);
        CREATE INDEX idx_android ON android (file, source);
        """
    )
    db.commit()
    db.close()
    queried = rng.sample(words, min(2_000, len(words)))
    with open(out + ".words", "w", encoding="utf-8") as f:
        for i, (expression, reading) in enumerate(queried):
            if i % 10 == 0:
                expression += "無"  # absent
            # a third asked without the reading the file has
            if i % 3 == 1:
                reading = "".join(rng.choice(KANA) for _ in range(3))
            f.write(f"{expression}\t{reading}\n")
    print(f"{rows} entries, {len(words)} words, {os.path.getsize(out) >> 20} MiB", file=sys.stderr)


main()
