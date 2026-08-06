//! Application state and pure (non-rendering) logic for the TUI: cursor,
//! selection, editing, undo/redo, clipboard, find/goto.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls TUI 状态机由交互回归约束，避免纯风格重写引入行为漂移"
)]

use std::path::PathBuf;

use easyexcel::formula::Engine;
use easyexcel::model::addr::col_index_to_letters;
use easyexcel::model::{Cell, CellAddress, CellRange, Sheet, Workbook};

use super::layout;
use super::parse::{edit_seed, parse_input};

/// How many empty rows/columns past the data extent the view may scroll, so the
/// last row/column isn't jammed against the viewport edge.
const MARGIN: u32 = 10;

/// Map a click `offset` along a scrollbar track of `track` cells to a scroll
/// position, centering the viewport's thumb on the click.
fn scroll_pos_from_track(offset: usize, track: usize, total: usize, visible: usize) -> usize {
    if track <= 1 || total <= visible {
        return 0;
    }
    let max_scroll = total - visible;
    // Fraction of the track clicked → fraction of the scrollable span. Dividing
    // by `track - 1` lets the first/last track cell reach 0 / max_scroll exactly.
    let denom = track - 1;
    ((offset * max_scroll + denom / 2) / denom).min(max_scroll)
}

/// A highlighted formula reference on the active sheet: normalized rectangle
/// (r0,c0,r1,c1) plus whether the edit caret is currently inside its text span.
pub type RefHighlight = (u32, u32, u32, u32, bool);

/// Scan formula `text` for unqualified cell/range references (e.g. `A1`,
/// `$B$2:C10`) and return them as highlight rectangles. The reference whose text
/// span contains `caret` (a character column) is flagged active. Sheet-qualified
/// references and text inside string literals are ignored. Pure so it can be
/// unit-tested without a terminal.
pub fn formula_ref_highlights(text: &str, caret: usize) -> Vec<RefHighlight> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if in_str {
            i += 1;
            continue;
        }
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let boundary = !matches!(prev, Some(p) if p.is_ascii_alphanumeric() || p == '_' || p == '!' || p == '$');
        if boundary
            && (ch == '$' || ch.is_ascii_alphabetic())
            && let Some((end, r0, c0, r1, c1)) = match_ref(&chars, i)
        {
            // Reject if it runs into an identifier, a function call `(`, or a
            // sheet qualifier `!` — those aren't current-sheet references.
            let after = chars.get(end);
            let bad_after = matches!(after, Some(a) if a.is_ascii_alphanumeric() || *a == '(' || *a == '!' || *a == '_');
            if !bad_after {
                let active = caret >= i && caret <= end;
                out.push((r0, c0, r1, c1, active));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Match a cell or range reference starting at `start`. Returns the end index
/// and the normalized rectangle (r0,c0,r1,c1).
fn match_ref(chars: &[char], start: usize) -> Option<(usize, u32, u32, u32, u32)> {
    let end1 = match_cell(chars, start)?;
    let mut end = end1;
    if chars.get(end) == Some(&':')
        && let Some(e2) = match_cell(chars, end + 1)
    {
        end = e2;
    }
    let token: String = chars[start..end].iter().collect();
    let range = CellRange::parse_a1(&token.to_ascii_uppercase())?;
    let (sr, er) = (
        range.start.row.min(range.end.row),
        range.start.row.max(range.end.row),
    );
    let (sc, ec) = (
        range.start.col.min(range.end.col),
        range.start.col.max(range.end.col),
    );
    Some((end, sr, sc, er, ec))
}

/// Match a single `$?LETTERS$?DIGITS` cell address; return the end index.
fn match_cell(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let letters = j;
    while chars.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        j += 1;
    }
    if j == letters {
        return None;
    }
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let digits = j;
    while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        j += 1;
    }
    if j == digits {
        return None;
    }
    Some(j)
}

/// The interaction mode the app is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Editing the active cell (the formula bar / inline editor is active).
    Edit,
    /// A modal text-input dialog is open (Go-to, Find, Save-as, command palette).
    Dialog,
}

/// Which dialog is active in [`Mode::Dialog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    GoTo,
    Find,
    SaveAs,
    /// `:`-style command palette.
    Command,
    /// Quit confirmation when there are unsaved changes.
    ConfirmQuit,
}

