---
name: xls
description: >
  MUST USE when the user wants to read, write, analyze, or convert Excel
  spreadsheets — .xlsx, legacy .xls, or .csv files — from the command line:
  查看/读取/写入/修改/分析/汇总/筛选/合并/对比 Excel 文件, xlsx/xls/csv
  电子表格, 公式求值, 透视/分组统计, SQL 查询表格。

  Provides 41 subcommands covering: metadata (info), cell/range reads (get),
  formula evaluation (eval), search (grep/head/tail), analysis
  (profile/pivot/filter/join/diff/query), mutation (set/batch/clear/fill/
  copy/move/insert-row/delete-col/sort/dedup/append/to-number/to-date/
  format-set/autofit/style/new/add-sheet/rename-sheet/name/table), and
  conversion (export/import). Read-only verbs print to stdout; mutating
  verbs edit in place with --dry-run/--backup/--output safety flags.

  NOT for: creating Excel files from code in-process (use a library);
  password-protected .xls (legacy RC4) files.
---

# xls — Excel 读写工具(智能体版)

`xls` 是单二进制 CLI:读写 **XLSX / XLS(BIFF8)/ CSV**,内置 **500+ 个 Excel
工作表函数**(实测注册表 503 个,含动态数组 spill 与 LAMBDA)。无外部运行时。

## 安装

```sh
npm i -g xls-cli        # 或 npx xls-cli <cmd>
# 验证
xls --version && xls --help
```

> npm 包在安装时按平台自动下载二进制(linux/macos/windows × x64/arm64)。

## 读取(打印到 stdout,不修改文件)

```sh
xls info report.xlsx                    # 元数据:格式/工作表/维度/日期系统/命名区域/表格
xls get report.xlsx Sheet1!B2           # 单个单元格显示值
xls get report.xlsx 'A1:J200'           # 范围值(默认表格)
xls get report.xlsx 'A1:J200' --format json --header   # JSON 数组(表头键值)
xls get report.xlsx 'A1:J200' --raw --dates iso        # 存储值;日期 ISO
xls format report.xlsx C2               # 单元格数字格式(如 DATE dd/mm/yyyy)
xls head report.xlsx -n 20              # 前 N 行
xls tail report.xlsx -n 10              # 后 N 行
xls grep report.xlsx ZANMAI             # 含子串的行 + 单元格地址
xls profile report.xlsx amount          # 列统计:计数/和/均值/极值/空值/去重 + "数字存为文本"警告
cat data.csv | xls eval - '=SUM(A:A)'   # 从 stdin 读 CSV
```

## 分析(只读,打印结果)

```sh
xls eval data.csv '=AVERAGE(A1:A10)'                # 公式求值(全 522 函数)
xls filter report.xlsx 'amount>1000' --format csv   # 谓词筛选(H>1000 / B==ZANMAI)
xls pivot report.xlsx --rows category --values amount --agg sum   # 分组聚合
xls join a.xlsx b.xlsx --on id                      # 两表按键内连接
xls diff before.xlsx after.xlsx --key date          # 键控行对比(省略 --key 逐格对比)
xls query report.xlsx "SELECT category, SUM(amount) AS total FROM Sheet1 GROUP BY category ORDER BY total DESC LIMIT 10"
    # 只读 SQL:工作表即表(第 0 行为表头),支持 WHERE/GROUP BY/JOIN/ORDER BY/LIMIT
```

## 写入/修改(就地保存;全部支持 --dry-run/--backup/--output 安全旗标)

```sh
xls new book.xlsx                                   # 新建空工作簿(按扩展名选格式)
xls set report.xlsx A1 '=SUM(B:B)'                  # 设值(公式/数字/文本)+ 重算 + 保存
xls batch report.xlsx --set A1=1 --set B2=hi        # 多次编辑一次原子保存
xls append base.xlsx new.xlsx                       # 按表头对齐追加行
xls sort report.xlsx --by amount --desc             # 稳定多键排序(保留表头)
xls dedup report.xlsx --on id                       # 按键列去重(保留首行)
xls to-number report.xlsx H1:H200                   # 文本数字 → 真数字
xls to-date report.xlsx A2:A83 --format dd/mm/yyyy  # 文本日期 → 真日期
xls format-set report.xlsx C2:C154 'dd/mm/yyyy'     # 设置数字格式
xls clear report.xlsx A1:B10 / xls fill report.xlsx A1:A10 0
xls copy report.xlsx A1:B3 D1 / xls move report.xlsx A1:B3 D1
xls insert-row report.xlsx 3 -n 2 / xls delete-col report.xlsx C
xls add-sheet report.xlsx Summary / xls rename-sheet report.xlsx Sheet1 Data
xls autofit report.xlsx                             # 列宽自适应
xls style report.xlsx A1:D1 --bold --bg FFFF00      # 基础样式
xls name add report.xlsx TaxRate 'Sheet1!$E$1'      # 命名区域
xls table add report.xlsx A1:C20 --name Sales       # Excel 表格对象
xls export old.xls --format xlsx                    # 格式互转
xls import data.csv --into book.xlsx                # CSV 作为新工作表导入
xls export huge.xlsx -f csv -o out.csv --stream     # 大文件内存有界导出
```

## 安全旗标(所有修改命令)

```sh
xls set report.xlsx A1 x --dry-run      # 打印差异,不写文件
xls set report.xlsx A1 x --backup       # 先写 report.xlsx.bak
xls set report.xlsx A1 x --output copy.xlsx   # 写副本,不改原文件
```

## 密码保护文件

```sh
xls info statement.xlsx                 # 无密码:报告加密方案(如 ECMA-376 agile)
xls get statement.xlsx B5 -p secret     # 解密读取(agile/standard)
# 修改加密文件保存为未加密副本(不重新加密)
```

## 边界与约定

- **格式支持**:XLSX/XLS 读+写,CSV/TSV 读+写(delimiter 自动检测、BOM、
  encoding_rs 转码);XLS 公式以缓存值存储。
- **文本数字**:SUM/AVERAGE 等会在求值时强制转换"数字样文本"
  (如 `6,000.00`),单元格保持文本、文件不被改写;需要持久转换用
  `to-number`。
- **不支持**:密码保护 .xls(legacy RC4)解密、重新加密、图表/宏/VBA 编辑。
- **智能体提示**:优先 `info` 探测结构 → `get`/`query` 取数 → `eval` 验证
  公式 → 修改类命令先 `--dry-run` 再落盘;所有输出支持 `--format
  table|csv|tsv|json|jsonl|md`。
