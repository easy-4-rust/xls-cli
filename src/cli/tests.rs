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
fn grep_reports_matches_and_zero_hits_without_error() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
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
    let hit = executor
        .execute(
            CommandRequest::Grep {
                input: workbook.clone(),
                pattern: "alice".to_owned(),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("grep hit");
    assert_eq!(hit.command, CommandName::Grep);
    let matches = hit.data["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["address"], "A2");
    assert_eq!(matches[0]["value"], "Alice");
    assert_eq!(hit.stats["matches"], 1);

    let miss = executor
        .execute(
            CommandRequest::Grep {
                input: workbook,
                pattern: "no-such-token".to_owned(),
                sheet: Some("Missing".to_owned()),
            },
            &ExecutionContext::new(),
        )
        .expect_err("sheet 不存在应报错");
    assert_eq!(miss.code, ErrorCode::SheetNotFound);
}

#[test]
fn profile_reports_column_stats_and_text_storage_warnings() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| amount | note |\n| --- | --- |\n| 42 | 6,000.00 |\n| 7 | 04/04/2025 |\n| 9 | ok |\n",
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
            CommandRequest::Profile {
                input: workbook,
                column: "amount".to_owned(),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("profile amount column");
    assert_eq!(result.command, CommandName::Profile);
    assert_eq!(result.data["column"], "amount");
    assert_eq!(result.data["count"], 3);
    assert_eq!(result.data["numeric"], 3);
    assert_eq!(result.data["min"], 7.0);
    assert_eq!(result.data["max"], 42.0);
    assert_eq!(result.data["sum"], 58.0);
    assert!(result.warnings.is_empty(), "数值列不应产生警告");

    // 备注列：一个数字文本 + 一个日期文本 → 两条稳定警告码
    let result = executor
        .execute(
            CommandRequest::Profile {
                input: directory.path().join("book.xlsx"),
                column: "note".to_owned(),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("profile note column");
    let codes: Vec<&str> = result
        .warnings
        .iter()
        .map(|warning| warning.code.as_str())
        .collect();
    assert!(codes.contains(&"NUMBERS_STORED_AS_TEXT"), "警告集：{codes:?}");
    assert!(codes.contains(&"DATES_STORED_AS_TEXT"), "警告集：{codes:?}");
    assert_eq!(result.data["text"], 3);
}

#[test]
fn eval_computes_scalars_and_dynamic_array_grids() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
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
    let scalar = executor
        .execute(
            CommandRequest::Eval {
                input: workbook.clone(),
                formula: "=SUM(B2:B3)".to_owned(),
                at: None,
            },
            &ExecutionContext::new(),
        )
        .expect("eval scalar");
    assert_eq!(scalar.command, CommandName::Eval);
    assert_eq!(scalar.data["formula"], "=SUM(B2:B3)");
    assert_eq!(scalar.data["value"].as_f64(), Some(49.0));

    let grid = executor
        .execute(
            CommandRequest::Eval {
                input: workbook,
                formula: "=SEQUENCE(2,2)".to_owned(),
                at: Some("Table1!A1".to_owned()),
            },
            &ExecutionContext::new(),
        )
        .expect("eval grid");
    assert_eq!(grid.data["grid"][0][0].as_f64(), Some(1.0));
    assert_eq!(grid.data["grid"][1][1].as_f64(), Some(4.0));
}

#[test]
fn format_describes_cell_number_format() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n",
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
            CommandRequest::Format {
                input: workbook,
                cell: "B2".to_owned(),
            },
            &ExecutionContext::new(),
        )
        .expect("format");
    assert_eq!(result.command, CommandName::Format);
    assert_eq!(result.data["cell"], "B2");
    assert_eq!(result.data["format"], "GENERAL");

    let missing = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::Format {
                input: directory.path().join("absent.xlsx"),
                cell: "B2".to_owned(),
            },
            &ExecutionContext::new(),
        )
        .expect_err("缺文件应报错");
    assert_eq!(missing.code, ErrorCode::FileNotFound);
}

