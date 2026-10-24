use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

fn rsqlite() -> Command {
    Command::cargo_bin("rsqlite").unwrap()
}

/// Runs the binary and parses stdout as CSV, returning (header, rows).
fn export_csv(db: &Path, extra: &[&str]) -> (Vec<String>, Vec<Vec<String>>) {
    let out = rsqlite()
        .arg("export")
        .arg(db)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Byte records: text columns may hold invalid UTF-8, which is passed through.
    let lossy = |f: &[u8]| String::from_utf8_lossy(f).into_owned();
    let mut rdr = csv::Reader::from_reader(out.stdout.as_slice());
    let header = rdr.byte_headers().unwrap().iter().map(lossy).collect();
    let rows = rdr
        .byte_records()
        .map(|r| r.unwrap().iter().map(lossy).collect())
        .collect();
    (header, rows)
}

/// Looks up the `v` column for the row whose `label` matches.
fn value_for<'a>(rows: &'a [Vec<String>], label: &str) -> &'a str {
    &rows
        .iter()
        .find(|r| r[1] == label)
        .unwrap_or_else(|| panic!("no row {label}"))[2]
}

/// Copies a fixture (and any -wal sidecar) into a temp dir so SQLite cannot
/// create -shm files next to the committed fixtures.
fn copy_to_temp(name: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    for suffix in ["", "-wal"] {
        let src = fixture(&format!("generated/{name}{suffix}"));
        if src.exists() {
            fs::copy(&src, dir.path().join(format!("{name}{suffix}"))).unwrap();
        }
    }
    let path = dir.path().join(name);
    (dir, path)
}

// --- real-world corpus ---------------------------------------------------

#[test]
fn chinook_lists_tables() {
    rsqlite()
        .args(["tables"])
        .arg(fixture("corpus/chinook.db"))
        .assert()
        .success()
        .stdout(
            "Album\nArtist\nCustomer\nEmployee\nGenre\nInvoice\nInvoiceLine\n\
             MediaType\nPlaylist\nPlaylistTrack\nTrack\n",
        );
}

#[test]
fn chinook_exports_table() {
    let (header, rows) = export_csv(&fixture("corpus/chinook.db"), &["-t", "Track"]);
    assert_eq!(header[..3], ["TrackId", "Name", "AlbumId"]);
    assert_eq!(rows.len(), 3503);
}

#[test]
fn chinook_exports_query() {
    let (header, rows) = export_csv(
        &fixture("corpus/chinook.db"),
        &["-q", "SELECT Name FROM Artist WHERE ArtistId = 1"],
    );
    assert_eq!(header, ["Name"]);
    assert_eq!(rows, [["AC/DC"]]);
}

#[test]
fn chinook_export_all() {
    let out = TempDir::new().unwrap();
    rsqlite()
        .arg("export")
        .arg(fixture("corpus/chinook.db"))
        .args(["--all", "--out-dir"])
        .arg(out.path())
        .assert()
        .success();
    assert_eq!(fs::read_dir(out.path()).unwrap().count(), 11);
}

/// Larger databases fetched by scripts/fetch_corpus.sh; skipped when absent.
#[test]
fn external_corpus_exports_all() {
    for (name, tables_and_views) in [("northwind.db", 30), ("sakila.db", 21)] {
        let db = fixture(&format!("external/{name}"));
        if !db.exists() {
            eprintln!("skipping {name}: run scripts/fetch_corpus.sh");
            continue;
        }
        let out = TempDir::new().unwrap();
        rsqlite()
            .arg("export")
            .arg(&db)
            .args(["--all", "--views", "--blob", "summary", "--out-dir"])
            .arg(out.path())
            .assert()
            .success();
        assert_eq!(
            fs::read_dir(out.path()).unwrap().count(),
            tables_and_views,
            "{name}"
        );
    }
}

// --- values --------------------------------------------------------------

#[test]
fn text_values_round_trip_through_csv() {
    let (_, rows) = export_csv(&fixture("generated/values.db"), &["-t", "texts"]);
    for (label, expected) in [
        ("empty", ""),
        ("comma", "a,b"),
        ("dquote", "say \"hi\""),
        ("newline", "line1\nline2"),
        ("crlf", "line1\r\nline2"),
        ("unicode", "Zoë – naïve café 日本語"),
        ("emoji", "🦀🎉"),
        ("leading_zero", "007"),
        ("leading_space", "  padded  "),
    ] {
        assert_eq!(value_for(&rows, label), expected, "{label}");
    }
    assert_eq!(value_for(&rows, "long").len(), 100_000);
}

#[test]
fn invalid_utf8_text_is_passed_through_unchanged() {
    let out = rsqlite()
        .arg("export")
        .arg(fixture("generated/values.db"))
        .args([
            "--no-header",
            "-q",
            "SELECT v FROM texts WHERE label = 'invalid_utf8'",
        ])
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"f\xffo\n");
}

#[test]
fn invalid_utf8_can_be_replaced() {
    let (_, rows) = export_csv(
        &fixture("generated/values.db"),
        &["-t", "texts", "--invalid-utf8", "replace"],
    );
    assert_eq!(value_for(&rows, "invalid_utf8"), "f\u{FFFD}o");
}

