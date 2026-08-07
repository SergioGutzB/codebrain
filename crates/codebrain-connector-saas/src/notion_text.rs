//! Notion block / rich_text → plain text for indexing.

use serde_json::Value;

pub fn rich_text_to_plain(items: &[Value]) -> String {
    items
        .iter()
        .filter_map(|item| item.get("plain_text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("")
}

pub fn block_to_text(block: &Value) -> String {
    let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
    let payload = block.get(btype).cloned().unwrap_or(Value::Null);
    let rich = payload
        .get("rich_text")
        .or_else(|| payload.get("text"))
        .and_then(Value::as_array)
        .map(|items| rich_text_to_plain(items))
        .unwrap_or_default();

    match btype {
        "paragraph" => {
            if rich.is_empty() {
                String::new()
            } else {
                format!("{rich}\n\n")
            }
        }
        "heading_1" => format!("# {rich}\n\n"),
        "heading_2" => format!("## {rich}\n\n"),
        "heading_3" => format!("### {rich}\n\n"),
        "bulleted_list_item" | "numbered_list_item" => format!("- {rich}\n"),
        "to_do" => {
            let checked = payload
                .get("checked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mark = if checked { "x" } else { " " };
            format!("- [{mark}] {rich}\n")
        }
        "quote" | "callout" => format!("> {rich}\n\n"),
        "code" => {
            let language = payload
                .get("language")
                .and_then(Value::as_str)
                .unwrap_or("");
            format!("```{language}\n{rich}\n```\n\n")
        }
        "divider" => "---\n\n".into(),
        "toggle" => format!("{rich}\n\n"),
        "child_page" => {
            let title = payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("child page");
            format!("[page: {title}]\n\n")
        }
        "child_database" => {
            let title = payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("database");
            format!("[database: {title}]\n\n")
        }
        "bookmark" | "link_preview" | "embed" => {
            let url = payload.get("url").and_then(Value::as_str).unwrap_or("");
            if url.is_empty() {
                String::new()
            } else {
                format!("{url}\n\n")
            }
        }
        "image" | "file" | "video" | "pdf" | "audio" => String::new(),
        "table" | "column_list" | "column" | "synced_block" => String::new(),
        _ => {
            if rich.is_empty() {
                String::new()
            } else {
                format!("{rich}\n")
            }
        }
    }
}

pub fn page_title_from_properties(properties: &Value) -> String {
    let Some(map) = properties.as_object() else {
        return "Untitled".into();
    };
    for value in map.values() {
        if value.get("type").and_then(Value::as_str) != Some("title") {
            continue;
        }
        if let Some(items) = value.get("title").and_then(Value::as_array) {
            let title = rich_text_to_plain(items).trim().to_string();
            if !title.is_empty() {
                return title;
            }
        }
    }
    "Untitled".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_title_and_paragraph() {
        let props = json!({
            "Name": {
                "type": "title",
                "title": [{ "plain_text": "Design Doc" }]
            }
        });
        assert_eq!(page_title_from_properties(&props), "Design Doc");

        let block = json!({
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "plain_text": "Hello MM-147" }]
            }
        });
        assert!(block_to_text(&block).contains("Hello MM-147"));
    }
}
