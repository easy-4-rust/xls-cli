//! Streaming export sinks: turn the core row stream into CSV/TSV/JSONL on the
//! fly, so huge files never load fully into memory. Output matches the
//! in-memory renderer cell-for-cell (minus formula recalculation).

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 流式 sink 保持输出字节语义，暂不做机械风格重写"
)]

use std::io::Write;

use easyexcel::io::{RowSink, StreamCell, StreamInfo};
use easyexcel::model::value::format_number_general;
use easyexcel::model::{CellValue, DateSystem};

use super::render::{self, DateMode, OutFormat, ReadOpts};

/// Whether `--stream` can serve this output format. The row-oriented formats
/// (csv/tsv/jsonl) stream cleanly; table/json/md need the full grid for
/// alignment or a single enclosing array, so they fall back to the in-memory
/// renderer.
pub fn format_is_streamable(fmt: OutFormat) -> bool {
    matches!(fmt, OutFormat::Csv | OutFormat::Tsv | OutFormat::Jsonl)
}

/// Render one streamed cell as plain text, mirroring `render::cell_string`.
fn cell_text(c: &StreamCell, opts: &ReadOpts, date_system: DateSystem) -> String {
    match &c.value {
        CellValue::Number(n) => {
            let is_date = !c.number_format.is_empty()
                && easyexcel::model::numfmt::is_date_format(&c.number_format);
            if is_date {
                match opts.dates {
                    Some(DateMode::Iso) => return render::serial_to_iso(date_system, *n),
                    Some(DateMode::Serial) => return format_number_general(*n),
                    None if opts.raw => return format_number_general(*n),
                    None => return c.display(date_system),
                }
            }
            if opts.raw {
                format_number_general(*n)
            } else {
                c.display(date_system)
            }
        }
        CellValue::Bool(b) => {
            if opts.raw {
                if *b { "true" } else { "false" }.to_string()
            } else if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        other => other.to_display_string(),
    }
}

/// Render one streamed cell as a JSON value token, mirroring `render::cell_json`.
fn cell_json(c: &StreamCell, opts: &ReadOpts, date_system: DateSystem) -> String {
    match &c.value {
        CellValue::Empty => "null".to_string(),
        CellValue::Number(n) => {
            let is_date = !c.number_format.is_empty()
                && easyexcel::model::numfmt::is_date_format(&c.number_format);
            if is_date {
                match opts.dates {
                    Some(DateMode::Serial) => render::json_number(*n),
                    _ => format!(
                        "\"{}\"",
                        render::json_escape(&render::serial_to_iso(date_system, *n))
                    ),
                }
            } else {
                render::json_number(*n)
            }
        }
        CellValue::Bool(b) => b.to_string(),
        CellValue::Text(s) => format!("\"{}\"", render::json_escape(s)),
        CellValue::Error(e) => format!("\"{}\"", render::json_escape(e.as_str())),
    }
}

/// Highest 0-based column index present in a (column-sorted) row.
fn max_col(cells: &[StreamCell]) -> u32 {
    cells.last().map(|c| c.col).unwrap_or(0)
}

/// CSV/TSV streaming sink.
pub struct CsvSink<W: Write> {
    wtr: csv::Writer<W>,
    opts: ReadOpts,
    date_system: DateSystem,
}

impl<W: Write> CsvSink<W> {
    pub fn new(writer: W, opts: ReadOpts) -> Self {
        let delim = if opts.format == OutFormat::Tsv {
            b'\t'
        } else {
            b','
        };
        let wtr = csv::WriterBuilder::new()
            .delimiter(delim)
            .flexible(true)
            .from_writer(writer);
        CsvSink {
            wtr,
            opts,
            date_system: DateSystem::Date1900,
        }
    }
}

impl<W: Write> RowSink for CsvSink<W> {
    fn begin(&mut self, info: &StreamInfo) -> easyexcel::io::Result<()> {
        self.date_system = info.date_system;
        Ok(())
    }

    fn row(&mut self, _row: u32, cells: &[StreamCell]) -> easyexcel::io::Result<()> {
        let n = max_col(cells) as usize + 1;
        let mut record: Vec<String> = vec![String::new(); n];
        for c in cells {
            record[c.col as usize] = cell_text(c, &self.opts, self.date_system);
        }
        // Trim trailing empties for compactness (matches the CSV writer).
        while record.last().map(|s| s.is_empty()).unwrap_or(false) {
            record.pop();
        }
        self.wtr
            .write_record(&record)
            .map_err(easyexcel::io::Error::from)?;
        Ok(())
    }

    fn end(&mut self) -> easyexcel::io::Result<()> {
        self.wtr.flush().map_err(easyexcel::io::Error::from)?;
        Ok(())
    }
}

/// JSON Lines streaming sink (one record per row).
pub struct JsonlSink<W: Write> {
    out: W,
    opts: ReadOpts,
    date_system: DateSystem,
    /// Header names captured from the first row when `--header` is set.
    header: Option<Vec<String>>,
    awaiting_header: bool,
}

impl<W: Write> JsonlSink<W> {
    pub fn new(writer: W, opts: ReadOpts) -> Self {
        let awaiting_header = opts.header;
        JsonlSink {
            out: writer,
            opts,
            date_system: DateSystem::Date1900,
            header: None,
            awaiting_header,
        }
    }

    /// Dense header names over columns `0..=max_col`, blanks → column letters.
    fn capture_header(&self, cells: &[StreamCell]) -> Vec<String> {
        let n = max_col(cells) as usize + 1;
        let mut names: Vec<String> = (0..n as u32)
            .map(easyexcel::model::addr::col_index_to_letters)
            .collect();
        for c in cells {
            let s = cell_text(c, &self.opts, self.date_system);
            if !s.is_empty() {
                names[c.col as usize] = s;
            }
        }
        names
    }
}

impl<W: Write> RowSink for JsonlSink<W> {
    fn begin(&mut self, info: &StreamInfo) -> easyexcel::io::Result<()> {
        self.date_system = info.date_system;
        Ok(())
    }

    fn row(&mut self, _row: u32, cells: &[StreamCell]) -> easyexcel::io::Result<()> {
        if self.awaiting_header {
            self.header = Some(self.capture_header(cells));
            self.awaiting_header = false;
            return Ok(());
        }
        let line = match &self.header {
            Some(names) => {
                // Object keyed by header position; missing cells → null.
                let mut by_col: Vec<Option<&StreamCell>> = vec![None; names.len()];
                for c in cells {
                    if (c.col as usize) < by_col.len() {
                        by_col[c.col as usize] = Some(c);
                    }
                }
                let pairs: Vec<String> = names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let val = by_col[i]
                            .map(|c| cell_json(c, &self.opts, self.date_system))
                            .unwrap_or_else(|| "null".to_string());
                        format!("\"{}\":{}", render::json_escape(name), val)
                    })
                    .collect();
                format!("{{{}}}", pairs.join(","))
            }
            None => {
                let n = max_col(cells) as usize + 1;
                let mut vals: Vec<String> = vec!["null".to_string(); n];
                for c in cells {
                    vals[c.col as usize] = cell_json(c, &self.opts, self.date_system);
                }
                format!("[{}]", vals.join(","))
            }
        };
        self.out
            .write_all(line.as_bytes())
            .map_err(easyexcel::io::Error::from)?;
        self.out
            .write_all(b"\n")
            .map_err(easyexcel::io::Error::from)?;
        Ok(())
    }
}
