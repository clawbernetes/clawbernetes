//! Error types for WireGuard key operations.

use thiserror::Error;

/// Errors that can occur during WireGuard key operations.
#[derive(Debug, Error)]
pub enum WireGuardError {
    /// Invalid key format.
    #[error("invalid key: {0}")]
    InvalidKey(String),

    /// Invalid base64 encoding.
    #[error("invalid base64 encoding: {0}")]
    InvalidBase64(String),

    /// Invalid key length.
    #[error("invalid key length: expected 32, got {0}")]
    InvalidKeyLength(usize),
}
