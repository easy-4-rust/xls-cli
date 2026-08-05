//! Command-line front-end (clap derive). All subcommands live here.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 命令实现以原回归测试为迁移门槛，避免纯风格重写改变行为"
)]

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use super::easyexcel_components as core;
use super::easyexcel_components::formula::{CellRef, Engine};
use super::easyexcel_components::model::{Cell, Workbook};
use super::easyexcel_components::value::CellValue;
use super::{render, stream};

// ─── Top-level CLI ──────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "xls",
    version,
    about = "A terminal spreadsheet for XLS/XLSX/CSV"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// A file to open in the interactive TUI (when no subcommand is given).
    file: Option<String>,

    /// Password for an encrypted (password-protected) workbook.
    #[arg(long, short = 'p', global = true)]
    password: Option<String>,

    /// For mutating commands: print what would change instead of writing.
    #[arg(long, global = true)]
    dry_run: bool,

    /// For mutating commands: write a `<file>.bak` copy before overwriting.
    #[arg(long, global = true)]
    backup: bool,

    /// For mutating commands: write the result to this path instead of
    /// overwriting the input file (the `-o`/copy-out shortcut).
    #[arg(long, global = true)]
    output: Option<PathBuf>,
}

/// Resolved save behavior for mutating commands (from the global flags).
#[derive(Clone)]
struct SaveOpts {
    dry_run: bool,
    backup: bool,
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Open a file in the interactive TUI.
    Open {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
    },

    /// Print file metadata: format, sheets, dimensions, date system.
    Info {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
    },

