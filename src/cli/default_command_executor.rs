use std::fs;
use std::path::{Path, PathBuf};

use easyexcel::formula::Engine;
use easyexcel::markdown::{
    MarkdownConversionReport, MarkdownExportOptions, MarkdownImportOptions, MarkdownWarningCode,
};
use easyexcel::model::{CellRange, Workbook};
use easyexcel::tabular::{TabularDocument, TabularFormat};
use serde_json::{Value, json};

use crate::cli::query::run_query;
use crate::cli::selection::{cell_value_json, render_selection, resolve_selection};
use crate::cli::workbook_io::{
    detect_tabular_format, export_markdown, import_markdown, mutation_target, open_workbook,
    save_workbook, write_text,
};
use crate::{
    CapabilityManifest, CommandError, CommandExecutor, CommandName, CommandRequest, CommandResult,
    CommandWarning, ErrorCode, ExecutionContext, ExecutionMode, GeneratedFile, OutputFormat,
    command_schema,
};

/// 无状态的默认命令执行器。
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultCommandExecutor;

impl DefaultCommandExecutor {
    /// 创建默认执行器。
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CommandExecutor for DefaultCommandExecutor {
    #[allow(
        clippy::too_many_lines,
        reason = "命令分派集中维护请求枚举的穷尽匹配，拆散会削弱协议审计性"
    )]
    fn execute(
        &self,
        request: CommandRequest,
        context: &ExecutionContext,
    ) -> Result<CommandResult, CommandError> {
        let command = request.command_name();
        match request {
            CommandRequest::Info { input } => info(&input, context),
            CommandRequest::Get {
                input,
                range,
                output_format,
            } => extract(&input, range.as_deref(), None, output_format, context),
            CommandRequest::Head {
                input,
                sheet,
                rows,
                output_format,
            } => head_or_tail(
                &input,
                sheet.as_deref(),
                rows,
                false,
                output_format,
                context,
            ),
            CommandRequest::Tail {
                input,
                sheet,
                rows,
                output_format,
            } => head_or_tail(&input, sheet.as_deref(), rows, true, output_format, context),
            CommandRequest::Set {
                input,
                cell,
                value,
                output,
            } => mutate(&input, output, context, CommandName::Set, |workbook| {
                let selection = resolve_selection(workbook, Some(&cell), None)?;
                if selection.range.rows() != 1 || selection.range.cols() != 1 {
                    return Err(CommandError::new(
                        ErrorCode::InvalidArgument,
                        "set 只接受单个单元格地址",
                    ));
                }
                workbook.sheets[selection.sheet_index].set(
                    selection.range.start.row,
                    selection.range.start.col,
                    value.into_cell(),
                );
                Ok(json!({"cell": cell}))
            }),
            CommandRequest::Clear {
                input,
                range,
                output,
            } => mutate(&input, output, context, CommandName::Clear, |workbook| {
                let selection = resolve_selection(workbook, Some(&range), None)?;
                workbook.sheets[selection.sheet_index].clear_range(selection.range);
                Ok(json!({"range": range}))
            }),
            CommandRequest::Fill {
                input,
                range,
                value,
                output,
            } => mutate(&input, output, context, CommandName::Fill, |workbook| {
                let selection = resolve_selection(workbook, Some(&range), None)?;
                for (row, column) in selection.range.iter_cells() {
                    workbook.sheets[selection.sheet_index].set(
                        row,
                        column,
                        value.clone().into_cell(),
                    );
                }
                Ok(json!({
                    "range": range,
                    "cells": u64::from(selection.range.rows()) * u64::from(selection.range.cols()),
                }))
            }),
            CommandRequest::InsertRows {
                input,
                sheet,
                at,
                count,
                output,
            } => mutate_axis(
                &input,
                output,
                context,
                command,
                sheet.as_deref(),
                at,
                count,
                true,
                true,
            ),
            CommandRequest::DeleteRows {
                input,
                sheet,
                at,
                count,
                output,
            } => mutate_axis(
                &input,
                output,
                context,
                command,
                sheet.as_deref(),
                at,
                count,
                true,
                false,
            ),
            CommandRequest::InsertColumns {
                input,
                sheet,
                at,
                count,
                output,
            } => mutate_axis(
                &input,
                output,
                context,
                command,
                sheet.as_deref(),
                at,
                count,
                false,
                true,
            ),
            CommandRequest::DeleteColumns {
                input,
                sheet,
                at,
                count,
                output,
            } => mutate_axis(
                &input,
                output,
                context,
                command,
                sheet.as_deref(),
                at,
                count,
                false,
                false,
            ),
            CommandRequest::New { output, sheets } => new_workbook(&output, &sheets, context),
            CommandRequest::AddSheet {
                input,
                name,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                validate_sheet_name(workbook, &name, None)?;
                workbook.add_sheet(&name);
                Ok(json!({"sheet": name}))
            }),
            CommandRequest::DeleteSheet {
                input,
                name,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                if workbook.sheets.len() == 1 {
                    return Err(CommandError::new(
                        ErrorCode::InvalidArgument,
                        "不能删除工作簿中的最后一个工作表",
                    ));
                }
                let index = workbook.sheet_index(&name).ok_or_else(|| {
                    CommandError::new(ErrorCode::SheetNotFound, format!("工作表不存在：{name}"))
                })?;
                workbook.sheets.remove(index);
                Ok(json!({"sheet": name}))
            }),
            CommandRequest::RenameSheet {
                input,
                name,
                new_name,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                let index = workbook.sheet_index(&name).ok_or_else(|| {
                    CommandError::new(ErrorCode::SheetNotFound, format!("工作表不存在：{name}"))
                })?;
                validate_sheet_name(workbook, &new_name, Some(index))?;
                workbook.sheets[index].name.clone_from(&new_name);
                Ok(json!({"old_name": name, "new_name": new_name}))
            }),
            CommandRequest::Query { input, sql } => query(&input, &sql, context),
            CommandRequest::Convert { input, output } => convert(&input, &output, context),
            CommandRequest::Import {
                input,
                output,
                markdown_options,
            } => import(&input, &output, markdown_options, context),
            CommandRequest::Export {
                input,
                output,
                output_format,
                markdown_options,
            } => export(&input, &output, output_format, markdown_options, context),
            CommandRequest::Recalc { input, output } => {
                mutate(&input, output, context, command, |workbook| {
                    let report = Engine::new().recalc(workbook);
                    Ok(json!({
                        "evaluated": report.evaluated,
                        "circular": report.circular,
                    }))
                })
            }
            CommandRequest::Capabilities => {
                let data = serde_json::to_value(CapabilityManifest::current())
                    .map_err(internal_serialization_error)?;
                Ok(CommandResult::new(command, data, is_dry_run(context)))
            }
            CommandRequest::Schema { target } => Ok(CommandResult::new(
                CommandName::Schema,
                command_schema(target),
                is_dry_run(context),
            )),
            CommandRequest::Grep {
                input,
                pattern,
                sheet,
            } => grep(&input, &pattern, sheet.as_deref(), context),
            CommandRequest::Profile {
                input,
                column,
                sheet,
            } => profile(&input, &column, sheet.as_deref(), context),
            CommandRequest::Eval {
                input,
                formula,
                at,
            } => eval(&input, &formula, at.as_deref(), context),
            CommandRequest::Format { input, cell } => format_cell(&input, &cell, context),
            CommandRequest::Filter {
                input,
                predicate,
                sheet,
            } => filter(&input, &predicate, sheet.as_deref(), context),
            CommandRequest::Sort {
                input,
                by,
                desc,
                sheet,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                sort_workbook(workbook, &by, desc, sheet.as_deref())
            }),
            CommandRequest::Dedup {
                input,
                on,
                sheet,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                dedup_workbook(workbook, &on, sheet.as_deref())
            }),
            CommandRequest::Pivot {
                input,
                rows,
                values,
                agg,
                sheet,
            } => pivot(&input, &rows, &values, agg, sheet.as_deref(), context),
            CommandRequest::Append {
                input,
                with,
                sheet,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                let addition = open_workbook(&with, context)?;
                append_workbook(workbook, &addition, sheet.as_deref())
            }),
            CommandRequest::Join { input, with, on } => join(&input, &with, &on, context),
            CommandRequest::Diff {
                input,
                with,
                key,
                sheet,
            } => diff(&input, &with, key.as_deref(), sheet.as_deref(), context),
            CommandRequest::Copy {
                input,
                source,
                target,
                sheet,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                copy_move_workbook(workbook, &source, &target, sheet.as_deref(), false)
            }),
            CommandRequest::Move {
                input,
                source,
                target,
                sheet,
                output,
            } => mutate(&input, output, context, command, |workbook| {
                copy_move_workbook(workbook, &source, &target, sheet.as_deref(), true)
            }),
            CommandRequest::Planned { command_name, .. } => Err(CommandError::new(
                ErrorCode::UnsupportedCommand,
                format!("当前版本尚不支持命令：{}", command_name.as_str()),
            )),
        }
    }
}

