mod python;
mod ruby;
mod rust;
mod typescript;

use std::path::{Path, PathBuf};

use chrono::Utc;
use codebrain_connector::{EdgeCandidate, EdgeType, ExtractBatch, FileNode, SymbolNode, WorkItem};
use tree_sitter::{Language as TreeSitterLanguage, Node, Parser};

use crate::error::{CodeConnectorError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    Python,
    Ruby,
}

impl Language {
    pub const ALL: [Self; 5] = [
        Self::Rust,
        Self::TypeScript,
        Self::Tsx,
        Self::Python,
        Self::Ruby,
    ];

    pub fn from_path(path: &Path) -> Option<Self> {
        if matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("Rakefile" | "Gemfile")
        ) {
            return Some(Self::Ruby);
        }
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("rs") => Some(Self::Rust),
            Some("ts") => Some(Self::TypeScript),
            Some("tsx") => Some(Self::Tsx),
            Some("py") => Some(Self::Python),
            Some("rb" | "rake" | "gemspec") => Some(Self::Ruby),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Python => "python",
            Self::Ruby => "ruby",
        }
    }

    fn grammar(self) -> TreeSitterLanguage {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        }
    }
}

impl TryFrom<&str> for Language {
    type Error = CodeConnectorError;

    fn try_from(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Ok(Self::Rust),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "tsx" => Ok(Self::Tsx),
            "python" | "py" => Ok(Self::Python),
            "ruby" | "rb" => Ok(Self::Ruby),
            _ => Err(CodeConnectorError::UnsupportedLanguage(PathBuf::from(
                value,
            ))),
        }
    }
}

#[derive(Debug, Default)]
pub struct ParsedCode {
    pub symbols: Vec<SymbolNode>,
    pub edges: Vec<EdgeCandidate>,
}

pub(crate) fn extract(item: &WorkItem) -> Result<ExtractBatch> {
    let path = PathBuf::from(&item.path);
    let language = Language::from_path(&path)
        .ok_or_else(|| CodeConnectorError::UnsupportedLanguage(path.clone()))?;
    let source = std::fs::read(&path).map_err(|source| CodeConnectorError::Io {
        path: path.clone(),
        source,
    })?;
    let parsed = parse_source(language, &item.id, &source)?;
    let content_hash = item
        .content_hash
        .clone()
        .unwrap_or_else(|| blake3::hash(&source).to_hex().to_string());
    let file = FileNode {
        path: item.id.clone(),
        language: Some(language.as_str().to_string()),
        content_hash,
        mtime: item.mtime.unwrap_or_else(Utc::now),
    };

    Ok(ExtractBatch {
        files: vec![file],
        symbols: parsed.symbols,
        edges: parsed.edges,
        ..ExtractBatch::default()
    })
}

