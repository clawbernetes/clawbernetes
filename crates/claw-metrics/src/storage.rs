//! In-memory metric storage with retention policies.
//!
//! This module provides the [`MetricStore`] which stores metric data points
//! in memory with automatic expiry based on retention duration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tracing::debug;

use crate::error::{MetricsError, Result};
use crate::types::{Aggregation, MetricName, MetricPoint, TimeRange};

/// Thread-safe in-memory storage for metrics.
///
/// The store automatically expires data older than the configured retention period.
/// All operations are thread-safe and optimized for concurrent access.
#[derive(Debug)]
pub struct MetricStore {
    /// The retention duration for metrics (in milliseconds).
    retention_millis: i64,
    /// The actual data storage, keyed by metric name.
    data: Arc<RwLock<HashMap<MetricName, Vec<MetricPoint>>>>,
}

impl MetricStore {
    /// Creates a new metric store with the given retention duration.
    ///
    /// Data points older than the retention duration will be automatically
    /// removed during query and push operations.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // Retention durations won't exceed i64::MAX ms (~292M years)
    pub fn new(retention: Duration) -> Self {
        Self {
            retention_millis: retention.as_millis() as i64,
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns the retention duration in milliseconds.
    #[must_use]
    pub const fn retention_millis(&self) -> i64 {
        self.retention_millis
    }

    /// Pushes a new metric point to the store.
    ///
    /// The point will be inserted in timestamp order and old data will be
    /// expired according to the retention policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    #[allow(clippy::significant_drop_tightening)] // Lock needed for multi-step atomic operation
    pub fn push(&self, name: &MetricName, point: MetricPoint) -> Result<()> {
        let cutoff = MetricPoint::now_timestamp() - self.retention_millis;

        let mut data = self.data.write();
        let points = data.entry(name.clone()).or_default();

        // Remove expired points
        points.retain(|p| p.timestamp >= cutoff);

        // Insert new point maintaining timestamp order
        let insert_pos = points
            .binary_search_by_key(&point.timestamp, |p| p.timestamp)
            .unwrap_or_else(|pos| pos);
        points.insert(insert_pos, point);

        debug!(
            metric = %name,
            points_count = points.len(),
            "pushed metric point"
        );

        Ok(())
    }

    /// Pushes multiple metric points in a batch.
    ///
    /// This is more efficient than calling `push` multiple times.
    ///
    /// # Errors
    ///
    /// Returns an error if any storage operation fails.
    #[allow(clippy::significant_drop_tightening)] // Lock needed for batch atomic operation
    pub fn push_batch(&self, metrics: Vec<(MetricName, MetricPoint)>) -> Result<()> {
        let cutoff = MetricPoint::now_timestamp() - self.retention_millis;

        let mut data = self.data.write();

        for (name, point) in metrics {
            let points = data.entry(name).or_default();

            // Remove expired points
            points.retain(|p| p.timestamp >= cutoff);

            // Insert new point maintaining timestamp order
            let insert_pos = points
                .binary_search_by_key(&point.timestamp, |p| p.timestamp)
                .unwrap_or_else(|pos| pos);
            points.insert(insert_pos, point);
        }

        Ok(())
    }

    /// Queries metric points within the given time range.
    ///
    /// If an aggregation is specified, the returned vector will contain
    /// a single point with the aggregated value.
    ///
    /// # Errors
    ///
    /// Returns `MetricsError::MetricNotFound` if the metric doesn't exist.
    #[allow(clippy::significant_drop_tightening)] // Lock scope is intentional for consistency
    pub fn query(
        &self,
        name: &MetricName,
        range: TimeRange,
        aggregation: Option<Aggregation>,
    ) -> Result<Vec<MetricPoint>> {
        let data = self.data.read();

        let points = data.get(name).ok_or_else(|| MetricsError::MetricNotFound {
            name: name.to_string(),
        })?;

        // Filter points within the time range
        let filtered: Vec<&MetricPoint> = points
            .iter()
            .filter(|p| range.contains(p.timestamp))
            .collect();

        match aggregation {
            Some(agg) => {
                let values: Vec<f64> = filtered.iter().map(|p| p.value).collect();

                agg.apply(&values).map_or_else(
                    || Ok(vec![]),
                    |result| {
                        // Use the latest timestamp for the aggregated result
                        let timestamp = filtered
                            .last()
                            .map_or_else(MetricPoint::now_timestamp, |p| p.timestamp);

                        Ok(vec![MetricPoint::new(timestamp, result)])
                    },
                )
            }
            None => Ok(filtered.into_iter().cloned().collect()),
        }
    }

    /// Returns a list of all metric names in the store.
    #[must_use]
    pub fn metrics_list(&self) -> Vec<MetricName> {
        let data = self.data.read();
        data.keys().cloned().collect()
    }

    /// Returns the number of data points for a given metric.
    ///
    /// Returns 0 if the metric doesn't exist.
    #[must_use]
    pub fn metric_count(&self, name: &MetricName) -> usize {
        let data = self.data.read();
        data.get(name).map_or(0, Vec::len)
    }

    /// Removes all data points for a given metric.
    ///
    /// Returns `true` if the metric existed and was removed.
    #[must_use]
    pub fn remove_metric(&self, name: &MetricName) -> bool {
        let mut data = self.data.write();
        data.remove(name).is_some()
    }

    /// Clears all metrics from the store.
    pub fn clear(&self) {
        let mut data = self.data.write();
        data.clear();
    }

    /// Manually triggers expiry of old data across all metrics.
    ///
    /// This is automatically done during push operations, but can be
    /// called manually to free up memory.
    pub fn expire_old_data(&self) {
        let cutoff = MetricPoint::now_timestamp() - self.retention_millis;

        let mut data = self.data.write();

        for points in data.values_mut() {
            points.retain(|p| p.timestamp >= cutoff);
        }

        // Remove empty metric series
        data.retain(|_, v| !v.is_empty());
    }
}

impl Clone for MetricStore {
    fn clone(&self) -> Self {
        Self {
            retention_millis: self.retention_millis,
            data: Arc::clone(&self.data),
        }
    }
}

impl Default for MetricStore {
    fn default() -> Self {
        // Default retention of 1 hour
        Self::new(Duration::from_secs(3600))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> MetricStore {
        MetricStore::new(Duration::from_secs(3600)) // 1 hour retention
    }

    fn test_metric_name() -> MetricName {
        MetricName::new("test_metric").unwrap()
    }

    /// Returns a recent timestamp offset by the given milliseconds from now.
    fn recent_ts(offset_ms: i64) -> i64 {
        MetricPoint::now_timestamp() - offset_ms
    }

    mod store_creation_tests {
        use super::*;

        #[test]
        fn create_store_with_retention() {
            let store = MetricStore::new(Duration::from_secs(3600));
            assert_eq!(store.retention_millis(), 3_600_000);
        }

        #[test]
        fn default_store_has_one_hour_retention() {
            let store = MetricStore::default();
            assert_eq!(store.retention_millis(), 3_600_000);
        }

        #[test]
        fn store_is_cloneable() {
            let store1 = test_store();
            let name = test_metric_name();

            store1
                .push(&name, MetricPoint::new(recent_ts(1000), 42.0))
                .unwrap();

            let store2 = store1.clone();

            // Both stores should see the same data
            let range = TimeRange::last_hours(1);
            let points1 = store1.query(&name, range, None).unwrap();
            let range = TimeRange::last_hours(1);
            let points2 = store2.query(&name, range, None).unwrap();

            assert_eq!(points1.len(), points2.len());
        }

        #[test]
        fn cloned_store_shares_data() {
            let store1 = test_store();
            let store2 = store1.clone();
            let name = test_metric_name();

            // Push to store1
            store1
                .push(&name, MetricPoint::new(recent_ts(2000), 42.0))
                .unwrap();

            // Store2 should see the data
            assert_eq!(store2.metric_count(&name), 1);

            // Push to store2
            store2
                .push(&name, MetricPoint::new(recent_ts(1000), 43.0))
                .unwrap();

            // Store1 should see both points
            assert_eq!(store1.metric_count(&name), 2);
        }
    }

    mod push_tests {
        use super::*;

        #[test]
        fn push_single_point() {
            let store = test_store();
            let name = test_metric_name();

            let result = store.push(&name, MetricPoint::now(42.0));
            assert!(result.is_ok());
            assert_eq!(store.metric_count(&name), 1);
        }

        #[test]
        fn push_multiple_points() {
            let store = test_store();
            let name = test_metric_name();

            store
                .push(&name, MetricPoint::new(recent_ts(3000), 42.0))
                .unwrap();
            store
                .push(&name, MetricPoint::new(recent_ts(2000), 43.0))
                .unwrap();
            store
                .push(&name, MetricPoint::new(recent_ts(1000), 44.0))
                .unwrap();

            assert_eq!(store.metric_count(&name), 3);
        }

        #[test]
        fn push_maintains_timestamp_order() {
            let store = test_store();
            let name = test_metric_name();

            let ts1 = recent_ts(3000);
            let ts2 = recent_ts(2000);
            let ts3 = recent_ts(1000);

            // Push out of order
            store
                .push(&name, MetricPoint::new(ts3, 44.0))
                .unwrap();
            store
                .push(&name, MetricPoint::new(ts1, 42.0))
                .unwrap();
            store
                .push(&name, MetricPoint::new(ts2, 43.0))
                .unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 3);
            assert_eq!(points[0].timestamp, ts1);
            assert_eq!(points[1].timestamp, ts2);
            assert_eq!(points[2].timestamp, ts3);
        }

        #[test]
        fn push_with_labels() {
            let store = test_store();
            let name = test_metric_name();

            let point = MetricPoint::now(85.0)
                .label("gpu_id", "0")
                .label("node", "node-1");

            store.push(&name, point).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 1);
            assert_eq!(points[0].labels.get("gpu_id"), Some(&"0".to_string()));
        }

        #[test]
        fn push_to_multiple_metrics() {
            let store = test_store();
            let name1 = MetricName::new("metric_a").unwrap();
            let name2 = MetricName::new("metric_b").unwrap();

            store.push(&name1, MetricPoint::now(1.0)).unwrap();
            store.push(&name2, MetricPoint::now(2.0)).unwrap();

            assert_eq!(store.metric_count(&name1), 1);
            assert_eq!(store.metric_count(&name2), 1);
        }
    }

    mod push_batch_tests {
        use super::*;

        #[test]
        fn push_batch_multiple_points() {
            let store = test_store();
            let name = test_metric_name();

            let metrics = vec![
                (name.clone(), MetricPoint::new(recent_ts(3000), 1.0)),
                (name.clone(), MetricPoint::new(recent_ts(2000), 2.0)),
                (name.clone(), MetricPoint::new(recent_ts(1000), 3.0)),
            ];

            let result = store.push_batch(metrics);
            assert!(result.is_ok());
            assert_eq!(store.metric_count(&name), 3);
        }

        #[test]
        fn push_batch_multiple_metrics() {
            let store = test_store();
            let name1 = MetricName::new("metric_a").unwrap();
            let name2 = MetricName::new("metric_b").unwrap();

            let metrics = vec![
                (name1.clone(), MetricPoint::new(recent_ts(3000), 1.0)),
                (name2.clone(), MetricPoint::new(recent_ts(2000), 2.0)),
                (name1.clone(), MetricPoint::new(recent_ts(1000), 3.0)),
            ];

            store.push_batch(metrics).unwrap();

            assert_eq!(store.metric_count(&name1), 2);
            assert_eq!(store.metric_count(&name2), 1);
        }
    }

    mod query_tests {
        use super::*;

        #[test]
        fn query_empty_metric() {
            let store = test_store();
            let name = test_metric_name();

            let range = TimeRange::last_hours(1);
            let result = store.query(&name, range, None);

            assert!(result.is_err());
            match result {
                Err(MetricsError::MetricNotFound { .. }) => {}
                _ => panic!("expected MetricNotFound error"),
            }
        }

        #[test]
        fn query_all_points() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 1.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 2.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 3.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 3);
        }

