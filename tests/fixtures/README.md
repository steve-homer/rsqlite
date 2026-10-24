# Test fixtures

## `corpus/`: real-world databases (committed)

| File | Source | License | Notes |
|---|---|---|---|
| `chinook.db` | [lerocha/chinook-database](https://github.com/lerocha/chinook-database) `Chinook_Sqlite.sqlite` | MIT (`chinook.LICENSE.md`) | 11 tables, ~15k rows, NUMERIC/DATETIME columns, FKs, Unicode names. 1 MB. |

## `external/`: larger databases (gitignored, fetched on demand)

Run `scripts/fetch_corpus.sh`. URLs are pinned to commits and checked by SHA-256.
Tests that need these files skip when they are missing.

| File | Source | License | Notes |
|---|---|---|---|
| `northwind.db` | [jpwhite3/northwind-SQLite3](https://github.com/jpwhite3/northwind-SQLite3) | MIT | 13 tables and 17 views, spaces in names (`Order Details`), BLOB images, 609k-row table. 25 MB. |
| `sakila.db` | [bradleygrant/sakila-sqlite3](https://github.com/bradleygrant/sakila-sqlite3) | BSD-3-Clause | 16 tables and 5 views, written by SQLite 3.11, 1 KiB pages, triggers. 6 MB. |

## `generated/`: edge cases (committed, built by `scripts/gen_fixtures.py`)

| File | Exercises |
|---|---|
| `values.db` | Every storage class: i64 min/max, ±0.0, ±inf, 1e308, subnormals, empty vs NULL, commas, quotes, CR/LF, tabs, emoji, RTL, 100 KB text, the literal string `NULL`, invalid UTF-8 in TEXT, empty or binary BLOBs, an untyped column with mixed types per row |
| `schema.db` | Keywords as identifiers (`"order"`, `"select"`), spaces, commas and quotes in names, Unicode names, `../escape` table name, names that sanitize to the same file (`a b`, `a_b`, `a_b_2`), empty table, `WITHOUT ROWID`, `STRICT`, generated columns (virtual and stored), `AUTOINCREMENT` (`sqlite_sequence`), a view with duplicate column names, FTS5 virtual table and shadow tables, index, trigger |
| `empty_schema.db`, `zero_bytes.db` | Valid databases with no tables |
| `wal_pending.db` + `-wal` | WAL mode with rows present only in the uncheckpointed WAL file |
| `utf16le.db`, `utf16be.db` | Non-UTF-8 database encodings |
| `page512.db`, `page65536.db` | Minimum and maximum page sizes |
| `wide.db`, `tall.db` | 500 columns; 50k rows |
| `not_a_database.db`, `truncated.db` | Files the tool must reject cleanly |

Tests that open WAL fixtures copy them to a temp dir first, so SQLite can't
create `-shm` files inside the repo.
