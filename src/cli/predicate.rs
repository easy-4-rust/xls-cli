//! `filter` 谓词 DSL：`<col> <op> <rhs>` 或 `<col>:number|text`。
//!
//! 自 terminal.rs 原样提取，供人类终端路径与结构化执行器共用。

use anyhow::{Result, bail};

use easyexcel::model::Workbook;
use easyexcel::model::value::CellValue;

/// 比较运算符。
#[derive(Clone, Copy)]
pub(crate) enum PredOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `~`（显示值包含）
    Contains,
    /// `:number`
    IsNumber,
    /// `:text`
    IsText,
}

/// 解析后的 filter 谓词。
pub(crate) struct Predicate {
    /// 列规格（表头名或列字母）。
    pub(crate) col: String,
    /// 运算符。
    op: PredOp,
    /// 右侧字面量。
    rhs: String,
}

impl Predicate {
    /// 解析谓词字符串。
    pub(crate) fn parse(s: &str) -> Result<Predicate> {
        // Type predicates: `<col>:number` / `<col>:text`.
        if let Some((c, kind)) = s.split_once(':') {
            match kind.trim().to_ascii_lowercase().as_str() {
                "number" => {
                    return Ok(Predicate {
                        col: c.trim().to_string(),
                        op: PredOp::IsNumber,
                        rhs: String::new(),
                    });
                }
                "text" => {
                    return Ok(Predicate {
                        col: c.trim().to_string(),
                        op: PredOp::IsText,
                        rhs: String::new(),
                    });
                }
                _ => {}
            }
        }
        // Comparison operators, longest symbols first.
        for (sym, op) in [
            (">=", PredOp::Ge),
            ("<=", PredOp::Le),
            ("!=", PredOp::Ne),
            ("==", PredOp::Eq),
            ("~", PredOp::Contains),
            (">", PredOp::Gt),
            ("<", PredOp::Lt),
        ] {
            if let Some(pos) = s.find(sym) {
                let col = s[..pos].trim().to_string();
                let rhs = s[pos + sym.len()..].trim().to_string();
                if col.is_empty() {
                    bail!("predicate is missing a column: '{s}'");
                }
                return Ok(Predicate { col, op, rhs });
            }
        }
        bail!("could not parse predicate: '{s}'")
    }

    /// 判断某行某列是否满足谓词（数值比较优先，回退显示值字典序）。
    pub(crate) fn matches(&self, workbook: &Workbook, sheet_idx: usize, row: u32, col: u32) -> bool {
        use std::cmp::Ordering;
        let cell = workbook.sheets[sheet_idx].value(row, col);
        match self.op {
            PredOp::IsNumber => matches!(cell, CellValue::Number(_)),
            PredOp::IsText => matches!(cell, CellValue::Text(_)),
            PredOp::Contains => workbook
                .display_cell(sheet_idx, row, col)
                .to_lowercase()
                .contains(&self.rhs.to_lowercase()),
            _ => {
                let numeric = match (&cell, self.rhs.parse::<f64>()) {
                    (CellValue::Number(left), Ok(right)) => Some((left, right)),
                    _ => None,
                };
                let Some((left, right)) = numeric else {
                    let display = workbook.display_cell(sheet_idx, row, col);
                    let (actual, expected) = (display.as_str(), self.rhs.as_str());
                    return match self.op {
                        PredOp::Eq => actual == expected,
                        PredOp::Ne => actual != expected,
                        PredOp::Gt => actual > expected,
                        PredOp::Ge => actual >= expected,
                        PredOp::Lt => actual < expected,
                        PredOp::Le => actual <= expected,
                        _ => false,
                    };
                };
                let order = left.partial_cmp(&right).unwrap_or(Ordering::Equal);
                match self.op {
                    PredOp::Eq => order == Ordering::Equal,
                    PredOp::Ne => order != Ordering::Equal,
                    PredOp::Gt => order == Ordering::Greater,
                    PredOp::Ge => order != Ordering::Less,
                    PredOp::Lt => order == Ordering::Less,
                    PredOp::Le => order != Ordering::Greater,
                    _ => false,
                }
            }
        }
    }
}
