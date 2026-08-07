//! Domain types, configuration, and orchestration for CodeBrain.

mod adr;
mod config;
mod doctor;
mod indexer;
mod linker;
mod paths;
mod promote;
mod query;
mod semantic;
mod watch;

pub use adr::{AddDecisionRequest, AddDecisionResult, add_architectural_decision, adr_note_path};
pub use config::{
    AdrConfig, Config, DatabaseConfig, EmbeddingsConfig, EmbeddingsProvider, IndexConfig,
    LinkerConfig, McpConfig, SourceConfig, SourceKindConfig, load_config,
};
pub use doctor::{CheckStatus, DoctorCheck, DoctorReport, run_doctor};
pub use indexer::{IndexReport, SourceIndexReport, index_configured_sources, reindex_source_paths};
pub use linker::{MentionIndex, MentionMatch, find_mentions, find_mentions_indexed};
pub use paths::{default_config_path, default_data_dir, expand_path};
pub use promote::{PromoteMentionRequest, PromoteMentionResult, promote_mention_edge};
pub use query::{
    ContextBundle, Neighborhood, NeighborhoodEdge, NeighborhoodNode, QueryBudget, documents,
    explore_context, neighborhood, resolve_source_filter, sources, symbols,
};
pub use semantic::{
    FusionWeights, SemanticHit, SemanticSearchResult, embed_documents, embed_extract_batch,
    embed_symbols, prepare_embedding_store, remove_document_chunks, remove_symbol_chunks,
    semantic_search,
};
pub use watch::{PendingChanges, ReindexJob, run_reindex_worker, spawn_watchers};

/// Build an embedder from product config (shared by indexer and MCP).
pub fn embedder_from_config(
    config: &Config,
) -> anyhow::Result<std::sync::Arc<dyn codebrain_embed::Embedder>> {
    use codebrain_embed::{EmbedderConfig, EmbedderKind, build_embedder};
    Ok(build_embedder(&EmbedderConfig {
        kind: match config.embeddings.provider {
            EmbeddingsProvider::None => EmbedderKind::None,
            EmbeddingsProvider::Fastembed => EmbedderKind::Fastembed,
            EmbeddingsProvider::OpenaiCompatible => EmbedderKind::OpenaiCompatible,
        },
        model: config.embeddings.model.clone(),
        dimension: config.embeddings.dimension as usize,
        base_url: config.embeddings.base_url.clone(),
        api_key_env: config.embeddings.api_key_env.clone(),
    })?)
}