pub fn parse_source(language: Language, relative_path: &str, source: &[u8]) -> Result<ParsedCode> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar())
        .map_err(|_| CodeConnectorError::Grammar {
            language: language.as_str(),
        })?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| CodeConnectorError::Parse(PathBuf::from(relative_path)))?;

    let module = module_path(relative_path);
    let mut parsed = ParsedCode::default();
    walk(
        language,
        tree.root_node(),
        source,
        relative_path,
        &module,
        &mut Vec::new(),
        None,
        &mut parsed,
    );
    Ok(parsed)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    language: Language,
    node: Node<'_>,
    source: &[u8],
    file_path: &str,
    module: &str,
    scopes: &mut Vec<String>,
    current_symbol: Option<&str>,
    parsed: &mut ParsedCode,
) {
    let symbol = symbol_at(language, node, source, file_path, module, scopes);
    let symbol_fqn = symbol.as_ref().map(|value| value.fqn.clone());
    if let Some(symbol) = symbol {
        scopes.push(symbol.name.clone());
        parsed.symbols.push(symbol);
    }
    let active_symbol = symbol_fqn.as_deref().or(current_symbol);

    // Ruby has no import syntax, so `require` and ordinary calls share the same node kind:
    // try the import reading first and only fall back to a call edge when it yields nothing.
    let import = is_import(language, node.kind())
        .then(|| import_target(language, node, source))
        .flatten();

    if let Some(target) = import {
        parsed.edges.push(EdgeCandidate {
            edge_type: EdgeType::Imports,
            from_key: format!("file:{file_path}"),
            to_key: format!("import:{target}"),
            confidence: Some(1.0),
            evidence: text(node, source).map(str::to_string),
        });
    } else if is_call(language, node.kind())
        && let (Some(from), Some(target)) = (active_symbol, call_target(node, source))
    {
        parsed.edges.push(EdgeCandidate {
            edge_type: EdgeType::Calls,
            from_key: format!("symbol:{from}"),
            to_key: format!("call:{target}"),
            confidence: Some(0.8),
            evidence: text(node, source).map(str::to_string),
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(
            language,
            child,
            source,
            file_path,
            module,
            scopes,
            active_symbol,
            parsed,
        );
    }

    if symbol_fqn.is_some() {
        scopes.pop();
    }
}

fn symbol_at(
    language: Language,
    node: Node<'_>,
    source: &[u8],
    file_path: &str,
    module: &str,
    scopes: &[String],
) -> Option<SymbolNode> {
    let kind = match language {
        Language::Rust => rust::symbol_kind(node.kind()),
        Language::TypeScript | Language::Tsx => typescript::symbol_kind(node, source),
        Language::Python => python::symbol_kind(node.kind()),
        Language::Ruby => ruby::symbol_kind(node.kind()),
    }?;
    let name_node = node.child_by_field_name("name")?;
    let name = text(name_node, source)?.to_string();
    let mut parts = Vec::with_capacity(scopes.len() + 2);
    if !module.is_empty() {
        parts.push(module.to_string());
    }
    parts.extend(scopes.iter().cloned());
    parts.push(name.clone());

    Some(SymbolNode {
        file_path: file_path.to_string(),
        name,
        fqn: parts.join("::"),
        kind: kind.to_string(),
        signature: signature(node, source),
        start_line: node.start_position().row as i64 + 1,
        end_line: node.end_position().row as i64 + 1,
        content_hash: blake3::hash(source.get(node.byte_range()).unwrap_or_default())
            .to_hex()
            .to_string(),
    })
}

fn module_path(relative_path: &str) -> String {
    relative_path
        .trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".rs")
        .trim_end_matches(".py")
        .trim_end_matches(".rb")
        .trim_end_matches("/mod")
        .trim_end_matches("/index")
        .replace('/', "::")
}

fn signature(node: Node<'_>, source: &[u8]) -> Option<String> {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |body| body.start_byte())
        .min(node.start_byte().saturating_add(500));
    std::str::from_utf8(source.get(node.start_byte()..end)?)
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_import(language: Language, kind: &str) -> bool {
    match language {
        Language::Rust => rust::is_import(kind),
        Language::TypeScript | Language::Tsx => typescript::is_import(kind),
        Language::Python => python::is_import(kind),
        Language::Ruby => ruby::is_import(kind),
    }
}

fn import_target(language: Language, node: Node<'_>, source: &[u8]) -> Option<String> {
    let raw = text(node, source)?;
    match language {
        Language::Rust => rust::import_target(raw),
        Language::TypeScript | Language::Tsx => typescript::import_target(raw),
        Language::Python => python::import_target(raw),
        Language::Ruby => ruby::import_target(raw),
    }
}

fn is_call(language: Language, kind: &str) -> bool {
    match language {
        Language::Rust => rust::is_call(kind),
        Language::TypeScript | Language::Tsx => typescript::is_call(kind),
        Language::Python => python::is_call(kind),
        Language::Ruby => ruby::is_call(kind),
    }
}

fn call_target(node: Node<'_>, source: &[u8]) -> Option<String> {
    let callable = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("method"))
        .or_else(|| node.child_by_field_name("name"))?;
    let raw = text(callable, source)?;
    raw.rsplit([':', '.', '/'])
        .find(|part| !part.is_empty())
        .map(|part| {
            part.trim_matches(|character: char| !character.is_alphanumeric() && character != '_')
        })
        .filter(|part| !part.is_empty())
        .map(str::to_string)
}

fn text<'a>(node: Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    std::str::from_utf8(source.get(node.byte_range())?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_symbols_imports_and_calls() {
        let source = br#"
            use crate::math;
            pub fn run() { math::sum(); }
        "#;
        let parsed = parse_source(Language::Rust, "src/lib.rs", source).expect("parse");

        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.fqn == "src::lib::run")
        );
        assert!(
            parsed
                .edges
                .iter()
                .any(|edge| edge.edge_type == EdgeType::Imports)
        );
        assert!(
            parsed
                .edges
                .iter()
                .any(|edge| edge.edge_type == EdgeType::Calls)
        );
    }

    #[test]
    fn extracts_typescript_and_python_symbols() {
        let ts = parse_source(
            Language::TypeScript,
            "src/service.ts",
            b"export function login(): void {} class User {}",
        )
        .expect("parse typescript");
        let py = parse_source(
            Language::Python,
            "app/auth.py",
            b"class Auth:\n    def login(self):\n        verify()\n",
        )
        .expect("parse python");

        assert_eq!(ts.symbols.len(), 2);
        assert_eq!(py.symbols.len(), 2);
    }

    #[test]
    fn extracts_ruby_modules_requires_and_calls() {
        let source = br#"
require_relative 'services/greeter'

module App
  class Runner
    def call(name)
      Services::Greeter.new.greet(name)
    end
  end
end
"#;
        let parsed = parse_source(Language::Ruby, "ruby/app.rb", source).expect("parse ruby");

        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.fqn == "ruby::app::App::Runner::call")
        );
        assert!(
            parsed
                .edges
                .iter()
                .any(|edge| edge.edge_type == EdgeType::Imports
                    && edge.to_key == "import:services/greeter")
        );
        assert!(
            parsed
                .edges
                .iter()
                .any(|edge| edge.edge_type == EdgeType::Calls)
        );
    }
}
