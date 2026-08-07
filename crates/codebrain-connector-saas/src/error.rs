//! Errors for SaaS connectors.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, SaasError>;

#[derive(Debug, Error)]
pub enum SaasError {
    #[error("saas config error: {0}")]
    Config(String),
    #[error("saas http error: {0}")]
    Http(String),
    #[error("{0}")]
    Message(String),
}
