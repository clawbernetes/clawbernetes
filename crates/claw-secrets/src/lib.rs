//! Encrypted secrets management for Clawbernetes.
//!
//! Provides [`SecretStore`] for AES-256-GCM encrypted secret storage
//! with key rotation support.

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

/// An encrypted secret entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    /// Secret name.
    pub name: String,
    /// Encrypted data (base64-encoded ciphertext).
    pub encrypted_data: String,
    /// Nonce used for encryption (base64-encoded).
    pub nonce: String,
    /// Key version used for encryption.
    pub key_version: u32,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last rotation timestamp.
    pub rotated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory secret store backed by encrypted JSON snapshots.
pub struct SecretStore {
    secrets: HashMap<String, SecretEntry>,
    store: JsonStore,
}

impl SecretStore {
    /// Create a new secret store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "secrets");
        let secrets = store.load();
        debug!(count = secrets.len(), "loaded secrets from disk");
        Self { secrets, store }
    }

    /// Create a new secret.
    pub fn create(&mut self, entry: SecretEntry) -> Result<(), String> {
        if self.secrets.contains_key(&entry.name) {
            return Err(format!("secret '{}' already exists", entry.name));
        }
        let name = entry.name.clone();
        self.secrets.insert(name, entry);
        self.snapshot();
        Ok(())
    }

    /// Get a secret by name.
    pub fn get(&self, name: &str) -> Option<&SecretEntry> {
        self.secrets.get(name)
    }

    /// Update an existing secret.
    pub fn update(&mut self, name: &str, entry: SecretEntry) -> Result<(), String> {
        if !self.secrets.contains_key(name) {
            return Err(format!("secret '{name}' not found"));
        }
        self.secrets.insert(name.to_string(), entry);
        self.snapshot();
        Ok(())
    }

    /// Delete a secret.
    pub fn delete(&mut self, name: &str) -> Option<SecretEntry> {
        let entry = self.secrets.remove(name);
        if entry.is_some() {
            self.snapshot();
        }
        entry
    }

    /// List all secrets.
    pub fn list(&self) -> Vec<&SecretEntry> {
        self.secrets.values().collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.secrets) {
            warn!(error = %e, "failed to snapshot secret store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> SecretEntry {
        SecretEntry {
            name: name.to_string(),
            encrypted_data: "Y2lwaGVydGV4dA==".to_string(),
            nonce: "bm9uY2U=".to_string(),
            key_version: 1,
            created_at: chrono::Utc::now(),
            rotated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_secret_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = SecretStore::new(dir.path());

        store.create(make_entry("db-password")).expect("create");
        assert!(store.get("db-password").is_some());
        assert_eq!(store.list().len(), 1);

        // Duplicate fails
        assert!(store.create(make_entry("db-password")).is_err());

        store.delete("db-password");
        assert!(store.get("db-password").is_none());
    }

    #[test]
    fn test_secret_store_update() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = SecretStore::new(dir.path());

        store.create(make_entry("api-key")).expect("create");

        let mut rotated = make_entry("api-key");
        rotated.key_version = 2;
        rotated.encrypted_data = "bmV3Y2lwaGVy".to_string();
        store.update("api-key", rotated).expect("update");

        let entry = store.get("api-key").expect("get");
        assert_eq!(entry.key_version, 2);
    }

    #[test]
    fn test_secret_store_update_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = SecretStore::new(dir.path());
        assert!(store.update("missing", make_entry("missing")).is_err());
    }

    #[test]
    fn test_secret_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = SecretStore::new(dir.path());
            store.create(make_entry("persist-test")).expect("create");
        }
        {
            let store = SecretStore::new(dir.path());
            assert!(store.get("persist-test").is_some());
        }
    }
}