#[test]
fn invalid_utf8_can_be_an_error() {
    rsqlite()
        .arg("export")
        .arg(fixture("generated/values.db"))
        .args(["-t", "texts", "--invalid-utf8", "error"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("column \"v\"").and(predicate::str::contains("UTF-8")));
}

#[test]
fn null_marker_is_configurable() {
    let (_, rows) = export_csv(
        &fixture("generated/values.db"),
        &["-t", "texts", "--null", "\\N"],
    );
    assert_eq!(value_for(&rows, "null"), "\\N");
    assert_eq!(value_for(&rows, "empty"), "");
}

#[test]
fn integer_extremes() {
    let (_, rows) = export_csv(&fixture("generated/values.db"), &["-t", "ints"]);
    assert_eq!(value_for(&rows, "i64_max"), "9223372036854775807");
    assert_eq!(value_for(&rows, "i64_min"), "-9223372036854775808");
}

#[test]
fn real_formatting() {
    let (_, rows) = export_csv(&fixture("generated/values.db"), &["-t", "reals"]);
    assert_eq!(value_for(&rows, "tenth"), "0.1");
    assert_eq!(value_for(&rows, "whole"), "2.0");
    assert_eq!(value_for(&rows, "big"), "1e308");
    // Shortest round-trippable form: more digits than sqlite3's %.15g, but lossless.
    assert_eq!(value_for(&rows, "third"), "0.3333333333333333");
    // SQLite stores integral REALs as integers on disk, so the sign of -0.0 is lost.
    assert_eq!(value_for(&rows, "neg_zero"), "0.0");
    assert_eq!(value_for(&rows, "inf"), "inf");
    assert_eq!(value_for(&rows, "neg_inf"), "-inf");
}

#[test]
fn blobs_render_as_hex() {
    let (_, rows) = export_csv(&fixture("generated/values.db"), &["-t", "blobs"]);
    assert_eq!(value_for(&rows, "ascii"), "68656c6c6f");
    assert_eq!(value_for(&rows, "empty"), "");
    assert_eq!(value_for(&rows, "bytes").len(), 512);
}

#[test]
fn dynamically_typed_column() {
    let (_, rows) = export_csv(&fixture("generated/values.db"), &["-t", "mixed"]);
    let vals: Vec<&str> = rows.iter().map(|r| r[2].as_str()).collect();
    assert_eq!(vals, ["42", "4.2", "forty-two", "002a", ""]);
}

// --- schema --------------------------------------------------------------

#[test]
fn awkward_identifiers() {
    let db = fixture("generated/schema.db");
    let (header, rows) = export_csv(&db, &["-t", "my table"]);
    assert_eq!(header, ["first name", "last,name", "quote\"d"]);
    assert_eq!(rows, [["Ada", "Lovelace", "q"]]);

    let (header, _) = export_csv(&db, &["-t", "order"]);
    assert_eq!(header, ["select", "from"]);
}

#[test]
fn generated_columns_are_exported() {
    let (header, rows) = export_csv(&fixture("generated/schema.db"), &["-t", "generated"]);
    assert_eq!(header, ["a", "b", "total", "label"]);
    assert_eq!(rows[1], ["10", "20", "30", "sum=30"]);
}

#[test]
fn empty_table_writes_header_only() {
    let (header, rows) = export_csv(&fixture("generated/schema.db"), &["-t", "empty_table"]);
    assert_eq!(header, ["a", "b"]);
    assert!(rows.is_empty());
}

#[test]
fn export_all_sanitizes_file_names() {
    let out = TempDir::new().unwrap();
    rsqlite()
        .arg("export")
        .arg(fixture("generated/schema.db"))
        .args(["--all", "--out-dir"])
        .arg(out.path())
        .assert()
        .success();
    assert!(out.path().join("___escape.csv").exists());
    assert!(out.path().join("my_table.csv").exists());
    // Tables whose names sanitize to the same stem get distinct files.
    for (file, value) in [
        ("a_b.csv", "space"),
        ("a_b_2.csv", "underscore"),
        ("a_b_2_2.csv", "suffix lookalike"),
    ] {
        let csv = fs::read_to_string(out.path().join(file)).unwrap();
        assert_eq!(csv, format!("v\n{value}\n"), "{file}");
    }
    // Nothing escaped the output directory.
    assert!(!out.path().parent().unwrap().join("escape.csv").exists());
}

// --- file formats --------------------------------------------------------

#[test]
fn reads_uncheckpointed_wal() {
    let (_dir, db) = copy_to_temp("wal_pending.db");
    let (_, rows) = export_csv(&db, &["-t", "t"]);
    assert_eq!(rows, [["1", "in_wal_1"], ["2", "in_wal_2"]]);
}

#[test]
fn non_default_encodings_and_page_sizes() {
    for name in ["utf16le.db", "utf16be.db", "page512.db", "page65536.db"] {
        let (_, rows) = export_csv(&fixture(&format!("generated/{name}")), &["-t", "t"]);
        assert_eq!(rows[1][1], "Zoë 日本語 🦀", "{name}");
    }
}

#[test]
fn wide_and_tall_tables() {
    let (header, _) = export_csv(&fixture("generated/wide.db"), &["-t", "wide"]);
    assert_eq!(header.len(), 500);
    let (_, rows) = export_csv(&fixture("generated/tall.db"), &["-t", "tall"]);
    assert_eq!(rows.len(), 50_000);
}

#[test]
fn empty_databases_have_no_tables() {
    for name in ["empty_schema.db", "zero_bytes.db"] {
        rsqlite()
            .arg("tables")
            .arg(fixture(&format!("generated/{name}")))
            .assert()
            .success()
            .stdout("");
    }
}

#[test]
fn rejects_non_database() {
    rsqlite()
        .arg("tables")
        .arg(fixture("generated/not_a_database.db"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a database"));
}

#[test]
fn rejects_truncated_database() {
    let out = TempDir::new().unwrap();
    rsqlite()
        .arg("export")
        .arg(fixture("generated/truncated.db"))
        .args(["--all", "--out-dir"])
        .arg(out.path())
        .assert()
        .failure();
}
