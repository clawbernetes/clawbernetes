//! Traits for log storage backends.
//!
//! This module provides the [`LogStoreTrait`] trait for abstracting over
//! different log storage implementations (in-memory, file-based, etc.).

use crate::error::Result;
use crate::types::{LogEntry, LogFilter, LogId};

/// Trait for log storage backends.
///
/// Implementors provide append, query, and management operations for log entries.
/// This trait allows code to work with different storage backends interchangeably.
pub trait LogStoreTrait: Send + Sync {
    /// Appends a log entry to the store, assigning it an ID.
    ///
    /// The entry's ID field will be overwritten with a newly assigned ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the store cannot accept the entry.
    fn append(&self, entry: LogEntry) -> Result<LogId>;

    /// Queries log entries matching the filter.
    ///
    /// Returns entries in reverse chronological order (newest first),
    /// limited to at most `limit` entries.
    fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry>;

    /// Gets a specific log entry by ID.
    ///
    /// Returns `None` if the entry is not found.
    fn get(&self, id: LogId) -> Option<LogEntry>;

    /// Returns the number of stored entries.
    ///
    /// For file-based stores, this may be an approximation.
    fn len(&self) -> usize;

    /// Returns true if the store is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all log entries.
    ///
    /// # Errors
    ///
    /// Returns an error if clearing fails.
    fn clear(&self) -> Result<()>;
}

/// Extension trait for stores that support rotation.
pub trait RotatableStore: LogStoreTrait {
    /// Rotates the current log storage.
    ///
    /// For file-based stores, this creates a new log file.
    /// For in-memory stores, this may be a no-op or trigger cleanup.
    ///
    /// # Errors
    ///
    /// Returns an error if rotation fails.
    fn rotate(&self) -> Result<()>;

    /// Enforces retention policy, removing old entries.
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails.
    fn enforce_retention(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LogLevel;
    use chrono::Utc;
    use std::collections::HashMap;
    use uuid::Uuid;

    /// A simple mock store for testing the trait.
    struct MockStore {
        entries: std::sync::Mutex<Vec<LogEntry>>,
        next_id: std::sync::atomic::AtomicU64,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                entries: std::sync::Mutex::new(Vec::new()),
                next_id: std::sync::atomic::AtomicU64::new(1),
            }
        }
    }

    impl LogStoreTrait for MockStore {
        fn append(&self, mut entry: LogEntry) -> Result<LogId> {
            let id = LogId(
                self.next_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );
            entry.id = id;
            self.entries.lock().map_err(|_| {
                crate::error::LogError::InvalidFilter("mutex poisoned".to_string())
            })?.push(entry);
            Ok(id)
        }

        fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
            let entries = match self.entries.lock() {
                Ok(e) => e,
                Err(_) => return Vec::new(),
            };
            entries
                .iter()
                .rev()
                .filter(|e| e.matches(filter))
                .take(limit)
                .cloned()
                .collect()
        }

        fn get(&self, id: LogId) -> Option<LogEntry> {
            let entries = self.entries.lock().ok()?;
            entries.iter().find(|e| e.id == id).cloned()
        }

        fn len(&self) -> usize {
            self.entries.lock().map(|e| e.len()).unwrap_or(0)
        }

        fn clear(&self) -> Result<()> {
            self.entries.lock().map_err(|_| {
                crate::error::LogError::InvalidFilter("mutex poisoned".to_string())
            })?.clear();
            Ok(())
        }
    }

    fn make_entry(message: &str) -> LogEntry {
        LogEntry {
            id: LogId(0),
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: message.to_string(),
            workload_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            fields: HashMap::new(),
        }
    }

    #[test]
    fn trait_append_and_get() {
        let store = MockStore::new();
        let entry = make_entry("test");

        let id = store.append(entry);
        assert!(id.is_ok());

        if let Ok(id) = id {
            let retrieved = store.get(id);
            assert!(retrieved.is_some());
            assert_eq!(retrieved.as_ref().map(|e| e.message.as_str()), Some("test"));
        }
    }

    #[test]
    fn trait_query() {
        let store = MockStore::new();
        let _ = store.append(make_entry("first"));
        let _ = store.append(make_entry("second"));

        let results = store.query(&LogFilter::default(), 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn trait_len_and_is_empty() {
        let store = MockStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        let _ = store.append(make_entry("test"));
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn trait_clear() {
        let store = MockStore::new();
        let _ = store.append(make_entry("test"));
        assert!(!store.is_empty());

        let result = store.clear();
        assert!(result.is_ok());
        assert!(store.is_empty());
    }
}
