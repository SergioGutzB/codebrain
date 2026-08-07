use std::collections::HashMap;

use codebrain_connector::ExtractBatch;
use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::{DbError, Result};
use crate::ids::{document_id, source_id, symbol_id};
use crate::queries::{NodeAddress, NodeKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersistedDocument {
    pub documents: usize,
}

#[derive(Debug, Deserialize)]
struct DocumentHashRow {
    path: String,
    content_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SymbolMentionTarget {
    pub source_name: String,
    pub name: String,
    pub fqn: String,
}

/// Ensure a deterministic Obsidian vault source node exists.
pub async fn upsert_obsidian_source(db: &Database, name: &str, root_path: &str) -> Result<()> {
    db.query(
        "
        UPSERT type::thing('source', $source_id) SET
            kind = 'obsidian_vault',
            name = $name,
            root_path = $root_path,
            remote_url = NONE,
            last_indexed = time::now(),
            meta = {};
        ",
    )
    .bind(("source_id", source_id(name)))
    .bind(("name", name.to_string()))
    .bind(("root_path", root_path.to_string()))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("upsert obsidian source failed: {error}")))?;
    Ok(())
}

pub async fn existing_document_hashes(
    db: &Database,
    source_name: &str,
) -> Result<HashMap<String, String>> {
    let mut response = db
        .query(
            "
            SELECT path, content_hash
            FROM document
            WHERE source = type::thing('source', $source_id);
            ",
        )
        .bind(("source_id", source_id(source_name)))
        .await?;
    let rows: Vec<DocumentHashRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .map(|row| (row.path, row.content_hash))
        .collect())
}

pub async fn delete_document(db: &Database, source_name: &str, path: &str) -> Result<()> {
    db.query(
        "
        BEGIN TRANSACTION;
        LET $document_record = type::thing('document', $document_id);
        DELETE references WHERE in = $document_record OR out = $document_record;
        DELETE mentions WHERE in = $document_record;
        DELETE explains WHERE in = $document_record;
        DELETE contains WHERE out = $document_record;
        DELETE $document_record;
        COMMIT TRANSACTION;
        ",
    )
    .bind(("document_id", document_id(source_name, path)))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("delete document failed: {error}")))?;
    Ok(())
}

/// Replace one or more documents atomically. Wikilink/mention edges are related afterwards.
pub async fn persist_document_batch(
    db: &Database,
    source_name: &str,
    root_path: &str,
    batch: &ExtractBatch,
) -> Result<PersistedDocument> {
    if batch.documents.is_empty() {
        return Ok(PersistedDocument::default());
    }

    #[derive(Debug, Serialize)]
    struct StoredDocument {
        id: String,
        path: String,
        title: String,
        aliases: Vec<String>,
        tags: Vec<String>,
        body: String,
        content_hash: String,
        updated_at: String,
    }

    let source_id = source_id(source_name);
    let documents: Vec<StoredDocument> = batch
        .documents
        .iter()
        .map(|document| StoredDocument {
            id: document_id(source_name, &document.path),
            path: document.path.clone(),
            title: document.title.clone(),
            aliases: document.aliases.clone(),
            tags: document.tags.clone(),
            body: document.body.clone(),
            content_hash: document.content_hash.clone(),
            updated_at: document.updated_at.to_rfc3339(),
        })
        .collect();
    let count = documents.len();

    db.query(
        "
        BEGIN TRANSACTION;
        LET $source_record = type::thing('source', $source_id);
        UPSERT $source_record SET
            kind = 'obsidian_vault',
            name = $source_name,
            root_path = $root_path,
            remote_url = NONE,
            last_indexed = time::now(),
            meta = {};

        FOR $doc IN $documents {
            LET $document_record = type::thing('document', $doc.id);
            DELETE references WHERE in = $document_record;
            DELETE mentions WHERE in = $document_record;
            DELETE contains WHERE in = $source_record AND out = $document_record;
            UPSERT $document_record SET
                source = $source_record,
                path = $doc.path,
                title = $doc.title,
                aliases = $doc.aliases,
                tags = $doc.tags,
                body = $doc.body,
                content_hash = $doc.content_hash,
                updated_at = <datetime>$doc.updated_at,
                embedding = NONE;
            RELATE $source_record->contains->$document_record;
        };
        COMMIT TRANSACTION;
        ",
    )
    .bind(("source_id", source_id))
    .bind(("source_name", source_name.to_string()))
    .bind(("root_path", root_path.to_string()))
    .bind(("documents", documents))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("persist document batch failed: {error}")))?;

    Ok(PersistedDocument { documents: count })
}

