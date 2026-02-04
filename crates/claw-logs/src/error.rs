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

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn error_missing_field_various() {
        let err = LogError::MissingField("id");
        assert!(err.to_string().contains("id"));

        let err = LogError::MissingField("message");
        assert!(err.to_string().contains("message"));

        let err = LogError::MissingField("level");
        assert!(err.to_string().contains("level"));
    }

    #[test]
    fn error_not_found_various_ids() {
        let err = LogError::NotFound(0);
        assert!(err.to_string().contains("0"));

        let err = LogError::NotFound(u64::MAX);
        assert!(err.to_string().contains(&u64::MAX.to_string()));
    }

    #[test]
    fn error_invalid_filter() {
        let err = LogError::InvalidFilter("time range invalid".to_string());
        assert_eq!(err.to_string(), "invalid filter: time range invalid");
    }

    #[test]
    fn error_parse_error() {
        let err = LogError::ParseError("unexpected format".to_string());
        assert_eq!(err.to_string(), "parse error: unexpected format");
    }

    #[test]
    fn error_io_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: LogError = io_err.into();
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn error_debug_format() {
        let err = LogError::CapacityExceeded;
        let debug = format!("{:?}", err);
        assert!(debug.contains("CapacityExceeded"));
    }

    #[test]
    fn error_debug_format_all_variants() {
        let errors = vec![
            LogError::MissingField("test"),
            LogError::CapacityExceeded,
            LogError::NotFound(1),
            LogError::StreamClosed,
            LogError::InvalidFilter("test".to_string()),
            LogError::ParseError("test".to_string()),
        ];

        for err in errors {
            let debug = format!("{:?}", err);
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn result_type_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn result_type_err() {
        let result: Result<i32> = Err(LogError::StreamClosed);
        assert!(result.is_err());
    }
}
