//! Interactive terminal UI (ratatui) for the spreadsheet.
//!
//! Entry points: [`run`] launches the UI for an in-memory [`Workbook`];
//! [`run_path`] opens a file first and remembers its path (so Ctrl+S can save
//! without prompting).
//!
//! Submodules:
//! * [`app`]   — application state + pure logic (cursor, edit, undo, clipboard).
//! * [`layout`]— pure grid geometry (widths, scroll maths, data edges).
//! * [`parse`] — string → [`Cell`] input parsing.
//! * [`editor`]— single-line text editor (formula bar / dialogs).
//! * [`theme`] — central colour palette + semantic styles.
//! * [`ui`]    — rendering.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls TUI 事件循环保持键鼠与终端恢复行为"
)]

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use easyexcel::model::{Cell, CellValue, Workbook};

use super::app::{App, Dialog, Mode};
use super::editor::Editor;
use super::{parse, theme, ui, workbook_io};

/// RAII guard: restores the terminal (cooked mode, main screen, mouse off) on
/// drop — including when unwinding from a panic.
struct TermGuard;

impl TermGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, EnableMouseCapture)?;
        Ok(TermGuard)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
        let _ = disable_raw_mode();
    }
}

/// Install a panic hook that restores the terminal before the default hook
/// prints the panic message (otherwise the trace is mangled by raw mode).
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
        let _ = disable_raw_mode();
        default(info);
    }));
}

/// Launch the interactive spreadsheet UI for `wb` (no associated file path).
pub fn run(wb: Workbook) -> anyhow::Result<()> {
    run_app(App::new(wb, None))
}

/// Open `path` and launch the UI, remembering the path for Ctrl+S.
pub fn run_path(path: PathBuf) -> anyhow::Result<()> {
    let wb = workbook_io::open_path(&path)?;
    run_app(App::new(wb, Some(path)))
}

fn run_app(mut app: App) -> anyhow::Result<()> {
    install_panic_hook();
    let _guard = TermGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;

    let mut editor = Editor::default();

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &mut app, &editor))?;

        // Block up to ~100ms for an event; redraw on timeout (keeps NOW/clock
        // and resize responsive without busy-spinning).
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                handle_key(&mut app, &mut editor, key);
            }
            Event::Mouse(m) => handle_mouse(&mut app, &mut editor, m),
            // After a resize, re-reveal the cursor in the new viewport.
            Event::Resize(_, _) => app.scroll_to_cursor = true,
            _ => {}
        }
    }
    // Guard restores terminal on the way out.
    Ok(())
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

fn handle_key(app: &mut App, editor: &mut Editor, key: KeyEvent) {
    match app.mode {
        Mode::Normal => handle_key_normal(app, editor, key),
        Mode::Edit => handle_key_edit(app, editor, key),
        Mode::Dialog => handle_key_dialog(app, editor, key),
    }
}

fn ctrl(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
}
fn shift(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::SHIFT)
}

