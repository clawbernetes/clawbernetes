//! Core types for the metrics system.
//!
//! This module provides the fundamental types used throughout the claw-metrics crate:
//! - [`MetricPoint`]: A single measurement with timestamp, value, and labels
//! - [`MetricName`]: A validated metric name
//! - [`TimeRange`]: A time range for queries
//! - [`Aggregation`]: Aggregation functions for metric queries

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{MetricsError, Result};

/// A single metric data point.
///
/// Represents a measurement at a specific point in time, with optional labels
/// for dimensional filtering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
    /// The measured value.
    pub value: f64,
    /// Optional dimensional labels (e.g., `gpu_id`, `node_id`).
    pub labels: HashMap<String, String>,
}

impl MetricPoint {
    /// Creates a new metric point with the given timestamp and value.
    #[must_use]
    pub fn new(timestamp: i64, value: f64) -> Self {
        Self {
            timestamp,
            value,
            labels: HashMap::new(),
        }
    }

    /// Creates a new metric point with the given timestamp, value, and labels.
    #[must_use]
    pub const fn with_labels(timestamp: i64, value: f64, labels: HashMap<String, String>) -> Self {
        Self {
            timestamp,
            value,
            labels,
        }
    }

    /// Adds a label to this metric point and returns self for chaining.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Returns the current timestamp in milliseconds.
    #[must_use]
    pub fn now_timestamp() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Creates a new metric point with the current timestamp.
    #[must_use]
    pub fn now(value: f64) -> Self {
        Self::new(Self::now_timestamp(), value)
    }
}

/// A validated metric name.
///
/// Metric names must:
/// - Be non-empty
/// - Contain only alphanumeric characters, underscores, and colons
/// - Start with a letter or underscore
/// - Be at most 256 characters long
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetricName(String);

impl MetricName {
    /// Maximum allowed length for a metric name.
    pub const MAX_LENGTH: usize = 256;

    /// Creates a new validated metric name.
    ///
    /// # Errors
    ///
    /// Returns `MetricsError::InvalidMetricName` if the name is invalid.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();

        if name.is_empty() {
            return Err(MetricsError::InvalidMetricName {
                reason: "metric name cannot be empty".to_string(),
            });
        }

        if name.len() > Self::MAX_LENGTH {
            return Err(MetricsError::InvalidMetricName {
                reason: format!(
                    "metric name exceeds maximum length of {} characters",
                    Self::MAX_LENGTH
                ),
            });
        }

        let first_char = name.chars().next();
        if let Some(c) = first_char {
            if !c.is_ascii_alphabetic() && c != '_' {
                return Err(MetricsError::InvalidMetricName {
                    reason: "metric name must start with a letter or underscore".to_string(),
                });
            }
        }

        for c in name.chars() {
            if !c.is_ascii_alphanumeric() && c != '_' && c != ':' {
                return Err(MetricsError::InvalidMetricName {
                    reason: format!("invalid character '{c}' in metric name"),
                });
            }
        }

        Ok(Self(name))
    }

    /// Returns the metric name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the `MetricName` and returns the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for MetricName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for MetricName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A time range for metric queries.
///
/// Both start and end are inclusive Unix timestamps in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start timestamp (inclusive), in milliseconds.
    pub start: i64,
    /// End timestamp (inclusive), in milliseconds.
    pub end: i64,
}

impl TimeRange {
    /// Creates a new time range.
    ///
    /// # Errors
    ///
    /// Returns `MetricsError::InvalidTimeRange` if start > end.
    pub const fn new(start: i64, end: i64) -> Result<Self> {
        if start > end {
            return Err(MetricsError::InvalidTimeRange { start, end });
        }
        Ok(Self { start, end })
    }

    /// Creates a time range for the last N milliseconds from now.
    #[must_use]
    pub fn last_millis(millis: i64) -> Self {
        let now = MetricPoint::now_timestamp();
        Self {
            start: now - millis,
            end: now,
        }
    }

    /// Creates a time range for the last N seconds from now.
    #[must_use]
    pub fn last_seconds(seconds: i64) -> Self {
        Self::last_millis(seconds * 1000)
    }

