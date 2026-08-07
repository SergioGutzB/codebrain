pub(super) fn symbol_kind(node_kind: &str) -> Option<&'static str> {
    match node_kind {
        "function_definition" => Some("function"),
        "class_definition" => Some("class"),
        _ => None,
    }
}

pub(super) fn is_import(node_kind: &str) -> bool {
    matches!(node_kind, "import_statement" | "import_from_statement")
}

pub(super) fn import_target(raw: &str) -> Option<String> {
    let mut parts = raw.split_whitespace();
    match parts.next()? {
        "from" | "import" => parts
            .next()
            .map(|value| value.trim_matches(',').to_string()),
        _ => None,
    }
}

pub(super) fn is_call(node_kind: &str) -> bool {
    matches!(node_kind, "call")
}
