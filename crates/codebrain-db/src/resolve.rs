use serde::Deserialize;

use crate::client::Database;
use crate::error::Result;
use crate::ids::source_id;

#[derive(Debug, Clone, Deserialize)]
pub struct DocumentLookup {
    pub path: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub body: String,
}

#[derive(Debug, Deserialize)]
struct DocumentRow {
    path: String,
    title: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    body: String,
}

pub async fn list_documents_for_resolution(
    db: &Database,
    source_name: &str,
) -> Result<Vec<DocumentLookup>> {
    let mut response = db
        .query(
            "
            SELECT path, title, aliases, body
            FROM document
            WHERE source = type::thing('source', $source_id);
            ",
        )
        .bind(("source_id", source_id(source_name)))
        .await?;
    let rows: Vec<DocumentRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .map(|row| DocumentLookup {
            path: row.path,
            title: row.title,
            aliases: row.aliases,
            body: row.body,
        })
        .collect())
}

/// Resolve Obsidian wikilink targets: exact path → title → alias.
pub fn resolve_wikilink(target: &str, documents: &[DocumentLookup]) -> Option<String> {
    let normalized = normalize_wikilink_target(target);
    if normalized.is_empty() {
        return None;
    }

    let by_path = documents.iter().find_map(|document| {
        let path = normalize_path(&document.path);
        let stem = path
            .trim_end_matches(".md")
            .trim_end_matches(".MD")
            .to_string();
        if path == normalized
            || stem == normalized
            || path.ends_with(&format!("/{normalized}"))
            || stem.ends_with(&format!("/{normalized}"))
            || path.ends_with(&format!("/{normalized}.md"))
        {
            Some(document.path.clone())
        } else {
            None
        }
    });
    if by_path.is_some() {
        return by_path;
    }

    let target_lower = normalized.to_ascii_lowercase();
    if let Some(document) = documents
        .iter()
        .find(|document| document.title.eq_ignore_ascii_case(&target_lower))
    {
        return Some(document.path.clone());
    }

    documents
        .iter()
        .find(|document| {
            document
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&target_lower))
        })
        .map(|document| document.path.clone())
}

fn normalize_wikilink_target(target: &str) -> String {
    normalize_path(
        target
            .trim()
            .trim_end_matches(".md")
            .trim_end_matches(".MD"),
    )
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_by_path_title_and_alias() {
        let docs = vec![
            DocumentLookup {
                path: "folder/Note B.md".into(),
                title: "Note B".into(),
                aliases: vec!["Bravo".into()],
                body: String::new(),
            },
            DocumentLookup {
                path: "Alpha.md".into(),
                title: "Something Else".into(),
                aliases: vec!["Note A".into()],
                body: String::new(),
            },
        ];

        assert_eq!(
            resolve_wikilink("folder/Note B", &docs).as_deref(),
            Some("folder/Note B.md")
        );
        assert_eq!(
            resolve_wikilink("Note B", &docs).as_deref(),
            Some("folder/Note B.md")
        );
        assert_eq!(
            resolve_wikilink("Bravo", &docs).as_deref(),
            Some("folder/Note B.md")
        );
        assert_eq!(
            resolve_wikilink("Note A", &docs).as_deref(),
            Some("Alpha.md")
        );
        assert_eq!(resolve_wikilink("Missing", &docs), None);
    }
}
