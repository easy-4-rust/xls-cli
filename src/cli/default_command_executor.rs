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
