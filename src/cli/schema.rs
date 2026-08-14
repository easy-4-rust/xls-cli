use serde_json::{Value, json};

use crate::CommandName;

/// 返回指定命令的简化 JSON Schema。
#[allow(
    clippy::too_many_lines,
    reason = "命令 schema 集中维护，拆散削弱协议审计性"
)]
#[must_use]
pub fn command_schema(command: CommandName) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert("command".to_owned(), json!({ "const": command.as_str() }));
    let mut required = vec![json!("command")];
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": command.as_str(),
        "type": "object",
        "properties": {},
        "required": [],
        "additionalProperties": true,
        "x-schema-version": "1.0"
    });
    match command {
        CommandName::Info => add_path(&mut properties, &mut required, "input"),
        CommandName::Get => {
            add_path(&mut properties, &mut required, "input");
            properties.insert("range".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert(
                "output_format".to_owned(),
                json!({"enum": ["json", "csv", "tsv", "markdown", "html"]}),
            );
        }
        CommandName::Import => {
            add_path(&mut properties, &mut required, "input");
            add_path(&mut properties, &mut required, "output");
            properties.insert(
                "markdown_options".to_owned(),
                json!({
                    "type": ["object", "null"],
                    "properties": {
                        "tables": {
                            "description": "all，或带 index/name 的表格选择对象"
                        },
                        "type_inference": {
                            "enum": ["text", "conservative", "aggressive"]
                        },
                        "apply_header_style": {"type": "boolean"}
                    }
                }),
            );
        }
        CommandName::Export => {
            add_path(&mut properties, &mut required, "input");
            add_path(&mut properties, &mut required, "output");
            properties.insert(
                "output_format".to_owned(),
                json!({"enum": ["json", "csv", "tsv", "markdown", "html"]}),
            );
            required.push(json!("output_format"));
            properties.insert(
                "markdown_options".to_owned(),
                json!({
                    "type": ["object", "null"],
                    "properties": {
                        "profile": {"enum": ["agent-stable", "human-readable"]},
                        "mode": {"enum": ["auto", "event", "workbook"]},
                        "formulas": {"enum": ["cached", "expression", "both"]},
                        "merges": {"enum": ["anchor", "repeat", "html", "error"]}
                    }
                }),
            );
        }
        CommandName::Grep => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "pattern".to_owned(),
                json!({"type": "string", "description": "大小写不敏感的子串"}),
            );
            required.push(json!("pattern"));
            properties.insert(
                "sheet".to_owned(),
                json!({"type": ["string", "null"], "description": "缺省为活跃工作表"}),
            );
        }
        CommandName::Profile => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "column".to_owned(),
                json!({"type": "string", "description": "表头名或列字母"}),
            );
            required.push(json!("column"));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Eval => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "formula".to_owned(),
                json!({"type": "string", "description": "如 =SUM(A1:A10)"}),
            );
            required.push(json!("formula"));
            properties.insert(
                "at".to_owned(),
                json!({"type": ["string", "null"], "description": "[Sheet!]A1 相对引用上下文"}),
            );
        }
        CommandName::Format => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "cell".to_owned(),
                json!({"type": "string", "description": "如 C2 或 Sheet1!C2"}),
            );
            required.push(json!("cell"));
        }
        CommandName::Filter => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "predicate".to_owned(),
                json!({"type": "string", "description": "如 amount>1000、name~ali、col:number"}),
            );
            required.push(json!("predicate"));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Sort => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "by".to_owned(),
                json!({"type": "array", "items": {"type": "string"}, "minItems": 1}),
            );
            required.push(json!("by"));
            properties.insert("desc".to_owned(), json!({"type": "boolean"}));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Dedup => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "on".to_owned(),
                json!({"type": "array", "items": {"type": "string"}, "description": "键列；缺省整行"}),
            );
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Copy | CommandName::Move => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "source".to_owned(),
                json!({"type": "string", "description": "源范围，如 A1:B3"}),
            );
            required.push(json!("source"));
            properties.insert(
                "target".to_owned(),
                json!({"type": "string", "description": "目标左上角单元格，如 D1"}),
            );
            required.push(json!("target"));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Pivot => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "rows".to_owned(),
                json!({"type": "string", "description": "分组列（字母或表头名）"}),
            );
            required.push(json!("rows"));
            properties.insert(
                "values".to_owned(),
                json!({"type": "string", "description": "聚合数值列"}),
            );
            required.push(json!("values"));
            properties.insert(
                "agg".to_owned(),
                json!({"enum": ["sum", "count", "mean", "min", "max"]}),
            );
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Append => {
            add_path(&mut properties, &mut required, "input");
            add_path(&mut properties, &mut required, "with");
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Join => {
            add_path(&mut properties, &mut required, "input");
            add_path(&mut properties, &mut required, "with");
            properties.insert(
                "on".to_owned(),
                json!({"type": "string", "description": "连接键列，两侧同名"}),
            );
            required.push(json!("on"));
        }
        CommandName::Diff => {
            add_path(&mut properties, &mut required, "input");
            add_path(&mut properties, &mut required, "with");
            properties.insert(
                "key".to_owned(),
                json!({"type": ["string", "null"], "description": "键列；提供时做行键比较"}),
            );
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::FormatSet => {
            add_path(&mut properties, &mut required, "input");
            properties.insert("range".to_owned(), json!({"type": "string"}));
            required.push(json!("range"));
            properties.insert("code".to_owned(), json!({"type": "string"}));
            required.push(json!("code"));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::ToNumber | CommandName::ToDate => {
            add_path(&mut properties, &mut required, "input");
            properties.insert("range".to_owned(), json!({"type": "string"}));
            required.push(json!("range"));
            if matches!(command, CommandName::ToDate) {
                properties.insert(
                    "format".to_owned(),
                    json!({"type": "string", "description": "源文本日期格式"}),
                );
                required.push(json!("format"));
            }
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Autofit => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "columns".to_owned(),
                json!({"type": ["string", "null"], "description": "列范围如 A:C；缺省全部"}),
            );
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Style => {
            add_path(&mut properties, &mut required, "input");
            properties.insert("range".to_owned(), json!({"type": "string"}));
            required.push(json!("range"));
            properties.insert("bold".to_owned(), json!({"type": "boolean"}));
            properties.insert("italic".to_owned(), json!({"type": "boolean"}));
            properties.insert(
                "color".to_owned(),
                json!({"type": ["string", "null"], "description": "RRGGBB"}),
            );
            properties.insert(
                "bg".to_owned(),
                json!({"type": ["string", "null"], "description": "RRGGBB"}),
            );
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Name | CommandName::Table => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "action".to_owned(),
                json!({
                    "oneOf": [
                        {"type": "string", "const": "list"},
                        {"type": "object", "properties": {"action": {"const": "add"}}},
                        {"type": "object", "properties": {"action": {"const": "remove"}}}
                    ]
                }),
            );
            required.push(json!("action"));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Batch => {
            add_path(&mut properties, &mut required, "input");
            properties.insert(
                "sets".to_owned(),
                json!({"type": "array", "items": {"type": "string"}, "description": "形如 A1=值 或 Sheet1!A1=值"}),
            );
            required.push(json!("sets"));
            properties.insert("sheet".to_owned(), json!({"type": ["string", "null"]}));
            properties.insert("output".to_owned(), json!({"type": ["string", "null"]}));
        }
        CommandName::Capabilities => {}
        CommandName::Schema => {
            properties.insert("target".to_owned(), json!({"type": "string"}));
            required.push(json!("target"));
        }
        _ => {
            schema["description"] =
                json!("该命令的请求类型已保留；请先读取 capabilities 判断当前构建是否支持。");
        }
    }
    schema["properties"] = Value::Object(properties);
    schema["required"] = Value::Array(required);
    schema
}

fn add_path(
    properties: &mut serde_json::Map<String, Value>,
    required: &mut Vec<Value>,
    name: &str,
) {
    properties.insert(name.to_owned(), json!({"type": "string"}));
    required.push(json!(name));
}
