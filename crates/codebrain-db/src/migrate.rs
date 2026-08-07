use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::client::Database;
use crate::error::{DbError, Result};

/// Current schema version written into `meta` after a successful migrate.
pub const SCHEMA_VERSION: &str = "1";

const SCHEMA_SQL: &str = include_str!("../../../schemas/v1.surql");

#[derive(Debug, Serialize, Deserialize)]
struct MetaRow {
    key: String,
    value: String,
}

/// Apply the embedded v1 schema. Safe to re-run (idempotent DEFINEs).
pub async fn apply_schema(db: &Database) -> Result<()> {
    info!("applying CodeBrain schema v{SCHEMA_VERSION}");
    // Strip USE / DEFINE NAMESPACE / DEFINE DATABASE — NS/DB already selected by the client.
    let statements: String = SCHEMA_SQL
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("USE ")
                && !trimmed.starts_with("DEFINE NAMESPACE")
                && !trimmed.starts_with("DEFINE DATABASE")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let response = db
        .query(statements)
        .await
        .map_err(|e| DbError::Migration(format!("failed to execute schema statements: {e}")))?;

    // Surface the first statement error if any (Surreal returns multi-result).
    if let Err(e) = response.check() {
        return Err(DbError::Migration(format!("schema check failed: {e}")));
    }

    upsert_schema_version(db).await?;
    debug!("schema v{SCHEMA_VERSION} applied successfully");
    Ok(())
}

async fn upsert_schema_version(db: &Database) -> Result<()> {
    db.query(
        "
        DELETE meta WHERE key = 'schema_version';
        CREATE meta SET
            key = 'schema_version',
            value = $version,
            updated_at = time::now();
        ",
    )
    .bind(("version", SCHEMA_VERSION.to_string()))
    .await?
    .check()
    .map_err(|e| DbError::Migration(format!("failed to record schema version: {e}")))?;
    Ok(())
}

/// Read the recorded schema version, if any.
pub async fn current_schema_version(db: &Database) -> Result<Option<String>> {
    let mut response = db
        .query("SELECT key, value FROM meta WHERE key = 'schema_version' LIMIT 1;")
        .await?;
    let rows: Vec<MetaRow> = response.take(0)?;
    Ok(rows.into_iter().next().map(|r| r.value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::open_memory;

    #[tokio::test]
    async fn migrate_is_idempotent() {
        let db = open_memory().await.expect("open memory db");
        apply_schema(&db).await.expect("first migrate");
        apply_schema(&db).await.expect("second migrate");
        let version = current_schema_version(&db)
            .await
            .expect("read version")
            .expect("version present");
        assert_eq!(version, SCHEMA_VERSION);
    }
}
