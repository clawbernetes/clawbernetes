//! Error types for molt-attestation.

use thiserror::Error;

/// Errors that can occur in attestation operations.
#[derive(Debug, Error)]
pub enum AttestationError {
    /// Hardware verification failed.
    #[error("hardware verification failed: {0}")]
    HardwareVerification(String),

    /// Execution proof invalid.
    #[error("execution proof invalid: {0}")]
    InvalidExecutionProof(String),

    /// Checkpoint verification failed.
    #[error("checkpoint verification failed: {0}")]
    CheckpointVerification(String),

    /// Signature verification failed.
    #[error("signature verification failed")]
    SignatureVerification,

    /// Attestation expired.
    #[error("attestation expired")]
    Expired,
}
