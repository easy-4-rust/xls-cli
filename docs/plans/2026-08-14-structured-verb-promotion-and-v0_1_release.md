# Plan: xls-cli 结构化动词全量提升与 v0.1.0 首发

- 状态：已完成（代码与本地发布准备；npm 实际发布待执行，见“剩余外部步骤”）
- 日期：2026-08-14
- 格式：Superpowers 计划（writing-plans / executing-plans 纪律）

## Goal

把 22 个 Partial 动词按技术方案既定四组逐组提升为结构化 Supported（JSON 协议 + dry-run +
warnings + schema + 契约测试），完成后发布 `@partme.ai/xls-cli` v0.1.0（8 平台 npm），使
openclaw/Hermes 等智能体通过 SKILL.md 无禁用清单地安全使用全部核心动词。

衡量标准：

- `xls capabilities --json` 中全部 42 个命令 status == "supported"
- 8 平台 `npx @partme.ai/xls-cli capabilities --json` 可用
- `tests/cli_protocol.rs` 与 `src/cli/tests.rs` 全绿
- terminal.rs 46 个迁移回归测试保持全绿

## Architecture（现状关键事实，任务设计的依据）

- 双面路由：`runner.rs:22` 对原始 argv 探测 `--json`；无 `--json` 且动词在
  `is_terminal_route` 白名单（runner.rs:124）→ 走 terminal.rs 人类路径；有 `--json` →
  clap → `CommandRequest` → `DefaultCommandExecutor`。动词提升后保持在 terminal 白名单中
  （同名词两种行为并存，与现有 20 个 Supported 动词一致）。
- 计算复用：terminal.rs 的 `cmd_*` 已是"Workbook 进/值出"纯函数（如 cmd_sort :2226、
  cmd_profile :2429、Predicate::parse :2905、snapshot_rows/rewrite_rows :2827/:2838），
  提升工作 = 输出建模（String/println → `json!` data/stats/warnings）+ I/O 替换
  （open_file_or_stdin → workbook_io::open_workbook；save_with_opts → mutate 管道）
  + 9 处同步链。
- 写动词统一走 `mutate()` 管道（default_command_executor.rs:370）：open（资源限制）→
  闭包变更 → mutation_target（--output/--force 语义）→ save_workbook（dry-run 不落盘、
  tempfile 原子写）→ CommandResult.files[].written。
- 9 处同步链（技术方案 §3）：args.rs → request.rs → command_request.rs →
  default_command_executor.rs → capability_manifest.rs（两个数组）→ schema.rs →
  src/cli/tests.rs（2 处 partial 断言）→ tests/cli_protocol.rs → README×2 + SKILL.md →
  `node scripts/sync-skills.js`。
- 项目 lint：`unsafe_code = forbid`、`missing_docs = warn`、clippy `pedantic = warn`。
  所有新增 pub enum variant/字段必须带中文 `///` 文档注释（与现有风格一致）。

## Non-goals

- 不修改 easyexcel-rust 引擎（发现引擎缺口 → 回 easyexcel-rust 仓库另行修复，本计划只记录）
- 不实现 MCP server（Future Work 占位）
- 不改动 TUI（保持 feature-gated 现状）
- 不新增 xls 原版没有的动词
- 不做 re-encryption（加密文件改存为明文 + 警告，现状语义保持）

---

## Milestone 0：提升基建（让后续 22 个动词不再各改一遍契约测试）

- [x] **Task 0.1 元契约测试：partial 集动态断言**（commit 6fce7f5）
  把 `tests/cli_protocol.rs::json_unsupported_error_is_stable_and_uses_stdout_only`
  （硬编码 `pivot --json` → 3）和 `src/cli/tests.rs::planned_command_never_silently_degrades`
  （硬编码 Pivot）改为：先跑 `capabilities --json`，对返回的每个 `status == "partial"`
  命令断言 `--json` 调用退出码 3 + `UNSUPPORTED_COMMAND`。此后动词移出 partial 集即自动
  免测，无需逐动词改测试。
  验证：`cargo test -p xls-cli` 全绿；人为把某动词标错状态时测试能抓到。
