use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{EmbedError, Result};
use crate::providers::Embedder;

/// OpenAI-compatible `/v1/embeddings` client (e.g. OpenAI, Ollama, local gateways).
pub struct OpenAiEmbedder {
    model: String,
    dimension: usize,
    base_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiEmbedder {
    pub fn try_new(
        model: &str,
        dimension: usize,
        base_url: Option<&str>,
        api_key_env: Option<&str>,
    ) -> Result<Self> {
        let env_name = api_key_env.unwrap_or("OPENAI_API_KEY");
        let api_key =
            std::env::var(env_name).map_err(|_| EmbedError::MissingApiKey(env_name.to_string()))?;
        let base_url = base_url
            .unwrap_or("https://api.openai.com/v1")
            .trim_end_matches('/')
            .to_string();
        Ok(Self {
            model: model.to_string(),
            dimension,
            base_url,
            api_key,
            client: reqwest::Client::new(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    fn kind(&self) -> &'static str {
        "openai_compatible"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!("{}/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| EmbedError::OpenAi(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("<unreadable body>"));
            return Err(EmbedError::OpenAi(format!("HTTP {status}: {text}")));
        }
        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|error| EmbedError::OpenAi(error.to_string()))?;
        let mut ordered = vec![Vec::new(); texts.len()];
        for item in parsed.data {
            if item.index >= ordered.len() {
                return Err(EmbedError::OpenAi(format!(
                    "unexpected embedding index {}",
                    item.index
                )));
            }
            if item.embedding.len() != self.dimension {
                return Err(EmbedError::Dimension {
                    expected: self.dimension,
                    got: item.embedding.len(),
                });
            }
            ordered[item.index] = item.embedding;
        }
        if ordered.iter().any(Vec::is_empty) {
            return Err(EmbedError::OpenAi(
                "provider returned fewer embeddings than inputs".into(),
            ));
        }
        Ok(ordered)
    }
}
