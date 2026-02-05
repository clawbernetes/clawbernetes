//! Error types for the claw-alerts crate.

use thiserror::Error;

/// Errors that can occur in the alerting system.
#[derive(Debug, Error)]
pub enum AlertError {
    /// Invalid alert rule configuration.
    #[error("invalid alert rule: {reason}")]
    InvalidRule {
        /// The reason the rule is invalid.
        reason: String,
    },

    /// Alert rule with the given name was not found.
    #[error("rule not found: {name}")]
    RuleNotFound {
        /// The rule name that was not found.
        name: String,
    },

    /// Alert with the given ID was not found.
    #[error("alert not found: {id}")]
    AlertNotFound {
        /// The alert ID that was not found.
        id: String,
    },

    /// Notification delivery failed.
    #[error("notification failed: {reason}")]
    NotificationFailed {
        /// The reason the notification failed.
        reason: String,
    },

    /// Failed to evaluate alert condition.
    #[error("condition evaluation failed: {reason}")]
    EvaluationError {
        /// The reason the evaluation failed.
        reason: String,
    },

    /// Metrics query failed.
    #[error("metrics error: {0}")]
    MetricsError(#[from] claw_metrics::MetricsError),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// Invalid duration specification.
    #[error("invalid duration: {reason}")]
    InvalidDuration {
        /// The reason the duration is invalid.
        reason: String,
    },

    /// Silence not found.
    #[error("silence not found: {id}")]
    SilenceNotFound {
        /// The silence ID that was not found.
        id: String,
    },
}

impl From<serde_json::Error> for AlertError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

/// Result type for alert operations.
pub type Result<T> = std::result::Result<T, AlertError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_invalid_rule() {
        let err = AlertError::InvalidRule {
            reason: "empty name".to_string(),
        };
        assert_eq!(err.to_string(), "invalid alert rule: empty name");
    }

    #[test]
    fn error_display_rule_not_found() {
        let err = AlertError::RuleNotFound {
            name: "high_cpu".to_string(),
        };
        assert_eq!(err.to_string(), "rule not found: high_cpu");
    }

    #[test]
    fn error_display_alert_not_found() {
        let err = AlertError::AlertNotFound {
            id: "abc-123".to_string(),
        };
        assert_eq!(err.to_string(), "alert not found: abc-123");
    }

    #[test]
    fn error_display_notification_failed() {
        let err = AlertError::NotificationFailed {
            reason: "connection refused".to_string(),
        };
        assert_eq!(err.to_string(), "notification failed: connection refused");
    }

    #[test]
    fn error_display_evaluation_error() {
        let err = AlertError::EvaluationError {
            reason: "metric not found".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "condition evaluation failed: metric not found"
        );
    }

    #[test]
    fn error_display_invalid_duration() {
        let err = AlertError::InvalidDuration {
            reason: "negative duration".to_string(),
        };
        assert_eq!(err.to_string(), "invalid duration: negative duration");
    }

    #[test]
    fn error_display_silence_not_found() {
        let err = AlertError::SilenceNotFound {
            id: "silence-123".to_string(),
        };
        assert_eq!(err.to_string(), "silence not found: silence-123");
    }

    #[test]
    fn error_from_serde_json() {
        let json_err = serde_json::from_str::<String>("invalid json");
        assert!(json_err.is_err());
        let alert_err: AlertError = json_err.unwrap_err().into();
        assert!(matches!(alert_err, AlertError::SerializationError(_)));
    }
}
