//! Embedding providers and chunking for GraphRAG.

mod chunk;
mod error;
mod factory;
mod providers;

pub use chunk::{ChunkDraft, chunk_document, chunk_symbol};
pub use error::{EmbedError, Result};
pub use factory::{EmbedderConfig, EmbedderKind, build_embedder};
pub use providers::{Embedder, FastembedEmbedder, HashEmbedder, NoneEmbedder, OpenAiEmbedder};
