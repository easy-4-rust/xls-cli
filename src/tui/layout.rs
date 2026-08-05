//! Pure grid-geometry helpers: column widths, scroll-to-keep-visible maths, and
//! cursor-movement clamping. No terminal/UI dependency, so unit-testable.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 网格几何算法保持边界与滚动语义"
)]

use easyexcel::model::Sheet;

/// Default visible column width in character cells when a column has no explicit
/// width set on the sheet.
pub const DEFAULT_COL_WIDTH: u16 = 9;
/// Clamp bounds for a rendered column width.
pub const MIN_COL_WIDTH: u16 = 3;
pub const MAX_COL_WIDTH: u16 = 60;
/// Width of the row-header gutter (enough for ~6-digit row numbers + padding).
pub const ROW_HEADER_WIDTH: u16 = 6;
/// Height of the column-header row.
pub const HEADER_HEIGHT: u16 = 1;

/// The visible width (in terminal cells) of column `col` on `sheet`.
///
/// Excel column width is in "character units"; we round it to a cell count and
/// clamp to a sane range. Hidden columns collapse to width 0.
pub fn col_width(sheet: &Sheet, col: u32) -> u16 {
    if let Some(info) = sheet.columns.get(&col) {
        if info.hidden {
            return 0;
        }
        if let Some(w) = info.width {
            return (w.round() as i64).clamp(MIN_COL_WIDTH as i64, MAX_COL_WIDTH as i64) as u16;
        }
    }
    DEFAULT_COL_WIDTH
}

/// Compute the left scroll offset (first visible column) needed so that `cursor`
/// column is fully visible, given the current `offset` and the available grid
/// width (excluding the row-header gutter). Columns are variable width.
///
/// `frozen_cols` columns are always pinned at the left and never scroll; the
/// returned offset is the first *non-frozen* scrolling column and will never be
/// less than `frozen_cols`.
pub fn scroll_col_to_visible(
    sheet: &Sheet,
    cursor_col: u32,
    mut offset: u32,
    grid_width: u16,
    frozen_cols: u32,
) -> u32 {
    if cursor_col < frozen_cols {
        // Cursor is inside the frozen region; the scroll region is unaffected.
        return offset.max(frozen_cols);
    }
    offset = offset.max(frozen_cols);

    // Width consumed by the pinned frozen columns.
    let mut frozen_w: u16 = 0;
    for c in 0..frozen_cols {
        frozen_w = frozen_w.saturating_add(col_width(sheet, c));
    }
    let avail = grid_width.saturating_sub(frozen_w);

    if cursor_col < offset {
        return cursor_col;
    }
    // Walk forward: ensure [offset..=cursor] fits within `avail`; if not, push
    // the offset rightward until it does.
    loop {
        let mut used: u16 = 0;
        let mut c = offset;
        let mut fits = false;
        while c <= cursor_col {
            used = used.saturating_add(col_width(sheet, c));
            if used > avail {
                break;
            }
            if c == cursor_col {
                fits = true;
                break;
            }
            c += 1;
        }
        if fits || offset >= cursor_col {
            return offset;
        }
        offset += 1;
    }
}

/// Compute the top scroll offset (first visible row) so that `cursor_row` is
/// visible given `offset`, the number of visible body rows, and `frozen_rows`.
pub fn scroll_row_to_visible(
    cursor_row: u32,
    mut offset: u32,
    visible_rows: u16,
    frozen_rows: u32,
) -> u32 {
    if cursor_row < frozen_rows {
        return offset.max(frozen_rows);
    }
    offset = offset.max(frozen_rows);
    let body = (visible_rows as u32).saturating_sub(frozen_rows).max(1);
    if cursor_row < offset {
        offset = cursor_row;
    } else if cursor_row >= offset + body {
        offset = cursor_row + 1 - body;
    }
    offset.max(frozen_rows)
}

/// Clamp a cursor move by `(d_row, d_col)` so it never goes below 0. There is no
/// hard upper bound (spreadsheets are effectively unbounded) but we cap at
/// Excel's limits to avoid pathological values.
pub const MAX_ROW: u32 = 1_048_575;
pub const MAX_COL: u32 = 16_383;

pub fn move_cursor(row: u32, col: u32, d_row: i64, d_col: i64) -> (u32, u32) {
    let nr = (row as i64 + d_row).clamp(0, MAX_ROW as i64) as u32;
    let nc = (col as i64 + d_col).clamp(0, MAX_COL as i64) as u32;
    (nr, nc)
}

