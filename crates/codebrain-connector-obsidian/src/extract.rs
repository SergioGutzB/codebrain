use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use codebrain_connector::{DocumentNode, EdgeCandidate, EdgeType, ExtractBatch, WorkItem};

use crate::error::{ObsidianConnectorError, Result};
use crate::frontmatter::{collect_tags, default_title, split_frontmatter};
use crate::wikilink::extract_wikilinks;

pub fn extract(item: &WorkItem) -> Result<ExtractBatch> {
    let path = PathBuf::from(&item.path);
    let raw = std::fs::read_to_string(&path).map_err(|source| ObsidianConnectorError::Io {
        path: path.clone(),
        source,
    })?;
    let content_hash = item
        .content_hash
        .clone()
        .unwrap_or_else(|| blake3::hash(raw.as_bytes()).to_hex().to_string());
    let updated_at = item
        .mtime
        .unwrap_or_else(|| DateTime::<Utc>::from(SystemTime::UNIX_EPOCH));

    Ok(extract_from_source(
        &item.id,
        &raw,
        content_hash,
        updated_at,
    ))
}

pub fn extract_from_source(
    relative_path: &str,
    raw: &str,
    content_hash: String,
    updated_at: DateTime<Utc>,
) -> ExtractBatch {
    let parsed = split_frontmatter(raw);
    let title = default_title(relative_path, &parsed.frontmatter, &parsed.body);
    let tags = collect_tags(&parsed.frontmatter, &parsed.body);
    let document = DocumentNode {
        path: relative_path.replace('\\', "/"),
        title,
        aliases: parsed.frontmatter.aliases,
        tags,
        body: parsed.body.clone(),
        content_hash,
        updated_at,
    };

    let edges = extract_wikilinks(&parsed.body)
        .into_iter()
        .filter(|link| !is_attachment(&link.target))
        .map(|link| EdgeCandidate {
            edge_type: EdgeType::References,
            from_key: format!("document:{}", document.path),
            to_key: format!("wikilink:{}", link.target),
            confidence: Some(1.0),
            evidence: Some(link.raw),
        })
        .collect();

    ExtractBatch {
        documents: vec![document],
        edges,
        ..ExtractBatch::default()
    }
}

/// Obsidian embeds attachments with the same `[[...]]` syntax as note links.
/// Those are binary assets, not missing notes, so they must not become `REFERENCES`.
fn is_attachment(target: &str) -> bool {
    const ATTACHMENT_EXTENSIONS: [&str; 16] = [
        "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico", "pdf", "mp3", "wav", "m4a",
        "mp4", "mov", "webm", "canvas",
    ];

    Path::new(target)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| ATTACHMENT_EXTENSIONS.contains(&extension.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_attachment_embeds_but_keeps_note_links() {
        let raw = "![[Pasted image 20260521181514.png]] and ![[diagram.pdf]] and [[Real Note]].";
        let batch = extract_from_source("Note.md", raw, "hash".into(), Utc::now());
        assert_eq!(batch.edges.len(), 1);
        assert_eq!(batch.edges[0].to_key, "wikilink:Real Note");
    }

    #[test]
    fn extracts_document_and_wikilink_edges() {
        let raw =
            "---\ntitle: Note A\naliases: [Alpha]\n---\n\nSee [[Note B]] and the Greeter class.\n";
        let batch = extract_from_source("Note A.md", raw, "hash".into(), Utc::now());
        assert_eq!(batch.documents.len(), 1);
        assert_eq!(batch.documents[0].title, "Note A");
        assert_eq!(batch.documents[0].aliases, vec!["Alpha"]);
        assert!(
            batch
                .edges
                .iter()
                .any(|edge| edge.to_key == "wikilink:Note B")
        );
    }
}
