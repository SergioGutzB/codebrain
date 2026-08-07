use async_trait::async_trait;

use crate::error::{EmbedError, Result};

mod fastembed_provider;
mod hash;
mod none;
mod openai;

pub use fastembed_provider::FastembedEmbedder;
pub use hash::HashEmbedder;
pub use none::NoneEmbedder;
pub use openai::OpenAiEmbedder;

/// Pluggable text → vector encoder. Implementations must be `Send + Sync`.
#[async_trait]
pub trait Embedder: Send + Sync {
    fn kind(&self) -> &'static str;
    fn model(&self) -> &str;
    fn dimension(&self) -> usize;
    fn enabled(&self) -> bool;

    /// Embed one or more texts. Empty input returns an empty vec.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    async fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut vectors = self.embed(&[text.to_string()]).await?;
        vectors
            .pop()
            .ok_or_else(|| EmbedError::Other(anyhow::anyhow!("embedder returned no vector")))
    }
}
