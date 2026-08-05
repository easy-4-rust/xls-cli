use crate::{CommandError, CommandRequest, CommandResult, ExecutionContext};

/// 可由 CLI、npm 包、智能体或 Rust 应用复用的命令执行边界。
pub trait CommandExecutor {
    /// 执行一个带类型请求。
    ///
    /// # Errors
    ///
    /// 参数、资源、安全策略、读写或后端能力不满足时返回稳定 [`CommandError`]。
    fn execute(
        &self,
        request: CommandRequest,
        context: &ExecutionContext,
    ) -> Result<CommandResult, CommandError>;
}
