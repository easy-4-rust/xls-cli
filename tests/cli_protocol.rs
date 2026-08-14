//! `xls` 二进制的进程级协议回归测试。

use std::path::Path;
use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xls"))
        .args(arguments)
        .output()
        .expect("run xls")
}

#[test]
fn json_success_uses_stdout_only() {
    let output = run(&["capabilities", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON result");
    assert_eq!(value["command"], "capabilities");
    assert_eq!(value["schema_version"]["major"], 1);
}

/// 元契约：capabilities 声明为 `partial` 的每个命令，其 `--json` 调用必须返回
/// 稳定错误（退出码 3 + `UNSUPPORTED_COMMAND`）且 stderr 保持为空。
/// 动词提升为 supported 后自动脱离本断言，无需逐动词修改。
#[test]
fn json_partial_commands_are_stable_unsupported_errors() {
    let capabilities = run(&["capabilities", "--json"]);
    assert!(capabilities.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&capabilities.stdout).expect("capabilities JSON");
    let partial: Vec<String> = value["data"]["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter(|entry| entry["status"] == "partial")
        .map(|entry| {
            entry["command"]
                .as_str()
                .expect("command name is a string")
                .to_owned()
        })
        .collect();
    assert!(
        !partial.is_empty(),
        "partial 集为空：全部命令已提升为 supported，请将本测试改为断言全集 supported"
    );
    for command in &partial {
        let output = run(&[command.as_str(), "--json"]);
        assert_eq!(
            output.status.code(),
            Some(3),
            "partial 命令 `{command}` 的 --json 退出码应为 3"
        );
        assert!(
            output.stderr.is_empty(),
            "partial 命令 `{command}` 的 stderr 应为空"
        );
        let error: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("JSON error");
        assert_eq!(
            error["error"]["code"], "UNSUPPORTED_COMMAND",
            "partial 命令 `{command}` 的错误码"
        );
    }
}

#[test]
fn grep_finds_case_insensitive_matches_with_addresses() {
    let directory = tempfile::tempdir().expect("temp directory");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables.md");
    let workbook_path = directory.path().join("tables.xlsx");
    let fixture_text = fixture.to_string_lossy();
    let workbook_text = workbook_path.to_string_lossy();
    let import = run(&["import", &fixture_text, &workbook_text, "--json"]);
    assert!(import.status.success());

    let grep = run(&["grep", &workbook_text, "ALICE", "--json"]);
    assert!(grep.status.success());
    assert!(grep.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&grep.stdout).expect("grep JSON");
    assert_eq!(value["command"], "grep");
    let matches = value["data"]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 1, "命中 Alice 所在单元格");
    assert_eq!(matches[0]["address"], "A2");
    assert_eq!(matches[0]["value"], "Alice");
    assert_eq!(value["stats"]["matches"], 1);

    let miss = run(&["grep", &workbook_text, "no-such-token", "--json"]);
    assert!(miss.status.success());
    let miss_value: serde_json::Value = serde_json::from_slice(&miss.stdout).expect("grep JSON");
    assert_eq!(miss_value["data"]["matches"].as_array().unwrap().len(), 0);
    assert_eq!(miss_value["stats"]["matches"], 0);
}

#[test]
fn group1_read_verbs_return_structured_contracts() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("group1.md");
    let workbook_path = directory.path().join("group1.xlsx");
    std::fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n",
    )
    .expect("fixture");
    let markdown_text = markdown.to_string_lossy();
    let workbook_text = workbook_path.to_string_lossy();
    assert!(run(&["import", &markdown_text, &workbook_text, "--json"])
        .status
        .success());

    let profile = run(&["profile", &workbook_text, "amount", "--json"]);
    assert!(profile.status.success());
    assert!(profile.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&profile.stdout).expect("JSON");
    assert_eq!(value["command"], "profile");
    assert_eq!(value["data"]["sum"], 49.0);

    let eval = run(&["eval", &workbook_text, "=SUM(B2:B3)", "--json"]);
    assert!(eval.status.success());
    let value: serde_json::Value = serde_json::from_slice(&eval.stdout).expect("JSON");
    assert_eq!(value["command"], "eval");
    assert_eq!(value["data"]["value"], 49.0);

    let format = run(&["format", &workbook_text, "B2", "--json"]);
    assert!(format.status.success());
    let value: serde_json::Value = serde_json::from_slice(&format.stdout).expect("JSON");
    assert_eq!(value["command"], "format");
    assert_eq!(value["data"]["format"], "GENERAL");
}

