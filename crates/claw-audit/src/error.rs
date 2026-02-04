//! Error types for the audit logging system.

use thiserror::Error;

/// Errors that can occur during audit logging operations.
#[derive(Debug, Error)]
pub enum AuditError {
    /// A required field was missing when building an event.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Failed to serialize an event.
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid event data.
    #[error("invalid event data: {0}")]
    InvalidData(String),
}

/// Result type alias for audit operations.
pub type Result<T> = std::result::Result<T, AuditError>;
