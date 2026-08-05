//! Rendering: draws the whole TUI for a given [`App`] state. Pure ratatui; the
//! only state it mutates on `App` is the hit-test rect caches (so mouse events
//! can map screen coords back to cells/tabs).

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 帧渲染保持命中区域与终端布局语义"
)]

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Dialog, Mode, formula_ref_highlights};
use super::editor::Editor;
use super::layout;
use super::theme;
use easyexcel::model::Cell;
use easyexcel::model::addr::col_index_to_letters;
use easyexcel::model::styles::HAlign;

/// Top-level draw entry. `editor` is the live text editor (used in Edit/Dialog).
pub fn draw(f: &mut Frame, app: &mut App, editor: &Editor) {
    let area = f.area();
    // Vertical layout: formula bar (1) | grid (rest) | tabs (1) | status (1).
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    app.edit_rects.clear();
    draw_formula_bar(f, app, editor, chunks[0]);
    draw_grid(f, app, editor, chunks[1]);
    draw_tabs(f, app, chunks[2]);
    draw_status(f, app, chunks[3]);

    if app.mode == Mode::Dialog {
        draw_dialog(f, app, editor, area);
    }
}

fn draw_formula_bar(f: &mut Frame, app: &mut App, editor: &Editor, area: Rect) {
    let label = app.cursor_label();
    let label_w = (label.len() as u16 + 2).max(6);
    let cols = Layout::horizontal([Constraint::Length(label_w), Constraint::Min(1)]).split(area);

    let name_box = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {label} "),
        theme::name_box(),
    )]));
    f.render_widget(name_box, cols[0]);

    if app.mode == Mode::Edit {
        // Live editor in the formula bar.
        f.render_widget(editor.widget(), cols[1]);
        app.edit_rects.push((cols[1].x, cols[1].y, cols[1].width));
    } else {
        let content = app.formula_bar_text();
        f.render_widget(
            Paragraph::new(Span::raw(content)).style(theme::formula_bar()),
            cols[1],
        );
    }
}

