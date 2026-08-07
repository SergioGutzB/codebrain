//! Jira → CodeBrain documents.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use codebrain_connector::{
    Connector, DocumentNode, ExtractBatch, IndexContext, SourceKind, WorkItem,
};

use crate::client::{JiraAuth, JiraClient, JiraIssue, issue_content_hash, render_issue_body};
use crate::error::{Result, SaasError};

#[derive(Clone)]
pub struct JiraConnector {
    id: String,
    client: Arc<JiraClient>,
    jql: String,
    max_issues: usize,
    /// Cached discover results so extract does not re-hit the API.
    cache: Arc<tokio::sync::RwLock<Vec<JiraIssue>>>,
}

impl JiraConnector {
    pub fn new(
        id: impl Into<String>,
        auth: JiraAuth,
        jql: impl Into<String>,
        max_issues: usize,
    ) -> Result<Self> {
        Ok(Self {
            id: id.into(),
            client: Arc::new(JiraClient::new(auth)?),
            jql: jql.into(),
            max_issues: max_issues.max(1),
            cache: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        })
    }

    async fn ensure_cache(&self) -> Result<()> {
        let cached = self.cache.read().await;
        if !cached.is_empty() {
            return Ok(());
        }
        drop(cached);
        let issues = self.client.search(&self.jql, self.max_issues).await?;
        *self.cache.write().await = issues;
        Ok(())
    }
}

#[async_trait]
impl Connector for JiraConnector {
    fn id(&self) -> &str {
        &self.id
    }

    fn source_kind(&self) -> SourceKind {
        SourceKind::Jira
    }

    async fn discover(&self, _ctx: &IndexContext) -> anyhow::Result<Vec<WorkItem>> {
        self.ensure_cache().await?;
        let issues = self.cache.read().await;
        Ok(issues
            .iter()
            .map(|issue| WorkItem {
                id: issue.key.clone(),
                path: issue.key.clone(),
                content_hash: Some(issue_content_hash(issue)),
                mtime: parse_jira_time(&issue.updated),
            })
            .collect())
    }

    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch> {
        self.ensure_cache().await?;
        let issues = self.cache.read().await;
        let Some(issue) = issues.iter().find(|issue| issue.key == item.id) else {
            return Err(SaasError::Message(format!("jira issue not in cache: {}", item.id)).into());
        };

        let mut tags = issue.labels.clone();
        tags.push(issue.status.clone());
        tags.push(issue.issue_type.clone());
        tags.retain(|tag| !tag.is_empty());

        let updated = parse_jira_time(&issue.updated).unwrap_or_else(Utc::now);
        let document = DocumentNode {
            path: issue.key.clone(),
            title: format!("{} — {}", issue.key, issue.summary),
            aliases: vec![issue.key.clone()],
            tags,
            body: render_issue_body(issue),
            content_hash: issue_content_hash(issue),
            updated_at: updated,
        };

        Ok(ExtractBatch {
            documents: vec![document],
            ..ExtractBatch::default()
        })
    }
}

fn parse_jira_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}