/// One undoable operation. We use coarse-grained commands that snapshot the
/// affected cells so undo/redo is a straightforward swap.
#[derive(Debug, Clone)]
pub enum UndoOp {
    /// A set of cell mutations: (sheet, row, col, old, new).
    Cells(Vec<(usize, u32, u32, Cell, Cell)>),
}

/// Rectangular clipboard payload (internal copy/paste buffer).
#[derive(Debug, Clone)]
pub struct Clip {
    pub rows: u32,
    pub cols: u32,
    /// Row-major cells.
    pub cells: Vec<Cell>,
}

/// The full application state.
pub struct App {
    pub wb: Workbook,
    pub engine: Engine,
    pub path: Option<PathBuf>,

    pub cursor_row: u32,
    pub cursor_col: u32,
    /// Selection anchor; selection is the rectangle between anchor and cursor.
    pub anchor_row: u32,
    pub anchor_col: u32,

    /// Top-left visible (scrolling) cell.
    pub scroll_row: u32,
    pub scroll_col: u32,
    /// When set, the next render scrolls the view to reveal the cursor. Cursor
    /// moves set it; direct view scrolling (mouse wheel) does not — so the wheel
    /// can move the view independently of the cursor.
    pub scroll_to_cursor: bool,

    pub mode: Mode,
    pub dialog: Option<Dialog>,

    pub modified: bool,
    pub status: String,
    pub should_quit: bool,

    pub undo_stack: Vec<UndoOp>,
    pub redo_stack: Vec<UndoOp>,
    pub clip: Option<Clip>,

    /// Last find query (for n/N repeat).
    pub last_find: String,

    /// Cached cell hit-test rects from the last render: (row, col, x, y, w, h).
    /// Used for mouse hit-testing. Updated by the renderer each frame.
    pub cell_rects: Vec<(u32, u32, u16, u16, u16, u16)>,
    /// Sheet-tab hit rects: (sheet_index, x, y, w).
    pub tab_rects: Vec<(usize, u16, u16, u16)>,
    /// Screen regions (text_start_x, y, width) where a click repositions the
    /// edit caret rather than moving the cell cursor. Set by the renderer in
    /// Edit mode (the formula-bar editor and the in-place cell editor).
    pub edit_rects: Vec<(u16, u16, u16)>,
    /// Grid body area in screen coords (x, y, w, h) for wheel/region tests.
    pub grid_area: (u16, u16, u16, u16),
    /// Vertical/horizontal scrollbar geometry for mouse interaction, set by the
    /// renderer: (x, y, len, total, visible). `None` when not drawn.
    pub vscroll: Option<(u16, u16, u16, usize, usize)>,
    pub hscroll: Option<(u16, u16, u16, usize, usize)>,
    /// Visible column-header rects for resize hit-testing: (col, x, width).
    pub col_header_rects: Vec<(u32, u16, u16)>,
    /// Active column resize drag: (col, left_x). `None` when not resizing.
    pub resizing_col: Option<(u32, u16)>,
    /// Last left-click (row, col, when) for double-click detection.
    pub last_click: Option<(u32, u32, std::time::Instant)>,
}

impl App {
    pub fn new(wb: Workbook, path: Option<PathBuf>) -> Self {
        let mut engine = Engine::new();
        let mut wb = wb;
        // Make sure cached formula values are fresh on open.
        engine.recalc(&mut wb);
        App {
            wb,
            engine,
            path,
            cursor_row: 0,
            cursor_col: 0,
            anchor_row: 0,
            anchor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            scroll_to_cursor: true,
            mode: Mode::Normal,
            dialog: None,
            modified: false,
            status:
                "arrows move · type/F2/double-click edit · Del clears · : commands · Ctrl+S save"
                    .to_string(),
            should_quit: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clip: None,
            last_find: String::new(),
            cell_rects: Vec::new(),
            tab_rects: Vec::new(),
            edit_rects: Vec::new(),
            grid_area: (0, 0, 0, 0),
            vscroll: None,
            hscroll: None,
            col_header_rects: Vec::new(),
            resizing_col: None,
            last_click: None,
        }
    }

