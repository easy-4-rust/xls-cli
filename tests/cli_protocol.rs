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
