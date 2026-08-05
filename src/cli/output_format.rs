use serde::{Deserialize, Serialize};

/// 提取和导出时的结构化输出格式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum OutputFormat {
    /// JSON 数组。
    #[default]
    Json,
    /// 逗号分隔文本。
    Csv,
    /// 制表符分隔文本。
    Tsv,
    /// GitHub Flavored Markdown 表格。
    Markdown,
    /// 静态 HTML 表格。
    Html,
}
