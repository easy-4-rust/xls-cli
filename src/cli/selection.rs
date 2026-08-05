use easyexcel::model::{CellRange, CellValue, Workbook};
use serde_json::{Value, json};

use crate::{CommandError, ErrorCode, OutputFormat};

pub(crate) struct Selection {
    pub(crate) sheet_index: usize,
    pub(crate) range: CellRange,
}

pub(crate) fn resolve_selection(
    workbook: &Workbook,
    specification: Option<&str>,
    default_sheet: Option<&str>,
) -> Result<Selection, CommandError> {
    let (sheet_name, range_text) = specification.map_or((default_sheet, None), |specification| {
        specification
            .rsplit_once('!')
            .map_or((default_sheet, Some(specification)), |(sheet, range)| {
                (Some(sheet.trim_matches('\'')), Some(range))
            })
    });
    let sheet_index = match sheet_name {
        Some(name) => workbook.sheet_index(name).ok_or_else(|| {
            CommandError::new(ErrorCode::SheetNotFound, format!("工作表不存在：{name}"))
        })?,
        None => 0,
    };
    let sheet = workbook
        .sheets
        .get(sheet_index)
        .ok_or_else(|| CommandError::new(ErrorCode::SheetNotFound, "工作簿中没有可用工作表"))?;
    let range = if let Some(text) = range_text {
        CellRange::parse_a1(text).ok_or_else(|| {
            CommandError::new(
                ErrorCode::InvalidArgument,
                format!("无效的 A1 范围：{text}"),
            )
        })?
    } else {
        let (rows, columns) = sheet.dimensions();
        if rows == 0 || columns == 0 {
            CellRange::parse_a1("A1").expect("valid constant range")
        } else {
            CellRange::new(
                easyexcel::model::CellAddress::new(0, 0),
                easyexcel::model::CellAddress::new(rows - 1, columns - 1),
            )
        }
    };
    Ok(Selection { sheet_index, range })
}

pub(crate) fn rows_json(workbook: &Workbook, selection: &Selection) -> Vec<Vec<Value>> {
    let sheet = &workbook.sheets[selection.sheet_index];
    (selection.range.start.row..=selection.range.end.row)
        .map(|row| {
            (selection.range.start.col..=selection.range.end.col)
                .map(|column| cell_value_json(&sheet.value(row, column)))
                .collect()
        })
        .collect()
}

pub(crate) fn render_selection(
    workbook: &Workbook,
    selection: &Selection,
    output_format: OutputFormat,
) -> Value {
    let rows = rows_json(workbook, selection);
    match output_format {
        OutputFormat::Json => json!({
            "sheet": workbook.sheets[selection.sheet_index].name,
            "range": selection.range.to_a1(),
            "rows": rows,
        }),
        OutputFormat::Csv => Value::String(render_delimited(&rows, ',')),
        OutputFormat::Tsv => Value::String(render_delimited(&rows, '\t')),
        OutputFormat::Markdown => Value::String(render_markdown(&rows)),
        OutputFormat::Html => Value::String(render_html(&rows)),
    }
}

pub(crate) fn cell_value_json(value: &CellValue) -> Value {
    match value {
        CellValue::Empty => Value::Null,
        CellValue::Number(number) => {
            serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number)
        }
        CellValue::Text(text) => Value::String(text.clone()),
        CellValue::Bool(value) => Value::Bool(*value),
        CellValue::Error(error) => json!({"error": error.as_str()}),
    }
}

fn render_delimited(rows: &[Vec<Value>], delimiter: char) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(value_text)
                .map(|value| quote_delimited(&value, delimiter))
                .collect::<Vec<_>>()
                .join(&delimiter.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_delimited(value: &str, delimiter: char) -> String {
    if value.contains(delimiter) || value.contains(['\n', '\r', '"']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn render_markdown(rows: &[Vec<Value>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut output = String::new();
    for (index, row) in rows.iter().enumerate() {
        output.push('|');
        for column in 0..columns {
            output.push(' ');
            output
                .push_str(&value_text(row.get(column).unwrap_or(&Value::Null)).replace('|', "\\|"));
            output.push_str(" |");
        }
        output.push('\n');
        if index == 0 {
            output.push('|');
            for _ in 0..columns {
                output.push_str(" --- |");
            }
            output.push('\n');
        }
    }
    output
}

fn render_html(rows: &[Vec<Value>]) -> String {
    let mut output = String::from("<table><tbody>");
    for row in rows {
        output.push_str("<tr>");
        for value in row {
            output.push_str("<td>");
            output.push_str(&escape_html(&value_text(value)));
            output.push_str("</td>");
        }
        output.push_str("</tr>");
    }
    output.push_str("</tbody></table>");
    output
}

fn value_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
