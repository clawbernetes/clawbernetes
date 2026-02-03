#![allow(clippy::doc_overindented_list_items)]
//! Deployment history tracking for rollback operations.
//!
//! This module provides functionality to track deployment history,
//! enabling rollback to previous versions.

use crate::types::{DeploymentId, DeploymentSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Tracks deployment history for rollback operations.
///
/// The history maintains a bounded queue of deployment snapshots,
/// automatically evicting the oldest entries when the limit is reached.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentHistory {
    /// Maximum number of snapshots to retain.
    max_snapshots: usize,
    /// Queue of deployment snapshots (newest at back).
    snapshots: VecDeque<DeploymentSnapshot>,
}

impl DeploymentHistory {
    /// Creates a new deployment history with the specified maximum capacity.
    ///
    /// # Arguments
    ///
    /// * `max_snapshots` - Maximum number of snapshots to retain. Must be at least 1.
    ///
    /// # Returns
    ///
    /// A new `DeploymentHistory` instance, or `None` if `max_snapshots` is 0.
    #[must_use]
    pub fn new(max_snapshots: usize) -> Option<Self> {
        if max_snapshots == 0 {
            return None;
        }
        Some(Self {
            max_snapshots,
            snapshots: VecDeque::with_capacity(max_snapshots),
        })
    }

    /// Records a new deployment snapshot.
    ///
    /// If the history is at capacity, the oldest snapshot will be evicted.
    pub fn record(&mut self, snapshot: DeploymentSnapshot) {
        // Check if we already have this deployment ID - if so, update it
        if let Some(pos) = self.snapshots.iter().position(|s| s.id == snapshot.id) {
            self.snapshots.remove(pos);
        }

        // Evict oldest if at capacity
        while self.snapshots.len() >= self.max_snapshots {
            self.snapshots.pop_front();
        }

        self.snapshots.push_back(snapshot);
    }

    /// Gets the deployment that was active before the specified one.
    ///
    /// # Arguments
    ///
    /// * `current_id` - The ID of the current deployment.
    ///
    /// # Returns
    ///
    /// The previous deployment snapshot, or `None` if not found or no previous exists.
    #[must_use]
    pub fn get_previous(&self, current_id: &DeploymentId) -> Option<DeploymentSnapshot> {
        let pos = self.snapshots.iter().position(|s| &s.id == current_id)?;
        if pos == 0 {
            return None;
        }
        self.snapshots.get(pos - 1).cloned()
    }

    /// Gets a deployment n versions back from the most recent.
    ///
    /// # Arguments
    ///
    /// * `n` - Number of versions to go back. 0 returns the most recent,
    ///         1 returns the second most recent, etc.
    ///
    /// # Returns
    ///
    /// The deployment snapshot at the specified position, or `None` if out of range.
    #[must_use]
    pub fn get_version(&self, n: usize) -> Option<DeploymentSnapshot> {
        if self.snapshots.is_empty() {
            return None;
        }
        let len = self.snapshots.len();
        if n >= len {
            return None;
        }
        self.snapshots.get(len - 1 - n).cloned()
    }

    /// Lists the most recent deployments.
    ///
    /// # Arguments
    ///
    /// * `count` - Maximum number of deployments to return.
    ///
    /// # Returns
    ///
    /// A vector of deployment snapshots, newest first.
    #[must_use]
    pub fn list_recent(&self, count: usize) -> Vec<DeploymentSnapshot> {
        let len = self.snapshots.len();
        let take_count = count.min(len);
        self.snapshots
            .iter()
            .rev()
            .take(take_count)
            .cloned()
            .collect()
    }