#[test]
fn group2_group3_verbs_return_structured_contracts() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("g23.md");
    let workbook_path = directory.path().join("g23.xlsx");
    std::fs::write(
        &markdown,
        "| name | amount |\n| --- | ---: |\n| Alice | 42 |\n| Bob | 7 |\n| Carol | 99 |\n",
    )
    .expect("fixture");
    let markdown_text = markdown.to_string_lossy();
    let workbook_text = workbook_path.to_string_lossy();
    assert!(run(&["import", &markdown_text, &workbook_text, "--json"])
        .status
        .success());

    for arguments in [
        vec!["filter", &workbook_text, "amount>40", "--json"],
        vec!["pivot", &workbook_text, "--rows", "name", "--values", "amount", "--json"],
        vec!["sort", &workbook_text, "--by", "amount", "--output",
             &directory.path().join("s.xlsx").to_string_lossy(), "--json"],
        vec!["dedup", &workbook_text, "--on", "name", "--output",
             &directory.path().join("d.xlsx").to_string_lossy(), "--json"],
        vec!["copy", &workbook_text, "A2:B2", "A5", "--output",
             &directory.path().join("c.xlsx").to_string_lossy(), "--json"],
        vec!["move", &workbook_text, "A2:B2", "A6", "--output",
             &directory.path().join("m.xlsx").to_string_lossy(), "--json"],
        vec!["join", &workbook_text, &workbook_text, "--on", "name", "--json"],
        vec!["diff", &workbook_text, &workbook_text, "--json"],
    ] {
        let output = run(&arguments);
        assert!(
            output.status.success(),
            "命令 {:?} 应成功",
            arguments.first()
        );
        assert!(
            output.stderr.is_empty(),
            "命令 {:?} 的 stderr 应为空",
            arguments.first()
        );
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("JSON");
        assert_eq!(
            value["command"],
            serde_json::json!(arguments.first().unwrap().to_string())
        );
    }

    // append 需要两个输入
    let appended = directory.path().join("a.xlsx").to_string_lossy().to_string();
    let output = run(&[
        "append", &workbook_text, &workbook_text, "--output", &appended, "--json",
    ]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(value["command"], "append");
    assert_eq!(value["data"]["appended"], 3);
}

#[test]
fn group4_verbs_return_structured_contracts() {
    let directory = tempfile::tempdir().expect("temp directory");
    let markdown = directory.path().join("g4.md");
    let workbook_path = directory.path().join("g4.xlsx");
    std::fs::write(
        &markdown,
        "| code | when |\n| --- | --- |\n| 6,000.00 | 04/04/2025 |\n| 7 | 05/04/2025 |\n",
    )
    .expect("fixture");
    let markdown_text = markdown.to_string_lossy();
    let workbook_text = workbook_path.to_string_lossy();
    assert!(run(&["import", &markdown_text, &workbook_text, "--json"])
        .status
        .success());

    for arguments in [
        vec!["format-set", &workbook_text, "B2:B3", "dd/mm/yyyy", "--output",
             &directory.path().join("a.xlsx").to_string_lossy(), "--json"],
        vec!["to-number", &workbook_text, "A2:A3", "--output",
             &directory.path().join("b.xlsx").to_string_lossy(), "--json"],
        vec!["to-date", &workbook_text, "B2:B3", "--format", "dd/mm/yyyy", "--output",
             &directory.path().join("c.xlsx").to_string_lossy(), "--json"],
        vec!["autofit", &workbook_text, "--output",
             &directory.path().join("d.xlsx").to_string_lossy(), "--json"],
        vec!["style", &workbook_text, "A1:B1", "--bold", "--bg", "FFFF00", "--output",
             &directory.path().join("e.xlsx").to_string_lossy(), "--json"],
        vec!["batch", &workbook_text, "--set", "A9=done", "--set", "B9=2", "--output",
             &directory.path().join("f.xlsx").to_string_lossy(), "--json"],
        vec!["name", &workbook_text, "add", "Total", "Table1!$A$1", "--output",
             &directory.path().join("g.xlsx").to_string_lossy(), "--json"],
        vec!["name", &workbook_text, "list", "--json"],
        vec!["table", &workbook_text, "add", "A1:B3", "--name", "SalesTable", "--output",
             &directory.path().join("h.xlsx").to_string_lossy(), "--json"],
        vec!["table", &workbook_text, "list", "--json"],
    ] {
        let output = run(&arguments);
        assert!(
            output.status.success(),
            "命令 {:?} 应成功：{}",
            arguments.first(),
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stderr.is_empty(),
            "命令 {:?} 的 stderr 应为空",
            arguments.first()
        );
        let value: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("JSON");
        let command = value["command"].as_str().unwrap_or_default().to_owned();
        assert_eq!(
            Some(command.as_str()),
            arguments.first().copied(),
            "command 字段应与动词一致"
        );
    }
}

#[test]
fn markdown_task_chain_dry_runs_writes_and_reopens_output() {
    let directory = tempfile::tempdir().expect("temp directory");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables.md");
    let output_path = directory.path().join("tables.xlsx");
    let fixture_text = fixture.to_string_lossy();
    let output_text = output_path.to_string_lossy();

    let dry_run = run(&["import", &fixture_text, &output_text, "--dry-run", "--json"]);
    assert!(dry_run.status.success());
    assert!(!output_path.exists());
    let dry_result: serde_json::Value =
        serde_json::from_slice(&dry_run.stdout).expect("dry-run JSON");
    assert_eq!(dry_result["files"][0]["written"], false);

    let import = run(&["import", &fixture_text, &output_text, "--json"]);
    assert!(import.status.success());
    assert!(output_path.exists());

    let get = run(&[
        "get",
        &output_text,
        "Sales!A1:B3",
        "--format",
        "json",
        "--json",
    ]);
    assert!(get.status.success());
    let value: serde_json::Value = serde_json::from_slice(&get.stdout).expect("get JSON");
    assert_eq!(value["data"]["rows"][1][0], "Alice");
    assert_eq!(value["data"]["rows"][1][1].as_f64(), Some(42.0));
}