fn draw_grid(f: &mut Frame, app: &mut App, editor: &Editor, area: Rect) {
    app.cell_rects.clear();
    if area.width <= layout::ROW_HEADER_WIDTH || area.height <= layout::HEADER_HEIGHT {
        app.grid_area = (area.x, area.y, area.width, area.height);
        return;
    }

    // Reserve a 1-cell strip on the right / bottom for the scrollbars when
    // there's enough room, then draw the cell grid into the shrunk `area`.
    let full = area;
    let vbar = u16::from(full.width > layout::ROW_HEADER_WIDTH + 3);
    let hbar = u16::from(full.height > layout::HEADER_HEIGHT + 3);
    let area = Rect::new(full.x, full.y, full.width - vbar, full.height - hbar);
    app.grid_area = (area.x, area.y, area.width, area.height);

    let si = app.active_sheet_idx();
    let frozen = app.wb.sheets[si].frozen;

    // Scroll to reveal the cursor only when it has just moved; mouse-wheel
    // scrolling sets scroll offsets directly and must not snap back.
    let grid_w = area.width - layout::ROW_HEADER_WIDTH;
    let grid_h = area.height - layout::HEADER_HEIGHT;
    if app.scroll_to_cursor {
        app.ensure_visible(grid_w, grid_h);
        app.scroll_to_cursor = false;
    }

    // Build the list of visible columns: frozen cols [0..frozen.cols), then the
    // scrolling window starting at scroll_col. Record (col, x, width).
    let mut col_layout: Vec<(u32, u16, u16)> = Vec::new();
    let mut x = area.x + layout::ROW_HEADER_WIDTH;
    let x_end = area.x + area.width;
    let sheet = &app.wb.sheets[si];

    let push_col = |col: u32, x: &mut u16, out: &mut Vec<(u32, u16, u16)>| -> bool {
        let w = layout::col_width(sheet, col);
        if w == 0 {
            return true; // hidden, skip but continue
        }
        if *x >= x_end {
            return false;
        }
        let draw_w = w.min(x_end - *x);
        out.push((col, *x, draw_w));
        *x += draw_w;
        *x < x_end
    };

    let mut more = true;
    for c in 0..frozen.cols {
        if !push_col(c, &mut x, &mut col_layout) {
            more = false;
            break;
        }
    }
    if more {
        let mut c = app.scroll_col.max(frozen.cols);
        while c <= layout::MAX_COL {
            if !push_col(c, &mut x, &mut col_layout) {
                break;
            }
            c += 1;
        }
    }

    // Visible rows: frozen rows [0..frozen.rows), then scrolling from scroll_row.
    let mut row_layout: Vec<(u32, u16)> = Vec::new();
    let mut y = area.y + layout::HEADER_HEIGHT;
    let y_end = area.y + area.height;
    for r in 0..frozen.rows {
        if y >= y_end {
            break;
        }
        row_layout.push((r, y));
        y += 1;
    }
    let mut r = app.scroll_row.max(frozen.rows);
    while y < y_end && r <= layout::MAX_ROW {
        row_layout.push((r, y));
        y += 1;
        r += 1;
    }

    let buf = f.buffer_mut();

    // Paint the canvas so the grid reads as one cohesive surface regardless of
    // the user's terminal background.
    fill_rect(buf, area, ' ', theme::cell());

    // Corner + column headers.
    let header_style = theme::header();
    fill_rect(
        buf,
        Rect::new(area.x, area.y, layout::ROW_HEADER_WIDTH, 1),
        ' ',
        header_style,
    );
    app.col_header_rects.clear();
    for &(col, cx, cw) in &col_layout {
        let label = col_index_to_letters(col);
        let style = if col == app.cursor_col {
            theme::header_active()
        } else {
            header_style
        };
        fill_rect(buf, Rect::new(cx, area.y, cw, 1), ' ', style);
        let centered = center(&label, cw as usize);
        buf.set_stringn(cx, area.y, &centered, cw as usize, style);
        app.col_header_rects.push((col, cx, cw));
    }

    // Row headers.
    for &(row, ry) in &row_layout {
        let label = format!("{}", row + 1);
        let style = if row == app.cursor_row {
            theme::header_active()
        } else {
            header_style
        };
        let cell = Rect::new(area.x, ry, layout::ROW_HEADER_WIDTH, 1);
        fill_rect(buf, cell, ' ', style);
        let padded = right_align(&label, layout::ROW_HEADER_WIDTH as usize - 1);
        buf.set_stringn(
            area.x,
            ry,
            &padded,
            layout::ROW_HEADER_WIDTH as usize,
            style,
        );
    }

    let sel = app.selection();
    // Screen rect of the active cell, captured for the in-place editor.
    let mut cursor_rect: Option<(u16, u16, u16)> = None;

    // While editing a formula, highlight the cell/range references it contains
    // (the range under the caret gets a distinct colour).
    let ref_highlights = if app.mode == Mode::Edit {
        let text = editor.text();
        if text.starts_with('=') {
            formula_ref_highlights(&text, editor.caret_col())
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Body cells.
    for &(row, ry) in &row_layout {
        for &(col, cx, cw) in &col_layout {
            let in_sel = sel.contains(row, col);
            let is_cursor = row == app.cursor_row && col == app.cursor_col;
            if is_cursor {
                cursor_rect = Some((cx, ry, cw));
            }

            let ref_hl = ref_highlights
                .iter()
                .find(|&&(r0, c0, r1, c1, _)| row >= r0 && row <= r1 && col >= c0 && col <= c1)
                .map(|&(.., active)| active);

            let mut style = cell_style(app, si, row, col);
            if is_cursor {
                style = style.patch(theme::cursor_cell());
            } else if let Some(active) = ref_hl {
                style = style.bg(if active {
                    theme::formula_ref_active_bg()
                } else {
                    theme::formula_ref_bg()
                });
            } else if in_sel {
                style = style.bg(theme::selection_bg());
            }

            let rect = Rect::new(cx, ry, cw, 1);
            fill_rect(buf, rect, ' ', style);

            let text = app.wb.display_cell(si, row, col);
            if !text.is_empty() {
                let right = matches!(
                    app.wb.sheets[si].get(row, col),
                    Some(Cell::Number(_))
                        | Some(Cell::Formula {
                            cached: easyexcel::model::CellValue::Number(_),
                            ..
                        })
                );
                let halign = explicit_halign(app, si, row, col);
                let aligned = align_in(&text, cw as usize, halign.unwrap_or(right));
                buf.set_stringn(cx, ry, &aligned, cw as usize, style);
            }

            app.cell_rects.push((row, col, cx, ry, cw, 1));
        }
    }

    // Draw a thin separator after frozen regions for orientation.
    if frozen.cols > 0
        && let Some(&(_, fx, fw)) = col_layout.iter().find(|&&(c, _, _)| c == frozen.cols - 1)
    {
        let sep_x = fx + fw;
        if sep_x < x_end {
            for &(_, ry) in &row_layout {
                buf[(sep_x, ry)].set_fg(theme::frozen_separator());
            }
        }
    }

    // In-place cell editor: overlay the live editor on the active cell while
    // editing, widened rightward to fit the text (like Excel). The formula bar
    // shows the same editor; both let the caret be clicked/arrowed.
    if app.mode == Mode::Edit
        && let Some((cx, ry, cw)) = cursor_rect
    {
        let want = (editor.len() as u16).saturating_add(2).max(cw);
        let w = want.min(x_end.saturating_sub(cx));
        let rect = Rect::new(cx, ry, w, 1);
        f.render_widget(Clear, rect);
        f.render_widget(editor.widget(), rect);
        app.edit_rects.push((cx, ry, w));
    }

    // Scrollbars along the reserved right/bottom strips. Positions reflect the
    // scroll offset within the sheet's data extent (so the thumb tracks where
    // you are in the used range).
    let (data_rows, data_cols) = app.wb.sheets[si].dimensions();
    app.vscroll = None;
    app.hscroll = None;
    let buf = f.buffer_mut();
    if vbar > 0 {
        let (bx, by, body_h) = (
            full.x + full.width - 1,
            area.y + layout::HEADER_HEIGHT,
            area.height - layout::HEADER_HEIGHT,
        );
        let visible = body_h as usize;
        let total = (data_rows as usize).max(app.scroll_row as usize + visible);
        draw_scrollbar(
            buf,
            bx,
            by,
            body_h,
            true,
            app.scroll_row as usize,
            total,
            visible,
        );
        app.vscroll = Some((bx, by, body_h, total, visible));
    }
    if hbar > 0 {
        let (bx, by, body_w) = (
            area.x + layout::ROW_HEADER_WIDTH,
            full.y + full.height - 1,
            area.width - layout::ROW_HEADER_WIDTH,
        );
        // Columns have variable width; approximate the visible count.
        let visible = (body_w / layout::DEFAULT_COL_WIDTH).max(1) as usize;
        let total = (data_cols as usize).max(app.scroll_col as usize + visible);
        draw_scrollbar(
            buf,
            bx,
            by,
            body_w,
            false,
            app.scroll_col as usize,
            total,
            visible,
        );
        app.hscroll = Some((bx, by, body_w, total, visible));
    }
}

/// Paint a background-filled scrollbar (track + thumb) along a 1-cell strip.
/// `vertical` selects orientation; `len` is the track length in cells. We paint
/// cell backgrounds rather than glyphs so the bar stays continuous across
/// terminals that add inter-row padding (per md-tui's approach).
#[allow(clippy::too_many_arguments)]
fn draw_scrollbar(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    len: u16,
    vertical: bool,
    scroll: usize,
    total: usize,
    visible: usize,
) {
    let track = len as usize;
    if track == 0 {
        return;
    }
    let (thumb_top, thumb_len) = if total <= visible || total == 0 {
        (0, track)
    } else {
        let h = (track * visible / total).clamp(1, track);
        let span = track - h;
        let max_scroll = total - visible;
        let top = (scroll * span + max_scroll / 2)
            .checked_div(max_scroll)
            .unwrap_or(0);
        (top.min(span), h)
    };
    for i in 0..track {
        let in_thumb = i >= thumb_top && i < thumb_top + thumb_len;
        let color = if in_thumb {
            theme::scrollbar_thumb()
        } else {
            theme::scrollbar_track()
        };
        let (cx, cy) = if vertical {
            (x, y + i as u16)
        } else {
            (x + i as u16, y)
        };
        if cx < buf.area.right() && cy < buf.area.bottom() {
            let cell = &mut buf[(cx, cy)];
            cell.set_char(' ');
            cell.set_bg(color);
        }
    }
}

/// Resolve the alignment of a cell: explicit style halign, else `default_right`.
fn align_in(text: &str, width: usize, right: bool) -> String {
    let tw = UnicodeWidthStr::width(text);
    if tw > width {
        return truncate_ellipsis(text, width);
    }
    let pad = width - tw;
    if right {
        format!("{}{}", " ".repeat(pad), text)
    } else {
        format!("{}{}", text, " ".repeat(pad))
    }
}

/// Returns an explicit alignment (true=right) if the cell's style pins one.
fn explicit_halign(app: &App, si: usize, row: u32, col: u32) -> Option<bool> {
    let sheet = &app.wb.sheets[si];
    let style_idx = sheet.style_at(row, col)?;
    let style = app.wb.styles.get(style_idx)?;
    match style.halign {
        HAlign::Left | HAlign::Fill | HAlign::Justify => Some(false),
        HAlign::Right => Some(true),
        HAlign::Center | HAlign::CenterContinuous | HAlign::Distributed => None, // handled as left
        HAlign::General => None,
    }
}

fn cell_style(app: &App, si: usize, row: u32, col: u32) -> Style {
    let mut style = theme::cell();
    let sheet = &app.wb.sheets[si];
    if let Some(idx) = sheet.style_at(row, col)
        && let Some(cs) = app.wb.styles.get(idx)
    {
        if cs.font.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if cs.font.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if let Some(argb) = cs.font.color.0 {
            style = style.fg(argb_to_color(argb));
        }
    }
    // Errors stand out in red.
    if matches!(
        sheet.get(row, col),
        Some(Cell::Error(_))
            | Some(Cell::Formula {
                cached: easyexcel::model::CellValue::Error(_),
                ..
            })
    ) {
        style = style.fg(theme::error_fg());
    }
    style
}

fn argb_to_color(argb: u32) -> Color {
    Color::Rgb(
        ((argb >> 16) & 0xFF) as u8,
        ((argb >> 8) & 0xFF) as u8,
        (argb & 0xFF) as u8,
    )
}

fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    app.tab_rects.clear();
    let active = app.active_sheet_idx();
    let mut x = area.x;
    let buf = f.buffer_mut();
    fill_rect(buf, area, ' ', theme::tab_bar());
    for (i, sheet) in app.wb.sheets.iter().enumerate() {
        let label = format!(" {} ", sheet.name);
        let w = (label.width() as u16).min(area.width.saturating_sub(x - area.x));
        if w == 0 {
            break;
        }
        let style = if i == active {
            theme::tab_active()
        } else {
            theme::tab_inactive()
        };
        buf.set_stringn(x, area.y, &label, w as usize, style);
        app.tab_rects.push((i, x, area.y, w));
        x += w + 1;
        if x >= area.x + area.width {
            break;
        }
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mode = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Edit => "EDIT",
        Mode::Dialog => "COMMAND",
    };
    let modified = if app.modified { " *" } else { "" };
    let si = app.active_sheet_idx();
    let kind = cell_kind(app, si, app.cursor_row, app.cursor_col);
    let range = if app.has_range_selection() {
        let sel = app.selection();
        format!(" [{}x{}]", sel.rows(), sel.cols())
    } else {
        String::new()
    };

    let badge = format!(" {mode} ");
    let info = format!(
        " {}{}{} · {} · {} ",
        app.cursor_label(),
        modified,
        range,
        kind,
        app.status
    );
    // A persistent right-aligned key hint so "how do I quit/cancel?" is always
    // answered, even after the transient status message changes.
    let hint = match app.mode {
        Mode::Normal => "q/Esc quit ",
        Mode::Edit => "Esc cancel · Enter confirm ",
        Mode::Dialog => "Esc cancel ",
    };
    let used = badge.width() + info.width() + hint.width();
    let pad = (area.width as usize).saturating_sub(used);

    let line = Line::from(vec![
        Span::styled(
            badge,
            theme::mode_badge(match app.mode {
                Mode::Normal => theme::mode_normal(),
                Mode::Edit => theme::mode_edit(),
                Mode::Dialog => theme::mode_command(),
            }),
        ),
        Span::raw(info),
        Span::raw(" ".repeat(pad)),
        Span::styled(hint, Style::default().fg(theme::SUBTEXT0)),
    ]);
    f.render_widget(Paragraph::new(line).style(theme::status_bar()), area);
}

fn cell_kind(app: &App, si: usize, row: u32, col: u32) -> &'static str {
    match app.wb.sheets[si].get(row, col) {
        None | Some(Cell::Empty) => "empty",
        Some(Cell::Number(_)) => "number",
        Some(Cell::Text(_)) => "text",
        Some(Cell::Bool(_)) => "bool",
        Some(Cell::Error(_)) => "error",
        Some(Cell::Formula { .. }) => "formula",
    }
}

