//! ADF (Atlassian Document Format) → plain text for indexing.

use serde_json::Value;

pub fn adf_to_text(node: &Value) -> String {
    match node {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(items) => items.iter().map(adf_to_text).collect(),
        Value::Object(map) => {
            let ntype = map.get("type").and_then(Value::as_str).unwrap_or("");
            if ntype == "text" {
                return map
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
            }
            if ntype == "hardBreak" {
                return "\n".into();
            }
            let inner = map.get("content").map(adf_to_text).unwrap_or_default();
            match ntype {
                "paragraph" => format!("{inner}\n\n"),
                "heading" => format!("## {}\n\n", inner.trim()),
                "listItem" => format!("- {}\n", inner.trim()),
                "codeBlock" => format!("```\n{inner}\n```\n\n"),
                "blockquote" => format!("> {}\n\n", inner.trim()),
                "rule" => "---\n\n".into(),
                "media" | "mediaGroup" => String::new(),
                _ => inner,
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattens_basic_adf_doc() {
        let doc = json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Hello" }]
                },
                {
                    "type": "bulletList",
                    "content": [{
                        "type": "listItem",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Item" }]
                        }]
                    }]
                }
            ]
        });
        let text = adf_to_text(&doc);
        assert!(text.contains("Hello"));
        assert!(text.contains("Item"));
    }
}
