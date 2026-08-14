use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use easyexcel::csv::{CsvReadOptions, CsvWriteOptions};
use easyexcel::io::{Format, ResourceLimits};
use easyexcel::markdown::{MarkdownConversionReport, MarkdownExportOptions, MarkdownImportOptions};
use easyexcel::model::{Cell, Workbook};

use crate::{
    CommandError, ErrorCode, ExecutionContext, ExecutionMode, OverwritePolicy, SecretString,
};

/// 读取工作簿，并在解析前后应用资源限制。
pub(crate) fn open_workbook(
    path: &Path,
    context: &ExecutionContext,
) -> Result<Workbook, CommandError> {
    let metadata = fs::metadata(path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::FileNotFound
        } else {
            ErrorCode::ReadFailed
        };
        CommandError::new(code, format!("无法读取输入文件：{}", path.display()))
            .with_diagnostic(error.to_string())
    })?;
    if metadata.len() > context.limits().max_file_bytes() {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            format!(
                "输入文件超过 {} 字节限制",
                context.limits().max_file_bytes()
            ),
        ));
    }
    let format = detect_input_format(path)?;
    let workbook = match format {
        Format::Xlsx => easyexcel::xlsx::read_path_with_password(
            path,
            context.password().map(SecretString::expose_secret),
        ),
        Format::Xls => easyexcel::xls::read_path(path),
        Format::Csv => File::open(path)
            .map_err(easyexcel::io::Error::from)
            .and_then(|file| easyexcel::csv::read_csv(file, &CsvReadOptions::default())),
        _ => Err(easyexcel::io::Error::Unsupported(
            "当前构建不支持该输入格式".to_owned(),
        )),
    }
    .map_err(|error| {
        CommandError::new(
            ErrorCode::ReadFailed,
            format!("无法解析工作簿：{}", path.display()),
        )
        .with_diagnostic(error.to_string())
    })?;
    validate_workbook(&workbook, context.limits())?;
    Ok(workbook)
}

/// 原子写入工作簿，默认拒绝覆盖。
pub(crate) fn save_workbook(
    workbook: &Workbook,
    target: &Path,
    context: &ExecutionContext,
) -> Result<bool, CommandError> {
    validate_target(target, context)?;
    validate_workbook(workbook, context.limits())?;
    if context.mode() == ExecutionMode::DryRun {
        return Ok(false);
    }
    let format = Format::from_path(target).ok_or_else(|| {
        CommandError::new(
            ErrorCode::UnsupportedFormat,
            format!("无法从扩展名识别输出格式：{}", target.display()),
        )
    })?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| write_error(target, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| write_error(target, error))?;
    write_to(workbook, format, temporary.as_file_mut())
        .map_err(|error| write_error(target, error))?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| write_error(target, error))?;
    temporary
        .persist(target)
        .map_err(|error| write_error(target, error.error))?;
    Ok(true)
}

pub(crate) fn write_text(
    text: &str,
    target: &Path,
    context: &ExecutionContext,
) -> Result<bool, CommandError> {
    validate_target(target, context)?;
    if context.mode() == ExecutionMode::DryRun {
        return Ok(false);
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| write_error(target, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| write_error(target, error))?;
    temporary
        .write_all(text.as_bytes())
        .map_err(|error| write_error(target, error))?;
    temporary
        .persist(target)
        .map_err(|error| write_error(target, error.error))?;
    Ok(true)
}

/// 通过 `EasyExcel` Markdown 门面原子导出，保留 dry-run 与覆盖保护。
pub(crate) fn export_markdown(
    input: &Path,
    target: &Path,
    options: &MarkdownExportOptions,
    context: &ExecutionContext,
) -> Result<(bool, MarkdownConversionReport), CommandError> {
    validate_target(target, context)?;
    let options = options.clone().with_limits(context.limits());
    if context.mode() == ExecutionMode::DryRun {
        let (_, report) = easyexcel::markdown::export_to_writer_with_password(
            input,
            Vec::new(),
            &options,
            context.password().map(SecretString::expose_secret),
        )
        .map_err(|error| markdown_error(&error))?;
        return Ok((false, report));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| write_error(target, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| write_error(target, error))?;
    let (_, report) = easyexcel::markdown::export_to_writer_with_password(
        input,
        temporary.as_file_mut(),
        &options,
        context.password().map(SecretString::expose_secret),
    )
    .map_err(|error| markdown_error(&error))?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| write_error(target, error))?;
    temporary
        .persist(target)
        .map_err(|error| write_error(target, error.error))?;
    Ok((true, report))
}

/// 通过 `EasyExcel` Markdown 门面原子导入，保留 dry-run 与覆盖保护。
pub(crate) fn import_markdown(
    input: &Path,
    target: &Path,
    options: &MarkdownImportOptions,
    context: &ExecutionContext,
) -> Result<(bool, MarkdownConversionReport), CommandError> {
    validate_target(target, context)?;
    let format = Format::from_path(target).ok_or_else(|| {
        CommandError::new(
            ErrorCode::UnsupportedFormat,
            format!("无法从扩展名识别输出格式：{}", target.display()),
        )
    })?;
    let options = options.clone().with_limits(context.limits());
    if context.mode() == ExecutionMode::DryRun {
        let (_, report) = easyexcel::markdown::import_to_writer(
            input,
            format,
            std::io::Cursor::new(Vec::new()),
            &options,
        )
        .map_err(|error| markdown_error(&error))?;
        return Ok((false, report));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| write_error(target, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| write_error(target, error))?;
    let (_, report) =
        easyexcel::markdown::import_to_writer(input, format, temporary.as_file_mut(), &options)
            .map_err(|error| markdown_error(&error))?;
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| write_error(target, error))?;
    temporary
        .persist(target)
        .map_err(|error| write_error(target, error.error))?;
    Ok((true, report))
}

pub(crate) fn mutation_target(
    input: &Path,
    output: Option<PathBuf>,
    context: &ExecutionContext,
) -> Result<PathBuf, CommandError> {
    if let Some(output) = output {
        return Ok(output);
    }
    if context.overwrite() != OverwritePolicy::Replace && !matches!(context.mode(), ExecutionMode::DryRun) {
        // DryRun 允许就地预览：save_workbook 在 DryRun 下不会写盘。
        return Err(CommandError::new(
            ErrorCode::OverwriteDenied,
            "修改源文件必须显式允许覆盖，或提供输出文件",
        ));
    }
    Ok(input.to_path_buf())
}

pub(crate) fn detect_tabular_format(
    path: &Path,
) -> Result<easyexcel::tabular::TabularFormat, CommandError> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown") => Ok(easyexcel::tabular::TabularFormat::Markdown),
        Some("html" | "htm") => Ok(easyexcel::tabular::TabularFormat::Html),
        Some("json") => Ok(easyexcel::tabular::TabularFormat::Json),
        _ => Err(CommandError::new(
            ErrorCode::UnsupportedFormat,
            format!("不支持的表格文档格式：{}", path.display()),
        )),
    }
}

