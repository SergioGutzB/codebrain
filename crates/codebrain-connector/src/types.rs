use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Origin kind stored on `source.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GitRepo,
    ObsidianVault,
    Notion,
    Confluence,
    Jira,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitRepo => "git_repo",
            Self::ObsidianVault => "obsidian_vault",
            Self::Notion => "notion",
            Self::Confluence => "confluence",
            Self::Jira => "jira",
        }
    }
}

/// Runtime context passed to connectors during an index run.
#[derive(Debug, Clone)]
pub struct IndexContext {
    pub source_name: String,
    pub root_path: std::path::PathBuf,
    pub excludes: Vec<String>,
}

/// Unit of work discovered by a connector (typically a file path or remote id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub path: String,
    pub content_hash: Option<String>,
    pub mtime: Option<DateTime<Utc>>,
}

/// Batch of extracted graph material ready for persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractBatch {
    pub files: Vec<FileNode>,
    pub symbols: Vec<SymbolNode>,
    pub documents: Vec<DocumentNode>,
    pub edges: Vec<EdgeCandidate>,
    pub chunks: Vec<ChunkNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    pub path: String,
    pub language: Option<String>,
    pub content_hash: String,
    pub mtime: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    pub file_path: String,
    pub name: String,
    pub fqn: String,
    pub kind: String,
    pub signature: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentNode {
    pub path: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub body: String,
    pub content_hash: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkNode {
    pub parent_key: String,
    pub ordinal: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeCandidate {
    pub edge_type: EdgeType,
    pub from_key: String,
    pub to_key: String,
    pub confidence: Option<f32>,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Contains,
    Defines,
    Imports,
    Calls,
    References,
    Mentions,
    Explains,
    Resolves,
    About,
}
