//! JSON file-backed persistence for Clawbernetes node state.
//!
//! Provides [`JsonStore`], a generic key-value store that keeps data in memory
//! and snapshots to a JSON file on every write.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A simple JSON file-backed store for a single domain of data.
///
/// Keeps data in memory and snapshots to `{state_path}/state/{domain}.json` on every write.
pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    /// Create a new store for the given domain under `state_path`.
    pub fn new(state_path: &Path, domain: &str) -> Self {
        let path = state_path.join("state").join(format!("{domain}.json"));
        Self { path }
    }

    /// Load data from disk. Returns empty map if file doesn't exist.
    pub fn load<T: for<'de> Deserialize<'de>>(&self) -> HashMap<String, T> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                warn!(path = %self.path.display(), error = %e, "corrupt state file, starting fresh");
                HashMap::new()
            }),
            Err(_) => {
                debug!(path = %self.path.display(), "no state file, starting fresh");
                HashMap::new()
            }
        }
    }

    /// Save data to disk. Creates directories as needed.
    pub fn save<T: Serialize>(&self, data: &HashMap<String, T>) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&self.path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_store_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = JsonStore::new(dir.path(), "test");

        let mut data = HashMap::new();
        data.insert("key1".to_string(), "value1".to_string());
        data.insert("key2".to_string(), "value2".to_string());
        store.save(&data).expect("save");

        let loaded: HashMap<String, String> = store.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get("key1").unwrap(), "value1");
    }

    #[test]
    fn test_json_store_empty_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = JsonStore::new(dir.path(), "nonexistent");
        let loaded: HashMap<String, String> = store.load();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_json_store_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state_dir = dir.path().join("state");
        std::fs::create_dir_all(&state_dir).expect("mkdir");
        std::fs::write(state_dir.join("corrupt.json"), "not json").expect("write");

        let store = JsonStore::new(dir.path(), "corrupt");
        let loaded: HashMap<String, String> = store.load();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_json_store_creates_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let deep_path = dir.path().join("a").join("b").join("c");
        let store = JsonStore::new(&deep_path, "deep");

        let mut data = HashMap::new();
        data.insert("k".to_string(), "v".to_string());
        store.save(&data).expect("save with nested dirs");

        let loaded: HashMap<String, String> = store.load();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn test_json_store_overwrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = JsonStore::new(dir.path(), "overwrite");

        let mut data = HashMap::new();
        data.insert("key".to_string(), "first".to_string());
        store.save(&data).expect("save1");

        data.insert("key".to_string(), "second".to_string());
        store.save(&data).expect("save2");

        let loaded: HashMap<String, String> = store.load();
        assert_eq!(loaded.get("key").unwrap(), "second");
    }
}
