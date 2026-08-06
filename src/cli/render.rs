//! Shared output helpers for headless reads.
//!
//! Renders a rectangular grid of cells across several output formats
//! (`table`/`csv`/`tsv`/`json`/`jsonl`/`md`), with optional `--raw`
//! (stored, unformatted values) and `--dates iso|serial` handling, plus
//! per-cell number-format introspection for the `format` command.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 输出渲染由快照与命令回归约束，暂不做机械风格重写"
)]

use easyexcel::model::value::format_number_general;
use easyexcel::model::{CellRange, CellValue, DateSystem, Workbook};

/// Output format for grid reads.
#[derive(Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutFormat {
    /// Aligned, human-readable table (default).
    #[default]
    Table,
    /// Comma-separated values.
    Csv,
    /// Tab-separated values.
    Tsv,
    /// A single JSON array (of objects with `--header`, else of arrays).
    Json,
    /// JSON Lines — one JSON record per row.
    Jsonl,
    /// GitHub-flavored Markdown table.
    Md,
}

/// How date-formatted numeric cells are rendered.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DateMode {
    /// ISO 8601 (`2026-03-17` or `2026-03-17 16:05:50`).
    Iso,
    /// The raw Excel serial number.
    Serial,
}

/// Options controlling how cells are read out.
#[derive(Clone, Copy)]
pub struct ReadOpts {
    pub format: OutFormat,
    pub raw: bool,
    pub dates: Option<DateMode>,
    pub header: bool,
}

impl Default for ReadOpts {
    fn default() -> Self {
        ReadOpts {
            format: OutFormat::Table,
            raw: false,
            dates: None,
            header: false,
        }
    }
}

/// True if the cell at (row, col) carries a date/time number format.
pub fn is_date_cell(wb: &Workbook, sheet_idx: usize, row: u32, col: u32) -> bool {
    wb.sheets
        .get(sheet_idx)
        .and_then(|s| s.style_at(row, col))
        .and_then(|si| wb.styles.get(si))
        .map(|st| st.is_date())
        .unwrap_or(false)
}

/// Render an Excel serial as an ISO date or date-time string.
pub(crate) fn serial_to_iso(system: DateSystem, serial: f64) -> String {
    match system.serial_to_datetime(serial) {
        Some(dt) => {
            // Whole-day serials render as a bare date; otherwise include time.
            if serial.fract() == 0.0 {
                dt.format("%Y-%m-%d").to_string()
            } else {
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            }
        }
        None => format_number_general(serial),
    }
}