- [x] **Task 0.2 提升标准链 checklist 固化**
  已固化为本文档附录 A。
- [x] **Task 0.3 发布卫生：清理遗留二进制**
  验证结果：`git check-ignore bin/xls` 命中 `.gitignore:2`；`git ls-files bin/` 仅
  platform.js 与 xls.js，二进制未入库。无需改动。

## Milestone 1：组1 只读动词（grep / profile / eval / format）

技术方案 P1 组1 门槛：结果数据模型、JSON renderer、golden 测试。全部为读动词（无 files
写出，无 dry-run 差异）。

- [x] **Task 1.1 grep**（stdout 单 JSON + 人类路径并存验证）
  data = `{matches: [{sheet, address, value}], pattern, sheet}`；stats = `{matches: N}`。
  复用 cmd_grep（terminal.rs:2511）的匹配逻辑，println 改为收集 Vec。退出码：结构化模式
  恒 0（命中数在 stats，人类路径保留 0/1 语义）。
  TDD：先写进程级失败测试 `grep --json`（当前返回 3）。
- [x] **Task 1.2 profile**（含 cast_precision_loss 局部 allow）
  data = `{column, count, non_null, numeric_count, text_count, distinct, sum, mean, min,
  max}`；warnings = 稳定字符串码 `NUMBERS_STORED_AS_TEXT` / `DATES_STORED_AS_TEXT`
  （含计数 detail，映射模式参照 markdown_result :648 的 MarkdownWarningCode 映射）。
  复用 cmd_profile（:2429）统计逻辑。
- [x] **Task 1.3 eval**（Array/Ref→grid，SORT/SEQUENCE 冒烟通过）
  data = `{formula, at, value}`，数组结果 `{grid: [[...]]}`（render_value_grid 的结构化
  版本，参照 selection.rs:54 的 CellValue→JSON）。复用 cmd_eval（:1529）的
  `Engine::new().recalc + eval_formula` 路径。
- [x] **Task 1.4 format**（复用 render::describe_number_format）
  data = `{cell, format: "DATE"|"NUMBER"|"GENERAL"|<numfmt-code>}`。terminal 侧对应实现
  迁移。
- [x] **Task 1.5 组1 收尾**（schema/双语 README/SKILL+dist/进程级契约测试）
  schema.rs 为 4 个动词写详细 schema（当前 `_ =>` 通用占位）；SKILL.md 命令表加行、禁用
  清单移除这 4 个；README×2 能力表更新；`node scripts/sync-skills.js`；git diff 确认
  dist 同步。

## Milestone 2：组2 行写动词（filter / sort / dedup / copy / move / append）

技术方案 P1 组2 门槛：输入/输出所有权、dry-run、回读断言。除 filter 外全部走 mutate 管道。

- [x] **Task 2.1 filter（读动词）**
  data = `{rows: [[...]], columns}`；stats = `{rows: N}`。谓词复用 `Predicate::parse`
  （terminal.rs:2905）。不复用 cmd_filter 的临时 Workbook+render 路径（探索报告风险点），
  改为直接产出行集。
- [x] **Task 2.2 sort（mutate）**
  闭包内复用 snapshot_rows/rewrite_rows + cmd_sort（:2226）；data = `{sorted_by,
  descending, rows: N}`。测试含 dry-run（files[].written == false）→ apply → `get` 回读
  断言顺序。
- [x] **Task 2.3 dedup（mutate）**：复用 cmd_dedup；data = `{removed: N, remaining: N}`；
  回读断言。
- [x] **Task 2.4 copy（mutate）**：data = `{source, target, cells: N}`；回读断言。
- [x] **Task 2.5 move（mutate）**：copy+clear 组合；data = `{source, target, cells: N}`；
  源范围回读断言为空。
