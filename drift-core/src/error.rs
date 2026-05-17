use thiserror::Error;

/// Top-level error type for the drift domain.
#[derive(Debug, Error)]
pub enum DriftError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}
