//! `xls-cli` 的可复用 CLI 与 TUI 产品内核。
//!
//! [`cli`] 承载结构化命令、执行上下文、能力清单、终端参数与工作簿
//! 用例编排；[`tui`] 承载交互式电子表格。二进制入口仅负责启动产品边界。

pub mod cli;
pub mod tui;

pub use cli::{
    CapabilityManifest, CapabilityStatus, CellInput, CommandCapability, CommandError,
    CommandExecutor, CommandName, CommandRequest, CommandResult, CommandWarning,
    DefaultCommandExecutor, ErrorCode, ExecutionContext, ExecutionMode, GeneratedFile,
    MarkdownCapability, OutputFormat, OverwritePolicy, ResourceLimits, SchemaVersion, SecretString,
    command_schema,
};
