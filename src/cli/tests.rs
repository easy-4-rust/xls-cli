use std::fs;

use serde_json::Value;

use super::*;

#[test]
fn capabilities_are_machine_readable_and_truthful() {
    let result = DefaultCommandExecutor::new()
        .execute(CommandRequest::Capabilities, &ExecutionContext::new())
        .expect("capabilities");
    assert_eq!(result.command, CommandName::Capabilities);
    let commands = result.data["commands"].as_array().expect("commands");
    assert!(
        commands
            .iter()
            .any(|entry| { entry["command"] == "get" && entry["status"] == "supported" })
    );
    assert!(
        commands
            .iter()
            .any(|entry| { entry["command"] == "pivot" && entry["status"] == "partial" })
    );
}

#[test]
fn planned_command_never_silently_degrades() {
    let error = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::Planned {
                command_name: CommandName::Pivot,
                arguments: Value::Null,
            },
            &ExecutionContext::new(),
        )
        .expect_err("pivot is not implemented");
    assert_eq!(error.code, ErrorCode::UnsupportedCommand);
}

#[test]
fn dry_run_plans_without_creating_file() {
    let directory = tempfile::tempdir().expect("temp directory");
    let output = directory.path().join("planned.xlsx");
    let context = ExecutionContext::new().with_mode(ExecutionMode::DryRun);
    let result = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::New {
                output: output.clone(),
                sheets: vec!["Data".to_owned()],
            },
            &context,
        )
        .expect("dry run");
    assert!(result.dry_run);
    assert!(!result.files[0].written);
    assert!(!output.exists());
}

#[test]
fn markdown_import_can_be_reopened_and_extracted() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("output.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n",
    )
    .expect("write fixture");
    let executor = DefaultCommandExecutor::new();
    executor
        .execute(
            CommandRequest::Import {
                input: markdown,
                output: workbook.clone(),
            },
            &ExecutionContext::new(),
        )
        .expect("import markdown");
    let result = executor
        .execute(
            CommandRequest::Get {
                input: workbook,
                range: Some("Table1!A1:B3".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read imported workbook");
    assert_eq!(result.data["rows"][1][0], "Alice");
    assert_eq!(result.data["rows"][1][1].as_f64(), Some(42.0));
}

#[test]
fn existing_target_requires_explicit_replace_policy() {
    let directory = tempfile::tempdir().expect("temp directory");
    let output = directory.path().join("book.xlsx");
    let executor = DefaultCommandExecutor::new();
    executor
        .execute(
            CommandRequest::New {
                output: output.clone(),
                sheets: Vec::new(),
            },
            &ExecutionContext::new(),
        )
        .expect("create workbook");
    let error = executor
        .execute(
            CommandRequest::New {
                output,
                sheets: Vec::new(),
            },
            &ExecutionContext::new(),
        )
        .expect_err("overwrite must be denied");
    assert_eq!(error.code, ErrorCode::OverwriteDenied);
}

#[test]
fn query_engine_is_available_through_command_contract() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("query.md");
    let workbook = directory.path().join("query.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n",
    )
    .expect("write fixture");
    let executor = DefaultCommandExecutor::new();
    executor
        .execute(
            CommandRequest::Import {
                input: markdown,
                output: workbook.clone(),
            },
            &ExecutionContext::new(),
        )
        .expect("import markdown");
    let result = executor
        .execute(
            CommandRequest::Query {
                input: workbook,
                sql: "SELECT name, amount FROM Table1 WHERE amount > 10".to_owned(),
            },
            &ExecutionContext::new(),
        )
        .expect("query");
    assert_eq!(result.data["rows"].as_array().expect("rows").len(), 1);
    assert_eq!(result.data["rows"][0][0], "Alice");
}
