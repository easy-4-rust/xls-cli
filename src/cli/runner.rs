//! `xls` 终端产品边界：参数解析、JSON/终端渲染和进程退出码。

use std::env;
use std::io::{self, BufRead};
use std::process::ExitCode;

use crate::cli::{
    CommandError, CommandExecutor, DefaultCommandExecutor, ErrorCode, ExecutionContext,
    ExecutionMode, OverwritePolicy, ResourceLimits, SchemaVersion, SecretString, into_request,
};
#[cfg(test)]
use clap::CommandFactory;
use clap::{Parser, error::ErrorKind};
use serde_json::json;

use super::Cli;

/// 解析终端参数并执行一次命令。
#[must_use]
pub fn main() -> ExitCode {
    let arguments = env::args_os().collect::<Vec<_>>();
    let json_requested = arguments.iter().any(|argument| argument == "--json");
    if !json_requested
        && let Some(command) = terminal_command_name(&arguments)
        && is_terminal_route(&command)
    {
        return run_terminal(arguments, &command);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            if json_requested {
                print_json_error(&CommandError::new(
                    ErrorCode::InvalidArgument,
                    error.to_string(),
                ));
            } else {
                let _ = error.print();
            }
            return ExitCode::from(2);
        }
    };
    // Clap 的 external subcommand 会把尾随参数保留给未实现命令；仍需兑现
    // 全局 `--json` 契约，因此以原始 argv 探测结果作为最终权威。
    let json_mode = cli.json || json_requested;

    let context = match execution_context(&cli) {
        Ok(context) => context,
        Err(error) => {
            render_error(&error, json_mode);
            return ExitCode::from(exit_code(&error));
        }
    };
    let request = match into_request(cli.command) {
        Ok(request) => request,
        Err(message) => {
            let error = CommandError::new(ErrorCode::InvalidArgument, message);
            render_error(&error, json_mode);
            return ExitCode::from(exit_code(&error));
        }
    };
    match DefaultCommandExecutor::new().execute(request, &context) {
        Ok(result) => {
            if json_mode {
                match serde_json::to_string(&result) {
                    Ok(serialized) => println!("{serialized}"),
                    Err(error) => {
                        let command_error =
                            CommandError::new(ErrorCode::Internal, "无法序列化命令结果")
                                .with_diagnostic(error.to_string());
                        print_json_error(&command_error);
                        return ExitCode::from(1);
                    }
                }
            } else {
                render_human(&result);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            render_error(&error, json_mode);
            ExitCode::from(exit_code(&error))
        }
    }
}

fn terminal_command_name(arguments: &[std::ffi::OsString]) -> Option<String> {
    let mut index = 1_usize;
    while index < arguments.len() {
        let value = arguments[index].to_string_lossy();
        if matches!(
            value.as_ref(),
            "--password-env"
                | "--max-file-bytes"
                | "--max-sheets"
                | "--max-rows"
                | "--max-formula-cells"
                | "--output"
                | "-o"
                | "--password"
                | "-p"
        ) {
            index = index.saturating_add(2);
            continue;
        }
        if value.starts_with('-') {
            index = index.saturating_add(1);
            continue;
        }
        return Some(value.into_owned());
    }
    None
}

fn is_terminal_route(command: &str) -> bool {
    let migrated_command = matches!(
        command,
        "open"
            | "info"
            | "get"
            | "set"
            | "eval"
            | "format"
            | "format-set"
            | "diff"
            | "clear"
            | "fill"
            | "to-number"
            | "to-date"
            | "copy"
            | "move"
            | "insert-row"
            | "delete-row"
            | "insert-col"
            | "delete-col"
            | "new"
            | "add-sheet"
            | "delete-sheet"
            | "rename-sheet"
            | "append"
            | "pivot"
            | "sort"
            | "dedup"
            | "filter"
            | "join"
            | "profile"
            | "grep"
            | "batch"
            | "autofit"
            | "query"
            | "style"
            | "name"
            | "table"
            | "head"
            | "tail"
    );
    migrated_command || looks_like_spreadsheet_path(command)
}

fn looks_like_spreadsheet_path(value: &str) -> bool {
    std::path::Path::new(value)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "xls" | "xlsx" | "xlsm" | "csv" | "tsv" | "txt"
            )
        })
}

