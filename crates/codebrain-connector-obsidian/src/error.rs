use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ObsidianConnectorError>;

#[derive(Debug, Error)]
pub enum ObsidianConnectorError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid vault root: {0}")]
    InvalidRoot(PathBuf),

    #[error("invalid UTF-8 path below vault root: {0}")]
    InvalidPath(PathBuf),

    #[error("background extraction task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}
