# rsqlite

A command-line tool that extracts data from SQLite database files as CSV.

- Exports a single table or view, the result of any SQL query, or every table at once.
- Opens databases read-only, so the source file is never modified.
- Ships with SQLite built in (via `rusqlite`'s `bundled` feature), so there's nothing extra to install.
- Handles awkward data: quotes, commas and newlines in values, BLOBs, NULLs, Unicode
  identifiers, UTF-16 databases, and WAL-mode databases with uncheckpointed changes.

## Install

```sh
cargo install --path .
```

Or build with `cargo build --release` and use `target/release/rsqlite`.

## Usage

List the tables in a database:

```console
$ rsqlite tables chinook.db
Album
Artist
Customer
...
```

Export a table to stdout:

```console
$ rsqlite export chinook.db -t Artist
ArtistId,Name
1,AC/DC
2,Accept
...
```

Export the result of a query to a file:

```console
$ rsqlite export chinook.db -o top_artists.csv -q "
    SELECT a.Name AS artist, count(*) AS albums
    FROM Album JOIN Artist a USING (ArtistId)
    GROUP BY a.Name ORDER BY albums DESC LIMIT 3"
$ cat top_artists.csv
artist,albums
Iron Maiden,21
Led Zeppelin,14
Deep Purple,11
```

Export every table (add `--views` to include views) into a directory, one CSV per table:

```console
$ rsqlite export chinook.db --all --out-dir csv/
Album: 347 rows -> csv/Album.csv
Artist: 275 rows -> csv/Artist.csv
...
```

Table names are turned into safe file names. If two names map to the same file
name, `_2`, `_3`, ... is appended.

### Output options

| Option | Default | Description |
| --- | --- | --- |
| `-d, --delimiter <C>` | `,` | Field delimiter (one ASCII character), e.g. `-d ';'` or `-d $'\t'` |
| `--no-header` | | Omit the header row |
| `--null <TEXT>` | empty | Text written for NULL, e.g. `--null '\N'` |
| `--blob hex\|summary` | `hex` | BLOBs as lowercase hex, or as a `<BLOB n bytes>` placeholder |
| `--invalid-utf8 raw\|replace\|error` | `raw` | TEXT that isn't valid UTF-8: copy the bytes unchanged, replace with U+FFFD, or fail |

Run `rsqlite export --help` for the full list.

### Value formatting

- **Integers** are written as-is.
- **Reals** use the shortest form that round-trips exactly (`0.1`, `2.0`,
  `0.3333333333333333`, `1e308`, `inf`). This can mean more digits than the
  `sqlite3` shell's 15 significant digits, but no precision is lost.
- **Text** is written as stored. SQLite doesn't enforce UTF-8, so see `--invalid-utf8`.
- **NULL** becomes the `--null` text, so by default it's indistinguishable from an
  empty string. Use `--null` when the difference matters.

## Development

```sh
cargo test                    # unit and integration tests
scripts/fetch_corpus.sh       # optional: download larger sample DBs for extra tests
python3 scripts/gen_fixtures.py   # regenerate the edge-case fixtures
```

The test databases are described in [`tests/fixtures/README.md`](tests/fixtures/README.md).
