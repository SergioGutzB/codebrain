use tree_sitter::Node;

pub(super) fn symbol_kind(node: Node<'_>, source: &[u8]) -> Option<&'static str> {
    match node.kind() {
        "function_declaration" | "method_definition" => Some("function"),
        "class_declaration" => Some("class"),
        "interface_declaration" => Some("interface"),
        "type_alias_declaration" => Some("type"),
        "enum_declaration" => Some("enum"),
        "variable_declarator" if has_function_value(node, source) => Some("function"),
        _ => None,
    }
}

fn has_function_value(node: Node<'_>, _source: &[u8]) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|value| matches!(value.kind(), "arrow_function" | "function_expression"))
}

pub(super) fn is_import(node_kind: &str) -> bool {
    matches!(node_kind, "import_statement")
}

pub(super) fn import_target(raw: &str) -> Option<String> {
    let end = raw.rfind(['\'', '"'])?;
    let quote = *raw.as_bytes().get(end)? as char;
    let start = raw.get(..end)?.rfind(quote)?;
    raw.get(start + 1..end).map(str::to_string)
}

pub(super) fn is_call(node_kind: &str) -> bool {
    matches!(node_kind, "call_expression" | "new_expression")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gets_module_from_import() {
        assert_eq!(
            import_target("import { sum } from './math';").as_deref(),
            Some("./math")
        );
    }
}
