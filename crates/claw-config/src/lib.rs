//! Configuration store for Clawbernetes nodes.
//!
//! Provides a key-value configuration store backed by [`claw_persist::JsonStore`].

#![forbid(unsafe_code)]

use claw_persist::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

/// A configuration entry (plain key-value data, no encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    /// Key-value data
    pub data: HashMap<String, String>,
    /// Whether this config can be modified after creation
    pub immutable: bool,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// In-memory config store backed by JSON snapshots.
pub struct ConfigStore {
    configs: HashMap<String, ConfigEntry>,
    store: JsonStore,
}

impl ConfigStore {
    /// Create a new config store, loading any existing state from disk.
    pub fn new(state_path: &Path) -> Self {
        let store = JsonStore::new(state_path, "configs");
        let configs = store.load();
        debug!(count = configs.len(), "loaded configs from disk");
        Self { configs, store }
    }

    /// Create a new configuration. Fails if name already exists.
    pub fn create(
        &mut self,
        name: String,
        data: HashMap<String, String>,
        immutable: bool,
    ) -> Result<(), String> {
        if self.configs.contains_key(&name) {
            return Err(format!("config '{name}' already exists"));
        }
        let now = chrono::Utc::now();
        self.configs.insert(
            name,
            ConfigEntry {
                data,
                immutable,
                created_at: now,
                updated_at: now,
            },
        );
        self.snapshot();
        Ok(())
    }

    /// Get a configuration by name.
    pub fn get(&self, name: &str) -> Option<&ConfigEntry> {
        self.configs.get(name)
    }

    /// Update a configuration. Fails if immutable or not found.
    pub fn update(&mut self, name: &str, data: HashMap<String, String>) -> Result<(), String> {
        let entry = self
            .configs
            .get_mut(name)
            .ok_or_else(|| format!("config '{name}' not found"))?;
        if entry.immutable {
            return Err(format!("config '{name}' is immutable"));
        }
        entry.data = data;
        entry.updated_at = chrono::Utc::now();
        self.snapshot();
        Ok(())
    }

    /// Delete a configuration. Fails if not found.
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        if self.configs.remove(name).is_none() {
            return Err(format!("config '{name}' not found"));
        }
        self.snapshot();
        Ok(())
    }

    /// List all configuration names with metadata.
    pub fn list(&self, prefix: Option<&str>) -> Vec<(&str, &ConfigEntry)> {
        self.configs
            .iter()
            .filter(|(k, _)| prefix.is_none() || k.starts_with(prefix.unwrap_or("")))
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    fn snapshot(&self) {
        if let Err(e) = self.store.save(&self.configs) {
            warn!(error = %e, "failed to snapshot config store");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_store_crud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        let mut data = HashMap::new();
        data.insert("key1".to_string(), "value1".to_string());
        store.create("test-config".to_string(), data, false).expect("create");

        let entry = store.get("test-config").expect("get");
        assert_eq!(entry.data.get("key1").unwrap(), "value1");
        assert!(!entry.immutable);

        let mut new_data = HashMap::new();
        new_data.insert("key1".to_string(), "updated".to_string());
        store.update("test-config", new_data).expect("update");
        let entry = store.get("test-config").expect("get after update");
        assert_eq!(entry.data.get("key1").unwrap(), "updated");

        let list = store.list(None);
        assert_eq!(list.len(), 1);

        store.delete("test-config").expect("delete");
        assert!(store.get("test-config").is_none());
    }

    #[test]
    fn test_config_store_immutable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        let mut data = HashMap::new();
        data.insert("key".to_string(), "val".to_string());
        store.create("immutable-cfg".to_string(), data, true).expect("create");

        let result = store.update("immutable-cfg", HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_config_store_duplicate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        store.create("dup".to_string(), HashMap::new(), false).expect("create");
        let result = store.create("dup".to_string(), HashMap::new(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_store_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        {
            let mut store = ConfigStore::new(dir.path());
            let mut data = HashMap::new();
            data.insert("db_host".to_string(), "localhost".to_string());
            store.create("db-config".to_string(), data, false).expect("create");
        }
        {
            let store = ConfigStore::new(dir.path());
            let entry = store.get("db-config").expect("get after reload");
            assert_eq!(entry.data.get("db_host").unwrap(), "localhost");
        }
    }

    #[test]
    fn test_config_store_prefix_filter() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::new(dir.path());

        store.create("app.db".to_string(), HashMap::new(), false).expect("create");
        store.create("app.cache".to_string(), HashMap::new(), false).expect("create");
        store.create("sys.network".to_string(), HashMap::new(), false).expect("create");

        assert_eq!(store.list(Some("app.")).len(), 2);
        assert_eq!(store.list(Some("sys.")).len(), 1);
        assert_eq!(store.list(None).len(), 3);
    }
}
