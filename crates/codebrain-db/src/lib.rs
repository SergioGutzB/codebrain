//! SurrealDB client, schema migrations, and persistence helpers for CodeBrain.

mod adr;
mod chunks;
mod client;
mod documents;
mod error;
mod graph;
mod ids;
mod migrate;
mod queries;
mod resolve;
mod status;

pub use adr::{
    ArchitectureDecision, decision_address, get_architecture_decision, relate_about,
    upsert_architecture_decision,
};
pub use chunks::{
    ChunkHit, StoredChunk, chunk_record_id, delete_chunks_for_parent, ensure_chunk_vector_index,
    fts_chunks, knn_chunks, read_embedding_dimension, record_embedding_meta, replace_chunks,
};
pub use client::{Database, open_and_migrate, open_embedded, open_memory};
pub use documents::{
    PersistedDocument, PromotedExplain, SymbolMentionTarget, delete_document,
    existing_document_hashes, list_symbols_for_mentions, persist_document_batch, promote_mention,
    relate_cross_reference, relate_mention, relate_reference, relate_resolves,
    upsert_confluence_source, upsert_jira_source, upsert_notion_source, upsert_obsidian_source,
};
pub use error::{DbError, Result};
pub use graph::{
    PersistedBatch, delete_code_file, existing_file_hashes, file_content_hash, find_symbol_fqn,
    list_symbol_fqns_for_file, persist_code_batch, relate_call, relate_import, upsert_code_source,
};
pub use migrate::{SCHEMA_VERSION, apply_schema, current_schema_version};
pub use queries::{
    Direction, DocumentHit, NeighborEdge, NodeAddress, NodeKind, SourceSummary, SymbolHit,
    highlight_excerpt, list_sources, neighbors, search_documents, search_symbols,
};
pub use resolve::{DocumentLookup, list_documents_for_resolution, resolve_wikilink};
pub use status::{DatabaseStatus, TableCount, collect_status};