    /// Print a cell or range value(s) to stdout.
    ///
    /// The reference may be a single cell (`A1`, `Sheet1!A1`) or a range
    /// (`A1:J200`, `Sheet0!A1:J200`). Use `--format` to choose the output
    /// shape and `--raw` for stored (unformatted) values.
    Get {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Cell or range reference, e.g. `A1` or `Sheet0!A1:J200`.
        cell: String,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Set a cell value and save the file.
    ///
    /// Values: a number, `true`/`false`, a formula starting with `=`, or text.
    Set {
        /// Path to the spreadsheet.
        file: String,
        /// Cell reference, e.g. `A1` or `Sheet1!A1`.
        cell: String,
        /// New value.
        value: String,
    },

    /// Evaluate a formula against the file's data.
    ///
    /// Array-returning formulas (e.g. `=FILTER(...)`) print as a grid in the
    /// chosen `--format`; scalars print as a single value.
    Eval {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Formula to evaluate, e.g. `=SUM(A1:A10)`.
        formula: String,
        /// Cell context for relative references (default: Sheet1!A1).
        #[arg(long)]
        at: Option<String>,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Print a cell's number format, e.g. `DATE dd/mm/yyyy` or `NUMBER 0.00`.
    Format {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Cell reference, e.g. `C2` or `Sheet1!C2`.
        cell: String,
    },

    /// Set the number format of a cell or range and save.
    FormatSet {
        /// Path to the spreadsheet.
        file: String,
        /// Cell or range, e.g. `C2` or `C2:C154`.
        range: String,
        /// Number-format code, e.g. `dd/mm/yyyy` or `#,##0.00`.
        code: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Convert a spreadsheet to another format.
    ///
    /// Text formats (csv/tsv/json/jsonl/md) may be streamed to stdout with
    /// `-o -`.
    Export {
        /// Path to the source file (or `-` to read CSV from stdin).
        file: String,
        /// Output format.
        #[arg(long, short = 'f')]
        format: ExportFormat,
        /// Sheet to export (name; defaults to the active sheet).
        #[arg(long, short = 's')]
        sheet: Option<String>,
        /// Output path (`-` for stdout; derived from input stem if omitted).
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
        /// Stream rows without loading the whole workbook (memory-bounded; for
        /// huge .xlsx/.csv files). Supports csv/tsv/jsonl; uses cached formula
        /// values (no recalculation). Other formats fall back to in-memory.
        #[arg(long)]
        stream: bool,
        #[command(flatten)]
        read: RawArgs,
    },

    /// Import a CSV file as a sheet into an existing (or new) workbook.
    Import {
        /// Path to the CSV file (or `-` to read from stdin).
        csv: String,
        /// Target workbook to add the sheet into.
        #[arg(long)]
        into: PathBuf,
        /// Name for the imported sheet (defaults to the CSV file stem).
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Show differences between two spreadsheets.
    ///
    /// Without `--key`, compares cell-by-cell (positionally). With `--key`,
    /// matches rows by the value in that column and reports rows
    /// added/removed/changed by key (robust when rows shift between versions).
    /// Exits 0 if identical, 1 if differences are found.
    Diff {
        /// First file.
        file1: String,
        /// Second file.
        file2: String,
        /// Key column (letter or header name) for a keyed, row-wise diff.
        #[arg(long)]
        key: Option<String>,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Clear every cell in a range and save (e.g. `A1:B10` or `A1`).
    Clear {
        /// Path to the spreadsheet.
        file: String,
        /// Range to clear, e.g. `A1:B10`.
        range: String,
        /// Sheet name (defaults to the active sheet).
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Set every cell in a range to the same value and save.
    ///
    /// Formula values are stored verbatim (references are not relatively
    /// adjusted across the range).
    Fill {
        /// Path to the spreadsheet.
        file: String,
        /// Range to fill, e.g. `A1:A10`.
        range: String,
        /// Value: number, `true`/`false`, `=formula`, or text.
        value: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Copy a range to a destination anchor cell and save.
    ///
    /// Cells are copied verbatim (formulas are not reference-adjusted).
    Copy {
        /// Path to the spreadsheet.
        file: String,
        /// Source range, e.g. `A1:B3`.
        src: String,
        /// Destination top-left cell, e.g. `D1`.
        dest: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Move a range to a destination anchor cell (copy then clear source).
    Move {
        /// Path to the spreadsheet.
        file: String,
        /// Source range, e.g. `A1:B3`.
        src: String,
        /// Destination top-left cell, e.g. `D1`.
        dest: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Insert blank rows above a 1-based row number and save.
    InsertRow {
        /// Path to the spreadsheet.
        file: String,
        /// 1-based row number to insert before, e.g. `3`.
        row: u32,
        /// How many rows to insert.
        #[arg(long, short = 'n', default_value_t = 1)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Delete rows starting at a 1-based row number and save.
    DeleteRow {
        /// Path to the spreadsheet.
        file: String,
        /// 1-based row number to delete from, e.g. `3`.
        row: u32,
        /// How many rows to delete.
        #[arg(long, short = 'n', default_value_t = 1)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Insert blank columns before a column (letter or 1-based number) and save.
    InsertCol {
        /// Path to the spreadsheet.
        file: String,
        /// Column to insert before, e.g. `C` or `3`.
        col: String,
        /// How many columns to insert.
        #[arg(long, short = 'n', default_value_t = 1)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Delete columns starting at a column (letter or 1-based number) and save.
    DeleteCol {
        /// Path to the spreadsheet.
        file: String,
        /// Column to delete from, e.g. `C` or `3`.
        col: String,
        /// How many columns to delete.
        #[arg(long, short = 'n', default_value_t = 1)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Convert "numbers stored as text" in a range into real numbers and save.
    ///
    /// Useful when SUM/AVERAGE return 0 because a column holds text like
    /// `"6,000.00"` (common in bank/CSV exports).
    ToNumber {
        /// Path to the spreadsheet.
        file: String,
        /// Range to convert, e.g. `H1:H200`.
        range: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Convert "dates stored as text" in a range into real date values and save.
    ///
    /// The date twin of `to-number`: parses each text cell with the given
    /// Excel-style `--format`, stores the Excel serial, and applies that format
    /// so the cell displays (and reads via `get --raw --dates`) as a real date.
    /// Useful for bank/CSV exports that store dates as text like `04/04/2025`.
    ToDate {
        /// Path to the spreadsheet.
        file: String,
        /// Range to convert, e.g. `A2:A200`.
        range: String,
        /// Excel-style date format, e.g. `dd/mm/yyyy` or `dd/mm/yyyy hh:mm:ss`.
        #[arg(long, short = 'f')]
        format: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Create a new empty workbook file (format from the extension).
    New {
        /// Path to create, e.g. `book.xlsx`, `book.xls`, or `book.csv`.
        file: String,
        /// Name of the initial sheet.
        #[arg(long, short = 's', default_value = "Sheet1")]
        sheet: String,
    },

    /// Add a new empty sheet and save.
    AddSheet {
        /// Path to the spreadsheet.
        file: String,
        /// Name for the new sheet.
        name: String,
    },

    /// Delete a sheet by name and save.
    DeleteSheet {
        /// Path to the spreadsheet.
        file: String,
        /// Sheet to delete.
        name: String,
    },

    /// Rename a sheet and save.
    RenameSheet {
        /// Path to the spreadsheet.
        file: String,
        /// Current sheet name.
        old: String,
        /// New sheet name.
        new: String,
    },

    /// Append the rows of one workbook below another, aligning by header name.
    Append {
        /// Base file (rows are appended into this one and it is saved).
        base: String,
        /// File whose data rows are appended to `base`.
        add: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Group rows by a column and aggregate another (category totals).
    Pivot {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Column to group by (letter or header name).
        #[arg(long)]
        rows: String,
        /// Column to aggregate (letter or header name).
        #[arg(long)]
        values: String,
        /// Aggregation function.
        #[arg(long, default_value = "sum")]
        agg: Agg,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Sort rows by one or more columns and save (stable, header preserved).
    Sort {
        /// Path to the spreadsheet.
        file: String,
        /// Sort key column(s) (letter or header name); repeatable.
        #[arg(long, required = true)]
        by: Vec<String>,
        /// Sort descending.
        #[arg(long)]
        desc: bool,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Drop duplicate rows by key column(s) (keeps the first) and save.
    Dedup {
        /// Path to the spreadsheet.
        file: String,
        /// Key column(s) (letter or header name); repeatable. Default: whole row.
        #[arg(long = "on")]
        on: Vec<String>,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Print rows matching a predicate, e.g. `H>1000` or `B==ZANMAI`.
    ///
    /// Operators: `==` `!=` `>` `>=` `<` `<=` `~`(contains). The special
    /// predicates `<col>:number` / `<col>:text` keep rows whose cell is a
    /// number / text.
    Filter {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Predicate, e.g. `H>1000`.
        predicate: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Inner-join two sheets on a key column and print the combined rows.
    Join {
        /// Left file.
        file1: String,
        /// Right file.
        file2: String,
        /// Join key column (letter or header name), present in both.
        #[arg(long)]
        on: String,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Summarize a column: count, sum, mean, min/max, nulls, distinct, and a
    /// warning when numbers are stored as text.
    Profile {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Column (letter `B`, `B:B`, or header name).
        column: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Print rows containing a substring match, with their cell addresses.
    Grep {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// Substring to search for (case-insensitive).
        pattern: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Print the first N rows of a sheet.
    Head {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        #[arg(long, short = 'n', default_value_t = 10)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Print the last N rows of a sheet.
    Tail {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        #[arg(long, short = 'n', default_value_t = 10)]
        count: u32,
        #[arg(long, short = 's')]
        sheet: Option<String>,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Apply many `--set CELL=VALUE` edits in a single open/save (atomic).
    Batch {
        /// Path to the spreadsheet.
        file: String,
        /// An edit `CELL=VALUE`, e.g. `--set A1=1 --set B2=hi`; repeatable.
        #[arg(long = "set", required = true)]
        set: Vec<String>,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Auto-fit column widths to their content and save.
    Autofit {
        /// Path to the spreadsheet.
        file: String,
        /// Columns to fit (letter or `A:C` range form); default: all used columns.
        cols: Option<String>,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Run a read-only SQL SELECT over the workbook's sheets (sheets are tables,
    /// row 0 is the header). Supports WHERE / GROUP BY / aggregates / ORDER BY /
    /// LIMIT and an equi-JOIN across sheets.
    Query {
        /// Path to the spreadsheet (or `-` to read CSV from stdin).
        file: String,
        /// SQL, e.g. `SELECT category, SUM(amount) FROM Sheet1 GROUP BY category`.
        sql: String,
        #[command(flatten)]
        read: ReadArgs,
    },

    /// Apply basic styling (bold/italic/colors) to a range and save.
    Style {
        /// Path to the spreadsheet.
        file: String,
        /// Cell or range, e.g. `A1:C1`.
        range: String,
        /// Bold the text.
        #[arg(long)]
        bold: bool,
        /// Italicize the text.
        #[arg(long)]
        italic: bool,
        /// Font color as `RRGGBB` hex, e.g. `FF0000`.
        #[arg(long)]
        color: Option<String>,
        /// Solid fill (background) color as `RRGGBB` hex.
        #[arg(long)]
        bg: Option<String>,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },

    /// Manage defined names (named ranges): list / add / rm.
    Name {
        #[command(subcommand)]
        action: NameAction,
    },

    /// Manage Excel table objects: list / add / rm.
    Table {
        #[command(subcommand)]
        action: TableAction,
    },
}

/// Subcommands for `name` (defined names / named ranges).
#[derive(Subcommand)]
enum NameAction {
    /// List defined names.
    List {
        /// Path to the spreadsheet.
        file: String,
    },
    /// Add or replace a defined name (e.g. `Sales` → `Sheet1!$A$1:$B$9`).
    Add {
        /// Path to the spreadsheet.
        file: String,
        /// The name to define.
        name: String,
        /// What it refers to (e.g. `Sheet1!$A$1:$B$9`).
        refers_to: String,
        /// Scope the name to one sheet (defaults to workbook scope).
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },
    /// Remove a defined name.
    Rm {
        /// Path to the spreadsheet.
        file: String,
        /// The name to remove.
        name: String,
    },
}

/// Subcommands for `table` (Excel table objects).
#[derive(Subcommand)]
enum TableAction {
    /// List tables.
    List {
        /// Path to the spreadsheet.
        file: String,
    },
    /// Create a table over a range. The first row is used as headers unless
    /// `--no-header` is given (then headers are auto-named `Column1`, …).
    Add {
        /// Path to the spreadsheet.
        file: String,
        /// The range, e.g. `A1:C20`.
        range: String,
        /// Table name (defaults to `Table1`, `Table2`, …).
        #[arg(long)]
        name: Option<String>,
        /// Sheet the range is on (defaults to the active sheet).
        #[arg(long, short = 's')]
        sheet: Option<String>,
        /// Treat the range as data only (no header row).
        #[arg(long)]
        no_header: bool,
    },
    /// Remove a table by name.
    Rm {
        /// Path to the spreadsheet.
        file: String,
        /// The table name to remove.
        name: String,
    },
}

/// Value-shaping flags shared by all headless reads (no output format — that
/// is chosen per-command, e.g. `export -f`).
#[derive(clap::Args, Clone, Default)]
struct RawArgs {
    /// Emit stored (unformatted) values: no thousands separators, booleans as
    /// `true`/`false`, dates per `--dates`.
    #[arg(long)]
    raw: bool,
    /// How date-formatted cells are rendered.
    #[arg(long, value_enum)]
    dates: Option<render::DateMode>,
    /// Treat the first row as a header (keys JSON objects; labels md/table).
    #[arg(long)]
    header: bool,
}

impl RawArgs {
    fn to_opts(&self, format: render::OutFormat) -> render::ReadOpts {
        render::ReadOpts {
            format,
            raw: self.raw,
            dates: self.dates,
            header: self.header,
        }
    }
}

/// Read flags plus an explicit output `--format` (for get/eval/filter/…).
#[derive(clap::Args, Clone, Default)]
struct ReadArgs {
    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    format: render::OutFormat,
    #[command(flatten)]
    raw: RawArgs,
}

impl ReadArgs {
    fn to_opts(&self) -> render::ReadOpts {
        self.raw.to_opts(self.format)
    }
}

/// Aggregation function for `pivot`.
#[derive(Clone, Copy, ValueEnum)]
pub enum Agg {
    Sum,
    Count,
    Mean,
    Min,
    Max,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ExportFormat {
    Csv,
    Tsv,
    Json,
    Jsonl,
    Md,
    Xlsx,
    Xls,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Tsv => "tsv",
            ExportFormat::Json => "json",
            ExportFormat::Jsonl => "jsonl",
            ExportFormat::Md => "md",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Xls => "xls",
        }
    }

    /// The text formats render via [`render`]; xlsx/xls are binary.
    fn as_out_format(self) -> Option<render::OutFormat> {
        match self {
            ExportFormat::Csv => Some(render::OutFormat::Csv),
            ExportFormat::Tsv => Some(render::OutFormat::Tsv),
            ExportFormat::Json => Some(render::OutFormat::Json),
            ExportFormat::Jsonl => Some(render::OutFormat::Jsonl),
            ExportFormat::Md => Some(render::OutFormat::Md),
            ExportFormat::Xlsx | ExportFormat::Xls => None,
        }
    }
}

// ─── Binary entry point ─────────────────────────────────────────────────────

/// Execute the migrated `xls` command surface from an explicit argv vector.
///
/// Supplying argv explicitly lets the new product boundary remove its own
/// guardrail flags before entering the source-compatible command parser.
pub(super) fn main_from(arguments: Vec<std::ffi::OsString>) -> ExitCode {
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
            let _ = error.print();
            return code;
        }
    };
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let Cli {
        command,
        file: top_file,
        password,
        dry_run,
        backup,
        output,
    } = cli;
    let password = password.as_deref();
    let save = SaveOpts {
        dry_run,
        backup,
        output,
    };
    match command {
        // ── open ────────────────────────────────────────────────────────────
        Some(Command::Open { file }) => {
            let wb = open_file_or_stdin(&file, password)?;
            #[cfg(feature = "tui")]
            {
                crate::tui::run(wb)?;
                Ok(ExitCode::SUCCESS)
            }
            #[cfg(not(feature = "tui"))]
            {
                cmd_info(&wb, &file);
                return Ok(ExitCode::SUCCESS);
            }
        }

        // ── info ─────────────────────────────────────────────────────────────
        Some(Command::Info { file }) => {
            let wb = open_file_or_stdin(&file, password)?;
            cmd_info(&wb, &file);
            Ok(ExitCode::SUCCESS)
        }

        // ── get ──────────────────────────────────────────────────────────────
        Some(Command::Get { file, cell, read }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let result = cmd_read(&wb, &cell, &read.to_opts())?;
            println!("{result}");
            Ok(ExitCode::SUCCESS)
        }

        // ── set ──────────────────────────────────────────────────────────────
        Some(Command::Set { file, cell, value }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_set(&mut wb, &cell, &value)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }

        // ── eval ─────────────────────────────────────────────────────────────
        Some(Command::Eval {
            file,
            formula,
            at,
            read,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let result = cmd_eval(&mut wb, &formula, at.as_deref(), &read.to_opts())?;
            println!("{result}");
            Ok(ExitCode::SUCCESS)
        }

        // ── format (introspection) ────────────────────────────────────────────
        Some(Command::Format { file, cell }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let (idx, a1) = split_sheet_cell(&wb, &cell);
            let addr = core::CellAddress::parse_a1(a1)
                .ok_or_else(|| anyhow::anyhow!("invalid cell reference: {a1}"))?;
            println!(
                "{}",
                render::describe_number_format(&wb, idx, addr.row, addr.col)
            );
            Ok(ExitCode::SUCCESS)
        }

        // ── format-set ─────────────────────────────────────────────────────────
        Some(Command::FormatSet {
            file,
            range,
            code,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let n = cmd_format_set(&mut wb, &range, &code, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            eprintln!("set number format on {n} cell(s)");
            Ok(ExitCode::SUCCESS)
        }

        // ── export ───────────────────────────────────────────────────────────
        Some(Command::Export {
            file,
            format,
            sheet,
            out,
            stream,
            read,
        }) => {
            // Streaming path: only when requested and viable; otherwise it
            // returns false and we fall through to the in-memory exporter.
            if stream && cmd_export_stream(&file, format, sheet.as_deref(), out.as_deref(), &read)?
            {
                return Ok(ExitCode::SUCCESS);
            }
            let wb = open_file_or_stdin(&file, password)?;
            cmd_export(&wb, &file, format, sheet.as_deref(), out.as_deref(), &read)?;
            Ok(ExitCode::SUCCESS)
        }

        // ── import ───────────────────────────────────────────────────────────
        Some(Command::Import { csv, into, sheet }) => {
            cmd_import(&csv, &into, sheet.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }

        // ── diff ─────────────────────────────────────────────────────────────
        Some(Command::Diff {
            file1,
            file2,
            key,
            sheet,
        }) => {
            let wb1 = open_file_or_stdin(&file1, password)?;
            let wb2 = open_file_or_stdin(&file2, password)?;
            let has_diffs = match key {
                Some(k) => cmd_diff_keyed(&wb1, &wb2, &k, sheet.as_deref())?,
                None => cmd_diff(&wb1, &wb2),
            };
            if has_diffs {
                Ok(ExitCode::FAILURE)
            } else {
                Ok(ExitCode::SUCCESS)
            }
        }

        // ── mutating commands ────────────────────────────────────────────────
        Some(Command::Clear { file, range, sheet }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_clear(&mut wb, &range, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Fill {
            file,
            range,
            value,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_fill(&mut wb, &range, &value, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Copy {
            file,
            src,
            dest,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_copy_move(&mut wb, &src, &dest, sheet.as_deref(), false)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Move {
            file,
            src,
            dest,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_copy_move(&mut wb, &src, &dest, sheet.as_deref(), true)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::InsertRow {
            file,
            row,
            count,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let at = one_based_to_index(row)?;
            mutate_sheet(&mut wb, sheet.as_deref(), |s| s.insert_rows(at, count))?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::DeleteRow {
            file,
            row,
            count,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let at = one_based_to_index(row)?;
            mutate_sheet(&mut wb, sheet.as_deref(), |s| s.delete_rows(at, count))?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::InsertCol {
            file,
            col,
            count,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let at = parse_column(&col)?;
            mutate_sheet(&mut wb, sheet.as_deref(), |s| s.insert_cols(at, count))?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::DeleteCol {
            file,
            col,
            count,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let at = parse_column(&col)?;
            mutate_sheet(&mut wb, sheet.as_deref(), |s| s.delete_cols(at, count))?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::ToNumber { file, range, sheet }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let rng = parse_range(&wb, &range)?;
            let mut n = 0;
            mutate_sheet(&mut wb, sheet.as_deref(), |s| {
                n = s.coerce_text_to_numbers(rng)
            })?;
            save_with_opts(&wb, &file, password, &save)?;
            println!("Converted {n} text cell(s) to numbers");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::ToDate {
            file,
            range,
            format,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let n = cmd_to_date(&mut wb, &range, &format, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            println!("Converted {n} text cell(s) to dates");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::New { file, sheet }) => {
            let mut wb = Workbook::empty();
            wb.add_sheet(sheet);
            core::save_path(&wb, Path::new(&file))?;
            println!("Created {file}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::AddSheet { file, name }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_add_sheet(&mut wb, &name)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::DeleteSheet { file, name }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_delete_sheet(&mut wb, &name)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::RenameSheet { file, old, new }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_rename_sheet(&mut wb, &old, &new)?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }

        // ── data manipulation ──────────────────────────────────────────────────
        Some(Command::Append { base, add, sheet }) => {
            let mut base_wb = open_file_or_stdin(&base, password)?;
            let add_wb = open_file_or_stdin(&add, password)?;
            let n = cmd_append(&mut base_wb, &add_wb, sheet.as_deref())?;
            save_with_opts(&base_wb, &base, password, &save)?;
            eprintln!("appended {n} row(s)");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Pivot {
            file,
            rows,
            values,
            agg,
            sheet,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_pivot(&wb, &rows, &values, agg, sheet.as_deref())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Sort {
            file,
            by,
            desc,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            cmd_sort(&mut wb, &by, desc, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Dedup { file, on, sheet }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let removed = cmd_dedup(&mut wb, &on, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            eprintln!("removed {removed} duplicate row(s)");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Filter {
            file,
            predicate,
            sheet,
            read,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_filter(&wb, &predicate, sheet.as_deref(), &read.to_opts())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Join {
            file1,
            file2,
            on,
            read,
        }) => {
            let wb1 = open_file_or_stdin(&file1, password)?;
            let wb2 = open_file_or_stdin(&file2, password)?;
            let out = cmd_join(&wb1, &wb2, &on, &read.to_opts())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Profile {
            file,
            column,
            sheet,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_profile(&wb, &column, sheet.as_deref())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Grep {
            file,
            pattern,
            sheet,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let found = cmd_grep(&wb, &pattern, sheet.as_deref())?;
            if found {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::FAILURE)
            }
        }
        Some(Command::Head {
            file,
            count,
            sheet,
            read,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_head_tail(&wb, count, sheet.as_deref(), false, &read.to_opts())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Tail {
            file,
            count,
            sheet,
            read,
        }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_head_tail(&wb, count, sheet.as_deref(), true, &read.to_opts())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Batch { file, set, sheet }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let n = cmd_batch(&mut wb, &set, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            eprintln!("applied {n} edit(s)");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Query { file, sql, read }) => {
            let wb = open_file_or_stdin(&file, password)?;
            let out = cmd_query(&wb, &sql, &read.to_opts())?;
            println!("{out}");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Autofit { file, cols, sheet }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let n = cmd_autofit(&mut wb, cols.as_deref(), sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            eprintln!("fit {n} column(s)");
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Style {
            file,
            range,
            bold,
            italic,
            color,
            bg,
            sheet,
        }) => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let opts = StyleOpts {
                bold,
                italic,
                color: color.as_deref(),
                bg: bg.as_deref(),
            };
            let n = cmd_style(&mut wb, &range, &opts, sheet.as_deref())?;
            save_with_opts(&wb, &file, password, &save)?;
            eprintln!("styled {n} cell(s)");
            Ok(ExitCode::SUCCESS)
        }

        // ── name (defined names) ───────────────────────────────────────────────
        Some(Command::Name { action }) => cmd_name(action, password, &save),

        // ── table (table objects) ──────────────────────────────────────────────
        Some(Command::Table { action }) => cmd_table(action, password, &save),

        // ── no subcommand ────────────────────────────────────────────────────
        None => {
            #[cfg(feature = "tui")]
            {
                if let Some(file) = top_file {
                    let wb = open_file_or_stdin(&file, password)?;
                    crate::tui::run(wb)?;
                    return Ok(ExitCode::SUCCESS);
                }
            }
            #[cfg(not(feature = "tui"))]
            {
                if let Some(file) = &top_file {
                    let wb = open_file_or_stdin(file, password)?;
                    cmd_info(&wb, file);
                    return Ok(ExitCode::SUCCESS);
                }
            }
            eprintln!("no command given; try `xls --help`");
            Ok(ExitCode::FAILURE)
        }
    }
}

// ─── Command implementations ─────────────────────────────────────────────────

/// Open a workbook from a file path or from stdin (when `path == "-"`),
/// decrypting with `password` if the file is password-protected.
///
/// Recalculates on open when the workbook contains formulas, so cached values
/// are fresh and **dynamic-array results spill** (spill regions are derived
/// state that isn't persisted — they're rebuilt by recalc, like Excel's
/// calculate-on-load).
fn open_file_or_stdin(path: &str, password: Option<&str>) -> anyhow::Result<Workbook> {
    let mut wb = if path == "-" {
        // Must be piped — bail out with a clear error if it's a terminal.
        if std::io::stdin().is_terminal() {
            anyhow::bail!("stdin is a terminal; pipe CSV data or provide a file path");
        }
        core::csv::read_csv(
            std::io::stdin().lock(),
            &core::csv::CsvReadOptions {
                sheet_name: "Sheet1".to_string(),
                ..Default::default()
            },
        )?
    } else {
        core::open_path_with_password(Path::new(path), password)?
    };
    recalc_if_formulas(&mut wb);
    Ok(wb)
}

/// Save a workbook back to `file` after a mutating command, honoring the
/// global `--dry-run` / `--backup` / `--output` flags.
///
/// * `--dry-run` prints the cell-level diff vs. the on-disk original and writes
///   nothing.
/// * `--backup` copies the original to `<file>.bak` before overwriting.
/// * `--output PATH` writes to a copy instead of overwriting the input.
///
/// Warns when the source was decrypted with a password (we cannot re-encrypt).
fn save_with_opts(
    wb: &Workbook,
    file: &str,
    password: Option<&str>,
    opts: &SaveOpts,
) -> anyhow::Result<()> {
    // Reading from stdin has no path to write back to — require `--output`.
    if file == "-" && opts.output.is_none() && !opts.dry_run {
        anyhow::bail!("reading from stdin (`-`) requires `--output <PATH>` to save");
    }
    let target: PathBuf = opts.output.clone().unwrap_or_else(|| PathBuf::from(file));

    if opts.dry_run {
        // Show what would change vs. the on-disk original, when readable.
        if file != "-"
            && Path::new(file).exists()
            && let Ok(orig) = core::open_path_with_password(Path::new(file), password)
            && !cmd_diff(&orig, wb)
        {
            println!("(dry run) no changes");
        }
        eprintln!("(dry run) not writing to {}", target.display());
        return Ok(());
    }

    if opts.backup && opts.output.is_none() && file != "-" && Path::new(file).exists() {
        let bak = format!("{file}.bak");
        std::fs::copy(file, &bak)?;
        eprintln!("backup written to {bak}");
    }

    if password.is_some() {
        eprintln!(
            "warning: '{file}' was opened with a password; saving writes an \
             UNENCRYPTED file (re-encryption is not supported)"
        );
    }
    core::save_path(wb, &target)?;
    Ok(())
}

/// Print workbook metadata to stdout.
pub fn cmd_info(wb: &Workbook, file: &str) {
    println!("File:        {file}");
    println!("Sheets:      {}", wb.sheets.len());
    for s in &wb.sheets {
        let (r, c) = s.dimensions();
        println!("  - {} ({}r x {}c)", s.name, r, c);
    }
    println!(
        "Date system: {}",
        match wb.date_system {
            core::DateSystem::Date1900 => "1900",
            core::DateSystem::Date1904 => "1904",
        }
    );
    if let Some(title) = &wb.metadata.title {
        println!("Title:       {title}");
    }
    if let Some(author) = &wb.metadata.author {
        println!("Author:      {author}");
    }
    if !wb.defined_names.is_empty() {
        println!("Named ranges:");
        for dn in &wb.defined_names {
            let scope = match dn.scope {
                Some(i) => wb
                    .sheets
                    .get(i)
                    .map(|s| s.name.as_str())
                    .unwrap_or("?")
                    .to_string(),
                None => "workbook".to_string(),
            };
            println!("  - {} = {} [{}]", dn.name, dn.refers_to, scope);
        }
    }
    let total_tables: usize = wb.sheets.iter().map(|s| s.tables.len()).sum();
    if total_tables > 0 {
        println!("Tables:");
        for sheet in &wb.sheets {
            for t in &sheet.tables {
                println!(
                    "  - {} ({}!{}) cols: {}",
                    t.name,
                    sheet.name,
                    t.range.to_a1(),
                    t.columns.join(", ")
                );
            }
        }
    }
}

/// Handle the `name` subcommand (defined names / named ranges).
fn cmd_name(
    action: NameAction,
    password: Option<&str>,
    save: &SaveOpts,
) -> anyhow::Result<ExitCode> {
    match action {
        NameAction::List { file } => {
            let wb = open_file_or_stdin(&file, password)?;
            if wb.defined_names.is_empty() {
                eprintln!("(no defined names)");
            }
            for dn in &wb.defined_names {
                let scope = match dn.scope {
                    Some(i) => wb.sheets.get(i).map(|s| s.name.as_str()).unwrap_or("?"),
                    None => "workbook",
                };
                println!("{}\t{}\t{}", dn.name, dn.refers_to, scope);
            }
            Ok(ExitCode::SUCCESS)
        }
        NameAction::Add {
            file,
            name,
            refers_to,
            sheet,
        } => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let scope = match sheet.as_deref() {
                Some(s) => Some(resolve_sheet_idx(&wb, Some(s))?),
                None => None,
            };
            // Replace any existing name with the same identifier + scope.
            wb.defined_names
                .retain(|d| !(d.name.eq_ignore_ascii_case(&name) && d.scope == scope));
            wb.defined_names.push(core::model::DefinedName {
                name: name.clone(),
                refers_to,
                scope,
                hidden: false,
            });
            save_with_opts(&wb, &file, password, save)?;
            eprintln!("defined name '{name}' set");
            Ok(ExitCode::SUCCESS)
        }
        NameAction::Rm { file, name } => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let before = wb.defined_names.len();
            wb.defined_names
                .retain(|d| !d.name.eq_ignore_ascii_case(&name));
            if wb.defined_names.len() == before {
                anyhow::bail!("no defined name '{name}'");
            }
            save_with_opts(&wb, &file, password, save)?;
            eprintln!("defined name '{name}' removed");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Handle the `table` subcommand (Excel table objects).
fn cmd_table(
    action: TableAction,
    password: Option<&str>,
    save: &SaveOpts,
) -> anyhow::Result<ExitCode> {
    match action {
        TableAction::List { file } => {
            let wb = open_file_or_stdin(&file, password)?;
            let mut any = false;
            for sheet in &wb.sheets {
                for t in &sheet.tables {
                    any = true;
                    println!(
                        "{}\t{}!{}\t{}",
                        t.name,
                        sheet.name,
                        t.range.to_a1(),
                        t.columns.join(", ")
                    );
                }
            }
            if !any {
                eprintln!("(no tables)");
            }
            Ok(ExitCode::SUCCESS)
        }
        TableAction::Add {
            file,
            range,
            name,
            sheet,
            no_header,
        } => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let sheet_idx = resolve_sheet_idx(&wb, sheet.as_deref())?;
            let cr = parse_range(&wb, &range)?;
            let table_name = match name {
                Some(n) => n,
                None => {
                    let n: usize = wb.sheets.iter().map(|s| s.tables.len()).sum::<usize>() + 1;
                    format!("Table{n}")
                }
            };
            if wb.table_by_name(&table_name).is_some() {
                anyhow::bail!("a table named '{table_name}' already exists");
            }
            // Column names: from the header row, else auto-named.
            let header_rows = if no_header { 0 } else { 1 };
            let ncols = cr.cols();
            let columns: Vec<String> = (0..ncols)
                .map(|i| {
                    let col = cr.start.col + i;
                    if no_header {
                        format!("Column{}", i + 1)
                    } else {
                        let s = wb.display_cell(sheet_idx, cr.start.row, col);
                        if s.is_empty() {
                            format!("Column{}", i + 1)
                        } else {
                            s
                        }
                    }
                })
                .collect();
            wb.sheets[sheet_idx].tables.push(core::model::Table {
                name: table_name.clone(),
                display_name: table_name.clone(),
                range: cr,
                columns,
                header_rows,
                totals_rows: 0,
                id: 0,
                raw_xml: Vec::new(),
            });
            save_with_opts(&wb, &file, password, save)?;
            eprintln!("table '{table_name}' created over {}", cr.to_a1());
            Ok(ExitCode::SUCCESS)
        }
        TableAction::Rm { file, name } => {
            let mut wb = open_file_or_stdin(&file, password)?;
            let mut removed = false;
            for sheet in &mut wb.sheets {
                let before = sheet.tables.len();
                sheet.tables.retain(|t| !t.name.eq_ignore_ascii_case(&name));
                removed |= sheet.tables.len() != before;
            }
            if !removed {
                anyhow::bail!("no table named '{name}'");
            }
            save_with_opts(&wb, &file, password, save)?;
            eprintln!("table '{name}' removed");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Return the displayed value of a cell.  Testable without I/O.
#[allow(
    dead_code,
    reason = "保留旧 xls 可独立测试的单单元格 helper，供结构化命令继续复用"
)]
pub fn cmd_get(wb: &Workbook, cell: &str) -> anyhow::Result<String> {
    let (sheet_idx, a1) = split_sheet_cell(wb, cell);
    let addr = core::CellAddress::parse_a1(a1)
        .ok_or_else(|| anyhow::anyhow!("invalid cell reference: {a1}"))?;
    Ok(wb.display_cell(sheet_idx, addr.row, addr.col))
}

/// Parse a value string and write it into the workbook at `cell`, recalculating
/// if needed.  Testable without I/O.
pub fn cmd_set(wb: &mut Workbook, cell: &str, value: &str) -> anyhow::Result<()> {
    let (sheet_idx, a1) = split_sheet_cell(wb, cell);
    let addr = core::CellAddress::parse_a1(a1)
        .ok_or_else(|| anyhow::anyhow!("invalid cell reference: {a1}"))?;

    let new_cell = parse_value_string(value);
    let is_formula = matches!(new_cell, Cell::Formula { .. });

    {
        let sheet = wb
            .sheet_mut(sheet_idx)
            .ok_or_else(|| anyhow::anyhow!("sheet index {sheet_idx} out of range"))?;
        sheet.set(addr.row, addr.col, new_cell);
    }

    // Recalc if we just set a formula, or if the sheet already has formulas.
    let sheet_has_formulas = wb
        .sheets
        .get(sheet_idx)
        .map(|s| s.cells.values().any(|c| c.is_formula()))
        .unwrap_or(false);

    if is_formula || sheet_has_formulas {
        Engine::new().recalc(wb);
    }

    Ok(())
}

/// Evaluate a formula against the workbook data.  Recalcs first so cell refs
/// are fresh.  Array/range results are rendered as a grid in `opts.format`;
/// scalars print as a single value.  Testable without I/O.
pub fn cmd_eval(
    wb: &mut Workbook,
    formula: &str,
    at: Option<&str>,
    opts: &render::ReadOpts,
) -> anyhow::Result<String> {
    use super::easyexcel_components::formula::value::Value;

    // Recalc so any formula cells have up-to-date cached values.
    Engine::new().recalc(wb);

    let at_ref = if let Some(spec) = at {
        parse_cell_ref(wb, spec)?
    } else {
        CellRef {
            sheet: 0,
            row: 0,
            col: 0,
        }
    };

    let value = Engine::new().eval_formula(wb, at_ref, formula);
    match &value {
        Value::Array(a) => Ok(render::render_value_grid(&a.data, a.rows, a.cols, opts)),
        Value::Ref(r) => {
            // Materialize the referenced range into scalars and render it.
            let (rows, cols) = (r.rows() as usize, r.cols() as usize);
            let mut data = Vec::with_capacity(rows * cols);
            for (rr, cc) in r.iter() {
                let cv = wb
                    .sheets
                    .get(r.sheet)
                    .map(|s| s.value(rr, cc))
                    .unwrap_or(super::easyexcel_components::value::CellValue::Empty);
                data.push(Value::from_cell_value(cv));
            }
            Ok(render::render_value_grid(&data, rows, cols, opts))
        }
        _ => Ok(value.to_cell_value().to_display_string()),
    }
}

/// Read a cell or range to a string in the requested format.  A bare single
/// cell with default options prints just its value (back-compatible).
pub fn cmd_read(wb: &Workbook, reference: &str, opts: &render::ReadOpts) -> anyhow::Result<String> {
    let (sheet_idx, range) = resolve_reference(wb, reference)?;
    Ok(render::render_range(wb, sheet_idx, range, opts))
}

/// Resolve a read reference that may be a plain `[Sheet!]A1[:B2]`, a defined
/// name, a table name, or a structured table reference (`Sales[Amount]`).
fn resolve_reference(wb: &Workbook, reference: &str) -> anyhow::Result<(usize, core::CellRange)> {
    let trimmed = reference.trim();
    // Table name or structured reference (`Sales`, `Sales[Amount]`, `Sales[#All]`).
    if let Some((idx, range)) = wb.resolve_structured(trimmed) {
        return Ok((idx, range));
    }
    // Defined name (workbook scope, case-insensitive).
    if let Some(def) = wb
        .defined_names
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(trimmed))
    {
        let refers = def.refers_to.trim();
        let refers = refers.strip_prefix('=').unwrap_or(refers);
        let (idx, a1) = split_sheet_cell(wb, refers);
        if let Some(range) = core::CellRange::parse_a1(a1) {
            return Ok((idx, range));
        }
    }
    // Plain [Sheet!]A1[:B2].
    let (idx, a1) = split_sheet_cell(wb, trimmed);
    let range = core::CellRange::parse_a1(a1)
        .or_else(|| core::CellAddress::parse_a1(a1).map(core::CellRange::single))
        .ok_or_else(|| anyhow::anyhow!("invalid cell, range, name, or table: {reference}"))?;
    Ok((idx, range))
}

/// Export a workbook (or one sheet) to a new format.
fn cmd_export(
    wb: &Workbook,
    src_path: &str,
    format: ExportFormat,
    sheet: Option<&str>,
    out: Option<&Path>,
    read: &RawArgs,
) -> anyhow::Result<()> {
    // Text formats render through `render`; they may also stream to stdout.
    if let Some(out_format) = format.as_out_format() {
        let sheet_idx = resolve_sheet_idx(wb, sheet)?;
        let (rows, cols) = wb.sheets[sheet_idx].dimensions();
        let opts = read.to_opts(out_format);
        let text = if rows == 0 || cols == 0 {
            String::new()
        } else {
            let range = core::CellRange::new(
                core::CellAddress::new(0, 0),
                core::CellAddress::new(rows - 1, cols - 1),
            );
            render::render_range(wb, sheet_idx, range, &opts)
        };
        let to_stdout = matches!(out, Some(p) if p.as_os_str() == "-");
        if to_stdout {
            println!("{text}");
        } else {
            let out_path = derive_out_path(out, src_path, format);
            std::fs::write(&out_path, format!("{text}\n"))?;
            eprintln!("Exported to {}", out_path.display());
        }
        return Ok(());
    }

    // Binary formats (xlsx/xls) — written to a real file.
    if matches!(out, Some(p) if p.as_os_str() == "-") {
        anyhow::bail!(
            "cannot write a binary format ({}) to stdout",
            format.extension()
        );
    }
    let out_path = derive_out_path(out, src_path, format);
    // For multi-sheet formats we may be writing all sheets or just one.
    let wb_to_write: std::borrow::Cow<Workbook> = if let Some(name) = sheet {
        let idx = wb
            .sheet_index(name)
            .ok_or_else(|| anyhow::anyhow!("sheet not found: {name}"))?;
        let mut single = Workbook::empty();
        single.sheets.push(wb.sheets[idx].clone());
        single.styles = wb.styles.clone();
        single.date_system = wb.date_system;
        std::borrow::Cow::Owned(single)
    } else {
        std::borrow::Cow::Borrowed(wb)
    };
    core::save_path(&wb_to_write, &out_path)?;
    eprintln!("Exported to {}", out_path.display());
    Ok(())
}

/// Streaming export for huge files. Returns `Ok(true)` when it handled the
/// export, `Ok(false)` (after a stderr note) when the format/input is not
/// streamable and the caller should fall back to the in-memory path.
fn cmd_export_stream(
    file: &str,
    format: ExportFormat,
    sheet: Option<&str>,
    out: Option<&Path>,
    read: &RawArgs,
) -> anyhow::Result<bool> {
    use std::io::{BufWriter, Write};

    // Only text row-formats stream; xlsx/xls and table/json/md do not.
    let Some(out_format) = format.as_out_format() else {
        eprintln!(
            "note: --stream does not apply to {} output; reading normally",
            format.extension()
        );
        return Ok(false);
    };
    if !stream::format_is_streamable(out_format) {
        eprintln!(
            "note: --stream supports csv/tsv/jsonl only; {} reads normally",
            format.extension()
        );
        return Ok(false);
    }
    // Only on-disk xlsx/csv can be streamed.
    if file == "-" || !core::stream::is_streamable(Path::new(file)) {
        eprintln!("note: input is not streamable; reading normally");
        return Ok(false);
    }

    let opts = read.to_opts(out_format);
    let to_stdout = matches!(out, Some(p) if p.as_os_str() == "-");
    let out_path = if to_stdout {
        None
    } else {
        Some(derive_out_path(out, file, format))
    };

    let writer: Box<dyn Write> = match &out_path {
        Some(p) => Box::new(BufWriter::new(std::fs::File::create(p)?)),
        None => Box::new(BufWriter::new(std::io::stdout().lock())),
    };

    let src = Path::new(file);
    match out_format {
        render::OutFormat::Csv | render::OutFormat::Tsv => {
            let mut sink = stream::CsvSink::new(writer, opts);
            core::stream::stream_path(src, sheet, &mut sink)?;
        }
        render::OutFormat::Jsonl => {
            let mut sink = stream::JsonlSink::new(writer, opts);
            core::stream::stream_path(src, sheet, &mut sink)?;
        }
        _ => unreachable!("format_is_streamable gates the formats above"),
    }

    if let Some(p) = &out_path {
        eprintln!("Exported to {} (streamed)", p.display());
    }
    Ok(true)
}

/// Resolve the output path: explicit `out`, else `<input-stem>.<ext>`.
fn derive_out_path(out: Option<&Path>, src_path: &str, format: ExportFormat) -> PathBuf {
    match out {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = if src_path == "-" {
                "output".to_string()
            } else {
                Path::new(src_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output")
                    .to_string()
            };
            PathBuf::from(format!("{}.{}", stem, format.extension()))
        }
    }
}

/// Import a CSV and add/replace it as a sheet inside `target`.
pub fn cmd_import(csv_path: &str, target: &Path, sheet_name: Option<&str>) -> anyhow::Result<()> {
    // Read the CSV.
    let default_name = if csv_path == "-" {
        "Sheet".to_string()
    } else {
        Path::new(csv_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Sheet")
            .to_string()
    };
    let name = sheet_name.unwrap_or(&default_name).to_string();

    let csv_wb = if csv_path == "-" {
        if std::io::stdin().is_terminal() {
            anyhow::bail!("stdin is a terminal; pipe CSV data or provide a file path");
        }
        core::csv::read_csv(
            std::io::stdin().lock(),
            &core::csv::CsvReadOptions {
                sheet_name: name.clone(),
                ..Default::default()
            },
        )?
    } else {
        let f = std::fs::File::open(csv_path)?;
        core::csv::read_csv(
            f,
            &core::csv::CsvReadOptions {
                sheet_name: name.clone(),
                ..Default::default()
            },
        )?
    };

    let csv_sheet = csv_wb
        .sheets
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("CSV produced no sheet"))?;

    // Open or create the target workbook.
    let mut target_wb = if target.exists() {
        core::open_path(target)?
    } else {
        let mut wb = Workbook::new();
        // Remove the default placeholder sheet so we start clean.
        wb.sheets.clear();
        wb
    };

    // Replace if a sheet with the same name exists, otherwise append.
    if let Some(idx) = target_wb.sheet_index(&name) {
        target_wb.sheets[idx] = csv_sheet;
    } else {
        target_wb.sheets.push(csv_sheet);
    }

    core::save_path(&target_wb, target)?;
    println!("Imported sheet '{}' into {}", name, target.display());
    Ok(())
}

/// Print cell-by-cell differences; returns `true` if any were found.
pub fn cmd_diff(wb1: &Workbook, wb2: &Workbook) -> bool {
    // Collect sheet names from both.
    let names: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut v = Vec::new();
        for s in wb1.sheets.iter().chain(wb2.sheets.iter()) {
            if seen.insert(s.name.to_ascii_lowercase()) {
                v.push(s.name.clone());
            }
        }
        v
    };

    let mut found = false;

    for sheet_name in &names {
        let idx1 = wb1.sheet_index(sheet_name);
        let idx2 = wb2.sheet_index(sheet_name);

        match (idx1, idx2) {
            (None, Some(i2)) => {
                let (rows, cols) = wb2.sheets[i2].dimensions();
                for r in 0..rows {
                    for c in 0..cols {
                        let v2 = wb2.display_cell(i2, r, c);
                        if !v2.is_empty() {
                            let addr = cell_addr_str(r, c);
                            println!("{sheet_name}!{addr}: <missing> | {v2}");
                            found = true;
                        }
                    }
                }
            }
            (Some(i1), None) => {
                let (rows, cols) = wb1.sheets[i1].dimensions();
                for r in 0..rows {
                    for c in 0..cols {
                        let v1 = wb1.display_cell(i1, r, c);
                        if !v1.is_empty() {
                            let addr = cell_addr_str(r, c);
                            println!("{sheet_name}!{addr}: {v1} | <missing>");
                            found = true;
                        }
                    }
                }
            }
            (Some(i1), Some(i2)) => {
                let (r1, c1) = wb1.sheets[i1].dimensions();
                let (r2, c2) = wb2.sheets[i2].dimensions();
                let max_r = r1.max(r2);
                let max_c = c1.max(c2);
                for r in 0..max_r {
                    for c in 0..max_c {
                        let v1 = wb1.display_cell(i1, r, c);
                        let v2 = wb2.display_cell(i2, r, c);
                        if v1 != v2 {
                            let addr = cell_addr_str(r, c);
                            println!("{sheet_name}!{addr}: {v1} | {v2}");
                            found = true;
                        }
                    }
                }
            }
            (None, None) => unreachable!(),
        }
    }

    found
}

/// Clear all cells in a range on the chosen sheet.
pub fn cmd_clear(wb: &mut Workbook, range: &str, sheet: Option<&str>) -> anyhow::Result<()> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, range)?;
    wb.sheets[idx].clear_range(rng);
    recalc_if_formulas(wb);
    Ok(())
}

/// Fill every cell in a range with the same parsed value.
pub fn cmd_fill(
    wb: &mut Workbook,
    range: &str,
    value: &str,
    sheet: Option<&str>,
) -> anyhow::Result<()> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, range)?;
    let cell = parse_value_string(value);
    for (r, c) in rng.iter_cells() {
        wb.sheets[idx].set(r, c, cell.clone());
    }
    recalc_if_formulas(wb);
    Ok(())
}

/// Copy a source range to a destination anchor; if `cut`, clear the source after.
pub fn cmd_copy_move(
    wb: &mut Workbook,
    src: &str,
    dest: &str,
    sheet: Option<&str>,
    cut: bool,
) -> anyhow::Result<()> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, src)?;
    let anchor = core::CellAddress::parse_a1(dest)
        .ok_or_else(|| anyhow::anyhow!("invalid destination cell: {dest}"))?;

    // Snapshot the source cells (verbatim) relative to the range's top-left.
    let mut payload = Vec::new();
    for (r, c) in rng.iter_cells() {
        if let Some(cell) = wb.sheets[idx].get(r, c) {
            payload.push((r - rng.start.row, c - rng.start.col, cell.clone()));
        }
    }
    if cut {
        wb.sheets[idx].clear_range(rng);
    }
    for (dr, dc, cell) in payload {
        wb.sheets[idx].set(anchor.row + dr, anchor.col + dc, cell);
    }
    recalc_if_formulas(wb);
    Ok(())
}

/// Add a new empty sheet (errors if the name is already taken).
pub fn cmd_add_sheet(wb: &mut Workbook, name: &str) -> anyhow::Result<()> {
    if wb.sheet_index(name).is_some() {
        anyhow::bail!("a sheet named '{name}' already exists");
    }
    wb.add_sheet(name);
    Ok(())
}

/// Delete a sheet by name (errors if missing or if it's the only sheet).
pub fn cmd_delete_sheet(wb: &mut Workbook, name: &str) -> anyhow::Result<()> {
    let idx = wb
        .sheet_index(name)
        .ok_or_else(|| anyhow::anyhow!("sheet not found: {name}"))?;
    if wb.sheets.len() == 1 {
        anyhow::bail!("cannot delete the only sheet");
    }
    wb.sheets.remove(idx);
    if wb.active_sheet >= wb.sheets.len() {
        wb.active_sheet = wb.sheets.len() - 1;
    }
    Ok(())
}

/// Rename a sheet (errors if missing or the new name is taken by another sheet).
pub fn cmd_rename_sheet(wb: &mut Workbook, old: &str, new: &str) -> anyhow::Result<()> {
    let idx = wb
        .sheet_index(old)
        .ok_or_else(|| anyhow::anyhow!("sheet not found: {old}"))?;
    if let Some(other) = wb.sheet_index(new)
        && other != idx
    {
        anyhow::bail!("a sheet named '{new}' already exists");
    }
    wb.sheets[idx].name = new.to_string();
    Ok(())
}

// ─── Data-manipulation & discovery verbs ─────────────────────────────────────

/// Set the number-format code on every cell in `range`. Returns the count.
pub fn cmd_format_set(
    wb: &mut Workbook,
    range: &str,
    code: &str,
    sheet: Option<&str>,
) -> anyhow::Result<usize> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, range)?;
    let mut n = 0;
    for (r, c) in rng.iter_cells() {
        let mut style = wb.sheets[idx]
            .style_at(r, c)
            .and_then(|si| wb.styles.get(si).cloned())
            .unwrap_or_default();
        style.number_format = code.to_string();
        style.number_format_id = None; // custom code; drop any built-in id
        let new_idx = wb.styles.intern(style);
        wb.sheets[idx].set_style(r, c, new_idx);
        n += 1;
    }
    Ok(n)
}

/// Convert text-stored dates in `range` into real date values using the
/// Excel-style `fmt`, applying `fmt` as the cell's number format so they
/// display (and read via `--raw --dates`) as dates. Returns the count.
/// Non-matching / non-text cells are left untouched (mirrors `to-number`).
pub fn cmd_to_date(
    wb: &mut Workbook,
    range: &str,
    fmt: &str,
    sheet: Option<&str>,
) -> anyhow::Result<usize> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, range)?;
    let system = wb.date_system;
    let mut converted = 0;
    for (r, c) in rng.iter_cells() {
        let serial = match wb.sheets[idx].get(r, c) {
            Some(Cell::Text(s)) => {
                super::easyexcel_components::dates::parse_text_date(s, fmt, system)
            }
            _ => None,
        };
        if let Some(serial) = serial {
            wb.sheets[idx].set(r, c, Cell::Number(serial));
            // Apply the date format (preserving any other style attributes).
            let mut style = wb.sheets[idx]
                .style_at(r, c)
                .and_then(|si| wb.styles.get(si).cloned())
                .unwrap_or_default();
            style.number_format = fmt.to_string();
            style.number_format_id = None;
            let new_idx = wb.styles.intern(style);
            wb.sheets[idx].set_style(r, c, new_idx);
            converted += 1;
        }
    }
    recalc_if_formulas(wb);
    Ok(converted)
}

/// Keyed, row-wise diff: match rows by the value in `key_col` and report rows
/// added / removed / changed by key. Row 0 is treated as a header.
pub fn cmd_diff_keyed(
    wb1: &Workbook,
    wb2: &Workbook,
    key: &str,
    sheet: Option<&str>,
) -> anyhow::Result<bool> {
    let i1 = resolve_sheet_idx(wb1, sheet)?;
    let i2 = resolve_sheet_idx(wb2, sheet)?;
    let k1 = resolve_col(wb1, i1, key)?;
    let k2 = resolve_col(wb2, i2, key)?;
    let (r1, c1) = wb1.sheets[i1].dimensions();
    let (r2, c2) = wb2.sheets[i2].dimensions();
    let ncols = c1.max(c2);

    // Map key → row values (data rows only; row 0 is the header).
    let collect = |wb: &Workbook, idx: usize, rows: u32, kcol: u32| {
        let mut map: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for r in 1..rows {
            let k = wb.display_cell(idx, r, kcol);
            if k.is_empty() {
                continue;
            }
            map.entry(k)
                .or_insert_with(|| row_values(wb, idx, r, ncols));
        }
        map
    };
    let m1 = collect(wb1, i1, r1, k1);
    let m2 = collect(wb2, i2, r2, k2);
    let headers = row_values(wb1, i1, 0, ncols);

    let mut found = false;
    for (k, v2) in &m2 {
        if !m1.contains_key(k) {
            println!("+ {k}");
            found = true;
        } else {
            let v1 = &m1[k];
            let mut diffs = Vec::new();
            for c in 0..ncols as usize {
                let a = v1.get(c).map(String::as_str).unwrap_or("");
                let b = v2.get(c).map(String::as_str).unwrap_or("");
                if a != b {
                    let label = headers
                        .get(c)
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .unwrap_or_else(|| core::addr::col_index_to_letters(c as u32));
                    diffs.push(format!("{label}: {a} | {b}"));
                }
            }
            if !diffs.is_empty() {
                println!("~ {k}: {}", diffs.join(", "));
                found = true;
            }
        }
    }
    for k in m1.keys() {
        if !m2.contains_key(k) {
            println!("- {k}");
            found = true;
        }
    }
    Ok(found)
}

/// Append the data rows of `add` below `base`, aligning columns by header name.
/// Returns the number of rows appended.
pub fn cmd_append(
    base: &mut Workbook,
    add: &Workbook,
    sheet: Option<&str>,
) -> anyhow::Result<usize> {
    let bi = resolve_sheet_idx(base, sheet)?;
    let ai = resolve_sheet_idx(add, sheet)?;
    let (b_rows, b_cols) = base.sheets[bi].dimensions();
    let (a_rows, _a_cols) = add.sheets[ai].dimensions();

    // Map each base column to the matching column in `add` (by header name).
    let mut col_map: Vec<Option<u32>> = Vec::with_capacity(b_cols as usize);
    for bc in 0..b_cols {
        let bh = base.display_cell(bi, 0, bc);
        col_map.push(if bh.is_empty() {
            None
        } else {
            find_header(add, ai, &bh)
        });
    }

    let mut appended = 0;
    for ar in 1..a_rows {
        let dest_row = b_rows + appended;
        for (bc, mapped) in col_map.iter().enumerate() {
            if let Some(ac) = mapped
                && let Some(cell) = add.sheets[ai].get(ar, *ac)
            {
                let cell = cell.clone();
                base.sheets[bi].set(dest_row, bc as u32, cell);
            }
        }
        appended += 1;
    }
    Ok(appended as usize)
}

/// Group rows by `rows_col`, aggregate `values_col`, return a printable table.
pub fn cmd_pivot(
    wb: &Workbook,
    rows_col: &str,
    values_col: &str,
    agg: Agg,
    sheet: Option<&str>,
) -> anyhow::Result<String> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rcol = resolve_col(wb, idx, rows_col)?;
    let vcol = resolve_col(wb, idx, values_col)?;
    let (rows, _cols) = wb.sheets[idx].dimensions();

    // key → (count, sum, min, max)
    let mut groups: std::collections::BTreeMap<String, (u64, f64, f64, f64)> =
        std::collections::BTreeMap::new();
    for r in 1..rows {
        let key = wb.display_cell(idx, r, rcol);
        if key.is_empty() {
            continue;
        }
        let entry = groups
            .entry(key)
            .or_insert((0, 0.0, f64::INFINITY, f64::NEG_INFINITY));
        entry.0 += 1;
        if let Some(n) = wb.sheets[idx].value(r, vcol).as_number() {
            entry.1 += n;
            entry.2 = entry.2.min(n);
            entry.3 = entry.3.max(n);
        }
    }

    let agg_name = match agg {
        Agg::Sum => "sum",
        Agg::Count => "count",
        Agg::Mean => "mean",
        Agg::Min => "min",
        Agg::Max => "max",
    };
    let key_label = {
        let h = wb.display_cell(idx, 0, rcol);
        if h.is_empty() {
            core::addr::col_index_to_letters(rcol)
        } else {
            h
        }
    };

    let mut out = String::new();
    out.push_str(&format!("{key_label}\t{agg_name}\n"));
    for (k, (count, sum, min, max)) in &groups {
        let v = match agg {
            Agg::Sum => *sum,
            Agg::Count => *count as f64,
            Agg::Mean => {
                if *count > 0 {
                    *sum / *count as f64
                } else {
                    0.0
                }
            }
            Agg::Min => *min,
            Agg::Max => *max,
        };
        out.push_str(&format!(
            "{k}\t{}\n",
            super::easyexcel_components::value::format_number_general(v)
        ));
    }
    Ok(out.trim_end().to_string())
}

/// Stable multi-key sort of data rows (row 0 kept as a header).
pub fn cmd_sort(
    wb: &mut Workbook,
    by: &[String],
    desc: bool,
    sheet: Option<&str>,
) -> anyhow::Result<()> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let keys: Vec<u32> = by
        .iter()
        .map(|b| resolve_col(wb, idx, b))
        .collect::<anyhow::Result<_>>()?;
    let (rows, cols) = wb.sheets[idx].dimensions();
    if rows <= 2 {
        return Ok(()); // header + ≤1 data row: nothing to reorder
    }

    let mut snap = snapshot_rows(&wb.sheets[idx], 1, rows, cols);
    // Stable sort by the key columns, comparing numerically when possible.
    snap.sort_by(|a, b| {
        for &k in &keys {
            let av = display_of(&a.0, k);
            let bv = display_of(&b.0, k);
            let ord = cmp_values(&av, &bv);
            if ord != std::cmp::Ordering::Equal {
                return if desc { ord.reverse() } else { ord };
            }
        }
        std::cmp::Ordering::Equal
    });
    rewrite_rows(&mut wb.sheets[idx], 1, rows, cols, snap);
    recalc_if_formulas(wb);
    Ok(())
}

/// Drop duplicate data rows by key column(s) (keeps first). Row 0 is a header.
pub fn cmd_dedup(wb: &mut Workbook, on: &[String], sheet: Option<&str>) -> anyhow::Result<usize> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let keys: Vec<u32> = on
        .iter()
        .map(|b| resolve_col(wb, idx, b))
        .collect::<anyhow::Result<_>>()?;
    let (rows, cols) = wb.sheets[idx].dimensions();
    if rows <= 1 {
        return Ok(0);
    }

    let snap = snapshot_rows(&wb.sheets[idx], 1, rows, cols);
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    let mut removed = 0;
    for row in snap {
        let sig = if keys.is_empty() {
            (0..cols)
                .map(|c| display_of(&row.0, c))
                .collect::<Vec<_>>()
                .join("\u{1}")
        } else {
            keys.iter()
                .map(|&c| display_of(&row.0, c))
                .collect::<Vec<_>>()
                .join("\u{1}")
        };
        if seen.insert(sig) {
            kept.push(row);
        } else {
            removed += 1;
        }
    }
    rewrite_rows(&mut wb.sheets[idx], 1, rows, cols, kept);
    recalc_if_formulas(wb);
    Ok(removed)
}

/// Print rows matching `predicate`. Row 0 is copied as a header.
pub fn cmd_filter(
    wb: &Workbook,
    predicate: &str,
    sheet: Option<&str>,
    opts: &render::ReadOpts,
) -> anyhow::Result<String> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let (rows, cols) = wb.sheets[idx].dimensions();
    let pred = Predicate::parse(predicate)?;
    let col = resolve_col(wb, idx, &pred.col)?;

    // Build a temp single-sheet workbook: header + matching rows, then render.
    let mut tmp = Workbook::empty();
    tmp.styles = wb.styles.clone();
    tmp.date_system = wb.date_system;
    let ti = tmp.add_sheet("filter");
    let mut out_row = 0u32;
    // Header row.
    copy_row(&wb.sheets[idx], &mut tmp.sheets[ti], 0, out_row, cols);
    out_row += 1;
    for r in 1..rows {
        if pred.matches(wb, idx, r, col) {
            copy_row(&wb.sheets[idx], &mut tmp.sheets[ti], r, out_row, cols);
            out_row += 1;
        }
    }
    if out_row == 0 {
        return Ok(String::new());
    }
    let range = core::CellRange::new(
        core::CellAddress::new(0, 0),
        core::CellAddress::new(out_row - 1, cols.saturating_sub(1)),
    );
    Ok(render::render_range(&tmp, ti, range, opts))
}

/// Inner-join two sheets on a key column and render the combined rows.
pub fn cmd_join(
    wb1: &Workbook,
    wb2: &Workbook,
    on: &str,
    opts: &render::ReadOpts,
) -> anyhow::Result<String> {
    let i1 = resolve_sheet_idx(wb1, None)?;
    let i2 = resolve_sheet_idx(wb2, None)?;
    let k1 = resolve_col(wb1, i1, on)?;
    let k2 = resolve_col(wb2, i2, on)?;
    let (r1, c1) = wb1.sheets[i1].dimensions();
    let (r2, c2) = wb2.sheets[i2].dimensions();

    // Index right-hand data rows by key.
    let mut rhs: std::collections::HashMap<String, Vec<u32>> = std::collections::HashMap::new();
    for r in 1..r2 {
        let k = wb2.display_cell(i2, r, k2);
        if !k.is_empty() {
            rhs.entry(k).or_default().push(r);
        }
    }

    let mut tmp = Workbook::empty();
    let ti = tmp.add_sheet("join");
    let total_cols = c1 + c2;
    // Header: left headers then right headers.
    for c in 0..c1 {
        tmp.sheets[ti].set(0, c, Cell::Text(wb1.display_cell(i1, 0, c)));
    }
    for c in 0..c2 {
        tmp.sheets[ti].set(0, c1 + c, Cell::Text(wb2.display_cell(i2, 0, c)));
    }
    let mut out_row = 1u32;
    for lr in 1..r1 {
        let k = wb1.display_cell(i1, lr, k1);
        let Some(matches) = rhs.get(&k) else { continue };
        for &rr in matches {
            for c in 0..c1 {
                tmp.sheets[ti].set(out_row, c, Cell::from_value(wb1.sheets[i1].value(lr, c)));
            }
            for c in 0..c2 {
                tmp.sheets[ti].set(
                    out_row,
                    c1 + c,
                    Cell::from_value(wb2.sheets[i2].value(rr, c)),
                );
            }
            out_row += 1;
        }
    }
    if out_row <= 1 && total_cols == 0 {
        return Ok(String::new());
    }
    let range = core::CellRange::new(
        core::CellAddress::new(0, 0),
        core::CellAddress::new(out_row.saturating_sub(1), total_cols.saturating_sub(1)),
    );
    Ok(render::render_range(&tmp, ti, range, opts))
}

/// Run a SQL `SELECT` over the workbook and render the result grid. Column
/// names become the header row (so `--format json` keys objects by them).
pub fn cmd_query(wb: &Workbook, sql: &str, opts: &render::ReadOpts) -> anyhow::Result<String> {
    let result = core::query::run_query(wb, sql)?;
    let ncols = result.columns.len();
    if ncols == 0 {
        return Ok(String::new());
    }

    // Build a temp single-sheet workbook (header + result rows), then render
    // it with the header treated as labels/keys.
    let mut tmp = Workbook::empty();
    let ti = tmp.add_sheet("query");
    for (c, name) in result.columns.iter().enumerate() {
        tmp.sheets[ti].set(0, c as u32, Cell::Text(name.clone()));
    }
    for (r, row) in result.rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            tmp.sheets[ti].set(r as u32 + 1, c as u32, Cell::from_value(v.clone()));
        }
    }
    let mut opts = *opts;
    opts.header = true; // column names are meaningful — always label/key by them
    let last_row = result.rows.len() as u32; // header is row 0, data starts at 1
    let range = core::CellRange::new(
        core::CellAddress::new(0, 0),
        core::CellAddress::new(last_row, ncols as u32 - 1),
    );
    Ok(render::render_range(&tmp, ti, range, &opts))
}

/// Summarize a column. Row 0 is treated as a header (excluded from stats).
pub fn cmd_profile(wb: &Workbook, column: &str, sheet: Option<&str>) -> anyhow::Result<String> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let col = resolve_col(wb, idx, column)?;
    let (rows, _cols) = wb.sheets[idx].dimensions();

    let mut count = 0u64;
    let mut nulls = 0u64;
    let mut numeric = 0u64;
    let mut text = 0u64;
    let mut text_numbers = 0u64;
    let mut text_dates = 0u64;
    let mut sum = 0.0;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut distinct = std::collections::HashSet::new();

    for r in 1..rows {
        let v = wb.sheets[idx].value(r, col);
        match v {
            super::easyexcel_components::value::CellValue::Empty => {
                nulls += 1;
                continue;
            }
            super::easyexcel_components::value::CellValue::Number(n) => {
                numeric += 1;
                sum += n;
                min = min.min(n);
                max = max.max(n);
            }
            super::easyexcel_components::value::CellValue::Text(ref t) => {
                text += 1;
                if super::easyexcel_components::formula::coerce::parse_number_text(t).is_some() {
                    text_numbers += 1;
                } else if super::easyexcel_components::dates::looks_like_date(t) {
                    text_dates += 1;
                }
            }
            _ => {}
        }
        count += 1;
        distinct.insert(wb.display_cell(idx, r, col));
    }

    let label = {
        let h = wb.display_cell(idx, 0, col);
        if h.is_empty() {
            core::addr::col_index_to_letters(col)
        } else {
            h
        }
    };
    let mut out = String::new();
    out.push_str(&format!("column:   {label}\n"));
    out.push_str(&format!("count:    {count}\n"));
    out.push_str(&format!("nulls:    {nulls}\n"));
    out.push_str(&format!("numeric:  {numeric}\n"));
    out.push_str(&format!("text:     {text}\n"));
    out.push_str(&format!("distinct: {}\n", distinct.len()));
    if numeric > 0 {
        let g = super::easyexcel_components::value::format_number_general;
        out.push_str(&format!("sum:      {}\n", g(sum)));
        out.push_str(&format!("mean:     {}\n", g(sum / numeric as f64)));
        out.push_str(&format!("min:      {}\n", g(min)));
        out.push_str(&format!("max:      {}\n", g(max)));
    }
    if text_numbers > 0 {
        out.push_str(&format!(
            "WARNING:  {text_numbers} value(s) are numbers stored as text \
             (SUM/AVERAGE ignore them) — run `xls to-number` to fix\n"
        ));
    }
    if text_dates > 0 {
        out.push_str(&format!(
            "WARNING:  {text_dates} value(s) look like dates stored as text \
             (not real dates) — run `xls to-date --format <code>` to fix\n"
        ));
    }
    Ok(out.trim_end().to_string())
}

/// Print cells whose displayed value contains `pattern` (case-insensitive),
/// with their addresses. Returns whether any matched.
pub fn cmd_grep(wb: &Workbook, pattern: &str, sheet: Option<&str>) -> anyhow::Result<bool> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let (rows, cols) = wb.sheets[idx].dimensions();
    let needle = pattern.to_lowercase();
    let sheet_name = &wb.sheets[idx].name;
    let mut found = false;
    for r in 0..rows {
        let mut hits = Vec::new();
        for c in 0..cols {
            let v = wb.display_cell(idx, r, c);
            if !v.is_empty() && v.to_lowercase().contains(&needle) {
                hits.push((c, v));
            }
        }
        if !hits.is_empty() {
            found = true;
            for (c, v) in hits {
                let addr = format!("{}{}", core::addr::col_index_to_letters(c), r + 1);
                println!("{sheet_name}!{addr}\t{v}");
            }
        }
    }
    Ok(found)
}

/// Render the first or last `count` rows of a sheet.
pub fn cmd_head_tail(
    wb: &Workbook,
    count: u32,
    sheet: Option<&str>,
    tail: bool,
    opts: &render::ReadOpts,
) -> anyhow::Result<String> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let (rows, cols) = wb.sheets[idx].dimensions();
    if rows == 0 || cols == 0 {
        return Ok(String::new());
    }
    let (start, end) = if tail {
        (rows.saturating_sub(count), rows - 1)
    } else {
        (0, count.min(rows) - 1)
    };
    let range = core::CellRange::new(
        core::CellAddress::new(start, 0),
        core::CellAddress::new(end, cols - 1),
    );
    Ok(render::render_range(wb, idx, range, opts))
}

/// Apply many `CELL=VALUE` edits in one open/save. Returns the count applied.
pub fn cmd_batch(wb: &mut Workbook, sets: &[String], sheet: Option<&str>) -> anyhow::Result<usize> {
    for s in sets {
        let (cellref, value) = s
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("expected CELL=VALUE, got '{s}'"))?;
        let (sidx, a1) = if cellref.contains('!') {
            let (i, a) = split_sheet_cell(wb, cellref);
            (i, a.to_string())
        } else {
            (resolve_sheet_idx(wb, sheet)?, cellref.trim().to_string())
        };
        let addr = core::CellAddress::parse_a1(&a1)
            .ok_or_else(|| anyhow::anyhow!("invalid cell reference: {a1}"))?;
        let cell = parse_value_string(value);
        wb.sheet_mut(sidx)
            .ok_or_else(|| anyhow::anyhow!("sheet index {sidx} out of range"))?
            .set(addr.row, addr.col, cell);
    }
    recalc_if_formulas(wb);
    Ok(sets.len())
}

/// Auto-fit column widths to content. Returns the number of columns fitted.
pub fn cmd_autofit(
    wb: &mut Workbook,
    cols: Option<&str>,
    sheet: Option<&str>,
) -> anyhow::Result<usize> {
    let idx = resolve_sheet_idx(wb, sheet)?;
    let (rows, ncols) = wb.sheets[idx].dimensions();
    let (c0, c1) = match cols {
        Some(spec) => parse_col_range(spec)?,
        None => (0, ncols.saturating_sub(1)),
    };
    let mut fitted = 0;
    for c in c0..=c1 {
        let mut width = 0usize;
        for r in 0..rows {
            width = width.max(wb.display_cell(idx, r, c).chars().count());
        }
        // Excel widths are ~character units; pad a little, clamp to a sane range.
        let w = ((width + 2).clamp(3, 120)) as f64;
        let info = wb.sheets[idx].columns.entry(c).or_default();
        info.width = Some(w);
        fitted += 1;
    }
    Ok(fitted)
}

/// Styling options for the `style` command.
pub struct StyleOpts<'a> {
    pub bold: bool,
    pub italic: bool,
    pub color: Option<&'a str>,
    pub bg: Option<&'a str>,
}

/// Apply bold/italic/colors to every cell in a range. Returns the count.
pub fn cmd_style(
    wb: &mut Workbook,
    range: &str,
    opts: &StyleOpts,
    sheet: Option<&str>,
) -> anyhow::Result<usize> {
    use super::easyexcel_components::styles::{Color, FillPattern};
    let idx = resolve_sheet_idx(wb, sheet)?;
    let rng = parse_range(wb, range)?;
    let font_color = opts.color.map(parse_hex_color).transpose()?;
    let bg_color = opts.bg.map(parse_hex_color).transpose()?;

    let mut n = 0;
    for (r, c) in rng.iter_cells() {
        let mut style = wb.sheets[idx]
            .style_at(r, c)
            .and_then(|si| wb.styles.get(si).cloned())
            .unwrap_or_default();
        if opts.bold {
            style.font.bold = true;
        }
        if opts.italic {
            style.font.italic = true;
        }
        if let Some(rgb) = font_color {
            style.font.color = Color::rgb(rgb);
        }
        if let Some(rgb) = bg_color {
            style.fill.pattern = FillPattern::Solid;
            style.fill.fg = Color::rgb(rgb);
        }
        let new_idx = wb.styles.intern(style);
        wb.sheets[idx].set_style(r, c, new_idx);
        n += 1;
    }
    Ok(n)
}

/// Parse an `RRGGBB` hex color into an opaque ARGB value.
fn parse_hex_color(s: &str) -> anyhow::Result<u32> {
    let h = s.trim().trim_start_matches('#');
    let rgb = u32::from_str_radix(h, 16)
        .map_err(|_| anyhow::anyhow!("invalid hex color '{s}' (expected RRGGBB)"))?;
    if h.len() != 6 {
        anyhow::bail!("invalid hex color '{s}' (expected 6 hex digits, RRGGBB)");
    }
    Ok(0xFF00_0000 | rgb)
}

/// Parse a column spec to a 0-based inclusive `(start, end)` index pair.
/// Accepts `B`, `B:D`, or a 1-based number.
fn parse_col_range(spec: &str) -> anyhow::Result<(u32, u32)> {
    let spec = spec.trim();
    if let Some((a, b)) = spec.split_once(':') {
        Ok((parse_column(a)?, parse_column(b)?))
    } else {
        let c = parse_column(spec)?;
        Ok((c, c))
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Split a `Sheet!A1` (or bare `A1`) reference.  Returns (sheet_index, a1_str).
fn split_sheet_cell<'a>(wb: &Workbook, cell: &'a str) -> (usize, &'a str) {
    if let Some((sheet, a1)) = cell.rsplit_once('!') {
        let name = sheet.trim_matches('\'');
        if let Some(idx) = wb.sheet_index(name) {
            return (idx, a1);
        }
    }
    (0, cell)
}

/// Parse `[Sheet!]A1` into a [`CellRef`].
fn parse_cell_ref(wb: &Workbook, spec: &str) -> anyhow::Result<CellRef> {
    let (sheet, a1) = split_sheet_cell(wb, spec);
    let addr = core::CellAddress::parse_a1(a1)
        .ok_or_else(|| anyhow::anyhow!("invalid cell reference: {a1}"))?;
    Ok(CellRef {
        sheet,
        row: addr.row,
        col: addr.col,
    })
}

/// Parse a user-supplied value string into a [`Cell`].
///
/// * Starts with `=` → Formula
/// * `true` / `false` (case-insensitive) → Bool
/// * Parses as a number → Number — including `%` and thousands separators
///   (`6,000.00`, `1,51,302.63`), which are stripped so the cell stores a real
///   number and exports correctly to Excel
/// * Otherwise → Text
fn parse_value_string(s: &str) -> Cell {
    if s.starts_with('=') {
        return Cell::Formula {
            expr: s.to_string(),
            cached: Default::default(),
        };
    }
    if s.eq_ignore_ascii_case("true") {
        return Cell::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Cell::Bool(false);
    }
    if let Some(n) = core::formula::coerce::parse_number_text(s) {
        return Cell::Number(n);
    }
    Cell::Text(s.to_string())
}

/// Parse an `A1:B2` (or bare `A1`) range into a normalized [`CellRange`].
fn parse_range(_wb: &Workbook, range: &str) -> anyhow::Result<core::CellRange> {
    core::CellRange::parse_a1(range.trim()).ok_or_else(|| anyhow::anyhow!("invalid range: {range}"))
}

/// Parse a column given as a letter (`C`) or a 1-based number (`3`) to a 0-based
/// column index.
fn parse_column(s: &str) -> anyhow::Result<u32> {
    let s = s.trim();
    if let Ok(n) = s.parse::<u32>() {
        return one_based_to_index(n);
    }
    core::addr::col_letters_to_index(&s.to_ascii_uppercase())
        .ok_or_else(|| anyhow::anyhow!("invalid column: {s}"))
}

/// Convert a 1-based row/column number to a 0-based index (errors on 0).
fn one_based_to_index(n: u32) -> anyhow::Result<u32> {
    n.checked_sub(1)
        .ok_or_else(|| anyhow::anyhow!("row/column numbers are 1-based (got 0)"))
}

/// Resolve the target sheet, apply `f`, then recalc if the workbook has formulas.
fn mutate_sheet<F>(wb: &mut Workbook, sheet: Option<&str>, f: F) -> anyhow::Result<()>
where
    F: FnOnce(&mut super::easyexcel_components::Sheet),
{
    let idx = resolve_sheet_idx(wb, sheet)?;
    f(&mut wb.sheets[idx]);
    recalc_if_formulas(wb);
    Ok(())
}

/// Recalculate only if some sheet actually contains a formula (cheap no-op
/// otherwise).
fn recalc_if_formulas(wb: &mut Workbook) {
    let has_formulas = wb
        .sheets
        .iter()
        .any(|s| s.cells.values().any(|c| c.is_formula()));
    if has_formulas {
        Engine::new().recalc(wb);
    }
}

/// Resolve a sheet name (or `None`) to an index.
fn resolve_sheet_idx(wb: &Workbook, sheet: Option<&str>) -> anyhow::Result<usize> {
    match sheet {
        Some(name) => wb
            .sheet_index(name)
            .ok_or_else(|| anyhow::anyhow!("sheet not found: {name}")),
        None => Ok(wb.active_sheet.min(wb.sheets.len().saturating_sub(1))),
    }
}

/// Format a 0-based (row, col) as an A1 string.
fn cell_addr_str(row: u32, col: u32) -> String {
    format!("{}{}", core::addr::col_index_to_letters(col), row + 1)
}

/// Resolve a column spec to a 0-based index: a header name (row 0,
/// case-insensitive) takes precedence, else a column letter (`H` or `H:H`).
fn resolve_col(wb: &Workbook, sheet_idx: usize, spec: &str) -> anyhow::Result<u32> {
    let trimmed = spec.trim();
    if let Some(c) = find_header(wb, sheet_idx, trimmed) {
        return Ok(c);
    }
    let letters = trimmed.split(':').next().unwrap_or(trimmed);
    if !letters.is_empty()
        && letters.chars().all(|c| c.is_ascii_alphabetic())
        && let Some(c) = core::addr::col_letters_to_index(&letters.to_ascii_uppercase())
    {
        return Ok(c);
    }
    anyhow::bail!("column not found (not a header name or column letter): {spec}")
}

/// Find a column by header name (row 0, case-insensitive).
fn find_header(wb: &Workbook, sheet_idx: usize, name: &str) -> Option<u32> {
    let (_, cols) = wb.sheets.get(sheet_idx)?.dimensions();
    (0..cols).find(|&c| wb.display_cell(sheet_idx, 0, c).eq_ignore_ascii_case(name))
}

/// Display values of one row across `ncols` columns.
fn row_values(wb: &Workbook, sheet_idx: usize, row: u32, ncols: u32) -> Vec<String> {
    (0..ncols)
        .map(|c| wb.display_cell(sheet_idx, row, c))
        .collect()
}

/// A snapshot of one row: per-column cell and style index.
type RowSnap = (Vec<Option<Cell>>, Vec<Option<u32>>);

/// Snapshot data rows `start..end` (exclusive end) with their cells and styles.
fn snapshot_rows(sheet: &core::Sheet, start: u32, end: u32, cols: u32) -> Vec<RowSnap> {
    (start..end)
        .map(|r| {
            let cells = (0..cols).map(|c| sheet.get(r, c).cloned()).collect();
            let styles = (0..cols).map(|c| sheet.style_at(r, c)).collect();
            (cells, styles)
        })
        .collect()
}

/// Clear data rows `start..end` then write `snap` back consecutively from `start`.
fn rewrite_rows(sheet: &mut core::Sheet, start: u32, end: u32, cols: u32, snap: Vec<RowSnap>) {
    if cols == 0 || end <= start {
        return;
    }
    let range = core::CellRange::new(
        core::CellAddress::new(start, 0),
        core::CellAddress::new(end - 1, cols - 1),
    );
    sheet.clear_range(range);
    for (i, (cells, styles)) in snap.into_iter().enumerate() {
        let r = start + i as u32;
        for c in 0..cols as usize {
            if let Some(cell) = &cells[c] {
                sheet.set(r, c as u32, cell.clone());
            }
            if let Some(si) = styles[c] {
                sheet.set_style(r, c as u32, si);
            }
        }
    }
}

/// Display string of a snapshot row's column.
fn display_of(cells: &[Option<Cell>], col: u32) -> String {
    cells
        .get(col as usize)
        .and_then(|o| o.as_ref())
        .map(|cell| cell.value().to_display_string())
        .unwrap_or_default()
}

/// Order two display strings numerically when both parse as numbers, else lexically.
fn cmp_values(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.cmp(b),
    }
}

/// Copy a row (cells + style indices) from `src` to `dst`. Style indices are
/// assumed to share a table (callers clone `wb.styles` into the temp workbook).
fn copy_row(src: &core::Sheet, dst: &mut core::Sheet, src_r: u32, dst_r: u32, cols: u32) {
    for c in 0..cols {
        if let Some(cell) = src.get(src_r, c) {
            dst.set(dst_r, c, cell.clone());
        }
        if let Some(si) = src.style_at(src_r, c) {
            dst.set_style(dst_r, c, si);
        }
    }
}

/// Comparison operator for `filter` predicates.
#[derive(Clone, Copy)]
enum PredOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
    IsNumber,
    IsText,
}

/// A parsed `filter` predicate: `<col> <op> <rhs>`, or `<col>:number|text`.
struct Predicate {
    col: String,
    op: PredOp,
    rhs: String,
}

impl Predicate {
    fn parse(s: &str) -> anyhow::Result<Predicate> {
        // Type predicates: `<col>:number` / `<col>:text`.
        if let Some((c, kind)) = s.split_once(':') {
            match kind.trim().to_ascii_lowercase().as_str() {
                "number" => {
                    return Ok(Predicate {
                        col: c.trim().to_string(),
                        op: PredOp::IsNumber,
                        rhs: String::new(),
                    });
                }
                "text" => {
                    return Ok(Predicate {
                        col: c.trim().to_string(),
                        op: PredOp::IsText,
                        rhs: String::new(),
                    });
                }
                _ => {}
            }
        }
        // Comparison operators, longest symbols first.
        for (sym, op) in [
            (">=", PredOp::Ge),
            ("<=", PredOp::Le),
            ("!=", PredOp::Ne),
            ("==", PredOp::Eq),
            ("~", PredOp::Contains),
            (">", PredOp::Gt),
            ("<", PredOp::Lt),
        ] {
            if let Some(pos) = s.find(sym) {
                let col = s[..pos].trim().to_string();
                let rhs = s[pos + sym.len()..].trim().to_string();
                if col.is_empty() {
                    anyhow::bail!("predicate is missing a column: '{s}'");
                }
                return Ok(Predicate { col, op, rhs });
            }
        }
        anyhow::bail!("could not parse predicate: '{s}'")
    }

    fn matches(&self, wb: &Workbook, sheet_idx: usize, row: u32, col: u32) -> bool {
        use std::cmp::Ordering;
        let v = wb.sheets[sheet_idx].value(row, col);
        match self.op {
            PredOp::IsNumber => matches!(v, CellValue::Number(_)),
            PredOp::IsText => matches!(v, CellValue::Text(_)),
            PredOp::Contains => wb
                .display_cell(sheet_idx, row, col)
                .to_lowercase()
                .contains(&self.rhs.to_lowercase()),
            _ => {
                let numeric = match (&v, self.rhs.parse::<f64>()) {
                    (CellValue::Number(x), Ok(y)) => Some((x, y)),
                    _ => None,
                };
                match numeric {
                    Some((x, y)) => {
                        let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                        match self.op {
                            PredOp::Eq => o == Ordering::Equal,
                            PredOp::Ne => o != Ordering::Equal,
                            PredOp::Gt => o == Ordering::Greater,
                            PredOp::Ge => o != Ordering::Less,
                            PredOp::Lt => o == Ordering::Less,
                            PredOp::Le => o != Ordering::Greater,
                            _ => false,
                        }
                    }
                    None => {
                        let disp = wb.display_cell(sheet_idx, row, col);
                        let (a, b) = (disp.as_str(), self.rhs.as_str());
                        match self.op {
                            PredOp::Eq => a == b,
                            PredOp::Ne => a != b,
                            PredOp::Gt => a > b,
                            PredOp::Ge => a >= b,
                            PredOp::Lt => a < b,
                            PredOp::Le => a <= b,
                            _ => false,
                        }
                    }
                }
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::easyexcel_components::csv::{CsvWriteOptions, write_csv};
    use crate::cli::easyexcel_components::model::{Cell, Workbook};
    use crate::cli::easyexcel_components::{dates, styles, value};
    use std::io::Write;

    // ── helpers ─────────────────────────────────────────────────────────────

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(name);
        p
    }

    fn make_simple_wb() -> Workbook {
        let mut wb = Workbook::new();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Number(1.0));
        s.set_a1("B1", Cell::Number(2.0));
        s.set_a1("A2", Cell::Text("hello".into()));
        s.set_a1("B2", Cell::Bool(true));
        wb
    }

    fn write_tmp_csv(name: &str, data: &str) -> PathBuf {
        let p = tmp_path(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(data.as_bytes()).unwrap();
        p
    }

    fn write_tmp_wb_csv(name: &str, wb: &Workbook) -> PathBuf {
        let p = tmp_path(name);
        let f = std::fs::File::create(&p).unwrap();
        write_csv(wb, 0, f, &CsvWriteOptions::default()).unwrap();
        p
    }

    fn write_tmp_wb_xlsx(name: &str, wb: &Workbook) -> PathBuf {
        let p = tmp_path(name);
        core::save_path(wb, &p).unwrap();
        p
    }

    // ── cmd_get ─────────────────────────────────────────────────────────────

    #[test]
    fn get_bare_a1() {
        let wb = make_simple_wb();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "1");
        assert_eq!(cmd_get(&wb, "A2").unwrap(), "hello");
        assert_eq!(cmd_get(&wb, "B2").unwrap(), "TRUE");
    }

    #[test]
    fn get_with_sheet_prefix() {
        let wb = make_simple_wb();
        assert_eq!(cmd_get(&wb, "Sheet1!B1").unwrap(), "2");
    }

    #[test]
    fn get_empty_cell() {
        let wb = make_simple_wb();
        assert_eq!(cmd_get(&wb, "Z99").unwrap(), "");
    }

    #[test]
    fn get_invalid_cell_ref() {
        let wb = make_simple_wb();
        assert!(cmd_get(&wb, "not_a_ref").is_err());
    }

    // ── cmd_set ─────────────────────────────────────────────────────────────

    #[test]
    fn set_number() {
        let mut wb = make_simple_wb();
        cmd_set(&mut wb, "A1", "42").unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "42");
    }

    #[test]
    fn set_bool() {
        let mut wb = make_simple_wb();
        cmd_set(&mut wb, "A1", "false").unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "FALSE");
    }

    #[test]
    fn set_text() {
        let mut wb = make_simple_wb();
        cmd_set(&mut wb, "A1", "world").unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "world");
    }

    #[test]
    fn set_formula_and_recalc() {
        let mut wb = Workbook::new();
        {
            let s = wb.sheet_mut(0).unwrap();
            s.set_a1("A1", Cell::Number(3.0));
            s.set_a1("A2", Cell::Number(4.0));
        }
        cmd_set(&mut wb, "A3", "=A1+A2").unwrap();
        assert_eq!(cmd_get(&wb, "A3").unwrap(), "7");
    }

    // ── cmd_eval ────────────────────────────────────────────────────────────

    #[test]
    fn eval_constant_formula() {
        let mut wb = Workbook::new();
        assert_eq!(
            cmd_eval(&mut wb, "=1+2", None, &render::ReadOpts::default()).unwrap(),
            "3"
        );
    }

    #[test]
    fn eval_with_cell_refs() {
        let mut wb = Workbook::new();
        {
            let s = wb.sheet_mut(0).unwrap();
            s.set_a1("A1", Cell::Number(10.0));
            s.set_a1("A2", Cell::Number(20.0));
        }
        assert_eq!(
            cmd_eval(&mut wb, "=SUM(A1:A2)", None, &render::ReadOpts::default()).unwrap(),
            "30"
        );
    }

    #[test]
    fn eval_with_at_flag() {
        let mut wb = Workbook::new();
        {
            let s = wb.sheet_mut(0).unwrap();
            s.set_a1("A1", Cell::Number(10.0));
            s.set_a1("A2", Cell::Number(20.0));
        }
        // Parse the at= flag as a CellRef without error.
        let r = cmd_eval(
            &mut wb,
            "=SUM(A1:A2)",
            Some("Sheet1!B5"),
            &render::ReadOpts::default(),
        )
        .unwrap();
        // SUM(A1:A2) is not relative to the context cell so the result is 30.
        assert_eq!(r, "30");
    }

    // ── cmd_export CSV ──────────────────────────────────────────────────────

    #[test]
    fn export_to_csv() {
        let wb = make_simple_wb();
        let out = tmp_path("cli_test_export.csv");
        cmd_export(
            &wb,
            "source.xlsx",
            ExportFormat::Csv,
            None,
            Some(&out),
            &RawArgs::default(),
        )
        .unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("hello"));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn export_derived_out_path() {
        let wb = make_simple_wb();
        // Supply no --out; path should be derived from the stem "mysrc" + ".csv"
        let expected_out = PathBuf::from("mysrc.csv");
        // We can't write to CWD in all test environments, so just check the
        // resolution logic via the naming convention, not actual I/O.
        let src = "mysrc.xlsx";
        let stem = Path::new(src)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let derived = PathBuf::from(format!("{}.{}", stem, ExportFormat::Csv.extension()));
        assert_eq!(derived, expected_out);
        drop(wb); // keep wb alive through the drop point
    }

    // ── cmd_export XLSX ─────────────────────────────────────────────────────

    #[test]
    fn export_to_xlsx_and_reimport() {
        let wb = make_simple_wb();
        let out = tmp_path("cli_test_export.xlsx");
        cmd_export(
            &wb,
            "src.csv",
            ExportFormat::Xlsx,
            None,
            Some(&out),
            &RawArgs::default(),
        )
        .unwrap();
        // Re-open and verify a cell.
        let wb2 = core::open_path(&out).unwrap();
        assert_eq!(cmd_get(&wb2, "A2").unwrap(), "hello");
        let _ = std::fs::remove_file(&out);
    }

    // ── cmd_import ──────────────────────────────────────────────────────────

    #[test]
    fn import_csv_into_new_workbook() {
        let csv = write_tmp_csv("cli_test_import_src.csv", "x,y\n1,2\n3,4\n");
        let target = tmp_path("cli_test_import_target.xlsx");
        cmd_import(csv.to_str().unwrap(), &target, Some("Data")).unwrap();
        let wb = core::open_path(&target).unwrap();
        assert!(wb.sheet_index("Data").is_some());
        assert_eq!(cmd_get(&wb, "Data!A1").unwrap(), "x");
        let _ = std::fs::remove_file(&csv);
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn import_csv_replaces_existing_sheet() {
        // Create a workbook with a sheet named "Data".
        let mut wb = Workbook::new();
        wb.sheets[0].name = "Data".to_string();
        wb.sheets[0].set_a1("A1", Cell::Text("old".into()));
        let target = tmp_path("cli_test_import_replace.xlsx");
        core::save_path(&wb, &target).unwrap();

        let csv = write_tmp_csv("cli_test_import_replace_src.csv", "new_val\n42\n");
        cmd_import(csv.to_str().unwrap(), &target, Some("Data")).unwrap();

        let wb2 = core::open_path(&target).unwrap();
        assert_eq!(cmd_get(&wb2, "Data!A1").unwrap(), "new_val");
        let _ = std::fs::remove_file(&csv);
        let _ = std::fs::remove_file(&target);
    }

    // ── cmd_diff ────────────────────────────────────────────────────────────

    #[test]
    fn diff_identical() {
        let wb1 = make_simple_wb();
        let wb2 = make_simple_wb();
        assert!(!cmd_diff(&wb1, &wb2));
    }

    #[test]
    fn diff_single_change() {
        let wb1 = make_simple_wb();
        let mut wb2 = make_simple_wb();
        wb2.sheet_mut(0).unwrap().set_a1("A1", Cell::Number(99.0));
        assert!(cmd_diff(&wb1, &wb2));
    }

    #[test]
    fn diff_missing_sheet() {
        let wb1 = make_simple_wb();
        let mut wb2 = Workbook::new();
        // wb2 has Sheet1 but all empty → A1 is "" vs "1" etc.
        wb2.sheets[0].name = "OtherSheet".to_string();
        // wb1 Sheet1 vs wb2 OtherSheet — different names
        assert!(cmd_diff(&wb1, &wb2));
    }

    // ── mutation commands ───────────────────────────────────────────────────

    #[test]
    fn clear_range_clears_cells() {
        let mut wb = make_simple_wb();
        cmd_clear(&mut wb, "A1:B2", None).unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "");
        assert_eq!(cmd_get(&wb, "B2").unwrap(), "");
    }

    #[test]
    fn fill_sets_whole_range() {
        let mut wb = Workbook::new();
        cmd_fill(&mut wb, "A1:A3", "7", None).unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "7");
        assert_eq!(cmd_get(&wb, "A3").unwrap(), "7");
    }

    #[test]
    fn copy_and_move_range() {
        let mut wb = Workbook::new();
        cmd_set(&mut wb, "A1", "1").unwrap();
        cmd_set(&mut wb, "A2", "2").unwrap();
        cmd_copy_move(&mut wb, "A1:A2", "C1", None, false).unwrap();
        assert_eq!(cmd_get(&wb, "C1").unwrap(), "1");
        assert_eq!(cmd_get(&wb, "C2").unwrap(), "2");
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "1"); // copy keeps source
        // Move clears the source.
        cmd_copy_move(&mut wb, "A1:A2", "E1", None, true).unwrap();
        assert_eq!(cmd_get(&wb, "E1").unwrap(), "1");
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "");
    }

    #[test]
    fn parse_column_letter_or_number() {
        assert_eq!(parse_column("A").unwrap(), 0);
        assert_eq!(parse_column("c").unwrap(), 2);
        assert_eq!(parse_column("3").unwrap(), 2);
        assert!(parse_column("0").is_err());
        assert!(parse_column("!!").is_err());
    }

    #[test]
    fn sheet_lifecycle() {
        let mut wb = Workbook::new();
        cmd_add_sheet(&mut wb, "Data").unwrap();
        assert!(wb.sheet_index("Data").is_some());
        assert!(cmd_add_sheet(&mut wb, "Data").is_err()); // duplicate
        cmd_rename_sheet(&mut wb, "Data", "Numbers").unwrap();
        assert!(wb.sheet_index("Data").is_none());
        assert!(wb.sheet_index("Numbers").is_some());
        cmd_delete_sheet(&mut wb, "Numbers").unwrap();
        assert!(wb.sheet_index("Numbers").is_none());
        // Cannot delete the last remaining sheet.
        assert!(cmd_delete_sheet(&mut wb, "Sheet1").is_err());
    }

    #[test]
    fn insert_row_via_sheet_helper() {
        let mut wb = Workbook::new();
        cmd_set(&mut wb, "A1", "1").unwrap();
        cmd_set(&mut wb, "A2", "2").unwrap();
        wb.sheets[0].insert_rows(0, 1); // exercised through core, mirrors CLI path
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "");
        assert_eq!(cmd_get(&wb, "A2").unwrap(), "1");
    }

    // ── round-trip via temp file ─────────────────────────────────────────────

    #[test]
    fn get_set_roundtrip_csv() {
        let wb = make_simple_wb();
        let path = write_tmp_wb_csv("cli_roundtrip_csv.csv", &wb);

        let mut wb2 = core::open_path(&path).unwrap();
        cmd_set(&mut wb2, "A1", "999").unwrap();
        core::save_path(&wb2, &path).unwrap();

        let wb3 = core::open_path(&path).unwrap();
        assert_eq!(cmd_get(&wb3, "A1").unwrap(), "999");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn get_set_roundtrip_xlsx() {
        let wb = make_simple_wb();
        let path = write_tmp_wb_xlsx("cli_roundtrip_xlsx.xlsx", &wb);

        let mut wb2 = core::open_path(&path).unwrap();
        cmd_set(&mut wb2, "B1", "777").unwrap();
        core::save_path(&wb2, &path).unwrap();

        let wb3 = core::open_path(&path).unwrap();
        assert_eq!(cmd_get(&wb3, "B1").unwrap(), "777");
        let _ = std::fs::remove_file(&path);
    }

    // ── data-manipulation & discovery verbs ──────────────────────────────────

    /// A small bank-statement-like workbook with a header row.
    fn make_table_wb() -> Workbook {
        let mut wb = Workbook::new();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("desc".into()));
        s.set_a1("B1", Cell::Text("category".into()));
        s.set_a1("C1", Cell::Text("amount".into()));
        let rows = [
            ("PETROL", "fuel", 1200.0),
            ("WAZIRX", "crypto", 5000.0),
            ("PETROL2", "fuel", 800.0),
            ("WAZIRX2", "crypto", 2500.0),
        ];
        for (i, (d, c, a)) in rows.iter().enumerate() {
            let r = i as u32 + 2; // rows 2..=5
            s.set_a1(&format!("A{r}"), Cell::Text((*d).into()));
            s.set_a1(&format!("B{r}"), Cell::Text((*c).into()));
            s.set_a1(&format!("C{r}"), Cell::Number(*a));
        }
        wb
    }

    #[test]
    fn read_range_and_single() {
        let wb = make_table_wb();
        let opts = render::ReadOpts {
            format: render::OutFormat::Csv,
            ..Default::default()
        };
        let out = cmd_read(&wb, "A1:C2", &opts).unwrap();
        assert_eq!(out, "desc,category,amount\nPETROL,fuel,1200");
        assert_eq!(
            cmd_read(&wb, "C2", &render::ReadOpts::default()).unwrap(),
            "1200"
        );
    }

    #[test]
    fn pivot_sum_and_count() {
        let wb = make_table_wb();
        assert_eq!(
            cmd_pivot(&wb, "category", "amount", Agg::Sum, None).unwrap(),
            "category\tsum\ncrypto\t7500\nfuel\t2000"
        );
        assert_eq!(
            cmd_pivot(&wb, "category", "amount", Agg::Count, None).unwrap(),
            "category\tcount\ncrypto\t2\nfuel\t2"
        );
    }

    #[test]
    fn sort_by_amount_desc_keeps_header() {
        let mut wb = make_table_wb();
        cmd_sort(&mut wb, &["amount".to_string()], true, None).unwrap();
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "desc"); // header pinned
        assert_eq!(cmd_get(&wb, "C2").unwrap(), "5000"); // largest first
        assert_eq!(cmd_get(&wb, "C5").unwrap(), "800"); // smallest last
    }

    #[test]
    fn dedup_on_category() {
        let mut wb = make_table_wb();
        let removed = cmd_dedup(&mut wb, &["category".to_string()], None).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(cmd_get(&wb, "B2").unwrap(), "fuel");
        assert_eq!(cmd_get(&wb, "B3").unwrap(), "crypto");
        assert_eq!(cmd_get(&wb, "B4").unwrap(), ""); // rows pulled up
    }

    #[test]
    fn filter_numeric_and_type() {
        let wb = make_table_wb();
        let opts = render::ReadOpts {
            format: render::OutFormat::Csv,
            ..Default::default()
        };
        let out = cmd_filter(&wb, "amount>1000", None, &opts).unwrap();
        assert!(out.contains("PETROL,fuel,1200"));
        assert!(out.contains("WAZIRX,crypto,5000"));
        assert!(!out.contains(",800"));
        let out2 = cmd_filter(&wb, "amount:number", None, &opts).unwrap();
        assert_eq!(out2.lines().count(), 5); // header + 4 numeric rows
    }

    #[test]
    fn profile_flags_text_numbers() {
        let mut wb = Workbook::new();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("amount".into()));
        s.set_a1("A2", Cell::Text("6,000.00".into())); // text-stored number
        s.set_a1("A3", Cell::Number(100.0));
        let out = cmd_profile(&wb, "amount", None).unwrap();
        assert!(out.contains("numbers stored as text"), "{out}");
    }

    #[test]
    fn profile_flags_text_dates() {
        let mut wb = Workbook::new();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("date".into()));
        s.set_a1("A2", Cell::Text("04/04/2025".into())); // text-stored date
        s.set_a1("A3", Cell::Text("13/04/2025".into()));
        let out = cmd_profile(&wb, "date", None).unwrap();
        assert!(out.contains("dates stored as text"), "{out}");
    }

    #[test]
    fn to_date_converts_and_formats() {
        let mut wb = Workbook::new();
        {
            let s = wb.sheet_mut(0).unwrap();
            s.set_a1("A1", Cell::Text("04/04/2025".into()));
            s.set_a1("A2", Cell::Text("13/04/2025".into()));
            s.set_a1("A3", Cell::Text("not a date".into()));
        }
        let n = cmd_to_date(&mut wb, "A1:A3", "dd/mm/yyyy", None).unwrap();
        assert_eq!(n, 2); // the garbage row is left as text

        // The converted cells are now real numeric date serials...
        let serial = dates::ymd_to_serial(wb.date_system, 2025, 4, 4).unwrap();
        assert_eq!(wb.sheets[0].value(0, 0), CellValue::Number(serial));
        assert_eq!(
            wb.sheets[0].value(2, 0),
            CellValue::Text("not a date".into())
        );

        // ...carrying a date number format (so `format`/`--dates` recognize them).
        assert_eq!(
            render::describe_number_format(&wb, 0, 0, 0),
            "DATE dd/mm/yyyy"
        );
        // Raw serial is now emittable directly.
        let opts = render::ReadOpts {
            raw: true,
            dates: Some(render::DateMode::Serial),
            ..Default::default()
        };
        assert_eq!(
            render::cell_string(&wb, 0, 0, 0, &opts),
            value::format_number_general(serial)
        );
    }

    #[test]
    fn keyed_diff_reports_changes() {
        let mut a = make_table_wb();
        a.sheet_mut(0)
            .unwrap()
            .set_a1("D1", Cell::Text("id".into()));
        for r in 2..=5 {
            a.sheet_mut(0)
                .unwrap()
                .set_a1(&format!("D{r}"), Cell::Number((r - 1) as f64));
        }
        let mut b = a.clone();
        b.sheet_mut(0).unwrap().set_a1("C3", Cell::Number(9999.0)); // change
        b.sheet_mut(0)
            .unwrap()
            .clear_range(core::CellRange::parse_a1("A5:D5").unwrap()); // drop id=4
        assert!(cmd_diff_keyed(&a, &b, "id", None).unwrap());
        // identical → no diff
        assert!(!cmd_diff_keyed(&a, &a, "id", None).unwrap());
    }

    #[test]
    fn append_aligns_by_header() {
        let mut base = make_table_wb();
        let mut add = Workbook::new();
        let s = add.sheet_mut(0).unwrap();
        // Different column order than base.
        s.set_a1("A1", Cell::Text("amount".into()));
        s.set_a1("B1", Cell::Text("desc".into()));
        s.set_a1("C1", Cell::Text("category".into()));
        s.set_a1("A2", Cell::Number(42.0));
        s.set_a1("B2", Cell::Text("NEWTXN".into()));
        s.set_a1("C2", Cell::Text("misc".into()));
        let n = cmd_append(&mut base, &add, None).unwrap();
        assert_eq!(n, 1);
        assert_eq!(cmd_get(&base, "A6").unwrap(), "NEWTXN");
        assert_eq!(cmd_get(&base, "B6").unwrap(), "misc");
        assert_eq!(cmd_get(&base, "C6").unwrap(), "42");
    }

    #[test]
    fn join_inner_on_key() {
        let left = make_table_wb();
        let mut right = Workbook::new();
        let s = right.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("category".into()));
        s.set_a1("B1", Cell::Text("budget".into()));
        s.set_a1("A2", Cell::Text("fuel".into()));
        s.set_a1("B2", Cell::Number(3000.0));
        let opts = render::ReadOpts {
            format: render::OutFormat::Csv,
            ..Default::default()
        };
        let out = cmd_join(&left, &right, "category", &opts).unwrap();
        assert!(out.contains("PETROL,fuel,1200,fuel,3000"));
        assert!(!out.contains("crypto"));
    }

    #[test]
    fn batch_applies_all_edits() {
        let mut wb = Workbook::new();
        let n = cmd_batch(
            &mut wb,
            &[
                "A1=hello".to_string(),
                "B2=42".to_string(),
                "C3==1+1".to_string(),
            ],
            None,
        )
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(cmd_get(&wb, "A1").unwrap(), "hello");
        assert_eq!(cmd_get(&wb, "B2").unwrap(), "42");
        assert_eq!(cmd_get(&wb, "C3").unwrap(), "2"); // formula evaluated
    }

    #[test]
    fn format_set_then_introspect() {
        let mut wb = make_table_wb();
        let n = cmd_format_set(&mut wb, "C2:C5", "#,##0.00", None).unwrap();
        assert_eq!(n, 4);
        let (idx, a1) = split_sheet_cell(&wb, "C2");
        let addr = core::CellAddress::parse_a1(a1).unwrap();
        assert_eq!(
            render::describe_number_format(&wb, idx, addr.row, addr.col),
            "NUMBER #,##0.00"
        );
    }

    #[test]
    fn resolve_col_header_or_letter() {
        let wb = make_table_wb();
        assert_eq!(resolve_col(&wb, 0, "amount").unwrap(), 2);
        assert_eq!(resolve_col(&wb, 0, "C").unwrap(), 2);
        assert_eq!(resolve_col(&wb, 0, "A:A").unwrap(), 0);
        // A spec that is neither a known header nor valid column letters errors.
        assert!(resolve_col(&wb, 0, "1!").is_err());
    }

    #[test]
    fn query_group_by_renders() {
        let wb = make_table_wb();
        let opts = render::ReadOpts {
            format: render::OutFormat::Csv,
            ..Default::default()
        };
        let out = cmd_query(
            &wb,
            "SELECT category, SUM(amount) AS total FROM Sheet1 GROUP BY category ORDER BY total DESC",
            &opts,
        )
        .unwrap();
        assert_eq!(out, "category,total\ncrypto,7500\nfuel,2000");
    }

    #[test]
    fn eval_array_renders_grid() {
        let mut wb = make_table_wb();
        let opts = render::ReadOpts {
            format: render::OutFormat::Csv,
            ..Default::default()
        };
        let out = cmd_eval(&mut wb, "=C2:C3", None, &opts).unwrap();
        assert_eq!(out, "1200\n5000");
    }

    #[test]
    fn autofit_sets_widths() {
        let mut wb = make_table_wb();
        cmd_autofit(&mut wb, None, None).unwrap();
        // Column A's widest content is "PETROL2" (7 chars) → 7 + 2 padding.
        let w = wb.sheets[0].columns.get(&0).and_then(|ci| ci.width);
        assert_eq!(w, Some(9.0));
    }

    #[test]
    fn style_applies_bold_and_color() {
        let mut wb = make_table_wb();
        let opts = StyleOpts {
            bold: true,
            italic: false,
            color: Some("FF0000"),
            bg: None,
        };
        let n = cmd_style(&mut wb, "A1:C1", &opts, None).unwrap();
        assert_eq!(n, 3);
        let si = wb.sheets[0].style_at(0, 0).unwrap();
        let st = wb.styles.get(si).unwrap();
        assert!(st.font.bold);
        assert_eq!(st.font.color, styles::Color::rgb(0xFFFF_0000));
    }

    #[test]
    fn parse_hex_color_validates() {
        assert_eq!(parse_hex_color("FF0000").unwrap(), 0xFFFF_0000);
        assert_eq!(parse_hex_color("#00FF00").unwrap(), 0xFF00_FF00);
        assert!(parse_hex_color("xyz").is_err());
        assert!(parse_hex_color("FFF").is_err()); // wrong length
    }
}
