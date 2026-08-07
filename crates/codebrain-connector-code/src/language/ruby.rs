pub(super) fn symbol_kind(node_kind: &str) -> Option<&'static str> {
    match node_kind {
        "method" | "singleton_method" => Some("function"),
        "class" => Some("class"),
        "module" => Some("module"),
        _ => None,
    }
}

pub(super) fn is_import(node_kind: &str) -> bool {
    matches!(node_kind, "call")
}

/// Ruby has no import syntax: `require`, `require_relative`, and `load` are method calls.
pub(super) fn import_target(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let rest = ["require_relative", "require", "load", "autoload"]
        .into_iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;

    let quoted = rest.trim_start_matches(['(', ' ']);
    let quote = quoted.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    quoted
        .get(1..)?
        .split(quote)
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn is_call(node_kind: &str) -> bool {
    matches!(node_kind, "call")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_require_relative_target() {
        assert_eq!(
            import_target("require_relative 'policy/validity'").as_deref(),
            Some("policy/validity")
        );
        assert_eq!(import_target("require \"json\"").as_deref(), Some("json"));
        assert_eq!(import_target("puts 'hello'"), None);
    }
}
