use serde_json::{Value, json};

use crate::CommandName;

/// 返回指定命令的简化 JSON Schema。
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
