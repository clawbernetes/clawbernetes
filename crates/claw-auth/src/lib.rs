//! API key management and audit logging for Clawbernetes RBAC.
//!
//! Provides [`ApiKeyStore`] with SHA-256 hashed secrets and [`AuditLogStore`]
//! for tracking all access and operations.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

// ─────────────────────────────────────────────────────────────
// API Key Store
// ─────────────────────────────────────────────────────────────

/// An API key record for RBAC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    /// Key ID (public identifier).
    pub key_id: String,
    /// Key name / description.
    pub name: String,
    /// Hashed secret (SHA-256 hex).
    pub secret_hash: String,
    /// Scopes granted to this key.
    pub scopes: Vec<String>,
    /// Role: "admin", "operator", "viewer".
    pub role: String,
    /// Whether the key is active.
    pub active: bool,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last used timestamp.
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory API key store backed by JSON snapshots.
pub struct ApiKeyStore {
    keys: HashMap<String, ApiKeyRecord>,
    store: JsonStore,
}

impl ApiKeyStore {
    /// Create a new API key store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "apikeys");
        let keys = store.load();
        debug!(count = keys.len(), "loaded API keys from disk");
        Self { keys, store }
    }

    /// Create a new API key.
    pub fn create(&mut self, record: ApiKeyRecord) -> Result<(), String> {
        if self.keys.contains_key(&record.key_id) {
            return Err(format!("key '{}' already exists", record.key_id));
        }
        let id = record.key_id.clone();
        self.keys.insert(id, record);
        self.snapshot();
        Ok(())
    }

    /// Get a key by ID.
    pub fn get(&self, key_id: &str) -> Option<&ApiKeyRecord> {
        self.keys.get(key_id)
    }

    /// Get a mutable reference to a key.
    pub fn get_mut(&mut self, key_id: &str) -> Option<&mut ApiKeyRecord> {
        self.keys.get_mut(key_id)
    }

    /// Find an active key by its secret hash.
    pub fn find_by_hash(&self, secret_hash: &str) -> Option<&ApiKeyRecord> {
        self.keys.values().find(|k| k.secret_hash == secret_hash && k.active)
    }

    /// Revoke a key (set active = false).
    pub fn revoke(&mut self, key_id: &str) -> Result<(), String> {
        let key = self.keys.get_mut(key_id).ok_or_else(|| format!("key '{key_id}' not found"))?;
        key.active = false;
        self.snapshot();
        Ok(())
    }

    /// Delete a key entirely.
    pub fn delete(&mut self, key_id: &str) -> Option<ApiKeyRecord> {
        let r = self.keys.remove(key_id);
        if r.is_some() {
            self.snapshot();
        }
        r
    }

    /// List all keys.
    pub fn list(&self) -> Vec<&ApiKeyRecord> {
        self.keys.values().collect()
    }

    /// Snapshot after external mutation via get_mut.
    pub fn update(&mut self, key_id: &str) {
        if self.keys.contains_key(key_id) {
            self.snapshot();
        }
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.keys) {
            warn!(error = %e, "failed to snapshot API key store");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Audit Log Store
// ─────────────────────────────────────────────────────────────

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// Entry ID.
    pub id: String,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Who performed the action.
    pub actor: String,
    /// What action was performed.
    pub action: String,
    /// Target resource type.
    pub resource: String,
    /// Target resource ID.
    pub resource_id: Option<String>,
    /// Result of the action.
    pub result: String,
    /// Additional details.
    pub details: Option<String>,
}

/// In-memory audit log store backed by JSON snapshots.
pub struct AuditLogStore {
    entries: HashMap<String, AuditLogEntry>,
    store: JsonStore,
}

