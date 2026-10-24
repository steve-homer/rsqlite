//! Core library for extracting SQLite data as CSV.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

/// How BLOB values are rendered in CSV output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum BlobFormat {
    /// Lowercase hexadecimal, e.g. `deadbeef`.
    #[default]
    Hex,
    /// Placeholder text `<BLOB n bytes>`; useful for eyeballing data.
    Summary,
}

/// What to do with TEXT values that are not valid UTF-8 (SQLite does not enforce it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum InvalidUtf8 {
    /// Write the stored bytes unchanged (lossless, but the CSV is not valid UTF-8).
    #[default]
    Raw,
    /// Replace invalid sequences with U+FFFD.
    Replace,
    /// Fail the export.
    Error,
}

/// Options controlling CSV rendering.
#[derive(Debug, Clone)]
pub struct CsvOptions {
    pub delimiter: u8,
    pub header: bool,
    /// Text written for SQL NULL.
    pub null: String,
    pub blob: BlobFormat,
    pub invalid_utf8: InvalidUtf8,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            header: true,
            null: String::new(),
            blob: BlobFormat::Hex,
            invalid_utf8: InvalidUtf8::Raw,
        }
    }
}

/// Opens a database read-only so extraction can never modify the source file.
pub fn open_readonly(path: &Path) -> Result<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)
        .with_context(|| format!("opening {}", path.display()))?;
    // Opening is lazy; touch the schema so a non-database file fails here.
    conn.query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(()))
        .with_context(|| format!("reading schema of {}", path.display()))?;
    Ok(conn)
}

/// Lists user tables (and optionally views), excluding SQLite internal objects.
pub fn list_tables(conn: &Connection, include_views: bool) -> Result<Vec<String>> {
    let sql = if include_views {
        "SELECT name FROM sqlite_schema WHERE type IN ('table','view') \
         AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name"
    } else {
        "SELECT name FROM sqlite_schema WHERE type = 'table' \
         AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// Quotes an identifier for safe interpolation into SQL.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Exports every row of `table` as CSV. Returns the number of data rows written.
pub fn export_table<W: Write>(
    conn: &Connection,
    table: &str,
    opts: &CsvOptions,
    out: W,
) -> Result<u64> {
    let sql = format!("SELECT * FROM {}", quote_ident(table));
    export_query(conn, &sql, opts, out).with_context(|| format!("exporting table {table:?}"))
}

/// Runs `sql` and writes the result set as CSV. Returns the number of data rows written.
pub fn export_query<W: Write>(
    conn: &Connection,
    sql: &str,
    opts: &CsvOptions,
    out: W,
) -> Result<u64> {
    let mut stmt = conn.prepare(sql)?;
    let ncols = stmt.column_count();
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(opts.delimiter)
        .from_writer(out);

    let columns: Vec<String> = stmt.column_names().map(String::from).collect();
    if opts.header {
        wtr.write_record(&columns)?;
    }

    let mut rows = stmt.query([])?;
    let mut count = 0u64;
    let mut record: Vec<Vec<u8>> = vec![Vec::new(); ncols];
    while let Some(row) = rows.next()? {
        for (i, field) in record.iter_mut().enumerate() {
            field.clear();
            render_value(row.get_ref(i)?, opts, field)
                .with_context(|| format!("row {}, column {:?}", count + 1, columns[i]))?;
        }
        wtr.write_record(&record)?;
        count += 1;
    }
    wtr.flush()?;
    Ok(count)
}

fn render_value(v: ValueRef<'_>, opts: &CsvOptions, buf: &mut Vec<u8>) -> Result<()> {
    match v {
        ValueRef::Null => buf.extend_from_slice(opts.null.as_bytes()),
        ValueRef::Integer(i) => buf.extend_from_slice(i.to_string().as_bytes()),
        // Debug formatting gives the shortest round-trippable form and switches to
        // exponent notation for very large/small magnitudes (1e308, not 309 digits).
        ValueRef::Real(f) => buf.extend_from_slice(format!("{f:?}").as_bytes()),
        ValueRef::Text(t) => match (opts.invalid_utf8, std::str::from_utf8(t)) {
            (_, Ok(_)) | (InvalidUtf8::Raw, Err(_)) => buf.extend_from_slice(t),
            (InvalidUtf8::Replace, Err(_)) => {
                buf.extend_from_slice(String::from_utf8_lossy(t).as_bytes())
            }
            (InvalidUtf8::Error, Err(e)) => bail!("text is not valid UTF-8: {e}"),
        },
        ValueRef::Blob(b) => match opts.blob {
            BlobFormat::Hex => {
                for byte in b {
                    write!(buf, "{byte:02x}").expect("writing to Vec cannot fail");
                }
            }
            BlobFormat::Summary => {
                write!(buf, "<BLOB {} bytes>", b.len()).expect("writing to Vec cannot fail")
            }
        },
    }
    Ok(())
}

/// Turns a table name into a safe file stem.
pub fn file_stem_for(table: &str) -> String {
    table
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Maps table names to distinct file stems, appending `_2`, `_3`, ... on collision.
/// Comparison ignores case so the files stay distinct on case-insensitive filesystems.
pub fn unique_file_stems(tables: &[String]) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    tables
        .iter()
        .map(|table| {
            let base = file_stem_for(table);
            let mut stem = base.clone();
            let mut n = 2;
            while !used.insert(stem.to_lowercase()) {
                stem = format!("{base}_{n}");
                n += 1;
            }
            stem
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_stems_resolve_collisions() {
        let tables: Vec<String> = ["a b", "a_b", "a_b_2", "A-B", "a-b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            unique_file_stems(&tables),
            ["a_b", "a_b_2", "a_b_2_2", "A-B", "a-b_2"]
        );
    }
}
