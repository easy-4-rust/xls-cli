use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// 面向脚本和智能体的电子表格命令行。
#[derive(Debug, Parser)]
#[command(
    name = "xls",
    version,
    about,
    after_long_help = "MIGRATED XLS/TUI COMMANDS:\n  open FILE                  Open the interactive TUI\n  eval FILE FORMULA          Evaluate a formula\n  format / format-set        Inspect or set number formats\n  to-number / to-date        Convert text cells to typed values\n  copy / move / append       Move or combine ranges and workbooks\n  filter / sort / dedup      Transform tabular rows\n  join / pivot / diff        Compare and reshape workbooks\n  grep / profile / batch     Inspect or batch-edit data\n  autofit / style            Adjust workbook presentation\n  name / table               Manage defined names and table objects\n\nRun `xls <command> --help` for the migrated command's full options.\nRun `xls FILE.xlsx` to open a workbook directly in the TUI."
)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "这些布尔值是相互独立且稳定的全局 CLI 开关，由 clap 直接映射"
)]
pub(crate) struct Cli {
    /// 仅向 stdout 输出一个完整 JSON 对象。
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// 验证写操作但不落盘。
    #[arg(long, global = true)]
    pub(crate) dry_run: bool,

    /// 显式允许覆盖目标文件或原文件。
    #[arg(long, global = true)]
    pub(crate) force: bool,

    /// 从 stdin 第一行读取密码。
    #[arg(long, global = true, conflicts_with = "password_env")]
    pub(crate) password_stdin: bool,

    /// 从指定环境变量读取密码；参数只是变量名。
    #[arg(
        long,
        global = true,
        value_name = "NAME",
        conflicts_with = "password_stdin"
    )]
    pub(crate) password_env: Option<String>,

    /// 最大输入文件字节数。
    #[arg(long, global = true, default_value_t = 256 * 1024 * 1024)]
    pub(crate) max_file_bytes: u64,

    /// 最大工作表数量。
    #[arg(long, global = true, default_value_t = 256)]
    pub(crate) max_sheets: usize,

    /// 所有工作表最大总行数。
    #[arg(long, global = true, default_value_t = 2_000_000)]
    pub(crate) max_rows: u64,

    /// 最大公式单元格数量。
    #[arg(long, global = true, default_value_t = 500_000)]
    pub(crate) max_formula_cells: u64,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

