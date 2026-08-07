use std::collections::HashMap;

use codebrain_connector::ExtractBatch;
use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::{DbError, Result};
use crate::ids::{file_id, source_id, symbol_id};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersistedBatch {
    pub files: usize,
    pub symbols: usize,
}

#[derive(Debug, Serialize)]
struct StoredSymbol {
    id: String,
    name: String,
    fqn: String,
    kind: String,
    signature: Option<String>,
    start_line: i64,
    end_line: i64,
    content_hash: String,
}

#[derive(Debug, Deserialize)]
struct StringValue {
    value: String,
}

#[derive(Debug, Deserialize)]
struct FileHashRow {
    path: String,
    content_hash: String,
}

/// Ensure a deterministic source node exists even when the repository is empty.
pub async fn upsert_code_source(db: &Database, name: &str, root_path: &str) -> Result<()> {
    db.query(
        "
        UPSERT type::thing('source', $source_id) SET
            kind = 'git_repo',
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
    .map_err(|error| DbError::Message(format!("upsert source failed: {error}")))?;
    Ok(())
}

/// Return the stored content hash for a deterministic source/file pair.
pub async fn file_content_hash(
    db: &Database,
    source_name: &str,
    path: &str,
) -> Result<Option<String>> {
    let file_id = file_id(source_name, path);
    let mut response = db
        .query(
            "
            SELECT content_hash AS value
            FROM file
            WHERE id = type::thing('file', $file_id)
            LIMIT 1;
            ",
        )
        .bind(("file_id", file_id))
        .await?;
    let values: Vec<StringValue> = response.take(0)?;
    Ok(values.into_iter().next().map(|row| row.value))
}

/// Fetch all content hashes in one query to avoid an N+1 discovery pass.
pub async fn existing_file_hashes(
    db: &Database,
    source_name: &str,
) -> Result<HashMap<String, String>> {
    let mut response = db
        .query(
            "
            SELECT path, content_hash
            FROM file
            WHERE source = type::thing('source', $source_id);
            ",
        )
        .bind(("source_id", source_id(source_name)))
        .await?;
    let rows: Vec<FileHashRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .map(|row| (row.path, row.content_hash))
        .collect())
}

/// Delete a removed file and every relation/symbol owned by it.
pub async fn delete_code_file(db: &Database, source_name: &str, path: &str) -> Result<()> {
    db.query(
        "
        BEGIN TRANSACTION;
        LET $file_record = type::thing('file', $file_id);
        LET $symbols = SELECT VALUE id FROM symbol WHERE file = $file_record;
        DELETE calls WHERE in IN $symbols OR out IN $symbols;
        DELETE imports WHERE in = $file_record OR out = $file_record;
        DELETE defines WHERE in = $file_record;
        DELETE contains WHERE out = $file_record;
        DELETE symbol WHERE file = $file_record;
        DELETE $file_record;
        COMMIT TRANSACTION;
        ",
    )
    .bind(("file_id", file_id(source_name, path)))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("delete code file failed: {error}")))?;
    Ok(())
}

/// Replace one or more files and their symbols atomically.
/// Edges are related after all changed files exist.
pub async fn persist_code_batch(
    db: &Database,
    source_name: &str,
    root_path: &str,
    batch: &ExtractBatch,
) -> Result<PersistedBatch> {
    if batch.files.is_empty() {
        return Ok(PersistedBatch::default());
    }

    #[derive(Debug, Serialize)]
    struct StoredFile {
        id: String,
        path: String,
        language: Option<String>,
        content_hash: String,
        mtime: String,
        symbols: Vec<StoredSymbol>,
    }

    let source_id = source_id(source_name);
    let mut files = Vec::with_capacity(batch.files.len());
    for file in &batch.files {
        let symbols: Vec<StoredSymbol> = batch
            .symbols
            .iter()
            .filter(|symbol| symbol.file_path == file.path)
            .map(|symbol| StoredSymbol {
                id: symbol_id(source_name, &symbol.fqn),
                name: symbol.name.clone(),
                fqn: symbol.fqn.clone(),
                kind: symbol.kind.clone(),
                signature: symbol.signature.clone(),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                content_hash: symbol.content_hash.clone(),
            })
            .collect();
        files.push(StoredFile {
            id: file_id(source_name, &file.path),
            path: file.path.clone(),
            language: file.language.clone(),
            content_hash: file.content_hash.clone(),
            mtime: file.mtime.to_rfc3339(),
            symbols,
        });
    }
    let file_count = files.len();
    let symbol_count = files.iter().map(|file| file.symbols.len()).sum();

    db.query(
        "
        BEGIN TRANSACTION;
        LET $source_record = type::thing('source', $source_id);
        UPSERT $source_record SET
            kind = 'git_repo',
            name = $source_name,
            root_path = $root_path,
            remote_url = NONE,
            last_indexed = time::now(),
            meta = {};

        FOR $file IN $files {
            LET $file_record = type::thing('file', $file.id);
            LET $old_symbols = SELECT VALUE id FROM symbol WHERE file = $file_record;
            DELETE calls WHERE in IN $old_symbols OR out IN $old_symbols;
            DELETE imports WHERE in = $file_record OR out = $file_record;
            DELETE defines WHERE in = $file_record;
            DELETE symbol WHERE file = $file_record;
            DELETE contains WHERE in = $source_record AND out = $file_record;

            UPSERT $file_record SET
                source = $source_record,
                path = $file.path,
                language = $file.language,
                content_hash = $file.content_hash,
                mtime = <datetime>$file.mtime,
                embedding = NONE;
            RELATE $source_record->contains->$file_record;

            FOR $symbol IN $file.symbols {
                LET $symbol_record = type::thing('symbol', $symbol.id);
                UPSERT $symbol_record SET
                    source = $source_record,
                    file = $file_record,
                    name = $symbol.name,
                    fqn = $symbol.fqn,
                    kind = $symbol.kind,
                    signature = $symbol.signature,
                    start_line = $symbol.start_line,
                    end_line = $symbol.end_line,
                    content_hash = $symbol.content_hash,
                    embedding = NONE;
                RELATE $file_record->defines->$symbol_record;
            };
        };
        COMMIT TRANSACTION;
        ",
    )
    .bind(("source_id", source_id))
    .bind(("source_name", source_name.to_string()))
    .bind(("root_path", root_path.to_string()))
    .bind(("files", files))
    .await?
    .check()
    .map_err(|error| DbError::Message(format!("persist code batch failed: {error}")))?;

    Ok(PersistedBatch {
        files: file_count,
        symbols: symbol_count,
    })
}