/// 解析目标工作表：指定名称时要求存在；未指定时沿用活跃表（与终端 `grep` 一致）。
fn resolve_sheet_index(
    workbook: &Workbook,
    sheet: Option<&str>,
) -> Result<usize, CommandError> {
    match sheet {
        Some(name) => workbook.sheet_index(name).ok_or_else(|| {
            CommandError::new(ErrorCode::SheetNotFound, format!("工作表不存在：{name}"))
        }),
        None => Ok(workbook
            .active_sheet
            .min(workbook.sheets.len().saturating_sub(1))),
    }
}

/// 在工作簿显示值中做大小写不敏感的子串搜索，返回命中单元格清单。
fn grep(
    path: &Path,
    pattern: &str,
    sheet: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let index = resolve_sheet_index(&workbook, sheet)?;
    let sheet = &workbook.sheets[index];
    let (rows, columns) = sheet.dimensions();
    let needle = pattern.to_lowercase();
    let sheet_name = sheet.name.clone();
    let mut matches = Vec::new();
    for row in 0..rows {
        for column in 0..columns {
            let value = workbook.display_cell(index, row, column);
            if !value.is_empty() && value.to_lowercase().contains(&needle) {
                matches.push(json!({
                    "sheet": sheet_name,
                    "address": format!(
                        "{}{}",
                        easyexcel::model::addr::col_index_to_letters(column),
                        row + 1
                    ),
                    "value": value,
                }));
            }
        }
    }
    let hit_count = matches.len();
    let data = json!({
        "pattern": pattern,
        "sheet": sheet_name,
        "matches": matches,
    });
    let mut result = CommandResult::new(CommandName::Grep, data, is_dry_run(context));
    result.stats.insert("matches".to_owned(), hit_count as u64);
    Ok(result)
}

/// 解析列规格：表头名（第 0 行，大小写不敏感）优先，其次列字母（如 `H` 或 `H:H`）。
fn resolve_column(workbook: &Workbook, sheet_index: usize, specification: &str) -> Result<u32, CommandError> {
    let trimmed = specification.trim();
    let header = (0..workbook.sheets[sheet_index].dimensions().1)
        .find(|&column| {
            workbook
                .display_cell(sheet_index, 0, column)
                .eq_ignore_ascii_case(trimmed)
        });
    if let Some(column) = header {
        return Ok(column);
    }
    let letters = trimmed.split(':').next().unwrap_or(trimmed);
    if !letters.is_empty() && letters.chars().all(|c| c.is_ascii_alphabetic()) {
        let upper = letters.to_ascii_uppercase();
        if let Some(column) = easyexcel::model::addr::col_letters_to_index(&upper) {
            return Ok(column);
        }
    }
    Err(CommandError::new(
        ErrorCode::InvalidArgument,
        format!("列不存在（既不是表头名也不是列字母）：{specification}"),
    ))
}

