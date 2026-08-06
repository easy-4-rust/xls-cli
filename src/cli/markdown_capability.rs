use serde::{Deserialize, Serialize};

/// Markdown 投影层的机器可读能力清单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownCapability {
    /// 是否支持 Markdown 导入。
    pub import: bool,
    /// 是否支持 Markdown 导出。
    pub export: bool,
    /// 可选输出档案。
    pub profiles: Vec<String>,
    /// 支持事件模式导出的工作簿格式。
    pub streaming_export: Vec<String>,
    /// 支持完整工作簿模式导出的格式。
    pub workbook_export: Vec<String>,
    /// 公式投影策略。
    pub formula_policies: Vec<String>,
    /// 合并单元格投影策略。
    pub merge_policies: Vec<String>,
}

impl Default for MarkdownCapability {
    fn default() -> Self {
        Self {
            import: true,
            export: true,
            profiles: vec!["agent-stable".to_owned(), "human-readable".to_owned()],
            streaming_export: vec!["xlsx".to_owned(), "csv".to_owned()],
            workbook_export: vec!["xls".to_owned(), "xlsx".to_owned(), "csv".to_owned()],
            formula_policies: vec![
                "cached".to_owned(),
                "expression".to_owned(),
                "both".to_owned(),
            ],
            merge_policies: vec![
                "anchor".to_owned(),
                "repeat".to_owned(),
                "html".to_owned(),
                "error".to_owned(),
            ],
        }
    }
}