fn draw_dialog(f: &mut Frame, app: &App, editor: &Editor, area: Rect) {
    let Some(dialog) = app.dialog else { return };
    let title = match dialog {
        Dialog::GoTo => "Go to cell (A1)",
        Dialog::Find => "Find",
        Dialog::SaveAs => "Save as (path)",
        Dialog::Command => "Command",
        Dialog::ConfirmQuit => "Unsaved changes — quit? (y/n)",
    };
    let w = area.width.clamp(20, 60);
    let h = 3;
    let rect = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::dialog_border())
        .style(theme::dialog())
        .title(title);
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if dialog == Dialog::ConfirmQuit {
        f.render_widget(
            Paragraph::new("y = quit without saving · n/Esc = cancel").style(theme::dialog()),
            inner,
        );
    } else {
        f.render_widget(editor.widget(), inner);
    }
}

// ---- low-level buffer helpers ---------------------------------------------

fn fill_rect(buf: &mut Buffer, rect: Rect, ch: char, style: Style) {
    for yy in rect.y..rect.y.saturating_add(rect.height) {
        for xx in rect.x..rect.x.saturating_add(rect.width) {
            if xx < buf.area.right() && yy < buf.area.bottom() {
                let cell = &mut buf[(xx, yy)];
                cell.set_char(ch);
                cell.set_style(style);
            }
        }
    }
}

