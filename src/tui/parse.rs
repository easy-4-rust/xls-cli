//! Pure input parsing: turn a string the user typed into a [`Cell`], and
//! related helpers. Kept free of any UI/terminal dependency so it can be unit
//! tested in isolation.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 输入解析规则保持既有单元格推断语义"
)]

use easyexcel::model::{Cell, CellValue};

/// Parse a string the user typed into a cell value.
///
/// Rules (mirroring spreadsheet behaviour):
/// * empty string → `Empty`
/// * starts with `=` → `Formula { expr, cached: Empty }` (recalc fills cached)
/// * `TRUE` / `FALSE` (case-insensitive) → `Bool`
/// * parses as a number → `Number`
/// * otherwise → `Text`
pub fn parse_input(s: &str) -> Cell {
    if s.is_empty() {
        return Cell::Empty;
    }
    if let Some(rest) = s.strip_prefix('=') {
        // Keep the leading '=' out of the stored expr? The engine strips a
        // leading '=' itself, so storing with or without is fine; we store the
        // canonical form *with* '=' so formula_text round-trips like Excel.
        let _ = rest;
        return Cell::Formula {
            expr: s.to_string(),
            cached: CellValue::Empty,
        };
    }
    if s.eq_ignore_ascii_case("true") {
        return Cell::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Cell::Bool(false);
    }
    if let Some(n) = parse_number(s) {
        return Cell::Number(n);
    }
    Cell::Text(s.to_string())
}

/// Parse a number the way a spreadsheet entry field would: plain decimals,
/// optional sign, scientific notation, a trailing `%` (divides by 100), and
/// thousands separators (`6,000.00`, `1,51,302.63`) which are stripped so the
/// value is stored as a real number — exporting correctly to Excel. Rejects
/// strings with surrounding junk so that e.g. `"1 2"` stays text.
fn parse_number(s: &str) -> Option<f64> {
    easyexcel::formula::formula::coerce::parse_number_text(s)
}

/// The seed text shown in the editor when editing an existing cell: the formula
/// source for formula cells, otherwise the raw stored value rendered plainly.
pub fn edit_seed(cell: Option<&Cell>) -> String {
    match cell {
        None | Some(Cell::Empty) => String::new(),
        Some(Cell::Formula { expr, .. }) => expr.clone(),
        Some(Cell::Number(n)) => easyexcel::model::value::format_number_general(*n),
        Some(Cell::Text(s)) => s.clone(),
        Some(Cell::Bool(b)) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Some(Cell::Error(e)) => e.as_str().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blank() {
        assert_eq!(parse_input(""), Cell::Empty);
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_input("42"), Cell::Number(42.0));
        assert_eq!(parse_input("-3.5"), Cell::Number(-3.5));
        assert_eq!(parse_input("1e3"), Cell::Number(1000.0));
        assert_eq!(parse_input("50%"), Cell::Number(0.5));
        // Thousands-separated numbers become real numbers (no comma stored).
        assert_eq!(parse_input("6,000.00"), Cell::Number(6000.0));
        assert_eq!(parse_input("1,51,302.63"), Cell::Number(151302.63));
    }

    #[test]
    fn parses_bools_case_insensitive() {
        assert_eq!(parse_input("TRUE"), Cell::Bool(true));
        assert_eq!(parse_input("true"), Cell::Bool(true));
        assert_eq!(parse_input("False"), Cell::Bool(false));
    }

    #[test]
    fn parses_formula() {
        match parse_input("=A1+1") {
            Cell::Formula { expr, cached } => {
                assert_eq!(expr, "=A1+1");
                assert_eq!(cached, CellValue::Empty);
            }
            other => panic!("expected formula, got {other:?}"),
        }
    }

    #[test]
    fn parses_text_fallback() {
        assert_eq!(parse_input("hello"), Cell::Text("hello".into()));
        // Numbers with junk stay text.
        assert_eq!(parse_input("1 2"), Cell::Text("1 2".into()));
        // A leading apostrophe-free word that looks boolean-ish but isn't.
        assert_eq!(parse_input("TRU"), Cell::Text("TRU".into()));
    }

    #[test]
    fn edit_seed_round_trips() {
        assert_eq!(edit_seed(None), "");
        assert_eq!(edit_seed(Some(&Cell::Number(2.5))), "2.5");
        assert_eq!(edit_seed(Some(&Cell::Text("hi".into()))), "hi");
        assert_eq!(edit_seed(Some(&Cell::Bool(true))), "TRUE");
        assert_eq!(
            edit_seed(Some(&Cell::Formula {
                expr: "=SUM(A1:A2)".into(),
                cached: CellValue::Empty,
            })),
            "=SUM(A1:A2)"
        );
    }
}
