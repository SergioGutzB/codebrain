//! Connector trait and shared ingest types for CodeBrain.

mod types;

pub use types::*;

use async_trait::async_trait;

/// Pluggable ingestion source. Implementations live in connector-* crates.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Stable connector instance id (usually matches config source name).
    fn id(&self) -> &str;

    /// Kind of origin this connector reads from.
    fn source_kind(&self) -> SourceKind;

    /// Discover work items that need (re)indexing.
    async fn discover(&self, ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>>;

    /// Extract graph nodes/edges from a single work item.
    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch>;
}
