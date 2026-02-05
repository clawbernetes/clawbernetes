//! In-memory log storage with retention and streaming.
//!
//! This module provides:
//! - [`LogStore`] — Thread-safe log storage with automatic retention
//! - [`LogStream`] — Async stream for tailing logs
//! - Implementation of [`LogStoreTrait`] for generic usage

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use chrono::Utc;
use futures::Stream;
use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::error::{LogError, Result};
use crate::index::LogIndex;
use crate::traits::{LogStoreTrait, RotatableStore};
use crate::types::{LogEntry, LogFilter, LogId};

/// Configuration for the log store.
#[derive(Debug, Clone)]
pub struct LogStoreConfig {
    /// Maximum number of log entries to keep.
    pub max_entries: usize,
    /// How long to retain log entries.
    pub retention: Duration,
    /// Channel capacity for streaming.
    pub stream_buffer_size: usize,
}

impl Default for LogStoreConfig {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            retention: Duration::from_secs(24 * 60 * 60), // 24 hours
            stream_buffer_size: 1024,
        }
    }
}

/// Internal log entry with ordering metadata.
#[derive(Debug, Clone)]
struct StoredEntry {
    entry: LogEntry,
    inserted_at: chrono::DateTime<Utc>,
}

/// Thread-safe in-memory log store with retention and streaming.
pub struct LogStore {
    /// Configuration
    config: LogStoreConfig,
    /// All log entries, ordered by insertion
    entries: RwLock<VecDeque<StoredEntry>>,
    /// Fast lookup by log ID
    by_id: RwLock<HashMap<LogId, LogEntry>>,
    /// Multi-dimensional index
    index: LogIndex,
    /// Next log ID counter
    next_id: AtomicU64,
    /// Broadcast channel for streaming
    broadcast: broadcast::Sender<LogEntry>,
    /// Whether the store is accepting new entries
    accepting: AtomicBool,
}

impl LogStore {
    /// Creates a new log store with the given configuration.
    #[must_use]
    pub fn new(max_entries: usize, retention: Duration) -> Self {
        Self::with_config(LogStoreConfig {
            max_entries,
            retention,
            ..Default::default()
        })
    }

    /// Creates a new log store with full configuration.
    #[must_use]
    pub fn with_config(config: LogStoreConfig) -> Self {
        let (broadcast, _) = broadcast::channel(config.stream_buffer_size);

        Self {
            config,
            entries: RwLock::new(VecDeque::new()),
            by_id: RwLock::new(HashMap::new()),
            index: LogIndex::new(),
            next_id: AtomicU64::new(1),
            broadcast,
            accepting: AtomicBool::new(true),
        }
    }

    /// Appends a new log entry, assigning it an ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the store is not accepting entries.
    #[allow(clippy::significant_drop_tightening)]
    pub fn append(&self, mut entry: LogEntry) -> Result<LogId> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(LogError::CapacityExceeded);
        }

        // Assign ID
        let id = LogId(self.next_id.fetch_add(1, Ordering::Relaxed));
        entry.id = id;

        // Run retention cleanup before inserting
        self.enforce_retention();

        // Store entry
        let stored = StoredEntry {
            entry: entry.clone(),
            inserted_at: Utc::now(),
        };

        // Index entry
        self.index.insert(
            id,
            entry.workload_id,
            entry.node_id,
            entry.level,
            &entry.message,
        );

        // Add to storage
        {
            let mut entries = self.entries.write();
            let mut by_id = self.by_id.write();

            entries.push_back(stored);
            by_id.insert(id, entry.clone());

            // Enforce max entries
            while entries.len() > self.config.max_entries {
                if let Some(removed) = entries.pop_front() {
                    by_id.remove(&removed.entry.id);
                    self.index.remove(
                        removed.entry.id,
                        removed.entry.workload_id,
                        removed.entry.node_id,
                        removed.entry.level,
                        &removed.entry.message,
                    );
                }
            }
        }

        // Broadcast to streams (ignore errors if no receivers)
        let _ = self.broadcast.send(entry);

        Ok(id)
    }

    /// Queries logs matching the filter.
    ///
    /// Returns entries in reverse chronological order (newest first).
    #[must_use]
    pub fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
        let entries = self.entries.read();

        entries
            .iter()
            .rev()
            .map(|s| &s.entry)
            .filter(|e| e.matches(filter))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Gets a specific log entry by ID.
    #[must_use]
    pub fn get(&self, id: LogId) -> Option<LogEntry> {
        self.by_id.read().get(&id).cloned()
    }

    /// Creates a stream that yields new log entries matching the filter.
    ///
    /// This is equivalent to `tail -f` — it streams new entries as they arrive.
    #[must_use]
    pub fn tail(&self, filter: LogFilter) -> LogStream {
        LogStream::new(self.broadcast.subscribe(), filter)
    }

    /// Returns the number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Returns true if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Clears all log entries.
    pub fn clear(&self) {
        self.entries.write().clear();
        self.by_id.write().clear();
        self.index.clear();
    }

    /// Stops accepting new entries.
    pub fn stop(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    /// Resumes accepting new entries.
    pub fn start(&self) {
        self.accepting.store(true, Ordering::Release);
    }

    /// Runs retention cleanup, removing entries older than the retention period.
    #[allow(clippy::significant_drop_tightening)]
    pub fn enforce_retention(&self) {
        let cutoff = Utc::now() - chrono::Duration::from_std(self.config.retention)
            .unwrap_or_else(|_| chrono::Duration::days(1));

        let mut entries = self.entries.write();
        let mut by_id = self.by_id.write();

        while let Some(front) = entries.front() {
            if front.inserted_at < cutoff {
                if let Some(removed) = entries.pop_front() {
                    by_id.remove(&removed.entry.id);
                    self.index.remove(
                        removed.entry.id,
                        removed.entry.workload_id,
                        removed.entry.node_id,
                        removed.entry.level,
                        &removed.entry.message,
                    );
                }
            } else {
                break;
            }
        }
    }

    /// Returns the configuration.
    #[must_use]
    pub const fn config(&self) -> &LogStoreConfig {
        &self.config
    }

    /// Returns a reference to the index.
    #[must_use]
    pub const fn index(&self) -> &LogIndex {
        &self.index
    }
}