    /// Record a left-click at `(row, col)` and report whether it completes a
    /// double-click (same cell within 400 ms). Used to start in-place editing.
    pub fn register_click(&mut self, row: u32, col: u32) -> bool {
        let now = std::time::Instant::now();
        let is_double = matches!(
            self.last_click,
            Some((r, c, t)) if r == row && c == col
                && now.duration_since(t) < std::time::Duration::from_millis(400)
        );
        // Reset after a double so a third click starts a fresh sequence.
        self.last_click = if is_double {
            None
        } else {
            Some((row, col, now))
        };
        is_double
    }

    pub fn active_sheet_idx(&self) -> usize {
        self.wb
            .active_sheet
            .min(self.wb.sheets.len().saturating_sub(1))
    }

    pub fn sheet(&self) -> &Sheet {
        &self.wb.sheets[self.active_sheet_idx()]
    }

    /// The current rectangular selection (normalised).
    pub fn selection(&self) -> CellRange {
        CellRange::new(
            CellAddress::new(self.anchor_row, self.anchor_col),
            CellAddress::new(self.cursor_row, self.cursor_col),
        )
    }

    pub fn has_range_selection(&self) -> bool {
        self.anchor_row != self.cursor_row || self.anchor_col != self.cursor_col
    }

    /// A1 label of the active cell, e.g. `B3`.
    pub fn cursor_label(&self) -> String {
        format!(
            "{}{}",
            col_index_to_letters(self.cursor_col),
            self.cursor_row + 1
        )
    }

    /// Formula-bar content for the active cell.
    pub fn formula_bar_text(&self) -> String {
        edit_seed(self.sheet().get(self.cursor_row, self.cursor_col))
    }

    /// Display string (number-format aware) for a cell on the active sheet.
    pub fn display(&self, row: u32, col: u32) -> String {
        self.wb.display_cell(self.active_sheet_idx(), row, col)
    }

    // ----- navigation -------------------------------------------------------

    /// Move the cursor by a delta. When `extend` is false the anchor follows
    /// (collapsing any selection); when true the anchor stays put.
    pub fn move_by(&mut self, d_row: i64, d_col: i64, extend: bool) {
        let (r, c) = layout::move_cursor(self.cursor_row, self.cursor_col, d_row, d_col);
        self.cursor_row = r;
        self.cursor_col = c;
        if !extend {
            self.anchor_row = r;
            self.anchor_col = c;
        }
        self.scroll_to_cursor = true;
    }

    pub fn move_to(&mut self, row: u32, col: u32, extend: bool) {
        self.cursor_row = row.min(layout::MAX_ROW);
        self.cursor_col = col.min(layout::MAX_COL);
        if !extend {
            self.anchor_row = self.cursor_row;
            self.anchor_col = self.cursor_col;
        }
        self.scroll_to_cursor = true;
    }

    /// Begin a column-width resize if `(col, row)` is on a column header's right
    /// border (the grab handle). Returns true if a resize started.
    pub fn try_begin_col_resize(&mut self, col: u16, row: u16) -> bool {
        if row != self.grid_area.1 {
            return false;
        }
        for &(c, x, w) in &self.col_header_rects {
            if w > 0 && col == x + w - 1 {
                self.resizing_col = Some((c, x));
                return true;
            }
        }
        false
    }

    /// Continue an in-progress column resize: set the column's width so its right
    /// edge follows the pointer at screen column `col`. Returns true while
    /// resizing.
    pub fn drag_col_resize(&mut self, col: u16) -> bool {
        let Some((c, x)) = self.resizing_col else {
            return false;
        };
        let new_w = (col.saturating_sub(x) + 1).clamp(layout::MIN_COL_WIDTH, layout::MAX_COL_WIDTH);
        let si = self.active_sheet_idx();
        self.wb.sheets[si].columns.entry(c).or_default().width = Some(new_w as f64);
        self.modified = true;
        true
    }

    /// End any in-progress mouse drag (column resize).
    pub fn end_drag(&mut self) {
        self.resizing_col = None;
    }

