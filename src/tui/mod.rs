//! 基于 `EasyExcel` 组件的交互式终端电子表格。

mod app;
mod editor;
mod layout;
mod parse;
mod runtime;
mod theme;
mod ui;
mod workbook_io;

pub use runtime::{run, run_path};
