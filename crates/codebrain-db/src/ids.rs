//! Deterministic Surreal record ids derived from stable business keys.

pub fn stable_id(value: &str) -> String {
    blake3::hash(value.as_bytes())
        .to_hex()
        .chars()
        .take(32)
        .collect()
}

pub fn source_id(name: &str) -> String {
    stable_id(name)
}

pub fn file_id(source_name: &str, path: &str) -> String {
    stable_id(&format!("{source_name}:file:{path}"))
}

pub fn symbol_id(source_name: &str, fqn: &str) -> String {
    stable_id(&format!("{source_name}:symbol:{fqn}"))
}

pub fn document_id(source_name: &str, path: &str) -> String {
    stable_id(&format!("{source_name}:document:{path}"))
}

pub fn decision_id(title: &str) -> String {
    stable_id(&format!("decision:{title}"))
}