    /// Handle a mouse press/drag at `(col, row)` on a scrollbar. Returns true if
    /// it landed on one (and scrolled the view accordingly).
    pub fn scrollbar_drag(&mut self, col: u16, row: u16) -> bool {
        if let Some((x, y, len, total, visible)) = self.vscroll
            && col == x
            && row >= y
            && row < y + len
        {
            self.scroll_row =
                scroll_pos_from_track((row - y) as usize, len as usize, total, visible)
                    .min(layout::MAX_ROW as usize) as u32;
            let si = self.active_sheet_idx();
            self.scroll_row = self.scroll_row.max(self.wb.sheets[si].frozen.rows);
            return true;
        }
        if let Some((x, y, len, total, visible)) = self.hscroll
            && row == y
            && col >= x
            && col < x + len
        {
            self.scroll_col =
                scroll_pos_from_track((col - x) as usize, len as usize, total, visible)
                    .min(layout::MAX_COL as usize) as u32;
            let si = self.active_sheet_idx();
            self.scroll_col = self.scroll_col.max(self.wb.sheets[si].frozen.cols);
            return true;
        }
        false
    }

    /// If screen `(col, row)` falls inside an active edit field, return the
    /// caret character column the click maps to (offset from the field's text
    /// start). Used so clicking while editing repositions the caret.
    pub fn edit_caret_at(&self, col: u16, row: u16) -> Option<usize> {
        self.edit_rects.iter().find_map(|&(x, y, w)| {
            (row == y && col >= x && col < x + w).then(|| (col - x) as usize)
        })
    }

    /// True when screen `(col, row)` is over the horizontal scrollbar — used so
    /// the wheel scrolls horizontally there even on terminals that never emit
    /// dedicated horizontal-scroll events.
    pub fn pointer_on_hscroll(&self, col: u16, row: u16) -> bool {
        matches!(self.hscroll, Some((x, y, len, _, _)) if row == y && col >= x && col < x + len)
    }

    /// Scroll the view by a delta **without** moving the cursor (mouse wheel).
    /// Clamps within the frozen panes and the sheet's data extent (+ a small
    /// margin) so the view can't get lost in empty space.
    pub fn scroll_by(&mut self, d_row: i64, d_col: i64) {
        let si = self.active_sheet_idx();
        let frozen = self.wb.sheets[si].frozen;
        let (rows, cols) = self.wb.sheets[si].dimensions();
        // Allow scrolling a little past the last data cell for breathing room.
        let max_row = rows.saturating_add(MARGIN).min(layout::MAX_ROW);
        let max_col = cols.saturating_add(MARGIN).min(layout::MAX_COL);
        self.scroll_row =
            (self.scroll_row as i64 + d_row).clamp(frozen.rows as i64, max_row as i64) as u32;
        self.scroll_col =
            (self.scroll_col as i64 + d_col).clamp(frozen.cols as i64, max_col as i64) as u32;
    }

    pub fn jump_data_edge(&mut self, d_row: i64, d_col: i64, extend: bool) {
        let sheet = self.sheet();
        let (nr, nc) = if d_row != 0 {
            (
                layout::data_edge_row(sheet, self.cursor_row, self.cursor_col, d_row),
                self.cursor_col,
            )
        } else {
            (
                self.cursor_row,
                layout::data_edge_col(sheet, self.cursor_row, self.cursor_col, d_col),
            )
        };
        self.move_to(nr, nc, extend);
    }

    /// Adjust scroll so the cursor is visible within the given grid viewport.
    pub fn ensure_visible(&mut self, grid_width: u16, grid_height: u16) {
        let si = self.active_sheet_idx();
        let frozen = self.wb.sheets[si].frozen;
        let new_row = layout::scroll_row_to_visible(
            self.cursor_row,
            self.scroll_row,
            grid_height,
            frozen.rows,
        );
        let new_col = layout::scroll_col_to_visible(
            &self.wb.sheets[si],
            self.cursor_col,
            self.scroll_col,
            grid_width,
            frozen.cols,
        );
        self.scroll_row = new_row;
        self.scroll_col = new_col;
    }

    // ----- editing & undo ---------------------------------------------------

    /// Apply a batch of cell changes, recording them for undo, recalc, and mark
    /// modified. `changes` is (row, col, new) on the active sheet.
    pub fn apply_cells(&mut self, changes: Vec<(u32, u32, Cell)>) {
        if changes.is_empty() {
            return;
        }
        let si = self.active_sheet_idx();
        let sheet = &mut self.wb.sheets[si];
        let mut record = Vec::with_capacity(changes.len());
        for (r, c, new) in changes {
            let old = sheet.get(r, c).cloned().unwrap_or(Cell::Empty);
            if old == new {
                continue;
            }
            sheet.set(r, c, new.clone());
            record.push((si, r, c, old, new));
        }
        if record.is_empty() {
            return;
        }
        self.undo_stack.push(UndoOp::Cells(record));
        self.redo_stack.clear();
        self.modified = true;
        self.recalc();
    }

