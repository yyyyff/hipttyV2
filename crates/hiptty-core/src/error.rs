use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable error codes for CLI / agent consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    AuthRequired,
    AuthFailed,
    Network,
    Parse,
    RateLimit,
    ForumMessage,
    NotImplemented,
    InvalidInput,
    NotFound,
}

impl ErrorCode {
    pub fn retryable(self) -> bool {
        matches!(self, Self::Network | Self::RateLimit)
    }
}

#[derive(Debug, Clone, Error)]
pub enum AdapterError {
    #[error("authentication required")]
    AuthRequired,

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("rate limited: {0}")]
    RateLimit(String),

    #[error("forum message: {0}")]
    ForumMessage(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl AdapterError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::AuthRequired => ErrorCode::AuthRequired,
            Self::AuthFailed(_) => ErrorCode::AuthFailed,
            Self::Network(_) => ErrorCode::Network,
            Self::Parse(_) => ErrorCode::Parse,
            Self::RateLimit(_) => ErrorCode::RateLimit,
            Self::ForumMessage(_) => ErrorCode::ForumMessage,
            Self::NotImplemented(_) => ErrorCode::NotImplemented,
            Self::InvalidInput(_) => ErrorCode::InvalidInput,
            Self::NotFound(_) => ErrorCode::NotFound,
        }
    }
}

pub type AdapterResult<T> = Result<T, AdapterError>;