impl AuditLogStore {
    /// Create a new audit log store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "audit_log");
        let entries = store.load();
        debug!(count = entries.len(), "loaded audit log from disk");
        Self { entries, store }
    }

    /// Append a new audit log entry.
    pub fn append(&mut self, entry: AuditLogEntry) {
        self.entries.insert(entry.id.clone(), entry);
        self.snapshot();
    }

    /// Query audit log entries with optional filters.
    pub fn query(&self, actor: Option<&str>, action: Option<&str>, limit: usize) -> Vec<&AuditLogEntry> {
        let mut results: Vec<_> = self.entries.values()
            .filter(|e| actor.map_or(true, |a| e.actor == a))
            .filter(|e| action.map_or(true, |a| e.action == a))
            .collect();
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        results
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.entries) {
            warn!(error = %e, "failed to snapshot audit log");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ApiKeyStore::new(dir.path());

        let key = ApiKeyRecord {
            key_id: "k-1".to_string(),
            name: "test-key".to_string(),
            secret_hash: "abc123hash".to_string(),
            scopes: vec!["workload.*".to_string()],
            role: "operator".to_string(),
            active: true,
            created_at: chrono::Utc::now(),
            last_used: None,
        };
        store.create(key).expect("create");
        assert!(store.get("k-1").is_some());
        assert!(store.find_by_hash("abc123hash").is_some());

        store.revoke("k-1").expect("revoke");
        assert!(!store.get("k-1").expect("get").active);
        assert!(store.find_by_hash("abc123hash").is_none()); // inactive

        store.delete("k-1");
        assert!(store.get("k-1").is_none());
    }

    #[test]
    fn test_api_key_store_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ApiKeyStore::new(dir.path());

        let key = ApiKeyRecord {
            key_id: "dup".to_string(),
            name: "dup-key".to_string(),
            secret_hash: "hash".to_string(),
            scopes: vec![],
            role: "viewer".to_string(),
            active: true,
            created_at: chrono::Utc::now(),
            last_used: None,
        };
        store.create(key.clone()).expect("create");
        assert!(store.create(key).is_err());
    }

    #[test]
    fn test_api_key_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = ApiKeyStore::new(dir.path());
            store.create(ApiKeyRecord {
                key_id: "persist-k".to_string(),
                name: "persist".to_string(),
                secret_hash: "h".to_string(),
                scopes: vec![],
                role: "admin".to_string(),
                active: true,
                created_at: chrono::Utc::now(),
                last_used: None,
            }).expect("create");
        }
        {
            let store = ApiKeyStore::new(dir.path());
            assert!(store.get("persist-k").is_some());
        }
    }

    #[test]
    fn test_audit_log_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AuditLogStore::new(dir.path());

        store.append(AuditLogEntry {
            id: "a-1".to_string(),
            timestamp: chrono::Utc::now(),
            actor: "admin".to_string(),
            action: "create_key".to_string(),
            resource: "apikey".to_string(),
            resource_id: Some("k-1".to_string()),
            result: "success".to_string(),
            details: None,
        });

        store.append(AuditLogEntry {
            id: "a-2".to_string(),
            timestamp: chrono::Utc::now(),
            actor: "operator".to_string(),
            action: "deploy".to_string(),
            resource: "deployment".to_string(),
            resource_id: Some("web".to_string()),
            result: "success".to_string(),
            details: None,
        });

        let all = store.query(None, None, 10);
        assert_eq!(all.len(), 2);

        let admin_only = store.query(Some("admin"), None, 10);
        assert_eq!(admin_only.len(), 1);

        let deploys = store.query(None, Some("deploy"), 10);
        assert_eq!(deploys.len(), 1);
    }

    #[test]
    fn test_audit_log_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = AuditLogStore::new(dir.path());

        for i in 0..5 {
            store.append(AuditLogEntry {
                id: format!("a-{i}"),
                timestamp: chrono::Utc::now(),
                actor: "sys".to_string(),
                action: "test".to_string(),
                resource: "test".to_string(),
                resource_id: None,
                result: "ok".to_string(),
                details: None,
            });
        }

        let limited = store.query(None, None, 3);
        assert_eq!(limited.len(), 3);
    }
}
