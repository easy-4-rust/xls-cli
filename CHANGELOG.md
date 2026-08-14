# Changelog

## 0.1.0 (2026-08-14)

首个公开发布：Agent-safe 电子表格 CLI 与 TUI，面向脚本、智能体与人。

### 结构化命令协议（schema 1.0）

- 43/44 命令为 `supported`：读（info/get/head/tail/grep/profile/eval/format/
  filter/pivot/join/diff/name-list/table-list）、写（set/clear/fill/insert-rows/
  delete-rows/insert-columns/delete-columns/new/add-sheet/delete-sheet/rename-sheet/
  recalc/sort/dedup/copy/move/append/convert/import/export/format-set/to-number/
  to-date/style/autofit/name-add+rm/table-add+rm/batch）与协议发现
  （capabilities/schema）。
- 仅 `open`（交互式 TUI）为 `partial`：结构化 JSON 请求返回稳定的
  `UNSUPPORTED_COMMAND`（退出码 3）。
- 安全语义：`--dry-run` 预览（`files[].written == false`）、`--output` 新路径默认、
  `--force` 显式覆盖、资源限制（`--max-*`）、稳定错误码、JSON 只走 stdout。
- 多输入动词（append/join/diff）以 `with` 携带第二输入。
- `batch` 原子性：任一 `CELL=VALUE` 非法则整体失败不落盘。

### 能力与技能

- `xls capabilities --json` / `xls schema --command NAME --json` 为运行时事实源。
- Agent Skill（`skills/dist/{openclaw,hermes}/xls-cli/SKILL.md`）：运行时契约、
  读/写安全流水线、按任务选命令表。

### 分发

- npm：`@partme.ai/xls-cli`（launcher）+ 8 平台子包（darwin/linux × arm64/x64 ×
  gnu/musl + win32）。
- Cargo：`cargo install xls-cli`（二进制名 `xls`，TUI feature-gated）。

### 工程基础

- 引擎全部来自 EasyExcel-Rust（无 xls fork 生产依赖）；terminal/TUI 实现源自
  zemse/xls（MIT OR Apache-2.0，见 NOTICE）。
- `unsafe_code = forbid`；clippy pedantic 全绿；terminal 迁移回归（46 项）与
  进程级协议契约测试全部通过。
