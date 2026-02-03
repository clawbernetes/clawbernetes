//! Query helpers for common metric operations.
//!
//! This module provides convenient functions for common metric queries
//! without needing to construct time ranges and aggregations manually.

use std::time::Duration;

use crate::error::Result;
use crate::storage::MetricStore;
use crate::types::{Aggregation, MetricName, MetricPoint, TimeRange};

/// Converts a Duration to milliseconds as i64.
/// Durations exceeding ~292 million years will be truncated.
#[allow(clippy::cast_possible_truncation)]
const fn duration_to_millis(duration: Duration) -> i64 {
    duration.as_millis() as i64
}

/// Returns the last (most recent) value for a metric.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
///
/// # Returns
///
/// Returns `Some(value)` if the metric has any data points, `None` otherwise.
#[must_use]
pub fn last_value(store: &MetricStore, name: &MetricName) -> Option<f64> {
    let range = TimeRange::last_hours(24); // Look back 24 hours for "last" value
    let points = store.query(name, range, Some(Aggregation::Last)).ok()?;
    points.first().map(|p| p.value)
}

/// Returns the average value over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(average)` if the metric has data in the range, `None` otherwise.
#[must_use]
pub fn average_over(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, Some(Aggregation::Avg)).ok()?;
    points.first().map(|p| p.value)
}

/// Returns the maximum value over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(max)` if the metric has data in the range, `None` otherwise.
#[must_use]
pub fn max_over(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, Some(Aggregation::Max)).ok()?;
    points.first().map(|p| p.value)
}

/// Returns the minimum value over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(min)` if the metric has data in the range, `None` otherwise.
#[must_use]
pub fn min_over(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, Some(Aggregation::Min)).ok()?;
    points.first().map(|p| p.value)
}

/// Returns the sum of values over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(sum)` if the metric has data in the range, `None` otherwise.
#[must_use]
pub fn sum_over(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, Some(Aggregation::Sum)).ok()?;
    points.first().map(|p| p.value)
}

/// Calculates the rate of change per second over a duration.
///
/// This is useful for counter-type metrics where you want to know
/// the rate of increase (e.g., requests per second).
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(rate)` if there are at least 2 data points, `None` otherwise.
/// The rate is calculated as `(last_value - first_value) / time_diff_seconds`.
#[must_use]
#[allow(clippy::cast_precision_loss)] // Timestamp diff precision loss acceptable for rate calculation
pub fn rate(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, None).ok()?;

    if points.len() < 2 {
        return None;
    }

    let first = points.first()?;
    let last = points.last()?;

    let time_diff_seconds = (last.timestamp - first.timestamp) as f64 / 1000.0;

    if time_diff_seconds <= 0.0 {
        return None;
    }

    Some((last.value - first.value) / time_diff_seconds)
}

/// Calculates the percentage increase over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(percentage)` if there are at least 2 data points and the first
/// value is non-zero, `None` otherwise.
#[must_use]
pub fn percentage_change(
    store: &MetricStore,
    name: &MetricName,
    duration: Duration,
) -> Option<f64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, None).ok()?;

    if points.len() < 2 {
        return None;
    }

    let first = points.first()?;
    let last = points.last()?;

    if first.value.abs() < f64::EPSILON {
        return None;
    }

    Some(((last.value - first.value) / first.value) * 100.0)
}

/// Returns the count of data points over a duration.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `duration` - The duration to look back from now
///
/// # Returns
///
/// Returns `Some(count)` if the metric exists, `None` otherwise.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Count is always a non-negative integer
pub fn count_over(store: &MetricStore, name: &MetricName, duration: Duration) -> Option<u64> {
    let range = TimeRange::last_millis(duration_to_millis(duration));
    let points = store.query(name, range, Some(Aggregation::Count)).ok()?;
    points.first().map(|p| p.value as u64)
}

/// Returns all points for a metric within a time range, with optional filtering.
///
/// # Arguments
///
/// * `store` - The metric store to query
/// * `name` - The metric name to look up
/// * `range` - The time range to query
/// * `label_filter` - Optional label key-value pair to filter by
///
/// # Errors
///
/// Returns an error if the metric doesn't exist.
pub fn points_with_filter(
    store: &MetricStore,
    name: &MetricName,
    range: TimeRange,
    label_filter: Option<(&str, &str)>,
) -> Result<Vec<MetricPoint>> {
    let points = store.query(name, range, None)?;

    match label_filter {
        Some((key, value)) => Ok(points
            .into_iter()
            .filter(|p| p.labels.get(key).is_some_and(|v| v == value))
            .collect()),
        None => Ok(points),
    }
}