pub async fn relate_reference(
    db: &Database,
    source_name: &str,
    from_path: &str,
    to_path: &str,
) -> Result<bool> {
    relate_documents(
        db,
        "references",
        document_id(source_name, from_path),
        document_id(source_name, to_path),
    )
    .await
}

/// Ensure a deterministic Jira source node exists.
pub async fn upsert_jira_source(db: &Database, name: &str, remote_url: &str) -> Result<()> {
    upsert_saas_source(db, name, "jira", remote_url).await
}

/// Ensure a deterministic Confluence source node exists.
pub async fn upsert_confluence_source(db: &Database, name: &str, remote_url: &str) -> Result<()> {
    upsert_saas_source(db, name, "confluence", remote_url).await
}

/// Ensure a deterministic Notion source node exists.
pub async fn upsert_notion_source(db: &Database, name: &str, remote_url: &str) -> Result<()> {
    upsert_saas_source(db, name, "notion", remote_url).await
}

async fn upsert_saas_source(db: &Database, name: &str, kind: &str, remote_url: &str) -> Result<()> {
    db.query(
        "
        UPSERT type::thing('source', $source_id) SET
            kind = $kind,
            name = $name,
            root_path = NONE,
            remote_url = $remote_url,
            last_indexed = time::now(),
            meta = {};
        ",
    )
    .bind(("source_id", source_id(name)))
    .bind(("kind", kind.to_string()))
    .bind(("name", name.to_string()))
    .bind(("remote_url", remote_url.to_string()))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("upsert {kind} source failed: {error}")))?;
    Ok(())
}

/// Create `references` edge across document sources (e.g. Confluence → Jira).
pub async fn relate_cross_reference(
    db: &Database,
    from_source: &str,
    from_path: &str,
    to_source: &str,
    to_path: &str,
) -> Result<bool> {
    relate_documents(
        db,
        "references",
        document_id(from_source, from_path),
        document_id(to_source, to_path),
    )
    .await
}

/// Create `RESOLVES` edge: symbol → document (ticket).
pub async fn relate_resolves(
    db: &Database,
    symbol_source: &str,
    symbol_fqn: &str,
    document_source: &str,
    document_path: &str,
) -> Result<bool> {
    let mut response = db
        .query(
            "
            LET $from = type::thing('symbol', $symbol_id);
            LET $to = type::thing('document', $document_id);
            IF record::exists($from) AND record::exists($to) {
                LET $existing = SELECT id FROM resolves WHERE in = $from AND out = $to LIMIT 1;
                IF array::len($existing) = 0 {
                    RELATE $from->resolves->$to;
                    RETURN true;
                } ELSE {
                    RETURN false;
                };
            } ELSE {
                RETURN false;
            };
            ",
        )
        .bind(("symbol_id", symbol_id(symbol_source, symbol_fqn)))
        .bind(("document_id", document_id(document_source, document_path)))
        .await?;
    let related: Option<bool> = response.take(2)?;
    Ok(related.unwrap_or(false))
}

pub async fn relate_mention(
    db: &Database,
    document_source: &str,
    document_path: &str,
    symbol_source: &str,
    symbol_fqn: &str,
    confidence: f32,
    evidence: Option<&str>,
) -> Result<bool> {
    let mut response = db
        .query(
            "
            LET $from = type::thing('document', $document_id);
            LET $to = type::thing('symbol', $symbol_id);
            IF record::exists($from) AND record::exists($to) {
                DELETE mentions WHERE in = $from AND out = $to;
                RELATE $from->mentions->$to SET
                    confidence = $confidence,
                    evidence = $evidence;
                RETURN true;
            } ELSE {
                RETURN false;
            };
            ",
        )
        .bind(("document_id", document_id(document_source, document_path)))
        .bind(("symbol_id", symbol_id(symbol_source, symbol_fqn)))
        .bind(("confidence", confidence))
        .bind(("evidence", evidence.map(str::to_string)))
        .await?;
    let related: Option<bool> = response.take(2)?;
    Ok(related.unwrap_or(false))
}