/// Async stream of log entries.
///
/// Yields new log entries as they are appended to the store.
pub struct LogStream {
    receiver: broadcast::Receiver<LogEntry>,
    filter: LogFilter,
    closed: bool,
}

impl LogStream {
    const fn new(receiver: broadcast::Receiver<LogEntry>, filter: LogFilter) -> Self {
        Self {
            receiver,
            filter,
            closed: false,
        }
    }

    /// Closes the stream.
    pub const fn close(&mut self) {
        self.closed = true;
    }

    /// Returns true if the stream is closed.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Receives the next matching entry asynchronously.
    async fn recv_next(&mut self) -> Option<LogEntry> {
        loop {
            match self.receiver.recv().await {
                Ok(entry) => {
                    if entry.matches(&self.filter) {
                        return Some(entry);
                    }
                    // Entry didn't match filter, try next
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.closed = true;
                    return None;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Some messages were dropped due to slow consumer
                    // Continue trying to receive
                }
            }
        }
    }
}

impl Stream for LogStream {
    type Item = LogEntry;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.closed {
            return Poll::Ready(None);
        }

        // Create a future for receiving the next entry
        let future = self.recv_next();
        tokio::pin!(future);

        future.poll(cx)
    }
}

/// Shared log store handle.
pub type SharedLogStore = Arc<LogStore>;

/// Creates a new shared log store.
#[must_use]
pub fn shared_store(max_entries: usize, retention: Duration) -> SharedLogStore {
    Arc::new(LogStore::new(max_entries, retention))
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl LogStoreTrait for LogStore {
    fn append(&self, entry: LogEntry) -> Result<LogId> {
        LogStore::append(self, entry)
    }

    fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
        LogStore::query(self, filter, limit)
    }

    fn get(&self, id: LogId) -> Option<LogEntry> {
        LogStore::get(self, id)
    }

    fn len(&self) -> usize {
        LogStore::len(self)
    }

    fn is_empty(&self) -> bool {
        LogStore::is_empty(self)
    }

    fn clear(&self) -> Result<()> {
        LogStore::clear(self);
        Ok(())
    }
}

impl RotatableStore for LogStore {
    fn rotate(&self) -> Result<()> {
        // In-memory store doesn't have file rotation, but we can trigger retention
        self.enforce_retention();
        Ok(())
    }

    fn enforce_retention(&self) -> Result<()> {
        LogStore::enforce_retention(self);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LogLevel;
    use futures::StreamExt;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_entry(level: LogLevel, message: &str) -> LogEntry {
        LogEntry {
            id: LogId(0), // Will be assigned by store
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
            workload_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            fields: HashMap::new(),
        }
    }

    fn make_entry_with_ids(
        level: LogLevel,
        message: &str,
        workload_id: Uuid,
        node_id: Uuid,
    ) -> LogEntry {
        LogEntry {
            id: LogId(0),
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
            workload_id,
            node_id,
            fields: HashMap::new(),
        }
    }

    #[test]
    fn store_append_assigns_id() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let entry = make_entry(LogLevel::Info, "test");

        let result = store.append(entry);
        assert!(result.is_ok());

        if let Ok(id) = result {
            assert_eq!(id.0, 1);
        }
    }

    #[test]
    fn store_append_increments_id() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        let id1 = store.append(make_entry(LogLevel::Info, "test1"));
        let id2 = store.append(make_entry(LogLevel::Info, "test2"));

        assert!(id1.is_ok());
        assert!(id2.is_ok());

