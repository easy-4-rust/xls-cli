use std::fmt;

use serde::{Deserialize, Serialize};

/// JSON 请求/响应协议版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// 破坏性变化版本。
    pub major: u16,
    /// 向后兼容扩展版本。
    pub minor: u16,
}

impl SchemaVersion {
    /// 当前协议版本 1.0。
    #[must_use]
    pub const fn current() -> Self {
        Self { major: 1, minor: 0 }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}
