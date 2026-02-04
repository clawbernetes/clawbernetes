//! Core types for the structured logging system.
//!
//! This module provides:
//! - [`LogLevel`] — Severity levels for log entries
//! - [`LogEntry`] — Structured log entry with metadata
//! - [`LogFilter`] — Query filters for searching logs
//! - [`LogId`] — Unique identifier for log entries
//! - [`TimeRange`] — Time-based filtering

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LogId(pub u64);

/// Log severity levels, ordered from most to least verbose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Most verbose, detailed debugging information
    Trace = 0,
    /// Debugging information
    Debug = 1,
    /// General information
    Info = 2,
    /// Warning conditions
    Warn = 3,
    /// Error conditions
    Error = 4,
}

/// A structured log entry with metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unique identifier for this entry
    pub id: LogId,
    /// When the log was created
    pub timestamp: DateTime<Utc>,
    /// Severity level
    pub level: LogLevel,
    /// The log message
    pub message: String,
    /// Associated workload identifier
    pub workload_id: Uuid,
    /// Node that generated this log
    pub node_id: Uuid,
    /// Additional structured fields
    #[serde(default)]
    pub fields: HashMap<String, serde_json::Value>,
}

/// Time range for filtering logs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// Start of the time range (inclusive)
    pub start: Option<DateTime<Utc>>,
    /// End of the time range (exclusive)
    pub end: Option<DateTime<Utc>>,
}

/// Filter criteria for querying logs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFilter {
    /// Filter by log levels (empty means all levels)
    #[serde(default)]
    pub levels: Vec<LogLevel>,
    /// Filter by workload IDs (empty means all workloads)
    #[serde(default)]
    pub workload_ids: Vec<Uuid>,
    /// Filter by node IDs (empty means all nodes)
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
    /// Text search in message field (case-insensitive contains)
    pub contains: Option<String>,
    /// Time range filter
    #[serde(default)]
    pub time_range: TimeRange,
}

impl LogLevel {
    /// Returns true if this level is at least as severe as the given level.
    #[must_use]
    pub fn is_at_least(&self, level: Self) -> bool {
        *self >= level
    }

    /// Returns the string representation of this level.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl LogEntry {
    /// Creates a new log entry builder.
    #[must_use]
    pub fn builder() -> LogEntryBuilder {
        LogEntryBuilder::default()
    }

    /// Checks if this entry matches the given filter.
    #[must_use]
    pub fn matches(&self, filter: &LogFilter) -> bool {
        // Check level filter
        if !filter.levels.is_empty() && !filter.levels.contains(&self.level) {
            return false;
        }

        // Check workload filter
        if !filter.workload_ids.is_empty() && !filter.workload_ids.contains(&self.workload_id) {
            return false;
        }

        // Check node filter
        if !filter.node_ids.is_empty() && !filter.node_ids.contains(&self.node_id) {
            return false;
        }

        // Check text search
        if let Some(ref search) = filter.contains {
            let search_lower = search.to_lowercase();
            if !self.message.to_lowercase().contains(&search_lower) {
                return false;
            }
        }

        // Check time range
        if let Some(start) = filter.time_range.start {
            if self.timestamp < start {
                return false;
            }
        }
        if let Some(end) = filter.time_range.end {
            if self.timestamp >= end {
                return false;
            }
        }

        true
    }
}

impl TimeRange {
    /// Creates a new time range with the given bounds.
    #[must_use]
    pub const fn new(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Self {
        Self { start, end }
    }

    /// Creates a time range from a start time to now.
    #[must_use]
    pub const fn since(start: DateTime<Utc>) -> Self {
        Self {
            start: Some(start),
            end: None,
        }
    }

    /// Checks if a timestamp falls within this range.
    #[must_use]
    pub fn contains(&self, timestamp: DateTime<Utc>) -> bool {
        if let Some(start) = self.start {
            if timestamp < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if timestamp >= end {
                return false;
            }
        }
        true
    }
}

impl LogFilter {
    /// Creates a new empty filter that matches all logs.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a level filter.
    #[must_use]
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.levels.push(level);
        self
    }

    /// Adds a workload ID filter.
    #[must_use]
    pub fn with_workload(mut self, workload_id: Uuid) -> Self {
        self.workload_ids.push(workload_id);
        self
    }

    /// Adds a node ID filter.
    #[must_use]
    pub fn with_node(mut self, node_id: Uuid) -> Self {
        self.node_ids.push(node_id);
        self
    }

    /// Adds a text search filter.
    #[must_use]
    pub fn with_contains(mut self, text: impl Into<String>) -> Self {
        self.contains = Some(text.into());
        self
    }

