//! PKI error types.

use thiserror::Error;

/// Result type for PKI operations.
pub type Result<T> = std::result::Result<T, Error>;

/// PKI error variants.
#[derive(Debug, Error)]
pub enum Error {
    /// Certificate generation failed.
    #[error("certificate generation failed: {0}")]
    Generation(String),

    /// Certificate parsing failed.
    #[error("certificate parsing failed: {0}")]
    Parse(String),

    /// Certificate validation failed.
    #[error("certificate validation failed: {0}")]
    Validation(String),

    /// Certificate not found.
    #[error("certificate not found: {0}")]
    NotFound(String),

    /// Certificate has expired.
    #[error("certificate has expired")]
    Expired,

    /// Certificate not yet valid.
    #[error("certificate not yet valid")]
    NotYetValid,

    /// Invalid key usage.
    #[error("invalid key usage: {0}")]
    InvalidKeyUsage(String),

    /// Certificate already revoked.
    #[error("certificate already revoked: {0}")]
    AlreadyRevoked(String),

    /// Storage error.
    #[error("storage error: {0}")]
    Storage(String),

    /// Invalid certificate chain.
    #[error("invalid certificate chain: {0}")]
    InvalidChain(String),

    /// Signature verification failed.
    #[error("signature verification failed: {0}")]
    SignatureVerification(String),

    /// Subject Alternative Name error.
    #[error("SAN error: {0}")]
    San(String),
}
