//! Thin binary entry point: dispatches to the CLI front-end, which decides
//! whether to run a headless command or launch the interactive TUI.

fn main() -> std::process::ExitCode {
    xls_cli::cli::main()
}
