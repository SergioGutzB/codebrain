//! Confluence → CodeBrain documents.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use codebrain_connector::{
    Connector, DocumentNode, ExtractBatch, IndexContext, SourceKind, WorkItem,
};

use crate::client::JiraAuth;
use crate::confluence_client::{
    ConfluenceClient, ConfluencePage, page_content_hash, render_page_body,
};
use crate::error::{Result, SaasError};

#[derive(Clone)]
pub struct ConfluenceConnector {
    id: String,
    client: Arc<ConfluenceClient>,
    cql: String,
    max_pages: usize,
    cache: Arc<tokio::sync::RwLock<Vec<ConfluencePage>>>,
}

impl ConfluenceConnector {
    pub fn new(
        id: impl Into<String>,
        auth: JiraAuth,
        cql: impl Into<String>,
        max_pages: usize,
    ) -> Result<Self> {
        Ok(Self {
            id: id.into(),
            client: Arc::new(ConfluenceClient::new(auth)?),
            cql: cql.into(),
            max_pages: max_pages.max(1),
            cache: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        })
    }

    async fn ensure_cache(&self) -> Result<()> {
        let cached = self.cache.read().await;
        if !cached.is_empty() {
            return Ok(());
        }
        drop(cached);
        let pages = self.client.search(&self.cql, self.max_pages).await?;
        *self.cache.write().await = pages;
        Ok(())
    }
}

#[async_trait]
impl Connector for ConfluenceConnector {
    fn id(&self) -> &str {
        &self.id
    }

    fn source_kind(&self) -> SourceKind {
        SourceKind::Confluence
    }

    async fn discover(&self, _ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>> {
        self.ensure_cache().await?;
        let pages = self.cache.read().await;
        Ok(pages
            .iter()
            .map(|page| WorkItem {
                id: page.id.clone(),
                path: page.id.clone(),
                content_hash: Some(page_content_hash(page)),
                mtime: parse_time(&page.updated),
            })
            .collect())
    }

    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch> {
        self.ensure_cache().await?;
        let pages = self.cache.read().await;
        let Some(page) = pages.iter().find(|page| page.id == item.id) else {
            return Err(
                SaasError::Message(format!("confluence page not in cache: {}", item.id)).into(),
            );
        };

        let mut aliases = vec![page.title.clone(), page.id.clone()];
        if !page.space_key.is_empty() {
            aliases.push(format!("{}/{}", page.space_key, page.title));
        }
        aliases.retain(|alias| !alias.is_empty());
        aliases.sort();
        aliases.dedup();

        let mut tags = page.labels.clone();
        if !page.space_key.is_empty() {
            tags.push(page.space_key.clone());
        }
        tags.retain(|tag| !tag.is_empty());

        let updated = parse_time(&page.updated).unwrap_or_else(Utc::now);
        let document = DocumentNode {
            path: page.id.clone(),
            title: page.title.clone(),
            aliases,
            tags,
            body: render_page_body(page),
            content_hash: page_content_hash(page),
            updated_at: updated,
        };

        Ok(ExtractBatch {
            documents: vec![document],
            ..ExtractBatch::default()
        })
    }
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
