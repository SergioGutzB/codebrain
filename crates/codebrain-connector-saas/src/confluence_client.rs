//! Confluence Cloud REST client (read-only).

use std::time::Duration;

use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;

use crate::adf::adf_to_text;
use crate::client::JiraAuth;
use crate::error::{Result, SaasError};
use crate::html::html_to_text;

#[derive(Debug, Clone)]
pub struct ConfluencePage {
    pub id: String,
    pub title: String,
    pub space_key: String,
    pub body: String,
    pub updated: String,
    pub url: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    results: Vec<Value>,
    size: Option<usize>,
    #[serde(rename = "_links")]
    links: Option<Links>,
}

#[derive(Debug, Deserialize)]
struct Links {
    next: Option<String>,
}

#[derive(Clone)]
pub struct ConfluenceClient {
    http: reqwest::Client,
    auth: JiraAuth,
}

impl ConfluenceClient {
    pub fn new(auth: JiraAuth) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("codebrain-confluence-connector/1.0"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| SaasError::Http(error.to_string()))?;
        Ok(Self { http, auth })
    }

    pub async fn search(&self, cql: &str, max_pages: usize) -> Result<Vec<ConfluencePage>> {
        let mut out = Vec::new();
        let page_size = max_pages.clamp(1, 50);
        let mut start: usize = 0;

        loop {
            if out.len() >= max_pages {
                break;
            }
            let remaining = max_pages - out.len();
            let take = remaining.min(page_size);

            let mut url = reqwest::Url::parse(&format!(
                "{}/wiki/rest/api/content/search",
                self.auth.base_url.trim_end_matches('/')
            ))
            .map_err(|error| SaasError::Config(error.to_string()))?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("cql", cql);
                query.append_pair("limit", &take.to_string());
                query.append_pair("start", &start.to_string());
                query.append_pair(
                    "expand",
                    "body.atlas_doc_format,body.storage,space,version,metadata.labels",
                );
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
                tracing::warn!(retry_after_s = retry, "confluence rate limited; sleeping");
                tokio::time::sleep(Duration::from_secs(retry)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(SaasError::Http(format!(
                    "confluence search {status}: {body}"
                )));
            }

            let payload: SearchResponse = response
                .json()
                .await
                .map_err(|error| SaasError::Http(error.to_string()))?;
            let batch_len = payload.results.len();
            for raw in &payload.results {
                out.push(normalize_page(raw, &self.auth.base_url)?);
                if out.len() >= max_pages {
                    break;
                }
            }

            let has_next = payload
                .links
                .as_ref()
                .and_then(|links| links.next.as_ref())
                .is_some();
            if !has_next || batch_len == 0 {
                break;
            }
            start += payload.size.unwrap_or(batch_len);
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        Ok(out)
    }
}

fn normalize_page(raw: &Value, base_url: &str) -> Result<ConfluencePage> {
    let id = raw
        .get("id")
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|n| n.to_string()))
        })
        .context("confluence page missing id")
        .map_err(|error| SaasError::Message(error.to_string()))?;
    let title = raw
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let space_key = raw
        .pointer("/space/key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let updated = raw
        .pointer("/version/when")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let labels = raw
        .pointer("/metadata/labels/results")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let body = page_body_text(raw);
    let links_webui = raw
        .pointer("/_links/webui")
        .and_then(Value::as_str)
        .unwrap_or("");
    let url = if links_webui.starts_with("http") {
        links_webui.to_string()
    } else if !links_webui.is_empty() {
        format!(
            "{}/wiki{}",
            base_url.trim_end_matches('/'),
            if links_webui.starts_with('/') {
                links_webui.to_string()
            } else {
                format!("/{links_webui}")
            }
        )
    } else {
        format!(
            "{}/wiki/spaces/{space_key}/pages/{id}",
            base_url.trim_end_matches('/')
        )
    };

    Ok(ConfluencePage {
        id,
        title,
        space_key,
        body,
        updated,
        url,
        labels,
    })
}

fn page_body_text(raw: &Value) -> String {
    if let Some(adf_value) = raw.pointer("/body/atlas_doc_format/value") {
        if let Some(text) = adf_value.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                let flat = adf_to_text(&parsed).trim().to_string();
                if !flat.is_empty() {
                    return flat;
                }
            }
        } else if adf_value.is_object() {
            let flat = adf_to_text(adf_value).trim().to_string();
            if !flat.is_empty() {
                return flat;
            }
        }
    }
    raw.pointer("/body/storage/value")
        .and_then(Value::as_str)
        .map(html_to_text)
        .unwrap_or_default()
}

pub fn page_content_hash(page: &ConfluencePage) -> String {
    let payload = format!(
        "{}|{}|{}|{}|{}",
        page.id, page.updated, page.title, page.space_key, page.body
    );
    blake3::hash(payload.as_bytes()).to_hex().to_string()
}

pub fn render_page_body(page: &ConfluencePage) -> String {
    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", page.title));
    if !page.space_key.is_empty() {
        body.push_str(&format!("- space: `{}`\n", page.space_key));
    }
    body.push_str(&format!("- page_id: `{}`\n", page.id));
    body.push_str(&format!("- url: {}\n", page.url));
    if !page.labels.is_empty() {
        body.push_str(&format!("- labels: {}\n", page.labels.join(", ")));
    }
    body.push('\n');
    if !page.body.is_empty() {
        body.push_str(&page.body);
        body.push('\n');
    }
    body
}