fn truncate_ellipsis(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > width - 1 {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

fn center(s: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w >= width {
        return truncate_ellipsis(s, width);
    }
    let left = (width - w) / 2;
    let right = width - w - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn right_align(s: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w >= width {
        return s.to_string();
    }
    format!("{}{}", " ".repeat(width - w), s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsis_truncation() {
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_ellipsis("hello", 3), "he…");
        assert_eq!(truncate_ellipsis("hello", 1), "…");
        assert_eq!(truncate_ellipsis("", 0), "");
    }

    #[test]
    fn alignment() {
        assert_eq!(align_in("7", 4, true), "   7");
        assert_eq!(align_in("hi", 4, false), "hi  ");
        // Overflow truncates regardless of side.
        assert_eq!(align_in("longtext", 4, true), "lon…");
    }

    #[test]
    fn header_labels() {
        assert_eq!(col_index_to_letters(0), "A");
        assert_eq!(center("A", 5), "  A  ");
        assert_eq!(right_align("3", 4), "   3");
    }

    // A full-frame render smoke test: editing must not panic and must register
    // edit-field hit rects (formula bar + in-place editor).
    #[test]
    fn edit_mode_renders_in_place_editor() {
        use crate::tui::app::{App, Mode};
        use crate::tui::editor::Editor;
        use easyexcel::model::Workbook;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new(Workbook::new(), None);
        app.mode = Mode::Edit;
        let editor = Editor::new("=SUM(A1:A3)");

        let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
        terminal.draw(|f| draw(f, &mut app, &editor)).unwrap();

        // One rect for the formula bar, one for the in-place cell editor.
        assert!(app.edit_rects.len() >= 2);
        // The caret hit-test resolves inside a registered field.
        let (x, y, _) = app.edit_rects[0];
        assert_eq!(app.edit_caret_at(x, y), Some(0));
    }
}
