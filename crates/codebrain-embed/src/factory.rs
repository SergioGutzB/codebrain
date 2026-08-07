use std::sync::Arc;

use crate::error::Result;
use crate::providers::{Embedder, FastembedEmbedder, HashEmbedder, NoneEmbedder, OpenAiEmbedder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedderKind {
    None,
    Fastembed,
    OpenaiCompatible,
    /// Deterministic vectors for tests (not exposed in product config).
    Hash,
}

#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    pub kind: EmbedderKind,
    pub model: String,
    pub dimension: usize,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            kind: EmbedderKind::None,
            model: "all-MiniLM-L6-v2".into(),
            dimension: 384,
            base_url: None,
            api_key_env: None,
        }
    }
}

/// Build the configured embedder. Fastembed may download model weights on first call.
pub fn build_embedder(config: &EmbedderConfig) -> Result<Arc<dyn Embedder>> {
    match config.kind {
        EmbedderKind::None => Ok(Arc::new(NoneEmbedder::new(config.dimension))),
        EmbedderKind::Hash => Ok(Arc::new(HashEmbedder::new(config.dimension))),
        EmbedderKind::Fastembed => Ok(Arc::new(FastembedEmbedder::try_new(
            &config.model,
            config.dimension,
        )?)),
        EmbedderKind::OpenaiCompatible => Ok(Arc::new(OpenAiEmbedder::try_new(
            &config.model,
            config.dimension,
            config.base_url.as_deref(),
            config.api_key_env.as_deref(),
        )?)),
    }
}
