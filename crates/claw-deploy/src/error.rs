//! Error types for the deployment system.
//!
//! This module defines all error types that can occur during deployment operations.

use thiserror::Error;

/// Result type alias for deployment operations.
pub type DeployResult<T> = Result<T, DeployError>;

/// Errors that can occur during deployment operations.
#[derive(Debug, Error)]
pub enum DeployError {
    /// Invalid deployment ID format
    #[error("invalid deployment id: {0}")]
    InvalidId(String),

    /// Invalid deployment intent
    #[error("invalid deployment intent: {0}")]
    InvalidIntent(String),

    /// Deployment not found
    #[error("deployment not found: {0}")]
    NotFound(String),

    /// Invalid state transition
    #[error("invalid state transition from {from} to {to}")]
    InvalidStateTransition {
        /// Current state
        from: String,
        /// Attempted target state
        to: String,
    },

    /// Strategy selection failed
    #[error("failed to select strategy: {0}")]
    StrategySelection(String),

    /// Execution error
    #[error("execution error: {0}")]
    Execution(String),

    /// Monitoring error
    #[error("monitoring error: {0}")]
    Monitoring(String),

    /// Parse error for intent strings
    #[error("parse error: {0}")]
    Parse(String),

    /// Health check failed
    #[error("health check failed: {0}")]
    HealthCheck(String),

    /// Rollback error
    #[error("rollback failed: {0}")]
    Rollback(String),

    /// Promotion error
    #[error("promotion failed: {0}")]
    Promotion(String),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl DeployError {
    /// Creates a not found error for a deployment ID.
    #[must_use]
    pub fn not_found(id: impl std::fmt::Display) -> Self {
        Self::NotFound(id.to_string())
    }

    /// Creates an invalid state transition error.
    #[must_use]
    pub fn invalid_transition(from: impl std::fmt::Display, to: impl std::fmt::Display) -> Self {
        Self::InvalidStateTransition {
            from: from.to_string(),
            to: to.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let err = DeployError::InvalidId("bad-uuid".to_string());
        assert_eq!(err.to_string(), "invalid deployment id: bad-uuid");

        let err = DeployError::NotFound("abc-123".to_string());
        assert_eq!(err.to_string(), "deployment not found: abc-123");

        let err = DeployError::invalid_transition("pending", "complete");
        assert_eq!(
            err.to_string(),
            "invalid state transition from pending to complete"
        );
    }

    #[test]
    fn not_found_helper() {
        let err = DeployError::not_found("test-id");
        match err {
            DeployError::NotFound(id) => assert_eq!(id, "test-id"),
            _ => panic!("Expected NotFound error"),
        }
    }
}
