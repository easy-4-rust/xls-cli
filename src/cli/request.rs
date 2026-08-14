use easyexcel::markdown::{
    MarkdownConversionMode, MarkdownExportOptions, MarkdownFormulaPolicy, MarkdownImportOptions,
    MarkdownMergePolicy, MarkdownSheetSelection, MarkdownTableSelection, MarkdownTypeInference,
};

use crate::cli::{
    CellInput, CliMarkdownFormulaPolicy, CliMarkdownMergePolicy, CliMarkdownMode,
    CliMarkdownTypeInference, CliOutputFormat, CommandName, CommandRequest, Commands, OutputFormat,
};

#[allow(
    clippy::too_many_lines,
    reason = "参数枚举到协议枚举保持单一穷尽映射，便于发现遗漏命令"
)]
pub(crate) fn into_request(command: Commands) -> Result<CommandRequest, String> {
    Ok(match command {
        Commands::Info { input } => CommandRequest::Info { input },
        Commands::Get {
            input,
            range,
            format,
        } => CommandRequest::Get {
            input,
            range,
            output_format: format.into(),
        },
        Commands::Head {
            input,
            rows,
            sheet,
            format,
        } => CommandRequest::Head {
            input,
            sheet,
            rows,
            output_format: format.into(),
        },
        Commands::Tail {
            input,
            rows,
            sheet,
            format,
        } => CommandRequest::Tail {
            input,
            sheet,
            rows,
            output_format: format.into(),
        },
        Commands::Set {
            input,
            cell,
            value,
            output,
        } => CommandRequest::Set {
            input,
            cell,
            value: parse_cell_input(&value),
            output,
        },
        Commands::Clear {
            input,
            range,
            output,
        } => CommandRequest::Clear {
            input,
            range,
            output,
        },
        Commands::Fill {
            input,
            range,
            value,
            output,
        } => CommandRequest::Fill {
            input,
            range,
            value: parse_cell_input(&value),
            output,
        },
        Commands::InsertRow {
            input,
            at,
            count,
            sheet,
            output,
        } => CommandRequest::InsertRows {
            input,
            sheet,
            at,
            count,
            output,
        },
        Commands::DeleteRow {
            input,
            at,
            count,
            sheet,
            output,
        } => CommandRequest::DeleteRows {
            input,
            sheet,
            at,
            count,
            output,
        },
        Commands::InsertCol {
            input,
            at,
            count,
            sheet,
            output,
        } => CommandRequest::InsertColumns {
            input,
            sheet,
            at,
            count,
            output,
        },
        Commands::DeleteCol {
            input,
            at,
            count,
            sheet,
            output,
        } => CommandRequest::DeleteColumns {
            input,
            sheet,
            at,
            count,
            output,
        },
        Commands::New { output, sheets } => CommandRequest::New { output, sheets },
        Commands::AddSheet {
            input,
            name,
            output,
        } => CommandRequest::AddSheet {
            input,
            name,
            output,
        },
        Commands::DeleteSheet {
            input,
            name,
            output,
        } => CommandRequest::DeleteSheet {
            input,
            name,
            output,
        },
        Commands::RenameSheet {
            input,
            name,
            new_name,
            output,
        } => CommandRequest::RenameSheet {
            input,
            name,
            new_name,
            output,
        },
        Commands::Query { input, sql } => CommandRequest::Query { input, sql },
        Commands::Convert { input, output } => CommandRequest::Convert { input, output },
        Commands::Import {
            input,
            output,
            table,
            infer_types,
        } => {
            let mut options =
                MarkdownImportOptions::default().with_type_inference(infer_types.into());
            if let Some(table) = table {
                options = options.with_tables(parse_table_selection(table));
            }
            CommandRequest::Import {
                input,
                output,
                markdown_options: Some(options),
            }
        }
        Commands::Export {
            input,
            output,
            format,
            mode,
            stream,
            sheet,
            formula,
            merge,
        } => CommandRequest::Export {
            input,
            output,
            output_format: format.into(),
            markdown_options: Some(
                MarkdownExportOptions::default()
                    .with_mode(if stream {
                        MarkdownConversionMode::Event
                    } else {
                        mode.into()
                    })
                    .with_sheets(sheet.map_or(MarkdownSheetSelection::All, parse_sheet_selection))
                    .with_formulas(formula.into())
                    .with_merges(merge.into()),
            ),
        },
        Commands::Recalc { input, output } => CommandRequest::Recalc { input, output },
        Commands::Capabilities => CommandRequest::Capabilities,
        Commands::Format { input, cell } => CommandRequest::Format { input, cell },
        Commands::Filter {
            input,
            predicate,
            sheet,
        } => CommandRequest::Filter {
            input,
            predicate,
            sheet,
        },
        Commands::Sort {
            input,
            by,
            desc,
            sheet,
            output,
        } => CommandRequest::Sort {
            input,
            by,
            desc,
            sheet,
            output,
        },
        Commands::Dedup {
            input,
            on,
            sheet,
            output,
        } => CommandRequest::Dedup {
            input,
            on,
            sheet,
            output,
        },
        Commands::FormatSet {
            input,
            range,
            code,
            sheet,
            output,
        } => CommandRequest::FormatSet {
            input,
            range,
            code,
            sheet,
            output,
        },
        Commands::ToNumber {
            input,
            range,
            sheet,
            output,
        } => CommandRequest::ToNumber {
            input,
            range,
            sheet,
            output,
        },
        Commands::ToDate {
            input,
            range,
            format,
            sheet,
            output,
        } => CommandRequest::ToDate {
            input,
            range,
            format,
            sheet,
            output,
        },
        Commands::Autofit {
            input,
            columns,
            sheet,
            output,
        } => CommandRequest::Autofit {
            input,
            columns,
            sheet,
            output,
        },
        Commands::Append {
            input,
            with,
            sheet,
            output,
        } => CommandRequest::Append {
            input,
            with,
            sheet,
            output,
        },
        Commands::Join { input, with, on } => CommandRequest::Join { input, with, on },
        Commands::Diff {
            input,
            with,
            key,
            sheet,
        } => CommandRequest::Diff {
            input,
            with,
            key,
            sheet,
        },
        Commands::Pivot {
            input,
            rows,
            values,
            agg,
            sheet,
        } => CommandRequest::Pivot {
            input,
            rows,
            values,
            agg,
            sheet,
        },
        Commands::Copy {
            input,
            src,
            dest,
            sheet,
            output,
        } => CommandRequest::Copy {
            input,
            source: src,
            target: dest,
            sheet,
            output,
        },
        Commands::Move {
            input,
            src,
            dest,
            sheet,
            output,
        } => CommandRequest::Move {
            input,
            source: src,
            target: dest,
            sheet,
            output,
        },
        Commands::Eval {
            input,
            formula,
            at,
        } => CommandRequest::Eval {
            input,
            formula,
            at,
        },
        Commands::Profile {
            input,
            column,
            sheet,
        } => CommandRequest::Profile {
            input,
            column,
            sheet,
        },
        Commands::Grep {
            input,
            pattern,
            sheet,
        } => CommandRequest::Grep {
            input,
            pattern,
            sheet,
        },
        Commands::Schema { target } => CommandRequest::Schema {
            target: parse_command_name(&target).ok_or_else(|| format!("未知命令名称：{target}"))?,
        },
        Commands::External(arguments) => {
            let name = arguments.first().ok_or_else(|| "缺少命令名称".to_owned())?;
            let command_name =
                parse_command_name(name).ok_or_else(|| format!("未知命令：{name}"))?;
            CommandRequest::Planned {
                command_name,
                arguments: serde_json::json!(arguments.get(1..).unwrap_or_default()),
            }
        }
    })
}

