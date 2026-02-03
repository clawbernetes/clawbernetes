//! Log collection from workloads and nodes.
//!
//! This module provides:
//! - [`WorkloadLogCollector`] — Collects logs from container stdout/stderr
//! - [`NodeLogCollector`] — Collects node-level events
//! - Line parsing with JSON detection

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::store::LogStore;
use crate::types::{LogEntry, LogId, LogLevel};

/// Source of a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogSource {
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// System/internal source
    System,
}

/// A parsed log line with metadata.
#[derive(Debug, Clone)]
pub struct ParsedLine {
    /// The parsed log level (or default)
    pub level: LogLevel,
    /// The message content
    pub message: String,
    /// Extracted fields from JSON
    pub fields: HashMap<String, serde_json::Value>,
    /// Whether this was parsed from JSON
    pub is_json: bool,
}

/// Parser for log lines.
pub struct LineParser {
    /// Default log level for unparseable lines
    default_level: LogLevel,
}

impl Default for LineParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LineParser {
    /// Creates a new line parser.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            default_level: LogLevel::Info,
        }
    }

    /// Creates a parser with a custom default level.
    #[must_use]
    pub const fn with_default_level(level: LogLevel) -> Self {
        Self {
            default_level: level,
        }
    }

    /// Parses a log line, detecting JSON and extracting fields.
    #[must_use]
    pub fn parse(&self, line: &str) -> ParsedLine {
        let trimmed = line.trim();

        // Try to parse as JSON
        if trimmed.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return self.parse_json(&json);
            }
        }

        // Try to detect log level from prefix
        let (level, message) = self.parse_plain(trimmed);

        ParsedLine {
            level,
            message,
            fields: HashMap::new(),
            is_json: false,
        }
    }

    /// Parses a JSON log entry.
    fn parse_json(&self, json: &serde_json::Value) -> ParsedLine {
        let Some(obj) = json.as_object() else {
            return ParsedLine {
                level: self.default_level,
                message: json.to_string(),
                fields: HashMap::new(),
                is_json: true,
            };
        };

        // Extract level from common field names
        let level = self.extract_level(obj);

        // Extract message from common field names
        let message = Self::extract_message(obj);

        // Collect remaining fields
        let mut fields = HashMap::new();
        for (key, value) in obj {
            let key_lower = key.to_lowercase();
            if !["level", "lvl", "severity", "msg", "message", "text"].contains(&key_lower.as_str())
            {
                fields.insert(key.clone(), value.clone());
            }
        }

        ParsedLine {
            level,
            message,
            fields,
            is_json: true,
        }
    }

    /// Extracts log level from JSON object.
    fn extract_level(&self, obj: &serde_json::Map<String, serde_json::Value>) -> LogLevel {
        let level_keys = ["level", "lvl", "severity"];

        for key in level_keys {
            if let Some(value) = obj.get(key) {
                if let Some(s) = value.as_str() {
                    return self.parse_level_string(s);
                }
            }
        }

        self.default_level
    }

    /// Extracts message from JSON object.
    fn extract_message(obj: &serde_json::Map<String, serde_json::Value>) -> String {
        let msg_keys = ["msg", "message", "text"];

        for key in msg_keys {
            if let Some(value) = obj.get(key) {
                if let Some(s) = value.as_str() {
                    return s.to_string();
                }
            }
        }

        // Fall back to stringifying the whole object
        serde_json::to_string(obj).unwrap_or_default()
    }

    /// Parses a plain text log line, detecting level prefix.
    fn parse_plain(&self, line: &str) -> (LogLevel, String) {
        let upper = line.to_uppercase();

        // Check for common level prefixes
        let prefixes = [
            ("ERROR", LogLevel::Error),
            ("ERR", LogLevel::Error),
            ("WARN", LogLevel::Warn),
            ("WARNING", LogLevel::Warn),
            ("INFO", LogLevel::Info),
            ("DEBUG", LogLevel::Debug),
            ("DBG", LogLevel::Debug),
            ("TRACE", LogLevel::Trace),
            ("TRC", LogLevel::Trace),
        ];

        for (prefix, level) in prefixes {
            if upper.starts_with(prefix) {
                // Check if followed by separator
                let rest = &line[prefix.len()..];
                if rest.starts_with(':')
                    || rest.starts_with(' ')
                    || rest.starts_with(']')
                    || rest.starts_with('|')
                {
                    let message = rest.trim_start_matches([':', ' ', '|']);
                    return (level, message.to_string());
                }
            }
            // Also check for bracketed format like [ERROR]
            let bracketed = format!("[{prefix}]");
            if upper.starts_with(&bracketed) {
                let message = line[bracketed.len()..].trim();
                return (level, message.to_string());
            }
        }

        (self.default_level, line.to_string())
    }

    /// Parses a level string to `LogLevel`.
    fn parse_level_string(&self, s: &str) -> LogLevel {
        match s.to_lowercase().as_str() {
            "trace" | "trc" => LogLevel::Trace,
            "debug" | "dbg" => LogLevel::Debug,
            "info" | "inf" => LogLevel::Info,
            "warn" | "warning" | "wrn" => LogLevel::Warn,
            "error" | "err" | "fatal" => LogLevel::Error,
            _ => self.default_level,
        }
    }
}