/// Find the next "data edge" in a direction (Ctrl+Arrow behaviour): if the
/// current cell is non-empty and the next is non-empty, jump to the last
/// contiguous non-empty cell; otherwise jump to the next non-empty cell. Mirrors
/// Excel's Ctrl+Arrow. `d` is -1 or +1 along the chosen axis.
pub fn data_edge_row(sheet: &Sheet, row: u32, col: u32, d: i64) -> u32 {
    edge(row, MAX_ROW, d, |r| !is_empty(sheet, r, col))
}

pub fn data_edge_col(sheet: &Sheet, row: u32, col: u32, d: i64) -> u32 {
    edge(col, MAX_COL, d, |c| !is_empty(sheet, row, c))
}

fn is_empty(sheet: &Sheet, row: u32, col: u32) -> bool {
    matches!(
        sheet.get(row, col),
        None | Some(easyexcel::model::Cell::Empty)
    )
}

fn edge(start: u32, max: u32, d: i64, occupied: impl Fn(u32) -> bool) -> u32 {
    let step = |v: u32| -> Option<u32> {
        let n = v as i64 + d;
        if n < 0 || n > max as i64 {
            None
        } else {
            Some(n as u32)
        }
    };
    let here = occupied(start);
    let next = match step(start) {
        Some(n) => n,
        None => return start,
    };
    if here && occupied(next) {
        // In a run: go to the end of the contiguous run.
        let mut cur = next;
        while let Some(n) = step(cur) {
            if occupied(n) {
                cur = n;
            } else {
                break;
            }
        }
        cur
    } else {
        // Skip blanks to the next occupied cell (or the boundary).
        let mut cur = next;
        loop {
            if occupied(cur) {
                return cur;
            }
            match step(cur) {
                Some(n) => cur = n,
                None => return cur,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel::model::{Cell, Sheet};

    fn sheet_with(cells: &[(u32, u32)]) -> Sheet {
        let mut s = Sheet::new("t");
        for &(r, c) in cells {
            s.set(r, c, Cell::Number(1.0));
        }
        s
    }

    #[test]
    fn default_width_when_unset() {
        let s = Sheet::new("t");
        assert_eq!(col_width(&s, 0), DEFAULT_COL_WIDTH);
    }

    #[test]
    fn width_clamped() {
        let mut s = Sheet::new("t");
        s.columns.insert(
            0,
            easyexcel_model::model::ColInfo {
                width: Some(200.0),
                hidden: false,
                style: None,
            },
        );
        assert_eq!(col_width(&s, 0), MAX_COL_WIDTH);
        s.columns.insert(
            1,
            easyexcel_model::model::ColInfo {
                width: Some(1.0),
                hidden: false,
                style: None,
            },
        );
        assert_eq!(col_width(&s, 1), MIN_COL_WIDTH);
    }

    #[test]
    fn hidden_column_zero_width() {
        let mut s = Sheet::new("t");
        s.columns.insert(
            0,
            easyexcel_model::model::ColInfo {
                width: Some(10.0),
                hidden: true,
                style: None,
            },
        );
        assert_eq!(col_width(&s, 0), 0);
    }

    #[test]
    fn move_cursor_clamps_at_zero() {
        assert_eq!(move_cursor(0, 0, -1, -1), (0, 0));
        assert_eq!(move_cursor(5, 5, -2, 3), (3, 8));
    }

    #[test]
    fn row_scroll_keeps_cursor_visible() {
        // 10 visible rows, no frozen. Cursor below window pushes offset down.
        assert_eq!(scroll_row_to_visible(15, 0, 10, 0), 6);
        // Cursor above window pulls offset up.
        assert_eq!(scroll_row_to_visible(3, 10, 10, 0), 3);
        // Cursor already visible: offset unchanged.
        assert_eq!(scroll_row_to_visible(5, 2, 10, 0), 2);
    }

    #[test]
    fn col_scroll_keeps_cursor_visible() {
        let s = Sheet::new("t"); // all default width 9
        // grid width 27 => 3 columns visible. Cursor at col 5 -> offset 3.
        let off = scroll_col_to_visible(&s, 5, 0, 27, 0);
        assert_eq!(off, 3);
        // Cursor at col 0 with offset 5 pulls back to 0.
        assert_eq!(scroll_col_to_visible(&s, 0, 5, 27, 0), 0);
    }

    #[test]
    fn data_edge_jumps_over_blanks() {
        // Data at rows 0,1,2 then blank then 5.
        let s = sheet_with(&[(0, 0), (1, 0), (2, 0), (5, 0)]);
        // From row 0 (in a run) down -> end of run = row 2.
        assert_eq!(data_edge_row(&s, 0, 0, 1), 2);
        // From row 2 (run end, next blank) down -> next data row 5.
        assert_eq!(data_edge_row(&s, 2, 0, 1), 5);
        // From a blank row 3 up -> previous data row 2.
        assert_eq!(data_edge_row(&s, 3, 0, -1), 2);
    }
}
