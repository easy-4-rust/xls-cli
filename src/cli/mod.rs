//! `xls-cli` 的命令行产品模块。
//!
//! 该模块同时承载稳定的命令协议、用例编排与终端参数适配；stdout、stderr
//! 和进程退出仍由 [`runner`] 统一控制，避免业务执行器直接操作进程边界。

mod args;
mod capability_manifest;
mod cell_input;
mod command_error;
mod command_executor;
mod command_name;
mod command_request;
mod command_result;
mod command_warning;
mod default_command_executor;
mod easyexcel_components;
mod execution_context;
mod markdown_capability;
mod output_format;
mod query;
mod render;
mod request;
mod runner;
mod schema;
mod schema_version;
mod selection;
mod stream;
mod terminal;
mod workbook_io;

pub(crate) use args::{
    Cli, CliMarkdownFormulaPolicy, CliMarkdownMergePolicy, CliMarkdownMode,
    CliMarkdownTypeInference, CliOutputFormat, Commands,
};
pub use capability_manifest::{CapabilityManifest, CapabilityStatus, CommandCapability};
pub use cell_input::CellInput;
pub use command_error::{CommandError, ErrorCode};
pub use command_executor::CommandExecutor;
pub use command_name::CommandName;
pub use command_request::CommandRequest;
pub use command_result::{CommandResult, GeneratedFile};
pub use command_warning::CommandWarning;
pub use default_command_executor::DefaultCommandExecutor;
pub use easyexcel::io::ResourceLimits;
pub use execution_context::{ExecutionContext, ExecutionMode, OverwritePolicy, SecretString};
pub use markdown_capability::MarkdownCapability;
pub use output_format::OutputFormat;
pub(crate) use request::into_request;
pub use runner::main;
pub use schema::command_schema;
pub use schema_version::SchemaVersion;
pub(crate) use workbook_io::{open_workbook, save_workbook};

#[cfg(test)]
mod tests;