/// CLI 薄适配层支持的子命令。
#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// 显示工作簿元数据。
    Info { input: PathBuf },
    /// 提取工作表或 A1 范围。
    Get {
        input: PathBuf,
        range: Option<String>,
        #[arg(long, value_enum, default_value_t = CliOutputFormat::Json)]
        format: CliOutputFormat,
    },
    /// 提取前 N 行。
    Head {
        input: PathBuf,
        #[arg(short = 'n', long, default_value_t = 10)]
        rows: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(long, value_enum, default_value_t = CliOutputFormat::Json)]
        format: CliOutputFormat,
    },
    /// 提取后 N 行。
    Tail {
        input: PathBuf,
        #[arg(short = 'n', long, default_value_t = 10)]
        rows: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(long, value_enum, default_value_t = CliOutputFormat::Json)]
        format: CliOutputFormat,
    },
    /// 写入单元格。
    Set {
        input: PathBuf,
        cell: String,
        value: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 清空范围。
    Clear {
        input: PathBuf,
        range: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 填充范围。
    Fill {
        input: PathBuf,
        range: String,
        value: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 插入行，索引从 0 开始。
    #[command(alias = "insert-rows")]
    InsertRow {
        input: PathBuf,
        at: u32,
        #[arg(short = 'n', long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 删除行，索引从 0 开始。
    #[command(alias = "delete-rows")]
    DeleteRow {
        input: PathBuf,
        at: u32,
        #[arg(short = 'n', long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 插入列，索引从 0 开始。
    #[command(alias = "insert-columns")]
    InsertCol {
        input: PathBuf,
        at: u32,
        #[arg(short = 'n', long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 删除列，索引从 0 开始。
    #[command(alias = "delete-columns")]
    DeleteCol {
        input: PathBuf,
        at: u32,
        #[arg(short = 'n', long, default_value_t = 1)]
        count: u32,
        #[arg(long)]
        sheet: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 新建工作簿。
    New {
        output: PathBuf,
        #[arg(long = "sheet")]
        sheets: Vec<String>,
    },
    /// 新增工作表。
    AddSheet {
        input: PathBuf,
        name: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 删除工作表。
    DeleteSheet {
        input: PathBuf,
        name: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 重命名工作表。
    RenameSheet {
        input: PathBuf,
        name: String,
        new_name: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 对工作簿运行只读 SQL。
    Query { input: PathBuf, sql: String },
    /// 在 XLS、XLSX、CSV 之间转换。
    Convert { input: PathBuf, output: PathBuf },
    /// 将 Markdown、HTML 或 JSON 表格导入 XLS/XLSX/CSV。
    Import {
        input: PathBuf,
        output: PathBuf,
        /// Markdown 表名或零基下标。
        #[arg(long)]
        table: Option<String>,
        /// Markdown 单元格类型推断策略。
        #[arg(long, value_enum, default_value_t = CliMarkdownTypeInference::Conservative)]
        infer_types: CliMarkdownTypeInference,
    },
    /// 将工作簿导出为 Markdown、HTML、JSON、CSV 或 TSV。
    Export {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, value_enum)]
        format: CliOutputFormat,
        /// Markdown 导出执行模式。
        #[arg(long, value_enum, default_value_t = CliMarkdownMode::Auto)]
        mode: CliMarkdownMode,
        /// `--mode event` 的兼容别名。
        #[arg(long, conflicts_with = "mode")]
        stream: bool,
        /// Markdown 工作表名或零基下标。
        #[arg(long)]
        sheet: Option<String>,
        /// Markdown 公式投影策略。
        #[arg(long, value_enum, default_value_t = CliMarkdownFormulaPolicy::Cached)]
        formula: CliMarkdownFormulaPolicy,
        /// Markdown 合并单元格投影策略。
        #[arg(long, value_enum, default_value_t = CliMarkdownMergePolicy::Anchor)]
        merge: CliMarkdownMergePolicy,
    },
    /// 重算公式缓存。
    Recalc {
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// 在工作簿显示值中做大小写不敏感的子串搜索。
    Grep {
        input: PathBuf,
        pattern: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },
    /// 统计一列的数据概况（计数/空值/数值/文本/去重与聚合）。
    Profile {
        input: PathBuf,
        column: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },
    /// 对工作簿数据求值单条公式（数组结果以网格返回）。
    Eval {
        input: PathBuf,
        formula: String,
        /// 相对引用的单元格上下文（默认 Sheet1!A1）。
        #[arg(long)]
        at: Option<String>,
    },
    /// 查询单元格的数字格式（DATE/NUMBER/GENERAL 或格式代码）。
    Format {
        input: PathBuf,
        cell: String,
    },
    /// 按谓词过滤数据行（如 `amount>1000` 或 `name~ali`）。
    Filter {
        input: PathBuf,
        predicate: String,
        #[arg(long, short = 's')]
        sheet: Option<String>,
    },
    /// 输出机器可读能力清单。
    Capabilities,
    /// 输出指定命令的 JSON Schema。
    Schema {
        #[arg(long = "command")]
        target: String,
    },
    /// 捕获尚未实现的已规划命令，并返回稳定错误码。
    #[command(external_subcommand)]
    External(Vec<String>),
}

/// CLI 层可选输出格式。
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliOutputFormat {
    #[default]
    Json,
    Csv,
    Tsv,
    #[value(alias = "md")]
    Markdown,
    Html,
}

/// Markdown 导出执行模式。
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliMarkdownMode {
    #[default]
    Auto,
    Event,
    Workbook,
}

/// Markdown 公式投影策略。
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliMarkdownFormulaPolicy {
    #[default]
    Cached,
    Expression,
    Both,
}

/// Markdown 合并单元格投影策略。
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliMarkdownMergePolicy {
    #[default]
    Anchor,
    Repeat,
    Html,
    Error,
}

/// Markdown 导入类型推断策略。
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliMarkdownTypeInference {
    Text,
    #[default]
    Conservative,
    Aggressive,
}