/// Collects logs from a workload's stdout/stderr.
pub struct WorkloadLogCollector {
    workload_id: Uuid,
    node_id: Uuid,
    store: Arc<LogStore>,
    parser: LineParser,
}

impl WorkloadLogCollector {
    /// Creates a new workload log collector.
    #[must_use]
    pub const fn new(workload_id: Uuid, node_id: Uuid, store: Arc<LogStore>) -> Self {
        Self {
            workload_id,
            node_id,
            store,
            parser: LineParser::new(),
        }
    }

    /// Creates a collector with a custom parser.
    #[must_use]
    pub const fn with_parser(
        workload_id: Uuid,
        node_id: Uuid,
        store: Arc<LogStore>,
        parser: LineParser,
    ) -> Self {
        Self {
            workload_id,
            node_id,
            store,
            parser,
        }
    }

    /// Collects a single line from stdout or stderr.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn collect_line(&self, line: &str, source: LogSource) -> Result<LogId> {
        let parsed = self.parser.parse(line);

        // Adjust level for stderr
        let level = if source == LogSource::Stderr && parsed.level == LogLevel::Info {
            LogLevel::Warn
        } else {
            parsed.level
        };

        let mut fields = parsed.fields;
        fields.insert(
            "source".to_string(),
            serde_json::json!(match source {
                LogSource::Stdout => "stdout",
                LogSource::Stderr => "stderr",
                LogSource::System => "system",
            }),
        );
        if parsed.is_json {
            fields.insert("format".to_string(), serde_json::json!("json"));
        }

        let entry = LogEntry {
            id: LogId(0), // Will be assigned by store
            timestamp: Utc::now(),
            level,
            message: parsed.message,
            workload_id: self.workload_id,
            node_id: self.node_id,
            fields,
        };

        self.store.append(entry)
    }

    /// Collects multiple lines.
    ///
    /// # Errors
    ///
    /// Returns an error if any line fails to be stored.
    pub fn collect_lines(&self, lines: &str, source: LogSource) -> Result<Vec<LogId>> {
        let mut ids = Vec::new();
        for line in lines.lines() {
            if !line.trim().is_empty() {
                ids.push(self.collect_line(line, source)?);
            }
        }
        Ok(ids)
    }

    /// Returns the workload ID.
    #[must_use]
    pub const fn workload_id(&self) -> Uuid {
        self.workload_id
    }

    /// Returns the node ID.
    #[must_use]
    pub const fn node_id(&self) -> Uuid {
        self.node_id
    }
}

/// Collects node-level events and logs.
pub struct NodeLogCollector {
    node_id: Uuid,
    store: Arc<LogStore>,
    parser: LineParser,
}

impl NodeLogCollector {
    /// Creates a new node log collector.
    #[must_use]
    pub const fn new(node_id: Uuid, store: Arc<LogStore>) -> Self {
        Self {
            node_id,
            store,
            parser: LineParser::new(),
        }
    }

    /// Logs a node event.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn log_event(
        &self,
        level: LogLevel,
        message: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
    ) -> Result<LogId> {
        let mut all_fields = fields;
        all_fields.insert("source".to_string(), serde_json::json!("node"));

        // Use a nil UUID for node-level logs (no specific workload)
        let entry = LogEntry {
            id: LogId(0),
            timestamp: Utc::now(),
            level,
            message: message.into(),
            workload_id: Uuid::nil(),
            node_id: self.node_id,
            fields: all_fields,
        };

        self.store.append(entry)
    }

    /// Logs an info event.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn info(&self, message: impl Into<String>) -> Result<LogId> {
        self.log_event(LogLevel::Info, message, HashMap::new())
    }

    /// Logs a warning event.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn warn(&self, message: impl Into<String>) -> Result<LogId> {
        self.log_event(LogLevel::Warn, message, HashMap::new())
    }

    /// Logs an error event.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn error(&self, message: impl Into<String>) -> Result<LogId> {
        self.log_event(LogLevel::Error, message, HashMap::new())
    }

    /// Collects a line from node-level output.
    ///
    /// # Errors
    ///
    /// Returns an error if the store rejects the entry.
    pub fn collect_line(&self, line: &str) -> Result<LogId> {
        let parsed = self.parser.parse(line);

        let mut fields = parsed.fields;
        fields.insert("source".to_string(), serde_json::json!("node"));

        let entry = LogEntry {
            id: LogId(0),
            timestamp: Utc::now(),
            level: parsed.level,
            message: parsed.message,
            workload_id: Uuid::nil(),
            node_id: self.node_id,
            fields,
        };

        self.store.append(entry)
    }

    /// Returns the node ID.
    #[must_use]
    pub const fn node_id(&self) -> Uuid {
        self.node_id
    }
}

