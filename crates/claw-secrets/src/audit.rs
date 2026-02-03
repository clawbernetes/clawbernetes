//! Audit logging for secrets access.
//!
//! This module provides a complete audit trail of all secret operations,
//! including who accessed what, when, and why.

use chrono::{DateTime, Utc};
use std::sync::RwLock;

use crate::types::{Accessor, AuditAction, AuditEntry, SecretId};

/// Filter criteria for querying audit logs.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Filter by secret ID.
    pub secret_id: Option<SecretId>,
    /// Filter by accessor.
    pub accessor: Option<Accessor>,
    /// Filter by action type.
    pub action: Option<AuditAction>,
    /// Filter entries after this time.
    pub after: Option<DateTime<Utc>>,
    /// Filter entries before this time.
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of entries to return.
    pub limit: Option<usize>,
}

impl AuditFilter {
    /// Creates a new empty filter that matches all entries.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filters by secret ID.
    #[must_use]
    pub fn for_secret(mut self, secret_id: SecretId) -> Self {
        self.secret_id = Some(secret_id);
        self
    }

    /// Filters by accessor.
    #[must_use]
    pub fn by_accessor(mut self, accessor: Accessor) -> Self {
        self.accessor = Some(accessor);
        self
    }

    /// Filters by action type.
    #[must_use]
    pub fn with_action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Filters entries after a given time.
    #[must_use]
    pub fn after(mut self, time: DateTime<Utc>) -> Self {
        self.after = Some(time);
        self
    }

    /// Filters entries before a given time.
    #[must_use]
    pub fn before(mut self, time: DateTime<Utc>) -> Self {
        self.before = Some(time);
        self
    }

    /// Limits the number of returned entries.
    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Checks if an entry matches this filter.
    fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(ref id) = self.secret_id {
            if &entry.secret_id != id {
                return false;
            }
        }

        if let Some(ref accessor) = self.accessor {
            if &entry.accessor != accessor {
                return false;
            }
        }

        if let Some(action) = self.action {
            if entry.action != action {
                return false;
            }
        }

        if let Some(after) = self.after {
            if entry.timestamp <= after {
                return false;
            }
        }

        if let Some(before) = self.before {
            if entry.timestamp >= before {
                return false;
            }
        }

        true
    }
}

/// An in-memory audit log.
///
/// This implementation stores audit entries in memory. For production use,
/// this should be backed by a persistent store.
pub struct AuditLog {
    entries: RwLock<Vec<AuditEntry>>,
}

impl AuditLog {
    /// Creates a new empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Records an audit entry.
    pub fn record(&self, entry: AuditEntry) {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.push(entry);
    }

    /// Queries the audit log with the given filter.
    ///
    /// Returns entries in reverse chronological order (newest first).
    #[must_use]
    pub fn query(&self, filter: &AuditFilter) -> Vec<AuditEntry> {
        let entries = self
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut results: Vec<AuditEntry> = entries
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();

        // Sort by timestamp descending (newest first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply limit if specified
        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        results
    }

