use serde::{Deserialize, Serialize};

/// 稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ErrorCode {
    /// 请求参数无效。
    InvalidArgument,
    /// 输入格式无法识别或不支持。
    UnsupportedFormat,
    /// 命令尚未实现。
    UnsupportedCommand,
    /// 文件不存在。
    FileNotFound,
    /// 安全策略拒绝覆盖。
    OverwriteDenied,
    /// 工作表不存在。
    SheetNotFound,
    /// 资源限制被触发。
    ResourceLimit,
    /// 工作簿读取失败。
    ReadFailed,
    /// 工作簿写入失败。
    WriteFailed,
    /// 查询失败。
    QueryFailed,
    /// 内部错误。
    Internal,
}

impl ErrorCode {
    /// 返回固定协议字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            Self::UnsupportedCommand => "UNSUPPORTED_COMMAND",
            Self::FileNotFound => "FILE_NOT_FOUND",
            Self::OverwriteDenied => "OVERWRITE_DENIED",
            Self::SheetNotFound => "SHEET_NOT_FOUND",
            Self::ResourceLimit => "RESOURCE_LIMIT",
            Self::ReadFailed => "READ_FAILED",
            Self::WriteFailed => "WRITE_FAILED",
            Self::QueryFailed => "QUERY_FAILED",
            Self::Internal => "INTERNAL",
        }
    }
}

/// 可序列化且不包含敏感输入的命令错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}", code = .code.as_str())]
pub struct CommandError {
    /// 稳定错误码。
    pub code: ErrorCode,
    /// 面向用户的安全消息。
    pub message: String,
    /// 面向诊断的非敏感上下文。
    pub diagnostic: Option<String>,
    /// 调用方是否可在状态变化后重试。
    pub retryable: bool,
}

impl CommandError {
    /// 创建不可重试错误。
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostic: None,
            retryable: false,
        }
    }

    /// 附加非敏感诊断信息。
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(diagnostic.into());
        self
    }
}