    /// Commit edited text into the active cell, advancing the cursor by `(dr,dc)`.
    pub fn commit_edit(&mut self, text: &str, d_row: i64, d_col: i64) {
        let cell = parse_input(text);
        self.apply_cells(vec![(self.cursor_row, self.cursor_col, cell)]);
        self.mode = Mode::Normal;
        if d_row != 0 || d_col != 0 {
            self.move_by(d_row, d_col, false);
        }
    }

    /// Convert text cells in the selection that look like numbers into real
    /// numbers (undoable). Returns the count converted. Fixes "numbers stored as
    /// text" that SUM/AVERAGE ignore.
    pub fn coerce_selection_to_numbers(&mut self) -> usize {
        let sel = self.selection();
        let si = self.active_sheet_idx();
        let mut changes = Vec::new();
        for (r, c) in sel.iter_cells() {
            if let Some(Cell::Text(s)) = self.wb.sheets[si].get(r, c)
                && let Some(n) = easyexcel::formula::formula::coerce::parse_number_text(s)
            {
                changes.push((r, c, Cell::Number(n)));
            }
        }
        let count = changes.len();
        self.apply_cells(changes);
        count
    }

    /// Clear all cells in the current selection.
    pub fn clear_selection(&mut self) {
        let sel = self.selection();
        let changes: Vec<(u32, u32, Cell)> =
            sel.iter_cells().map(|(r, c)| (r, c, Cell::Empty)).collect();
        self.apply_cells(changes);
    }

    pub fn undo(&mut self) {
        if let Some(op) = self.undo_stack.pop() {
            match op {
                UndoOp::Cells(changes) => {
                    let mut redo = Vec::with_capacity(changes.len());
                    for (si, r, c, old, new) in changes {
                        if let Some(sheet) = self.wb.sheets.get_mut(si) {
                            sheet.set(r, c, old.clone());
                        }
                        redo.push((si, r, c, old, new));
                    }
                    self.redo_stack.push(UndoOp::Cells(redo));
                    self.modified = true;
                    self.recalc();
                    self.status = "Undo".into();
                }
            }
        } else {
            self.status = "Nothing to undo".into();
        }
    }

    pub fn redo(&mut self) {
        if let Some(op) = self.redo_stack.pop() {
            match op {
                UndoOp::Cells(changes) => {
                    let mut undo = Vec::with_capacity(changes.len());
                    for (si, r, c, old, new) in changes {
                        if let Some(sheet) = self.wb.sheets.get_mut(si) {
                            sheet.set(r, c, new.clone());
                        }
                        undo.push((si, r, c, old, new));
                    }
                    self.undo_stack.push(UndoOp::Cells(undo));
                    self.modified = true;
                    self.recalc();
                    self.status = "Redo".into();
                }
            }
        } else {
            self.status = "Nothing to redo".into();
        }
    }

    fn recalc(&mut self) {
        self.engine.recalc(&mut self.wb);
    }

    // ----- clipboard --------------------------------------------------------

    /// Copy the current selection into the internal buffer and return the
    /// tab/newline-separated text for the OS clipboard.
    pub fn copy_selection(&mut self) -> String {
        let sel = self.selection();
        let (rows, cols) = (sel.rows(), sel.cols());
        let mut cells = Vec::with_capacity((rows * cols) as usize);
        let mut text = String::new();
        for r in sel.start.row..=sel.end.row {
            for c in sel.start.col..=sel.end.col {
                let cell = self.sheet().get(r, c).cloned().unwrap_or(Cell::Empty);
                cells.push(cell);
                if c > sel.start.col {
                    text.push('\t');
                }
                text.push_str(&self.display(r, c));
            }
            text.push('\n');
        }
        self.clip = Some(Clip { rows, cols, cells });
        self.status = format!("Copied {rows}x{cols}");
        text
    }

    pub fn cut_selection(&mut self) -> String {
        let text = self.copy_selection();
        self.clear_selection();
        self.status = "Cut".into();
        text
    }