fn run_terminal(mut arguments: Vec<std::ffi::OsString>, command: &str) -> ExitCode {
    if arguments.iter().any(|value| {
        matches!(
            value.to_string_lossy().as_ref(),
            "--max-file-bytes" | "--max-sheets" | "--max-rows" | "--max-formula-cells"
        )
    }) {
        let error = CommandError::new(
            ErrorCode::InvalidArgument,
            "迁移终端命令暂不能与资源限制参数组合使用",
        );
        render_error(&error, false);
        return ExitCode::from(exit_code(&error));
    }

    let mutating = is_terminal_mutation(command, &arguments);
    let force_requested = arguments.iter().any(|value| value == "--force");
    if let Some(output) = terminal_option_value(&arguments, &["--output", "-o"])
        && output != "-"
        && std::path::Path::new(&output).exists()
        && !force_requested
    {
        let error = CommandError::new(
            ErrorCode::OverwriteDenied,
            format!("目标文件已存在，必须显式 --force：{output}"),
        );
        render_error(&error, false);
        return ExitCode::from(exit_code(&error));
    }
    let explicit_write_policy = arguments.iter().any(|value| {
        matches!(
            value.to_string_lossy().as_ref(),
            "--force" | "--dry-run" | "--backup" | "--output" | "-o"
        )
    });
    let safe_new_target = command == "new" && new_target_does_not_exist(&arguments);
    if mutating && !explicit_write_policy && !safe_new_target {
        let error = CommandError::new(
            ErrorCode::OverwriteDenied,
            "修改命令必须提供 --output、--dry-run、--backup 或显式 --force",
        );
        render_error(&error, false);
        return ExitCode::from(exit_code(&error));
    }
    if arguments
        .iter()
        .any(|value| matches!(value.to_string_lossy().as_ref(), "--password" | "-p"))
    {
        let error = CommandError::new(
            ErrorCode::InvalidArgument,
            "密码不得直接出现在命令参数中；请使用 --password-stdin 或 --password-env",
        );
        render_error(&error, false);
        return ExitCode::from(exit_code(&error));
    }
    if let Err(error) = inject_terminal_password(&mut arguments) {
        render_error(&error, false);
        return ExitCode::from(exit_code(&error));
    }
    arguments.retain(|value| value != "--force");
    super::terminal::main_from(arguments)
}

fn terminal_option_value(arguments: &[std::ffi::OsString], options: &[&str]) -> Option<String> {
    arguments.iter().enumerate().find_map(|(index, argument)| {
        options
            .iter()
            .any(|option| argument == option)
            .then(|| arguments.get(index + 1))
            .flatten()
            .map(|value| value.to_string_lossy().into_owned())
    })
}

fn is_terminal_mutation(command: &str, arguments: &[std::ffi::OsString]) -> bool {
    if matches!(command, "name" | "table") {
        let action = arguments
            .iter()
            .position(|value| value == command)
            .and_then(|position| arguments.get(position + 1))
            .map(|value| value.to_string_lossy());
        return action.is_some_and(|value| matches!(value.as_ref(), "add" | "rm"));
    }
    matches!(
        command,
        "set"
            | "clear"
            | "fill"
            | "format-set"
            | "to-number"
            | "to-date"
            | "copy"
            | "move"
            | "insert-row"
            | "delete-row"
            | "insert-col"
            | "delete-col"
            | "new"
            | "add-sheet"
            | "delete-sheet"
            | "rename-sheet"
            | "append"
            | "sort"
            | "dedup"
            | "batch"
            | "autofit"
            | "style"
    )
}

fn new_target_does_not_exist(arguments: &[std::ffi::OsString]) -> bool {
    arguments
        .iter()
        .position(|argument| argument == "new")
        .and_then(|position| arguments.get(position + 1))
        .map(std::path::Path::new)
        .is_some_and(|path| !path.exists())
}

fn inject_terminal_password(arguments: &mut Vec<std::ffi::OsString>) -> Result<(), CommandError> {
    let stdin_position = arguments
        .iter()
        .position(|value| value == "--password-stdin");
    let environment_position = arguments.iter().position(|value| value == "--password-env");
    if stdin_position.is_some() && environment_position.is_some() {
        return Err(CommandError::new(
            ErrorCode::InvalidArgument,
            "--password-stdin 与 --password-env 不能同时使用",
        ));
    }

    let password = if let Some(position) = stdin_position {
        arguments.remove(position);
        let mut password = String::new();
        io::stdin()
            .lock()
            .read_line(&mut password)
            .map_err(|error| {
                CommandError::new(ErrorCode::InvalidArgument, "无法从 stdin 读取密码")
                    .with_diagnostic(error.to_string())
            })?;
        trim_line_ending(&mut password);
        Some(password)
    } else if let Some(position) = environment_position {
        if position + 1 >= arguments.len() {
            return Err(CommandError::new(
                ErrorCode::InvalidArgument,
                "--password-env 缺少环境变量名",
            ));
        }
        let name = arguments[position + 1]
            .to_str()
            .ok_or_else(|| {
                CommandError::new(ErrorCode::InvalidArgument, "密码环境变量名不是有效 UTF-8")
            })?
            .to_owned();
        arguments.drain(position..=position + 1);
        Some(env::var(&name).map_err(|_| {
            CommandError::new(
                ErrorCode::InvalidArgument,
                format!("密码环境变量未设置：{name}"),
            )
        })?)
    } else {
        None
    };

    if let Some(password) = password {
        arguments.push("--password".into());
        arguments.push(password.into());
    }
    Ok(())
}