fn parse_sheet_selection(value: String) -> MarkdownSheetSelection {
    value.parse::<usize>().map_or_else(
        |_| MarkdownSheetSelection::Name(value),
        MarkdownSheetSelection::Index,
    )
}

fn parse_table_selection(value: String) -> MarkdownTableSelection {
    value.parse::<usize>().map_or_else(
        |_| MarkdownTableSelection::Name(value),
        MarkdownTableSelection::Index,
    )
}

impl From<CliMarkdownMode> for MarkdownConversionMode {
    fn from(value: CliMarkdownMode) -> Self {
        match value {
            CliMarkdownMode::Auto => Self::Auto,
            CliMarkdownMode::Event => Self::Event,
            CliMarkdownMode::Workbook => Self::Workbook,
        }
    }
}

impl From<CliMarkdownFormulaPolicy> for MarkdownFormulaPolicy {
    fn from(value: CliMarkdownFormulaPolicy) -> Self {
        match value {
            CliMarkdownFormulaPolicy::Cached => Self::CachedValue,
            CliMarkdownFormulaPolicy::Expression => Self::Expression,
            CliMarkdownFormulaPolicy::Both => Self::ExpressionAndCached,
        }
    }
}

impl From<CliMarkdownMergePolicy> for MarkdownMergePolicy {
    fn from(value: CliMarkdownMergePolicy) -> Self {
        match value {
            CliMarkdownMergePolicy::Anchor => Self::AnchorWithWarning,
            CliMarkdownMergePolicy::Repeat => Self::RepeatAnchor,
            CliMarkdownMergePolicy::Html => Self::HtmlFallback,
            CliMarkdownMergePolicy::Error => Self::Error,
        }
    }
}