    /// Returns the total number of audit entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Returns true if the audit log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all audit entries.
    ///
    /// This is primarily useful for testing.
    pub fn clear(&self) {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        entries.clear();
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.len();
        f.debug_struct("AuditLog")
            .field("entries_count", &len)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WorkloadId;

    fn make_entry(secret: &str, action: AuditAction) -> AuditEntry {
        AuditEntry::new(
            SecretId::new(secret).expect("valid id"),
            Accessor::System,
            action,
            "test reason",
        )
    }

    #[test]
    fn audit_log_new_is_empty() {
        let log = AuditLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn audit_log_record_entry() {
        let log = AuditLog::new();
        let entry = make_entry("secret1", AuditAction::Created);

        log.record(entry);

        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn audit_log_query_all() {
        let log = AuditLog::new();

        log.record(make_entry("secret1", AuditAction::Created));
        log.record(make_entry("secret2", AuditAction::Read));
        log.record(make_entry("secret1", AuditAction::Updated));

        let results = log.query(&AuditFilter::new());
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn audit_log_query_by_secret_id() {
        let log = AuditLog::new();

        log.record(make_entry("secret1", AuditAction::Created));
        log.record(make_entry("secret2", AuditAction::Read));
        log.record(make_entry("secret1", AuditAction::Updated));

        let filter = AuditFilter::new().for_secret(SecretId::new("secret1").expect("valid"));

        let results = log.query(&filter);
        assert_eq!(results.len(), 2);

        for entry in &results {
            assert_eq!(entry.secret_id.as_str(), "secret1");
        }
    }

    #[test]
    fn audit_log_query_by_action() {
        let log = AuditLog::new();

        log.record(make_entry("secret1", AuditAction::Created));
        log.record(make_entry("secret2", AuditAction::Read));
        log.record(make_entry("secret3", AuditAction::Read));

        let filter = AuditFilter::new().with_action(AuditAction::Read);

        let results = log.query(&filter);
        assert_eq!(results.len(), 2);

        for entry in &results {
            assert_eq!(entry.action, AuditAction::Read);
        }
    }

    #[test]
    fn audit_log_query_by_accessor() {
        let log = AuditLog::new();

        let workload = WorkloadId::new("worker-1");
        let accessor = Accessor::Workload(workload.clone());

        log.record(AuditEntry::new(
            SecretId::new("secret1").expect("valid"),
            accessor.clone(),
            AuditAction::Read,
            "test",
        ));

        log.record(AuditEntry::new(
            SecretId::new("secret2").expect("valid"),
            Accessor::System,
            AuditAction::Read,
            "test",
        ));

        let filter = AuditFilter::new().by_accessor(accessor.clone());

        let results = log.query(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].accessor, accessor);
    }

    #[test]
    fn audit_log_query_with_limit() {
        let log = AuditLog::new();

        for i in 0..10 {
            log.record(make_entry(&format!("secret{i}"), AuditAction::Created));
        }

        let filter = AuditFilter::new().limit(3);

        let results = log.query(&filter);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn audit_log_query_returns_newest_first() {
        let log = AuditLog::new();

        // Record entries with small delays to ensure different timestamps
        log.record(make_entry("first", AuditAction::Created));
        std::thread::sleep(std::time::Duration::from_millis(10));
        log.record(make_entry("second", AuditAction::Created));
        std::thread::sleep(std::time::Duration::from_millis(10));
        log.record(make_entry("third", AuditAction::Created));

        let results = log.query(&AuditFilter::new());

        assert_eq!(results[0].secret_id.as_str(), "third");
        assert_eq!(results[1].secret_id.as_str(), "second");
        assert_eq!(results[2].secret_id.as_str(), "first");
    }

    #[test]
    fn audit_log_query_time_range() {
        let log = AuditLog::new();

        log.record(make_entry("first", AuditAction::Created));
        std::thread::sleep(std::time::Duration::from_millis(50));

        let middle_time = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(50));

        log.record(make_entry("second", AuditAction::Created));
        std::thread::sleep(std::time::Duration::from_millis(50));
        log.record(make_entry("third", AuditAction::Created));

        // Query entries after middle_time
        let filter = AuditFilter::new().after(middle_time);
        let results = log.query(&filter);

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.timestamp > middle_time));
    }

    #[test]
    fn audit_log_clear() {
        let log = AuditLog::new();

        log.record(make_entry("secret1", AuditAction::Created));
        log.record(make_entry("secret2", AuditAction::Created));

        assert_eq!(log.len(), 2);

        log.clear();

        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn audit_filter_chaining() {
        let filter = AuditFilter::new()
            .for_secret(SecretId::new("mysecret").expect("valid"))
            .with_action(AuditAction::Read)
            .limit(10);

        assert!(filter.secret_id.is_some());
        assert_eq!(filter.action, Some(AuditAction::Read));
        assert_eq!(filter.limit, Some(10));
    }

    #[test]
    fn audit_log_debug() {
        let log = AuditLog::new();
        log.record(make_entry("secret1", AuditAction::Created));

        let debug = format!("{log:?}");
        assert!(debug.contains("AuditLog"));
        assert!(debug.contains("entries_count"));
    }

    #[test]
    fn audit_log_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let log = Arc::new(AuditLog::new());
        let mut handles = vec![];

        // Spawn multiple threads recording entries
        for i in 0..10 {
            let log_clone = Arc::clone(&log);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let id = format!("secret-{i}-{j}");
                    log_clone.record(make_entry(&id, AuditAction::Created));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("thread should complete");
        }

        assert_eq!(log.len(), 100);
    }
}