        #[test]
        fn query_with_time_filter() {
            let store = test_store();
            let name = test_metric_name();

            let ts1 = recent_ts(4000);
            let ts2 = recent_ts(3000);
            let ts3 = recent_ts(2000);
            let ts4 = recent_ts(1000);

            store.push(&name, MetricPoint::new(ts1, 1.0)).unwrap();
            store.push(&name, MetricPoint::new(ts2, 2.0)).unwrap();
            store.push(&name, MetricPoint::new(ts3, 3.0)).unwrap();
            store.push(&name, MetricPoint::new(ts4, 4.0)).unwrap();

            // Query only middle points
            let range = TimeRange::new(ts2, ts3).unwrap();
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 2);
            assert_eq!(points[0].timestamp, ts2);
            assert_eq!(points[1].timestamp, ts3);
        }

        #[test]
        fn query_with_aggregation_sum() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 1.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 2.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 3.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, Some(Aggregation::Sum)).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 6.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_with_aggregation_avg() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 10.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 20.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 30.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, Some(Aggregation::Avg)).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 20.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_with_aggregation_min() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 10.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 5.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 15.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, Some(Aggregation::Min)).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 5.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_with_aggregation_max() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 10.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 25.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 15.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, Some(Aggregation::Max)).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 25.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_with_aggregation_last() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::new(recent_ts(3000), 10.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(2000), 20.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(1000), 30.0)).unwrap();

            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, Some(Aggregation::Last)).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 30.0).abs() < f64::EPSILON);
        }

        #[test]
        fn query_no_points_in_range() {
            let store = test_store();
            let name = test_metric_name();

            // Push points in the past hour
            store.push(&name, MetricPoint::new(recent_ts(60000), 1.0)).unwrap();
            store.push(&name, MetricPoint::new(recent_ts(50000), 2.0)).unwrap();

            // Query a range that has no data (far future)
            let future = MetricPoint::now_timestamp() + 100_000;
            let range = TimeRange::new(future, future + 10000).unwrap();
            let points = store.query(&name, range, None).unwrap();

            assert!(points.is_empty());
        }

        #[test]
        fn query_aggregation_empty_range() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::now(1.0)).unwrap();

            // Query a range that has no data
            let future = MetricPoint::now_timestamp() + 100_000;
            let range = TimeRange::new(future, future + 10000).unwrap();
            let points = store.query(&name, range, Some(Aggregation::Sum)).unwrap();

            assert!(points.is_empty());
        }
    }

    mod metrics_list_tests {
        use super::*;

        #[test]
        fn empty_store_returns_empty_list() {
            let store = test_store();
            let list = store.metrics_list();
            assert!(list.is_empty());
        }

        #[test]
        fn metrics_list_returns_all_metrics() {
            let store = test_store();

            let name1 = MetricName::new("metric_a").unwrap();
            let name2 = MetricName::new("metric_b").unwrap();
            let name3 = MetricName::new("metric_c").unwrap();

            store.push(&name1, MetricPoint::now(1.0)).unwrap();
            store.push(&name2, MetricPoint::now(2.0)).unwrap();
            store.push(&name3, MetricPoint::now(3.0)).unwrap();

            let mut list = store.metrics_list();
            list.sort_by(|a, b| a.as_str().cmp(b.as_str()));

            assert_eq!(list.len(), 3);
            assert_eq!(list[0].as_str(), "metric_a");
            assert_eq!(list[1].as_str(), "metric_b");
            assert_eq!(list[2].as_str(), "metric_c");
        }
    }

    mod removal_tests {
        use super::*;

        #[test]
        fn remove_existing_metric() {
            let store = test_store();
            let name = test_metric_name();

            store.push(&name, MetricPoint::now(1.0)).unwrap();
            assert_eq!(store.metric_count(&name), 1);

            let removed = store.remove_metric(&name);
            assert!(removed);
            assert_eq!(store.metric_count(&name), 0);
        }

        #[test]
        fn remove_nonexistent_metric() {
            let store = test_store();
            let name = test_metric_name();

            let removed = store.remove_metric(&name);
            assert!(!removed);
        }

        #[test]
        fn clear_removes_all_metrics() {
            let store = test_store();

            let name1 = MetricName::new("metric_a").unwrap();
            let name2 = MetricName::new("metric_b").unwrap();

            store.push(&name1, MetricPoint::now(1.0)).unwrap();
            store.push(&name2, MetricPoint::now(2.0)).unwrap();

            store.clear();

            assert!(store.metrics_list().is_empty());
        }
    }

    mod expiry_tests {
        use super::*;

        #[test]
        fn expire_old_data_removes_old_points() {
            // Create store with very short retention
            let store = MetricStore::new(Duration::from_millis(100));
            let name = test_metric_name();

            // Push old data
            let old_timestamp = MetricPoint::now_timestamp() - 1000; // 1 second ago
            store
                .push(&name, MetricPoint::new(old_timestamp, 1.0))
                .unwrap();

            // Push recent data
            store.push(&name, MetricPoint::now(2.0)).unwrap();

            store.expire_old_data();

            // Old point should be expired
            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 1);
            assert!((points[0].value - 2.0).abs() < f64::EPSILON);
        }

        #[test]
        fn push_triggers_expiry() {
            // Create store with very short retention
            let store = MetricStore::new(Duration::from_millis(100));
            let name = test_metric_name();

            // Push old data
            let old_timestamp = MetricPoint::now_timestamp() - 1000;
            store
                .push(&name, MetricPoint::new(old_timestamp, 1.0))
                .unwrap();

            // Push new data (should trigger expiry)
            store.push(&name, MetricPoint::now(2.0)).unwrap();

            // Old point should be expired
            let range = TimeRange::last_hours(1);
            let points = store.query(&name, range, None).unwrap();

            assert_eq!(points.len(), 1);
        }
    }

    mod concurrent_tests {
        use super::*;
        use std::thread;

        #[test]
        fn concurrent_push() {
            let store = test_store();
            let name = MetricName::new("concurrent_metric").unwrap();
            let base_ts = MetricPoint::now_timestamp();

            let mut handles = vec![];

            for i in 0..10 {
                let store_clone = store.clone();
                let name_clone = name.clone();

                let handle = thread::spawn(move || {
                    for j in 0..100 {
                        // Use timestamps relative to now
                        let timestamp = base_ts - 60_000 + (i * 1000 + j) as i64;
                        let value = (i * 100 + j) as f64;
                        store_clone
                            .push(&name_clone, MetricPoint::new(timestamp, value))
                            .unwrap();
                    }
                });

                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            assert_eq!(store.metric_count(&name), 1000);
        }

        #[test]
        fn concurrent_read_write() {
            let store = test_store();
            let name = MetricName::new("rw_metric").unwrap();
            let base_ts = MetricPoint::now_timestamp();

            // Pre-populate with some data
            for i in 0..100 {
                let ts = base_ts - 120_000 + i * 100;
                store
                    .push(&name, MetricPoint::new(ts, i as f64))
                    .unwrap();
            }

            let mut handles = vec![];

            // Writers
            for i in 0..5 {
                let store_clone = store.clone();
                let name_clone = name.clone();

                let handle = thread::spawn(move || {
                    for j in 0..100 {
                        let timestamp = base_ts - 60_000 + (i * 1000 + j) as i64;
                        store_clone
                            .push(&name_clone, MetricPoint::new(timestamp, timestamp as f64))
                            .unwrap();
                    }
                });

                handles.push(handle);
            }

            // Readers
            for _ in 0..5 {
                let store_clone = store.clone();
                let name_clone = name.clone();

                let handle = thread::spawn(move || {
                    for _ in 0..100 {
                        let range = TimeRange::last_hours(1);
                        let _ = store_clone.query(&name_clone, range, None);
                    }
                });

                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }

            // Should have all points (100 initial + 500 from writers)
            assert_eq!(store.metric_count(&name), 600);
        }
    }
}
