//! 旧 `xls` CLI 源码到 `EasyExcel` 细粒度组件的窄适配层。
//!
//! 该模块不复制工作簿业务逻辑，只把原先集中在 `xls::core` 的公开路径映射到
//! `easyexcel-model`、`easyexcel-formula`、`easyexcel-io` 及格式 crate。这样旧
//! 命令实现和注释可以原样迁入，同时生产依赖链不再指向旧 `xls` fork。

use std::io::Read;
use std::path::Path;

pub(crate) use easyexcel::io::{Error, Format, Result};
pub(crate) use easyexcel::model::{CellAddress, CellRange, DateSystem, Sheet, Workbook};

pub(crate) mod addr {
    pub(crate) use easyexcel::model::addr::{col_index_to_letters, col_letters_to_index};
}

pub(crate) mod csv {
    pub(crate) use easyexcel::csv::{CsvReadOptions, read_csv};
    #[cfg(test)]
    pub(crate) use easyexcel::csv::{CsvWriteOptions, write_csv};
}

pub(crate) mod dates {
    #[cfg(test)]
    pub(crate) use easyexcel::model::dates::ymd_to_serial;
    pub(crate) use easyexcel::model::dates::{looks_like_date, parse_text_date};
}

pub(crate) mod formula {
    pub(crate) use easyexcel_formula::{CellRef, Engine};

    pub(crate) mod coerce {
        pub(crate) use easyexcel_formula::formula::coerce::parse_number_text;
    }

    pub(crate) mod value {
        pub(crate) use easyexcel_formula::Value;
    }
}

pub(crate) mod model {
    pub(crate) use easyexcel_model::model::{Cell, DefinedName, Table, Workbook};
}

pub(crate) mod query {
    pub(crate) use super::super::query::run_query;
}

pub(crate) mod styles {
    pub(crate) use easyexcel::model::styles::{Color, FillPattern};
}

pub(crate) mod value {
    pub(crate) use easyexcel::model::CellValue;
    pub(crate) use easyexcel::model::value::format_number_general;
}

/// 判断路径是否支持恒定行缓冲的流式读取。
pub(crate) mod stream {
    use std::io::Read;
    use std::path::Path;

    use easyexcel::io::{Error, Format, Result};
    pub(crate) use easyexcel::io::{RowSink, StreamCell, StreamInfo};
    use easyexcel::model::{CellValue, DateSystem};

    /// `.xlsx`/`.xlsm` 与 CSV 家族支持逐行读取。
    pub(crate) fn is_streamable(path: &Path) -> bool {
        matches!(Format::from_path(path), Some(Format::Xlsx | Format::Csv))
    }

    /// 将一个工作表逐行推送给消费者，不构造完整工作簿。
    pub(crate) fn stream_path<S: RowSink>(
        path: &Path,
        sheet: Option<&str>,
        sink: &mut S,
    ) -> Result<()> {
        match Format::from_path(path) {
            Some(Format::Xlsx) => {
                let file = std::fs::File::open(path)?;
                easyexcel_xlsx::stream(file, sheet, sink)
            }
            Some(Format::Csv) => {
                let file = std::fs::File::open(path)?;
                stream_csv(file, sink)
            }
            Some(_) | None => Err(Error::Unsupported(format!(
                "{} cannot be streamed; use the in-memory reader",
                path.display()
            ))),
        }
    }

    /// 逐条读取分隔文本；仅保留分隔符探测样本和当前记录。
    fn stream_csv<R: Read, S: RowSink>(mut reader: R, sink: &mut S) -> Result<()> {
        let mut head = vec![0_u8; 64 * 1024];
        let count = reader.read(&mut head)?;
        head.truncate(count);
        let sample = easyexcel::csv::decode_bytes(&head);
        let delimiter = easyexcel::csv::detect_delimiter(&sample);
        let full = Read::chain(std::io::Cursor::new(head), reader);

        sink.begin(&StreamInfo {
            sheet_name: "Sheet1".to_owned(),
            date_system: DateSystem::Date1900,
        })?;

        let mut csv_reader = ::csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(false)
            .flexible(true)
            .from_reader(full);
        let mut record = ::csv::StringRecord::new();
        let mut row_index = 0_u32;
        let mut cells = Vec::new();
        while csv_reader.read_record(&mut record).map_err(Error::from)? {
            cells.clear();
            for (column, field) in record.iter().enumerate() {
                let value = easyexcel::csv::infer_cell(field).value();
                if !matches!(value, CellValue::Empty) {
                    cells.push(StreamCell {
                        col: u32::try_from(column).map_err(|_| {
                            Error::Other("CSV column index exceeds the supported range".to_owned())
                        })?,
                        value,
                        number_format: String::new(),
                    });
                }
            }
            if !cells.is_empty() {
                sink.row(row_index, &cells)?;
            }
            row_index = row_index.checked_add(1).ok_or_else(|| {
                Error::Other("CSV row index exceeds the supported range".to_owned())
            })?;
        }
        sink.end()
    }
}

/// 打开工作簿，并在需要时解密受密码保护的 XLSX。
pub(crate) fn open_path_with_password(path: &Path, password: Option<&str>) -> Result<Workbook> {
    match Format::from_path(path).unwrap_or_else(|| sniff_format(path).unwrap_or(Format::Csv)) {
        Format::Xlsx => easyexcel_xlsx::read_path_with_password(path, password),
        Format::Xls => easyexcel_xls::read_path(path),
        Format::Csv => {
            let file = std::fs::File::open(path)?;
            let sheet_name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Sheet1")
                .to_owned();
            easyexcel::csv::read_csv(
                file,
                &easyexcel::csv::CsvReadOptions {
                    sheet_name,
                    ..Default::default()
                },
            )
        }
        _ => Err(Error::Unsupported("unsupported input format".to_owned())),
    }
}

/// 打开未加密或无需显式密码的工作簿。
pub(crate) fn open_path(path: &Path) -> Result<Workbook> {
    open_path_with_password(path, None)
}

/// 根据扩展名保存 XLS、XLSX 或 CSV。
pub(crate) fn save_path(workbook: &Workbook, path: &Path) -> Result<()> {
    match Format::from_path(path).ok_or_else(|| {
        Error::Unsupported(format!("cannot determine format for {}", path.display()))
    })? {
        Format::Xlsx => easyexcel_xlsx::write_path(workbook, path),
        Format::Xls => easyexcel_xls::write_path(workbook, path),
        Format::Csv => {
            let file = std::fs::File::create(path)?;
            easyexcel::csv::write_csv(
                workbook,
                workbook
                    .active_sheet
                    .min(workbook.sheets.len().saturating_sub(1)),
                file,
                &easyexcel::csv::CsvWriteOptions::default(),
            )
        }
        _ => Err(Error::Unsupported("unsupported output format".to_owned())),
    }
}

fn sniff_format(path: &Path) -> Result<Format> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0_u8; 8];
    let count = file.read(&mut magic)?;
    Ok(Format::from_magic(&magic[..count]))
}