/// A builder for constructing complex queries.
#[derive(Debug)]
pub struct QueryBuilder<'a> {
    store: &'a MetricStore,
    name: MetricName,
    range: Option<TimeRange>,
    aggregation: Option<Aggregation>,
    label_filters: Vec<(String, String)>,
}

impl<'a> QueryBuilder<'a> {
    /// Creates a new query builder.
    #[must_use]
    pub const fn new(store: &'a MetricStore, name: MetricName) -> Self {
        Self {
            store,
            name,
            range: None,
            aggregation: None,
            label_filters: Vec::new(),
        }
    }

    /// Sets the time range for the query.
    #[must_use]
    pub const fn range(mut self, range: TimeRange) -> Self {
        self.range = Some(range);
        self
    }

    /// Sets the query to look back a specific duration from now.
    #[must_use]
    pub fn last(mut self, duration: Duration) -> Self {
        self.range = Some(TimeRange::last_millis(duration_to_millis(duration)));
        self
    }

    /// Sets the aggregation function.
    #[must_use]
    pub const fn aggregate(mut self, agg: Aggregation) -> Self {
        self.aggregation = Some(agg);
        self
    }

    /// Adds a label filter.
    #[must_use]
    pub fn filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.label_filters.push((key.into(), value.into()));
        self
    }

    /// Executes the query and returns the matching points.
    ///
    /// # Errors
    ///
    /// Returns an error if the metric doesn't exist.
    pub fn execute(self) -> Result<Vec<MetricPoint>> {
        let range = self.range.unwrap_or_else(|| TimeRange::last_hours(1));

        let points = self.store.query(&self.name, range, None)?;

        // Apply label filters
        let filtered: Vec<MetricPoint> = points
            .into_iter()
            .filter(|p| {
                self.label_filters
                    .iter()
                    .all(|(k, v)| p.labels.get(k).is_some_and(|pv| pv == v))
            })
            .collect();

        // Apply aggregation if specified
        match self.aggregation {
            Some(agg) => {
                let values: Vec<f64> = filtered.iter().map(|p| p.value).collect();
                agg.apply(&values).map_or_else(
                    || Ok(vec![]),
                    |result| {
                        let timestamp = filtered
                            .last()
                            .map_or_else(MetricPoint::now_timestamp, |p| p.timestamp);
                        Ok(vec![MetricPoint::new(timestamp, result)])
                    },
                )
            }
            None => Ok(filtered),
        }
    }

    /// Executes the query and returns just the value (for aggregated queries).
    ///
    /// # Errors
    ///
    /// Returns an error if the metric doesn't exist.
    pub fn value(self) -> Result<Option<f64>> {
        let points = self.execute()?;
        Ok(points.first().map(|p| p.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_store() -> (MetricStore, MetricName) {
        let store = MetricStore::new(Duration::from_secs(3600));
        let name = MetricName::new("test_metric").unwrap();

        // Add some test data points with known timestamps
        let now = MetricPoint::now_timestamp();

        // Push points at different intervals
        for i in 0..10 {
            let point = MetricPoint::new(now - (10 - i) * 1000, (i + 1) as f64);
            store.push(&name, point).unwrap();
        }

        (store, name)
    }

    mod last_value_tests {
        use super::*;

        #[test]
        fn last_value_returns_most_recent() {
            let (store, name) = setup_test_store();

            let value = last_value(&store, &name);
            assert!(value.is_some());
            assert!((value.unwrap() - 10.0).abs() < f64::EPSILON);
        }

        #[test]
        fn last_value_returns_none_for_nonexistent_metric() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("nonexistent").unwrap();

            let value = last_value(&store, &name);
            assert!(value.is_none());
        }
    }

    mod average_over_tests {
        use super::*;

        #[test]
        fn average_over_calculates_correctly() {
            let (store, name) = setup_test_store();

            let avg = average_over(&store, &name, Duration::from_secs(60));
            assert!(avg.is_some());

            // Values 1-10, average should be 5.5
            assert!((avg.unwrap() - 5.5).abs() < f64::EPSILON);
        }

        #[test]
        fn average_over_returns_none_for_empty() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("empty").unwrap();

            let avg = average_over(&store, &name, Duration::from_secs(60));
            assert!(avg.is_none());
        }
    }

    mod max_over_tests {
        use super::*;

        #[test]
        fn max_over_finds_maximum() {
            let (store, name) = setup_test_store();

            let max = max_over(&store, &name, Duration::from_secs(60));
            assert!(max.is_some());
            assert!((max.unwrap() - 10.0).abs() < f64::EPSILON);
        }
    }

    mod min_over_tests {
        use super::*;

        #[test]
        fn min_over_finds_minimum() {
            let (store, name) = setup_test_store();

            let min = min_over(&store, &name, Duration::from_secs(60));
            assert!(min.is_some());
            assert!((min.unwrap() - 1.0).abs() < f64::EPSILON);
        }
    }

    mod sum_over_tests {
        use super::*;

        #[test]
        fn sum_over_calculates_correctly() {
            let (store, name) = setup_test_store();

            let sum = sum_over(&store, &name, Duration::from_secs(60));
            assert!(sum.is_some());

            // Sum of 1-10 = 55
            assert!((sum.unwrap() - 55.0).abs() < f64::EPSILON);
        }
    }

    mod rate_tests {
        use super::*;

        #[test]
        fn rate_calculates_change_per_second() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("counter").unwrap();

            let now = MetricPoint::now_timestamp();

            // Push two points 10 seconds apart
            store
                .push(&name, MetricPoint::new(now - 10_000, 100.0))
                .unwrap();
            store.push(&name, MetricPoint::new(now, 200.0)).unwrap();

            let r = rate(&store, &name, Duration::from_secs(60));
            assert!(r.is_some());

            // (200 - 100) / 10 seconds = 10 per second
            assert!((r.unwrap() - 10.0).abs() < 0.1);
        }

        #[test]
        fn rate_returns_none_for_single_point() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("single").unwrap();

            store.push(&name, MetricPoint::now(100.0)).unwrap();

            let r = rate(&store, &name, Duration::from_secs(60));
            assert!(r.is_none());
        }

        #[test]
        fn rate_returns_none_for_nonexistent() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("nonexistent").unwrap();

            let r = rate(&store, &name, Duration::from_secs(60));
            assert!(r.is_none());
        }
    }

    mod percentage_change_tests {
        use super::*;

        #[test]
        fn percentage_change_calculates_correctly() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("growth").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(&name, MetricPoint::new(now - 10_000, 100.0))
                .unwrap();
            store.push(&name, MetricPoint::new(now, 150.0)).unwrap();

            let pct = percentage_change(&store, &name, Duration::from_secs(60));
            assert!(pct.is_some());

            // (150 - 100) / 100 * 100 = 50%
            assert!((pct.unwrap() - 50.0).abs() < f64::EPSILON);
        }

        #[test]
        fn percentage_change_handles_decrease() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("decrease").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(&name, MetricPoint::new(now - 10_000, 100.0))
                .unwrap();
            store.push(&name, MetricPoint::new(now, 75.0)).unwrap();

            let pct = percentage_change(&store, &name, Duration::from_secs(60));
            assert!(pct.is_some());

            // (75 - 100) / 100 * 100 = -25%
            assert!((pct.unwrap() - (-25.0)).abs() < f64::EPSILON);
        }

        #[test]
        fn percentage_change_returns_none_for_zero_start() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("zero_start").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(&name, MetricPoint::new(now - 10_000, 0.0))
                .unwrap();
            store.push(&name, MetricPoint::new(now, 100.0)).unwrap();

            let pct = percentage_change(&store, &name, Duration::from_secs(60));
            assert!(pct.is_none());
        }
    }

    mod count_over_tests {
        use super::*;

        #[test]
        fn count_over_returns_point_count() {
            let (store, name) = setup_test_store();

            let count = count_over(&store, &name, Duration::from_secs(60));
            assert!(count.is_some());
            assert_eq!(count.unwrap(), 10);
        }
    }

    mod points_with_filter_tests {
        use super::*;

        #[test]
        fn filter_by_label() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("labeled").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(
                    &name,
                    MetricPoint::new(now - 2000, 1.0).label("gpu_id", "0"),
                )
                .unwrap();
            store
                .push(
                    &name,
                    MetricPoint::new(now - 1000, 2.0).label("gpu_id", "1"),
                )
                .unwrap();
            store
                .push(&name, MetricPoint::new(now, 3.0).label("gpu_id", "0"))
                .unwrap();

            let range = TimeRange::last_seconds(60);

            // Filter for gpu_id=0
            let points = points_with_filter(&store, &name, range, Some(("gpu_id", "0"))).unwrap();
            assert_eq!(points.len(), 2);
            assert!((points[0].value - 1.0).abs() < f64::EPSILON);
            assert!((points[1].value - 3.0).abs() < f64::EPSILON);

            // Filter for gpu_id=1
            let range = TimeRange::last_seconds(60);
            let points = points_with_filter(&store, &name, range, Some(("gpu_id", "1"))).unwrap();
            assert_eq!(points.len(), 1);
            assert!((points[0].value - 2.0).abs() < f64::EPSILON);
        }

        #[test]
        fn no_filter_returns_all() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("all").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(&name, MetricPoint::new(now - 1000, 1.0))
                .unwrap();
            store.push(&name, MetricPoint::new(now, 2.0)).unwrap();

            let range = TimeRange::last_seconds(60);
            let points = points_with_filter(&store, &name, range, None).unwrap();
            assert_eq!(points.len(), 2);
        }

        #[test]
        fn filter_nonexistent_label() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("no_labels").unwrap();

            store.push(&name, MetricPoint::now(1.0)).unwrap();

            let range = TimeRange::last_seconds(60);
            let points =
                points_with_filter(&store, &name, range, Some(("nonexistent", "value"))).unwrap();
            assert!(points.is_empty());
        }
    }

    mod query_builder_tests {
        use super::*;

        #[test]
        fn query_builder_basic() {
            let (store, name) = setup_test_store();

            let points = QueryBuilder::new(&store, name)
                .last(Duration::from_secs(60))
                .execute()
                .unwrap();

            assert_eq!(points.len(), 10);
        }

        #[test]
        fn query_builder_with_aggregation() {
            let (store, name) = setup_test_store();

            let points = QueryBuilder::new(&store, name)
                .last(Duration::from_secs(60))
                .aggregate(Aggregation::Max)
                .execute()
                .unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 10.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_builder_with_label_filter() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("filtered").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(
                    &name,
                    MetricPoint::new(now - 2000, 1.0).label("env", "prod"),
                )
                .unwrap();
            store
                .push(
                    &name,
                    MetricPoint::new(now - 1000, 2.0).label("env", "dev"),
                )
                .unwrap();
            store
                .push(&name, MetricPoint::new(now, 3.0).label("env", "prod"))
                .unwrap();

            let points = QueryBuilder::new(&store, name)
                .last(Duration::from_secs(60))
                .filter("env", "prod")
                .execute()
                .unwrap();

            assert_eq!(points.len(), 2);
        }

        #[test]
        fn query_builder_value_helper() {
            let (store, name) = setup_test_store();

            let value = QueryBuilder::new(&store, name)
                .last(Duration::from_secs(60))
                .aggregate(Aggregation::Avg)
                .value()
                .unwrap();

            assert!(value.is_some());
            assert!((value.unwrap() - 5.5).abs() < f64::EPSILON);
        }

        #[test]
        fn query_builder_chained_filters() {
            let store = MetricStore::new(Duration::from_secs(3600));
            let name = MetricName::new("multi_filter").unwrap();

            let now = MetricPoint::now_timestamp();

            store
                .push(
                    &name,
                    MetricPoint::new(now - 2000, 1.0)
                        .label("env", "prod")
                        .label("region", "us"),
                )
                .unwrap();
            store
                .push(
                    &name,
                    MetricPoint::new(now - 1000, 2.0)
                        .label("env", "prod")
                        .label("region", "eu"),
                )
                .unwrap();
            store
                .push(
                    &name,
                    MetricPoint::new(now, 3.0)
                        .label("env", "dev")
                        .label("region", "us"),
                )
                .unwrap();

            let points = QueryBuilder::new(&store, name)
                .last(Duration::from_secs(60))
                .filter("env", "prod")
                .filter("region", "us")
                .execute()
                .unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 1.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_builder_with_explicit_range() {
            let (store, name) = setup_test_store();
            let now = MetricPoint::now_timestamp();

            let range = TimeRange::new(now - 5000, now).unwrap();

            let points = QueryBuilder::new(&store, name)
                .range(range)
                .execute()
                .unwrap();

            // Should get approximately half the points
            assert!(points.len() >= 4);
            assert!(points.len() <= 6);
        }
    }
}
