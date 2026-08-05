//! A thin wrapper over [`tui_textarea::TextArea`] for single-line text entry
//! (the formula bar and modal dialogs).

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 单行编辑器保持上游输入行为"
)]

use ratatui::style::Style;
use tui_textarea::{CursorMove, Input, TextArea};

/// A single-line text editor.
pub struct Editor {
    area: TextArea<'static>,
}

impl Editor {
    /// Create an editor seeded with `text`, cursor at end.
    pub fn new(text: &str) -> Self {
        let mut area = TextArea::new(vec![text.to_string()]);
        area.set_cursor_line_style(Style::default());
        area.move_cursor(CursorMove::End);
        Editor { area }
    }

    /// The current single-line content.
    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    /// Number of characters in the content (display-width approximation for a
    /// single line of plain text). Used to size the in-place editor.
    pub fn len(&self) -> usize {
        self.area
            .lines()
            .first()
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    /// The caret's character column within the (single) line.
    pub fn caret_col(&self) -> usize {
        self.area.cursor().1
    }

    /// Move the text caret to character column `col` (clamped to the content).
    /// Used to position the caret from a mouse click.
    pub fn set_caret_col(&mut self, col: usize) {
        let c = col.min(self.len()) as u16;
        self.area.move_cursor(CursorMove::Jump(0, c));
    }

    /// Style the editor's text (background/foreground) — e.g. to make an
    /// in-place edit box stand out over the grid.
    pub fn set_style(&mut self, style: Style) {
        self.area.set_style(style);
    }

    /// Feed a crossterm key/paste event into the editor. Returns true if the
    /// content changed.
    pub fn input(&mut self, input: impl Into<Input>) -> bool {
        self.area.input(input)
    }

    /// The underlying text area; in tui-textarea 0.7 a `&TextArea` is itself a
    /// `Widget`, so render it via `Frame::render_widget(editor.widget(), area)`.
    pub fn widget(&self) -> &TextArea<'static> {
        &self.area
    }
}

impl Default for Editor {
    fn default() -> Self {
        Editor::new("")
    }
}