    /// Paste the internal buffer (preferred) at the cursor; if absent, parse
    /// `os_text` (TSV) and paste that.
    pub fn paste(&mut self, os_text: &str) {
        let base_r = self.cursor_row;
        let base_c = self.cursor_col;
        let mut changes = Vec::new();
        if let Some(clip) = self.clip.clone() {
            let mut i = 0;
            for dr in 0..clip.rows {
                for dc in 0..clip.cols {
                    let cell = clip.cells[i].clone();
                    i += 1;
                    changes.push((base_r + dr, base_c + dc, cell));
                }
            }
        } else if !os_text.is_empty() {
            for (dr, line) in os_text.lines().enumerate() {
                for (dc, field) in line.split('\t').enumerate() {
                    changes.push((base_r + dr as u32, base_c + dc as u32, parse_input(field)));
                }
            }
        }
        if changes.is_empty() {
            self.status = "Clipboard empty".into();
            return;
        }
        self.apply_cells(changes);
        self.status = "Pasted".into();
    }

    // ----- find / goto ------------------------------------------------------

    /// Move to an A1 address. Returns false if it doesn't parse.
    pub fn goto_a1(&mut self, a1: &str) -> bool {
        if let Some(addr) = CellAddress::parse_a1(a1.trim()) {
            self.move_to(addr.row, addr.col, false);
            self.status = format!("Moved to {}", self.cursor_label());
            true
        } else {
            self.status = format!("Invalid address: {a1}");
            false
        }
    }

    /// Find the next cell (row-major from just after the cursor, wrapping) whose
    /// display text contains `query` (case-insensitive). Moves the cursor.
    pub fn find_next(&mut self, query: &str, backward: bool) -> bool {
        if query.is_empty() {
            return false;
        }
        self.last_find = query.to_string();
        let needle = query.to_lowercase();
        let si = self.active_sheet_idx();
        let (rows, cols) = self.wb.sheets[si].dimensions();
        if rows == 0 || cols == 0 {
            self.status = "Not found".into();
            return false;
        }
        let total = rows as u64 * cols as u64;
        let start = self.cursor_row as u64 * cols as u64 + self.cursor_col as u64;
        for step in 1..=total {
            let idx = if backward {
                (start + total - step) % total
            } else {
                (start + step) % total
            };
            let r = (idx / cols as u64) as u32;
            let c = (idx % cols as u64) as u32;
            if self.display(r, c).to_lowercase().contains(&needle) {
                self.move_to(r, c, false);
                self.status = format!("Found at {}", self.cursor_label());
                return true;
            }
        }
        self.status = format!("Not found: {query}");
        false
    }

    // ----- sheets -----------------------------------------------------------

    pub fn switch_sheet(&mut self, idx: usize) {
        if idx < self.wb.sheets.len() {
            self.wb.active_sheet = idx;
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.anchor_row = 0;
            self.anchor_col = 0;
            self.scroll_row = 0;
            self.scroll_col = 0;
        }
    }

    pub fn next_sheet(&mut self, d: i64) {
        let n = self.wb.sheets.len();
        if n == 0 {
            return;
        }
        let cur = self.active_sheet_idx() as i64;
        let next = (cur + d).rem_euclid(n as i64) as usize;
        self.switch_sheet(next);
    }

    // ----- save -------------------------------------------------------------