pub async fn relate_import(
    db: &Database,
    source_name: &str,
    from_file: &str,
    to_file: &str,
) -> Result<bool> {
    relate(
        db,
        "imports",
        "file",
        file_id(source_name, from_file),
        "file",
        file_id(source_name, to_file),
    )
    .await
}

pub async fn relate_call(
    db: &Database,
    source_name: &str,
    from_fqn: &str,
    to_fqn: &str,
) -> Result<bool> {
    relate(
        db,
        "calls",
        "symbol",
        symbol_id(source_name, from_fqn),
        "symbol",
        symbol_id(source_name, to_fqn),
    )
    .await
}

async fn relate(
    db: &Database,
    edge: &str,
    from_table: &str,
    from_id: String,
    to_table: &str,
    to_id: String,
) -> Result<bool> {
    let query = format!(
        "
        LET $from = type::thing('{from_table}', $from_id);
        LET $to = type::thing('{to_table}', $to_id);
        IF record::exists($from) AND record::exists($to) {{
            DELETE {edge} WHERE in = $from AND out = $to;
            RELATE $from->{edge}->$to;
            RETURN true;
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

pub async fn list_symbol_fqns_for_file(
    db: &Database,
    source_name: &str,
    path: &str,
) -> Result<Vec<String>> {
    #[derive(Debug, Deserialize)]
    struct FqnRow {
        fqn: String,
    }

    let mut response = db
        .query(
            "
            SELECT fqn FROM symbol
            WHERE source = type::thing('source', $source_id)
              AND file = type::thing('file', $file_id);
            ",
        )
        .bind(("source_id", source_id(source_name)))
        .bind(("file_id", file_id(source_name, path)))
        .await?;
    let rows: Vec<FqnRow> = response.take(0)?;
    Ok(rows.into_iter().map(|row| row.fqn).collect())
}

/// Resolve a symbol name to an FQN, preferring a symbol from the same file.
pub async fn find_symbol_fqn(
    db: &Database,
    source_name: &str,
    name: &str,
    preferred_file: Option<&str>,
) -> Result<Option<String>> {
    let source_id = source_id(source_name);
    if let Some(path) = preferred_file {
        let mut response = db
            .query(
                "
                SELECT fqn AS value
                FROM symbol
                WHERE source = type::thing('source', $source_id)
                  AND file = type::thing('file', $file_id)
                  AND name = $name
                LIMIT 1;
                ",
            )
            .bind(("source_id", source_id.clone()))
            .bind(("file_id", file_id(source_name, path)))
            .bind(("name", name.to_string()))
            .await?;
        let values: Vec<StringValue> = response.take(0)?;
        if let Some(value) = values.into_iter().next() {
            return Ok(Some(value.value));
        }
    }

    let mut response = db
        .query(
            "
            SELECT fqn AS value
            FROM symbol
            WHERE source = type::thing('source', $source_id)
              AND name = $name
            LIMIT 1;
            ",
        )
        .bind(("source_id", source_id))
        .bind(("name", name.to_string()))
        .await?;
    let values: Vec<StringValue> = response.take(0)?;
    Ok(values.into_iter().next().map(|row| row.value))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use codebrain_connector::{ExtractBatch, FileNode, SymbolNode};

    use super::*;
    use crate::{apply_schema, open_memory};

    #[tokio::test]
    async fn persists_and_skips_by_content_hash() {
        let db = open_memory().await.expect("open db");
        apply_schema(&db).await.expect("schema");
        let batch = ExtractBatch {
            files: vec![FileNode {
                path: "src/lib.rs".into(),
                language: Some("rust".into()),
                content_hash: "abc".into(),
                mtime: Utc::now(),
            }],
            symbols: vec![SymbolNode {
                file_path: "src/lib.rs".into(),
                name: "run".into(),
                fqn: "src::lib::run".into(),
                kind: "function".into(),
                signature: Some("pub fn run()".into()),
                start_line: 1,
                end_line: 1,
                content_hash: "def".into(),
            }],
            ..ExtractBatch::default()
        };

        let stored = persist_code_batch(&db, "fixture", "/tmp/fixture", &batch)
            .await
            .expect("persist");

        assert_eq!(stored.symbols, 1);
        assert_eq!(
            file_content_hash(&db, "fixture", "src/lib.rs")
                .await
                .expect("hash")
                .as_deref(),
            Some("abc")
        );

        delete_code_file(&db, "fixture", "src/lib.rs")
            .await
            .expect("delete file");
        assert_eq!(
            file_content_hash(&db, "fixture", "src/lib.rs")
                .await
                .expect("hash after delete"),
            None
        );
    }
}
