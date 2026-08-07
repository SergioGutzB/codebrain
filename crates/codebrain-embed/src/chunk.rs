use codebrain_connector::{DocumentNode, SymbolNode};

/// Intermediate chunk before an embedding is attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDraft {
    pub parent_key: String,
    pub ordinal: i64,
    pub text: String,
}

const DEFAULT_DOC_CHARS: usize = 1200;
const DEFAULT_DOC_OVERLAP: usize = 150;

/// One chunk per symbol: kind, FQN, signature.
pub fn chunk_symbol(source: &str, symbol: &SymbolNode) -> ChunkDraft {
    let mut parts = vec![
        format!("{} {}", symbol.kind, symbol.name),
        symbol.fqn.clone(),
    ];
    if let Some(signature) = &symbol.signature {
        parts.push(signature.clone());
    }
    parts.push(format!("file {}", symbol.file_path));
    ChunkDraft {
        parent_key: format!("symbol:{source}:{}", symbol.fqn),
        ordinal: 0,
        text: parts.join("\n"),
    }
}

/// Sliding windows over a document body (plus title/tags as the first chunk).
pub fn chunk_document(source: &str, document: &DocumentNode) -> Vec<ChunkDraft> {
    chunk_document_sized(source, document, DEFAULT_DOC_CHARS, DEFAULT_DOC_OVERLAP)
}

pub fn chunk_document_sized(
    source: &str,
    document: &DocumentNode,
    window: usize,
    overlap: usize,
) -> Vec<ChunkDraft> {
    let parent = format!("document:{source}:{}", document.path);
    let mut drafts = Vec::new();
    let header = {
        let mut parts = vec![document.title.clone()];
        if !document.tags.is_empty() {
            parts.push(format!("tags: {}", document.tags.join(", ")));
        }
        if !document.aliases.is_empty() {
            parts.push(format!("aliases: {}", document.aliases.join(", ")));
        }
        parts.join("\n")
    };
    drafts.push(ChunkDraft {
        parent_key: parent.clone(),
        ordinal: 0,
        text: header,
    });

    let body = document.body.trim();
    if body.is_empty() {
        return drafts;
    }

    let window = window.max(64);
    let overlap = overlap.min(window / 2);
    let mut start = 0;
    let mut ordinal = 1_i64;
    let chars: Vec<char> = body.chars().collect();
    while start < chars.len() {
        let end = (start + window).min(chars.len());
        let slice: String = chars[start..end].iter().collect();
        drafts.push(ChunkDraft {
            parent_key: parent.clone(),
            ordinal,
            text: slice,
        });
        ordinal += 1;
        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(overlap);
        if start == 0 {
            start = end;
        }
    }
    drafts
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn chunks_symbol_with_signature() {
        let symbol = SymbolNode {
            file_path: "a.rb".into(),
            name: "Greeter".into(),
            fqn: "Services::Greeter".into(),
            kind: "class".into(),
            signature: Some("class Greeter".into()),
            start_line: 1,
            end_line: 2,
            content_hash: "h".into(),
        };
        let chunk = chunk_symbol("code", &symbol);
        assert_eq!(chunk.parent_key, "symbol:code:Services::Greeter");
        assert!(chunk.text.contains("Services::Greeter"));
        assert!(chunk.text.contains("class Greeter"));
    }

    #[test]
    fn chunks_long_document_into_windows() {
        let document = DocumentNode {
            path: "Note.md".into(),
            title: "Note".into(),
            aliases: vec!["Alias".into()],
            tags: vec!["arch".into()],
            body: "word ".repeat(500),
            content_hash: "h".into(),
            updated_at: Utc::now(),
        };
        let chunks = chunk_document_sized("notes", &document, 80, 10);
        assert!(chunks.len() > 2);
        assert_eq!(chunks[0].ordinal, 0);
        assert!(chunks[0].text.contains("Note"));
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.parent_key == "document:notes:Note.md")
        );
    }
}
