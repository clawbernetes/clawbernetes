//! Error types for the authentication and authorization system.

use thiserror::Error;

/// Errors that can occur in the authentication and authorization system.
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid API key format.
    #[error("invalid api key: {reason}")]
    InvalidApiKey {
        /// The reason the API key is invalid.
        reason: String,
    },

    /// API key not found.
    #[error("api key not found: {id}")]
    ApiKeyNotFound {
        /// The ID of the API key that was not found.
        id: String,
    },

    /// API key has expired.
    #[error("api key expired: {id}")]
    ApiKeyExpired {
        /// The ID of the expired API key.
        id: String,
    },

    /// API key has been revoked.
    #[error("api key revoked: {id}")]
    ApiKeyRevoked {
        /// The ID of the revoked API key.
        id: String,
    },

    /// Invalid user identifier.
    #[error("invalid user id: {reason}")]
    InvalidUserId {
        /// The reason the user ID is invalid.
        reason: String,
    },

    /// User not found.
    #[error("user not found: {id}")]
    UserNotFound {
        /// The ID of the user that was not found.
        id: String,
    },

    /// Invalid role name.
    #[error("invalid role: {reason}")]
    InvalidRole {
        /// The reason the role is invalid.
        reason: String,
    },

    /// Role not found.
    #[error("role not found: {name}")]
    RoleNotFound {
        /// The name of the role that was not found.
        name: String,
    },

    /// Permission denied.
    #[error("permission denied: {reason}")]
    PermissionDenied {
        /// The reason permission was denied.
        reason: String,
    },

    /// Invalid permission format.
    #[error("invalid permission: {reason}")]
    InvalidPermission {
        /// The reason the permission is invalid.
        reason: String,
    },

    /// JWT token error.
    #[error("jwt error: {reason}")]
    JwtError {
        /// The reason the JWT operation failed.
        reason: String,
    },

    /// Token has expired.
    #[error("token expired")]
    TokenExpired,

    /// Invalid token.
    #[error("invalid token: {reason}")]
    InvalidToken {
        /// The reason the token is invalid.
        reason: String,
    },

    /// Authentication required.
    #[error("authentication required")]
    AuthenticationRequired,

    /// Insufficient scope.
    #[error("insufficient scope: required {required}, got {actual}")]
    InsufficientScope {
        /// The required scope.
        required: String,
        /// The actual scope.
        actual: String,
    },

    /// Serialization error.
    #[error("serialization error: {reason}")]
    SerializationError {
        /// The reason serialization failed.
        reason: String,
    },

    /// Crypto error.
    #[error("crypto error: {reason}")]
    CryptoError {
        /// The reason the crypto operation failed.
        reason: String,
    },
}

/// Result type alias for auth operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_formats_correctly() {
        let err = Error::InvalidApiKey {
            reason: "too short".to_string(),
        };
        assert_eq!(err.to_string(), "invalid api key: too short");

        let err = Error::ApiKeyNotFound {
            id: "key-123".to_string(),
        };
        assert_eq!(err.to_string(), "api key not found: key-123");

        let err = Error::PermissionDenied {
            reason: "user lacks admin role".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "permission denied: user lacks admin role"
        );
    }

    #[test]
    fn error_display_jwt_errors() {
        let err = Error::JwtError {
            reason: "invalid signature".to_string(),
        };
        assert_eq!(err.to_string(), "jwt error: invalid signature");

        let err = Error::TokenExpired;
        assert_eq!(err.to_string(), "token expired");

        let err = Error::InvalidToken {
            reason: "malformed header".to_string(),
        };
        assert_eq!(err.to_string(), "invalid token: malformed header");
    }

    #[test]
    fn error_display_insufficient_scope() {
        let err = Error::InsufficientScope {
            required: "admin:write".to_string(),
            actual: "admin:read".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "insufficient scope: required admin:write, got admin:read"
        );
    }
}
