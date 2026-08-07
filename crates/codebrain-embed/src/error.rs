use thiserror::Error;

pub type Result<T> = std::result::Result<T, EmbedError>;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("embeddings are disabled (provider=none)")]
    Disabled,

    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    Dimension { expected: usize, got: usize },

    #[error("fastembed failed: {0}")]
    Fastembed(String),

    #[error("openai-compatible request failed: {0}")]
    OpenAi(String),

    #[error("missing embedding API key from env `{0}`")]
    MissingApiKey(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