fn handle_key_normal(app: &mut App, editor: &mut Editor, key: KeyEvent) {
    let ext = shift(&key);
    // Quit
    if (key.code == KeyCode::Char('q') && !ext)
        || (ctrl(&key) && key.code == KeyCode::Char('q'))
        || key.code == KeyCode::Esc
    {
        request_quit(app, editor);
        return;
    }

    // Ctrl-modified commands.
    if ctrl(&key) {
        match key.code {
            KeyCode::Char('s') => {
                save_or_prompt(app, editor);
                return;
            }
            KeyCode::Char('z') => {
                app.undo();
                return;
            }
            KeyCode::Char('y') => {
                app.redo();
                return;
            }
            KeyCode::Char('c') => {
                let text = app.copy_selection();
                set_os_clipboard(&text);
                return;
            }
            KeyCode::Char('x') => {
                let text = app.cut_selection();
                set_os_clipboard(&text);
                return;
            }
            KeyCode::Char('v') => {
                let os = get_os_clipboard();
                app.paste(&os);
                return;
            }
            KeyCode::Char('g') => {
                open_dialog(app, editor, Dialog::GoTo, "");
                return;
            }
            KeyCode::Char('f') => {
                open_dialog(app, editor, Dialog::Find, "");
                return;
            }
            KeyCode::Char('a') => {
                let si = app.active_sheet_idx();
                let (rows, cols) = app.wb.sheets[si].dimensions();
                app.anchor_row = 0;
                app.anchor_col = 0;
                app.cursor_row = rows.saturating_sub(1);
                app.cursor_col = cols.saturating_sub(1);
                return;
            }
            KeyCode::Home => {
                app.move_to(0, 0, ext);
                return;
            }
            KeyCode::End => {
                let si = app.active_sheet_idx();
                let (rows, cols) = app.wb.sheets[si].dimensions();
                app.move_to(rows.saturating_sub(1), cols.saturating_sub(1), ext);
                return;
            }
            KeyCode::Up => {
                app.jump_data_edge(-1, 0, ext);
                return;
            }
            KeyCode::Down => {
                app.jump_data_edge(1, 0, ext);
                return;
            }
            KeyCode::Left => {
                app.jump_data_edge(0, -1, ext);
                return;
            }
            KeyCode::Right => {
                app.jump_data_edge(0, 1, ext);
                return;
            }
            _ => {}
        }
    }

    let page = app.grid_area.3.saturating_sub(2).max(1) as i64;
    match key.code {
        KeyCode::Up => app.move_by(-1, 0, ext),
        KeyCode::Down => app.move_by(1, 0, ext),
        KeyCode::Left => app.move_by(0, -1, ext),
        KeyCode::Right => app.move_by(0, 1, ext),
        KeyCode::Tab => app.move_by(0, 1, false),
        KeyCode::BackTab => app.move_by(0, -1, false),
        KeyCode::Enter => app.move_by(1, 0, false),
        KeyCode::Home => app.move_to(app.cursor_row, 0, ext),
        KeyCode::End => {
            let si = app.active_sheet_idx();
            let (_, cols) = app.wb.sheets[si].dimensions();
            app.move_to(app.cursor_row, cols.saturating_sub(1), ext);
        }
        KeyCode::PageDown => app.move_by(page, 0, ext),
        KeyCode::PageUp => app.move_by(-page, 0, ext),
        KeyCode::Delete | KeyCode::Backspace => app.clear_selection(),
        // Sheet switching: Alt+Left/Right would be nicer but keep it simple with
        // Ctrl+PageUp/Down semantics via '<'/'>'.
        KeyCode::Char('>') if !ext => app.next_sheet(1),
        KeyCode::Char('<') if !ext => app.next_sheet(-1),
        // Start editing.
        KeyCode::F(2) => start_edit(app, editor),
        // Repeat last find (F3 forward, Shift+F3 backward).
        KeyCode::F(3) => {
            let q = app.last_find.clone();
            if q.is_empty() {
                open_dialog(app, editor, Dialog::Find, "");
            } else {
                app.find_next(&q, ext);
            }
        }
        KeyCode::Char(':') => open_dialog(app, editor, Dialog::Command, ""),
        KeyCode::F(5) => open_dialog(app, editor, Dialog::GoTo, ""),
        KeyCode::Char(_) if !ctrl(&key) => {
            // Typing edits the existing content (caret at end), inserting the
            // character — it does not replace the cell. Use Del to clear first.
            start_edit(app, editor);
            editor.input(key);
        }
        _ => {}
    }
}

fn handle_key_edit(app: &mut App, editor: &mut Editor, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.status = "Edit cancelled".into();
        }
        KeyCode::Enter => {
            let text = editor.text();
            app.commit_edit(&text, 1, 0);
        }
        KeyCode::Tab => {
            let text = editor.text();
            app.commit_edit(&text, 0, 1);
        }
        KeyCode::BackTab => {
            let text = editor.text();
            app.commit_edit(&text, 0, -1);
        }
        _ => {
            editor.input(key);
        }
    }
}

fn handle_key_dialog(app: &mut App, editor: &mut Editor, key: KeyEvent) {
    let Some(dialog) = app.dialog else {
        app.mode = Mode::Normal;
        return;
    };

    if dialog == Dialog::ConfirmQuit {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => app.should_quit = true,
            _ => {
                app.mode = Mode::Normal;
                app.dialog = None;
            }
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.dialog = None;
            app.status = "Cancelled".into();
        }
        KeyCode::Enter => {
            let text = editor.text();
            app.mode = Mode::Normal;
            app.dialog = None;
            match dialog {
                Dialog::GoTo => {
                    app.goto_a1(&text);
                }
                Dialog::Find => {
                    app.find_next(&text, false);
                }
                Dialog::SaveAs => {
                    let path = PathBuf::from(text.trim());
                    if let Err(e) = app.save_to(&path) {
                        app.status = format!("Save failed: {e}");
                    }
                }
                Dialog::Command => run_command(app, editor, &text),
                Dialog::ConfirmQuit => {}
            }
        }
        _ => {
            editor.input(key);
        }
    }
}

