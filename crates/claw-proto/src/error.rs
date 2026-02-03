//! Error types for the claw-proto crate.

use thiserror::Error;

/// Errors that can occur during protocol operations.
#[derive(Debug, Error)]
pub enum ProtoError {
    /// Failed to encode a message.
    #[error("encoding error: {0}")]
    Encoding(String),

    /// Failed to decode a message.
    #[error("decoding error: {0}")]
    Decoding(String),

    /// Invalid message type.
    #[error("invalid message type: {0}")]
    InvalidMessageType(u32),

    /// Missing required field.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// Validation error.
    #[error("validation error: {0}")]
    Validation(String),
}
