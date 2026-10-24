#!/usr/bin/env python3
"""Generate small, hand-crafted SQLite fixtures that exercise CSV edge cases.

Output goes to tests/fixtures/generated/. The files are committed so tests do
not need Python; re-run this script only when changing the fixtures:

    python3 scripts/gen_fixtures.py
"""

import os
import shutil
import sqlite3
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "generated"


def fresh(name: str) -> Path:
    path = OUT / name
    for suffix in ("", "-wal", "-shm", "-journal"):
        p = Path(str(path) + suffix)
        if p.exists():
            p.unlink()
    return path


def connect(name: str, **pragmas) -> sqlite3.Connection:
    conn = sqlite3.connect(fresh(name), isolation_level=None)
    for key, value in pragmas.items():
        conn.execute(f"PRAGMA {key}={value}")
    return conn


def values_db():
    """Every storage class plus the values that commonly break CSV writers."""
    conn = connect("values.db")
    conn.executescript(
        """
        CREATE TABLE ints  (id INTEGER PRIMARY KEY, label TEXT, v INTEGER);
        CREATE TABLE reals (id INTEGER PRIMARY KEY, label TEXT, v REAL);
        CREATE TABLE texts (id INTEGER PRIMARY KEY, label TEXT, v TEXT);
        CREATE TABLE blobs (id INTEGER PRIMARY KEY, label TEXT, v BLOB);
        -- No declared type: SQLite's dynamic typing allows anything per row.
        CREATE TABLE mixed (id INTEGER PRIMARY KEY, label TEXT, v);
        """
    )
    conn.executemany(
        "INSERT INTO ints(label, v) VALUES (?, ?)",
        [("zero", 0), ("one", 1), ("neg", -1), ("i64_max", 2**63 - 1),
         ("i64_min", -(2**63)), ("null", None)],
    )
    conn.executemany(
        "INSERT INTO reals(label, v) VALUES (?, ?)",
        [("zero", 0.0), ("neg_zero", -0.0), ("tenth", 0.1), ("third", 1 / 3),
         ("whole", 2.0), ("big", 1e308), ("tiny", 5e-324), ("neg", -123.456),
         ("inf", float("inf")), ("neg_inf", float("-inf")), ("null", None)],
    )
    conn.executemany(
        "INSERT INTO texts(label, v) VALUES (?, ?)",
        [("empty", ""), ("null", None), ("comma", "a,b"), ("dquote", 'say "hi"'),
         ("squote", "it's"), ("newline", "line1\nline2"), ("crlf", "line1\r\nline2"),
         ("tab", "a\tb"), ("leading_space", "  padded  "), ("leading_zero", "007"),
         ("unicode", "Zoë – naïve café 日本語"), ("emoji", "🦀🎉"),
         ("rtl", "שלום"), ("null_literal", "NULL"), ("backslash", "C:\\path\\x"),
         ("long", "x" * 100_000)],
    )
    conn.executemany(
        "INSERT INTO blobs(label, v) VALUES (?, ?)",
        [("empty", b""), ("bytes", bytes(range(256))), ("ascii", b"hello"),
         ("null", None)],
    )
    conn.executemany(
        "INSERT INTO mixed(label, v) VALUES (?, ?)",
        [("int", 42), ("real", 4.2), ("text", "forty-two"), ("blob", b"\x00\x2a"),
         ("null", None)],
    )
    # Text column holding bytes that are not valid UTF-8.
    conn.execute("INSERT INTO texts(label, v) VALUES ('invalid_utf8', CAST(x'66ff6f' AS TEXT))")
    conn.close()


