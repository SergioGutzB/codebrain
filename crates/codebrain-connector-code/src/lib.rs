//! High-performance, incremental source-code connector.

mod discovery;
mod error;
mod language;

use std::collections::HashSet;

use async_trait::async_trait;
use codebrain_connector::{Connector, ExtractBatch, IndexContext, SourceKind, WorkItem};

pub use error::{CodeConnectorError, Result};
pub use language::{Language, ParsedCode};

/// Discovers supported source files and extracts graph material with tree-sitter.
pub struct CodeConnector {
    id: String,
    languages: HashSet<Language>,
}

impl CodeConnector {
    /// Create a connector. An empty language list enables every supported language.
    pub fn new(id: impl Into<String>, languages: impl IntoIterator<Item = Language>) -> Self {
        let languages = languages.into_iter().collect();
        Self {
            id: id.into(),
            languages,
        }
    }

    pub fn all_languages(id: impl Into<String>) -> Self {
        Self::new(id, Language::ALL)
    }
}

#[async_trait]
impl Connector for CodeConnector {
    fn id(&self) -> &str {
        &self.id
    }

    fn source_kind(&self) -> SourceKind {
        SourceKind::GitRepo
    }

    async fn discover(&self, ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>> {
        Ok(discovery::discover(ctx, &self.languages).await?)
    }

    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch> {
        let item = item.clone();
        Ok(tokio::task::spawn_blocking(move || language::extract(&item)).await??)
    }
}
