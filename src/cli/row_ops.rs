//! 行级快照/重写工具：自 terminal.rs 原样提取，供人类终端路径与结构化执行器共用。

use easyexcel::model::Cell;
use easyexcel::model::CellAddress;
use easyexcel::model::CellRange;
use easyexcel::model::Sheet;

/// 一行的快照：各列单元格与样式索引。
pub(crate) type RowSnap = (Vec<Option<Cell>>, Vec<Option<u32>>);

/// 快照数据行 `start..end`（不含 end）及其单元格与样式。
#[allow(
    clippy::cast_possible_truncation,
    reason = "列数在本库上限内远小于 u32::MAX"
)]
pub(crate) fn snapshot_rows(sheet: &Sheet, start: u32, end: u32, cols: u32) -> Vec<RowSnap> {
    (start..end)
        .map(|r| {
            let cells = (0..cols).map(|c| sheet.get(r, c).cloned()).collect();
            let styles = (0..cols).map(|c| sheet.style_at(r, c)).collect();
            (cells, styles)
        })
        .collect()
}

/// 清空数据行 `start..end` 后，把 `snap` 从 `start` 起连续写回。
#[allow(
    clippy::cast_possible_truncation,
    reason = "行/列号在本库上限内远小于 u32::MAX"
)]
pub(crate) fn rewrite_rows(sheet: &mut Sheet, start: u32, end: u32, cols: u32, snap: Vec<RowSnap>) {
    if cols == 0 || end <= start {
        return;
    }
    let range = CellRange::new(
        CellAddress::new(start, 0),
        CellAddress::new(end - 1, cols - 1),
    );
    sheet.clear_range(range);
    for (i, (cells, styles)) in snap.into_iter().enumerate() {
        let r = start + i as u32;
        for c in 0..cols as usize {
            if let Some(cell) = &cells[c] {
                sheet.set(r, c as u32, cell.clone());
            }
            if let Some(si) = styles[c] {
                sheet.set_style(r, c as u32, si);
            }
        }
    }
}

/// 复制一行（单元格 + 样式索引）；样式索引假定两侧共享样式表。
pub(crate) fn copy_row(src: &Sheet, dst: &mut Sheet, src_r: u32, dst_r: u32, cols: u32) {
    for c in 0..cols {
        if let Some(cell) = src.get(src_r, c) {
            dst.set(dst_r, c, cell.clone());
        }
        if let Some(si) = src.style_at(src_r, c) {
            dst.set_style(dst_r, c, si);
        }
    }
}

/// 快照行某列的显示字符串。
pub(crate) fn display_of(cells: &[Option<Cell>], col: u32) -> String {
    cells
        .get(col as usize)
        .and_then(|o| o.as_ref())
        .map(|cell| cell.value().to_display_string())
        .unwrap_or_default()
}

/// 两个显示字符串可解析为数字时按数值比较，否则按字典序。
pub(crate) fn cmp_values(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.cmp(b),
    }
}