def schema_db():
    """Awkward identifiers and schema features."""
    conn = connect("schema.db")
    conn.executescript(
        """
        CREATE TABLE "order" ("select" INTEGER, "from" TEXT);
        INSERT INTO "order" VALUES (1, 'keywords');

        CREATE TABLE "my table" ("first name" TEXT, "last,name" TEXT, "quote""d" TEXT);
        INSERT INTO "my table" VALUES ('Ada', 'Lovelace', 'q');

        CREATE TABLE "naïve/表" ("ключ" TEXT);
        INSERT INTO "naïve/表" VALUES ('unicode identifiers');

        CREATE TABLE "../escape" (x);
        INSERT INTO "../escape" VALUES ('path traversal in name');

        -- These all map to the file stem "a_b" and must not overwrite each other.
        CREATE TABLE "a b" (v TEXT);
        INSERT INTO "a b" VALUES ('space');
        CREATE TABLE "a_b" (v TEXT);
        INSERT INTO "a_b" VALUES ('underscore');
        CREATE TABLE "a_b_2" (v TEXT);
        INSERT INTO "a_b_2" VALUES ('suffix lookalike');

        CREATE TABLE empty_table (a INTEGER, b TEXT);

        CREATE TABLE no_rowid (k TEXT PRIMARY KEY, v INTEGER) WITHOUT ROWID;
        INSERT INTO no_rowid VALUES ('b', 2), ('a', 1);

        CREATE TABLE strict_t (id INTEGER PRIMARY KEY, n INTEGER, s TEXT, any_col ANY) STRICT;
        INSERT INTO strict_t VALUES (1, 10, 'ten', 1.5);

        CREATE TABLE generated (
            a INTEGER,
            b INTEGER,
            total INTEGER GENERATED ALWAYS AS (a + b) VIRTUAL,
            label TEXT GENERATED ALWAYS AS ('sum=' || (a + b)) STORED
        );
        INSERT INTO generated(a, b) VALUES (1, 2), (10, 20);

        CREATE TABLE autoinc (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT);
        INSERT INTO autoinc(v) VALUES ('creates sqlite_sequence');

        CREATE TABLE dup_src (a INTEGER, b INTEGER);
        INSERT INTO dup_src VALUES (1, 2);
        CREATE VIEW dup_cols AS SELECT a AS x, b AS x FROM dup_src;

        CREATE VIEW v_order AS SELECT "from" AS what FROM "order";
        CREATE INDEX idx_gen ON generated(a);
        CREATE TRIGGER trg AFTER INSERT ON empty_table BEGIN SELECT 1; END;
        """
    )
    try:
        conn.executescript(
            "CREATE VIRTUAL TABLE docs USING fts5(title, body);"
            "INSERT INTO docs VALUES ('fts', 'virtual table with shadow tables');"
        )
    except sqlite3.OperationalError as exc:  # FTS5 not compiled in
        print(f"skipping fts5 table: {exc}")
    conn.close()


def empty_db():
    """A valid database with no tables, and a zero-byte file."""
    conn = connect("empty_schema.db")
    conn.execute("PRAGMA user_version=1")  # forces the header page to be written
    conn.close()
    fresh("zero_bytes.db").write_bytes(b"")


def wal_db():
    """WAL-mode database whose latest rows exist only in the -wal file."""
    conn = connect("wal.db", journal_mode="WAL", wal_autocheckpoint=0)
    conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    conn.executemany("INSERT INTO t(v) VALUES (?)", [("in_wal_1",), ("in_wal_2",)])
    # Copy while the connection is open so the WAL is not checkpointed on close.
    for suffix in ("", "-wal"):
        shutil.copy(str(OUT / "wal.db") + suffix, str(OUT / "wal_pending.db") + suffix)
    conn.close()
    # The original is now fully checkpointed; keep only the pending copy.
    for suffix in ("", "-wal", "-shm"):
        p = Path(str(OUT / "wal.db") + suffix)
        if p.exists():
            p.unlink()


def encoding_and_pages():
    """Non-default text encoding and page sizes."""
    for name, pragmas in [
        ("utf16le.db", {"encoding": "'UTF-16le'"}),
        ("utf16be.db", {"encoding": "'UTF-16be'"}),
        ("page512.db", {"page_size": 512}),
        ("page65536.db", {"page_size": 65536}),
    ]:
        conn = connect(name, **pragmas)
        conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
        conn.executemany("INSERT INTO t(v) VALUES (?)", [("ascii",), ("Zoë 日本語 🦀",)])
        conn.close()


def wide_and_tall():
    """Many columns, and enough rows to span many pages / overflow pages."""
    conn = connect("wide.db")
    ncols = 500
    cols = ", ".join(f"c{i} INTEGER" for i in range(ncols))
    conn.execute(f"CREATE TABLE wide ({cols})")
    conn.execute(f"INSERT INTO wide VALUES ({', '.join('?' * ncols)})", list(range(ncols)))
    conn.close()

    conn = connect("tall.db")
    conn.execute("CREATE TABLE tall (id INTEGER PRIMARY KEY, v TEXT)")
    conn.execute("BEGIN")
    conn.executemany("INSERT INTO tall(v) VALUES (?)", ((f"row {i}",) for i in range(50_000)))
    conn.execute("COMMIT")
    conn.close()


def broken_files():
    """Files the tool must reject cleanly."""
    fresh("not_a_database.db").write_text("this is plain text, not sqlite\n")
    conn = connect("truncated_src.db")
    conn.execute("CREATE TABLE t (v TEXT)")
    conn.executemany("INSERT INTO t VALUES (?)", (("x" * 500,) for _ in range(200)))
    conn.close()
    src = OUT / "truncated_src.db"
    data = src.read_bytes()
    fresh("truncated.db").write_bytes(data[: len(data) // 3])
    src.unlink()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    print(f"sqlite {sqlite3.sqlite_version} -> {OUT}")
    values_db()
    schema_db()
    empty_db()
    wal_db()
    encoding_and_pages()
    wide_and_tall()
    broken_files()
    for p in sorted(OUT.iterdir()):
        print(f"  {p.name:24} {os.path.getsize(p):>9} bytes")


if __name__ == "__main__":
    main()
