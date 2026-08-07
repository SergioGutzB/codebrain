pub(super) fn symbol_kind(node_kind: &str) -> Option<&'static str> {
    match node_kind {
        "function_item" => Some("function"),
        "struct_item" => Some("class"),
        "enum_item" => Some("enum"),
        "trait_item" => Some("interface"),
        "mod_item" => Some("module"),
        "type_item" => Some("type"),
        "const_item" | "static_item" => Some("constant"),
        _ => None,
    }
}

pub(super) fn is_import(node_kind: &str) -> bool {
    matches!(node_kind, "use_declaration")
}

pub(super) fn import_target(raw: &str) -> Option<String> {
    raw.trim()
        .strip_prefix("use ")?
        .trim_end_matches(';')
        .split(['{', ','])
        .next()
        .map(str::trim)
        .map(|value| value.trim_end_matches("::"))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn is_call(node_kind: &str) -> bool {
    matches!(node_kind, "call_expression" | "macro_invocation")
}