/// Execute a command-palette line (`:w`, `:q`, `:wq`, `:w path`, `:goto B3`,
/// `:find foo`, `:sort`).
fn run_command(app: &mut App, editor: &mut Editor, line: &str) {
    let line = line.trim();
    let (cmd, rest) = match line.split_once(char::is_whitespace) {
        Some((c, r)) => (c, r.trim()),
        None => (line, ""),
    };
    match cmd {
        "w" | "write" => {
            if !rest.is_empty() {
                let _ = app
                    .save_to(std::path::Path::new(rest))
                    .map_err(|e| app.status = format!("Save failed: {e}"));
            } else {
                save_or_prompt(app, editor);
            }
        }
        "q" | "quit" => request_quit(app, editor),
        "wq" | "x" => {
            let saved = match app.save() {
                Ok(true) => true,
                Ok(false) => {
                    if !rest.is_empty() {
                        app.save_to(std::path::Path::new(rest)).is_ok()
                    } else {
                        open_dialog(app, editor, Dialog::SaveAs, "");
                        false
                    }
                }
                Err(e) => {
                    app.status = format!("Save failed: {e}");
                    false
                }
            };
            if saved {
                app.should_quit = true;
            }
        }
        "goto" | "g" => {
            app.goto_a1(rest);
        }
        "find" | "f" => {
            app.find_next(rest, false);
        }
        "sort" => sort_selection(app, rest),
        "tonum" | "num" => {
            let n = app.coerce_selection_to_numbers();
            app.status = if n > 0 {
                format!("Converted {n} text cell(s) to numbers")
            } else {
                "No numeric text in selection".into()
            };
        }
        "" => {}
        other => {
            app.status = format!("Unknown command: {other}");
        }
    }
}

/// Sort the selected range's rows by the first selected column. `rest` may be
/// `desc` for descending. Best-effort: sorts numbers then text.
fn sort_selection(app: &mut App, rest: &str) {
    let desc = rest.eq_ignore_ascii_case("desc");
    let sel = app.selection();
    if sel.rows() < 2 {
        app.status = "Select 2+ rows to sort".into();
        return;
    }
    let si = app.active_sheet_idx();
    let key_col = sel.start.col;
    // Snapshot rows of the selection.
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    for r in sel.start.row..=sel.end.row {
        let mut row = Vec::new();
        for c in sel.start.col..=sel.end.col {
            row.push(app.wb.sheets[si].get(r, c).cloned().unwrap_or(Cell::Empty));
        }
        rows.push(row);
    }
    let key_idx = (key_col - sel.start.col) as usize;
    rows.sort_by(|a, b| {
        let av = a[key_idx].value();
        let bv = b[key_idx].value();
        let ord = sort_key(&av)
            .partial_cmp(&sort_key(&bv))
            .unwrap_or(std::cmp::Ordering::Equal);
        if desc { ord.reverse() } else { ord }
    });
    // Write back.
    let mut changes = Vec::new();
    for (dr, row) in rows.into_iter().enumerate() {
        let r = sel.start.row + dr as u32;
        for (dc, cell) in row.into_iter().enumerate() {
            changes.push((r, sel.start.col + dc as u32, cell));
        }
    }
    app.apply_cells(changes);
    app.status = format!("Sorted {} rows", sel.rows());
}

