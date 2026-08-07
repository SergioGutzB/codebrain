use async_trait::async_trait;

use crate::error::{EmbedError, Result};
use crate::providers::Embedder;

/// No-op provider used when `embeddings.provider = none`.
pub struct NoneEmbedder {
    dimension: usize,
}

impl NoneEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl Embedder for NoneEmbedder {
    fn kind(&self) -> &'static str {
        "none"
    }

    fn model(&self) -> &str {
        "none"
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn enabled(&self) -> bool {
        false
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Err(EmbedError::Disabled)
    }
}