/// 统计一列的数据概况，并对“数字/日期存为文本”给出稳定警告码。
fn profile(
    path: &Path,
    column: &str,
    sheet: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let index = resolve_sheet_index(&workbook, sheet)?;
    let column = resolve_column(&workbook, index, column)?;
    let rows = workbook.sheets[index].dimensions().0;
    let label = {
        let header = workbook.display_cell(index, 0, column);
        if header.is_empty() {
            easyexcel::model::addr::col_index_to_letters(column)
        } else {
            header
        }
    };

    let mut count = 0u64;
    let mut nulls = 0u64;
    let mut numeric = 0u64;
    let mut text = 0u64;
    let mut text_numbers = 0u64;
    let mut text_dates = 0u64;
    let mut sum = 0.0_f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut distinct = std::collections::HashSet::new();
    for row in 1..rows {
        let value = workbook.sheets[index].value(row, column);
        match value {
            easyexcel::model::value::CellValue::Empty => {
                nulls += 1;
                continue;
            }
            easyexcel::model::value::CellValue::Number(n) => {
                numeric += 1;
                sum += n;
                min = min.min(n);
                max = max.max(n);
            }
            easyexcel::model::value::CellValue::Text(t) => {
                text += 1;
                if easyexcel::formula::formula::coerce::parse_number_text(&t).is_some() {
                    text_numbers += 1;
                } else if easyexcel::model::dates::looks_like_date(&t) {
                    text_dates += 1;
                }
            }
            _ => {}
        }
        count += 1;
        distinct.insert(workbook.display_cell(index, row, column));
    }

    let mut data = json!({
        "column": label,
        "count": count,
        "nulls": nulls,
        "numeric": numeric,
        "text": text,
        "distinct": distinct.len(),
    });
    if numeric > 0 {
        // 计数远小于 2^52，计数转 f64 不损失电子表格语义。
        #[allow(clippy::cast_precision_loss)]
        let mean = sum / numeric as f64;
        data["sum"] = json!(sum);
        data["mean"] = json!(mean);
        data["min"] = json!(min);
        data["max"] = json!(max);
    }
    let mut result = CommandResult::new(CommandName::Profile, data, is_dry_run(context));
    result.stats.insert("count".to_owned(), count);
    if text_numbers > 0 {
        result.warnings.push(CommandWarning::new(
            "NUMBERS_STORED_AS_TEXT",
            format!("{text_numbers} 个值是文本存储的数字（SUM/AVERAGE 会忽略）——可用 to-number 转换"),
        ));
    }
    if text_dates > 0 {
        result.warnings.push(CommandWarning::new(
            "DATES_STORED_AS_TEXT",
            format!("{text_dates} 个值疑似文本存储的日期（非真实日期）——可用 to-date 转换"),
        ));
    }
    Ok(result)
}

/// 把公式求值结果转成 JSON 标量（复用单元格值的 JSON 映射）。
fn formula_value_json(value: &easyexcel::formula::Value) -> Value {
    cell_value_json(&value.clone().to_cell_value())
}

/// 解析 `[Sheet!]A1` 单元格上下文；sheet 名未命中时回退到第 0 张表（与终端一致）。
fn parse_cell_context(workbook: &Workbook, specification: &str) -> Result<easyexcel::formula::CellRef, CommandError> {
    let (sheet, a1) = specification.rsplit_once('!').map_or((0usize, specification), |(sheet, a1)| {
        (
            workbook
                .sheet_index(sheet.trim_matches('\''))
                .unwrap_or(0),
            a1,
        )
    });
    let address = easyexcel::model::CellAddress::parse_a1(a1).ok_or_else(|| {
        CommandError::new(ErrorCode::InvalidArgument, format!("无效的单元格引用：{a1}"))
    })?;
    Ok(easyexcel::formula::CellRef {
        sheet,
        row: address.row,
        col: address.col,
    })
}