        if let (Ok(id1), Ok(id2)) = (id1, id2) {
            assert_eq!(id1.0, 1);
            assert_eq!(id2.0, 2);
        }
    }

    #[test]
    fn store_get_by_id() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let entry = make_entry(LogLevel::Info, "test message");

        let id = store.append(entry);
        assert!(id.is_ok());

        if let Ok(id) = id {
            let retrieved = store.get(id);
            assert!(retrieved.is_some());

            if let Some(e) = retrieved {
                assert_eq!(e.message, "test message");
            }
        }
    }

    #[test]
    fn store_query_returns_newest_first() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        let _ = store.append(make_entry(LogLevel::Info, "first"));
        let _ = store.append(make_entry(LogLevel::Info, "second"));
        let _ = store.append(make_entry(LogLevel::Info, "third"));

        let results = store.query(&LogFilter::default(), 10);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].message, "third");
        assert_eq!(results[1].message, "second");
        assert_eq!(results[2].message, "first");
    }

    #[test]
    fn store_query_with_limit() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        for i in 0..10 {
            let _ = store.append(make_entry(LogLevel::Info, &format!("message {i}")));
        }

        let results = store.query(&LogFilter::default(), 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn store_query_with_filter() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        let _ = store.append(make_entry(LogLevel::Info, "info message"));
        let _ = store.append(make_entry(LogLevel::Error, "error message"));
        let _ = store.append(make_entry(LogLevel::Warn, "warn message"));

        let filter = LogFilter::new().with_level(LogLevel::Error);
        let results = store.query(&filter, 10);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].level, LogLevel::Error);
    }

    #[test]
    fn store_enforces_max_entries() {
        let store = LogStore::new(5, Duration::from_secs(3600));

        for i in 0..10 {
            let _ = store.append(make_entry(LogLevel::Info, &format!("message {i}")));
        }

        assert_eq!(store.len(), 5);

        // Should have the last 5 entries
        let results = store.query(&LogFilter::default(), 10);
        assert!(results[0].message.contains('9'));
        assert!(results[4].message.contains('5'));
    }

    #[test]
    fn store_clear() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        let _ = store.append(make_entry(LogLevel::Info, "test"));
        assert!(!store.is_empty());

        store.clear();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn store_stop_rejects_new_entries() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        store.stop();

        let result = store.append(make_entry(LogLevel::Info, "test"));
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, LogError::CapacityExceeded));
        }
    }

    #[test]
    fn store_start_after_stop() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        store.stop();
        store.start();

        let result = store.append(make_entry(LogLevel::Info, "test"));
        assert!(result.is_ok());
    }

    #[test]
    fn store_indexes_entries() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        let _ = store.append(make_entry_with_ids(
            LogLevel::Error,
            "error message",
            workload_id,
            node_id,
        ));

        // Query using filter (which uses index internally via matches)
        let filter = LogFilter::new().with_workload(workload_id);
        let results = store.query(&filter, 10);
        assert_eq!(results.len(), 1);

        let filter = LogFilter::new().with_node(node_id);
        let results = store.query(&filter, 10);
        assert_eq!(results.len(), 1);

        let filter = LogFilter::new().with_level(LogLevel::Error);
        let results = store.query(&filter, 10);
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn stream_receives_new_entries() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let mut stream = store.tail(LogFilter::default());

        // Append after creating stream
        let _ = store.append(make_entry(LogLevel::Info, "streamed message"));

        // Should receive the entry
        let entry = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;

        assert!(entry.is_ok());
        if let Ok(Some(e)) = entry {
            assert_eq!(e.message, "streamed message");
        }
    }

    #[tokio::test]
    async fn stream_filters_entries() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let filter = LogFilter::new().with_level(LogLevel::Error);
        let mut stream = store.tail(filter);

        // Append entries
        let _ = store.append(make_entry(LogLevel::Info, "info message"));
        let _ = store.append(make_entry(LogLevel::Error, "error message"));

        // Should only receive the error
        let entry = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;

        assert!(entry.is_ok());
        if let Ok(Some(e)) = entry {
            assert_eq!(e.level, LogLevel::Error);
            assert_eq!(e.message, "error message");
        }
    }

    #[tokio::test]
    async fn stream_can_be_closed() {
        let store = LogStore::new(100, Duration::from_secs(3600));
        let mut stream = store.tail(LogFilter::default());

        assert!(!stream.is_closed());
        stream.close();
        assert!(stream.is_closed());

        // Should return None
        let entry = stream.next().await;
        assert!(entry.is_none());
    }

    #[test]
    fn shared_store_works() {
        let store = shared_store(100, Duration::from_secs(3600));

        let _ = store.append(make_entry(LogLevel::Info, "test"));
        assert_eq!(store.len(), 1);

        // Clone the Arc
        let store2 = Arc::clone(&store);
        let _ = store2.append(make_entry(LogLevel::Info, "test2"));
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn store_config_defaults() {
        let config = LogStoreConfig::default();
        assert_eq!(config.max_entries, 100_000);
        assert_eq!(config.retention, Duration::from_secs(24 * 60 * 60));
        assert_eq!(config.stream_buffer_size, 1024);
    }

    #[test]
    fn store_query_with_text_search() {
        let store = LogStore::new(100, Duration::from_secs(3600));

        let _ = store.append(make_entry(LogLevel::Info, "connection established"));
        let _ = store.append(make_entry(LogLevel::Error, "connection failed"));
        let _ = store.append(make_entry(LogLevel::Info, "shutdown complete"));

        let filter = LogFilter::new().with_contains("connection");
        let results = store.query(&filter, 10);

        assert_eq!(results.len(), 2);
    }
}