    /// Creates a time range for the last N minutes from now.
    #[must_use]
    pub fn last_minutes(minutes: i64) -> Self {
        Self::last_seconds(minutes * 60)
    }

    /// Creates a time range for the last N hours from now.
    #[must_use]
    pub fn last_hours(hours: i64) -> Self {
        Self::last_minutes(hours * 60)
    }

    /// Returns the duration of this time range in milliseconds.
    #[must_use]
    pub const fn duration_millis(&self) -> i64 {
        self.end - self.start
    }

    /// Checks if a timestamp falls within this range (inclusive).
    #[must_use]
    pub const fn contains(&self, timestamp: i64) -> bool {
        timestamp >= self.start && timestamp <= self.end
    }
}

/// Aggregation functions for metric queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregation {
    /// Sum of all values.
    Sum,
    /// Average (mean) of all values.
    Avg,
    /// Minimum value.
    Min,
    /// Maximum value.
    Max,
    /// Last (most recent) value.
    Last,
    /// Count of data points.
    Count,
}

impl Aggregation {
    /// Applies this aggregation to a slice of values.
    ///
    /// Returns `None` if the slice is empty.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Intentional: metrics won't have billions of points
    pub fn apply(&self, values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return None;
        }

        match self {
            Self::Sum => Some(values.iter().sum()),
            Self::Avg => Some(values.iter().sum::<f64>() / values.len() as f64),
            Self::Min => values.iter().copied().reduce(f64::min),
            Self::Max => values.iter().copied().reduce(f64::max),
            Self::Last => values.last().copied(),
            Self::Count => Some(values.len() as f64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod metric_point_tests {
        use super::*;

        #[test]
        fn create_metric_point() {
            let point = MetricPoint::new(1000, 42.5);
            assert_eq!(point.timestamp, 1000);
            assert!((point.value - 42.5).abs() < f64::EPSILON);
            assert!(point.labels.is_empty());
        }

        #[test]
        fn create_metric_point_with_labels() {
            let mut labels = HashMap::new();
            labels.insert("gpu_id".to_string(), "0".to_string());
            labels.insert("node".to_string(), "node-1".to_string());

            let point = MetricPoint::with_labels(2000, 75.0, labels);
            assert_eq!(point.timestamp, 2000);
            assert!((point.value - 75.0).abs() < f64::EPSILON);
            assert_eq!(point.labels.get("gpu_id"), Some(&"0".to_string()));
            assert_eq!(point.labels.get("node"), Some(&"node-1".to_string()));
        }

        #[test]
        fn metric_point_label_chaining() {
            let point = MetricPoint::new(3000, 80.0)
                .label("gpu_id", "1")
                .label("model", "RTX4090");

            assert_eq!(point.labels.get("gpu_id"), Some(&"1".to_string()));
            assert_eq!(point.labels.get("model"), Some(&"RTX4090".to_string()));
        }

        #[test]
        fn metric_point_now_returns_reasonable_timestamp() {
            let before = MetricPoint::now_timestamp();
            let point = MetricPoint::now(100.0);
            let after = MetricPoint::now_timestamp();

            assert!(point.timestamp >= before);
            assert!(point.timestamp <= after);
        }

        #[test]
        fn metric_point_serialization_roundtrip() {
            let original = MetricPoint::new(5000, 123.456).label("test", "value");

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<MetricPoint> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }

    mod metric_name_tests {
        use super::*;

        #[test]
        fn valid_metric_name() {
            let name = MetricName::new("gpu_utilization");
            assert!(name.is_ok());
            assert_eq!(name.unwrap().as_str(), "gpu_utilization");
        }

        #[test]
        fn valid_metric_name_with_colons() {
            let name = MetricName::new("claw:gpu:utilization");
            assert!(name.is_ok());
            assert_eq!(name.unwrap().as_str(), "claw:gpu:utilization");
        }

        #[test]
        fn valid_metric_name_starting_with_underscore() {
            let name = MetricName::new("_internal_metric");
            assert!(name.is_ok());
        }

        #[test]
        fn valid_metric_name_with_numbers() {
            let name = MetricName::new("gpu0_temp_celsius");
            assert!(name.is_ok());
        }

        #[test]
        fn empty_metric_name_fails() {
            let name = MetricName::new("");
            assert!(name.is_err());
            match name {
                Err(MetricsError::InvalidMetricName { reason }) => {
                    assert!(reason.contains("empty"));
                }
                _ => panic!("expected InvalidMetricName error"),
            }
        }

        #[test]
        fn metric_name_starting_with_number_fails() {
            let name = MetricName::new("0_invalid");
            assert!(name.is_err());
            match name {
                Err(MetricsError::InvalidMetricName { reason }) => {
                    assert!(reason.contains("start with"));
                }
                _ => panic!("expected InvalidMetricName error"),
            }
        }

        #[test]
        fn metric_name_with_invalid_characters_fails() {
            let name = MetricName::new("invalid-name");
            assert!(name.is_err());

            let name = MetricName::new("invalid.name");
            assert!(name.is_err());

            let name = MetricName::new("invalid name");
            assert!(name.is_err());
        }

        #[test]
        fn metric_name_too_long_fails() {
            let long_name = "a".repeat(MetricName::MAX_LENGTH + 1);
            let name = MetricName::new(long_name);
            assert!(name.is_err());
            match name {
                Err(MetricsError::InvalidMetricName { reason }) => {
                    assert!(reason.contains("maximum length"));
                }
                _ => panic!("expected InvalidMetricName error"),
            }
        }

        #[test]
        fn metric_name_max_length_succeeds() {
            let max_name = "a".repeat(MetricName::MAX_LENGTH);
            let name = MetricName::new(max_name);
            assert!(name.is_ok());
        }

        #[test]
        fn metric_name_display() {
            let name = MetricName::new("test_metric").unwrap();
            assert_eq!(format!("{name}"), "test_metric");
        }

        #[test]
        fn metric_name_as_ref() {
            let name = MetricName::new("ref_test").unwrap();
            let s: &str = name.as_ref();
            assert_eq!(s, "ref_test");
        }

        #[test]
        fn metric_name_into_inner() {
            let name = MetricName::new("owned_test").unwrap();
            let s = name.into_inner();
            assert_eq!(s, "owned_test");
        }

        #[test]
        fn metric_name_serialization_roundtrip() {
            let original = MetricName::new("serialized_metric").unwrap();

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<MetricName> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }

        #[test]
        fn metric_name_hash_equality() {
            use std::collections::HashSet;

            let name1 = MetricName::new("test").unwrap();
            let name2 = MetricName::new("test").unwrap();
            let name3 = MetricName::new("other").unwrap();

            let mut set = HashSet::new();
            set.insert(name1.clone());

            assert!(set.contains(&name2));
            assert!(!set.contains(&name3));
        }
    }

    mod time_range_tests {
        use super::*;

        #[test]
        fn valid_time_range() {
            let range = TimeRange::new(1000, 2000);
            assert!(range.is_ok());
            let range = range.unwrap();
            assert_eq!(range.start, 1000);
            assert_eq!(range.end, 2000);
        }

        #[test]
        fn time_range_same_start_end() {
            let range = TimeRange::new(1000, 1000);
            assert!(range.is_ok());
        }

        #[test]
        fn time_range_start_greater_than_end_fails() {
            let range = TimeRange::new(2000, 1000);
            assert!(range.is_err());
            match range {
                Err(MetricsError::InvalidTimeRange { start, end }) => {
                    assert_eq!(start, 2000);
                    assert_eq!(end, 1000);
                }
                _ => panic!("expected InvalidTimeRange error"),
            }
        }

        #[test]
        fn time_range_duration() {
            let range = TimeRange::new(1000, 5000).unwrap();
            assert_eq!(range.duration_millis(), 4000);
        }

        #[test]
        fn time_range_contains() {
            let range = TimeRange::new(1000, 2000).unwrap();

            // Inside range
            assert!(range.contains(1500));

            // At boundaries (inclusive)
            assert!(range.contains(1000));
            assert!(range.contains(2000));

            // Outside range
            assert!(!range.contains(999));
            assert!(!range.contains(2001));
        }

        #[test]
        fn time_range_last_seconds() {
            let range = TimeRange::last_seconds(60);
            assert_eq!(range.duration_millis(), 60_000);
        }

        #[test]
        fn time_range_last_minutes() {
            let range = TimeRange::last_minutes(5);
            assert_eq!(range.duration_millis(), 5 * 60 * 1000);
        }

        #[test]
        fn time_range_last_hours() {
            let range = TimeRange::last_hours(1);
            assert_eq!(range.duration_millis(), 60 * 60 * 1000);
        }

        #[test]
        fn time_range_serialization_roundtrip() {
            let original = TimeRange::new(1000, 2000).unwrap();

            let json = serde_json::to_string(&original);
            assert!(json.is_ok());

            let parsed: serde_json::Result<TimeRange> = serde_json::from_str(&json.unwrap());
            assert!(parsed.is_ok());
            assert_eq!(parsed.unwrap(), original);
        }
    }

    mod aggregation_tests {
        use super::*;

        #[test]
        fn aggregation_sum() {
            let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
            let result = Aggregation::Sum.apply(&values);
            assert_eq!(result, Some(15.0));
        }

        #[test]
        fn aggregation_avg() {
            let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
            let result = Aggregation::Avg.apply(&values);
            assert_eq!(result, Some(3.0));
        }

        #[test]
        fn aggregation_min() {
            let values = vec![3.0, 1.0, 4.0, 1.5, 9.0];
            let result = Aggregation::Min.apply(&values);
            assert_eq!(result, Some(1.0));
        }

        #[test]
        fn aggregation_max() {
            let values = vec![3.0, 1.0, 4.0, 1.5, 9.0];
            let result = Aggregation::Max.apply(&values);
            assert_eq!(result, Some(9.0));
        }

        #[test]
        fn aggregation_last() {
            let values = vec![1.0, 2.0, 3.0];
            let result = Aggregation::Last.apply(&values);
            assert_eq!(result, Some(3.0));
        }

        #[test]
        fn aggregation_count() {
            let values = vec![1.0, 2.0, 3.0, 4.0];
            let result = Aggregation::Count.apply(&values);
            assert_eq!(result, Some(4.0));
        }

        #[test]
        fn aggregation_empty_slice() {
            let values: Vec<f64> = vec![];

            assert_eq!(Aggregation::Sum.apply(&values), None);
            assert_eq!(Aggregation::Avg.apply(&values), None);
            assert_eq!(Aggregation::Min.apply(&values), None);
            assert_eq!(Aggregation::Max.apply(&values), None);
            assert_eq!(Aggregation::Last.apply(&values), None);
            assert_eq!(Aggregation::Count.apply(&values), None);
        }

        #[test]
        fn aggregation_single_value() {
            let values = vec![42.0];

            assert_eq!(Aggregation::Sum.apply(&values), Some(42.0));
            assert_eq!(Aggregation::Avg.apply(&values), Some(42.0));
            assert_eq!(Aggregation::Min.apply(&values), Some(42.0));
            assert_eq!(Aggregation::Max.apply(&values), Some(42.0));
            assert_eq!(Aggregation::Last.apply(&values), Some(42.0));
            assert_eq!(Aggregation::Count.apply(&values), Some(1.0));
        }

        #[test]
        fn aggregation_serialization_roundtrip() {
            for agg in [
                Aggregation::Sum,
                Aggregation::Avg,
                Aggregation::Min,
                Aggregation::Max,
                Aggregation::Last,
                Aggregation::Count,
            ] {
                let json = serde_json::to_string(&agg);
                assert!(json.is_ok());

                let parsed: serde_json::Result<Aggregation> =
                    serde_json::from_str(&json.unwrap());
                assert!(parsed.is_ok());
                assert_eq!(parsed.unwrap(), agg);
            }
        }
    }
}