/// Builder for creating log entries with custom timestamps.
pub struct LogEntryFactory {
    workload_id: Uuid,
    node_id: Uuid,
}

impl LogEntryFactory {
    /// Creates a new factory.
    #[must_use]
    pub const fn new(workload_id: Uuid, node_id: Uuid) -> Self {
        Self {
            workload_id,
            node_id,
        }
    }

    /// Creates a log entry with the given parameters.
    #[must_use]
    pub fn create(
        &self,
        timestamp: DateTime<Utc>,
        level: LogLevel,
        message: impl Into<String>,
    ) -> LogEntry {
        LogEntry {
            id: LogId(0),
            timestamp,
            level,
            message: message.into(),
            workload_id: self.workload_id,
            node_id: self.node_id,
            fields: HashMap::new(),
        }
    }

    /// Creates a log entry with fields.
    #[must_use]
    pub fn create_with_fields(
        &self,
        timestamp: DateTime<Utc>,
        level: LogLevel,
        message: impl Into<String>,
        fields: HashMap<String, serde_json::Value>,
    ) -> LogEntry {
        LogEntry {
            id: LogId(0),
            timestamp,
            level,
            message: message.into(),
            workload_id: self.workload_id,
            node_id: self.node_id,
            fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::LogStore;
    use std::time::Duration;

    fn make_store() -> Arc<LogStore> {
        Arc::new(LogStore::new(1000, Duration::from_secs(3600)))
    }

    // ===========================================
    // LineParser Tests
    // ===========================================

    #[test]
    fn parser_detects_json() {
        let parser = LineParser::new();
        let result = parser.parse(r#"{"level":"error","msg":"something failed"}"#);

        assert!(result.is_json);
        assert_eq!(result.level, LogLevel::Error);
        assert_eq!(result.message, "something failed");
    }

    #[test]
    fn parser_extracts_json_fields() {
        let parser = LineParser::new();
        let result = parser.parse(r#"{"level":"info","msg":"test","user_id":"123","action":"login"}"#);

        assert!(result.is_json);
        assert!(result.fields.contains_key("user_id"));
        assert!(result.fields.contains_key("action"));
        assert!(!result.fields.contains_key("level"));
        assert!(!result.fields.contains_key("msg"));
    }

    #[test]
    fn parser_handles_json_level_variations() {
        let parser = LineParser::new();

        let result = parser.parse(r#"{"lvl":"warn","message":"test"}"#);
        assert_eq!(result.level, LogLevel::Warn);

        let result = parser.parse(r#"{"severity":"debug","text":"test"}"#);
        assert_eq!(result.level, LogLevel::Debug);
    }

    #[test]
    fn parser_detects_plain_level_prefix() {
        let parser = LineParser::new();

        let result = parser.parse("ERROR: connection failed");
        assert_eq!(result.level, LogLevel::Error);
        assert_eq!(result.message, "connection failed");

        let result = parser.parse("WARN something is wrong");
        assert_eq!(result.level, LogLevel::Warn);
        assert_eq!(result.message, "something is wrong");
    }

    #[test]
    fn parser_detects_bracketed_level() {
        let parser = LineParser::new();

        let result = parser.parse("[ERROR] something bad");
        assert_eq!(result.level, LogLevel::Error);
        assert_eq!(result.message, "something bad");

        let result = parser.parse("[DEBUG] detailed info");
        assert_eq!(result.level, LogLevel::Debug);
    }

    #[test]
    fn parser_handles_plain_text() {
        let parser = LineParser::new();
        let result = parser.parse("Just a plain message");

        assert!(!result.is_json);
        assert_eq!(result.level, LogLevel::Info);
        assert_eq!(result.message, "Just a plain message");
    }

    #[test]
    fn parser_custom_default_level() {
        let parser = LineParser::with_default_level(LogLevel::Debug);
        let result = parser.parse("plain message");

        assert_eq!(result.level, LogLevel::Debug);
    }

    #[test]
    fn parser_handles_malformed_json() {
        let parser = LineParser::new();
        let result = parser.parse("{not valid json");

        assert!(!result.is_json);
        assert_eq!(result.message, "{not valid json");
    }

    // ===========================================
    // WorkloadLogCollector Tests
    // ===========================================

    #[test]
    fn workload_collector_collects_stdout() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        let result = collector.collect_line("INFO: application started", LogSource::Stdout);
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Info);
    }

    #[test]
    fn workload_collector_elevates_stderr_level() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        // Plain message from stderr should be elevated to Warn
        let result = collector.collect_line("something happened", LogSource::Stderr);
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries[0].level, LogLevel::Warn);
    }

    #[test]
    fn workload_collector_preserves_error_level() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        // Explicit error level should be preserved
        let result = collector.collect_line("ERROR: critical failure", LogSource::Stderr);
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries[0].level, LogLevel::Error);
    }

