use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::error::{EmbedError, Result};
use crate::providers::Embedder;

/// Local ONNX MiniLM (or configured) embeddings via `fastembed`.
pub struct FastembedEmbedder {
    model_name: String,
    dimension: usize,
    inner: Arc<Mutex<TextEmbedding>>,
}

impl FastembedEmbedder {
    pub fn try_new(model: &str, expected_dimension: usize) -> Result<Self> {
        let embedding_model = resolve_model(model)?;
        let options = InitOptions::new(embedding_model).with_show_download_progress(true);
        let mut engine = TextEmbedding::try_new(options)
            .map_err(|error| EmbedError::Fastembed(error.to_string()))?;

        // Probe once so we fail early on dimension mismatches.
        let probe = engine
            .embed(vec!["codebrain dimension probe"], Some(1))
            .map_err(|error| EmbedError::Fastembed(error.to_string()))?;
        let got = probe.first().map_or(0, Vec::len);
        if got != expected_dimension {
            return Err(EmbedError::Dimension {
                expected: expected_dimension,
                got,
            });
        }

        Ok(Self {
            model_name: model.to_string(),
            dimension: expected_dimension,
            inner: Arc::new(Mutex::new(engine)),
        })
    }
}

fn resolve_model(model: &str) -> Result<EmbeddingModel> {
    let normalized = model.trim().to_ascii_lowercase();
    let chosen = match normalized.as_str() {
        "all-minilm-l6-v2" | "sentence-transformers/all-minilm-l6-v2" | "minilm" => {
            EmbeddingModel::AllMiniLML6V2
        }
        "all-minilm-l12-v2" => EmbeddingModel::AllMiniLML12V2,
        "bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
        other => {
            return Err(EmbedError::Fastembed(format!(
                "unsupported fastembed model `{other}` (try all-MiniLM-L6-v2)"
            )));
        }
    };
    Ok(chosen)
}

#[async_trait]
impl Embedder for FastembedEmbedder {
    fn kind(&self) -> &'static str {
        "fastembed"
    }

    fn model(&self) -> &str {
        &self.model_name
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
        let texts = texts.to_vec();
        let dimension = self.dimension;
        let engine = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = engine
                .lock()
                .map_err(|_| EmbedError::Fastembed("fastembed mutex poisoned".into()))?;
            let vectors = guard
                .embed(texts, None)
                .map_err(|error| EmbedError::Fastembed(error.to_string()))?;
            for vector in &vectors {
                if vector.len() != dimension {
                    return Err(EmbedError::Dimension {
                        expected: dimension,
                        got: vector.len(),
                    });
                }
            }
            Ok(vectors)
        })
        .await
        .map_err(|error| EmbedError::Fastembed(error.to_string()))?
    }
}
