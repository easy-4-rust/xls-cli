# xls-cli

**Excel CLI for agents** — 读写 **XLSX / XLS(BIFF8)/ CSV** 的单二进制工具,
内置 500+ 个 Excel 工作表函数的公式引擎(实测注册表 503 个,含动态数组 spill
与 LAMBDA)。为 LLM 智能体(Claude/OpenClaw/Hermes 等)和脚本场景设计:只读
命令输出 `table|csv|tsv|json|jsonl|md`,修改命令支持 `--dry-run / --backup /
--output`。

底层是 Rust 的 [`xls`](https://github.com/easy-4-rust/xls)(`xls-rs` 的增强
fork),无 Node 原生依赖,无 Python 运行时。

## 安装

```sh
npm i -g xls-cli
```

安装脚本按 `platform × arch` 自动下载对应 GitHub Release 二进制:

| 平台 | x64 | arm64 |
| --- | --- | --- |
| macOS | x86_64-apple-darwin | aarch64-apple-darwin |
| Linux | x86_64-unknown-linux-gnu | aarch64-unknown-linux-gnu |
| Windows | x86_64-pc-windows-msvc | aarch64-pc-windows-msvc |

不依赖网络时(内网/离线),可手动放置二进制到 `bin/xls` 后执行
`npm rebuild xls-cli`。

## 快速上手

```sh
xls --help                          # 41 个子命令总览
xls info report.xlsx                # 元数据
xls get report.xlsx 'A1:J200' --header --format csv
xls query report.xlsx "SELECT * FROM Sheet1 WHERE amount > 1000 ORDER BY amount DESC LIMIT 50"
xls pivot report.xlsx --rows category --values amount --agg sum
xls set report.xlsx A1 '=SUM(B:B)'  # 写入公式并重算
xls diff before.xlsx after.xlsx --key id
xls export old.xls --format xlsx -o new.xlsx
```

## 与智能体集成

### OpenClaw / Claude Code / Hermes(Agent Skills)

仓库的 `skills/xls/SKILL.md` 是通用 Agent Skill:frontmatter(`name`/`description`)
+ 命令文档。安装到任意 agent 的 skills 目录:

```sh
mkdir -p ~/.claude/skills && cp -r skills/xls ~/.claude/skills/
# OpenClaw:
mkdir -p ~/.openclaw/skills && cp -r skills/xls ~/.openclaw/skills/
```

agent 即可按 skill 约定调用 `xls info → xls get/query → xls eval` 的
"探测 → 取数 → 验证" 流程操作电子表格。

### 脚本

```sh
# 管道:stdin CSV → 公式求值
cat data.csv | xls eval - '=SUM(B2:B100)'

# JSON 输出直接喂给 jq
xls get report.xlsx 'A1:J200' --format json | jq '.[0]'

# 修改前先看差异
xls set report.xlsx A1 42 --dry-run
xls set report.xlsx A1 42 --backup    # 落盘前保留 .bak
```

## 命令一览(41)

| 类别 | 命令 |
| --- | --- |
| 元数据 | `info` |
| 读取 | `get`, `format`, `head`, `tail`, `grep` |
| 分析 | `eval`, `profile`, `filter`, `pivot`, `join`, `diff`, `query` |
| 写入 | `new`, `set`, `batch`, `append`, `clear`, `fill`, `copy`, `move`, `insert-row`, `delete-row`, `insert-col`, `delete-col` |
| 转换 | `export`, `import`, `to-number`, `to-date`, `format-set` |
| 结构 | `add-sheet`, `delete-sheet`, `rename-sheet`, `autofit`, `style`, `name`, `table` |
| 整理 | `sort`, `dedup` |

全部命令的旗标与示例见 `skills/xls/SKILL.md`。

## 边界

- **格式**:XLSX/XLS 读+写,CSV/TSV 读+写(BOM/编码自动检测);XLS 公式以
  缓存值存储。
- **加密**:支持解密读取 agile/standard 加密的 XLSX(`xls get -p`);不支持
  legacy RC4 的 .xls 解密、不支持重新加密。
- **不支持**:图表/宏/VBA 编辑;XLSX 部分稀有特性(如内容类型非常规的
  工作簿)走 typed 错误而非静默降级。

## 开发

```sh
# 构建 CLI 二进制(fork 仓库内)
cargo build --features cli --release

# 本地打包模拟 npm 安装
node install.js && node bin/xls.js --version
```

## License

MIT OR Apache-2.0(与上游 `xls-rs` 一致)。
