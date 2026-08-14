//! `pivot` 的聚合函数选择。

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// 聚合函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Aggregation {
    /// 求和。
    Sum,
    /// 计数。
    Count,
    /// 平均。
    Mean,
    /// 最小值。
    Min,
    /// 最大值。
    Max,
}

impl Aggregation {
    /// 返回稳定名称。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Count => "count",
            Self::Mean => "mean",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}
