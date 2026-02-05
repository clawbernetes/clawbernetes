//! Error types for the preemption system.

use thiserror::Error;

/// Result type for preemption operations.
pub type Result<T> = std::result::Result<T, PreemptionError>;

/// Errors that can occur in the preemption system.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum PreemptionError {
    /// Invalid priority class configuration.
    #[error("invalid priority class: {reason}")]
    InvalidPriorityClass {
        /// Description of why the priority class is invalid.
        reason: String,
    },

    /// Priority class not found.
    #[error("priority class not found: {name}")]
    PriorityClassNotFound {
        /// Name of the priority class that was not found.
        name: String,
    },

    /// Workload not found.
    #[error("workload not found: {workload_id}")]
    WorkloadNotFound {
        /// ID of the workload that was not found.
        workload_id: String,
    },

    /// Preemption not allowed.
    #[error("preemption not allowed: {reason}")]
    PreemptionNotAllowed {
        /// Description of why preemption is not allowed.
        reason: String,
    },

    /// Eviction failed.
    #[error("eviction failed: {reason}")]
    EvictionFailed {
        /// Description of why the eviction failed.
        reason: String,
    },

    /// Grace period exceeded without completion.
    #[error("grace period exceeded for workload {workload_id}")]
    GracePeriodExceeded {
        /// ID of the workload that exceeded the grace period.
        workload_id: String,
    },

    /// Invalid resource specification.
    #[error("invalid resource specification: {reason}")]
    InvalidResource {
        /// Description of why the resource specification is invalid.
        reason: String,
    },

    /// Insufficient resources for preemption.
    #[error("insufficient resources: need {needed}, available {available}")]
    InsufficientResources {
        /// Resources needed.
        needed: String,
        /// Resources available.
        available: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_invalid_priority_class() {
        let err = PreemptionError::InvalidPriorityClass {
            reason: "value out of range".into(),
        };
        assert_eq!(
            err.to_string(),
            "invalid priority class: value out of range"
        );
    }

    #[test]
    fn error_display_priority_class_not_found() {
        let err = PreemptionError::PriorityClassNotFound {
            name: "system-critical".into(),
        };
        assert_eq!(
            err.to_string(),
            "priority class not found: system-critical"
        );
    }

    #[test]
    fn error_display_workload_not_found() {
        let err = PreemptionError::WorkloadNotFound {
            workload_id: "job-123".into(),
        };
        assert_eq!(err.to_string(), "workload not found: job-123");
    }

    #[test]
    fn error_display_preemption_not_allowed() {
        let err = PreemptionError::PreemptionNotAllowed {
            reason: "workload has never preempt policy".into(),
        };
        assert_eq!(
            err.to_string(),
            "preemption not allowed: workload has never preempt policy"
        );
    }

    #[test]
    fn error_display_eviction_failed() {
        let err = PreemptionError::EvictionFailed {
            reason: "container not responding".into(),
        };
        assert_eq!(err.to_string(), "eviction failed: container not responding");
    }

    #[test]
    fn error_display_grace_period_exceeded() {
        let err = PreemptionError::GracePeriodExceeded {
            workload_id: "job-456".into(),
        };
        assert_eq!(
            err.to_string(),
            "grace period exceeded for workload job-456"
        );
    }

    #[test]
    fn error_display_invalid_resource() {
        let err = PreemptionError::InvalidResource {
            reason: "negative GPU count".into(),
        };
        assert_eq!(
            err.to_string(),
            "invalid resource specification: negative GPU count"
        );
    }

    #[test]
    fn error_display_insufficient_resources() {
        let err = PreemptionError::InsufficientResources {
            needed: "4 GPUs".into(),
            available: "2 GPUs".into(),
        };
        assert_eq!(
            err.to_string(),
            "insufficient resources: need 4 GPUs, available 2 GPUs"
        );
    }

    #[test]
    fn error_clone_and_eq() {
        let err1 = PreemptionError::WorkloadNotFound {
            workload_id: "job-1".into(),
        };
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn error_debug_format() {
        let err = PreemptionError::EvictionFailed {
            reason: "timeout".into(),
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("EvictionFailed"));
        assert!(debug.contains("timeout"));
    }
}