fn execution_context(cli: &Cli) -> Result<ExecutionContext, CommandError> {
    let mode = if cli.dry_run {
        ExecutionMode::DryRun
    } else {
        ExecutionMode::Apply
    };
    let overwrite = if cli.force {
        OverwritePolicy::Replace
    } else {
        OverwritePolicy::Deny
    };
    let limits = ResourceLimits::new(
        cli.max_file_bytes,
        cli.max_sheets,
        cli.max_rows,
        cli.max_formula_cells,
    );
    let mut context = ExecutionContext::new()
        .with_mode(mode)
        .with_overwrite(overwrite)
        .with_limits(limits);
    if cli.password_stdin {
        let mut password = String::new();
        io::stdin()
            .lock()
            .read_line(&mut password)
            .map_err(|error| {
                CommandError::new(ErrorCode::InvalidArgument, "无法从 stdin 读取密码")
                    .with_diagnostic(error.to_string())
            })?;
        trim_line_ending(&mut password);
        context = context.with_password(SecretString::new(password));
    } else if let Some(name) = &cli.password_env {
        let password = env::var(name).map_err(|_| {
            CommandError::new(
                ErrorCode::InvalidArgument,
                format!("密码环境变量未设置：{name}"),
            )
        })?;
        context = context.with_password(SecretString::new(password));
    }
    Ok(context)
}

fn trim_line_ending(value: &mut String) {
    while matches!(value.chars().last(), Some('\n' | '\r')) {
        value.pop();
    }
}

fn render_human(result: &crate::cli::CommandResult) {
    if let Some(text) = result.data.as_str() {
        println!("{text}");
    } else if let Ok(serialized) = serde_json::to_string_pretty(&result.data) {
        println!("{serialized}");
    }
    for file in &result.files {
        let action = if file.written { "written" } else { "planned" };
        eprintln!("{action}: {}", file.path.display());
    }
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }
}

fn render_error(error: &CommandError, json: bool) {
    if json {
        print_json_error(error);
    } else {
        eprintln!("error [{}]: {}", error.code.as_str(), error.message);
        if let Some(diagnostic) = &error.diagnostic {
            eprintln!("diagnostic: {diagnostic}");
        }
    }
}

fn print_json_error(error: &CommandError) {
    let result = json!({
        "schema_version": SchemaVersion::current(),
        "error": error,
    });
    println!("{result}");
}

const fn exit_code(error: &CommandError) -> u8 {
    match error.code {
        ErrorCode::InvalidArgument => 2,
        ErrorCode::UnsupportedCommand | ErrorCode::UnsupportedFormat => 3,
        ErrorCode::ResourceLimit | ErrorCode::OverwriteDenied => 4,
        ErrorCode::FileNotFound
        | ErrorCode::ReadFailed
        | ErrorCode::WriteFailed
        | ErrorCode::QueryFailed
        | ErrorCode::SheetNotFound => 5,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn trims_password_line_endings() {
        let mut value = "secret\r\n".to_owned();
        trim_line_ending(&mut value);
        assert_eq!(value, "secret");
    }

    #[test]
    fn routes_migrated_terminal_commands_and_direct_workbook_paths() {
        assert!(is_terminal_route("pivot"));
        assert!(is_terminal_route("report.xlsx"));
        assert!(!is_terminal_route("capabilities"));
    }

    #[test]
    fn distinguishes_read_only_and_mutating_nested_commands() {
        let name_list = ["xls", "name", "list", "book.xlsx"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        let table_add = ["xls", "table", "add", "book.xlsx", "A1:C3"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        assert!(!is_terminal_mutation("name", &name_list));
        assert!(is_terminal_mutation("table", &table_add));
    }
}
