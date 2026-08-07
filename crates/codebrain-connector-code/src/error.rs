use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CodeConnectorError>;

#[derive(Debug, Error)]
pub enum CodeConnectorError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid root path: {0}")]
    InvalidRoot(PathBuf),

    #[error("unsupported source extension for {0}")]
    UnsupportedLanguage(PathBuf),

    #[error("tree-sitter rejected the {language} grammar")]
    Grammar { language: &'static str },

    #[error("tree-sitter could not build a syntax tree for {0}")]
    Parse(PathBuf),

    #[error("background extraction task failed: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("invalid UTF-8 path below source root: {0}")]
    InvalidPath(PathBuf),
}
