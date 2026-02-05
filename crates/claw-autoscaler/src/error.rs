//! Error types for the autoscaler system.

use thiserror::Error;

/// Result type for autoscaler operations.
pub type Result<T> = std::result::Result<T, AutoscalerError>;

/// Errors that can occur in the autoscaler system.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AutoscalerError {
    /// Invalid scaling policy configuration.
    #[error("invalid scaling policy: {reason}")]
    InvalidPolicy {
        /// Description of why the policy is invalid.
        reason: String,
    },

    /// Invalid node pool configuration.
    #[error("invalid node pool: {reason}")]
    InvalidNodePool {
        /// Description of why the pool is invalid.
        reason: String,
    },

    /// Node pool not found.
    #[error("node pool not found: {pool_id}")]
    PoolNotFound {
        /// ID of the pool that was not found.
        pool_id: String,
    },

    /// Node not found in any pool.
    #[error("node not found: {node_id}")]
    NodeNotFound {
        /// ID of the node that was not found.
        node_id: String,
    },

    /// Scaling operation not allowed.
    #[error("scaling not allowed: {reason}")]
    ScalingNotAllowed {
        /// Description of why scaling is not allowed.
        reason: String,
    },

    /// Cooldown period active.
    #[error("cooldown active: cannot scale until {cooldown_ends}")]
    CooldownActive {
        /// When the cooldown period ends.
        cooldown_ends: String,
    },

    /// Metrics error.
    #[error("metrics error: {message}")]
    MetricsError {
        /// Description of the metrics error.
        message: String,
    },

    /// Invalid schedule expression.
    #[error("invalid schedule: {reason}")]
    InvalidSchedule {
        /// Description of why the schedule is invalid.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_invalid_policy() {
        let err = AutoscalerError::InvalidPolicy {
            reason: "min > max".into(),
        };
        assert_eq!(err.to_string(), "invalid scaling policy: min > max");
    }

    #[test]
    fn error_display_pool_not_found() {
        let err = AutoscalerError::PoolNotFound {
            pool_id: "gpu-pool-1".into(),
        };
        assert_eq!(err.to_string(), "node pool not found: gpu-pool-1");
    }

    #[test]
    fn error_display_cooldown_active() {
        let err = AutoscalerError::CooldownActive {
            cooldown_ends: "2024-01-15T10:30:00Z".into(),
        };
        assert_eq!(
            err.to_string(),
            "cooldown active: cannot scale until 2024-01-15T10:30:00Z"
        );
    }

    #[test]
    fn error_clone_and_eq() {
        let err1 = AutoscalerError::NodeNotFound {
            node_id: "node-1".into(),
        };
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn error_debug_format() {
        let err = AutoscalerError::MetricsError {
            message: "connection failed".into(),
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("MetricsError"));
        assert!(debug.contains("connection failed"));
    }
}