- [x] **Task 2.6 append（mutate）——与 Task 3.1 多输入协议合并执行**：按表头名对齐语义（复用 cmd_append）；data =
  `{appended: N, matched_columns}`；回读断言。
- [x] **Task 2.7 组2 收尾**：schema×6、SKILL.md、README×2、sync-skills。

## Milestone 3：组3 多工作簿动词（join / pivot / diff）

技术方案 P1 组3 门槛：多工作簿 schema、内存预算、错误/排序语义。

- [x] **Task 3.1 多输入协议扩展**
  `CommandRequest` 相关动词增加第二输入字段（命名 `with: PathBuf`，join/diff 用）；
  workbook_io::open_workbook 复用（资源限制对两个文件各自生效）；错误码沿用文件类（5）
  + 明确 `diagnostic` 指明是哪个输入。args.rs 参数名与 terminal 侧对齐（`--with`）。
- [x] **Task 3.2 pivot（读）**
  data = `{rows, columns}`；`Agg` 枚举 serde 化（sum/count/avg/min/max）。BTreeMap 分组
  语义复用 cmd_pivot（:2154），TSV 拼接改为行集。
- [x] **Task 3.3 join（读）**
  data = `{rows, columns}`；equi-join 语义复用 cmd_join（:2337），去掉临时 Workbook。
  stats = `{rows: N}`。
- [x] **Task 3.4 diff（读，双模式）**
  位置模式 + `--key` 行键模式；data = `{mode, differences: [{kind: "cell"|"added"|
  "removed"|"changed", sheet?, address?, key?, left, right}]}`；stats =
  `{differences: N}`。语义复用 cmd_diff（:1816）/cmd_diff_keyed（:2047）。
- [x] **Task 3.5 组3 收尾**：多输入 schema、SKILL.md（diff/join 需写双文件安全规约：两个
  输入都不得是输出路径）、README×2、sync-skills。

## Milestone 4：组4 样式/元数据/批量动词（format-set / to-number / to-date / style / autofit / name / table / batch）

技术方案 P2 组4 门槛：格式/元数据 result、原子性、覆盖保护。全部 mutate 管道。

- [x] **Task 4.1 format-set（mutate）**：data = `{range, format_code, cells: N}`；回读用
  `format` 命令断言。
- [x] **Task 4.2 to-number（mutate）**：复用 `Sheet::coerce_text_to_numbers`
  （terminal.rs:966 内联逻辑提取）；data = `{converted: N}`；回读 `get --raw` 断言数值化。
- [x] **Task 4.3 to-date（mutate）**：复用 cmd_to_date（:2010）；data =
  `{converted: N, format}`。
- [x] **Task 4.4 style（mutate）**：bold/bg 等映射到 easyexcel styles（Color/FillPattern，
  经 easyexcel_components）；data = `{range, properties}`。
- [x] **Task 4.5 autofit（mutate）**：列宽计算复用 terminal 实现；data = `{columns: N}`。
- [x] **Task 4.6 name（mutate，子动作）**：`CommandRequest::Name { action: add|remove|list }`；
  list 为读（data = `{names: [...]}`），add/remove 为 mutate。与 terminal 的 NameAction
  语义对齐。
- [x] **Task 4.7 table（mutate，子动作）**：同上模式（add/remove/list）。
- [x] **Task 4.8 batch（mutate，原子性核心）**
  data = `{edits: [{cell, value}], applied: N}`；一次 open/一次 save（terminal cmd_batch
  :2562 语义）；dry-run 下全部 edits 校验但不落盘；部分失败 → 整体失败不写（原子性断言
  进测试）。
- [x] **Task 4.9 全量收尾**
  此时 42 个命令全部 Supported：SKILL.md 删除整个禁用清单段落（"Do not use …"）；
  capability_manifest 的 Partial 分支保留机制但数组清空；README×2 状态段与能力表重写；
  sync-skills。

