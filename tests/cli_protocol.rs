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

#[test]
fn json_unsupported_error_is_stable_and_uses_stdout_only() {
    let output = run(&["pivot", "--json"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(value["error"]["code"], "UNSUPPORTED_COMMAND");
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
        "Table1!A1:B3",
        "--format",
        "json",
        "--json",
    ]);
    assert!(get.status.success());
    let value: serde_json::Value = serde_json::from_slice(&get.stdout).expect("get JSON");
    assert_eq!(value["data"]["rows"][1][0], "Alice");
    assert_eq!(value["data"]["rows"][1][1].as_f64(), Some(42.0));
}
