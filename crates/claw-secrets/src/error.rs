//! Error types for the secrets management system.

use thiserror::Error;

/// Errors that can occur in the secrets management system.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid secret identifier format.
    #[error("invalid secret id: {reason}")]
    InvalidSecretId {
        /// The reason the identifier is invalid.
        reason: String,
    },

    /// Secret not found in the store.
    #[error("secret not found: {id}")]
    SecretNotFound {
        /// The identifier of the secret that was not found.
        id: String,
    },

    /// Access denied to the secret.
    #[error("access denied: {reason}")]
    AccessDenied {
        /// The reason access was denied.
        reason: String,
    },

    /// Encryption or decryption failed.
    #[error("encryption error: {reason}")]
    EncryptionError {
        /// The reason encryption failed.
        reason: String,
    },

    /// Policy validation failed.
    #[error("policy error: {reason}")]
    PolicyError {
        /// The reason policy validation failed.
        reason: String,
    },

    /// Secret has expired.
    #[error("secret expired: {id}")]
    SecretExpired {
        /// The identifier of the expired secret.
        id: String,
    },

    /// Serialization or deserialization failed.
    #[error("serialization error: {reason}")]
    SerializationError {
        /// The reason serialization failed.
        reason: String,
    },
}

/// Result type alias for secrets operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_formats_correctly() {
        let err = Error::InvalidSecretId {
            reason: "contains spaces".to_string(),
        };
        assert_eq!(err.to_string(), "invalid secret id: contains spaces");

        let err = Error::SecretNotFound {
            id: "my-secret".to_string(),
        };
        assert_eq!(err.to_string(), "secret not found: my-secret");

        let err = Error::AccessDenied {
            reason: "workload not allowed".to_string(),
        };
        assert_eq!(err.to_string(), "access denied: workload not allowed");
    }
}