/// Map a cell value to a comparable f64 key (text sorts after numbers).
fn sort_key(v: &CellValue) -> f64 {
    match v {
        CellValue::Number(n) => *n,
        CellValue::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        CellValue::Empty => f64::INFINITY,
        // Text/Error: sort lexically-ish by first char to keep it stable.
        CellValue::Text(s) => 1e18 + s.bytes().next().map(|b| b as f64).unwrap_or(0.0),
        CellValue::Error(_) => 2e18,
    }
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

fn handle_mouse(app: &mut App, editor: &mut Editor, m: MouseEvent) {
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Column-resize grab on a header border?
            if app.try_begin_col_resize(m.column, m.row) {
                return;
            }
            // Scrollbar press?
            if app.scrollbar_drag(m.column, m.row) {
                return;
            }
            // Tab click?
            if let Some((idx, _, _, _)) = app
                .tab_rects
                .iter()
                .find(|&&(_, x, y, w)| m.row == y && m.column >= x && m.column < x + w)
                .copied()
            {
                app.switch_sheet(idx);
                return;
            }
            // While editing, a click inside the edit field moves the caret;
            // a click elsewhere commits before relocating the cell cursor.
            let was_edit = app.mode == Mode::Edit;
            if was_edit {
                if let Some(col) = app.edit_caret_at(m.column, m.row) {
                    editor.set_caret_col(col);
                    return;
                }
                let text = editor.text();
                app.commit_edit(&text, 0, 0);
            }
            if let Some((r, c)) = hit_cell(app, m.column, m.row) {
                app.move_to(r, c, false);
                // Double-click also enters edit mode on the cell.
                if !was_edit && app.register_click(r, c) {
                    start_edit(app, editor);
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            // A column resize in progress takes priority, then scrollbar drag,
            // otherwise extend the selection.
            if app.drag_col_resize(m.column) {
                return;
            }
            if app.scrollbar_drag(m.column, m.row) {
                return;
            }
            if let Some((r, c)) = hit_cell(app, m.column, m.row) {
                app.move_to(r, c, true); // extend selection
            }
        }
        MouseEventKind::Up(MouseButton::Left) => app.end_drag(),
        // Wheel scrolls the view without moving the cursor. It scrolls
        // horizontally when Shift is held or the pointer is over the horizontal
        // scrollbar — a reliable horizontal-scroll path on terminals that never
        // emit dedicated horizontal-scroll events.
        MouseEventKind::ScrollDown => {
            if m.modifiers.contains(KeyModifiers::SHIFT) || app.pointer_on_hscroll(m.column, m.row)
            {
                app.scroll_by(0, 3);
            } else {
                app.scroll_by(3, 0);
            }
        }
        MouseEventKind::ScrollUp => {
            if m.modifiers.contains(KeyModifiers::SHIFT) || app.pointer_on_hscroll(m.column, m.row)
            {
                app.scroll_by(0, -3);
            } else {
                app.scroll_by(-3, 0);
            }
        }
        MouseEventKind::ScrollLeft => app.scroll_by(0, -3),
        MouseEventKind::ScrollRight => app.scroll_by(0, 3),
        _ => {}
    }
}

/// Hit-test a screen coordinate against the cached cell rects from last render.
fn hit_cell(app: &App, col: u16, row: u16) -> Option<(u32, u32)> {
    app.cell_rects
        .iter()
        .find(|&&(_, _, x, y, w, h)| col >= x && col < x + w && row >= y && row < y + h)
        .map(|&(r, c, _, _, _, _)| (r, c))
}

// ---------------------------------------------------------------------------
// Helpers tying state + editor together
// ---------------------------------------------------------------------------

/// Enter edit mode seeded with the active cell's existing content (caret at the
/// end). Typing then extends the content rather than replacing it; use Del to
/// clear a cell first if you want to start from scratch.
fn start_edit(app: &mut App, editor: &mut Editor) {
    let seed = parse::edit_seed(app.sheet().get(app.cursor_row, app.cursor_col));
    *editor = Editor::new(&seed);
    editor.set_style(theme::editor());
    app.mode = Mode::Edit;
    app.status = "Editing — Esc cancel · Enter/Tab confirm · ←→/click move caret".into();
}

fn open_dialog(app: &mut App, editor: &mut Editor, dialog: Dialog, seed: &str) {
    *editor = Editor::new(seed);
    app.dialog = Some(dialog);
    app.mode = Mode::Dialog;
}

fn request_quit(app: &mut App, editor: &mut Editor) {
    if app.modified {
        open_dialog(app, editor, Dialog::ConfirmQuit, "");
    } else {
        app.should_quit = true;
    }
}

fn save_or_prompt(app: &mut App, editor: &mut Editor) {
    match app.save() {
        Ok(true) => {}
        Ok(false) => {
            // No path: prompt for one.
            open_dialog(app, editor, Dialog::SaveAs, "");
            app.status = "Enter a path to save (e.g. out.xlsx / out.csv)".into();
        }
        Err(e) => app.status = format!("Save failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// OS clipboard (best-effort; arboard can fail in headless/unsupported envs)
// ---------------------------------------------------------------------------

fn set_os_clipboard(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}

fn get_os_clipboard() -> String {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
        .unwrap_or_default()
}