#[test]
fn filter_returns_matching_rows_as_json() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n| Carol | 99 |\n",
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
            CommandRequest::Filter {
                input: workbook,
                predicate: "amount>40".to_owned(),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("filter");
    assert_eq!(result.command, CommandName::Filter);
    assert_eq!(
        result.data["columns"],
        serde_json::json!(["name", "amount"])
    );
    let rows = result.data["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "42 与 99 命中");
    assert_eq!(rows[0][0], "Alice");
    assert_eq!(rows[1][0], "Carol");
    assert_eq!(result.stats["rows"], 2);

    let bad = DefaultCommandExecutor::new()
        .execute(
            CommandRequest::Filter {
                input: directory.path().join("book.xlsx"),
                predicate: "not a predicate".to_owned(),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect_err("非法谓词应报错");
    assert_eq!(bad.code, ErrorCode::InvalidArgument);
}

#[test]
fn sort_reorders_rows_with_dry_run_and_readback() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n| Carol | 99 |\n",
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

    // dry-run：不落盘
    let dry = executor
        .execute(
            CommandRequest::Sort {
                input: workbook.clone(),
                by: vec!["amount".to_owned()],
                desc: false,
                sheet: None,
                output: None,
            },
            &ExecutionContext::new().with_mode(ExecutionMode::DryRun),
        )
        .expect("dry-run sort");
    assert!(dry.dry_run);
    assert!(!dry.files[0].written);

    // apply：覆盖原文件需要 Replace；改用 --output 新文件
    let sorted = directory.path().join("sorted.xlsx");
    let result = executor
        .execute(
            CommandRequest::Sort {
                input: workbook,
                by: vec!["amount".to_owned()],
                desc: false,
                sheet: None,
                output: Some(sorted.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("sort");
    assert_eq!(result.command, CommandName::Sort);
    assert_eq!(result.data["rows"], 3);
    assert!(result.files[0].written);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: sorted,
                range: Some("Table1!A2:B4".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back");
    assert_eq!(readback.data["rows"][0][0], "Bob");
    assert_eq!(readback.data["rows"][2][0], "Carol");
}

#[test]
fn dedup_removes_duplicate_rows_by_key() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| id | note |\n| --- | --- |\n| a | x |\n| b | y |\n| a | z |\n",
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
    let deduped = directory.path().join("dedup.xlsx");
    let result = executor
        .execute(
            CommandRequest::Dedup {
                input: workbook,
                on: vec!["id".to_owned()],
                sheet: None,
                output: Some(deduped),
            },
            &ExecutionContext::new(),
        )
        .expect("dedup");
    assert_eq!(result.command, CommandName::Dedup);
    assert_eq!(result.data["removed"], 1);
    assert_eq!(result.data["remaining"], 2);
}

#[test]
fn copy_and_move_ranges_with_readback() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n",
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

    let copied = directory.path().join("copied.xlsx");
    let result = executor
        .execute(
            CommandRequest::Copy {
                input: workbook.clone(),
                source: "A2:B2".to_owned(),
                target: "A5".to_owned(),
                sheet: None,
                output: Some(copied.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("copy");
    assert_eq!(result.command, CommandName::Copy);
    assert_eq!(result.data["cells"], 2);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: copied,
                range: Some("Table1!A5".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back copy");
    assert_eq!(readback.data["rows"][0][0], "Alice");

    let moved = directory.path().join("moved.xlsx");
    let result = executor
        .execute(
            CommandRequest::Move {
                input: workbook,
                source: "A2:B2".to_owned(),
                target: "A6".to_owned(),
                sheet: None,
                output: Some(moved.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("move");
    assert_eq!(result.data["cells"], 2);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: moved,
                range: Some("Table1!A2:B6".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back move");
    // 源已清空（Empty→null），目标有值
    assert_eq!(readback.data["rows"][0][0], Value::Null);
    assert_eq!(readback.data["rows"][4][0], "Alice");
}

#[test]
fn pivot_groups_and_aggregates_into_rows() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| region | amount |\n| --- | ---: |\n| north | 10 |\n| south | 5 |\n| north | 7 |\n",
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
            CommandRequest::Pivot {
                input: workbook,
                rows: "region".to_owned(),
                values: "amount".to_owned(),
                agg: Aggregation::Sum,
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("pivot");
    assert_eq!(result.command, CommandName::Pivot);
    assert_eq!(result.data["columns"], serde_json::json!(["region", "sum"]));
    let rows = result.data["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], "north");
    assert_eq!(rows[0][1].as_f64(), Some(17.0));
    assert_eq!(rows[1][0], "south");
    assert_eq!(rows[1][1].as_f64(), Some(5.0));
    assert_eq!(result.stats["groups"], 2);
}

#[test]
fn multi_input_verbs_append_join_diff() {
    let directory = tempfile::tempdir().expect("temp directory");
    let base_md = directory.path().join("base.md");
    let base = directory.path().join("base.xlsx");
    let add_md = directory.path().join("add.md");
    let add = directory.path().join("add.xlsx");
    fs::write(
        &base_md,
        "| id | note |\n| --- | --- |\n| a | x |\n| b | y |\n",
    )
    .expect("base fixture");
    fs::write(
        &add_md,
        "| note | id |\n| --- | --- |\n| z | c |\n| w | a |\n",
    )
    .expect("add fixture");
    let executor = DefaultCommandExecutor::new();
    for (input_md, output) in [(&base_md, &base), (&add_md, &add)] {
        executor
            .execute(
                CommandRequest::Import {
                    input: input_md.clone(),
                    output: output.clone(),
                    markdown_options: None,
                },
                &ExecutionContext::new(),
            )
            .expect("import");
    }

    // append：按表头名对齐（add 列序不同也应对齐），追加 2 行
    let appended = directory.path().join("appended.xlsx");
    let result = executor
        .execute(
            CommandRequest::Append {
                input: base.clone(),
                with: add.clone(),
                sheet: None,
                output: Some(appended.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("append");
    assert_eq!(result.data["appended"], 2);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: appended,
                range: Some("Table1!A4:B5".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back append");
    assert_eq!(readback.data["rows"][0][0], "c");
    assert_eq!(readback.data["rows"][0][1], "z");

    // join：id 相等内连接
    let result = executor
        .execute(
            CommandRequest::Join {
                input: base.clone(),
                with: add.clone(),
                on: "id".to_owned(),
            },
            &ExecutionContext::new(),
        )
        .expect("join");
    let rows = result.data["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "只有 a 同时存在");
    assert_eq!(rows[0][0], "a");
    assert_eq!(result.stats["rows"], 1);

    // diff keyed：c 为新增（+），b 为删除（-）
    let result = executor
        .execute(
            CommandRequest::Diff {
                input: base,
                with: add,
                key: Some("id".to_owned()),
                sheet: None,
            },
            &ExecutionContext::new(),
        )
        .expect("diff");
    let kinds: Vec<&str> = result.data["differences"]
        .as_array()
        .expect("differences")
        .iter()
        .map(|entry| entry["kind"].as_str().expect("kind"))
        .collect();
    assert!(kinds.contains(&"added"), "kinds: {kinds:?}");
    assert!(kinds.contains(&"removed"), "kinds: {kinds:?}");
    assert_eq!(result.stats["differences"], kinds.len() as u64);
}

#[test]
fn group4_batch1_format_number_date_autofit() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| code | when |\n| --- | --- |\n| 6,000.00 | 04/04/2025 |\n| 7 | 05/04/2025 |\n",
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

    // format-set：给 B 列设置日期格式
    let formatted = directory.path().join("formatted.xlsx");
    let result = executor
        .execute(
            CommandRequest::FormatSet {
                input: workbook.clone(),
                range: "B2:B3".to_owned(),
                code: "dd/mm/yyyy".to_owned(),
                sheet: None,
                output: Some(formatted.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("format-set");
    assert_eq!(result.data["cells"], 2);
    let check = executor
        .execute(
            CommandRequest::Format {
                input: formatted.clone(),
                cell: "B2".to_owned(),
            },
            &ExecutionContext::new(),
        )
        .expect("format check");
    assert_eq!(check.data["format"], "DATE dd/mm/yyyy");

    // to-number：把 "6,000.00" 文本转数值
    let numbered = directory.path().join("numbered.xlsx");
    let result = executor
        .execute(
            CommandRequest::ToNumber {
                input: formatted,
                range: "A2:A3".to_owned(),
                sheet: None,
                output: Some(numbered.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("to-number");
    assert_eq!(result.data["converted"], 1);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: numbered.clone(),
                range: Some("Table1!A2:A2".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back");
    assert_eq!(readback.data["rows"][0][0].as_f64(), Some(6000.0));

    // to-date：把文本日期转日期序列
    let dated = directory.path().join("dated.xlsx");
    let result = executor
        .execute(
            CommandRequest::ToDate {
                input: numbered,
                range: "B2:B3".to_owned(),
                format: "dd/mm/yyyy".to_owned(),
                sheet: None,
                output: Some(dated.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("to-date");
    assert_eq!(result.data["converted"], 2);

    // autofit
    let fitted = directory.path().join("fitted.xlsx");
    let result = executor
        .execute(
            CommandRequest::Autofit {
                input: dated,
                columns: None,
                sheet: None,
                output: Some(fitted),
            },
            &ExecutionContext::new(),
        )
        .expect("autofit");
    assert_eq!(result.data["columns"], 2);
}

#[test]
#[allow(clippy::too_many_lines, reason = "组4 批2 的端到端场景测试集中覆盖四类动词")]
fn group4_batch2_style_name_table_batch() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("input.md");
    let workbook = directory.path().join("book.xlsx");
    fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n",
    )
    .expect("write fixture");
    let executor = DefaultCommandExecutor::new();
    let import = CommandRequest::Import {
        input: markdown,
        output: workbook,
        markdown_options: None,
    };
    executor
        .execute(import, &ExecutionContext::new())
        .expect("import markdown");

    // style：范围加粗 + 底色
    let styled = directory.path().join("styled.xlsx");
    let result = executor
        .execute(
            CommandRequest::Style {
                input: directory.path().join("book.xlsx"),
                range: "A1:B1".to_owned(),
                bold: true,
                italic: false,
                color: None,
                bg: Some("FFFF00".to_owned()),
                sheet: None,
                output: Some(styled.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("style");
    assert_eq!(result.data["cells"], 2);

    // name：add → list → remove
    let named = directory.path().join("named.xlsx");
    executor
        .execute(
            CommandRequest::Name {
                input: styled,
                action: NameAction::Add {
                    name: "Sales".to_owned(),
                    refers_to: "Table1!$A$1:$B$2".to_owned(),
                    sheet: None,
                },
                output: Some(named.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("name add");
    let listed = executor
        .execute(
            CommandRequest::Name {
                input: named.clone(),
                action: NameAction::List,
                output: None,
            },
            &ExecutionContext::new(),
        )
        .expect("name list");
    assert_eq!(
        listed.data["names"][0]["name"], "Sales",
        "应列出刚定义的名称"
    );

    // table：add → list
    let tabled = directory.path().join("tabled.xlsx");
    executor
        .execute(
            CommandRequest::Table {
                input: named,
                action: TableAction::Add {
                    range: "A1:B2".to_owned(),
                    name: Some("SalesTable".to_owned()),
                    sheet: None,
                    no_header: false,
                },
                output: Some(tabled.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("table add");
    let listed = executor
        .execute(
            CommandRequest::Table {
                input: tabled.clone(),
                action: TableAction::List,
                output: None,
            },
            &ExecutionContext::new(),
        )
        .expect("table list");
    assert_eq!(listed.data["tables"][0]["name"], "SalesTable");

    // batch：两个 CELL=VALUE 一次落盘，回读验证
    let batched = directory.path().join("batched.xlsx");
    let result = executor
        .execute(
            CommandRequest::Batch {
                input: tabled,
                sets: vec!["A5=hello".to_owned(), "B5=7".to_owned()],
                sheet: None,
                output: Some(batched.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect("batch");
    assert_eq!(result.data["applied"], 2);
    let readback = executor
        .execute(
            CommandRequest::Get {
                input: batched,
                range: Some("Table1!A5:B5".to_owned()),
                output_format: OutputFormat::Json,
            },
            &ExecutionContext::new(),
        )
        .expect("read back batch");
    assert_eq!(readback.data["rows"][0][0], "hello");
    assert_eq!(readback.data["rows"][0][1].as_f64(), Some(7.0));

    // batch 原子性：含非法项时整体失败且不写输出
    let broken = directory.path().join("broken.xlsx");
    let error = executor
        .execute(
            CommandRequest::Batch {
                input: directory.path().join("book.xlsx"),
                sets: vec!["A6=ok".to_owned(), "not-an-assignment".to_owned()],
                sheet: None,
                output: Some(broken.clone()),
            },
            &ExecutionContext::new(),
        )
        .expect_err("batch 必须整体失败");
    assert_eq!(error.code, ErrorCode::InvalidArgument);
    assert!(!broken.exists(), "失败时不得写出输出文件");
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
