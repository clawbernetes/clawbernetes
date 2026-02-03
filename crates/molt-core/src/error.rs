//! Error types for molt-core.

use thiserror::Error;

/// Errors that can occur in MOLT core operations.
#[derive(Debug, Error)]
pub enum MoltError {
    /// Invalid amount (overflow, underflow, or negative).
    #[error("invalid amount: {0}")]
    InvalidAmount(String),

    /// Cryptographic operation failed.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Invalid signature.
    #[error("invalid signature")]
    InvalidSignature,

    /// Wallet error.
    #[error("wallet error: {0}")]
    Wallet(String),

    /// Policy violation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),
}
