---
name: xls-cli
description: Safely inspect, extract, query, create, edit, recalculate, import, export, or convert local XLS, XLSX, CSV, Markdown, HTML, and JSON tables with the xls binary. Use when an OpenClaw, Hermes, Codex, or other agent must work with spreadsheet files, produce machine-readable table data, generate workbooks from Markdown/HTML, export spreadsheets to AgentStable Markdown, or modify a workbook without data loss. Always discover runtime capabilities, use JSON for automation, dry-run writes, avoid source overwrite, preserve warnings, and reopen generated files for verification.
---

# xls-cli

Use the `xls` binary as a subprocess. Let EasyExcel perform spreadsheet parsing, formula handling, and file generation; never reproduce those algorithms in the agent.

## Establish the runtime contract

1. Run `command -v xls` and `xls --version`. If the binary is missing, stop and report that the package must be installed; do not install software unless the user asks.
2. Run `xls capabilities --json` for every new runtime or task session.
3. Select only a command whose capability status is `supported`.
4. Run `xls schema --command NAME --json` before constructing an unfamiliar structured request.
5. Treat `partial` as unavailable to automation. Never parse its human terminal output as a replacement JSON API.

Use subprocess argument arrays when possible. If a shell is unavoidable, quote every path, A1 range, formula, and SQL expression.

## Follow the mandatory safety sequence

For reads:

```text
capabilities → info → focused read/query → validate result
```

For writes:

```text
capabilities → info when input exists → choose new output → dry-run
→ review files/warnings/stats → apply → info output → focused get output
```

Apply these rules:

- Add `--json` to every structured automation call.
- Write to a new output path by default.
- Run the exact write command with `--dry-run` before applying it.
- Require `files[].written == false` during dry-run and `true` after apply.
- Never add `--force` unless the user explicitly authorizes replacement of that exact path.
- Never delete the source or generated file as an implicit cleanup step.
- Treat every item in `warnings` as result data. Explain material projection or fidelity loss to the user.
- Reopen every generated XLS/XLSX/CSV file with `xls info`, then verify the intended sheet/range with `xls get`.

## Select commands by task

| Goal | Structured commands | Notes |
|---|---|---|
| Inspect | `info`, `get`, `head`, `tail` | Use a narrow range after discovering sheet names. |
| Search cells | `grep` | Case-insensitive substring over displayed values; matches carry addresses. |
| Column quality | `profile` | Column stats plus `NUMBERS_STORED_AS_TEXT` / `DATES_STORED_AS_TEXT` warnings. |
| Compute | `eval` | Single formula; scalars return `data.value`, arrays return `data.grid`. |
| Number format | `format` | Cell number-format category and code. |
| Filter rows | `filter` | Predicate DSL `amount>1000`, `name~ali`, `col:number`; returns matched rows. |
| Reshape rows | `sort`, `dedup`, `copy`, `move`, `append` | Mutating verbs; always dry-run first, write to a new output path. |
| Aggregate | `pivot` | Group by one column, aggregate another (sum/count/mean/min/max). |
| Combine workbooks | `join`, `diff` | Two inputs (`input` + `with`); join on a shared key column, diff keyed or cell-level. |
| Query | `query` | Read-only SQL; sheets are tables and row 0 is the header. |
| Edit cells | `set`, `clear`, `fill` | Provide `--output` unless creating a new workbook. |
| Edit axes | `insert-row`, `delete-row`, `insert-col`, `delete-col` | CLI positions are zero-based. |
| Manage workbook | `new`, `add-sheet`, `delete-sheet`, `rename-sheet`, `recalc` | Reopen after every applied write. |
| Exchange workbook formats | `convert` | Use for XLS/XLSX/CSV container conversion. |
| Import tables | `import` | Markdown/HTML/JSON to XLS/XLSX/XLS/CSV. |
| Export tables | `export` | Markdown/HTML/JSON/CSV/TSV output. |
| Discover protocol | `capabilities`, `schema` | Runtime truth; do not cache indefinitely. |

Do not use `open`, `format-set`, `to-number`, `to-date`, `style`, `autofit`, `batch`, `name`, or `table` when capability marks them `partial`, unless the user explicitly requests a human-operated terminal workflow. For `join`, `diff`, and `append`, the second input (`with`) must differ from the output path, and neither input may be overwritten.