    /// Save to the known path. Returns Ok(true) if saved, Ok(false) if no path
    /// is set (caller should prompt).
    pub fn save(&mut self) -> anyhow::Result<bool> {
        match self.path.clone() {
            Some(p) => {
                self.save_to(&p)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn save_to(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        super::workbook_io::save_path(&self.wb, path)?;
        self.path = Some(path.to_path_buf());
        self.modified = false;
        self.status = format!("Saved {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel::model::CellValue;

    fn app() -> App {
        App::new(Workbook::new(), None)
    }

    #[test]
    fn cursor_label_is_a1() {
        let mut a = app();
        a.move_to(2, 1, false); // B3
        assert_eq!(a.cursor_label(), "B3");
        a.move_to(0, 26, false); // AA1
        assert_eq!(a.cursor_label(), "AA1");
    }

    #[test]
    fn edit_commit_parses_and_recalcs() {
        let mut a = app();
        a.move_to(0, 0, false);
        a.commit_edit("10", 0, 0);
        a.move_to(1, 0, false);
        a.commit_edit("=A1*2", 0, 0);
        assert_eq!(a.sheet().value(1, 0), CellValue::Number(20.0));
        assert!(a.modified);
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut a = app();
        a.move_to(0, 0, false);
        a.commit_edit("5", 0, 0);
        assert_eq!(a.sheet().value(0, 0), CellValue::Number(5.0));
        a.undo();
        assert_eq!(a.sheet().value(0, 0), CellValue::Empty);
        a.redo();
        assert_eq!(a.sheet().value(0, 0), CellValue::Number(5.0));
    }

    #[test]
    fn selection_and_clear() {
        let mut a = app();
        a.move_to(0, 0, false);
        a.commit_edit("1", 0, 0);
        a.move_to(1, 1, false);
        a.commit_edit("2", 0, 0);
        // Select A1:B2 and clear.
        a.move_to(0, 0, false);
        a.move_to(1, 1, true);
        assert!(a.has_range_selection());
        a.clear_selection();
        assert_eq!(a.sheet().value(0, 0), CellValue::Empty);
        assert_eq!(a.sheet().value(1, 1), CellValue::Empty);
    }

    #[test]
    fn copy_paste_internal() {
        let mut a = app();
        a.move_to(0, 0, false);
        a.commit_edit("7", 0, 0);
        a.move_to(0, 0, false);
        let _ = a.copy_selection();
        a.move_to(3, 3, false);
        a.paste("");
        assert_eq!(a.sheet().value(3, 3), CellValue::Number(7.0));
    }

    #[test]
    fn wheel_scroll_is_independent_of_cursor() {
        let mut a = app();
        // Put some data so the extent allows scrolling.
        a.move_to(50, 10, false);
        a.commit_edit("x", 0, 0);
        a.move_to(0, 0, false);
        let (cr, cc) = (a.cursor_row, a.cursor_col);
        // Simulate a render consuming the snap-to-cursor request.
        a.scroll_to_cursor = false;
        a.scroll_by(5, 0);
        assert_eq!(a.scroll_row, 5);
        // Cursor unchanged and a wheel scroll does not request a snap-back.
        assert_eq!((a.cursor_row, a.cursor_col), (cr, cc));
        assert!(!a.scroll_to_cursor);
        // Scrolling up past the top clamps at 0.
        a.scroll_by(-100, 0);
        assert_eq!(a.scroll_row, 0);
    }

    #[test]
    fn formula_ref_highlight_scan() {
        // Range under the caret is flagged active.
        assert_eq!(
            formula_ref_highlights("=SUM(A1:A10)", 6),
            vec![(0, 0, 9, 0, true)]
        );
        // Two single-cell refs; caret on the first.
        assert_eq!(
            formula_ref_highlights("=A1+B2", 2),
            vec![(0, 0, 0, 0, true), (1, 1, 1, 1, false)]
        );
        // Function names are not refs; refs inside string literals are ignored.
        assert_eq!(
            formula_ref_highlights("=\"A1\"&B2", 0),
            vec![(1, 1, 1, 1, false)]
        );
        // Sheet-qualified refs (other sheet) are skipped; current-sheet B2 kept.
        assert_eq!(
            formula_ref_highlights("=Sheet2!A1+B2", 0),
            vec![(1, 1, 1, 1, false)]
        );
        // Absolute markers parse; lowercase is accepted.
        assert_eq!(
            formula_ref_highlights("=$c$3:d4", 99),
            vec![(2, 2, 3, 3, false)]
        );
    }

    #[test]
    fn coerce_selection_to_numbers_is_undoable() {
        let mut a = app();
        a.move_to(0, 0, false);
        a.commit_edit("6,000.00", 0, 0); // parsed as text by the TUI input parser?
        // Force a text cell regardless of input parsing.
        let si = a.active_sheet_idx();
        a.wb.sheets[si].set_a1("A1", Cell::Text("6,000.00".into()));
        a.move_to(0, 0, false);
        let n = a.coerce_selection_to_numbers();
        assert_eq!(n, 1);
        assert_eq!(a.sheet().value(0, 0), CellValue::Number(6000.0));
        a.undo();
        assert_eq!(a.sheet().value(0, 0), CellValue::Text("6,000.00".into()));
    }

    #[test]
    fn pointer_on_hscroll_hit_test() {
        let mut a = app();
        a.hscroll = Some((6, 23, 40, 100, 40)); // bar on row 23, cols 6..46
        assert!(a.pointer_on_hscroll(10, 23));
        assert!(!a.pointer_on_hscroll(10, 22)); // wrong row
        assert!(!a.pointer_on_hscroll(46, 23)); // just past the bar
        a.hscroll = None;
        assert!(!a.pointer_on_hscroll(10, 23));
    }

    #[test]
    fn double_click_detection() {
        let mut a = app();
        assert!(!a.register_click(2, 3)); // first click — single
        assert!(a.register_click(2, 3)); // same cell quickly — double
        assert!(!a.register_click(2, 3)); // sequence reset — single again
        a.register_click(2, 3); // arm
        assert!(!a.register_click(4, 5)); // different cell — single
    }

    #[test]
    fn column_resize_drag() {
        let mut a = app();
        a.grid_area = (0, 0, 80, 24); // header row at y = 0
        // Column 2 header at x=20, width 9 → right-edge grab handle at x+w-1 = 28.
        a.col_header_rects = vec![(2, 20, 9)];
        assert!(!a.try_begin_col_resize(25, 0)); // not on the handle
        assert!(!a.try_begin_col_resize(28, 1)); // wrong row
        assert!(a.try_begin_col_resize(28, 0)); // on the handle
        assert_eq!(a.resizing_col, Some((2, 20)));
        // Drag the right edge to column 35 → width = 35 - 20 + 1 = 16.
        assert!(a.drag_col_resize(35));
        let si = a.active_sheet_idx();
        assert_eq!(a.wb.sheets[si].columns.get(&2).unwrap().width, Some(16.0));
        assert!(a.modified);
        // Dragging left past the minimum clamps.
        a.drag_col_resize(20);
        assert_eq!(
            a.wb.sheets[si].columns.get(&2).unwrap().width,
            Some(layout::MIN_COL_WIDTH as f64)
        );
        a.end_drag();
        assert!(a.resizing_col.is_none());
        assert!(!a.drag_col_resize(40)); // no-op after release
    }

    #[test]
    fn scrollbar_drag_maps_to_scroll() {
        let mut a = app();
        // Vertical bar at column 79, rows 1..21 (len 20), 200 rows total, 20 visible.
        a.vscroll = Some((79, 1, 20, 200, 20));
        // Click at the very top of the track → scroll near 0.
        assert!(a.scrollbar_drag(79, 1));
        assert_eq!(a.scroll_row, 0);
        // Click at the bottom of the track → scroll near max (total - visible).
        assert!(a.scrollbar_drag(79, 20));
        assert_eq!(a.scroll_row, 180);
        // A click off the bar returns false and changes nothing.
        a.scroll_row = 5;
        assert!(!a.scrollbar_drag(10, 10));
        assert_eq!(a.scroll_row, 5);
    }

    #[test]
    fn edit_caret_hit_test() {
        let mut a = app();
        // Editor text starts at x=10 on row 0, width 8 (cols 10..18).
        a.edit_rects.push((10, 0, 8));
        assert_eq!(a.edit_caret_at(10, 0), Some(0)); // first char
        assert_eq!(a.edit_caret_at(13, 0), Some(3));
        assert_eq!(a.edit_caret_at(17, 0), Some(7)); // last column in field
        assert_eq!(a.edit_caret_at(18, 0), None); // just past the field
        assert_eq!(a.edit_caret_at(13, 1), None); // wrong row
    }

    #[test]
    fn cursor_move_requests_scroll() {
        let mut a = app();
        a.scroll_to_cursor = false;
        a.move_by(1, 0, false);
        assert!(a.scroll_to_cursor);
    }

    #[test]
    fn goto_and_find() {
        let mut a = app();
        a.move_to(4, 2, false);
        a.commit_edit("needle", 0, 0);
        a.move_to(0, 0, false);
        assert!(a.goto_a1("C5"));
        assert_eq!((a.cursor_row, a.cursor_col), (4, 2));
        a.move_to(0, 0, false);
        assert!(a.find_next("needle", false));
        assert_eq!((a.cursor_row, a.cursor_col), (4, 2));
        assert!(!a.find_next("missing", false));
    }
}
