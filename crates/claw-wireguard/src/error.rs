//! Error types for WireGuard operations.

use thiserror::Error;

/// Result type alias for WireGuard operations.
pub type Result<T> = std::result::Result<T, WireGuardError>;

/// Errors that can occur during WireGuard operations.
#[derive(Debug, Error)]
pub enum WireGuardError {
    /// Invalid key format.
    #[error("invalid key: {0}")]
    InvalidKey(String),

    /// Invalid base64 encoding.
    #[error("invalid base64 encoding: {0}")]
    InvalidBase64(String),

    /// Invalid base58 encoding.
    #[error("invalid base58 encoding: {0}")]
    InvalidBase58(String),

    /// Invalid key length.
    #[error("invalid key length: expected 32, got {0}")]
    InvalidKeyLength(usize),

    /// Invalid CIDR notation.
    #[error("invalid CIDR: {0}")]
    InvalidCidr(String),

    /// Invalid endpoint.
    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),

    /// Invalid configuration.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Interface operation error.
    #[error("interface error: {0}")]
    InterfaceError(String),

    /// Interface already exists.
    #[error("interface already exists: {0}")]
    InterfaceExists(String),

    /// Interface not found.
    #[error("interface not found: {0}")]
    InterfaceNotFound(String),

    /// Peer already exists.
    #[error("peer already exists: {0}")]
    PeerExists(String),

    /// Peer not found.
    #[error("peer not found: {0}")]
    PeerNotFound(String),

    /// IO error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Base64 decode error.
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    /// Parse error for configuration files.
    #[error("config parse error at line {line}: {message}")]
    ParseError {
        /// Line number where the error occurred.
        line: usize,
        /// Error message.
        message: String,
    },
}
