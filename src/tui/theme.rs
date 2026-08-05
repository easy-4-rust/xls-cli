//! Central colour palette and semantic styles for the TUI.
//!
//! The palette is **Catppuccin Mocha** — a soft, low-glare dark theme that is
//! comfortable for long spreadsheet sessions while still giving the grid a
//! cohesive, modern look. All rendering code should pull colours from here
//! rather than hardcoding them, so the scheme can be tuned in one place.

#![allow(
    clippy::all,
    clippy::pedantic,
    reason = "来源保真的旧 xls 主题常量保持现有视觉语义"
)]

use ratatui::style::{Color, Modifier, Style};

// ---- raw palette (Catppuccin Mocha) ---------------------------------------

/// Window background — the canvas the whole grid sits on.
pub const BASE: Color = Color::Rgb(30, 30, 46); // #1e1e2e
/// One step darker than base — used for chrome (tab bar, status bar).
pub const MANTLE: Color = Color::Rgb(24, 24, 37); // #181825
/// Darkest shade — text drawn on bright accents reads as near-black.
pub const CRUST: Color = Color::Rgb(17, 17, 27); // #11111b

/// Raised surface — headers, inactive tabs.
pub const SURFACE0: Color = Color::Rgb(49, 50, 68); // #313244
/// Slightly brighter surface — selection highlight, cursor header.
pub const SURFACE1: Color = Color::Rgb(69, 71, 90); // #45475a

/// Muted foreground — grid lines, frozen separators, dimmed labels.
pub const OVERLAY0: Color = Color::Rgb(108, 112, 134); // #6c7086
/// Brighter muted tone — scrollbar thumb.
pub const OVERLAY1: Color = Color::Rgb(127, 132, 156); // #7f849c

/// Primary foreground — cell contents.
pub const TEXT: Color = Color::Rgb(205, 214, 244); // #cdd6f4
/// Secondary foreground — header labels.
pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200); // #a6adc8

// Accents.
pub const BLUE: Color = Color::Rgb(137, 180, 250); // #89b4fa
pub const LAVENDER: Color = Color::Rgb(180, 190, 254); // #b4befe
pub const GREEN: Color = Color::Rgb(166, 227, 161); // #a6e3a1
pub const PEACH: Color = Color::Rgb(250, 179, 135); // #fab387
pub const RED: Color = Color::Rgb(243, 139, 168); // #f38ba8
pub const MAUVE: Color = Color::Rgb(203, 166, 247); // #cba6f7

// ---- semantic styles ------------------------------------------------------

/// Default style for a grid body cell (text on the canvas background).
pub fn cell() -> Style {
    Style::default().fg(TEXT).bg(BASE)
}

/// The active cell under the cursor — a bright accent block.
pub fn cursor_cell() -> Style {
    Style::default()
        .fg(CRUST)
        .bg(BLUE)
        .add_modifier(Modifier::BOLD)
}

/// Background for a cell inside the current range selection — a subtle raised
/// highlight applied over the cell's own foreground style.
pub fn selection_bg() -> Color {
    SURFACE1
}

/// Background tint for a cell referenced by the formula being edited.
pub fn formula_ref_bg() -> Color {
    Color::Rgb(45, 62, 80) // muted blue
}

/// Background tint for the referenced range the edit caret is currently on.
pub fn formula_ref_active_bg() -> Color {
    Color::Rgb(58, 90, 64) // muted green — the "range you're on"
}

/// Column / row header in its resting state.
pub fn header() -> Style {
    Style::default().fg(SUBTEXT0).bg(SURFACE0)
}

/// Header for the row/column the cursor is on — highlighted to track position.
pub fn header_active() -> Style {
    Style::default()
        .fg(CRUST)
        .bg(LAVENDER)
        .add_modifier(Modifier::BOLD)
}

/// Cell contents that resolve to an error.
pub fn error_fg() -> Color {
    RED
}

/// The thin separator drawn after a frozen pane.
pub fn frozen_separator() -> Color {
    OVERLAY0
}

/// Scrollbar track (the groove the thumb slides in).
pub fn scrollbar_track() -> Color {
    SURFACE0
}

/// Scrollbar thumb (the draggable position indicator).
pub fn scrollbar_thumb() -> Color {
    OVERLAY1
}

/// The cell-address "name box" at the left of the formula bar.
pub fn name_box() -> Style {
    Style::default()
        .fg(CRUST)
        .bg(MAUVE)
        .add_modifier(Modifier::BOLD)
}

/// The formula / value text shown in the formula bar.
pub fn formula_bar() -> Style {
    Style::default().fg(TEXT).bg(BASE)
}

/// Background of the sheet-tab strip.
pub fn tab_bar() -> Style {
    Style::default().bg(MANTLE)
}

/// The currently selected sheet tab.
pub fn tab_active() -> Style {
    Style::default()
        .fg(CRUST)
        .bg(BLUE)
        .add_modifier(Modifier::BOLD)
}

/// A background (unselected) sheet tab.
pub fn tab_inactive() -> Style {
    Style::default().fg(SUBTEXT0).bg(SURFACE0)
}

/// Background of the status bar.
pub fn status_bar() -> Style {
    Style::default().fg(TEXT).bg(MANTLE)
}

/// The coloured mode badge at the left of the status bar. `bg` is the
/// per-mode accent supplied by the caller.
pub fn mode_badge(bg: Color) -> Style {
    Style::default()
        .fg(CRUST)
        .bg(bg)
        .add_modifier(Modifier::BOLD)
}

/// Accent colour for the NORMAL mode badge.
pub fn mode_normal() -> Color {
    GREEN
}

/// Accent colour for the EDIT mode badge.
pub fn mode_edit() -> Color {
    PEACH
}

/// Accent colour for the COMMAND/dialog mode badge.
pub fn mode_command() -> Color {
    MAUVE
}

/// Border style for modal dialogs.
pub fn dialog_border() -> Style {
    Style::default().fg(MAUVE)
}

/// Inner background / text for modal dialogs.
pub fn dialog() -> Style {
    Style::default().fg(TEXT).bg(SURFACE0)
}

/// The active text editor (formula bar in Edit mode, and the in-place cell
/// editor) — a bright input field that stands out over the grid.
pub fn editor() -> Style {
    Style::default().fg(CRUST).bg(LAVENDER)
}
