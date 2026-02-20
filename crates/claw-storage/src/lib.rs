//! Volume and backup management for Clawbernetes.
//!
//! Provides [`VolumeStore`] for persistent volume lifecycle and
//! [`BackupStore`] for backup/restore operations.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ─────────────────────────────────────────────────────────────
// Volume Store
// ─────────────────────────────────────────────────────────────

/// A persistent volume record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeRecord {
    /// Volume name.
    pub name: String,
    /// "emptydir", "hostpath", "nfs", "pvc"
    pub volume_type: String,
    /// Host path (for hostpath type).
    pub host_path: Option<String>,
    /// Size limit (e.g., "10Gi").
    pub size: Option<String>,
    /// State: "available", "bound", "released".
    pub state: String,
    /// Container ID this volume is mounted to (if bound).
    pub bound_to: Option<String>,
    /// Mount path inside the container.
    pub mount_path: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory volume store backed by JSON snapshots.
pub struct VolumeStore {
    volumes: HashMap<String, VolumeRecord>,
    store: JsonStore,
}

impl VolumeStore {
    /// Create a new volume store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "volumes");
        let volumes = store.load();
        debug!(count = volumes.len(), "loaded volumes from disk");
        Self { volumes, store }
    }

    /// Create a new volume.
    pub fn create(&mut self, record: VolumeRecord) -> Result<(), String> {
        if self.volumes.contains_key(&record.name) {
            return Err(format!("volume '{}' already exists", record.name));
        }
        let name = record.name.clone();
        self.volumes.insert(name, record);
        self.snapshot();
        Ok(())
    }

    /// Get a volume by name.
    pub fn get(&self, name: &str) -> Option<&VolumeRecord> {
        self.volumes.get(name)
    }

    /// Get a mutable reference to a volume.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut VolumeRecord> {
        self.volumes.get_mut(name)
    }

    /// Snapshot after external mutation via get_mut.
    pub fn update(&mut self, name: &str) {
        if self.volumes.contains_key(name) {
            self.snapshot();
        }
    }

    /// Delete a volume.
    pub fn delete(&mut self, name: &str) -> Option<VolumeRecord> {
        let r = self.volumes.remove(name);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    /// List all volumes.
    pub fn list(&self) -> Vec<&VolumeRecord> {
        self.volumes.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.volumes) {
            warn!(error = %e, "failed to snapshot volume store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Backup Store
// ─────────────────────────────────────────────────────────────

/// A backup entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    /// Backup ID.
    pub id: String,
    /// Scope of the backup.
    pub scope: String,
    /// Destination path or URL.
    pub destination: String,
    /// State: "pending", "completed", "failed".
    pub state: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory backup store backed by JSON snapshots.
pub struct BackupStore {
    backups: HashMap<String, BackupEntry>,
    store: JsonStore,
}

impl BackupStore {
    /// Create a new backup store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "backups");
        let backups = store.load();
        Self { backups, store }
    }

    /// Create a new backup entry.
    pub fn create(&mut self, entry: BackupEntry) -> Result<(), String> {
        if self.backups.contains_key(&entry.id) {
            return Err(format!("backup '{}' already exists", entry.id));
        }
        let id = entry.id.clone();
        self.backups.insert(id, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a backup by ID.
    pub fn get(&self, id: &str) -> Option<&BackupEntry> {
        self.backups.get(id)
    }

    /// Get a mutable reference to a backup.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut BackupEntry> {
        self.backups.get_mut(id)
    }

    /// List all backups.
    pub fn list(&self) -> Vec<&BackupEntry> {
        self.backups.values().collect()
    }

    /// Snapshot after external mutation.
    pub fn update(&mut self) {
        self.snapshot();
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.backups) {
            warn!(error = %e, "failed to snapshot backup store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = VolumeStore::new(dir.path());

        let vol = VolumeRecord {
            name: "data-vol".to_string(),
            volume_type: "hostpath".to_string(),
            host_path: Some("/mnt/data".to_string()),
            size: Some("10Gi".to_string()),
            state: "available".to_string(),
            bound_to: None,
            mount_path: None,
            created_at: chrono::Utc::now(),
        };
        store.create(vol).expect("create");
        assert!(store.get("data-vol").is_some());

        // Bind it
        if let Some(v) = store.get_mut("data-vol") {
            v.state = "bound".to_string();
            v.bound_to = Some("container-1".to_string());
        }
        store.update("data-vol");

        let v = store.get("data-vol").expect("get");
        assert_eq!(v.state, "bound");

        store.delete("data-vol");
        assert!(store.get("data-vol").is_none());
    }

    #[test]
    fn test_volume_store_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = VolumeStore::new(dir.path());

        let vol = VolumeRecord {
            name: "dup".to_string(),
            volume_type: "emptydir".to_string(),
            host_path: None,
            size: None,
            state: "available".to_string(),
            bound_to: None,
            mount_path: None,
            created_at: chrono::Utc::now(),
        };
        store.create(vol.clone()).expect("create");
        assert!(store.create(vol).is_err());
    }

    #[test]
    fn test_volume_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = VolumeStore::new(dir.path());
            store.create(VolumeRecord {
                name: "persist-vol".to_string(),
                volume_type: "hostpath".to_string(),
                host_path: Some("/tmp".to_string()),
                size: None,
                state: "available".to_string(),
                bound_to: None,
                mount_path: None,
                created_at: chrono::Utc::now(),
            }).expect("create");
        }
        {
            let store = VolumeStore::new(dir.path());
            assert!(store.get("persist-vol").is_some());
        }
    }

    #[test]
    fn test_backup_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = BackupStore::new(dir.path());

        let entry = BackupEntry {
            id: "bk-1".to_string(),
            scope: "full".to_string(),
            destination: "/backups/bk-1.tar.gz".to_string(),
            state: "pending".to_string(),
            created_at: chrono::Utc::now(),
        };
        store.create(entry).expect("create");
        assert!(store.get("bk-1").is_some());

        if let Some(bk) = store.get_mut("bk-1") {
            bk.state = "completed".to_string();
        }
        store.update();

        let bk = store.get("bk-1").expect("get");
        assert_eq!(bk.state, "completed");
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn test_backup_store_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = BackupStore::new(dir.path());

        let entry = BackupEntry {
            id: "dup-bk".to_string(),
            scope: "full".to_string(),
            destination: "/backups".to_string(),
            state: "pending".to_string(),
            created_at: chrono::Utc::now(),
        };
        store.create(entry.clone()).expect("create");
        assert!(store.create(entry).is_err());
    }
}
