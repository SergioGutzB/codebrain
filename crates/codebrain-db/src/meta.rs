//! Generic `meta` key/value helpers (schema v1 `meta` table).

use serde::Deserialize;

use crate::client::Database;
use crate::error::{DbError, Result};

#[derive(Debug, Deserialize)]
struct MetaRow {
    #[serde(rename = "value")]
    stored: String,
}

/// Read a string value from `meta` by key.
pub async fn read_meta(db: &Database, key: &str) -> Result<Option<String>> {
    let mut response = db
        .query("SELECT key, value FROM meta WHERE key = $key LIMIT 1;")
        .bind(("key", key.to_string()))
        .await?;
    let rows: Vec<MetaRow> = response.take(0)?;
    Ok(rows.into_iter().next().map(|row| row.stored))
}

/// Upsert a string value into `meta`.
pub async fn write_meta(db: &Database, key: &str, value: &str) -> Result<()> {
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
    .map_err(|error| DbError::Message(format!("write meta failed: {error}")))?;
    Ok(())
}

/// Delete a meta key if present.
pub async fn delete_meta(db: &Database, key: &str) -> Result<()> {
    db.query("DELETE meta WHERE key = $key;")
        .bind(("key", key.to_string()))
        .await?
        .check()
        .map_err(|error| DbError::Message(format!("delete meta failed: {error}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_schema, open_memory};

    #[tokio::test]
    async fn roundtrips_meta_values() {
        let db = open_memory().await.expect("open");
        apply_schema(&db).await.expect("schema");

        assert_eq!(
            read_meta(&db, "jira.tickets.updated_cursor").await.unwrap(),
            None
        );
        write_meta(&db, "jira.tickets.updated_cursor", "2026-08-07T12:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            read_meta(&db, "jira.tickets.updated_cursor")
                .await
                .unwrap()
                .as_deref(),
            Some("2026-08-07T12:00:00Z")
        );
        delete_meta(&db, "jira.tickets.updated_cursor")
            .await
            .unwrap();
        assert_eq!(
            read_meta(&db, "jira.tickets.updated_cursor").await.unwrap(),
            None
        );
    }
}
