//! Jira Cloud REST client (read-only).

use std::time::Duration;

use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;

use crate::adf::adf_to_text;
use crate::error::{Result, SaasError};

const ISSUE_FIELDS: &str = "summary,description,status,issuetype,priority,labels,components,assignee,reporter,created,updated,parent";

#[derive(Debug, Clone)]
pub struct JiraAuth {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
}

#[derive(Debug, Clone)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub description: String,
    pub status: String,
    pub issue_type: String,
    pub priority: Option<String>,
    pub labels: Vec<String>,
    pub updated: String,
    pub created: String,
    pub url: String,
    pub assignee: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    issues: Vec<Value>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "isLast")]
    is_last: Option<bool>,
}

#[derive(Clone)]
pub struct JiraClient {
    http: reqwest::Client,
    auth: JiraAuth,
}

impl JiraClient {
    pub fn new(auth: JiraAuth) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("codebrain-jira-connector/1.0"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| SaasError::Http(error.to_string()))?;
        Ok(Self { http, auth })
    }

    pub async fn search(&self, jql: &str, max_issues: usize) -> Result<Vec<JiraIssue>> {
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        let page_size = max_issues.clamp(1, 100);

        loop {
            if out.len() >= max_issues {
                break;
            }
            let remaining = max_issues - out.len();
            let take = remaining.min(page_size);

            let mut url = reqwest::Url::parse(&format!(
                "{}/rest/api/3/search/jql",
                self.auth.base_url.trim_end_matches('/')
            ))
            .map_err(|error| SaasError::Config(error.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("jql", jql);
                query.append_pair("maxResults", &take.to_string());
                query.append_pair("fields", ISSUE_FIELDS);
                if let Some(token) = &page_token {
                    query.append_pair("nextPageToken", token);
                }
            }

            let response = self
                .http
                .get(url)
                .basic_auth(&self.auth.email, Some(&self.auth.api_token))
                .send()
                .await
                .map_err(|error| SaasError::Http(error.to_string()))?;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(2);
                tracing::warn!(retry_after_s = retry, "jira rate limited; sleeping");
                tokio::time::sleep(Duration::from_secs(retry)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(SaasError::Http(format!("jira search {status}: {body}")));
            }

            let payload: SearchResponse = response
                .json()
                .await
                .map_err(|error| SaasError::Http(error.to_string()))?;
            for issue in &payload.issues {
                out.push(normalize_issue(issue, &self.auth.base_url)?);
                if out.len() >= max_issues {
                    break;
                }
            }

            let done = payload.is_last.unwrap_or(true) || payload.next_page_token.is_none();
            if done {
                break;
            }
            page_token = payload.next_page_token;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Ok(out)
    }
}

fn normalize_issue(raw: &Value, base_url: &str) -> Result<JiraIssue> {
    let key = raw
        .get("key")
        .and_then(Value::as_str)
        .context("jira issue missing key")
        .map_err(|error| SaasError::Message(error.to_string()))?
        .to_string();
    if key.is_empty() {
        return Err(SaasError::Message("empty jira issue key".into()));
    }
    let fields = raw.get("fields").cloned().unwrap_or(Value::Null);
    let summary = fields
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let description = fields
        .get("description")
        .map(adf_to_text)
        .unwrap_or_default()
        .trim()
        .to_string();
    let status = fields
        .pointer("/status/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let issue_type = fields
        .pointer("/issuetype/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let priority = fields
        .pointer("/priority/name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let labels = fields
        .get("labels")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let updated = fields
        .get("updated")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let created = fields
        .get("created")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let assignee = fields
        .pointer("/assignee/displayName")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(JiraIssue {
        key: key.clone(),
        summary,
        description,
        status,
        issue_type,
        priority,
        labels,
        updated,
        created,
        assignee,
        url: format!("{}/browse/{key}", base_url.trim_end_matches('/')),
    })
}

pub fn issue_content_hash(issue: &JiraIssue) -> String {
    let payload = format!(
        "{}|{}|{}|{}|{}",
        issue.key, issue.updated, issue.summary, issue.status, issue.description
    );
    blake3::hash(payload.as_bytes()).to_hex().to_string()
}

pub fn render_issue_body(issue: &JiraIssue) -> String {
    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", issue.summary));
    body.push_str(&format!("- key: `{}`\n", issue.key));
    body.push_str(&format!("- type: {}\n", issue.issue_type));
    body.push_str(&format!("- status: {}\n", issue.status));
    if let Some(priority) = &issue.priority {
        body.push_str(&format!("- priority: {priority}\n"));
    }
    if let Some(assignee) = &issue.assignee {
        body.push_str(&format!("- assignee: {assignee}\n"));
    }
    body.push_str(&format!("- url: {}\n", issue.url));
    if !issue.labels.is_empty() {
        body.push_str(&format!("- labels: {}\n", issue.labels.join(", ")));
    }
    body.push('\n');
    if !issue.description.is_empty() {
        body.push_str("## Description\n\n");
        body.push_str(&issue.description);
        body.push('\n');
    }
    body
}
