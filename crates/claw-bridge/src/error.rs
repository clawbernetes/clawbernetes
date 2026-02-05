//! Bridge error types

use thiserror::Error;

/// Bridge error type
#[derive(Debug, Error)]
pub enum BridgeError {
    /// JSON parsing/serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Method not found
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid parameters
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Resource exhausted
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    /// Service unavailable
    #[error("Unavailable: {0}")]
    Unavailable(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl BridgeError {
    /// Get the JSON-RPC error code for this error
    pub fn code(&self) -> i32 {
        use crate::protocol::*;
        match self {
            Self::Json(_) => PARSE_ERROR,
            Self::MethodNotFound(_) => METHOD_NOT_FOUND,
            Self::InvalidParams(_) => INVALID_PARAMS,
            Self::NotFound(_) => NOT_FOUND,
            Self::PermissionDenied(_) => PERMISSION_DENIED,
            Self::ResourceExhausted(_) => RESOURCE_EXHAUSTED,
            Self::Unavailable(_) => UNAVAILABLE,
            Self::Internal(_) | Self::Io(_) => INTERNAL_ERROR,
        }
    }
}

/// Bridge result type
pub type BridgeResult<T> = Result<T, BridgeError>;
