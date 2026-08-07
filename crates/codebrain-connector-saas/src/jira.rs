//! Jira → CodeBrain documents.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use codebrain_connector::{
    Connector, DocumentNode, ExtractBatch, IndexContext, SourceKind, WorkItem,
};

use crate::client::{JiraAuth, JiraClient, JiraIssue, issue_content_hash, render_issue_body};
use crate::error::{Result, SaasError};
use crate::jira_cursor::parse_jira_updated;

#[derive(Clone)]
pub struct JiraConnector {
    id: String,
    client: Arc<JiraClient>,
    jql: String,
    max_issues: usize,
    /// Cached discover results so extract does not re-hit the API.
    /// `None` = not loaded yet; `Some(vec)` may be empty after a successful search.
    cache: Arc<tokio::sync::RwLock<Option<Vec<JiraIssue>>>>,
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
            cache: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Raw issues from the last discover (for cursor advancement).
    pub async fn cached_issues(&self) -> Vec<JiraIssue> {
        self.cache.read().await.clone().unwrap_or_default()
    }

    async fn ensure_cache(&self) -> Result<()> {
        {
            let cached = self.cache.read().await;
            if cached.is_some() {
                return Ok(());
            }
        }
        let issues = self.client.search(&self.jql, self.max_issues).await?;
        *self.cache.write().await = Some(issues);
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
        let Some(issues) = issues.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(issues
            .iter()
            .map(|issue| WorkItem {
                id: issue.key.clone(),
                path: issue.key.clone(),
                content_hash: Some(issue_content_hash(issue)),
                mtime: parse_jira_updated(&issue.updated),
            })
            .collect())
    }

    async fn extract(&self, item: &WorkItem) -> anyhow::Result<ExtractBatch> {
        self.ensure_cache().await?;
        let issues = self.cache.read().await;
        let Some(issues) = issues.as_ref() else {
            return Err(SaasError::Message("jira cache not loaded".into()).into());
        };
        let Some(issue) = issues.iter().find(|issue| issue.key == item.id) else {
            return Err(SaasError::Message(format!("jira issue not in cache: {}", item.id)).into());
        };

        let mut tags = issue.labels.clone();
        tags.push(issue.status.clone());
        tags.push(issue.issue_type.clone());
        tags.retain(|tag| !tag.is_empty());

        let updated = parse_jira_updated(&issue.updated).unwrap_or_else(Utc::now);
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

#[cfg(test)]
mod tests {
    use crate::jira_cursor::parse_jira_updated;

    #[test]
    fn parses_jira_offset_without_colon() {
        assert!(parse_jira_updated("2026-08-07T12:00:00.000+0000").is_some());
    }
}
