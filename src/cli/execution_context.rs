use std::fmt;

use easyexcel::io::ResourceLimits;

/// 执行模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecutionMode {
    /// 实际落盘。
    #[default]
    Apply,
    /// 只验证和生成变更计划，不写文件。
    DryRun,
}

/// 已存在目标文件的处理策略。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum OverwritePolicy {
    /// 拒绝覆盖。
    #[default]
    Deny,
    /// 显式允许替换。
    Replace,
}

/// 不在 `Debug` 中泄露内容的敏感字符串。
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    /// 包装密码或令牌。
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 仅在格式后端调用点暴露敏感内容。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

/// 一次命令执行的安全策略、资源限制和敏感输入。
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    mode: ExecutionMode,
    overwrite: OverwritePolicy,
    limits: ResourceLimits,
    password: Option<SecretString>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Apply,
            overwrite: OverwritePolicy::Deny,
            limits: ResourceLimits::default(),
            password: None,
        }
    }
}

impl ExecutionContext {
    /// 创建默认安全上下文。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置执行模式。
    #[must_use]
    pub const fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.mode = mode;
        self
    }

    /// 设置覆盖策略。
    #[must_use]
    pub const fn with_overwrite(mut self, overwrite: OverwritePolicy) -> Self {
        self.overwrite = overwrite;
        self
    }

    /// 设置资源限制。
    #[must_use]
    pub const fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// 设置密码。调用方应从 stdin、环境变量或安全文件描述符获取。
    #[must_use]
    pub fn with_password(mut self, password: SecretString) -> Self {
        self.password = Some(password);
        self
    }

    /// 返回执行模式。
    #[must_use]
    pub const fn mode(&self) -> ExecutionMode {
        self.mode
    }

    /// 返回覆盖策略。
    #[must_use]
    pub const fn overwrite(&self) -> OverwritePolicy {
        self.overwrite
    }

    /// 返回资源限制。
    #[must_use]
    pub const fn limits(&self) -> ResourceLimits {
        self.limits
    }

    /// 返回密码引用。
    #[must_use]
    pub fn password(&self) -> Option<&SecretString> {
        self.password.as_ref()
    }
}
