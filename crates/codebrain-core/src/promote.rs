//! Controlled promotion of `mentions` → `explains`.

use anyhow::{Context, bail};
use codebrain_db::{Database, NodeAddress, NodeKind, PromotedExplain, promote_mention};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct PromoteMentionRequest {
    pub document: NodeAddress,
    pub symbol: NodeAddress,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromoteMentionResult {
    pub document: String,
    pub symbol: String,
    pub confidence: f32,
    pub evidence: Option<String>,
    pub promoted_from: String,
    pub relation: &'static str,
}

/// Promote a mention edge into an `explains` edge (agent-approved link).
pub async fn promote_mention_edge(
    db: &Database,
    request: PromoteMentionRequest,
) -> anyhow::Result<PromoteMentionResult> {
    if request.document.kind != NodeKind::Document {
        bail!(
            "document must be a document token (document:source:path), got {}",
            request.document.to_token()
        );
    }
    if request.symbol.kind != NodeKind::Symbol {
        bail!(
            "symbol must be a symbol token (symbol:source:fqn), got {}",
            request.symbol.to_token()
        );
    }

    let promoted = promote_mention(db, &request.document, &request.symbol)
        .await
        .context("promote mention")?
        .with_context(|| {
            format!(
                "no mentions edge between {} and {}",
                request.document.to_token(),
                request.symbol.to_token()
            )
        })?;

    Ok(to_result(promoted))
}

fn to_result(promoted: PromotedExplain) -> PromoteMentionResult {
    PromoteMentionResult {
        document: promoted.document.to_token(),
        symbol: promoted.symbol.to_token(),
        confidence: promoted.confidence,
        evidence: promoted.evidence,
        promoted_from: promoted.promoted_from,
        relation: "explains",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use codebrain_connector::{DocumentNode, ExtractBatch, FileNode, SymbolNode};
    use codebrain_db::{
        apply_schema, open_memory, persist_code_batch, persist_document_batch, relate_mention,
    };

    #[tokio::test]
    async fn promotes_existing_mention_to_explains() {
        let db = open_memory().await.expect("db");
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
                body: "Greeter owns copy.".into(),
                content_hash: "d".into(),
                updated_at: Utc::now(),
            }],
            ..ExtractBatch::default()
        };
        persist_document_batch(&db, "notes", "/tmp/notes", &notes)
            .await
            .expect("notes");
        relate_mention(
            &db,
            "notes",
            "Design.md",
            "code",
            "Services::Greeter",
            0.91,
            Some("Greeter"),
        )
        .await
        .expect("mention");

        let result = promote_mention_edge(
            &db,
            PromoteMentionRequest {
                document: NodeAddress {
                    kind: NodeKind::Document,
                    source: "notes".into(),
                    key: "Design.md".into(),
                },
                symbol: NodeAddress {
                    kind: NodeKind::Symbol,
                    source: "code".into(),
                    key: "Services::Greeter".into(),
                },
            },
        )
        .await
        .expect("promote");

        assert_eq!(result.relation, "explains");
        assert!((result.confidence - 0.91).abs() < f32::EPSILON);
        assert!(result.promoted_from.contains("mentions"));
    }
}
