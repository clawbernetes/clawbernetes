//! Indexing for fast log lookups.
//!
//! This module provides:
//! - [`LogIndex`] — Multi-dimensional index for log entries
//! - Fast lookups by `workload_id`, `node_id`, and level
//! - Simple text search with inverted index

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::types::{LogId, LogLevel};

/// Multi-dimensional index for fast log lookups.
///
/// Maintains indices for:
/// - Workload ID → Log IDs
/// - Node ID → Log IDs
/// - Log Level → Log IDs
/// - Text tokens → Log IDs (simple inverted index)
#[allow(clippy::struct_field_names)]
pub struct LogIndex {
    /// Index by workload ID
    by_workload: RwLock<HashMap<Uuid, Vec<LogId>>>,
    /// Index by node ID
    by_node: RwLock<HashMap<Uuid, Vec<LogId>>>,
    /// Index by log level
    by_level: RwLock<HashMap<LogLevel, Vec<LogId>>>,
    /// Simple inverted index for text search (lowercase tokens)
    by_token: RwLock<HashMap<String, HashSet<LogId>>>,
}

impl Default for LogIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl LogIndex {
    /// Creates a new empty index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_workload: RwLock::new(HashMap::new()),
            by_node: RwLock::new(HashMap::new()),
            by_level: RwLock::new(HashMap::new()),
            by_token: RwLock::new(HashMap::new()),
        }
    }

    /// Indexes a log entry.
    pub fn insert(
        &self,
        id: LogId,
        workload_id: Uuid,
        node_id: Uuid,
        level: LogLevel,
        message: &str,
    ) {
        // Index by workload
        self.by_workload
            .write()
            .entry(workload_id)
            .or_default()
            .push(id);

        // Index by node
        self.by_node.write().entry(node_id).or_default().push(id);

        // Index by level
        self.by_level.write().entry(level).or_default().push(id);

        // Index tokens for text search
        self.index_message(id, message);
    }

    /// Removes an entry from all indices.
    pub fn remove(
        &self,
        id: LogId,
        workload_id: Uuid,
        node_id: Uuid,
        level: LogLevel,
        message: &str,
    ) {
        // Remove from workload index
        if let Some(ids) = self.by_workload.write().get_mut(&workload_id) {
            ids.retain(|i| *i != id);
        }

        // Remove from node index
        if let Some(ids) = self.by_node.write().get_mut(&node_id) {
            ids.retain(|i| *i != id);
        }

        // Remove from level index
        if let Some(ids) = self.by_level.write().get_mut(&level) {
            ids.retain(|i| *i != id);
        }

        // Remove from token index
        self.remove_message_index(id, message);
    }

    /// Gets all log IDs for a workload.
    #[must_use]
    pub fn by_workload(&self, workload_id: Uuid) -> Vec<LogId> {
        self.by_workload
            .read()
            .get(&workload_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Gets all log IDs for a node.
    #[must_use]
    pub fn by_node(&self, node_id: Uuid) -> Vec<LogId> {
        self.by_node
            .read()
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Gets all log IDs for a level.
    #[must_use]
    pub fn by_level(&self, level: LogLevel) -> Vec<LogId> {
        self.by_level
            .read()
            .get(&level)
            .cloned()
            .unwrap_or_default()
    }

    /// Searches for log IDs containing all tokens in the search string.
    ///
    /// Returns IDs of entries where the message contains the search string
    /// (case-insensitive substring match).
    #[must_use]
    #[allow(clippy::significant_drop_tightening)]
    pub fn search(&self, query: &str) -> HashSet<LogId> {
        let tokens = Self::tokenize(query);
        if tokens.is_empty() {
            return HashSet::new();
        }

        let index = self.by_token.read();
        let mut result: Option<HashSet<LogId>> = None;

        for token in tokens {
            let token_ids = index.get(&token).cloned().unwrap_or_default();

            result = match result {
                None => Some(token_ids),
                Some(current) => Some(current.intersection(&token_ids).copied().collect()),
            };
        }

        result.unwrap_or_default()
    }

    /// Gets IDs matching any of the given workloads.
    #[must_use]
    pub fn by_workloads(&self, workload_ids: &[Uuid]) -> HashSet<LogId> {
        let index = self.by_workload.read();
        let mut result = HashSet::new();

        for id in workload_ids {
            if let Some(ids) = index.get(id) {
                result.extend(ids.iter().copied());
            }
        }

        result
    }

    /// Gets IDs matching any of the given nodes.
    #[must_use]
    pub fn by_nodes(&self, node_ids: &[Uuid]) -> HashSet<LogId> {
        let index = self.by_node.read();
        let mut result = HashSet::new();

        for id in node_ids {
            if let Some(ids) = index.get(id) {
                result.extend(ids.iter().copied());
            }
        }

        result
    }

    /// Gets IDs matching any of the given levels.
    #[must_use]
    pub fn by_levels(&self, levels: &[LogLevel]) -> HashSet<LogId> {
        let index = self.by_level.read();
        let mut result = HashSet::new();

        for level in levels {
            if let Some(ids) = index.get(level) {
                result.extend(ids.iter().copied());
            }
        }

        result
    }

    /// Clears all indices.
    pub fn clear(&self) {
        self.by_workload.write().clear();
        self.by_node.write().clear();
        self.by_level.write().clear();
        self.by_token.write().clear();
    }

    /// Returns the number of indexed entries (approximate, based on workload index).
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_workload
            .read()
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Returns true if the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_workload.read().is_empty()
    }

    /// Tokenizes a message into lowercase tokens.
    fn tokenize(message: &str) -> Vec<String> {
        message
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty() && s.len() >= 2)
            .map(String::from)
            .collect()
    }

    /// Indexes a message's tokens.
    fn index_message(&self, id: LogId, message: &str) {
        let tokens = Self::tokenize(message);
        let mut index = self.by_token.write();

        for token in tokens {
            index.entry(token).or_default().insert(id);
        }
    }

    /// Removes a message's tokens from the index.
    fn remove_message_index(&self, id: LogId, message: &str) {
        let tokens = Self::tokenize(message);
        let mut index = self.by_token.write();

        for token in tokens {
            if let Some(ids) = index.get_mut(&token) {
                ids.remove(&id);
                if ids.is_empty() {
                    index.remove(&token);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_ids() -> (Uuid, Uuid) {
        (Uuid::new_v4(), Uuid::new_v4())
    }

    #[test]
    fn index_insert_and_lookup_by_workload() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test message");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Debug, "another message");

        let results = index.by_workload(workload_id);
        assert_eq!(results.len(), 2);
        assert!(results.contains(&LogId(1)));
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_insert_and_lookup_by_node() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test");
        index.insert(LogId(2), Uuid::new_v4(), node_id, LogLevel::Debug, "test");

        let results = index.by_node(node_id);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn index_insert_and_lookup_by_level() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Error, "error 1");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Error, "error 2");
        index.insert(LogId(3), workload_id, node_id, LogLevel::Info, "info");

        let results = index.by_level(LogLevel::Error);
        assert_eq!(results.len(), 2);
        assert!(results.contains(&LogId(1)));
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_text_search_single_token() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "connection established");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Info, "connection failed");
        index.insert(LogId(3), workload_id, node_id, LogLevel::Info, "shutdown complete");

        let results = index.search("connection");
        assert_eq!(results.len(), 2);
        assert!(results.contains(&LogId(1)));
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_text_search_multiple_tokens() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "connection established successfully");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Info, "connection failed");

        let results = index.search("connection established");
        assert_eq!(results.len(), 1);
        assert!(results.contains(&LogId(1)));
    }

    #[test]
    fn index_text_search_case_insensitive() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "ERROR occurred");

        let results = index.search("error");
        assert_eq!(results.len(), 1);

        let results = index.search("ERROR");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn index_remove_entry() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test message");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Info, "test message");

        index.remove(LogId(1), workload_id, node_id, LogLevel::Info, "test message");

        let results = index.by_workload(workload_id);
        assert_eq!(results.len(), 1);
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_by_workloads_union() {
        let index = LogIndex::new();
        let workload1 = Uuid::new_v4();
        let workload2 = Uuid::new_v4();
        let node_id = Uuid::new_v4();

        index.insert(LogId(1), workload1, node_id, LogLevel::Info, "test");
        index.insert(LogId(2), workload2, node_id, LogLevel::Info, "test");
        index.insert(LogId(3), Uuid::new_v4(), node_id, LogLevel::Info, "test");

        let results = index.by_workloads(&[workload1, workload2]);
        assert_eq!(results.len(), 2);
        assert!(results.contains(&LogId(1)));
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_by_levels_union() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Error, "error");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Warn, "warn");
        index.insert(LogId(3), workload_id, node_id, LogLevel::Info, "info");

        let results = index.by_levels(&[LogLevel::Error, LogLevel::Warn]);
        assert_eq!(results.len(), 2);
        assert!(results.contains(&LogId(1)));
        assert!(results.contains(&LogId(2)));
    }

    #[test]
    fn index_clear() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test");
        assert!(!index.is_empty());

        index.clear();
        assert!(index.is_empty());
    }

    #[test]
    fn index_len() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        assert_eq!(index.len(), 0);

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test");
        index.insert(LogId(2), workload_id, node_id, LogLevel::Info, "test");

        assert_eq!(index.len(), 2);
    }

    #[test]
    fn tokenize_handles_special_characters() {
        let tokens = LogIndex::tokenize("error: connection-failed (timeout=30s)");
        assert!(tokens.contains(&"error".to_string()));
        assert!(tokens.contains(&"connection".to_string()));
        assert!(tokens.contains(&"failed".to_string()));
        assert!(tokens.contains(&"timeout".to_string()));
        assert!(tokens.contains(&"30s".to_string()));
    }

    #[test]
    fn tokenize_filters_short_tokens() {
        let tokens = LogIndex::tokenize("a an the is it to");
        // Single char tokens should be filtered
        assert!(!tokens.contains(&"a".to_string()));
        // Two char tokens like "an", "is", "it", "to" should be included
        assert!(tokens.contains(&"an".to_string()));
        assert!(tokens.contains(&"is".to_string()));
    }

    #[test]
    fn index_empty_search_returns_empty() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test message");

        let results = index.search("");
        assert!(results.is_empty());
    }

    #[test]
    fn index_search_no_match_returns_empty() {
        let index = LogIndex::new();
        let (workload_id, node_id) = make_test_ids();

        index.insert(LogId(1), workload_id, node_id, LogLevel::Info, "test message");

        let results = index.search("nonexistent");
        assert!(results.is_empty());
    }
}
