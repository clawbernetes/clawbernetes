//! File-based log storage with rotation support.
//!
//! This module provides:
//! - [`FileLogStore`] — Persistent log storage backed by files
//! - Log rotation based on size and age
//! - JSON-lines format for log entries
//! - Implementation of [`LogStoreTrait`] and [`RotatableStore`]

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;

use crate::error::Result;
use crate::traits::{LogStoreTrait, RotatableStore};
use crate::types::{LogEntry, LogFilter, LogId, RetentionPolicy};

/// Configuration for file-based log storage.
#[derive(Debug, Clone)]
pub struct FileLogStoreConfig {
    /// Base directory for log files.
    pub base_dir: PathBuf,
    /// Retention policy for log cleanup.
    pub retention: RetentionPolicy,
    /// Maximum size of a single log file before rotation (bytes).
    pub max_file_size: u64,
    /// Prefix for log file names.
    pub file_prefix: String,
}

impl Default for FileLogStoreConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("logs"),
            retention: RetentionPolicy::default(),
            max_file_size: 10 * 1024 * 1024, // 10 MB
            file_prefix: "claw".to_string(),
        }
    }
}

impl FileLogStoreConfig {
    /// Creates a new config with the given base directory.
    #[must_use]
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            ..Default::default()
        }
    }

    /// Sets the retention policy.
    #[must_use]
    pub fn with_retention(mut self, retention: RetentionPolicy) -> Self {
        self.retention = retention;
        self
    }

    /// Sets the max file size for rotation.
    #[must_use]
    pub const fn with_max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }

    /// Sets the file prefix.
    #[must_use]
    pub fn with_file_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.file_prefix = prefix.into();
        self
    }
}

/// Internal state for file management.
struct FileState {
    /// Current active log file path.
    current_file: Option<PathBuf>,
    /// Current file size in bytes.
    current_size: u64,
    /// List of rotated log files (oldest first).
    rotated_files: Vec<PathBuf>,
}

/// File-based log storage with rotation.
///
/// Stores log entries as JSON-lines in files, rotating when size limits
/// are reached and enforcing retention policies.
pub struct FileLogStore {
    config: FileLogStoreConfig,
    state: RwLock<FileState>,
    next_id: AtomicU64,
    /// Monotonic counter for unique filenames (avoids collisions within same millisecond).
    file_seq: AtomicU64,
}