/// Promote an existing `mentions` edge into a stronger `explains` edge.
///
/// Returns `None` when no mention exists between the document and symbol.
pub async fn promote_mention(
    db: &Database,
    document: &NodeAddress,
    symbol: &NodeAddress,
) -> Result<Option<PromotedExplain>> {
    if document.kind != NodeKind::Document {
        return Err(DbError::Message(format!(
            "promote_mention document must be a document token, got {:?}",
            document.kind
        )));
    }
    if symbol.kind != NodeKind::Symbol {
        return Err(DbError::Message(format!(
            "promote_mention symbol must be a symbol token, got {:?}",
            symbol.kind
        )));
    }

    #[derive(Debug, Deserialize)]
    struct MentionRow {
        id: surrealdb::RecordId,
        confidence: f32,
        evidence: Option<String>,
    }

    let mut response = db
        .query(
            "
            LET $from = type::thing('document', $document_id);
            LET $to = type::thing('symbol', $symbol_id);
            LET $mention = (
                SELECT id, confidence, evidence
                FROM mentions
                WHERE in = $from AND out = $to
                LIMIT 1
            )[0];
            IF $mention = NONE {
                RETURN NONE;
            } ELSE {
                DELETE explains WHERE in = $from AND out = $to;
                RELATE $from->explains->$to SET
                    confidence = $mention.confidence,
                    promoted_from = <string>$mention.id;
                RETURN $mention;
            };
            ",
        )
        .bind(("document_id", document.record_id()))
        .bind(("symbol_id", symbol.record_id()))
        .await?;

    let rows: Vec<Option<MentionRow>> = response.take(3)?;
    let Some(Some(mention)) = rows.into_iter().next() else {
        return Ok(None);
    };

    Ok(Some(PromotedExplain {
        document: document.clone(),
        symbol: symbol.clone(),
        confidence: mention.confidence,
        evidence: mention.evidence,
        promoted_from: mention.id.to_string(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotedExplain {
    pub document: NodeAddress,
    pub symbol: NodeAddress,
    pub confidence: f32,
    pub evidence: Option<String>,
    pub promoted_from: String,
}

pub async fn list_symbols_for_mentions(db: &Database) -> Result<Vec<SymbolMentionTarget>> {
    #[derive(Debug, Deserialize)]
    struct SourceRow {
        name: String,
    }

    #[derive(Debug, Deserialize)]
    struct SymbolOnly {
        name: String,
        fqn: String,
    }

    let mut sources_response = db.query("SELECT name FROM source;").await?;
    let sources: Vec<SourceRow> = sources_response.take(0)?;

    let mut out = Vec::new();
    for source in sources {
        let mut response = db
            .query(
                "
                SELECT name, fqn
                FROM symbol
                WHERE source = type::thing('source', $source_id);
                ",
            )
            .bind(("source_id", source_id(&source.name)))
            .await?;
        let rows: Vec<SymbolOnly> = response.take(0)?;
        out.extend(rows.into_iter().map(|row| SymbolMentionTarget {
            source_name: source.name.clone(),
            name: row.name,
            fqn: row.fqn,
        }));
    }
    Ok(out)
}

async fn relate_documents(
    db: &Database,
    edge: &str,
    from_id: String,
    to_id: String,
) -> Result<bool> {
    let query = format!(
        "
        LET $from = type::thing('document', $from_id);
        LET $to = type::thing('document', $to_id);
        IF record::exists($from) AND record::exists($to) {{
            LET $existing = SELECT id FROM {edge} WHERE in = $from AND out = $to LIMIT 1;
            IF array::len($existing) = 0 {{
                RELATE $from->{edge}->$to;
                RETURN true;
            }} ELSE {{
                RETURN false;
            }};
        }} ELSE {{
            RETURN false;
        }};
        "
    );
    let mut response = db
        .query(query)
        .bind(("from_id", from_id))
        .bind(("to_id", to_id))
        .await?;
    let related: Option<bool> = response.take(2)?;
    Ok(related.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use codebrain_connector::{DocumentNode, ExtractBatch};

    use super::*;
    use crate::{apply_schema, open_memory};

    #[tokio::test]
    async fn persists_and_deletes_document() {
        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");
        let batch = ExtractBatch {
            documents: vec![DocumentNode {
                path: "Note A.md".into(),
                title: "Note A".into(),
                aliases: vec!["Alpha".into()],
                tags: vec!["docs".into()],
                body: "Hello [[Note B]]".into(),
                content_hash: "hash-a".into(),
                updated_at: Utc::now(),
            }],
            ..ExtractBatch::default()
        };

        let stored = persist_document_batch(&db, "vault", "/tmp/vault", &batch)
            .await
            .expect("persist");
        assert_eq!(stored.documents, 1);
        assert_eq!(
            existing_document_hashes(&db, "vault")
                .await
                .expect("hashes")
                .get("Note A.md")
                .map(String::as_str),
            Some("hash-a")
        );

        delete_document(&db, "vault", "Note A.md")
            .await
            .expect("delete");
        assert!(
            existing_document_hashes(&db, "vault")
                .await
                .expect("hashes after delete")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn promotes_mention_into_explains() {
        use crate::persist_code_batch;
        use codebrain_connector::{FileNode, SymbolNode};

        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");

        let code = ExtractBatch {
            files: vec![FileNode {
                path: "a.rb".into(),
                language: Some("ruby".into()),
                content_hash: "h".into(),
                mtime: Utc::now(),
            }],
            symbols: vec![SymbolNode {
                file_path: "a.rb".into(),
                name: "Greeter".into(),
                fqn: "Services::Greeter".into(),
                kind: "class".into(),
                signature: None,
                start_line: 1,
                end_line: 2,
                content_hash: "s".into(),
            }],
            ..ExtractBatch::default()
        };
        persist_code_batch(&db, "code", "/tmp", &code)
            .await
            .expect("code");

        let notes = ExtractBatch {
            documents: vec![DocumentNode {
                path: "Design.md".into(),
                title: "Design".into(),
                aliases: Vec::new(),
                tags: Vec::new(),
                body: "Greeter".into(),
                content_hash: "d".into(),
                updated_at: Utc::now(),
            }],
            ..ExtractBatch::default()
        };
        persist_document_batch(&db, "notes", "/tmp/notes", &notes)
            .await
            .expect("notes");
        assert!(
            relate_mention(
                &db,
                "notes",
                "Design.md",
                "code",
                "Services::Greeter",
                0.88,
                Some("Greeter"),
            )
            .await
            .expect("mention")
        );

        let promoted = promote_mention(
            &db,
            &NodeAddress {
                kind: NodeKind::Document,
                source: "notes".into(),
                key: "Design.md".into(),
            },
            &NodeAddress {
                kind: NodeKind::Symbol,
                source: "code".into(),
                key: "Services::Greeter".into(),
            },
        )
        .await
        .expect("promote")
        .expect("edge");
        assert!((promoted.confidence - 0.88).abs() < f32::EPSILON);
        assert!(promoted.promoted_from.contains("mentions"));
    }

    #[tokio::test]
    async fn relates_symbol_to_jira_ticket() {
        use crate::persist_code_batch;
        use codebrain_connector::{FileNode, SymbolNode};

        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");

        let code = ExtractBatch {
            files: vec![FileNode {
                path: "a.rb".into(),
                language: Some("ruby".into()),
                content_hash: "h".into(),
                mtime: Utc::now(),
            }],
            symbols: vec![SymbolNode {
                file_path: "a.rb".into(),
                name: "Plan".into(),
                fqn: "Services::Plan".into(),
                kind: "class".into(),
                signature: None,
                start_line: 1,
                end_line: 2,
                content_hash: "s".into(),
            }],
            ..ExtractBatch::default()
        };
        persist_code_batch(&db, "code", "/tmp", &code)
            .await
            .expect("code");

        let tickets = ExtractBatch {
            documents: vec![DocumentNode {
                path: "PPS-479".into(),
                title: "Remove flag".into(),
                aliases: Vec::new(),
                tags: Vec::new(),
                body: "ticket body".into(),
                content_hash: "t".into(),
                updated_at: Utc::now(),
            }],
            ..ExtractBatch::default()
        };
        persist_document_batch(&db, "tickets", "https://example.atlassian.net", &tickets)
            .await
            .expect("tickets");

        assert!(
            relate_resolves(&db, "code", "Services::Plan", "tickets", "PPS-479")
                .await
                .expect("resolves")
        );
        assert!(
            !relate_resolves(&db, "code", "Services::Plan", "tickets", "PPS-479")
                .await
                .expect("resolves idempotent"),
            "second relate should not count as new"
        );
    }
}
