//! Deployment orchestration for Clawbernetes.
//!
//! Provides [`WorkloadStore`] for container lifecycle tracking and [`DeployStore`]
//! for rolling deployments with revision history and rollback support.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ─────────────────────────────────────────────────────────────
// Workload Store
// ─────────────────────────────────────────────────────────────

/// A persistent workload record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadRecord {
    /// Workload ID (UUID).
    pub id: String,
    /// Container image.
    pub image: String,
    /// Docker container ID (if running).
    pub container_id: Option<String>,
    /// Allocated GPU indices.
    pub gpu_ids: Vec<u32>,
    /// Current state: "running", "stopped", "failed", "exited".
    pub state: String,
    /// Container name (if assigned).
    pub name: Option<String>,
    /// Environment variables.
    pub env: Vec<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last state change timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Exit code (if exited).
    pub exit_code: Option<i32>,
}

/// In-memory workload store backed by JSON snapshots.
pub struct WorkloadStore {
    workloads: HashMap<String, WorkloadRecord>,
    store: JsonStore,
}

impl WorkloadStore {
    /// Create a new workload store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "workloads");
        let workloads = store.load();
        debug!(count = workloads.len(), "loaded workloads from disk");
        Self { workloads, store }
    }

    /// Insert or update a workload record.
    pub fn upsert(&mut self, record: WorkloadRecord) {
        self.workloads.insert(record.id.clone(), record);
        self.snapshot();
    }

    /// Get a workload by ID.
    pub fn get(&self, id: &str) -> Option<&WorkloadRecord> {
        self.workloads.get(id)
    }

    /// Get a mutable reference to a workload.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut WorkloadRecord> {
        self.workloads.get_mut(id)
    }

    /// Update a workload's state.
    pub fn update_state(&mut self, id: &str, state: &str, exit_code: Option<i32>) {
        if let Some(record) = self.workloads.get_mut(id) {
            record.state = state.to_string();
            record.exit_code = exit_code;
            record.updated_at = chrono::Utc::now();
            self.snapshot();
        }
    }

    /// Remove a workload by ID.
    pub fn remove(&mut self, id: &str) -> Option<WorkloadRecord> {
        let record = self.workloads.remove(id);
        if record.is_some() {
            self.snapshot();
        }
        record
    }

    /// List all workloads.
    pub fn list(&self) -> Vec<&WorkloadRecord> {
        self.workloads.values().collect()
    }

    /// Get all workloads that were in "running" state (for reconciliation on startup).
    pub fn running(&self) -> Vec<&WorkloadRecord> {
        self.workloads
            .values()
            .filter(|w| w.state == "running")
            .collect()
    }

    /// Find a workload by its Docker container ID.
    pub fn find_by_container_id(&self, container_id: &str) -> Option<&WorkloadRecord> {
        self.workloads
            .values()
            .find(|w| w.container_id.as_deref() == Some(container_id))
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.workloads) {
            warn!(error = %e, "failed to snapshot workload store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Deploy Store
// ─────────────────────────────────────────────────────────────

/// A deployment record tracking rolling updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRecord {
    /// Deployment name.
    pub name: String,
    /// Current image.
    pub image: String,
    /// Previous image (for rollback).
    pub previous_image: Option<String>,
    /// Target replica count.
    pub replicas: u32,
    /// Active container IDs.
    pub container_ids: Vec<String>,
    /// GPU count per replica.
    pub gpus_per_replica: u32,
    /// Memory limit per replica.
    pub memory: Option<String>,
    /// CPU limit per replica.
    pub cpu: Option<f32>,
    /// Deploy strategy: "rolling", "blue-green", "immediate".
    pub strategy: String,
    /// Current state: "active", "updating", "paused", "failed", "deleted".
    pub state: String,
    /// Revision number (increments on each update).
    pub revision: u32,
    /// History of previous revisions.
    pub history: Vec<DeployRevision>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A historical deployment revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRevision {
    /// Revision number.
    pub revision: u32,
    /// Image used in this revision.
    pub image: String,
    /// Replica count.
    pub replicas: u32,
    /// When this revision was created.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Reason for this revision.
    pub reason: Option<String>,
}

/// In-memory deploy store backed by JSON snapshots.
pub struct DeployStore {
    deploys: HashMap<String, DeployRecord>,
    store: JsonStore,
}

impl DeployStore {
    /// Create a new deploy store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "deploys");
        let deploys = store.load();
        debug!(count = deploys.len(), "loaded deploys from disk");
        Self { deploys, store }
    }

    /// Create a new deployment.
    pub fn create(&mut self, record: DeployRecord) -> Result<(), String> {
        if self.deploys.contains_key(&record.name) {
            return Err(format!("deployment '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.deploys.insert(name, record);
        self.snapshot();
        Ok(())
    }

    /// Get a deployment by name.
    pub fn get(&self, name: &str) -> Option<&DeployRecord> {
        self.deploys.get(name)
    }

    /// Get a mutable reference to a deployment.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut DeployRecord> {
        self.deploys.get_mut(name)
    }

    /// Snapshot after external mutation via get_mut.
    pub fn update(&mut self, name: &str) {
        if self.deploys.contains_key(name) {
            self.snapshot();
        }
    }

    /// Delete a deployment.
    pub fn delete(&mut self, name: &str) -> Option<DeployRecord> {
        let record = self.deploys.remove(name);
        if record.is_some() {
            self.snapshot();
        }
        record
    }

    /// List all deployments.
    pub fn list(&self) -> Vec<&DeployRecord> {
        self.deploys.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.deploys) {
            warn!(error = %e, "failed to snapshot deploy store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workload_store_lifecycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = WorkloadStore::new(dir.path());

        let record = WorkloadRecord {
            id: "w-1".to_string(),
            image: "nginx:latest".to_string(),
            container_id: Some("abc123".to_string()),
            gpu_ids: vec![0],
            state: "running".to_string(),
            name: Some("web".to_string()),
            env: vec!["PORT=80".to_string()],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            exit_code: None,
        };
        store.upsert(record);

        assert!(store.get("w-1").is_some());
        assert_eq!(store.running().len(), 1);
        assert!(store.find_by_container_id("abc123").is_some());

        store.update_state("w-1", "stopped", None);
        assert_eq!(store.running().len(), 0);

        store.remove("w-1");
        assert!(store.get("w-1").is_none());
    }

    #[test]
    fn test_workload_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = WorkloadStore::new(dir.path());
            store.upsert(WorkloadRecord {
                id: "w-p".to_string(),
                image: "redis".to_string(),
                container_id: None,
                gpu_ids: vec![],
                state: "running".to_string(),
                name: None,
                env: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                exit_code: None,
            });
        }
        {
            let store = WorkloadStore::new(dir.path());
            assert!(store.get("w-p").is_some());
            assert_eq!(store.running().len(), 1);
        }
    }

    #[test]
    fn test_deploy_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = DeployStore::new(dir.path());

        let record = DeployRecord {
            name: "web-app".to_string(),
            image: "nginx:1.25".to_string(),
            previous_image: None,
            replicas: 3,
            container_ids: vec![],
            gpus_per_replica: 0,
            memory: None,
            cpu: None,
            strategy: "rolling".to_string(),
            state: "active".to_string(),
            revision: 1,
            history: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.create(record).expect("create");

        assert!(store.get("web-app").is_some());
        assert_eq!(store.list().len(), 1);

        // Duplicate fails
        let dup = DeployRecord {
            name: "web-app".to_string(),
            image: "nginx:1.26".to_string(),
            previous_image: None,
            replicas: 1,
            container_ids: vec![],
            gpus_per_replica: 0,
            memory: None,
            cpu: None,
            strategy: "rolling".to_string(),
            state: "active".to_string(),
            revision: 1,
            history: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert!(store.create(dup).is_err());

        store.delete("web-app");
        assert!(store.get("web-app").is_none());
    }

    #[test]
    fn test_deploy_store_mutation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = DeployStore::new(dir.path());

        store.create(DeployRecord {
            name: "api".to_string(),
            image: "api:v1".to_string(),
            previous_image: None,
            replicas: 2,
            container_ids: vec![],
            gpus_per_replica: 0,
            memory: None,
            cpu: None,
            strategy: "rolling".to_string(),
            state: "active".to_string(),
            revision: 1,
            history: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }).expect("create");

        if let Some(deploy) = store.get_mut("api") {
            deploy.image = "api:v2".to_string();
            deploy.revision = 2;
        }
        store.update("api");

        let deploy = store.get("api").expect("get");
        assert_eq!(deploy.image, "api:v2");
        assert_eq!(deploy.revision, 2);
    }
}