## Inspect and extract data

```sh
xls capabilities --json
xls info report.xlsx --json
xls get report.xlsx 'Sales!A1:J200' --format json --json
xls head report.xlsx --sheet Sales -n 20 --format json --json
xls query report.xlsx \
  'SELECT category, SUM(amount) AS total FROM Sales GROUP BY category ORDER BY total DESC' \
  --json
```

Read extraction rows from `data.rows`. Read query columns from `data.columns` and rows from `data.rows`. Validate that the selected sheet, range, row count, and columns match the request before presenting the result.

## Import Markdown, HTML, or JSON

```sh
xls import tables.md generated.xlsx \
  --infer-types conservative --dry-run --json
xls import tables.md generated.xlsx \
  --infer-types conservative --json
xls info generated.xlsx --json
xls get generated.xlsx 'Sales!A1:F20' --format json --json
```

Use these import rules:

- Prefer `--infer-types conservative`; it preserves identifiers such as `007` as text.
- Use `--table NAME_OR_ZERO_BASED_INDEX` when selecting one Markdown table.
- Require an explicit table selection when importing multi-table Markdown to CSV.
- Treat Markdown text beginning with `=` as text, not as an instruction to create a formula.
- Parse local static HTML tables only. Do not fetch linked resources or execute scripts.

## Export AgentStable Markdown

```sh
xls export report.xlsx report.md --format markdown \
  --mode auto --formula cached --merge anchor --dry-run --json
xls export report.xlsx report.md --format markdown \
  --mode auto --formula cached --merge anchor --json
```

Choose Markdown options deliberately:

- Use `--mode auto` by default.
- Use `--mode event` only for XLSX/CSV input with cached formulas and compatible merge handling.
- Never request Event Mode for XLS. Use Workbook Mode for XLS, formula expressions, `repeat`, `html`, or `error` merge policies.
- Use `--formula cached` for stable agent output; use `expression` or `both` only when the user needs formulas displayed.
- Use `--merge anchor` by default and preserve its structured warnings. Select `repeat`, `html`, or `error` only for an explicit requirement.
- Use `--sheet NAME_OR_ZERO_BASED_INDEX` to restrict export when requested.

Markdown is a semantic projection, not a lossless workbook round trip. Report warnings about hidden sheets, merges, styles, images, charts, comments, macros, or unavailable metadata.

## Edit without overwriting the source

```sh
xls set source.xlsx 'Summary!B2' 42 \
  --output revised.xlsx --dry-run --json
xls set source.xlsx 'Summary!B2' 42 \
  --output revised.xlsx --json
xls info revised.xlsx --json
xls get revised.xlsx 'Summary!B2' --format json --json
```

Values beginning with `=` create formulas. Run `recalc` explicitly when the task requires refreshed cached results:

```sh
xls recalc revised.xlsx --output recalculated.xlsx --dry-run --json
xls recalc revised.xlsx --output recalculated.xlsx --json
```

## Handle errors and warnings

- On a nonzero exit, parse the JSON error when present and branch on its stable `code`.
- Correct `INVALID_ARGUMENT` or `SHEET_NOT_FOUND` by re-reading help/info; do not retry unchanged.
- Correct `OVERWRITE_DENIED` by choosing a new output path. Do not escalate to `--force` automatically.
- Correct `RESOURCE_LIMIT` only by narrowing the task or after the user authorizes a larger explicit limit.
- Treat `UNSUPPORTED_COMMAND` or `UNSUPPORTED_FORMAT` as a hard capability boundary.
- Treat `READ_FAILED` and `WRITE_FAILED` as evidence that the task did not complete. Preserve the source and diagnostic context.
- Never declare success from exit code alone; validate the JSON shape, generated file, reopen operation, and focused data assertion.

## Enforce resource and secret guardrails

- Keep the defaults unless the user authorizes more: 256 MiB per file, 256 sheets, 2,000,000 total rows, and 500,000 formula cells.
- Lower limits for untrusted input with `--max-file-bytes`, `--max-sheets`, `--max-rows`, and `--max-formula-cells`.
- Never place passwords in argv, prompts, logs, or result summaries. Use `--password-stdin` or `--password-env NAME` and pass only the environment variable name.
- Do not expose stdout/stderr containing secrets. In JSON mode, expect exactly one JSON document on stdout and diagnostics only on stderr.
