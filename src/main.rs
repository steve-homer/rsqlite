use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use rsqlite::{BlobFormat, CsvOptions, InvalidUtf8};

/// Extract data from SQLite database files as CSV.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the tables in a database.
    Tables {
        db: PathBuf,
        /// Include views.
        #[arg(long)]
        views: bool,
    },
    /// Export a table, a query, or every table as CSV.
    Export(ExportArgs),
}

#[derive(Args)]
struct ExportArgs {
    db: PathBuf,

    /// Table (or view) to export.
    #[arg(short, long, conflicts_with_all = ["query", "all"])]
    table: Option<String>,

    /// SQL query whose result set is exported.
    #[arg(short, long, conflicts_with = "all")]
    query: Option<String>,

    /// Export every table to <OUT_DIR>/<table>.csv.
    #[arg(long, requires = "out_dir")]
    all: bool,

    /// Also export views when using --all.
    #[arg(long, requires = "all")]
    views: bool,

    /// Output file for --table/--query (default: stdout).
    #[arg(short, long, conflicts_with = "all")]
    output: Option<PathBuf>,

    /// Output directory for --all.
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Field delimiter (single byte).
    #[arg(short, long, default_value = ",")]
    delimiter: char,

    /// Omit the header row.
    #[arg(long)]
    no_header: bool,

    /// Text written for NULL values.
    #[arg(long, default_value = "")]
    null: String,

    /// How BLOB values are rendered.
    #[arg(long, value_enum, default_value_t = BlobFormat::Hex)]
    blob: BlobFormat,

    /// How TEXT values that are not valid UTF-8 are handled.
    #[arg(long, value_enum, default_value_t = InvalidUtf8::Raw)]
    invalid_utf8: InvalidUtf8,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Tables { db, views } => {
            let conn = rsqlite::open_readonly(&db)?;
            let mut out = io::stdout().lock();
            for name in rsqlite::list_tables(&conn, views)? {
                writeln!(out, "{name}")?;
            }
            Ok(())
        }
        Command::Export(args) => export(args),
    }
}

fn export(args: ExportArgs) -> Result<()> {
    if !args.delimiter.is_ascii() {
        bail!("delimiter must be a single ASCII character");
    }
    let opts = CsvOptions {
        delimiter: args.delimiter as u8,
        header: !args.no_header,
        null: args.null,
        blob: args.blob,
        invalid_utf8: args.invalid_utf8,
    };
    let conn = rsqlite::open_readonly(&args.db)?;

    if args.all {
        let dir = args.out_dir.expect("clap enforces --out-dir with --all");
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let tables = rsqlite::list_tables(&conn, args.views)?;
        for (table, stem) in tables.iter().zip(rsqlite::unique_file_stems(&tables)) {
            let path = dir.join(format!("{stem}.csv"));
            let file =
                File::create(&path).with_context(|| format!("creating {}", path.display()))?;
            let n = rsqlite::export_table(&conn, table, &opts, BufWriter::new(file))?;
            eprintln!("{table}: {n} rows -> {}", path.display());
        }
        return Ok(());
    }

    let out: Box<dyn Write> = match &args.output {
        Some(path) => Box::new(BufWriter::new(
            File::create(path).with_context(|| format!("creating {}", path.display()))?,
        )),
        None => Box::new(BufWriter::new(io::stdout().lock())),
    };

    match (args.table, args.query) {
        (Some(table), None) => rsqlite::export_table(&conn, &table, &opts, out)?,
        (None, Some(sql)) => rsqlite::export_query(&conn, &sql, &opts, out)?,
        _ => bail!("specify one of --table, --query or --all"),
    };
    Ok(())
}
