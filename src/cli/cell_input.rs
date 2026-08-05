use easyexcel::model::{Cell, CellValue};
use serde::{Deserialize, Serialize};

/// 命令协议中的单元格输入值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CellInput {
    /// 空值。
    Empty,
    /// 数值。
    Number(f64),
    /// 文本。
    Text(String),
    /// 布尔值。
    Bool(bool),
    /// 公式文本。
    Formula(String),
}

impl CellInput {
    /// 转换为工作簿模型单元格。
    #[must_use]
    pub fn into_cell(self) -> Cell {
        match self {
            Self::Empty => Cell::Empty,
            Self::Number(value) => Cell::Number(value),
            Self::Text(value) => Cell::Text(value),
            Self::Bool(value) => Cell::Bool(value),
            Self::Formula(expr) => Cell::Formula {
                expr,
                cached: CellValue::Empty,
            },
        }
    }
}
