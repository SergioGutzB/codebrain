//! Notion REST client (read-only, internal integration token).

use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{Result, SaasError};
use crate::notion_text::{block_to_text, page_title_from_properties};

const NOTION_VERSION: &str = "2022-06-28";
const API_BASE: &str = "https://api.notion.com/v1";
const MAX_BLOCK_DEPTH: usize = 3;
const MAX_BLOCKS_PER_PAGE: usize = 400;

#[derive(Debug, Clone)]
pub struct NotionAuth {
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    results: Vec<Value>,
    #[serde(default)]
    has_more: bool,
    next_cursor: Option<String>,
}

#[derive(Clone)]
pub struct NotionClient {
    http: reqwest::Client,
    auth: NotionAuth,
}

impl NotionClient {
    pub fn new(auth: NotionAuth) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("codebrain-notion-connector/1.0"),
        );
        headers.insert("Notion-Version", HeaderValue::from_static(NOTION_VERSION));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| SaasError::Http(error.to_string()))?;
        Ok(Self { http, auth })
    }

    pub async fn search_pages(&self, query: &str, max_pages: usize) -> Result<Vec<NotionPage>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        let page_size = max_pages.clamp(1, 100);

        loop {
            if out.len() >= max_pages {
                break;
            }
            let remaining = max_pages - out.len();
            let take = remaining.min(page_size);

            let mut body = json!({
                "filter": { "property": "object", "value": "page" },
                "sort": { "direction": "descending", "timestamp": "last_edited_time" },
                "page_size": take,
            });
            if !query.trim().is_empty() {
                body["query"] = Value::String(query.trim().to_string());
            }
            if let Some(cursor) = &cursor {
                body["start_cursor"] = Value::String(cursor.clone());
            }

            let response = self
                .http
                .post(format!("{API_BASE}/search"))
                .header(AUTHORIZATION, format!("Bearer {}", self.auth.token))
                .json(&body)
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
                tracing::warn!(retry_after_s = retry, "notion rate limited; sleeping");
                tokio::time::sleep(Duration::from_secs(retry)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(SaasError::Http(format!("notion search {status}: {body}")));
            }

            let payload: ListResponse = response
                .json()
                .await
                .map_err(|error| SaasError::Http(error.to_string()))?;

            for raw in &payload.results {
                if raw.get("object").and_then(Value::as_str) != Some("page") {
                    continue;
                }
                let page = self.hydrate_page(raw).await?;
                out.push(page);
                if out.len() >= max_pages {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            if !payload.has_more || payload.next_cursor.is_none() {
                break;
            }
            cursor = payload.next_cursor;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Ok(out)
    }

    async fn hydrate_page(&self, raw: &Value) -> Result<NotionPage> {
        let id = raw
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| SaasError::Message("notion page missing id".into()))?
            .to_string();
        let title = page_title_from_properties(raw.get("properties").unwrap_or(&Value::Null));
        let updated = raw
            .get("last_edited_time")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let url = raw
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let body = self.fetch_blocks_text(&id, 0).await?;
        Ok(NotionPage {
            id,
            title,
            body,
            updated,
            url,
        })
    }

    async fn fetch_blocks_text(&self, block_id: &str, depth: usize) -> Result<String> {
        if depth > MAX_BLOCK_DEPTH {
            return Ok(String::new());
        }
        let mut out = String::new();
        let mut cursor: Option<String> = None;
        let mut total = 0usize;

        loop {
            let mut url = reqwest::Url::parse(&format!("{API_BASE}/blocks/{block_id}/children"))
                .map_err(|error| SaasError::Config(error.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("page_size", "100");
                if let Some(cursor) = &cursor {
                    query.append_pair("start_cursor", cursor);
                }
            }

            let response = self
                .http
                .get(url)
                .header(AUTHORIZATION, format!("Bearer {}", self.auth.token))
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
                tokio::time::sleep(Duration::from_secs(retry)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(SaasError::Http(format!(
                    "notion blocks {block_id} {status}: {body}"
                )));
            }

            let payload: ListResponse = response
                .json()
                .await
                .map_err(|error| SaasError::Http(error.to_string()))?;

            for block in &payload.results {
                out.push_str(&block_to_text(block));
                total += 1;
                if total >= MAX_BLOCKS_PER_PAGE {
                    return Ok(out);
                }
                let has_children = block
                    .get("has_children")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                // Skip diving into child pages/databases (separate documents via search).
                if has_children
                    && depth < MAX_BLOCK_DEPTH
                    && btype != "child_page"
                    && btype != "child_database"
                {
                    if let Some(child_id) = block.get("id").and_then(Value::as_str) {
                        let nested = Box::pin(self.fetch_blocks_text(child_id, depth + 1)).await?;
                        out.push_str(&nested);
                    }
                }
            }

            if !payload.has_more || payload.next_cursor.is_none() {
                break;
            }
            cursor = payload.next_cursor;
        }

        Ok(out)
    }
}

pub fn page_content_hash(page: &NotionPage) -> String {
    let payload = format!("{}|{}|{}|{}", page.id, page.updated, page.title, page.body);
    blake3::hash(payload.as_bytes()).to_hex().to_string()
}

pub fn render_page_body(page: &NotionPage) -> String {
    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", page.title));
    body.push_str(&format!("- page_id: `{}`\n", page.id));
    if !page.url.is_empty() {
        body.push_str(&format!("- url: {}\n", page.url));
    }
    body.push('\n');
    if !page.body.is_empty() {
        body.push_str(&page.body);
        body.push('\n');
    }
    body
}
