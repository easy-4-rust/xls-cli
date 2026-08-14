use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use easyexcel::markdown::{MarkdownExportOptions, MarkdownImportOptions};

use crate::{CellInput, CommandName, OutputFormat};

/// 带类型的命令请求。路径使用平台原生 [`PathBuf`]，不强制 UTF-8。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case")]
#[non_exhaustive]
#[allow(
    missing_docs,
    reason = "各 variant 已描述命令语义，字段由 JSON Schema 与同名参数共同约束"
)]
pub enum CommandRequest {
    /// 读取工作簿元数据。
    Info { input: PathBuf },
    /// 提取指定范围，例如 `Sheet1!A1:C10`。
    Get {
        input: PathBuf,
        range: Option<String>,
        output_format: OutputFormat,
    },
    /// 提取工作表前若干行。
    Head {
        input: PathBuf,
        sheet: Option<String>,
        rows: u32,
        output_format: OutputFormat,
    },
    /// 提取工作表后若干行。
    Tail {
        input: PathBuf,
        sheet: Option<String>,
        rows: u32,
        output_format: OutputFormat,
    },
    /// 设置单个单元格。
    Set {
        input: PathBuf,
        cell: String,
        value: CellInput,
        output: Option<PathBuf>,
    },
    /// 清空一个范围。
    Clear {
        input: PathBuf,
        range: String,
        output: Option<PathBuf>,
    },
    /// 用同一值填充一个范围。
    Fill {
        input: PathBuf,
        range: String,
        value: CellInput,
        output: Option<PathBuf>,
    },
    /// 插入空行。
    InsertRows {
        input: PathBuf,
        sheet: Option<String>,
        at: u32,
        count: u32,
        output: Option<PathBuf>,
    },
    /// 删除行。
    DeleteRows {
        input: PathBuf,
        sheet: Option<String>,
        at: u32,
        count: u32,
        output: Option<PathBuf>,
    },
    /// 插入空列。
    InsertColumns {
        input: PathBuf,
        sheet: Option<String>,
        at: u32,
        count: u32,
        output: Option<PathBuf>,
    },
    /// 删除列。
    DeleteColumns {
        input: PathBuf,
        sheet: Option<String>,
        at: u32,
        count: u32,
        output: Option<PathBuf>,
    },
    /// 新建工作簿。
    New {
        output: PathBuf,
        sheets: Vec<String>,
    },
    /// 新增工作表。
    AddSheet {
        input: PathBuf,
        name: String,
        output: Option<PathBuf>,
    },
    /// 删除工作表。
    DeleteSheet {
        input: PathBuf,
        name: String,
        output: Option<PathBuf>,
    },
    /// 重命名工作表。
    RenameSheet {
        input: PathBuf,
        name: String,
        new_name: String,
        output: Option<PathBuf>,
    },
    /// 对工作簿运行只读 SQL。
    Query { input: PathBuf, sql: String },
    /// 根据输出扩展名转换 XLS、XLSX、CSV。
    Convert { input: PathBuf, output: PathBuf },
    /// 从 Markdown、HTML 或 JSON 文档导入工作簿。
    Import {
        input: PathBuf,
        output: PathBuf,
        #[serde(default)]
        markdown_options: Option<MarkdownImportOptions>,
    },
    /// 将工作簿导出为 Markdown、HTML 或 JSON 文档。
    Export {
        input: PathBuf,
        output: PathBuf,
        output_format: OutputFormat,
        #[serde(default)]
        markdown_options: Option<MarkdownExportOptions>,
    },
    /// 重算公式并保存缓存值。
    Recalc {
        input: PathBuf,
        output: Option<PathBuf>,
    },
    /// 返回当前构建的能力清单。
    Capabilities,
    /// 返回指定命令的 JSON Schema。
    Schema { target: CommandName },
    /// 在工作簿显示值中做大小写不敏感的子串搜索，返回命中单元格清单。
    Grep {
        input: PathBuf,
        pattern: String,
        sheet: Option<String>,
    },
    /// 统计一列的数据概况，并对“数字/日期存为文本”给出稳定警告。
    Profile {
        input: PathBuf,
        column: String,
        sheet: Option<String>,
    },
    /// 已进入协议但尚未迁入生产实现的命令。
    Planned {
        command_name: CommandName,
        #[serde(default)]
        arguments: Value,
    },
}

impl CommandRequest {
    /// 返回请求的稳定命令标识。
    #[must_use]
    pub const fn command_name(&self) -> CommandName {
        match self {
            Self::Info { .. } => CommandName::Info,
            Self::Get { .. } => CommandName::Get,
            Self::Head { .. } => CommandName::Head,
            Self::Tail { .. } => CommandName::Tail,
            Self::Set { .. } => CommandName::Set,
            Self::Clear { .. } => CommandName::Clear,
            Self::Fill { .. } => CommandName::Fill,
            Self::InsertRows { .. } => CommandName::InsertRows,
            Self::DeleteRows { .. } => CommandName::DeleteRows,
            Self::InsertColumns { .. } => CommandName::InsertColumns,
            Self::DeleteColumns { .. } => CommandName::DeleteColumns,
            Self::New { .. } => CommandName::New,
            Self::AddSheet { .. } => CommandName::AddSheet,
            Self::DeleteSheet { .. } => CommandName::DeleteSheet,
            Self::RenameSheet { .. } => CommandName::RenameSheet,
            Self::Query { .. } => CommandName::Query,
            Self::Convert { .. } => CommandName::Convert,
            Self::Import { .. } => CommandName::Import,
            Self::Export { .. } => CommandName::Export,
            Self::Recalc { .. } => CommandName::Recalc,
            Self::Capabilities => CommandName::Capabilities,
            Self::Schema { .. } => CommandName::Schema,
            Self::Grep { .. } => CommandName::Grep,
            Self::Profile { .. } => CommandName::Profile,
            Self::Planned { command_name, .. } => *command_name,
        }
    }
}
