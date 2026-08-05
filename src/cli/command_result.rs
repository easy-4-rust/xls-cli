use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CommandName, SchemaVersion};

/// 命令生成或计划生成的文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// 目标文件路径。
    pub path: PathBuf,
    /// dry-run 时为 `false`。
    pub written: bool,
}

/// 稳定、可序列化的命令结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResult {
    /// JSON 协议版本。
    pub schema_version: SchemaVersion,
    /// 已执行命令。
    pub command: CommandName,
    /// 命令特定数据。
    pub data: Value,
    /// 生成或计划生成的文件。
    pub files: Vec<GeneratedFile>,
    /// 非致命能力降级或兼容性提示。
    pub warnings: Vec<String>,
    /// 稳定的数值统计信息。
    pub stats: BTreeMap<String, u64>,
    /// 是否为 dry-run。
    pub dry_run: bool,
}

impl CommandResult {
    /// 创建结果对象。
    #[must_use]
    pub fn new(command: CommandName, data: Value, dry_run: bool) -> Self {
        Self {
            schema_version: SchemaVersion::current(),
            command,
            data,
            files: Vec::new(),
            warnings: Vec::new(),
            stats: BTreeMap::new(),
            dry_run,
        }
    }
}
