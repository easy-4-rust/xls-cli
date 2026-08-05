use serde::{Deserialize, Serialize};

/// 稳定的命令标识；JSON 协议和 capabilities 均使用该枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CommandName {
    /// 启动交互式 TUI。
    Open,
    /// 工作簿信息。
    Info,
    /// 提取范围。
    Get,
    /// 提取前若干行。
    Head,
    /// 提取后若干行。
    Tail,
    /// 写入单元格。
    Set,
    /// 清空范围。
    Clear,
    /// 填充范围。
    Fill,
    /// 插入行。
    InsertRows,
    /// 删除行。
    DeleteRows,
    /// 插入列。
    InsertColumns,
    /// 删除列。
    DeleteColumns,
    /// 新建工作簿。
    New,
    /// 新增工作表。
    AddSheet,
    /// 删除工作表。
    DeleteSheet,
    /// 重命名工作表。
    RenameSheet,
    /// SQL 查询。
    Query,
    /// 格式转换。
    Convert,
    /// 导入 Markdown、HTML 或 JSON。
    Import,
    /// 导出 Markdown、HTML、JSON 或分隔文本。
    Export,
    /// 公式重算。
    Recalc,
    /// 能力清单。
    Capabilities,
    /// 命令 Schema。
    Schema,
    /// 文本搜索。
    Grep,
    /// 性能与结构分析。
    Profile,
    /// 复制范围。
    Copy,
    /// 移动范围。
    Move,
    /// 追加数据。
    Append,
    /// 过滤数据。
    Filter,
    /// 排序数据。
    Sort,
    /// 数据去重。
    Dedup,
    /// 数据连接。
    Join,
    /// 透视表。
    Pivot,
    /// 差异比较。
    Diff,
    /// 单元格格式。
    Format,
    /// 设置数字格式代码。
    FormatSet,
    /// 将文本数字转换为数值。
    ToNumber,
    /// 将文本日期转换为日期序列。
    ToDate,
    /// 样式操作。
    Style,
    /// 自动列宽。
    Autofit,
    /// 批处理。
    Batch,
    /// 名称管理。
    Name,
    /// 表格对象管理。
    Table,
    /// 单公式求值。
    Eval,
}

impl CommandName {
    /// 返回命令行与协议使用的固定名称。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Info => "info",
            Self::Get => "get",
            Self::Head => "head",
            Self::Tail => "tail",
            Self::Set => "set",
            Self::Clear => "clear",
            Self::Fill => "fill",
            Self::InsertRows => "insert-rows",
            Self::DeleteRows => "delete-rows",
            Self::InsertColumns => "insert-columns",
            Self::DeleteColumns => "delete-columns",
            Self::New => "new",
            Self::AddSheet => "add-sheet",
            Self::DeleteSheet => "delete-sheet",
            Self::RenameSheet => "rename-sheet",
            Self::Query => "query",
            Self::Convert => "convert",
            Self::Import => "import",
            Self::Export => "export",
            Self::Recalc => "recalc",
            Self::Capabilities => "capabilities",
            Self::Schema => "schema",
            Self::Grep => "grep",
            Self::Profile => "profile",
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Append => "append",
            Self::Filter => "filter",
            Self::Sort => "sort",
            Self::Dedup => "dedup",
            Self::Join => "join",
            Self::Pivot => "pivot",
            Self::Diff => "diff",
            Self::Format => "format",
            Self::FormatSet => "format-set",
            Self::ToNumber => "to-number",
            Self::ToDate => "to-date",
            Self::Style => "style",
            Self::Autofit => "autofit",
            Self::Batch => "batch",
            Self::Name => "name",
            Self::Table => "table",
            Self::Eval => "eval",
        }
    }
}
