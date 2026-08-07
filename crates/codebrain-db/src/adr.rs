use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::{DbError, Result};
use crate::ids::decision_id;
use crate::queries::{NodeAddress, NodeKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureDecision {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub vault_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StoredDecision {
    id: String,
    title: String,
    body: String,
    created_at: String,
    created_by: String,
    vault_path: Option<String>,
}

/// Upsert an architectural decision and return its stable id.
pub async fn upsert_architecture_decision(
    db: &Database,
    title: &str,
    body: &str,
    created_by: &str,
    vault_path: Option<&str>,
    created_at: DateTime<Utc>,
) -> Result<ArchitectureDecision> {
    let id = decision_id(title);
    let stored = StoredDecision {
        id: id.clone(),
        title: title.to_string(),
        body: body.to_string(),
        created_at: created_at.to_rfc3339(),
        created_by: created_by.to_string(),
        vault_path: vault_path.map(str::to_string),
    };

    db.query(
        "
        LET $record = type::thing('architecture_decision', $decision.id);
        UPSERT $record SET
            title = $decision.title,
            body = $decision.body,
            created_at = <datetime>$decision.created_at,
            created_by = $decision.created_by,
            vault_path = $decision.vault_path,
            embedding = NONE;
        ",
    )
    .bind(("decision", stored))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("upsert architecture decision failed: {error}")))?;

    Ok(ArchitectureDecision {
        id,
        title: title.to_string(),
        body: body.to_string(),
        created_at,
        created_by: created_by.to_string(),
        vault_path: vault_path.map(str::to_string),
    })
}

/// Create `ABOUT` edges from a decision to symbol/file/document nodes.
pub async fn relate_about(
    db: &Database,
    decision_title: &str,
    targets: &[NodeAddress],
) -> Result<usize> {
    let from_id = decision_id(decision_title);
    let mut related = 0;
    for target in targets {
        let to_table = target.kind.table();
        let to_id = target.record_id();
        let query = format!(
            "
            LET $from = type::thing('architecture_decision', $from_id);
            LET $to = type::thing('{to_table}', $to_id);
            IF record::exists($from) AND record::exists($to) {{
                DELETE about WHERE in = $from AND out = $to;
                RELATE $from->about->$to;
                RETURN true;
            }} ELSE {{
                RETURN false;
            }};
            "
        );
        let mut response = db
            .query(query)
            .bind(("from_id", from_id.clone()))
            .bind(("to_id", to_id))
            .await?;
        let ok: Option<bool> = response.take(2)?;
        if ok.unwrap_or(false) {
            related += 1;
        }
    }
    Ok(related)
}

pub async fn get_architecture_decision(
    db: &Database,
    title: &str,
) -> Result<Option<ArchitectureDecision>> {
    #[derive(Debug, Deserialize)]
    struct Row {
        title: String,
        body: String,
        created_at: DateTime<Utc>,
        created_by: String,
        vault_path: Option<String>,
    }

    let mut response = db
        .query(
            "
            SELECT title, body, created_at, created_by, vault_path
            FROM type::thing('architecture_decision', $id)
            LIMIT 1;
            ",
        )
        .bind(("id", decision_id(title)))
        .await?;
    let rows: Vec<Row> = response.take(0)?;
    Ok(rows.into_iter().next().map(|row| ArchitectureDecision {
        id: decision_id(&row.title),
        title: row.title,
        body: row.body,
        created_at: row.created_at,
        created_by: row.created_by,
        vault_path: row.vault_path,
    }))
}

pub fn decision_address(title: &str) -> NodeAddress {
    NodeAddress {
        kind: NodeKind::Decision,
        source: "system".into(),
        key: title.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_schema, open_memory, persist_code_batch};
    use chrono::Utc;
    use codebrain_connector::{ExtractBatch, FileNode, SymbolNode};

    #[tokio::test]
    async fn persists_decision_and_about_edge() {
        let db = open_memory().await.expect("db");
        apply_schema(&db).await.expect("schema");
        let batch = ExtractBatch {
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
        persist_code_batch(&db, "code", "/tmp", &batch)
            .await
            .expect("code");

        let decision = upsert_architecture_decision(
            &db,
            "Use Greeter facade",
            "All greeting copy goes through Greeter.",
            "agent",
            None,
            Utc::now(),
        )
        .await
        .expect("adr");

        let about = relate_about(
            &db,
            &decision.title,
            &[NodeAddress {
                kind: NodeKind::Symbol,
                source: "code".into(),
                key: "Services::Greeter".into(),
            }],
        )
        .await
        .expect("about");
        assert_eq!(about, 1);
        assert!(
            get_architecture_decision(&db, "Use Greeter facade")
                .await
                .expect("get")
                .is_some()
        );
    }
}
