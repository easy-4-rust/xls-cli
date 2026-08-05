---
name: xls-cli
description: Use the xls binary provided by xls-cli to safely inspect, extract, query, create, edit, recalculate, or convert XLS, XLSX, CSV, Markdown, HTML, and JSON tables. Use for spreadsheet tasks from OpenClaw, Hermes, or another agent. Always probe capabilities, prefer JSON, dry-run writes, avoid source overwrite, and verify generated files by reading them again.
---

# xls-cli agent workflow

Use the `xls` binary as a subprocess. Do not reproduce spreadsheet parsing or writing in the agent.

## Mandatory safety sequence

1. Run `xls capabilities --json` and inspect the requested command's status.
2. For an existing workbook, run `xls info INPUT --json` before selecting ranges or sheets.
3. Use `--json` for machine consumption. Treat stdout as one JSON object; never mix it with logs.
4. For every write, choose a new output path and run the exact command once with `--dry-run`.
5. Run the command without `--dry-run` only after the planned path and warnings are acceptable.
6. Confirm the output exists, then run `xls info OUTPUT --json` and a focused `xls get OUTPUT RANGE --json`.
7. Never add `--force` unless the user explicitly authorizes replacement of that exact path.

## Supported production commands

Use `capabilities` as the runtime authority. This version implements:

- Inspect/extract: `info`, `get`, `head`, `tail`, `query`.
- Edit: `set`, `clear`, `fill`, `insert-row`, `delete-row`, `insert-col`, `delete-col`.
- Workbook: `new`, `add-sheet`, `delete-sheet`, `rename-sheet`, `recalc`.
- Exchange: `convert` for XLS/XLSX/CSV; `import` for Markdown/HTML/JSON to XLS/XLSX; `export` for Markdown/HTML/JSON/CSV/TSV.
- Protocol: `capabilities`, `schema --command NAME`.

Commands reported as `partial` have a migrated human-terminal implementation but do not yet expose the stable JSON result contract. Agents must treat `partial` as unavailable unless a user explicitly asks for an interactive terminal workflow. Never emulate or silently replace them.

## Extract data

```sh
xls capabilities --json
xls info report.xlsx --json
xls get report.xlsx 'Sheet1!A1:J200' --format json --json
xls query report.xlsx 'SELECT * FROM Sheet1 WHERE amount > 1000 LIMIT 50' --json
```

The extracted matrix is in `data.rows`. Query output contains `data.columns` and `data.rows`.

## Generate workbooks from Markdown or HTML

```sh
xls import tables.md generated.xlsx --dry-run --json
xls import tables.md generated.xlsx --json
xls info generated.xlsx --json
xls get generated.xlsx 'Sheet1!A1:F20' --format json --json

xls import local-tables.html generated-from-html.xlsx --dry-run --json
xls import local-tables.html generated-from-html.xlsx --json
```

HTML import parses local static table markup only. It does not execute scripts, load network resources, or apply uncontrolled CSS.

## Edit without overwriting the source

```sh
xls set source.xlsx 'Summary!B2' 42 --output revised.xlsx --dry-run --json
xls set source.xlsx 'Summary!B2' 42 --output revised.xlsx --json
xls get revised.xlsx 'Summary!B2' --json
```

Formula values start with `=`. Recalculate explicitly when needed:

```sh
xls recalc source.xlsx --output recalculated.xlsx --dry-run --json
xls recalc source.xlsx --output recalculated.xlsx --json
```

## Resource and password guardrails

- Keep the defaults unless the user authorizes larger input: 256 MiB per file, 256 sheets, 2,000,000 total rows, and 500,000 formula cells.
- Lower a limit with `--max-file-bytes`, `--max-sheets`, `--max-rows`, or `--max-formula-cells` for untrusted inputs.
- Never put a password directly in arguments. Use `--password-stdin` or `--password-env NAME`; do not echo or log its value.
- Reject unexpected output paths, unsupported capability states, nonzero exit codes, missing output files, and files that cannot be reopened.