impl FileLogStore {
    /// Creates a new file log store.
    ///
    /// # Errors
    ///
    /// Returns an error if the base directory cannot be created.
    pub fn new(config: FileLogStoreConfig) -> Result<Self> {
        fs::create_dir_all(&config.base_dir)?;

        let mut rotated_files = Vec::new();
        let mut current_file = None;
        let mut current_size = 0u64;
        let mut max_id = 0u64;

        // Scan existing files
        if let Ok(entries) = fs::read_dir(&config.base_dir) {
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|ext| ext == "log")
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with(&config.file_prefix))
                })
                .collect();
            files.sort();

            // Scan for highest ID
            for file in &files {
                if let Ok(max) = Self::scan_file_max_id(file) {
                    max_id = max_id.max(max);
                }
            }

            if let Some(last) = files.pop() {
                current_size = fs::metadata(&last).map(|m| m.len()).unwrap_or(0);
                current_file = Some(last);
            }
            rotated_files = files;
        }

        Ok(Self {
            config,
            state: RwLock::new(FileState {
                current_file,
                current_size,
                rotated_files,
            }),
            next_id: AtomicU64::new(max_id + 1),
            file_seq: AtomicU64::new(0),
        })
    }

    /// Creates a file log store with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the base directory cannot be created.
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Result<Self> {
        Self::new(FileLogStoreConfig::new(base_dir))
    }

    /// Appends a log entry to the store.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry cannot be written.
    pub fn append(&self, mut entry: LogEntry) -> Result<LogId> {
        let id = LogId(self.next_id.fetch_add(1, Ordering::Relaxed));
        entry.id = id;

        let json = serde_json::to_string(&entry)?;
        let line = format!("{json}\n");
        let line_bytes = line.len() as u64;

        let mut state = self.state.write();

        // Check if rotation is needed
        if state.current_size + line_bytes > self.config.max_file_size {
            self.rotate_locked(&mut state)?;
        }

        // Ensure we have a current file
        if state.current_file.is_none() {
            let filename = self.generate_filename();
            let path = self.config.base_dir.join(&filename);
            state.current_file = Some(path);
            state.current_size = 0;
        }

        // Write the entry
        if let Some(ref path) = state.current_file {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(line.as_bytes())?;
            writer.flush()?;
            state.current_size += line_bytes;
        }

        // Enforce retention
        drop(state);
        self.enforce_retention()?;

        Ok(id)
    }

    /// Queries log entries matching the filter.
    ///
    /// Returns entries in reverse chronological order (newest first).
    #[must_use]
    pub fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
        let state = self.state.read();
        let mut results = Vec::new();

        // Start with current file (newest entries)
        if let Some(ref path) = state.current_file {
            self.read_matching_entries(path, filter, limit, &mut results);
        }

        // Continue with rotated files in reverse order (newest first)
        for path in state.rotated_files.iter().rev() {
            if results.len() >= limit {
                break;
            }
            let remaining = limit - results.len();
            self.read_matching_entries(path, filter, remaining, &mut results);
        }

        // Sort by timestamp descending and take limit
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        results
    }

    /// Gets a specific log entry by ID.
    ///
    /// Note: This is O(n) as it scans all files. For frequent lookups,
    /// consider using the in-memory store instead.
    #[must_use]
    pub fn get(&self, id: LogId) -> Option<LogEntry> {
        let state = self.state.read();
        let mut all_files: Vec<&PathBuf> = state.rotated_files.iter().collect();
        if let Some(ref current) = state.current_file {
            all_files.push(current);
        }

        for path in all_files {
            if let Some(entry) = self.find_entry_in_file(path, id) {
                return Some(entry);
            }
        }
        None
    }

    /// Rotates the current log file.
    ///
    /// # Errors
    ///
    /// Returns an error if rotation fails.
    pub fn rotate(&self) -> Result<()> {
        let mut state = self.state.write();
        self.rotate_locked(&mut state)
    }

    /// Enforces retention policy, removing old files.
    ///
    /// # Errors
    ///
    /// Returns an error if files cannot be deleted.
    pub fn enforce_retention(&self) -> Result<()> {
        let mut state = self.state.write();
        let now = Utc::now();

        // Remove expired files
        if self.config.retention.max_age.is_some() {
            state.rotated_files.retain(|path| {
                if let Some(timestamp) = self.extract_file_timestamp(path) {
                    if self.config.retention.is_expired(timestamp, now) {
                        let _ = fs::remove_file(path);
                        return false;
                    }
                }
                true
            });
        }

        // Enforce max size across all files
        if let Some(max_size) = self.config.retention.max_size {
            let mut total_size = state.current_size;
            for path in &state.rotated_files {
                total_size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }

            while total_size > max_size && !state.rotated_files.is_empty() {
                if let Some(oldest) = state.rotated_files.first().cloned() {
                    let file_size = fs::metadata(&oldest).map(|m| m.len()).unwrap_or(0);
                    let _ = fs::remove_file(&oldest);
                    state.rotated_files.remove(0);
                    total_size = total_size.saturating_sub(file_size);
                } else {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Returns the number of log files.
    #[must_use]
    pub fn file_count(&self) -> usize {
        let state = self.state.read();
        let count = state.rotated_files.len();
        if state.current_file.is_some() {
            count + 1
        } else {
            count
        }
    }

    /// Returns the total size of all log files in bytes.
    #[must_use]
    pub fn total_size(&self) -> u64 {
        let state = self.state.read();
        let mut total = state.current_size;
        for path in &state.rotated_files {
            total += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        }
        total
    }

    /// Clears all log files.
    ///
    /// # Errors
    ///
    /// Returns an error if files cannot be deleted.
    pub fn clear(&self) -> Result<()> {
        let mut state = self.state.write();

        // Remove all rotated files
        for path in &state.rotated_files {
            fs::remove_file(path)?;
        }
        state.rotated_files.clear();

        // Remove current file
        if let Some(ref path) = state.current_file {
            fs::remove_file(path)?;
            state.current_file = None;
            state.current_size = 0;
        }

        Ok(())
    }

    /// Returns the configuration.
    #[must_use]
    pub const fn config(&self) -> &FileLogStoreConfig {
        &self.config
    }

    // ========== Internal Methods ==========

    fn rotate_locked(&self, state: &mut FileState) -> Result<()> {
        if let Some(current) = state.current_file.take() {
            state.rotated_files.push(current);
            state.current_size = 0;
        }

        let filename = self.generate_filename();
        let path = self.config.base_dir.join(&filename);
        state.current_file = Some(path);
        state.current_size = 0;

        Ok(())
    }

    fn generate_filename(&self) -> String {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%3f");
        let seq = self.file_seq.fetch_add(1, Ordering::Relaxed);
        format!("{}_{}_s{:04}.log", self.config.file_prefix, timestamp, seq)
    }

    fn extract_file_timestamp(&self, path: &Path) -> Option<DateTime<Utc>> {
        let filename = path.file_stem()?.to_string_lossy();
        let parts: Vec<&str> = filename.split('_').collect();
        // Format: prefix_YYYYMMDD_HHMMSS_mmm_sNNNN.log -> need at least 5 parts (with seq)
        // Or legacy: prefix_YYYYMMDD_HHMMSS_mmm.log -> 4 parts
        if parts.len() >= 5 && parts[parts.len() - 1].starts_with('s') {
            // New format with sequence: skip the sequence part
            let date_str = parts[parts.len() - 4];
            let time_str = parts[parts.len() - 3];
            let datetime_str = format!("{date_str}_{time_str}");
            DateTime::parse_from_str(&format!("{datetime_str} +0000"), "%Y%m%d_%H%M%S %z")
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else if parts.len() >= 4 {
            // Legacy format without sequence
            let date_str = parts[parts.len() - 3];
            let time_str = parts[parts.len() - 2];
            let datetime_str = format!("{date_str}_{time_str}");
            DateTime::parse_from_str(&format!("{datetime_str} +0000"), "%Y%m%d_%H%M%S %z")
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else if parts.len() >= 3 {
            // Very old format without milliseconds
            let date_str = parts[parts.len() - 2];
            let time_str = parts[parts.len() - 1];
            let datetime_str = format!("{date_str}_{time_str}");
            DateTime::parse_from_str(&format!("{datetime_str} +0000"), "%Y%m%d_%H%M%S %z")
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else {
            None
        }
    }

    fn read_matching_entries(
        &self,
        path: &Path,
        filter: &LogFilter,
        limit: usize,
        results: &mut Vec<LogEntry>,
    ) {
        let Ok(file) = File::open(path) else {
            return;
        };
        let reader = BufReader::new(file);

        // Read all entries, then filter and take from the end (newest)
        let mut file_entries: Vec<LogEntry> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| serde_json::from_str::<LogEntry>(&line).ok())
            .filter(|entry| entry.matches(filter))
            .collect();

        // Take the newest entries (from the end)
        let take_count = limit.min(file_entries.len());
        let start = file_entries.len().saturating_sub(take_count);
        results.extend(file_entries.drain(start..));
    }

    fn find_entry_in_file(&self, path: &Path, id: LogId) -> Option<LogEntry> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(std::result::Result::ok) {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                if entry.id == id {
                    return Some(entry);
                }
            }
        }
        None
    }

    fn scan_file_max_id(path: &Path) -> Result<u64> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut max_id = 0u64;

        for line in reader.lines().map_while(std::result::Result::ok) {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                max_id = max_id.max(entry.id.0);
            }
        }
        Ok(max_id)
    }

    /// Counts the total number of log entries across all files.
    fn count_entries(&self) -> usize {
        let state = self.state.read();
        let mut count = 0;

        for path in &state.rotated_files {
            count += Self::count_entries_in_file(path);
        }

        if let Some(ref path) = state.current_file {
            count += Self::count_entries_in_file(path);
        }

        count
    }

    fn count_entries_in_file(path: &Path) -> usize {
        let Ok(file) = File::open(path) else {
            return 0;
        };
        BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .filter(|l| serde_json::from_str::<LogEntry>(l).is_ok())
            .count()
    }
}

// ============================================================================
// Trait Implementations
// ============================================================================

impl LogStoreTrait for FileLogStore {
    fn append(&self, entry: LogEntry) -> Result<LogId> {
        FileLogStore::append(self, entry)
    }

    fn query(&self, filter: &LogFilter, limit: usize) -> Vec<LogEntry> {
        FileLogStore::query(self, filter, limit)
    }

    fn get(&self, id: LogId) -> Option<LogEntry> {
        FileLogStore::get(self, id)
    }

    fn len(&self) -> usize {
        self.count_entries()
    }

    fn clear(&self) -> Result<()> {
        FileLogStore::clear(self)
    }
}

impl RotatableStore for FileLogStore {
    fn rotate(&self) -> Result<()> {
        FileLogStore::rotate(self)
    }

    fn enforce_retention(&self) -> Result<()> {
        FileLogStore::enforce_retention(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LogLevel;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_entry(level: LogLevel, message: &str) -> LogEntry {
        LogEntry {
            id: LogId(0),
            timestamp: Utc::now(),
            level,
            message: message.to_string(),
            workload_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            fields: HashMap::new(),
        }
    }

    fn make_temp_store() -> (FileLogStore, TempDir) {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = FileLogStoreConfig::new(temp_dir.path());
        let store = FileLogStore::new(config).expect("create store");
        (store, temp_dir)
    }

    #[test]
    fn file_store_creates_directory() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = temp_dir.path().join("nested/logs");
        let config = FileLogStoreConfig::new(&path);
        let store = FileLogStore::new(config);
        assert!(store.is_ok());
        assert!(path.exists());
    }

    #[test]
    fn file_store_append_assigns_id() {
        let (store, _dir) = make_temp_store();
        let entry = make_entry(LogLevel::Info, "test");

        let result = store.append(entry);
        assert!(result.is_ok());

        if let Ok(id) = result {
            assert_eq!(id.0, 1);
        }
    }

    #[test]
    fn file_store_append_increments_id() {
        let (store, _dir) = make_temp_store();

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
    fn file_store_get_by_id() {
        let (store, _dir) = make_temp_store();
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
    fn file_store_query_returns_newest_first() {
        let (store, _dir) = make_temp_store();

        let _ = store.append(make_entry(LogLevel::Info, "first"));
        let _ = store.append(make_entry(LogLevel::Info, "second"));
        let _ = store.append(make_entry(LogLevel::Info, "third"));

        let results = store.query(&LogFilter::default(), 10);

        assert_eq!(results.len(), 3);
        // Newest first
        assert_eq!(results[0].message, "third");
        assert_eq!(results[1].message, "second");
        assert_eq!(results[2].message, "first");
    }

    #[test]
    fn file_store_query_with_limit() {
        let (store, _dir) = make_temp_store();

        for i in 0..10 {
            let _ = store.append(make_entry(LogLevel::Info, &format!("message {i}")));
        }

        let results = store.query(&LogFilter::default(), 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn file_store_query_with_filter() {
        let (store, _dir) = make_temp_store();

        let _ = store.append(make_entry(LogLevel::Info, "info message"));
        let _ = store.append(make_entry(LogLevel::Error, "error message"));
        let _ = store.append(make_entry(LogLevel::Warn, "warn message"));

        let filter = LogFilter::new().with_level(LogLevel::Error);
        let results = store.query(&filter, 10);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].level, LogLevel::Error);
    }

    #[test]
    fn file_store_rotates_on_size() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = FileLogStoreConfig::new(temp_dir.path())
            .with_max_file_size(100); // Very small to trigger rotation

        let store = FileLogStore::new(config).expect("create store");

        // Add enough entries to trigger rotation
        for i in 0..10 {
            let _ = store.append(make_entry(LogLevel::Info, &format!("message {i} with some extra text to make it bigger")));
        }

        assert!(store.file_count() > 1);
    }

    #[test]
    fn file_store_manual_rotation() {
        let (store, _dir) = make_temp_store();

        let _ = store.append(make_entry(LogLevel::Info, "before rotation"));
        assert_eq!(store.file_count(), 1);

        let result = store.rotate();
        assert!(result.is_ok());

        let _ = store.append(make_entry(LogLevel::Info, "after rotation"));
        assert_eq!(store.file_count(), 2);

        // Should still be able to query all entries
        let results = store.query(&LogFilter::default(), 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn file_store_clear() {
        let (store, _dir) = make_temp_store();

        let _ = store.append(make_entry(LogLevel::Info, "test"));
        assert!(store.file_count() > 0);

        let result = store.clear();
        assert!(result.is_ok());
        assert_eq!(store.file_count(), 0);
        assert_eq!(store.total_size(), 0);
    }

    #[test]
    fn file_store_total_size() {
        let (store, _dir) = make_temp_store();

        let _ = store.append(make_entry(LogLevel::Info, "test message"));
        assert!(store.total_size() > 0);
    }

    #[test]
    fn file_store_config_defaults() {
        let config = FileLogStoreConfig::default();
        assert_eq!(config.base_dir, PathBuf::from("logs"));
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert_eq!(config.file_prefix, "claw");
    }

    #[test]
    fn file_store_config_builder() {
        let config = FileLogStoreConfig::new("/var/log/app")
            .with_retention(RetentionPolicy::with_max_entries(1000))
            .with_max_file_size(5 * 1024 * 1024)
            .with_file_prefix("myapp");

        assert_eq!(config.base_dir, PathBuf::from("/var/log/app"));
        assert_eq!(config.max_file_size, 5 * 1024 * 1024);
        assert_eq!(config.file_prefix, "myapp");
        assert_eq!(config.retention.max_entries, Some(1000));
    }

    #[test]
    fn file_store_persists_across_reopen() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = FileLogStoreConfig::new(temp_dir.path());

        // Create and write
        {
            let store = FileLogStore::new(config.clone()).expect("create store");
            let _ = store.append(make_entry(LogLevel::Info, "persisted"));
        }

        // Reopen and read
        {
            let store = FileLogStore::new(config).expect("reopen store");
            let results = store.query(&LogFilter::default(), 10);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].message, "persisted");
        }
    }

    #[test]
    fn file_store_continues_id_sequence() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let config = FileLogStoreConfig::new(temp_dir.path());

        // Create and write
        let first_id = {
            let store = FileLogStore::new(config.clone()).expect("create store");
            store.append(make_entry(LogLevel::Info, "first")).expect("append")
        };

        // Reopen and write more
        let second_id = {
            let store = FileLogStore::new(config).expect("reopen store");
            store.append(make_entry(LogLevel::Info, "second")).expect("append")
        };

        assert!(second_id.0 > first_id.0);
    }

    #[test]
    fn file_store_get_nonexistent_returns_none() {
        let (store, _dir) = make_temp_store();
        let result = store.get(LogId(999));
        assert!(result.is_none());
    }

    #[test]
    fn file_store_query_empty_returns_empty() {
        let (store, _dir) = make_temp_store();
        let results = store.query(&LogFilter::default(), 10);
        assert!(results.is_empty());
    }
}