/// Render a single cell as a plain string for text formats (table/csv/tsv/md).
pub fn cell_string(wb: &Workbook, sheet_idx: usize, row: u32, col: u32, opts: &ReadOpts) -> String {
    let v = wb
        .sheets
        .get(sheet_idx)
        .map(|s| s.value(row, col))
        .unwrap_or(CellValue::Empty);
    match v {
        CellValue::Number(n) => {
            if is_date_cell(wb, sheet_idx, row, col) {
                match opts.dates {
                    Some(DateMode::Iso) => return serial_to_iso(wb.date_system, n),
                    Some(DateMode::Serial) => return format_number_general(n),
                    None if opts.raw => return format_number_general(n),
                    None => {} // fall through to formatted display
                }
            } else if opts.raw {
                return format_number_general(n);
            }
            wb.display_cell(sheet_idx, row, col)
        }
        CellValue::Bool(b) => {
            if opts.raw {
                if b { "true" } else { "false" }.to_string()
            } else if b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        other => other.to_display_string(),
    }
}

/// Escape a string for embedding in a JSON double-quoted literal.
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render a single cell as a JSON value token (number/bool/null/string).
pub fn cell_json(wb: &Workbook, sheet_idx: usize, row: u32, col: u32, opts: &ReadOpts) -> String {
    let v = wb
        .sheets
        .get(sheet_idx)
        .map(|s| s.value(row, col))
        .unwrap_or(CellValue::Empty);
    match v {
        CellValue::Empty => "null".to_string(),
        CellValue::Number(n) => {
            if is_date_cell(wb, sheet_idx, row, col) {
                // JSON dates default to ISO strings unless `--dates serial`.
                match opts.dates {
                    Some(DateMode::Serial) => json_number(n),
                    _ => format!("\"{}\"", json_escape(&serial_to_iso(wb.date_system, n))),
                }
            } else {
                json_number(n)
            }
        }
        CellValue::Bool(b) => b.to_string(),
        CellValue::Text(s) => format!("\"{}\"", json_escape(&s)),
        CellValue::Error(e) => format!("\"{}\"", json_escape(e.as_str())),
    }
}

/// JSON number token; falls back to a string for non-finite values (invalid in JSON).
pub(crate) fn json_number(n: f64) -> String {
    if n.is_finite() {
        format_number_general(n)
    } else {
        format!("\"{}\"", format_number_general(n))
    }
}

/// Render `range` on `sheet_idx` to a string in the requested format.
pub fn render_range(wb: &Workbook, sheet_idx: usize, range: CellRange, opts: &ReadOpts) -> String {
    let r0 = range.start.row;
    let r1 = range.end.row;
    let c0 = range.start.col;
    let c1 = range.end.col;
    let ncols = (c1 - c0 + 1) as usize;

    match opts.format {
        OutFormat::Csv | OutFormat::Tsv => {
            let delim = if opts.format == OutFormat::Csv {
                b','
            } else {
                b'\t'
            };
            let mut wtr = csv::WriterBuilder::new()
                .delimiter(delim)
                .from_writer(Vec::new());
            for r in r0..=r1 {
                let row: Vec<String> = (c0..=c1)
                    .map(|c| cell_string(wb, sheet_idx, r, c, opts))
                    .collect();
                wtr.write_record(&row).expect("write csv record");
            }
            let bytes = wtr.into_inner().expect("flush csv");
            String::from_utf8(bytes)
                .expect("utf8 csv")
                .trim_end()
                .to_string()
        }
        OutFormat::Json => {
            let mut rows: Vec<String> = Vec::new();
            let header = if opts.header {
                Some(header_names(wb, sheet_idx, r0, c0, c1))
            } else {
                None
            };
            let body_start = if opts.header { r0 + 1 } else { r0 };
            for r in body_start..=r1 {
                rows.push(json_row(wb, sheet_idx, r, c0, c1, header.as_deref(), opts));
            }
            format!("[{}]", rows.join(","))
        }
        OutFormat::Jsonl => {
            let header = if opts.header {
                Some(header_names(wb, sheet_idx, r0, c0, c1))
            } else {
                None
            };
            let body_start = if opts.header { r0 + 1 } else { r0 };
            let mut lines: Vec<String> = Vec::new();
            for r in body_start..=r1 {
                lines.push(json_row(wb, sheet_idx, r, c0, c1, header.as_deref(), opts));
            }
            lines.join("\n")
        }
        OutFormat::Md => {
            let header = if opts.header {
                header_names(wb, sheet_idx, r0, c0, c1)
            } else {
                (c0..=c1)
                    .map(easyexcel::model::addr::col_index_to_letters)
                    .collect()
            };
            let body_start = if opts.header { r0 + 1 } else { r0 };
            let mut out = String::new();
            out.push_str(&format!("| {} |\n", header.join(" | ")));
            out.push_str(&format!("| {} |\n", vec!["---"; ncols].join(" | ")));
            for r in body_start..=r1 {
                let cells: Vec<String> = (c0..=c1)
                    .map(|c| cell_string(wb, sheet_idx, r, c, opts).replace('|', "\\|"))
                    .collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
            out.trim_end().to_string()
        }
        OutFormat::Table => {
            // Compute the textual grid then pad columns to equal width.
            let mut grid: Vec<Vec<String>> = Vec::new();
            for r in r0..=r1 {
                grid.push(
                    (c0..=c1)
                        .map(|c| cell_string(wb, sheet_idx, r, c, opts))
                        .collect(),
                );
            }
            let mut widths = vec![0usize; ncols];
            for row in &grid {
                for (i, cell) in row.iter().enumerate() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            for row in &grid {
                let line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        let pad = widths[i] - cell.chars().count();
                        format!("{cell}{}", " ".repeat(pad))
                    })
                    .collect();
                out.push_str(line.join("  ").trim_end());
                out.push('\n');
            }
            out.trim_end().to_string()
        }
    }
}

/// Render a flat row-major slice of scalar formula values as a grid (used for
/// array-returning `eval`). Honors the text/json/md formats; `header` is not
/// applied (array results have no header row).
pub fn render_value_grid(
    data: &[easyexcel::formula::Value],
    rows: usize,
    cols: usize,
    opts: &ReadOpts,
) -> String {
    use easyexcel::formula::Value;

    let at = |r: usize, c: usize| -> &Value { &data[r * cols + c] };
    let as_string = |v: &Value| -> String {
        match v {
            Value::Empty => String::new(),
            Value::Number(n) => format_number_general(*n),
            Value::Text(s) => s.clone(),
            Value::Bool(b) => {
                if opts.raw {
                    if *b { "true" } else { "false" }.to_string()
                } else if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Value::Error(e) => e.as_str().to_string(),
            // Nested arrays/refs shouldn't appear in a materialized result.
            _ => String::new(),
        }
    };
    let as_json = |v: &Value| -> String {
        match v {
            Value::Empty => "null".to_string(),
            Value::Number(n) => json_number(*n),
            Value::Text(s) => format!("\"{}\"", json_escape(s)),
            Value::Bool(b) => b.to_string(),
            Value::Error(e) => format!("\"{}\"", json_escape(e.as_str())),
            _ => "null".to_string(),
        }
    };

    match opts.format {
        OutFormat::Json => {
            let mut out = Vec::new();
            for r in 0..rows {
                let row: Vec<String> = (0..cols).map(|c| as_json(at(r, c))).collect();
                out.push(format!("[{}]", row.join(",")));
            }
            format!("[{}]", out.join(","))
        }
        OutFormat::Jsonl => {
            let mut out = Vec::new();
            for r in 0..rows {
                let row: Vec<String> = (0..cols).map(|c| as_json(at(r, c))).collect();
                out.push(format!("[{}]", row.join(",")));
            }
            out.join("\n")
        }
        OutFormat::Csv | OutFormat::Tsv => {
            let delim = if opts.format == OutFormat::Csv {
                b','
            } else {
                b'\t'
            };
            let mut wtr = csv::WriterBuilder::new()
                .delimiter(delim)
                .from_writer(Vec::new());
            for r in 0..rows {
                let row: Vec<String> = (0..cols).map(|c| as_string(at(r, c))).collect();
                wtr.write_record(&row).expect("write csv record");
            }
            let bytes = wtr.into_inner().expect("flush csv");
            String::from_utf8(bytes)
                .expect("utf8 csv")
                .trim_end()
                .to_string()
        }
        OutFormat::Md => {
            let header: Vec<String> = (0..cols as u32)
                .map(easyexcel::model::addr::col_index_to_letters)
                .collect();
            let mut out = String::new();
            out.push_str(&format!("| {} |\n", header.join(" | ")));
            out.push_str(&format!("| {} |\n", vec!["---"; cols].join(" | ")));
            for r in 0..rows {
                let cells: Vec<String> = (0..cols)
                    .map(|c| as_string(at(r, c)).replace('|', "\\|"))
                    .collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
            out.trim_end().to_string()
        }
        OutFormat::Table => {
            let mut grid: Vec<Vec<String>> = Vec::new();
            for r in 0..rows {
                grid.push((0..cols).map(|c| as_string(at(r, c))).collect());
            }
            let mut widths = vec![0usize; cols];
            for row in &grid {
                for (i, cell) in row.iter().enumerate() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
            let mut out = String::new();
            for row in &grid {
                let line: Vec<String> = row
                    .iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        format!("{cell}{}", " ".repeat(widths[i] - cell.chars().count()))
                    })
                    .collect();
                out.push_str(line.join("  ").trim_end());
                out.push('\n');
            }
            out.trim_end().to_string()
        }
    }
}

/// Header names from the header row, falling back to the column letter when empty.
fn header_names(wb: &Workbook, sheet_idx: usize, row: u32, c0: u32, c1: u32) -> Vec<String> {
    (c0..=c1)
        .map(|c| {
            let s = wb.display_cell(sheet_idx, row, c);
            if s.is_empty() {
                easyexcel::model::addr::col_index_to_letters(c)
            } else {
                s
            }
        })
        .collect()
}

/// Render one data row as a JSON object (keyed by `header`) or array.
fn json_row(
    wb: &Workbook,
    sheet_idx: usize,
    row: u32,
    c0: u32,
    c1: u32,
    header: Option<&[String]>,
    opts: &ReadOpts,
) -> String {
    match header {
        Some(names) => {
            let pairs: Vec<String> = (c0..=c1)
                .enumerate()
                .map(|(i, c)| {
                    let key = json_escape(&names[i]);
                    let val = cell_json(wb, sheet_idx, row, c, opts);
                    format!("\"{key}\":{val}")
                })
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
        None => {
            let vals: Vec<String> = (c0..=c1)
                .map(|c| cell_json(wb, sheet_idx, row, c, opts))
                .collect();
            format!("[{}]", vals.join(","))
        }
    }
}

/// Describe a cell's number format (for the `format` command), e.g.
/// `DATE dd/mm/yyyy`, `DATE_TIME dd/mm/yyyy hh:mm:ss`, `NUMBER 0.00`, `GENERAL`.
pub fn describe_number_format(wb: &Workbook, sheet_idx: usize, row: u32, col: u32) -> String {
    let style = wb
        .sheets
        .get(sheet_idx)
        .and_then(|s| s.style_at(row, col))
        .and_then(|si| wb.styles.get(si));
    let Some(style) = style else {
        return "GENERAL".to_string();
    };
    let code = style.number_format.trim();
    if code.is_empty() || code.eq_ignore_ascii_case("general") {
        return "GENERAL".to_string();
    }
    if style.is_date() {
        // DATE_TIME when the code contains time components, else DATE.
        let has_time = code.chars().any(|c| matches!(c, 'h' | 'H' | 's' | 'S'))
            || code.contains("AM/PM")
            || code.contains("am/pm");
        if has_time {
            format!("DATE_TIME {code}")
        } else {
            format!("DATE {code}")
        }
    } else {
        format!("NUMBER {code}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel::model::styles::CellStyle;
    use easyexcel::model::{Cell, Workbook};

    fn wb_grid() -> Workbook {
        let mut wb = Workbook::new();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("name".into()));
        s.set_a1("B1", Cell::Text("amt".into()));
        s.set_a1("A2", Cell::Text("foo".into()));
        s.set_a1("B2", Cell::Number(1234.5));
        s.set_a1("A3", Cell::Text("bar".into()));
        s.set_a1("B3", Cell::Number(6000.0));
        wb
    }

    fn rng(a: &str) -> CellRange {
        CellRange::parse_a1(a).unwrap()
    }

    #[test]
    fn csv_render() {
        let wb = wb_grid();
        let out = render_range(
            &wb,
            0,
            rng("A1:B3"),
            &ReadOpts::default().with(OutFormat::Csv),
        );
        assert_eq!(out, "name,amt\nfoo,1234.5\nbar,6000");
    }

    #[test]
    fn json_records_with_header() {
        let wb = wb_grid();
        let opts = ReadOpts {
            format: OutFormat::Json,
            header: true,
            ..Default::default()
        };
        let out = render_range(&wb, 0, rng("A1:B3"), &opts);
        assert_eq!(
            out,
            r#"[{"name":"foo","amt":1234.5},{"name":"bar","amt":6000}]"#
        );
    }

    #[test]
    fn markdown_table() {
        let wb = wb_grid();
        let opts = ReadOpts {
            format: OutFormat::Md,
            header: true,
            ..Default::default()
        };
        let out = render_range(&wb, 0, rng("A1:B3"), &opts);
        assert!(out.starts_with("| name | amt |\n| --- | --- |\n| foo |"));
    }

    #[test]
    fn raw_number_no_separators() {
        let mut wb = Workbook::new();
        let idx = {
            let st = CellStyle {
                number_format: "#,##0.00".into(),
                ..Default::default()
            };
            wb.styles.intern(st)
        };
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Number(1234567.5));
        s.set_style(0, 0, idx);
        // Formatted display has separators; raw does not.
        assert_eq!(
            cell_string(&wb, 0, 0, 0, &ReadOpts::default()),
            "1,234,567.50"
        );
        assert_eq!(
            cell_string(&wb, 0, 0, 0, &ReadOpts::default().with_raw()),
            "1234567.5"
        );
    }

    #[test]
    fn date_iso_and_serial() {
        let mut wb = Workbook::new();
        let idx = {
            let st = CellStyle {
                number_format: "yyyy-mm-dd".into(),
                ..Default::default()
            };
            wb.styles.intern(st)
        };
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Number(45000.0));
        s.set_style(0, 0, idx);
        let iso = cell_string(
            &wb,
            0,
            0,
            0,
            &ReadOpts {
                dates: Some(DateMode::Iso),
                ..Default::default()
            },
        );
        assert_eq!(iso, "2023-03-15");
        let serial = cell_string(
            &wb,
            0,
            0,
            0,
            &ReadOpts {
                dates: Some(DateMode::Serial),
                ..Default::default()
            },
        );
        assert_eq!(serial, "45000");
    }

    #[test]
    fn describe_formats() {
        let mut wb = Workbook::new();
        let date_idx = {
            let st = CellStyle {
                number_format: "dd/mm/yyyy".into(),
                ..Default::default()
            };
            wb.styles.intern(st)
        };
        let num_idx = {
            let st = CellStyle {
                number_format: "0.00".into(),
                ..Default::default()
            };
            wb.styles.intern(st)
        };
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Number(45000.0));
        s.set_style(0, 0, date_idx);
        s.set_a1("A2", Cell::Number(3.5));
        s.set_style(1, 0, num_idx);
        assert_eq!(describe_number_format(&wb, 0, 0, 0), "DATE dd/mm/yyyy");
        assert_eq!(describe_number_format(&wb, 0, 1, 0), "NUMBER 0.00");
        assert_eq!(describe_number_format(&wb, 0, 5, 5), "GENERAL");
    }

    // Small builder helpers for terse tests.
    impl ReadOpts {
        fn with(mut self, f: OutFormat) -> Self {
            self.format = f;
            self
        }
        fn with_raw(mut self) -> Self {
            self.raw = true;
            self
        }
    }
}
