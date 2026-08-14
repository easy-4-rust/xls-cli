use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use easyexcel::markdown::{MarkdownExportOptions, MarkdownImportOptions};

use crate::{Aggregation, CellInput, CommandName, OutputFormat};

/// `name` 命令的子动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, clap::Subcommand)]
#[serde(tag = "action", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NameAction {
    /// 列出全部定义名称。
    List,
    /// 新增或替换定义名称。
    Add {
        /// 名称标识。
        name: String,
        /// 引用目标（如 `Sheet1!$A$1:$B$9`）。
        refers_to: String,
        /// 限定作用域的工作表；缺省为工作簿级。
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },
    /// 删除定义名称。
    Remove {
        /// 要删除的名称。
        name: String,
    },
}

/// `table` 命令的子动作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, clap::Subcommand)]
#[serde(tag = "action", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TableAction {
    /// 列出全部表格对象。
    List,
    /// 在范围上创建表格（首行为表头）。
    Add {
        /// 范围，如 `A1:C20`。
        range: String,
        /// 表名；缺省 `Table1`、`Table2`…。
        #[arg(long)]
        name: Option<String>,
        /// 范围所在工作表；缺省活跃表。
        #[arg(long, short = 's')]
        sheet: Option<String>,
        /// 范围仅含数据（无表头行）。
        #[arg(long)]
        no_header: bool,
    },
    /// 按名称删除表格。
    Remove {
        /// 要删除的表名。
        name: String,
    },
}

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
    /// 对工作簿数据求值单条公式；数组结果以网格返回。
    Eval {
        input: PathBuf,
        formula: String,
        at: Option<String>,
    },
    /// 查询单元格的数字格式类别与格式代码。
    Format {
        input: PathBuf,
        cell: String,
    },
    /// 按谓词过滤数据行（如 `amount>1000`），返回命中行集。
    Filter {
        input: PathBuf,
        predicate: String,
        sheet: Option<String>,
    },
    /// 按键列稳定多键排序数据行（保留表头）。
    Sort {
        input: PathBuf,
        by: Vec<String>,
        desc: bool,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 按键列去重数据行（保留首见行）。
    Dedup {
        input: PathBuf,
        on: Vec<String>,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 把范围复制到目标锚点单元格。
    Copy {
        input: PathBuf,
        source: String,
        target: String,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 把范围移动到目标锚点单元格（复制后清空源）。
    Move {
        input: PathBuf,
        source: String,
        target: String,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 按行键列分组并聚合数值列，返回分组表。
    Pivot {
        input: PathBuf,
        rows: String,
        values: String,
        agg: Aggregation,
        sheet: Option<String>,
    },
    /// 按表头名对齐，把另一工作簿的数据行追加到当前工作簿。
    Append {
        input: PathBuf,
        with: PathBuf,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 两工作簿按键列做内连接，返回合并行集。
    Join {
        input: PathBuf,
        with: PathBuf,
        on: String,
    },
    /// 比较两工作簿；提供键列时做行键比较，否则做单元格级比较。
    Diff {
        input: PathBuf,
        with: PathBuf,
        key: Option<String>,
        sheet: Option<String>,
    },
    /// 为范围设置数字格式代码。
    FormatSet {
        input: PathBuf,
        range: String,
        code: String,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 把范围内文本存储的数字强制转换为数值。
    ToNumber {
        input: PathBuf,
        range: String,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 把范围内文本日期解析为日期序列并应用格式。
    ToDate {
        input: PathBuf,
        range: String,
        format: String,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 按内容自适应列宽。
    Autofit {
        input: PathBuf,
        columns: Option<String>,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 为范围设置字体/填充样式。
    Style {
        input: PathBuf,
        range: String,
        bold: bool,
        italic: bool,
        color: Option<String>,
        bg: Option<String>,
        sheet: Option<String>,
        output: Option<PathBuf>,
    },
    /// 管理定义名称（命名范围）。
    Name {
        input: PathBuf,
        action: NameAction,
        output: Option<PathBuf>,
    },
    /// 管理 Excel 表格对象。
    Table {
        input: PathBuf,
        action: TableAction,
        output: Option<PathBuf>,
    },
    /// 一次打开保存内应用多条 CELL=VALUE 编辑（原子）。
    Batch {
        input: PathBuf,
        sets: Vec<String>,
        sheet: Option<String>,
        output: Option<PathBuf>,
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
            Self::Eval { .. } => CommandName::Eval,
            Self::Format { .. } => CommandName::Format,
            Self::Filter { .. } => CommandName::Filter,
            Self::Sort { .. } => CommandName::Sort,
            Self::Dedup { .. } => CommandName::Dedup,
            Self::Copy { .. } => CommandName::Copy,
            Self::Move { .. } => CommandName::Move,
            Self::Pivot { .. } => CommandName::Pivot,
            Self::Append { .. } => CommandName::Append,
            Self::Join { .. } => CommandName::Join,
            Self::Diff { .. } => CommandName::Diff,
            Self::FormatSet { .. } => CommandName::FormatSet,
            Self::ToNumber { .. } => CommandName::ToNumber,
            Self::ToDate { .. } => CommandName::ToDate,
            Self::Autofit { .. } => CommandName::Autofit,
            Self::Style { .. } => CommandName::Style,
            Self::Name { .. } => CommandName::Name,
            Self::Table { .. } => CommandName::Table,
            Self::Batch { .. } => CommandName::Batch,
            Self::Planned { command_name, .. } => *command_name,
        }
    }
}
