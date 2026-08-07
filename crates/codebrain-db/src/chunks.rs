use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::{DbError, Result};
use crate::ids::stable_id;

#[derive(Debug, Clone, Serialize)]
pub struct StoredChunk {
    pub id: String,
    pub parent: String,
    pub ordinal: i64,
    pub text: String,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkHit {
    pub parent: String,
    pub ordinal: i64,
    pub text: String,
    pub distance: Option<f32>,
    pub score: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingMetaRow {
    #[serde(rename = "value")]
    stored: String,
}

/// Persist embedding provider metadata so `doctor` can detect dimension drift.
pub async fn record_embedding_meta(
    db: &Database,
    provider: &str,
    model: &str,
    dimension: u32,
) -> Result<()> {
    for (key, value) in [
        ("embedding.provider", provider),
        ("embedding.model", model),
        ("embedding.dimension", &dimension.to_string()),
    ] {
        db.query(
            "
            DELETE meta WHERE key = $key;
            CREATE meta SET
                key = $key,
                value = $value,
                updated_at = time::now();
            ",
        )
        .bind(("key", key.to_string()))
        .bind(("value", value.to_string()))
        .await?
        .check()
        .map_err(|error| DbError::Message(format!("record embedding meta failed: {error}")))?;
    }
    Ok(())
}

pub async fn read_embedding_dimension(db: &Database) -> Result<Option<u32>> {
    // `value` is reserved alone in SurrealQL projections; select with the key column too.
    let mut response = db
        .query("SELECT key, value FROM meta WHERE key = 'embedding.dimension' LIMIT 1;")
        .await?;
    let rows: Vec<EmbeddingMetaRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|row| row.stored.parse().ok()))
}

/// Create or replace the HNSW index so its DIMENSION matches the active model.
pub async fn ensure_chunk_vector_index(db: &Database, dimension: u32) -> Result<()> {
    if dimension == 0 {
        return Err(DbError::Message("embedding dimension must be > 0".into()));
    }
    let sql = format!(
        "DEFINE INDEX OVERWRITE chunk_vec ON chunk FIELDS embedding \
         HNSW DIMENSION {dimension} DIST COSINE TYPE F32;"
    );
    db.query(sql)
        .await?
        .check()
        .map_err(|error| DbError::Message(format!("ensure chunk vector index failed: {error}")))?;
    Ok(())
}

/// Replace every chunk belonging to a parent.
///
/// Uses delete + individual creates instead of one giant Surreal transaction: binding
/// dozens of 384-d float vectors inside `BEGIN…COMMIT` regularly aborts on SurrealKV.
pub async fn replace_chunks(db: &Database, parent: &str, chunks: &[StoredChunk]) -> Result<usize> {
    delete_chunks_for_parent(db, parent).await?;
    for chunk in chunks {
        db.query(
            "
            CREATE type::thing('chunk', $id) SET
                parent = $parent,
                ordinal = $ordinal,
                text = $text,
                embedding = $embedding;
            ",
        )
        .bind(("id", chunk.id.clone()))
        .bind(("parent", chunk.parent.clone()))
        .bind(("ordinal", chunk.ordinal))
        .bind(("text", chunk.text.clone()))
        .bind(("embedding", chunk.embedding.clone()))
        .await?
        .check()
        .map_err(|error| DbError::Message(format!("create chunk failed: {error}")))?;
    }
    Ok(chunks.len())
}

pub async fn delete_chunks_for_parent(db: &Database, parent: &str) -> Result<()> {
    db.query("DELETE chunk WHERE parent = $parent;")
        .bind(("parent", parent.to_string()))
        .await?
        .check()
        .map_err(|error| DbError::Message(format!("delete chunks failed: {error}")))?;
    Ok(())
}

/// Approximate nearest-neighbour search over chunk embeddings.
pub async fn knn_chunks(db: &Database, vector: &[f32], limit: usize) -> Result<Vec<ChunkHit>> {
    // SurrealQL requires a literal integer inside `<|k, ef|>`; bind parameters are rejected.
    let k = limit.max(1);
    let sql = format!(
        "
        SELECT parent, ordinal, text, vector::distance::knn() AS distance
        FROM chunk
        WHERE embedding <|{k}, 100|> $vector
        ORDER BY distance
        LIMIT {k};
        "
    );
    let mut response = db.query(sql).bind(("vector", vector.to_vec())).await?;
    let rows: Vec<ChunkHit> = response.take(0)?;
    Ok(rows)
}

/// Full-text fallback over chunk text when embeddings are disabled.
pub async fn fts_chunks(db: &Database, query: &str, limit: usize) -> Result<Vec<ChunkHit>> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let mut response = db
        .query(
            "
            SELECT parent, ordinal, text
            FROM chunk
            WHERE string::lowercase(text) CONTAINS $needle
            LIMIT $limit;
            ",
        )
        .bind(("needle", needle))
        .bind(("limit", limit as i64))
        .await?;
    let rows: Vec<ChunkHit> = response.take(0)?;
    Ok(rows)
}

pub fn chunk_record_id(parent: &str, ordinal: i64) -> String {
    stable_id(&format!("chunk:{parent}:{ordinal}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_schema, open_memory};

    #[tokio::test]
    async fn replaces_chunks_and_runs_knn() {
        let db = open_memory().await.expect("open");
        apply_schema(&db).await.expect("schema");
        ensure_chunk_vector_index(&db, 4).await.expect("index");

        let parent = "symbol:code:Greeter";
        let chunks = [
            StoredChunk {
                id: chunk_record_id(parent, 0),
                parent: parent.into(),
                ordinal: 0,
                text: "greeter class".into(),
                embedding: Some(vec![1.0, 0.0, 0.0, 0.0]),
            },
            StoredChunk {
                id: chunk_record_id("symbol:code:Other", 0),
                parent: "symbol:code:Other".into(),
                ordinal: 0,
                text: "other".into(),
                embedding: Some(vec![0.0, 1.0, 0.0, 0.0]),
            },
        ];
        // Insert both parents separately for clarity.
        replace_chunks(&db, parent, &chunks[..1])
            .await
            .expect("replace a");
        replace_chunks(&db, "symbol:code:Other", &chunks[1..])
            .await
            .expect("replace b");

        let hits = knn_chunks(&db, &[0.9, 0.1, 0.0, 0.0], 2)
            .await
            .expect("knn");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].parent, parent);
    }
}
