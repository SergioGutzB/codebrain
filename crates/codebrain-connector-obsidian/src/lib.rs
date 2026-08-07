//! Obsidian vault connector: markdown discovery, frontmatter, and wikilinks.

mod discovery;
mod error;
mod extract;
mod frontmatter;
mod wikilink;

use async_trait::async_trait;
use codebrain_connector::{Connector, ExtractBatch, IndexContext, SourceKind, WorkItem};

pub use error::{ObsidianConnectorError, Result};
pub use frontmatter::{Frontmatter, ParsedNote, split_frontmatter};
pub use wikilink::{Wikilink, extract_wikilinks};

/// Discovers Markdown notes and extracts documents + `REFERENCES` candidates.
pub struct ObsidianConnector {
    id: String,
}

impl ObsidianConnector {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl Connector for ObsidianConnector {
    fn id(&self) -> &str {
        &self.id
    }

    fn source_kind(&self) -> SourceKind {
        SourceKind::ObsidianVault
    }

    async fn discover(&self, ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>> {
        Ok(discovery::discover(ctx).await?)
    }

    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch> {
        let item = item.clone();
        Ok(tokio::task::spawn_blocking(move || extract::extract(&item)).await??)
    }
}