## Milestone 5：v0.1.0 发布

- [x] **Task 5.1 版本对齐**：确定版本号（建议 0.1.0 保持）；Cargo.toml、根 package.json、
  packages/*/package.json × 8 全部一致，`node scripts/check-versions.js <VERSION>` 通过。
- [x] **Task 5.2 capability notes 英文化**：`"交互终端命令已迁移…"` 等面向 agent 的字符串
  改英文（docs 注释保持中文），Partial 数组已空则仅处理残留字符串。
- [x] **Task 5.3 README 状态段更新**：移除 "workspace currently contains development
  changes" 警告，动词表与 capabilities 对齐。
- [ ] **Task 5.4 release.yml 演练**：push tag 前手动触发/act 演练 8 目标构建；每目标
  smoke：`xls --version` + `xls capabilities --json` + fixture（tables.md import→get）。
- [ ] **Task 5.5 npm 发布（顺序：8 平台包 → launcher）**：`npm publish` 各 packages/*，
  再发根包；`npm dist-tag ls` 验证 latest。
- [ ] **Task 5.6 发布后验证矩阵**：8 平台中至少 darwin-arm64 + linux-x64 实机
  `npx @partme.ai/xls-cli capabilities --json`；`npx skills add easy-4-rust/xls-cli`
  走通；GitHub release 附 SHA256SUMS（release.yml 既定产物）。
- [x] **Task 5.7 仓库发布记录**：CHANGELOG.md 记录 0.1.0 能力面。

## Milestone 6：收尾

- [x] **Task 6.1 全量质量门**：`cargo test`（含 terminal.rs 46 个迁移回归）、
  `cargo clippy --all-targets -- -D warnings`、`cargo fmt --check`。
- [x] **Task 6.2 Future Work 占位**：本计划尾节记录 MCP server（`xls mcp` 子命令，
  JSON-RPC over stdio，复用 CommandExecutor——借鉴 OfficeCLI McpServer）、TUI/structured
  收敛、eval 公式覆盖表发布。
- [x] **Task 6.3 计划归档**：勾选完毕的本文档 commit，技术方案文档 §5 阶段 C/D 标记完成。

---

## 执行纪律（superpowers executing-plans）

1. 严格一次一个任务；完成 → 勾选 → `git commit`（动词任务建议一动词一提交，消息含动词名
   与 "structured promotion"）。
2. TDD：每个动词先写失败的进程级 `--json` 契约测试（当前退出码 3），再实现，最后测试
   转绿。
3. 每完成一个动词：`cargo test -p xls-cli` + `node scripts/sync-skills.js` +
   `git diff --stat skills/` 确认同步。
4. 上下文不足（如某 cmd_* 实现与预期不符）→ 停下重读 terminal.rs 对应段与
   docs/XlsCli-Technical-Solution.zh_CN.md，不猜。
5. 发现 easyexcel 引擎缺陷 → 不在本仓库修，记入本计划 "Engine findings" 清单，回
   easyexcel-rust 处理。
6. 每动词提交粒度的验证标准：stdout 单 JSON、stderr 空、错误码稳定、dry-run 时
   files[].written == false。

## 风险与对策

| 风险 | 对策 |
|---|---|
| cmd_* 复用时发现隐式 I/O/全局状态 | 附录 A checklist 含"纯函数确认"步骤；不纯则提取纯化后复用 |
| 多输入（join/diff）语义与 SKILL 安全规约冲突 | Task 3.1 先定协议再实现；两输入路径相同 → 明确错误 |
| terminal 46 个回归测试因共享辅助函数被改而破 | 只新增不改既有 cmd_* 签名；需要重构时先保回归绿再动 |
| npm 首发遇平台包遗漏/版本漂移 | Task 5.1 check-versions 强制；5.4 演练先行 |
| 战线长（约 30 任务）导致中途漂移 | 每 Milestone 结束即收尾任务（schema/SKILL/README 同步），不欠账 |

## 附录 A：动词提升标准链 checklist（每动词任务逐项勾选）

- [ ] 1. args.rs：clap 子命令/参数定义（含 `///` 文档）
- [ ] 2. request.rs：`into_request` 映射
- [ ] 3. command_request.rs：新 variant（含 `///` 文档）+ `command_name()` 分支
- [ ] 4. default_command_executor.rs：match 分支实现（读：data/stats；写：mutate 管道）
- [ ] 5. capability_manifest.rs：从 TERMINAL_ONLY 移入 SUPPORTED
- [ ] 6. schema.rs：详细 schema（不用 `_ =>` 占位）
- [ ] 7. 测试：进程级 `--json` 契约（stdout 单 JSON/stderr 空/退出码）+ 单元级
      （成功/失败/dry-run/overwrite-deny 按动词适用性）
- [ ] 8. README.md + README.zh-CN.md 能力表
- [ ] 9. SKILL.md（命令表/禁用清单）→ `node scripts/sync-skills.js` → git diff 校验 dist
- [ ] 0. 纯函数确认：复用的 cmd_* / 辅助函数无 I/O、无 println、无全局状态；不纯则先纯化
      且 terminal 46 个回归保持绿

## Engine findings（执行中记录，不在本仓库修）

- （暂无）

## Future Work（本计划不做，占位）

- MCP server：`xls mcp` 子命令，JSON-RPC over stdio，暴露 capabilities/get/query 等稳定
  tool，复用 CommandExecutor（借鉴 OfficeCLI McpServer 的 Initialize/ToolsList/ToolsCall
  骨架与 skill 查询）
- TUI 与 structured 命令面收敛（同一动词两种实现的长期统一）
- eval 公式覆盖表随 release 发布（供 agent 判断可用函数）


---

## 执行结果（2026-08-14）

- 43/44 命令 supported；唯一 partial 为 `open`（交互式 TUI，by design）。
- 质量门全绿：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`（0 错误）、
  `cargo test`（116 单元 + 7 进程级协议 + TUI/terminal 回归）。
- 版本对齐：`node scripts/check-versions.js 0.1.0` 通过。
- 共享提取：`cli::predicate`（filter 谓词 DSL）、`cli::row_ops`（行快照/重写/复制/比较）、
  `Aggregation`/`NameAction`/`TableAction`（serde+clap 共用枚举）。
- 协议演进：DryRun 允许就地预览（mutation_target/validate_target），与 terminal guardrail
  语义一致；嵌套子命令（name/table）的 `--output` 为 global。

## 剩余外部步骤 —— 已于 2026-08-14 执行完毕

1. ✅ push main + tag v0.1.0（移动到最终 commit）触发 release.yml：8 平台构建全部
   成功（发布前修正了 CI 固定的 easyexcel-rust ref：92c7f8c[0.1.0] → 4dca346[0.1.3]，
   否则 --locked 必败——这正是 8 月 6 日发布失败的原因）。
2. ✅ CI publish 因 npm --provenance 把目录参数误解析为 git shorthand 失败；改为
   本地发布：下载 8 平台 artifacts → 本地 `npm publish`（8 平台包 + launcher，
   --access public）。release.yml 已去掉 --provenance 供未来使用。
3. ✅ GitHub release v0.1.0 创建：8 个平台二进制 + SHA256SUMS。
4. ✅ 端到端验证：`npm view` dist-tags.latest=0.1.0；`npm install` + `npx xls
   --version`=0.1.0；`import` markdown→xlsx + `eval =SUM` 全链路通过（darwin-arm64）。
   注：新包根 packument 元数据有数分钟传播延迟（tarball 与版本端点即时可用）。

副产物修复：rustfmt 1.88 长属性折行、clippy 1.88 `&&str.to_string` —— CI 工具链
与本地新工具链的差异，main ci 已绿。
