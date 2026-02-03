//! Error types for the claw-metrics crate.

use thiserror::Error;

/// Errors that can occur in the metrics system.
#[derive(Debug, Error)]
pub enum MetricsError {
    /// The metric name is invalid (empty or contains invalid characters).
    #[error("invalid metric name: {reason}")]
    InvalidMetricName {
        /// The reason the name is invalid.
        reason: String,
    },

    /// The time range is invalid (start > end or negative values).
    #[error("invalid time range: start={start}, end={end}")]
    InvalidTimeRange {
        /// Start timestamp.
        start: i64,
        /// End timestamp.
        end: i64,
    },

    /// A metric with the given name was not found.
    #[error("metric not found: {name}")]
    MetricNotFound {
        /// The metric name that was not found.
        name: String,
    },

    /// Storage operation failed.
    #[error("storage error: {reason}")]
    StorageError {
        /// The reason the storage operation failed.
        reason: String,
    },

    /// Collection operation failed.
    #[error("collection error: {reason}")]
    CollectionError {
        /// The reason the collection operation failed.
        reason: String,
    },

    /// Insufficient data for the requested operation.
    #[error("insufficient data: {reason}")]
    InsufficientData {
        /// The reason there is insufficient data.
        reason: String,
    },
}

/// Result type for metrics operations.
pub type Result<T> = std::result::Result<T, MetricsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_invalid_metric_name() {
        let err = MetricsError::InvalidMetricName {
            reason: "empty name".to_string(),
        };
        assert_eq!(err.to_string(), "invalid metric name: empty name");
    }

    #[test]
    fn error_display_invalid_time_range() {
        let err = MetricsError::InvalidTimeRange { start: 100, end: 50 };
        assert_eq!(err.to_string(), "invalid time range: start=100, end=50");
    }

    #[test]
    fn error_display_metric_not_found() {
        let err = MetricsError::MetricNotFound {
            name: "gpu_utilization".to_string(),
        };
        assert_eq!(err.to_string(), "metric not found: gpu_utilization");
    }

    #[test]
    fn error_display_storage_error() {
        let err = MetricsError::StorageError {
            reason: "lock poisoned".to_string(),
        };
        assert_eq!(err.to_string(), "storage error: lock poisoned");
    }

    #[test]
    fn error_display_collection_error() {
        let err = MetricsError::CollectionError {
            reason: "gpu not accessible".to_string(),
        };
        assert_eq!(err.to_string(), "collection error: gpu not accessible");
    }

    #[test]
    fn error_display_insufficient_data() {
        let err = MetricsError::InsufficientData {
            reason: "need at least 2 points for rate".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "insufficient data: need at least 2 points for rate"
        );
    }
}
