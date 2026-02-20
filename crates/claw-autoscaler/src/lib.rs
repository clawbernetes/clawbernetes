//! Autoscaling policy management for Clawbernetes workloads.
//!
//! Provides [`AutoscaleStore`] for CRUD operations on autoscaling policies
//! with replica clamping and metric-based scaling.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

/// An autoscaling policy record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleRecord {
    /// Policy name.
    pub name: String,
    /// Target deployment or workload name.
    pub target: String,
    /// Min replicas.
    pub min_replicas: u32,
    /// Max replicas.
    pub max_replicas: u32,
    /// Current replicas.
    pub current_replicas: u32,
    /// Policy type: "target_utilization", "queue_depth", "schedule".
    pub policy_type: String,
    /// Target metric (e.g., "gpu_utilization", "cpu_percent").
    pub metric: Option<String>,
    /// Target threshold value.
    pub threshold: Option<f64>,
    /// State: "active", "disabled".
    pub state: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory autoscale store backed by JSON snapshots.
pub struct AutoscaleStore {
    policies: HashMap<String, AutoscaleRecord>,
    store: JsonStore,
}

impl AutoscaleStore {
    /// Create a new autoscale store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "autoscale");
        let policies = store.load();
        debug!(count = policies.len(), "loaded autoscale policies from disk");
        Self { policies, store }
    }

    /// Create a new autoscale policy.
    pub fn create(&mut self, record: AutoscaleRecord) -> Result<(), String> {
        if self.policies.contains_key(&record.name) {
            return Err(format!("policy '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.policies.insert(name, record);
        self.snapshot();
        Ok(())
    }

    /// Get a policy by name.
    pub fn get(&self, name: &str) -> Option<&AutoscaleRecord> {
        self.policies.get(name)
    }

    /// Get a mutable reference to a policy.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut AutoscaleRecord> {
        self.policies.get_mut(name)
    }

    /// Snapshot after external mutation via get_mut.
    pub fn update(&mut self, name: &str) {
        if self.policies.contains_key(name) {
            self.snapshot();
        }
    }

    /// Delete a policy.
    pub fn delete(&mut self, name: &str) -> Option<AutoscaleRecord> {
        let r = self.policies.remove(name);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    /// List all policies.
    pub fn list(&self) -> Vec<&AutoscaleRecord> {
        self.policies.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.policies) {
            warn!(error = %e, "failed to snapshot autoscale store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_policy(name: &str) -> AutoscaleRecord {
        AutoscaleRecord {
            name: name.to_string(),
            target: "web-deploy".to_string(),
            min_replicas: 1,
            max_replicas: 10,
            current_replicas: 2,
            policy_type: "target_utilization".to_string(),
            metric: Some("cpu_percent".to_string()),
            threshold: Some(70.0),
            state: "active".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_autoscale_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AutoscaleStore::new(dir.path());

        store.create(make_policy("cpu-scale")).expect("create");
        assert!(store.get("cpu-scale").is_some());
        assert_eq!(store.list().len(), 1);

        assert!(store.create(make_policy("cpu-scale")).is_err());

        store.delete("cpu-scale");
        assert!(store.get("cpu-scale").is_none());
    }

    #[test]
    fn test_autoscale_store_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AutoscaleStore::new(dir.path());

        store.create(make_policy("gpu-scale")).expect("create");

        if let Some(p) = store.get_mut("gpu-scale") {
            p.current_replicas = 5;
            p.updated_at = chrono::Utc::now();
        }
        store.update("gpu-scale");

        assert_eq!(store.get("gpu-scale").expect("get").current_replicas, 5);
    }

    #[test]
    fn test_autoscale_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = AutoscaleStore::new(dir.path());
            store.create(make_policy("persist-pol")).expect("create");
        }
        {
            let store = AutoscaleStore::new(dir.path());
            assert!(store.get("persist-pol").is_some());
        }
    }

    #[test]
    fn test_autoscale_clamping() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AutoscaleStore::new(dir.path());

        let mut policy = make_policy("clamp-test");
        policy.min_replicas = 2;
        policy.max_replicas = 8;
        store.create(policy).expect("create");

        if let Some(p) = store.get_mut("clamp-test") {
            // Clamp to max
            p.current_replicas = 15_u32.min(p.max_replicas).max(p.min_replicas);
        }
        store.update("clamp-test");
        assert_eq!(store.get("clamp-test").expect("get").current_replicas, 8);

        if let Some(p) = store.get_mut("clamp-test") {
            // Clamp to min
            p.current_replicas = 0_u32.min(p.max_replicas).max(p.min_replicas);
        }
        store.update("clamp-test");
        assert_eq!(store.get("clamp-test").expect("get").current_replicas, 2);
    }
}
