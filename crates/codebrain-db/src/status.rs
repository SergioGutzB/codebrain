use serde::{Deserialize, Serialize};

use crate::client::Database;
use crate::error::Result;
use crate::migrate::{SCHEMA_VERSION, current_schema_version};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableCount {
    pub table: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub schema_version: Option<String>,
    pub expected_schema_version: String,
    pub schema_ok: bool,
    pub tables: Vec<TableCount>,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

const TRACKED_TABLES: &[&str] = &[
    "source",
    "file",
    "symbol",
    "document",
    "chunk",
    "architecture_decision",
    "mentions",
    "explains",
    "resolves",
    "meta",
];

/// Collect high-level database health for `codebrain status` / MCP resource.
pub async fn collect_status(db: &Database) -> Result<DatabaseStatus> {
    let schema_version = current_schema_version(db).await?;
    let schema_ok = schema_version.as_deref() == Some(SCHEMA_VERSION);

    let mut tables = Vec::with_capacity(TRACKED_TABLES.len());
    for table in TRACKED_TABLES {
        let count = count_table(db, table).await.unwrap_or(0);
        tables.push(TableCount {
            table: (*table).to_string(),
            count,
        });
    }

    Ok(DatabaseStatus {
        schema_version,
        expected_schema_version: SCHEMA_VERSION.to_string(),
        schema_ok,
        tables,
    })
}

async fn count_table(db: &Database, table: &str) -> Result<i64> {
    // Table names are from a fixed allowlist — not user input.
    let sql = format!("SELECT count() AS count FROM {table} GROUP ALL;");
    let mut response = db.query(sql).await?;
    let rows: Vec<CountRow> = response.take(0)?;
    Ok(rows.first().map(|r| r.count).unwrap_or(0))
}
