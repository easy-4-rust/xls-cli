use serde::{Deserialize, Serialize};

use crate::{CommandName, MarkdownCapability, SchemaVersion};

/// 单项能力状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CapabilityStatus {
    /// 当前构建中可用并有实现。
    Supported,
    /// 已规划但当前构建不支持。
    Unsupported,
    /// 可用但存在明确限制。
    Partial,
}

/// 一项命令能力声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandCapability {
    /// 命令名。
    pub command: CommandName,
    /// 当前状态。
    pub status: CapabilityStatus,
    /// 限制说明。
    pub notes: Vec<String>,
}

/// 当前构建可机器读取的能力清单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// 协议版本。
    pub schema_version: SchemaVersion,
    /// 支持读取的文件格式。
    pub read_formats: Vec<String>,
    /// 支持写入的文件格式。
    pub write_formats: Vec<String>,
    /// 命令能力。
    pub commands: Vec<CommandCapability>,
    /// 读取模式。
    pub read_modes: Vec<String>,
    /// 写入模式。
    pub write_modes: Vec<String>,
    /// Markdown 投影层能力。
    pub markdown: MarkdownCapability,
}

impl CapabilityManifest {
    /// 构造与本 crate 实现一致的能力清单。
    #[must_use]
    pub fn current() -> Self {
        const SUPPORTED: &[CommandName] = &[
            CommandName::Info,
            CommandName::Get,
            CommandName::Head,
            CommandName::Tail,
            CommandName::Set,
            CommandName::Clear,
            CommandName::Fill,
            CommandName::InsertRows,
            CommandName::DeleteRows,
            CommandName::InsertColumns,
            CommandName::DeleteColumns,
            CommandName::New,
            CommandName::AddSheet,
            CommandName::DeleteSheet,
            CommandName::RenameSheet,
            CommandName::Query,
            CommandName::Convert,
            CommandName::Import,
            CommandName::Export,
            CommandName::Recalc,
            CommandName::Capabilities,
            CommandName::Schema,
            CommandName::Grep,
            CommandName::Profile,
        ];
        const TERMINAL_ONLY: &[CommandName] = &[
            CommandName::Open,
            CommandName::Copy,
            CommandName::Move,
            CommandName::Append,
            CommandName::Filter,
            CommandName::Sort,
            CommandName::Dedup,
            CommandName::Join,
            CommandName::Pivot,
            CommandName::Diff,
            CommandName::Format,
            CommandName::FormatSet,
            CommandName::ToNumber,
            CommandName::ToDate,
            CommandName::Style,
            CommandName::Autofit,
            CommandName::Batch,
            CommandName::Name,
            CommandName::Table,
            CommandName::Eval,
        ];
        let commands = SUPPORTED
            .iter()
            .map(|command| CommandCapability {
                command: *command,
                status: CapabilityStatus::Supported,
                notes: Vec::new(),
            })
            .chain(TERMINAL_ONLY.iter().map(|command| CommandCapability {
                command: *command,
                status: CapabilityStatus::Partial,
                notes: vec![
                    "交互终端命令已迁移并可执行；结构化 JSON 请求仍返回 UNSUPPORTED_COMMAND"
                        .to_owned(),
                ],
            }))
            .collect();
        Self {
            schema_version: SchemaVersion::current(),
            read_formats: vec![
                "xls".to_owned(),
                "xlsx".to_owned(),
                "csv".to_owned(),
                "markdown".to_owned(),
                "html".to_owned(),
                "json".to_owned(),
            ],
            write_formats: vec![
                "xls".to_owned(),
                "xlsx".to_owned(),
                "csv".to_owned(),
                "markdown".to_owned(),
                "html".to_owned(),
                "json".to_owned(),
            ],
            commands,
            read_modes: vec!["workbook".to_owned(), "xlsx-event".to_owned()],
            write_modes: vec!["generate".to_owned(), "round-trip".to_owned()],
            markdown: MarkdownCapability::default(),
        }
    }
}
