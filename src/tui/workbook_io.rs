//! TUI 工作簿会话的读写适配。
//!
//! 交互式会话中的 Ctrl+S 是用户明确发起的覆盖动作，因此保存已关联路径时
//! 使用 [`OverwritePolicy::Replace`]；文件解析和原子写入仍复用 CLI 的统一
//! 资源限制、格式识别与错误转换。

use std::path::Path;

use easyexcel::model::Workbook;

use crate::cli::{ExecutionContext, OverwritePolicy, open_workbook, save_workbook};

/// 使用统一资源限制打开工作簿。
pub(super) fn open_path(path: &Path) -> anyhow::Result<Workbook> {
    open_workbook(path, &ExecutionContext::new()).map_err(anyhow::Error::new)
}

/// 原子保存工作簿；该调用仅由用户主动保存操作触发。
pub(super) fn save_path(workbook: &Workbook, path: &Path) -> anyhow::Result<()> {
    let context = ExecutionContext::new().with_overwrite(OverwritePolicy::Replace);
    save_workbook(workbook, path, &context)
        .map(|_| ())
        .map_err(anyhow::Error::new)
}
