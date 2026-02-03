//! Error types for the logging system.

use thiserror::Error;

/// Errors that can occur in the logging system.
#[derive(Debug, Error)]
pub enum LogError {
    /// A required field was not provided.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// The log store is full and cannot accept more entries.
    #[error("log store capacity exceeded")]
    CapacityExceeded,

    /// An entry with the given ID was not found.
    #[error("log entry not found: {0}")]
    NotFound(u64),

    /// Serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The log stream was closed.
    #[error("log stream closed")]
    StreamClosed,

    /// Invalid filter configuration.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    /// Parse error for log lines.
    #[error("parse error: {0}")]
    ParseError(String),
}

/// Result type alias for log operations.
pub type Result<T> = std::result::Result<T, LogError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let err = LogError::MissingField("timestamp");
        assert_eq!(err.to_string(), "missing required field: timestamp");

        let err = LogError::CapacityExceeded;
        assert_eq!(err.to_string(), "log store capacity exceeded");

        let err = LogError::NotFound(42);
        assert_eq!(err.to_string(), "log entry not found: 42");

        let err = LogError::StreamClosed;
        assert_eq!(err.to_string(), "log stream closed");
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<LogError>();
    }
}