fn validate_target(target: &Path, context: &ExecutionContext) -> Result<(), CommandError> {
    if target.exists()
        && context.overwrite() != OverwritePolicy::Replace
        && !matches!(context.mode(), ExecutionMode::DryRun)
    {
        return Err(CommandError::new(
            ErrorCode::OverwriteDenied,
            format!("目标文件已存在：{}", target.display()),
        ));
    }
    Ok(())
}

fn detect_input_format(path: &Path) -> Result<Format, CommandError> {
    if let Some(format) = Format::from_path(path) {
        return Ok(format);
    }
    let mut file = File::open(path).map_err(|error| {
        CommandError::new(
            ErrorCode::ReadFailed,
            format!("无法打开输入文件：{}", path.display()),
        )
        .with_diagnostic(error.to_string())
    })?;
    let mut magic = [0_u8; 8];
    let count = file.read(&mut magic).map_err(|error| {
        CommandError::new(ErrorCode::ReadFailed, "无法读取文件头")
            .with_diagnostic(error.to_string())
    })?;
    Ok(Format::from_magic(&magic[..count]))
}

fn validate_workbook(workbook: &Workbook, limits: ResourceLimits) -> Result<(), CommandError> {
    if workbook.sheets.len() > limits.max_sheets() {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            format!("工作表数量超过 {} 限制", limits.max_sheets()),
        ));
    }
    let total_rows = workbook
        .sheets
        .iter()
        .map(|sheet| u64::from(sheet.dimensions().0))
        .sum::<u64>();
    if total_rows > limits.max_rows() {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            format!("总行数超过 {} 限制", limits.max_rows()),
        ));
    }
    let formula_cells = workbook
        .sheets
        .iter()
        .flat_map(|sheet| sheet.cells.values())
        .filter(|cell| matches!(cell, Cell::Formula { .. }))
        .count() as u64;
    if formula_cells > limits.max_formula_cells() {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            format!("公式单元格超过 {} 限制", limits.max_formula_cells()),
        ));
    }
    Ok(())
}

fn write_to<W: Read + Write + Seek>(
    workbook: &Workbook,
    format: Format,
    writer: W,
) -> easyexcel::io::Result<()> {
    match format {
        Format::Xlsx => easyexcel::xlsx::write(workbook, writer),
        Format::Xls => easyexcel::xls::write(workbook, writer),
        Format::Csv => easyexcel::csv::write_csv(workbook, 0, writer, &CsvWriteOptions::default()),
        _ => Err(easyexcel::io::Error::Unsupported(
            "当前构建不支持该输出格式".to_owned(),
        )),
    }
}

fn write_error(path: &Path, error: impl std::fmt::Display) -> CommandError {
    CommandError::new(
        ErrorCode::WriteFailed,
        format!("无法写入目标文件：{}", path.display()),
    )
    .with_diagnostic(error.to_string())
}

fn markdown_error(error: &easyexcel::ExcelError) -> CommandError {
    let code = match error {
        easyexcel::ExcelError::ResourceLimit(_) => ErrorCode::ResourceLimit,
        easyexcel::ExcelError::Unsupported(_) => ErrorCode::UnsupportedFormat,
        _ => ErrorCode::WriteFailed,
    };
    CommandError::new(code, "Markdown 转换失败").with_diagnostic(error.to_string())
}
