-- CodeBrain schema v1
-- Idempotent: safe to re-run on every init / migrate.

DEFINE NAMESPACE IF NOT EXISTS codebrain;
DEFINE DATABASE IF NOT EXISTS codebrain;

USE NS codebrain DB codebrain;

-- Schema version tracking
DEFINE TABLE IF NOT EXISTS meta SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS key ON meta TYPE string;
DEFINE FIELD IF NOT EXISTS value ON meta TYPE string;
DEFINE FIELD IF NOT EXISTS updated_at ON meta TYPE datetime;
DEFINE INDEX IF NOT EXISTS meta_key ON meta FIELDS key UNIQUE;

-- ===== Sources =====
DEFINE TABLE IF NOT EXISTS source SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS kind ON source TYPE string
    ASSERT $value IN ['git_repo', 'obsidian_vault', 'notion', 'confluence', 'jira'];
DEFINE FIELD IF NOT EXISTS name ON source TYPE string;
DEFINE FIELD IF NOT EXISTS root_path ON source TYPE option<string>;
DEFINE FIELD IF NOT EXISTS remote_url ON source TYPE option<string>;
DEFINE FIELD IF NOT EXISTS last_indexed ON source TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS meta ON source FLEXIBLE TYPE object;
DEFINE INDEX IF NOT EXISTS source_name ON source FIELDS name UNIQUE;

-- ===== Code =====
DEFINE TABLE IF NOT EXISTS file SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS source ON file TYPE record<source>;
DEFINE FIELD IF NOT EXISTS path ON file TYPE string;
DEFINE FIELD IF NOT EXISTS language ON file TYPE option<string>;
DEFINE FIELD IF NOT EXISTS content_hash ON file TYPE string;
DEFINE FIELD IF NOT EXISTS mtime ON file TYPE datetime;
DEFINE FIELD IF NOT EXISTS embedding ON file TYPE option<array<float>>;
DEFINE INDEX IF NOT EXISTS file_path ON file FIELDS source, path UNIQUE;

DEFINE TABLE IF NOT EXISTS symbol SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS source ON symbol TYPE record<source>;
DEFINE FIELD IF NOT EXISTS file ON symbol TYPE record<file>;
DEFINE FIELD IF NOT EXISTS name ON symbol TYPE string;
DEFINE FIELD IF NOT EXISTS fqn ON symbol TYPE string;
DEFINE FIELD IF NOT EXISTS kind ON symbol TYPE string;
DEFINE FIELD IF NOT EXISTS signature ON symbol TYPE option<string>;
DEFINE FIELD IF NOT EXISTS start_line ON symbol TYPE int;
DEFINE FIELD IF NOT EXISTS end_line ON symbol TYPE int;
DEFINE FIELD IF NOT EXISTS content_hash ON symbol TYPE string;
DEFINE FIELD IF NOT EXISTS embedding ON symbol TYPE option<array<float>>;
DEFINE INDEX IF NOT EXISTS symbol_fqn ON symbol FIELDS source, fqn UNIQUE;
DEFINE INDEX IF NOT EXISTS symbol_name ON symbol FIELDS name;

-- ===== Documents (Obsidian now; SaaS later) =====
DEFINE TABLE IF NOT EXISTS document SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS source ON document TYPE record<source>;
DEFINE FIELD IF NOT EXISTS path ON document TYPE string;
DEFINE FIELD IF NOT EXISTS title ON document TYPE string;
DEFINE FIELD IF NOT EXISTS aliases ON document TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS tags ON document TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS body ON document TYPE string;
DEFINE FIELD IF NOT EXISTS content_hash ON document TYPE string;
DEFINE FIELD IF NOT EXISTS updated_at ON document TYPE datetime;
DEFINE FIELD IF NOT EXISTS embedding ON document TYPE option<array<float>>;
DEFINE INDEX IF NOT EXISTS document_path ON document FIELDS source, path UNIQUE;
DEFINE INDEX IF NOT EXISTS document_title ON document FIELDS title;

-- Chunks for semantic search (Phase 4; table ready in v1)
DEFINE TABLE IF NOT EXISTS chunk SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS parent ON chunk TYPE string;
DEFINE FIELD IF NOT EXISTS ordinal ON chunk TYPE int;
DEFINE FIELD IF NOT EXISTS text ON chunk TYPE string;
DEFINE FIELD IF NOT EXISTS embedding ON chunk TYPE option<array<float>>;
DEFINE INDEX IF NOT EXISTS chunk_parent ON chunk FIELDS parent;

-- ===== Architecture decisions (write-back, Phase 5) =====
DEFINE TABLE IF NOT EXISTS architecture_decision SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS title ON architecture_decision TYPE string;
DEFINE FIELD IF NOT EXISTS body ON architecture_decision TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON architecture_decision TYPE datetime;
DEFINE FIELD IF NOT EXISTS created_by ON architecture_decision TYPE string;
DEFINE FIELD IF NOT EXISTS vault_path ON architecture_decision TYPE option<string>;
DEFINE FIELD IF NOT EXISTS embedding ON architecture_decision TYPE option<array<float>>;

-- ===== Relations =====
DEFINE TABLE IF NOT EXISTS contains TYPE RELATION IN source OUT file | document ENFORCED;
DEFINE TABLE IF NOT EXISTS defines TYPE RELATION IN file OUT symbol ENFORCED;
DEFINE TABLE IF NOT EXISTS imports TYPE RELATION IN file | symbol OUT file | symbol ENFORCED;
DEFINE TABLE IF NOT EXISTS calls TYPE RELATION IN symbol OUT symbol ENFORCED;
DEFINE TABLE IF NOT EXISTS references TYPE RELATION IN document OUT document ENFORCED;

DEFINE TABLE IF NOT EXISTS mentions TYPE RELATION IN document OUT symbol ENFORCED;
DEFINE FIELD IF NOT EXISTS confidence ON mentions TYPE float DEFAULT 0.0;
DEFINE FIELD IF NOT EXISTS evidence ON mentions TYPE option<string>;

DEFINE TABLE IF NOT EXISTS explains TYPE RELATION IN document | architecture_decision OUT symbol ENFORCED;
DEFINE FIELD IF NOT EXISTS confidence ON explains TYPE float DEFAULT 0.0;
DEFINE FIELD IF NOT EXISTS promoted_from ON explains TYPE option<string>;

DEFINE TABLE IF NOT EXISTS resolves TYPE RELATION IN symbol OUT document ENFORCED;
DEFINE TABLE IF NOT EXISTS about TYPE RELATION IN architecture_decision OUT symbol | document | file ENFORCED;

-- Full-text search (basic analyzers)
DEFINE ANALYZER IF NOT EXISTS codebrain_analyzer
    TOKENIZERS blank, class
    FILTERS lowercase, edgengram(2,12);

DEFINE INDEX IF NOT EXISTS symbol_fts ON symbol
    FIELDS name, fqn, signature
    SEARCH ANALYZER codebrain_analyzer BM25;

DEFINE INDEX IF NOT EXISTS document_fts ON document
    FIELDS title, body, aliases
    SEARCH ANALYZER codebrain_analyzer BM25;

DEFINE INDEX IF NOT EXISTS chunk_fts ON chunk
    FIELDS text
    SEARCH ANALYZER codebrain_analyzer BM25;

-- Vector HNSW index is created at runtime to match embeddings.dimension
-- (see codebrain_db::ensure_chunk_vector_index / Phase 4).