    /// Returns the total number of recorded snapshots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns true if no snapshots have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Returns the maximum number of snapshots this history can hold.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.max_snapshots
    }

    /// Gets the most recent deployment.
    #[must_use]
    pub fn current(&self) -> Option<DeploymentSnapshot> {
        self.snapshots.back().cloned()
    }

    /// Gets the oldest deployment in the history.
    #[must_use]
    pub fn oldest(&self) -> Option<DeploymentSnapshot> {
        self.snapshots.front().cloned()
    }

    /// Finds a deployment by its ID.
    #[must_use]
    pub fn find(&self, id: &DeploymentId) -> Option<DeploymentSnapshot> {
        self.snapshots.iter().find(|s| &s.id == id).cloned()
    }

    /// Clears all deployment history.
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DeploymentSpec;
    use chrono::{Duration, Utc};

    fn create_snapshot(id: &str) -> DeploymentSnapshot {
        DeploymentSnapshot::new(
            DeploymentId::new(id),
            DeploymentSpec::new("test-app", format!("test-app:{id}")),
        )
    }

    fn create_snapshot_with_time(id: &str, hours_ago: i64) -> DeploymentSnapshot {
        let timestamp = Utc::now() - Duration::hours(hours_ago);
        DeploymentSnapshot::new(
            DeploymentId::new(id),
            DeploymentSpec::new("test-app", format!("test-app:{id}")),
        )
        .with_timestamp(timestamp)
    }

    mod constructor_tests {
        use super::*;

        #[test]
        fn new_with_valid_capacity_returns_some() {
            let history = DeploymentHistory::new(10);
            assert!(history.is_some());
        }

        #[test]
        fn new_with_zero_capacity_returns_none() {
            let history = DeploymentHistory::new(0);
            assert!(history.is_none());
        }

        #[test]
        fn new_history_is_empty() {
            let history = DeploymentHistory::new(10);
            assert!(history.is_some());
            let history = history.unwrap_or_else(|| panic!("should be Some"));
            assert!(history.is_empty());
            assert_eq!(history.len(), 0);
        }

        #[test]
        fn capacity_matches_constructor_argument() {
            let history = DeploymentHistory::new(5);
            assert!(history.is_some());
            let history = history.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(history.capacity(), 5);
        }
    }

    mod record_tests {
        use super::*;

        #[test]
        fn record_adds_snapshot() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            let snapshot = create_snapshot("v1");

            history.record(snapshot);

            assert_eq!(history.len(), 1);
            assert!(!history.is_empty());
        }

        #[test]
        fn record_multiple_snapshots() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));

            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            assert_eq!(history.len(), 3);
        }

        #[test]
        fn record_evicts_oldest_when_at_capacity() {
            let mut history = DeploymentHistory::new(3).unwrap_or_else(|| panic!("should be Some"));

            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));
            history.record(create_snapshot("v4"));

            assert_eq!(history.len(), 3);
            // v1 should be evicted
            assert!(history.find(&DeploymentId::new("v1")).is_none());
            assert!(history.find(&DeploymentId::new("v2")).is_some());
            assert!(history.find(&DeploymentId::new("v4")).is_some());
        }

        #[test]
        fn record_same_id_updates_existing() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));

            let snapshot1 = create_snapshot("v1");
            let snapshot2 = DeploymentSnapshot::new(
                DeploymentId::new("v1"),
                DeploymentSpec::new("updated-app", "updated:v1"),
            );

            history.record(snapshot1);
            history.record(snapshot2);

            assert_eq!(history.len(), 1);
            let found = history.find(&DeploymentId::new("v1"));
            assert!(found.is_some());
            let found = found.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(found.spec.name, "updated-app");
        }
    }

    mod get_previous_tests {
        use super::*;

        #[test]
        fn get_previous_returns_none_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            let result = history.get_previous(&DeploymentId::new("v1"));
            assert!(result.is_none());
        }

        #[test]
        fn get_previous_returns_none_for_first_deployment() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));

            let result = history.get_previous(&DeploymentId::new("v1"));
            assert!(result.is_none());
        }

        #[test]
        fn get_previous_returns_none_for_unknown_id() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));

            let result = history.get_previous(&DeploymentId::new("unknown"));
            assert!(result.is_none());
        }

        #[test]
        fn get_previous_returns_correct_snapshot() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.get_previous(&DeploymentId::new("v3"));
            assert!(result.is_some());
            let result = result.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(result.id, DeploymentId::new("v2"));
        }

        #[test]
        fn get_previous_returns_correct_for_middle_deployment() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.get_previous(&DeploymentId::new("v2"));
            assert!(result.is_some());
            let result = result.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(result.id, DeploymentId::new("v1"));
        }
    }

    mod get_version_tests {
        use super::*;

        #[test]
        fn get_version_returns_none_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            assert!(history.get_version(0).is_none());
        }

        #[test]
        fn get_version_0_returns_most_recent() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.get_version(0);
            assert!(result.is_some());
            let result = result.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(result.id, DeploymentId::new("v3"));
        }

        #[test]
        fn get_version_1_returns_second_most_recent() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.get_version(1);
            assert!(result.is_some());
            let result = result.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(result.id, DeploymentId::new("v2"));
        }

        #[test]
        fn get_version_returns_none_for_out_of_range() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            assert!(history.get_version(2).is_none());
            assert!(history.get_version(100).is_none());
        }

        #[test]
        fn get_version_returns_oldest_for_last_index() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.get_version(2);
            assert!(result.is_some());
            let result = result.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(result.id, DeploymentId::new("v1"));
        }
    }

    mod list_recent_tests {
        use super::*;

        #[test]
        fn list_recent_returns_empty_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            let result = history.list_recent(5);
            assert!(result.is_empty());
        }

        #[test]
        fn list_recent_returns_all_when_count_exceeds_size() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            let result = history.list_recent(10);
            assert_eq!(result.len(), 2);
        }

        #[test]
        fn list_recent_returns_newest_first() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let result = history.list_recent(3);
            assert_eq!(result.len(), 3);
            assert_eq!(result[0].id, DeploymentId::new("v3"));
            assert_eq!(result[1].id, DeploymentId::new("v2"));
            assert_eq!(result[2].id, DeploymentId::new("v1"));
        }

        #[test]
        fn list_recent_respects_count_limit() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));
            history.record(create_snapshot("v4"));
            history.record(create_snapshot("v5"));

            let result = history.list_recent(2);
            assert_eq!(result.len(), 2);
            assert_eq!(result[0].id, DeploymentId::new("v5"));
            assert_eq!(result[1].id, DeploymentId::new("v4"));
        }

        #[test]
        fn list_recent_with_zero_returns_empty() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));

            let result = history.list_recent(0);
            assert!(result.is_empty());
        }
    }

    mod current_and_oldest_tests {
        use super::*;

        #[test]
        fn current_returns_none_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            assert!(history.current().is_none());
        }

        #[test]
        fn current_returns_most_recent() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            let current = history.current();
            assert!(current.is_some());
            let current = current.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(current.id, DeploymentId::new("v2"));
        }

        #[test]
        fn oldest_returns_none_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            assert!(history.oldest().is_none());
        }

        #[test]
        fn oldest_returns_first_recorded() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            let oldest = history.oldest();
            assert!(oldest.is_some());
            let oldest = oldest.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(oldest.id, DeploymentId::new("v1"));
        }

        #[test]
        fn oldest_updates_after_eviction() {
            let mut history = DeploymentHistory::new(2).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let oldest = history.oldest();
            assert!(oldest.is_some());
            let oldest = oldest.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(oldest.id, DeploymentId::new("v2"));
        }
    }

    mod find_tests {
        use super::*;

        #[test]
        fn find_returns_none_for_empty_history() {
            let history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            assert!(history.find(&DeploymentId::new("v1")).is_none());
        }

        #[test]
        fn find_returns_none_for_unknown_id() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));

            assert!(history.find(&DeploymentId::new("unknown")).is_none());
        }

        #[test]
        fn find_returns_matching_snapshot() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));
            history.record(create_snapshot("v3"));

            let found = history.find(&DeploymentId::new("v2"));
            assert!(found.is_some());
            let found = found.unwrap_or_else(|| panic!("should be Some"));
            assert_eq!(found.id, DeploymentId::new("v2"));
        }
    }

    mod clear_tests {
        use super::*;

        #[test]
        fn clear_empties_history() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            history.clear();

            assert!(history.is_empty());
            assert_eq!(history.len(), 0);
        }

        #[test]
        fn clear_preserves_capacity() {
            let mut history = DeploymentHistory::new(5).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));

            history.clear();

            assert_eq!(history.capacity(), 5);
        }
    }

    mod serialization_tests {
        use super::*;

        #[test]
        fn serialization_roundtrip() {
            let mut history = DeploymentHistory::new(10).unwrap_or_else(|| panic!("should be Some"));
            history.record(create_snapshot("v1"));
            history.record(create_snapshot("v2"));

            let json = serde_json::to_string(&history).unwrap_or_default();
            let deserialized: Result<DeploymentHistory, _> = serde_json::from_str(&json);
            
            assert!(deserialized.is_ok());
            let deserialized = deserialized.unwrap_or_else(|_| panic!("should deserialize"));
            assert_eq!(deserialized.len(), 2);
            assert_eq!(deserialized.capacity(), 10);
        }
    }
}