/// 对工作簿数据求值单条公式；先重算保证引用值最新，数组结果以网格返回。
fn eval(
    path: &Path,
    formula: &str,
    at: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let mut workbook = open_workbook(path, context)?;
    Engine::new().recalc(&mut workbook);
    let at_ref = match at {
        Some(specification) => parse_cell_context(&workbook, specification)?,
        None => easyexcel::formula::CellRef {
            sheet: 0,
            row: 0,
            col: 0,
        },
    };
    let value = Engine::new().eval_formula(&workbook, at_ref, formula);
    let mut data = json!({
        "formula": formula,
        "at": format!("{}!{}", workbook.sheets[at_ref.sheet].name, easyexcel::model::addr::col_index_to_letters(at_ref.col) + &(at_ref.row + 1).to_string()),
    });
    match &value {
        easyexcel::formula::Value::Array(array) => {
            let grid = (0..array.rows)
                .map(|row| {
                    (0..array.cols)
                        .map(|column| {
                            formula_value_json(&array.data[row * array.cols + column])
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            data["grid"] = json!(grid);
        }
        easyexcel::formula::Value::Ref(range) => {
            let cols = range.cols() as usize;
            let mut grid: Vec<Vec<Value>> = Vec::new();
            let mut line: Vec<Value> = Vec::with_capacity(cols);
            for (row, column) in range.iter() {
                if line.len() == cols {
                    grid.push(std::mem::take(&mut line));
                }
                let cell = workbook
                    .sheets
                    .get(range.sheet)
                    .map_or(
                        easyexcel::model::value::CellValue::Empty,
                        |sheet| sheet.value(row, column),
                    );
                line.push(cell_value_json(&cell));
            }
            if !line.is_empty() {
                grid.push(line);
            }
            data["grid"] = json!(grid);
        }
        _ => {
            data["value"] = formula_value_json(&value);
        }
    }
    Ok(CommandResult::new(
        CommandName::Eval,
        data,
        is_dry_run(context),
    ))
}

/// 查询单元格的数字格式类别与格式代码（GENERAL / DATE… / NUMBER…）。
fn format_cell(
    path: &Path,
    cell: &str,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let at_ref = parse_cell_context(&workbook, cell)?;
    let description = crate::cli::render::describe_number_format(
        &workbook,
        at_ref.sheet,
        at_ref.row,
        at_ref.col,
    );
    let sheet_name = &workbook.sheets[at_ref.sheet].name;
    let data = json!({
        "cell": cell,
        "at": format!(
            "{sheet_name}!{}{}",
            easyexcel::model::addr::col_index_to_letters(at_ref.col),
            at_ref.row + 1
        ),
        "format": description,
    });
    Ok(CommandResult::new(
        CommandName::Format,
        data,
        is_dry_run(context),
    ))
}

/// 按谓词过滤数据行：表头 + 命中行以 JSON 行集返回（不改文件）。
fn filter(
    path: &Path,
    predicate: &str,
    sheet: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let index = resolve_sheet_index(&workbook, sheet)?;
    let (rows, columns) = workbook.sheets[index].dimensions();
    let predicate = crate::cli::predicate::Predicate::parse(predicate).map_err(|error| {
        CommandError::new(ErrorCode::InvalidArgument, format!("谓词解析失败：{error}"))
    })?;
    let column = resolve_column(&workbook, index, &predicate.col)?;

    let headers = (0..columns)
        .map(|column| {
            let header = workbook.display_cell(index, 0, column);
            if header.is_empty() {
                easyexcel::model::addr::col_index_to_letters(column)
            } else {
                header
            }
        })
        .collect::<Vec<_>>();
    let mut matched = Vec::new();
    for row in 1..rows {
        if predicate.matches(&workbook, index, row, column) {
            let line = (0..columns)
                .map(|column| {
                    let cell = workbook.sheets[index].value(row, column);
                    cell_value_json(&cell)
                })
                .collect::<Vec<_>>();
            matched.push(line);
        }
    }
    let hit_count = matched.len();
    let data = json!({
        "predicate": predicate.col,
        "sheet": workbook.sheets[index].name,
        "columns": headers,
        "rows": matched,
    });
    let mut result = CommandResult::new(CommandName::Filter, data, is_dry_run(context));
    result.stats.insert("rows".to_owned(), hit_count as u64);
    Ok(result)
}

/// 按键列稳定多键排序数据行（表头保留），数值优先比较。
fn sort_workbook(
    workbook: &mut Workbook,
    by: &[String],
    desc: bool,
    sheet: Option<&str>,
) -> Result<Value, CommandError> {
    let index = resolve_sheet_index(workbook, sheet)?;
    let keys = by
        .iter()
        .map(|specification| resolve_column(workbook, index, specification))
        .collect::<Result<Vec<_>, _>>()?;
    let (rows, columns) = workbook.sheets[index].dimensions();
    let data_rows = rows.saturating_sub(1);
    if data_rows > 1 {
        let mut snapshot =
            crate::cli::row_ops::snapshot_rows(&workbook.sheets[index], 1, rows, columns);
        snapshot.sort_by(|left, right| {
            for &key in &keys {
                let order = crate::cli::row_ops::cmp_values(
                    &crate::cli::row_ops::display_of(&left.0, key),
                    &crate::cli::row_ops::display_of(&right.0, key),
                );
                if order != std::cmp::Ordering::Equal {
                    return if desc { order.reverse() } else { order };
                }
            }
            std::cmp::Ordering::Equal
        });
        crate::cli::row_ops::rewrite_rows(&mut workbook.sheets[index], 1, rows, columns, snapshot);
        Engine::new().recalc(workbook);
    }
    Ok(json!({
        "sorted_by": by,
        "descending": desc,
        "rows": data_rows,
    }))
}

/// 按键列去重数据行（保留首见行）；键缺省为整行显示值。
fn dedup_workbook(
    workbook: &mut Workbook,
    on: &[String],
    sheet: Option<&str>,
) -> Result<Value, CommandError> {
    let index = resolve_sheet_index(workbook, sheet)?;
    let keys = on
        .iter()
        .map(|specification| resolve_column(workbook, index, specification))
        .collect::<Result<Vec<_>, _>>()?;
    let (rows, columns) = workbook.sheets[index].dimensions();
    let data_rows = rows.saturating_sub(1);
    let snapshot = crate::cli::row_ops::snapshot_rows(&workbook.sheets[index], 1, rows, columns);
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::new();
    let mut removed = 0u64;
    for row in snapshot {
        let signature = if keys.is_empty() {
            (0..columns)
                .map(|column| crate::cli::row_ops::display_of(&row.0, column))
                .collect::<Vec<_>>()
                .join("\u{1}")
        } else {
            keys.iter()
                .map(|&column| crate::cli::row_ops::display_of(&row.0, column))
                .collect::<Vec<_>>()
                .join("\u{1}")
        };
        if seen.insert(signature) {
            kept.push(row);
        } else {
            removed += 1;
        }
    }
    crate::cli::row_ops::rewrite_rows(&mut workbook.sheets[index], 1, rows, columns, kept);
    let remaining = u64::from(data_rows).saturating_sub(removed);
    Ok(json!({
        "removed": removed,
        "remaining": remaining,
    }))
}

/// 把范围（逐字快照）复制到目标锚点；`cut` 为真时清空源范围。
fn copy_move_workbook(
    workbook: &mut Workbook,
    source: &str,
    target: &str,
    sheet: Option<&str>,
    cut: bool,
) -> Result<Value, CommandError> {
    let selection = resolve_selection(workbook, Some(source), sheet)?;
    let anchor = easyexcel::model::CellAddress::parse_a1(target).ok_or_else(|| {
        CommandError::new(ErrorCode::InvalidArgument, format!("无效的目标单元格：{target}"))
    })?;
    let range = selection.range;
    let index = selection.sheet_index;
    let mut payload = Vec::new();
    for (row, column) in range.iter_cells() {
        if let Some(cell) = workbook.sheets[index].get(row, column) {
            payload.push((
                row - range.start.row,
                column - range.start.col,
                cell.clone(),
            ));
        }
    }
    let cells = payload.len();
    if cut {
        workbook.sheets[index].clear_range(range);
    }
    for (row_offset, column_offset, cell) in payload {
        workbook.sheets[index].set(
            anchor.row + row_offset,
            anchor.col + column_offset,
            cell,
        );
    }
    Engine::new().recalc(workbook);
    Ok(json!({
        "source": source,
        "target": target,
        "cells": cells,
    }))
}

/// 按行键列分组并聚合数值列，返回分组行集（不改文件）。
fn pivot(
    path: &Path,
    rows: &str,
    values: &str,
    agg: crate::Aggregation,
    sheet: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let index = resolve_sheet_index(&workbook, sheet)?;
    let row_key = resolve_column(&workbook, index, rows)?;
    let value_column = resolve_column(&workbook, index, values)?;
    let row_count = workbook.sheets[index].dimensions().0;

    // key → (count, sum, min, max)
    let mut groups: std::collections::BTreeMap<String, (u64, f64, f64, f64)> =
        std::collections::BTreeMap::new();
    for row in 1..row_count {
        let key = workbook.display_cell(index, row, row_key);
        if key.is_empty() {
            continue;
        }
        let entry = groups.entry(key).or_insert((0, 0.0, f64::INFINITY, f64::NEG_INFINITY));
        entry.0 += 1;
        if let easyexcel::model::value::CellValue::Number(n) =
            workbook.sheets[index].value(row, value_column)
        {
            entry.1 += n;
            entry.2 = entry.2.min(n);
            entry.3 = entry.3.max(n);
        }
    }

    let key_label = {
        let header = workbook.display_cell(index, 0, row_key);
        if header.is_empty() {
            easyexcel::model::addr::col_index_to_letters(row_key)
        } else {
            header
        }
    };
    let aggregate = |(count, sum, min, max): &(u64, f64, f64, f64)| -> f64 {
        match agg {
            crate::Aggregation::Sum => *sum,
            crate::Aggregation::Count => {
                #[allow(clippy::cast_precision_loss, reason = "计数远小于 2^52")]
                {
                    *count as f64
                }
            }
            crate::Aggregation::Mean => {
                #[allow(clippy::cast_precision_loss, reason = "计数远小于 2^52")]
                {
                    if *count > 0 { *sum / *count as f64 } else { 0.0 }
                }
            }
            crate::Aggregation::Min => *min,
            crate::Aggregation::Max => *max,
        }
    };
    let grouped = groups
        .iter()
        .map(|(key, stats)| json!([key, aggregate(stats)]))
        .collect::<Vec<_>>();
    let group_count = grouped.len();
    let data = json!({
        "columns": [key_label, agg.as_str()],
        "rows": grouped,
    });
    let mut result = CommandResult::new(CommandName::Pivot, data, is_dry_run(context));
    result.stats.insert("groups".to_owned(), group_count as u64);
    Ok(result)
}

/// 按表头名对齐，把 `addition` 的数据行追加到 `workbook`（保持列序）。
#[allow(clippy::cast_possible_truncation, reason = "列数远小于 u32::MAX")]
fn append_workbook(
    workbook: &mut Workbook,
    addition: &Workbook,
    sheet: Option<&str>,
) -> Result<Value, CommandError> {
    let base_index = resolve_sheet_index(workbook, sheet)?;
    let add_index = resolve_sheet_index(addition, sheet)?;
    let (base_rows, base_columns) = workbook.sheets[base_index].dimensions();
    let (add_rows, _) = addition.sheets[add_index].dimensions();

    let column_map: Vec<Option<u32>> = (0..base_columns)
        .map(|column| {
            let header = workbook.display_cell(base_index, 0, column);
            if header.is_empty() {
                None
            } else {
                (0..addition.sheets[add_index].dimensions().1).find(|&candidate| {
                    addition
                        .display_cell(add_index, 0, candidate)
                        .eq_ignore_ascii_case(&header)
                })
            }
        })
        .collect();

    let mut appended = 0u32;
    for source_row in 1..add_rows {
        let destination_row = base_rows + appended;
        for (base_column, mapped) in column_map.iter().enumerate() {
            if let Some(add_column) = mapped
                && let Some(cell) = addition.sheets[add_index].get(source_row, *add_column)
            {
                workbook.sheets[base_index].set(
                    destination_row,
                    base_column as u32,
                    cell.clone(),
                );
            }
        }
        appended += 1;
    }
    Ok(json!({ "appended": appended }))
}

/// 两工作簿按键列做内连接：左表头 + 右表头，行 = 左行 × 键相等右行。
fn join(
    left_path: &Path,
    right_path: &Path,
    on: &str,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let left = open_workbook(left_path, context)?;
    let right = open_workbook(right_path, context)?;
    let left_index = resolve_sheet_index(&left, None)?;
    let right_index = resolve_sheet_index(&right, None)?;
    let left_key = resolve_column(&left, left_index, on)?;
    let right_key = resolve_column(&right, right_index, on)?;
    let (left_rows, left_columns) = left.sheets[left_index].dimensions();
    let (right_rows, right_columns) = right.sheets[right_index].dimensions();

    let mut right_index_by_key: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();
    for row in 1..right_rows {
        let key = right.display_cell(right_index, row, right_key);
        if !key.is_empty() {
            right_index_by_key.entry(key).or_default().push(row);
        }
    }

    let headers = (0..left_columns)
        .map(|column| left.display_cell(left_index, 0, column))
        .chain((0..right_columns).map(|column| right.display_cell(right_index, 0, column)))
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    for left_row in 1..left_rows {
        let key = left.display_cell(left_index, left_row, left_key);
        let Some(matches) = right_index_by_key.get(&key) else {
            continue;
        };
        for &right_row in matches {
            let mut line = (0..left_columns)
                .map(|column| cell_value_json(&left.sheets[left_index].value(left_row, column)))
                .collect::<Vec<_>>();
            line.extend((0..right_columns).map(|column| {
                cell_value_json(&right.sheets[right_index].value(right_row, column))
            }));
            rows.push(line);
        }
    }
    let row_count = rows.len();
    let data = json!({
        "on": on,
        "columns": headers,
        "rows": rows,
    });
    let mut result = CommandResult::new(CommandName::Join, data, is_dry_run(context));
    result.stats.insert("rows".to_owned(), row_count as u64);
    Ok(result)
}

/// 比较两工作簿：键列行键比较（added/removed/changed）或单元格级比较（cell）。
#[allow(clippy::too_many_lines, reason = "双模式 diff 语义集中维护，拆散削弱协议审计性")]
fn diff(
    left_path: &Path,
    right_path: &Path,
    key: Option<&str>,
    sheet: Option<&str>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let left = open_workbook(left_path, context)?;
    let right = open_workbook(right_path, context)?;
    let mut differences = Vec::new();
    let mode = if let Some(key) = key {
        let left_index = resolve_sheet_index(&left, sheet)?;
        let right_index = resolve_sheet_index(&right, sheet)?;
        let left_key = resolve_column(&left, left_index, key)?;
        let right_key = resolve_column(&right, right_index, key)?;
        let (left_rows, left_columns) = left.sheets[left_index].dimensions();
        let (right_rows, right_columns) = right.sheets[right_index].dimensions();
        let columns = left_columns.max(right_columns);
        let headers = (0..columns)
            .map(|column| {
                let header = left.display_cell(left_index, 0, column);
                if header.is_empty() {
                    easyexcel::model::addr::col_index_to_letters(column)
                } else {
                    header
                }
            })
            .collect::<Vec<_>>();
        let collect = |workbook: &Workbook, index: usize, rows: u32, key_column: u32| {
            let mut map: std::collections::BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();
            for row in 1..rows {
                let row_key = workbook.display_cell(index, row, key_column);
                if row_key.is_empty() {
                    continue;
                }
                map.entry(row_key)
                    .or_insert_with(|| (0..columns).map(|c| workbook.display_cell(index, row, c)).collect());
            }
            map
        };
        let left_map = collect(&left, left_index, left_rows, left_key);
        let right_map = collect(&right, right_index, right_rows, right_key);
        for (row_key, right_values) in &right_map {
            if let Some(left_values) = left_map.get(row_key) {
                let changed: Vec<Value> = left_values
                    .iter()
                    .zip(right_values.iter())
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(column, (a, b))| {
                        json!({"column": headers.get(column).cloned().unwrap_or_default(),
                               "left": a, "right": b})
                    })
                    .collect();
                if !changed.is_empty() {
                    differences.push(json!({"kind": "changed", "key": row_key, "fields": changed}));
                }
            } else {
                differences.push(json!({"kind": "added", "key": row_key}));
            }
        }
        for row_key in left_map.keys() {
            if !right_map.contains_key(row_key) {
                differences.push(json!({"kind": "removed", "key": row_key}));
            }
        }
        "keyed"
    } else {
        // 单元格级：工作表并集逐格比较显示值。
        let mut names: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for sheet_entry in left.sheets.iter().chain(right.sheets.iter()) {
            if seen.insert(sheet_entry.name.to_ascii_lowercase()) {
                names.push(sheet_entry.name.clone());
            }
        }
        for name in &names {
            let left_index = left.sheet_index(name);
            let right_index = right.sheet_index(name);
            let dims = |workbook: &Workbook, index: Option<usize>| {
                index.map_or((0, 0), |i| workbook.sheets[i].dimensions())
            };
            let (rows_left, cols_left) = dims(&left, left_index);
            let (rows_right, cols_right) = dims(&right, right_index);
            let rows = rows_left.max(rows_right);
            let cols = cols_left.max(cols_right);
            for row in 0..rows {
                for column in 0..cols {
                    let value_of = |workbook: &Workbook, index: Option<usize>| {
                        index.map_or_else(String::new, |i| {
                            workbook.display_cell(i, row, column)
                        })
                    };
                    let left_value = value_of(&left, left_index);
                    let right_value = value_of(&right, right_index);
                    if left_value != right_value {
                        differences.push(json!({
                            "kind": "cell",
                            "sheet": name,
                            "address": format!(
                                "{}{}",
                                easyexcel::model::addr::col_index_to_letters(column),
                                row + 1
                            ),
                            "left": if left_value.is_empty() { Value::Null } else { json!(left_value) },
                            "right": if right_value.is_empty() { Value::Null } else { json!(right_value) },
                        }));
                    }
                }
            }
        }
        "cell"
    };
    let count = differences.len();
    let data = json!({
        "mode": mode,
        "differences": differences,
    });
    let mut result = CommandResult::new(CommandName::Diff, data, is_dry_run(context));
    result.stats.insert("differences".to_owned(), count as u64);
    Ok(result)
}

fn info(path: &Path, context: &ExecutionContext) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let sheets = workbook
        .sheets
        .iter()
        .map(|sheet| {
            let (rows, columns) = sheet.dimensions();
            let formulas = sheet
                .cells
                .values()
                .filter(|cell| cell.is_formula())
                .count();
            json!({
                "name": sheet.name,
                "rows": rows,
                "columns": columns,
                "formulas": formulas,
                "merges": sheet.merged.len(),
                "tables": sheet.tables.len(),
            })
        })
        .collect::<Vec<_>>();
    let data = json!({
        "path": path,
        "sheet_count": workbook.sheets.len(),
        "defined_names": workbook.defined_names.len(),
        "sheets": sheets,
    });
    Ok(CommandResult::new(
        CommandName::Info,
        data,
        is_dry_run(context),
    ))
}

fn extract(
    path: &Path,
    range: Option<&str>,
    sheet: Option<&str>,
    output_format: OutputFormat,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(path, context)?;
    let selection = resolve_selection(&workbook, range, sheet)?;
    let data = render_selection(&workbook, &selection, output_format);
    let mut result = CommandResult::new(CommandName::Get, data, is_dry_run(context));
    result.stats.insert(
        "cells".to_owned(),
        u64::from(selection.range.rows()) * u64::from(selection.range.cols()),
    );
    Ok(result)
}

fn head_or_tail(
    path: &Path,
    sheet_name: Option<&str>,
    rows: u32,
    tail: bool,
    output_format: OutputFormat,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    if rows == 0 {
        return Err(CommandError::new(
            ErrorCode::InvalidArgument,
            "rows 必须大于 0",
        ));
    }
    let workbook = open_workbook(path, context)?;
    let base = resolve_selection(&workbook, None, sheet_name)?;
    let sheet = &workbook.sheets[base.sheet_index];
    let (row_count, column_count) = sheet.dimensions();
    let start = if tail {
        row_count.saturating_sub(rows)
    } else {
        0
    };
    let end = row_count.min(start.saturating_add(rows)).saturating_sub(1);
    let range = if row_count == 0 || column_count == 0 {
        CellRange::parse_a1("A1").expect("valid constant range")
    } else {
        CellRange::new(
            easyexcel::model::CellAddress::new(start, 0),
            easyexcel::model::CellAddress::new(end, column_count - 1),
        )
    };
    let selection = crate::cli::selection::Selection {
        sheet_index: base.sheet_index,
        range,
    };
    let command = if tail {
        CommandName::Tail
    } else {
        CommandName::Head
    };
    Ok(CommandResult::new(
        command,
        render_selection(&workbook, &selection, output_format),
        is_dry_run(context),
    ))
}

fn mutate<F>(
    input: &Path,
    output: Option<PathBuf>,
    context: &ExecutionContext,
    command: CommandName,
    mutation: F,
) -> Result<CommandResult, CommandError>
where
    F: FnOnce(&mut Workbook) -> Result<Value, CommandError>,
{
    let mut workbook = open_workbook(input, context)?;
    let data = mutation(&mut workbook)?;
    let target = mutation_target(input, output, context)?;
    let written = save_workbook(&workbook, &target, context)?;
    let mut result = CommandResult::new(command, data, is_dry_run(context));
    result.files.push(GeneratedFile {
        path: target,
        written,
    });
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn mutate_axis(
    input: &Path,
    output: Option<PathBuf>,
    context: &ExecutionContext,
    command: CommandName,
    sheet_name: Option<&str>,
    at: u32,
    count: u32,
    rows: bool,
    insert: bool,
) -> Result<CommandResult, CommandError> {
    if count == 0 {
        return Err(CommandError::new(
            ErrorCode::InvalidArgument,
            "count 必须大于 0",
        ));
    }
    mutate(input, output, context, command, |workbook| {
        let selection = resolve_selection(workbook, None, sheet_name)?;
        let sheet = &mut workbook.sheets[selection.sheet_index];
        match (rows, insert) {
            (true, true) => sheet.insert_rows(at, count),
            (true, false) => sheet.delete_rows(at, count),
            (false, true) => sheet.insert_cols(at, count),
            (false, false) => sheet.delete_cols(at, count),
        }
        Ok(json!({"at": at, "count": count}))
    })
}

fn new_workbook(
    output: &Path,
    sheets: &[String],
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let names = if sheets.is_empty() {
        vec!["Sheet1".to_owned()]
    } else {
        sheets.to_vec()
    };
    let mut workbook = Workbook::empty();
    for name in &names {
        validate_sheet_name(&workbook, name, None)?;
        workbook.add_sheet(name);
    }
    let written = save_workbook(&workbook, output, context)?;
    let mut result = CommandResult::new(
        CommandName::New,
        json!({"sheets": names}),
        is_dry_run(context),
    );
    result.files.push(GeneratedFile {
        path: output.to_path_buf(),
        written,
    });
    Ok(result)
}

fn query(
    input: &Path,
    sql: &str,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(input, context)?;
    let query_result = run_query(&workbook, sql).map_err(|error| {
        CommandError::new(ErrorCode::QueryFailed, "工作簿查询失败")
            .with_diagnostic(error.to_string())
    })?;
    let rows = query_result
        .rows
        .iter()
        .map(|row| row.iter().map(cell_value_json).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut result = CommandResult::new(
        CommandName::Query,
        json!({"columns": query_result.columns, "rows": rows}),
        is_dry_run(context),
    );
    result.stats.insert("rows".to_owned(), rows.len() as u64);
    Ok(result)
}

fn convert(
    input: &Path,
    output: &Path,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let workbook = open_workbook(input, context)?;
    let written = save_workbook(&workbook, output, context)?;
    let mut result = CommandResult::new(
        CommandName::Convert,
        json!({"input": input, "output": output}),
        is_dry_run(context),
    );
    if output.extension().and_then(|value| value.to_str()) == Some("csv")
        && workbook.sheets.len() > 1
    {
        result.warnings.push(CommandWarning::new(
            "CSV_FIRST_SHEET_ONLY",
            "CSV 只能保存第一个工作表",
        ));
    }
    result.files.push(GeneratedFile {
        path: output.to_path_buf(),
        written,
    });
    Ok(result)
}

fn import(
    input: &Path,
    output: &Path,
    markdown_options: Option<MarkdownImportOptions>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    let format = detect_tabular_format(input)?;
    if format == TabularFormat::Markdown {
        let (written, report) = import_markdown(
            input,
            output,
            &markdown_options.unwrap_or_default(),
            context,
        )?;
        return Ok(markdown_result(
            CommandName::Import,
            input,
            output,
            written,
            report,
            context,
        ));
    }
    let document_text = fs::read_to_string(input).map_err(|error| {
        CommandError::new(
            ErrorCode::ReadFailed,
            format!("无法读取表格文档：{}", input.display()),
        )
        .with_diagnostic(error.to_string())
    })?;
    let document = easyexcel::tabular::parse_document(&document_text, format).map_err(|error| {
        CommandError::new(ErrorCode::ReadFailed, "表格文档解析失败")
            .with_diagnostic(error.to_string())
    })?;
    let workbook = document.to_workbook();
    let written = save_workbook(&workbook, output, context)?;
    let mut result = CommandResult::new(
        CommandName::Import,
        json!({"tables": document.tables().len()}),
        is_dry_run(context),
    );
    result.files.push(GeneratedFile {
        path: output.to_path_buf(),
        written,
    });
    Ok(result)
}

fn export(
    input: &Path,
    output: &Path,
    output_format: OutputFormat,
    markdown_options: Option<MarkdownExportOptions>,
    context: &ExecutionContext,
) -> Result<CommandResult, CommandError> {
    if output_format == OutputFormat::Markdown {
        let (written, report) = export_markdown(
            input,
            output,
            &markdown_options.unwrap_or_default(),
            context,
        )?;
        return Ok(markdown_result(
            CommandName::Export,
            input,
            output,
            written,
            report,
            context,
        ));
    }
    let workbook = open_workbook(input, context)?;
    let document = TabularDocument::from_workbook(&workbook);
    let rendered_text = match output_format {
        OutputFormat::Markdown => unreachable!("Markdown 已由 easyexcel::markdown 处理"),
        OutputFormat::Html => easyexcel::tabular::render_html(&document),
        OutputFormat::Json => easyexcel::tabular::render_json(&document),
        OutputFormat::Csv | OutputFormat::Tsv => {
            let selection = resolve_selection(&workbook, None, None)?;
            render_selection(&workbook, &selection, output_format)
                .as_str()
                .unwrap_or_default()
                .to_owned()
        }
    };
    let written = write_text(&rendered_text, output, context)?;
    let mut result = CommandResult::new(
        CommandName::Export,
        json!({"tables": document.tables().len(), "output_format": output_format}),
        is_dry_run(context),
    );
    result.files.push(GeneratedFile {
        path: output.to_path_buf(),
        written,
    });
    Ok(result)
}

fn markdown_result(
    command: CommandName,
    input: &Path,
    output: &Path,
    written: bool,
    report: MarkdownConversionReport,
    context: &ExecutionContext,
) -> CommandResult {
    let mut result = CommandResult::new(
        command,
        json!({
            "input": input,
            "output": output,
            "mode": report.mode_used,
            "tables": report.tables_processed,
        }),
        is_dry_run(context),
    );
    result.files.push(GeneratedFile {
        path: output.to_path_buf(),
        written,
    });
    result
        .stats
        .insert("sheets".to_owned(), report.sheets_processed as u64);
    result
        .stats
        .insert("tables".to_owned(), report.tables_processed as u64);
    result
        .stats
        .insert("rows".to_owned(), report.rows_processed);
    result
        .stats
        .insert("cells".to_owned(), report.cells_processed);
    result
        .stats
        .insert("output_bytes".to_owned(), report.output_bytes);
    result
        .warnings
        .extend(report.warnings.into_iter().map(|warning| CommandWarning {
            code: markdown_warning_code(warning.code).to_owned(),
            message: warning.message,
            sheet: warning.sheet,
            range: warning.range,
        }));
    result
}

const fn markdown_warning_code(code: MarkdownWarningCode) -> &'static str {
    match code {
        MarkdownWarningCode::MergeFlattened => "MERGE_FLATTENED",
        MarkdownWarningCode::MergeMetadataUnavailable => "MERGE_METADATA_UNAVAILABLE",
        MarkdownWarningCode::HiddenSheetSkipped => "HIDDEN_SHEET_SKIPPED",
        MarkdownWarningCode::StyleDropped => "STYLE_DROPPED",
        MarkdownWarningCode::UnsupportedObjectDropped => "UNSUPPORTED_OBJECT_DROPPED",
        MarkdownWarningCode::EmptySheet => "EMPTY_SHEET",
    }
}

fn validate_sheet_name(
    workbook: &Workbook,
    name: &str,
    allowed_index: Option<usize>,
) -> Result<(), CommandError> {
    if name.is_empty()
        || name.chars().count() > 31
        || name
            .chars()
            .any(|character| matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']'))
    {
        return Err(CommandError::new(
            ErrorCode::InvalidArgument,
            format!("无效工作表名称：{name}"),
        ));
    }
    if workbook
        .sheets
        .iter()
        .enumerate()
        .any(|(index, sheet)| Some(index) != allowed_index && sheet.name.eq_ignore_ascii_case(name))
    {
        return Err(CommandError::new(
            ErrorCode::InvalidArgument,
            format!("工作表名称已存在：{name}"),
        ));
    }
    Ok(())
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "serde_json::to_value 的 map_err 回调按值接收错误"
)]
fn internal_serialization_error(error: serde_json::Error) -> CommandError {
    CommandError::new(ErrorCode::Internal, "结果序列化失败").with_diagnostic(error.to_string())
}

fn is_dry_run(context: &ExecutionContext) -> bool {
    context.mode() == ExecutionMode::DryRun
}