    /// Adds a time range filter.
    #[must_use]
    pub const fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = time_range;
        self
    }
}

/// Builder for constructing log entries.
#[derive(Debug, Default)]
pub struct LogEntryBuilder {
    id: Option<LogId>,
    timestamp: Option<DateTime<Utc>>,
    level: Option<LogLevel>,
    message: Option<String>,
    workload_id: Option<Uuid>,
    node_id: Option<Uuid>,
    fields: HashMap<String, serde_json::Value>,
}

impl LogEntryBuilder {
    /// Sets the log ID.
    #[must_use]
    pub const fn id(mut self, id: LogId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the timestamp.
    #[must_use]
    pub const fn timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Sets the log level.
    #[must_use]
    pub const fn level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// Sets the message.
    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Sets the workload ID.
    #[must_use]
    pub const fn workload_id(mut self, workload_id: Uuid) -> Self {
        self.workload_id = Some(workload_id);
        self
    }

    /// Sets the node ID.
    #[must_use]
    pub const fn node_id(mut self, node_id: Uuid) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Adds a field.
    #[must_use]
    pub fn field(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.fields.insert(key.into(), value);
        self
    }

    /// Builds the log entry, returning an error if required fields are missing.
    ///
    /// # Errors
    ///
    /// Returns an error if any required field is not set.
    pub fn build(self) -> Result<LogEntry, crate::error::LogError> {
        let id = self.id.ok_or(crate::error::LogError::MissingField("id"))?;
        let timestamp = self
            .timestamp
            .ok_or(crate::error::LogError::MissingField("timestamp"))?;
        let level = self
            .level
            .ok_or(crate::error::LogError::MissingField("level"))?;
        let message = self
            .message
            .ok_or(crate::error::LogError::MissingField("message"))?;
        let workload_id = self
            .workload_id
            .ok_or(crate::error::LogError::MissingField("workload_id"))?;
        let node_id = self
            .node_id
            .ok_or(crate::error::LogError::MissingField("node_id"))?;

        Ok(LogEntry {
            id,
            timestamp,
            level,
            message,
            workload_id,
            node_id,
            fields: self.fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===========================================
    // LogLevel Tests
    // ===========================================

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn log_level_is_at_least() {
        assert!(LogLevel::Error.is_at_least(LogLevel::Trace));
        assert!(LogLevel::Error.is_at_least(LogLevel::Error));
        assert!(!LogLevel::Debug.is_at_least(LogLevel::Info));
    }

    #[test]
    fn log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "trace");
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[test]
    fn log_level_serialization() {
        let level = LogLevel::Info;
        let json = serde_json::to_string(&level).map_err(|e| format!("serialize: {e}"));
        assert_eq!(json, Ok("\"info\"".to_string()));

        let deserialized: Result<LogLevel, _> =
            serde_json::from_str("\"warn\"").map_err(|e| format!("deserialize: {e}"));
        assert_eq!(deserialized, Ok(LogLevel::Warn));
    }

    // ===========================================
    // LogEntry Tests
    // ===========================================

    fn make_test_entry() -> LogEntry {
        LogEntry {
            id: LogId(1),
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: "Test message".to_string(),
            workload_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            fields: HashMap::new(),
        }
    }

    #[test]
    fn log_entry_builder_success() {
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let now = Utc::now();

        let entry = LogEntry::builder()
            .id(LogId(42))
            .timestamp(now)
            .level(LogLevel::Warn)
            .message("Something happened")
            .workload_id(workload_id)
            .node_id(node_id)
            .field("key", serde_json::json!("value"))
            .build();

        assert!(entry.is_ok());
        let entry = entry.map_err(|e| format!("{e:?}"));
        let entry = entry.ok();
        assert!(entry.is_some());
        let entry = entry.as_ref();
        let entry = entry.map(|e| e.clone());
        let entry = entry.map(|e| {
            assert_eq!(e.id, LogId(42));
            assert_eq!(e.level, LogLevel::Warn);
            assert_eq!(e.message, "Something happened");
            assert_eq!(e.workload_id, workload_id);
            assert_eq!(e.node_id, node_id);
            assert!(e.fields.contains_key("key"));
            e
        });
        assert!(entry.is_some());
    }

    #[test]
    fn log_entry_builder_missing_field() {
        let result = LogEntry::builder()
            .id(LogId(1))
            .level(LogLevel::Info)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn log_entry_serialization_roundtrip() {
        let entry = make_test_entry();
        let json = serde_json::to_string(&entry);
        assert!(json.is_ok());

        let json = json.map_err(|e| format!("{e}"));
        let json = json.as_ref();
        let json = json.map(|s| s.as_str());
        let json = json.ok();

        if let Some(json_str) = json {
            let deserialized: Result<LogEntry, _> = serde_json::from_str(json_str);
            assert!(deserialized.is_ok());
        }
    }

    // ===========================================
    // LogFilter Tests
    // ===========================================

    #[test]
    fn filter_matches_all_by_default() {
        let filter = LogFilter::new();
        let entry = make_test_entry();
        assert!(entry.matches(&filter));
    }

    #[test]
    fn filter_by_level() {
        let entry = make_test_entry();
        let filter = LogFilter::new().with_level(LogLevel::Info);
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_level(LogLevel::Error);
        assert!(!entry.matches(&filter));
    }

    #[test]
    fn filter_by_workload() {
        let entry = make_test_entry();
        let filter = LogFilter::new().with_workload(entry.workload_id);
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_workload(Uuid::new_v4());
        assert!(!entry.matches(&filter));
    }

    #[test]
    fn filter_by_node() {
        let entry = make_test_entry();
        let filter = LogFilter::new().with_node(entry.node_id);
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_node(Uuid::new_v4());
        assert!(!entry.matches(&filter));
    }

    #[test]
    fn filter_by_contains() {
        let entry = make_test_entry();
        let filter = LogFilter::new().with_contains("Test");
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_contains("test"); // case insensitive
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_contains("not found");
        assert!(!entry.matches(&filter));
    }

    #[test]
    fn filter_by_time_range() {
        let entry = make_test_entry();
        let past = entry.timestamp - chrono::Duration::hours(1);
        let future = entry.timestamp + chrono::Duration::hours(1);

        let filter = LogFilter::new().with_time_range(TimeRange::new(Some(past), Some(future)));
        assert!(entry.matches(&filter));

        let filter = LogFilter::new().with_time_range(TimeRange::new(Some(future), None));
        assert!(!entry.matches(&filter));
    }

    // ===========================================
    // TimeRange Tests
    // ===========================================

    #[test]
    fn time_range_contains() {
        let now = Utc::now();
        let past = now - chrono::Duration::hours(1);
        let future = now + chrono::Duration::hours(1);

        let range = TimeRange::new(Some(past), Some(future));
        assert!(range.contains(now));
        assert!(!range.contains(past - chrono::Duration::seconds(1)));
        assert!(!range.contains(future));
    }

    #[test]
    fn time_range_since() {
        let start = Utc::now() - chrono::Duration::hours(1);
        let range = TimeRange::since(start);

        assert!(range.contains(Utc::now()));
        assert!(!range.contains(start - chrono::Duration::seconds(1)));
    }

    // ===========================================
    // LogId Tests
    // ===========================================

    #[test]
    fn log_id_equality() {
        let id1 = LogId(42);
        let id2 = LogId(42);
        let id3 = LogId(43);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn log_id_serialization() {
        let id = LogId(12345);
        let json = serde_json::to_string(&id);
        assert!(json.is_ok());

        if let Ok(json_str) = json {
            let deserialized: Result<LogId, _> = serde_json::from_str(&json_str);
            assert!(deserialized.is_ok());
            if let Ok(deserialized_id) = deserialized {
                assert_eq!(deserialized_id, id);
            }
        }
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn log_level_as_str_all() {
        // All levels should have string representations
        assert!(!LogLevel::Trace.as_str().is_empty());
        assert!(!LogLevel::Debug.as_str().is_empty());
        assert!(!LogLevel::Info.as_str().is_empty());
        assert!(!LogLevel::Warn.as_str().is_empty());
        assert!(!LogLevel::Error.as_str().is_empty());
    }

    #[test]
    fn log_level_debug_format() {
        let level = LogLevel::Info;
        let debug = format!("{:?}", level);
        assert!(debug.contains("Info"));
    }

    #[test]
    fn log_level_clone() {
        let level = LogLevel::Warn;
        let cloned = level;
        assert_eq!(level, cloned);
    }

    #[test]
    fn log_level_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(LogLevel::Info);
        set.insert(LogLevel::Warn);
        set.insert(LogLevel::Info); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn log_id_inner_value() {
        let id = LogId(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn log_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(LogId(1));
        set.insert(LogId(2));
        set.insert(LogId(1)); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn log_id_clone() {
        let id = LogId(42);
        let cloned = id;
        assert_eq!(id, cloned);
    }

    #[test]
    fn log_id_debug() {
        let id = LogId(123);
        let debug = format!("{:?}", id);
        assert!(debug.contains("LogId"));
        assert!(debug.contains("123"));
    }

    #[test]
    fn log_entry_with_fields() {
        let entry = LogEntry::builder()
            .id(LogId(1))
            .timestamp(Utc::now())
            .level(LogLevel::Info)
            .message("test")
            .workload_id(Uuid::new_v4())
            .node_id(Uuid::new_v4())
            .field("string_field", serde_json::json!("value"))
            .field("number_field", serde_json::json!(42))
            .field("bool_field", serde_json::json!(true))
            .build()
            .expect("should build");

        assert_eq!(entry.fields.len(), 3);
    }

    #[test]
    fn log_entry_clone() {
        let entry = make_test_entry();
        let cloned = entry.clone();
        assert_eq!(entry.id, cloned.id);
        assert_eq!(entry.message, cloned.message);
    }

    #[test]
    fn log_entry_debug() {
        let entry = make_test_entry();
        let debug = format!("{:?}", entry);
        assert!(debug.contains("LogEntry"));
    }

    #[test]
    fn log_filter_clone() {
        let filter = LogFilter::new()
            .with_level(LogLevel::Info)
            .with_contains("test");
        let cloned = filter.clone();
        assert_eq!(filter.levels, cloned.levels);
        assert_eq!(filter.contains, cloned.contains);
    }

    #[test]
    fn log_filter_debug() {
        let filter = LogFilter::new();
        let debug = format!("{:?}", filter);
        assert!(debug.contains("LogFilter"));
    }

    #[test]
    fn log_filter_multiple_criteria() {
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        let entry = LogEntry {
            id: LogId(1),
            timestamp: Utc::now(),
            level: LogLevel::Warn,
            message: "Warning: disk space low".to_string(),
            workload_id,
            node_id,
            fields: HashMap::new(),
        };

        // All criteria match
        let filter = LogFilter::new()
            .with_level(LogLevel::Warn)
            .with_workload(workload_id)
            .with_node(node_id)
            .with_contains("disk");
        assert!(entry.matches(&filter));

        // One criterion doesn't match
        let filter = LogFilter::new()
            .with_level(LogLevel::Error); // Entry is Warn, not Error
        assert!(!entry.matches(&filter));
    }

    #[test]
    fn log_filter_serialization() {
        let filter = LogFilter::new()
            .with_level(LogLevel::Info)
            .with_contains("test");
        let json = serde_json::to_string(&filter).expect("serialize");
        let parsed: LogFilter = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(filter.levels, parsed.levels);
        assert_eq!(filter.contains, parsed.contains);
    }

    #[test]
    fn time_range_unbounded() {
        let range = TimeRange::new(None, None);
        assert!(range.contains(Utc::now()));
        assert!(range.contains(Utc::now() - chrono::Duration::days(365)));
        assert!(range.contains(Utc::now() + chrono::Duration::days(365)));
    }

    #[test]
    fn time_range_until() {
        let end = Utc::now();
        let range = TimeRange::new(None, Some(end));
        assert!(range.contains(end - chrono::Duration::hours(1)));
        assert!(!range.contains(end + chrono::Duration::hours(1)));
    }

    #[test]
    fn time_range_clone() {
        let now = Utc::now();
        let range = TimeRange::new(Some(now), Some(now + chrono::Duration::hours(1)));
        let cloned = range.clone();
        assert_eq!(range.start, cloned.start);
        assert_eq!(range.end, cloned.end);
    }

    #[test]
    fn time_range_debug() {
        let range = TimeRange::since(Utc::now());
        let debug = format!("{:?}", range);
        assert!(debug.contains("TimeRange"));
    }

    #[test]
    fn time_range_serialization() {
        let now = Utc::now();
        let range = TimeRange::new(Some(now), Some(now + chrono::Duration::hours(1)));
        let json = serde_json::to_string(&range).expect("serialize");
        let parsed: TimeRange = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(range.start, parsed.start);
    }

    #[test]
    fn log_entry_builder_defaults() {
        // Builder with all required fields using defaults where possible
        let entry = LogEntry::builder()
            .id(LogId(0))
            .timestamp(Utc::now())
            .level(LogLevel::Trace)
            .message("")
            .workload_id(Uuid::nil())
            .node_id(Uuid::nil())
            .build()
            .expect("should build with minimal values");

        assert_eq!(entry.id, LogId(0));
        assert!(entry.message.is_empty());
        assert!(entry.fields.is_empty());
    }

    #[test]
    fn filter_contains_case_insensitive() {
        let entry = LogEntry {
            id: LogId(1),
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: "Hello World".to_string(),
            workload_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            fields: HashMap::new(),
        };

        assert!(entry.matches(&LogFilter::new().with_contains("hello")));
        assert!(entry.matches(&LogFilter::new().with_contains("HELLO")));
        assert!(entry.matches(&LogFilter::new().with_contains("HeLLo WoRLD")));
    }
}
