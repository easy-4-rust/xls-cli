use std::fmt;

use serde::{Deserialize, Serialize};

/// 命令执行产生的结构化非致命警告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandWarning {
    /// 稳定警告码。
    pub code: String,
    /// 面向用户的警告消息。
    pub message: String,
    /// 可选工作表名称。
    pub sheet: Option<String>,
    /// 可选 A1 范围。
    pub range: Option<String>,
}

impl CommandWarning {
    /// 创建通用警告。
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            sheet: None,
            range: None,
        }
    }
}

impl fmt::Display for CommandWarning {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}