    #[test]
    fn workload_collector_adds_source_field() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        let _ = collector.collect_line("test", LogSource::Stdout);

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert!(entries[0].fields.contains_key("source"));
        assert_eq!(entries[0].fields["source"], serde_json::json!("stdout"));
    }

    #[test]
    fn workload_collector_collects_multiple_lines() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        let lines = "line 1\nline 2\nline 3\n";
        let result = collector.collect_lines(lines, LogSource::Stdout);

        assert!(result.is_ok());
        if let Ok(ids) = result {
            assert_eq!(ids.len(), 3);
        }
    }

    #[test]
    fn workload_collector_skips_empty_lines() {
        let store = make_store();
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let collector = WorkloadLogCollector::new(workload_id, node_id, Arc::clone(&store));

        let lines = "line 1\n\n   \nline 2\n";
        let result = collector.collect_lines(lines, LogSource::Stdout);

        assert!(result.is_ok());
        if let Ok(ids) = result {
            assert_eq!(ids.len(), 2);
        }
    }

    // ===========================================
    // NodeLogCollector Tests
    // ===========================================

    #[test]
    fn node_collector_logs_event() {
        let store = make_store();
        let node_id = Uuid::new_v4();
        let collector = NodeLogCollector::new(node_id, Arc::clone(&store));

        let result = collector.info("Node started");
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Info);
        assert_eq!(entries[0].node_id, node_id);
        assert_eq!(entries[0].workload_id, Uuid::nil());
    }

    #[test]
    fn node_collector_logs_warning() {
        let store = make_store();
        let node_id = Uuid::new_v4();
        let collector = NodeLogCollector::new(node_id, Arc::clone(&store));

        let result = collector.warn("Low disk space");
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries[0].level, LogLevel::Warn);
    }

    #[test]
    fn node_collector_logs_error() {
        let store = make_store();
        let node_id = Uuid::new_v4();
        let collector = NodeLogCollector::new(node_id, Arc::clone(&store));

        let result = collector.error("Connection lost");
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries[0].level, LogLevel::Error);
    }

    #[test]
    fn node_collector_logs_with_fields() {
        let store = make_store();
        let node_id = Uuid::new_v4();
        let collector = NodeLogCollector::new(node_id, Arc::clone(&store));

        let mut fields = HashMap::new();
        fields.insert("disk_usage".to_string(), serde_json::json!(85.5));
        fields.insert("partition".to_string(), serde_json::json!("/data"));

        let result = collector.log_event(LogLevel::Warn, "Disk usage high", fields);
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert!(entries[0].fields.contains_key("disk_usage"));
        assert!(entries[0].fields.contains_key("partition"));
    }

    #[test]
    fn node_collector_collects_line() {
        let store = make_store();
        let node_id = Uuid::new_v4();
        let collector = NodeLogCollector::new(node_id, Arc::clone(&store));

        let result = collector.collect_line("ERROR: something failed");
        assert!(result.is_ok());

        let entries = store.query(&crate::types::LogFilter::default(), 10);
        assert_eq!(entries[0].level, LogLevel::Error);
    }

    // ===========================================
    // LogEntryFactory Tests
    // ===========================================

    #[test]
    fn factory_creates_entry() {
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let factory = LogEntryFactory::new(workload_id, node_id);

        let entry = factory.create(Utc::now(), LogLevel::Info, "test message");

        assert_eq!(entry.workload_id, workload_id);
        assert_eq!(entry.node_id, node_id);
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.message, "test message");
    }

    #[test]
    fn factory_creates_entry_with_fields() {
        let workload_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let factory = LogEntryFactory::new(workload_id, node_id);

        let mut fields = HashMap::new();
        fields.insert("key".to_string(), serde_json::json!("value"));

        let entry = factory.create_with_fields(Utc::now(), LogLevel::Debug, "test", fields);

        assert!(entry.fields.contains_key("key"));
    }
}
