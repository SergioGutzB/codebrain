use std::path::Path;

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem, SurrealKv};
use tracing::info;

use crate::error::Result;
use crate::migrate;

/// Embedded CodeBrain database handle.
pub type Database = Surreal<Db>;

const NS: &str = "codebrain";
const DB_NAME: &str = "codebrain";

/// Open a persistent SurrealKV database at `path` and select the CodeBrain NS/DB.
pub async fn open_embedded(path: impl AsRef<Path>) -> Result<Database> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    info!(path = %path.display(), "opening embedded SurrealKV database");
    let db = Surreal::new::<SurrealKv>(path).await?;
    db.use_ns(NS).use_db(DB_NAME).await?;
    Ok(db)
}

/// Open an in-memory database (tests / ephemeral sessions).
pub async fn open_memory() -> Result<Database> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns(NS).use_db(DB_NAME).await?;
    Ok(db)
}

/// Open (or create) the database and apply the latest schema.
pub async fn open_and_migrate(path: impl AsRef<Path>) -> Result<Database> {
    let db = open_embedded(path).await?;
    migrate::apply_schema(&db).await?;
    Ok(db)
}
