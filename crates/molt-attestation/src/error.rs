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

    /// Challenge has expired (too old).
    #[error("challenge expired: age {age_secs}s exceeds max {max_age_secs}s")]
    ChallengeExpired {
        /// The age of the challenge in seconds.
        age_secs: u64,
        /// The maximum allowed age in seconds.
        max_age_secs: u64,
    },

    /// Nonce has already been used (replay attack detected).
    #[error("nonce replay detected: nonce has already been used")]
    NonceReplay,

    /// Challenge was issued for a different verifier.
    #[error("challenge verifier mismatch: expected {expected}, got {actual}")]
    VerifierMismatch {
        /// The expected verifier ID.
        expected: String,
        /// The actual verifier ID in the challenge.
        actual: String,
    },

    /// Challenge is missing when required.
    #[error("challenge required but not provided")]
    ChallengeMissing,

    /// Verification rate limit exceeded.
    #[error("verification rate limit exceeded: must wait {remaining_secs}s before next verification")]
    RateLimitExceeded {
        /// Seconds remaining until next verification is allowed.
        remaining_secs: u64,
    },

    /// Verification cooldown active after failed verification.
    #[error("verification cooldown active: {remaining_secs}s remaining after failed verification")]
    CooldownActive {
        /// Seconds remaining in the cooldown period.
        remaining_secs: u64,
    },
}