impl From<CliMarkdownTypeInference> for MarkdownTypeInference {
    fn from(value: CliMarkdownTypeInference) -> Self {
        match value {
            CliMarkdownTypeInference::Text => Self::Text,
            CliMarkdownTypeInference::Conservative => Self::Conservative,
            CliMarkdownTypeInference::Aggressive => Self::Aggressive,
        }
    }
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(value: CliOutputFormat) -> Self {
        match value {
            CliOutputFormat::Json => Self::Json,
            CliOutputFormat::Csv => Self::Csv,
            CliOutputFormat::Tsv => Self::Tsv,
            CliOutputFormat::Markdown => Self::Markdown,
            CliOutputFormat::Html => Self::Html,
        }
    }
}

fn parse_cell_input(value: &str) -> CellInput {
    if value.eq_ignore_ascii_case("null") || value.eq_ignore_ascii_case("empty") {
        CellInput::Empty
    } else if value.starts_with('=') {
        CellInput::Formula(value.to_owned())
    } else if value.eq_ignore_ascii_case("true") {
        CellInput::Bool(true)
    } else if value.eq_ignore_ascii_case("false") {
        CellInput::Bool(false)
    } else if let Ok(number) = value.parse::<f64>() {
        CellInput::Number(number)
    } else {
        CellInput::Text(value.to_owned())
    }
}

fn parse_command_name(value: &str) -> Option<CommandName> {
    Some(match value {
        "open" => CommandName::Open,
        "info" => CommandName::Info,
        "get" => CommandName::Get,
        "head" => CommandName::Head,
        "tail" => CommandName::Tail,
        "set" => CommandName::Set,
        "clear" => CommandName::Clear,
        "fill" => CommandName::Fill,
        "insert-row" | "insert-rows" => CommandName::InsertRows,
        "delete-row" | "delete-rows" => CommandName::DeleteRows,
        "insert-col" | "insert-columns" => CommandName::InsertColumns,
        "delete-col" | "delete-columns" => CommandName::DeleteColumns,
        "new" => CommandName::New,
        "add-sheet" => CommandName::AddSheet,
        "delete-sheet" => CommandName::DeleteSheet,
        "rename-sheet" => CommandName::RenameSheet,
        "query" => CommandName::Query,
        "convert" => CommandName::Convert,
        "import" => CommandName::Import,
        "export" => CommandName::Export,
        "recalc" => CommandName::Recalc,
        "capabilities" => CommandName::Capabilities,
        "schema" => CommandName::Schema,
        "grep" => CommandName::Grep,
        "profile" => CommandName::Profile,
        "copy" => CommandName::Copy,
        "move" => CommandName::Move,
        "append" => CommandName::Append,
        "filter" => CommandName::Filter,
        "sort" => CommandName::Sort,
        "dedup" => CommandName::Dedup,
        "join" => CommandName::Join,
        "pivot" => CommandName::Pivot,
        "diff" => CommandName::Diff,
        "format" => CommandName::Format,
        "format-set" => CommandName::FormatSet,
        "to-number" => CommandName::ToNumber,
        "to-date" => CommandName::ToDate,
        "style" => CommandName::Style,
        "autofit" => CommandName::Autofit,
        "batch" => CommandName::Batch,
        "name" => CommandName::Name,
        "table" => CommandName::Table,
        "eval" => CommandName::Eval,
        _ => return None,
    })
}
