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
    assert_eq!(result.data["markdown"]["import"], true);
    assert_eq!(
        result.data["markdown"]["streamingExport"],
        serde_json::json!(["xlsx", "csv"])
    );
    // 元契约：partial 集中的每个命令名都必须能 round-trip 为 CommandName，
    // 防止能力清单与命令路由出现拼写/命名漂移。动词提升后集合收缩，断言自动适应。
    let partial: Vec<String> = commands
        .iter()
        .filter(|entry| entry["status"] == "partial")
        .map(|entry| {
            entry["command"]
                .as_str()
                .expect("command name is a string")
                .to_owned()
        })
        .collect();
    for name in &partial {
        let parsed: Result<CommandName, _> = serde_json::from_str(&format!("\"{name}\""));
        assert!(
            parsed.is_ok(),
            "partial 命令名 `{name}` 无法反序列化为 CommandName"
        );
    }
}

#[test]
fn markdown_dry_run_validates_without_creating_output() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("output.xlsx");
    fs::write(&markdown, "| id |\n| --- |\n| 007 |\n").expect("fixture");
    let result = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::Import {
                input: markdown,
                output: workbook.clone(),
                markdown_options: None,
            },
            &ExecutionContext::new().with_mode(ExecutionMode::DryRun),
        )
        .expect("dry-run import");
    assert!(result.dry_run);
    assert!(!result.files[0].written);
    assert!(!workbook.exists());
    assert_eq!(result.stats["tables"], 1);
}

#[test]
fn planned_command_never_silently_degrades() {
    // 动态选取：优先用 capabilities 中仍在 partial 集的命令；全部提升后
    // 退回 Pivot 占位，保证 Planned 路径本身的行为始终被覆盖。
    let capabilities = DefaultCommandExecutor::new()
        .execute(CommandRequest::Capabilities, &ExecutionContext::new())
        .expect("capabilities");
    let partial_name = capabilities.data["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .find(|entry| entry["status"] == "partial")
        .and_then(|entry| entry["command"].as_str())
        .map(|name| serde_json::from_str::<CommandName>(&format!("\"{name}\"")))
        .and_then(Result::ok);
    let command_name = partial_name.unwrap_or(CommandName::Pivot);
    let error = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::Planned {
                command_name,
                arguments: Value::Null,
            },
            &ExecutionContext::new(),
        )
        .expect_err("planned command is not implemented");
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
                markdown_options: None,
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
                markdown_options: None,
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
