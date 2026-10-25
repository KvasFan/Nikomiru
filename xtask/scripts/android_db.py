# SPDX-License-Identifier: GPL-3.0-only
"""Small databases in the format of the Local Audio Server for Yomichan, for
the tests of AUD-003 (TEST §4): written by SQLite itself (Python's sqlite3),
so the reader is checked against the real format. Made-up words, and audio
that is text naming its own file, so a test can tell which file came back.

    python3 -I android_db.py OUT_DIR

- android.db: 1 KiB pages (b-trees several levels deep, blobs on overflow
  chains), the indexes the real file has;
- android-noindex.db: the same rows, no index, 4 KiB pages;
- android-empty.db: the tables, no audio;
- other.db: an SQLite file of another kind.
"""

import os
import sqlite3
import sys

SCHEMA = """
CREATE TABLE entries (
    id INTEGER PRIMARY KEY, expression TEXT NOT NULL, reading TEXT,
    source TEXT NOT NULL, speaker TEXT, display TEXT, file TEXT NOT NULL);
CREATE TABLE android (file TEXT NOT NULL, source TEXT NOT NULL, data BLOB NOT NULL);
"""
INDEXES = """
CREATE INDEX idx_expr_reading ON entries (expression, reading);
CREATE INDEX idx_android ON android (file, source);
"""

# the words the tests ask for: (expression, reading, source, file, blob size)
KNOWN = [
    ("食べる", "たべる", "forvo", "食べる/a.mp3", 300),
    ("食べる", "たべる", "forvo", "食べる/b.mp3", 300),
    ("食べる", "たべる", "jpod", "jpod/1.mp3", 300),
    ("食べる", "たべる", "nhk16", "nhk16/1.mp3", 50_000),
    ("行く", "いく", "jpod", "jpod/2.mp3", 300),
    ("行く", "ゆく", "nhk16", "nhk16/2.mp3", 300),
    ("飲む", "のむ", "shinmeikai8", "shinmeikai8/1.mp3", 3_000),
]


def blob(file, source, size):
    text = f"{source}|{file}|".encode()
    return (text * (size // len(text) + 1))[:size]


def rows():
    out = list(KNOWN)
    # enough filler to make the b-trees several pages deep
    for i in range(1500):
        word = f"語{i:04d}"
        out.append((word, f"ご{i:04d}", ["forvo", "jpod", "nhk16"][i % 3], f"filler/{i}.mp3", 64))
    return out


def write(path, page_size, indexes, audio=True):
    if os.path.exists(path):
        os.remove(path)
    db = sqlite3.connect(path)
    db.execute(f"PRAGMA page_size = {page_size}")
    db.executescript(SCHEMA)
    for expression, reading, source, file, size in rows():
        db.execute(
            "INSERT INTO entries (expression, reading, source, speaker, display, file) "
            "VALUES (?, ?, ?, 'x', ?, ?)",
            (expression, reading, source, expression, file),
        )
        if audio:
            db.execute("INSERT INTO android (file, source, data) VALUES (?, ?, ?)", (file, source, blob(file, source, size)))
    if indexes:
        db.executescript(INDEXES)
    db.commit()
    db.close()


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    write(os.path.join(out, "android.db"), 1024, True)
    write(os.path.join(out, "android-noindex.db"), 4096, False)
    write(os.path.join(out, "android-empty.db"), 4096, True, audio=False)
    other = os.path.join(out, "other.db")
    if os.path.exists(other):
        os.remove(other)
    db = sqlite3.connect(other)
    db.executescript("CREATE TABLE notes (id INTEGER PRIMARY KEY, text TEXT); INSERT INTO notes (text) VALUES ('x');")
    db.commit()
    db.close()


main()
