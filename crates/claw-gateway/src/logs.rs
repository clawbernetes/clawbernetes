//! Workload log storage for collecting and retrieving logs from nodes.
//!
//! This module provides a bounded log store that accumulates stdout/stderr
//! from workloads and allows retrieval with optional tail limits.

use std::collections::{HashMap, VecDeque};

use claw_proto::WorkloadId;

/// Default maximum lines to store per workload per stream.
pub const DEFAULT_MAX_LINES: usize = 10_000;

/// Stored logs for a single workload.
#[derive(Debug, Clone, Default)]
pub struct WorkloadLogs {
    /// Stdout log lines.
    stdout: VecDeque<String>,
    /// Stderr log lines.
    stderr: VecDeque<String>,
    /// Maximum lines to store per stream.
    max_lines: usize,
}

impl WorkloadLogs {
    /// Create a new workload log store with default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_LINES)
    }

    /// Create a new workload log store with custom capacity.
    #[must_use]
    pub fn with_capacity(max_lines: usize) -> Self {
        Self {
            stdout: VecDeque::new(),
            stderr: VecDeque::new(),
            max_lines,
        }
    }

    /// Append stdout lines.
    ///
    /// Older lines are dropped if capacity is exceeded.
    pub fn append_stdout(&mut self, lines: impl IntoIterator<Item = impl Into<String>>) {
        for line in lines {
            self.stdout.push_back(line.into());
            if self.stdout.len() > self.max_lines {
                self.stdout.pop_front();
            }
        }
    }

    /// Append stderr lines.
    ///
    /// Older lines are dropped if capacity is exceeded.
    pub fn append_stderr(&mut self, lines: impl IntoIterator<Item = impl Into<String>>) {
        for line in lines {
            self.stderr.push_back(line.into());
            if self.stderr.len() > self.max_lines {
                self.stderr.pop_front();
            }
        }
    }

    /// Get stdout lines, optionally limited to the last N.
    #[must_use]
    pub fn get_stdout(&self, tail: Option<usize>) -> Vec<String> {
        match tail {
            Some(n) => self.stdout.iter().rev().take(n).rev().cloned().collect(),
            None => self.stdout.iter().cloned().collect(),
        }
    }

    /// Get stderr lines, optionally limited to the last N.
    #[must_use]
    pub fn get_stderr(&self, tail: Option<usize>) -> Vec<String> {
        match tail {
            Some(n) => self.stderr.iter().rev().take(n).rev().cloned().collect(),
            None => self.stderr.iter().cloned().collect(),
        }
    }

    /// Get the number of stdout lines.
    #[must_use]
    pub fn stdout_len(&self) -> usize {
        self.stdout.len()
    }

    /// Get the number of stderr lines.
    #[must_use]
    pub fn stderr_len(&self) -> usize {
        self.stderr.len()
    }

    /// Check if both streams are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stdout.is_empty() && self.stderr.is_empty()
    }

    /// Clear all logs.
    pub fn clear(&mut self) {
        self.stdout.clear();
        self.stderr.clear();
    }
}

/// Central log storage for all workloads.
#[derive(Debug, Default)]
pub struct WorkloadLogStore {
    /// Logs indexed by workload ID.
    logs: HashMap<WorkloadId, WorkloadLogs>,
    /// Maximum lines per workload per stream.
    max_lines_per_workload: usize,
}

impl WorkloadLogStore {
    /// Create a new log store with default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_LINES)
    }

    /// Create a new log store with custom capacity per workload.
    #[must_use]
    pub fn with_capacity(max_lines_per_workload: usize) -> Self {
        Self {
            logs: HashMap::new(),
            max_lines_per_workload,
        }
    }

    /// Append stdout lines for a workload.
    pub fn append_stdout(
        &mut self,
        workload_id: WorkloadId,
        lines: impl IntoIterator<Item = impl Into<String>>,
    ) {
        let logs = self
            .logs
            .entry(workload_id)
            .or_insert_with(|| WorkloadLogs::with_capacity(self.max_lines_per_workload));
        logs.append_stdout(lines);
    }

    /// Append stderr lines for a workload.
    pub fn append_stderr(
        &mut self,
        workload_id: WorkloadId,
        lines: impl IntoIterator<Item = impl Into<String>>,
    ) {
        let logs = self
            .logs
            .entry(workload_id)
            .or_insert_with(|| WorkloadLogs::with_capacity(self.max_lines_per_workload));
        logs.append_stderr(lines);
    }

    /// Get logs for a workload.
    #[must_use]
    pub fn get_logs(&self, workload_id: WorkloadId) -> Option<&WorkloadLogs> {
        self.logs.get(&workload_id)
    }

    /// Get stdout and stderr with optional tail limit.
    #[must_use]
    pub fn get_logs_with_tail(
        &self,
        workload_id: WorkloadId,
        tail: Option<usize>,
    ) -> Option<(Vec<String>, Vec<String>)> {
        self.logs
            .get(&workload_id)
            .map(|logs| (logs.get_stdout(tail), logs.get_stderr(tail)))
    }

    /// Clear logs for a specific workload.
    pub fn clear(&mut self, workload_id: WorkloadId) {
        self.logs.remove(&workload_id);
    }

    /// Clear all logs.
    pub fn clear_all(&mut self) {
        self.logs.clear();
    }

    /// Get the number of workloads with logs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.logs.len()
    }

    /// Check if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.logs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== WorkloadLogs Tests ====================

    #[test]
    fn workload_logs_new_is_empty() {
        let logs = WorkloadLogs::new();
        assert!(logs.is_empty());
        assert_eq!(logs.stdout_len(), 0);
        assert_eq!(logs.stderr_len(), 0);
    }

    #[test]
    fn workload_logs_append_stdout() {
        let mut logs = WorkloadLogs::new();
        logs.append_stdout(["line 1", "line 2"]);

        assert_eq!(logs.stdout_len(), 2);
        assert_eq!(logs.get_stdout(None), vec!["line 1", "line 2"]);
    }

    #[test]
    fn workload_logs_append_stderr() {
        let mut logs = WorkloadLogs::new();
        logs.append_stderr(["error 1"]);

        assert_eq!(logs.stderr_len(), 1);
        assert_eq!(logs.get_stderr(None), vec!["error 1"]);
    }

    #[test]
    fn workload_logs_tail() {
        let mut logs = WorkloadLogs::new();
        logs.append_stdout(["a", "b", "c", "d", "e"]);

        assert_eq!(logs.get_stdout(Some(2)), vec!["d", "e"]);
        assert_eq!(logs.get_stdout(Some(10)), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn workload_logs_truncates_at_capacity() {
        let mut logs = WorkloadLogs::with_capacity(3);
        logs.append_stdout(["a", "b", "c", "d", "e"]);

        // Should only have the last 3 lines
        assert_eq!(logs.stdout_len(), 3);
        assert_eq!(logs.get_stdout(None), vec!["c", "d", "e"]);
    }

    #[test]
    fn workload_logs_clear() {
        let mut logs = WorkloadLogs::new();
        logs.append_stdout(["line"]);
        logs.append_stderr(["error"]);

        assert!(!logs.is_empty());
        logs.clear();
        assert!(logs.is_empty());
    }

    // ==================== WorkloadLogStore Tests ====================

    #[test]
    fn log_store_new_is_empty() {
        let store = WorkloadLogStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn log_store_append_and_get() {
        let mut store = WorkloadLogStore::new();
        let wid = WorkloadId::new();

        store.append_stdout(wid, ["stdout line"]);
        store.append_stderr(wid, ["stderr line"]);

        let logs = store.get_logs(wid).unwrap();
        assert_eq!(logs.get_stdout(None), vec!["stdout line"]);
        assert_eq!(logs.get_stderr(None), vec!["stderr line"]);
    }

    #[test]
    fn log_store_get_with_tail() {
        let mut store = WorkloadLogStore::new();
        let wid = WorkloadId::new();

        store.append_stdout(wid, ["a", "b", "c", "d"]);
        store.append_stderr(wid, ["e1", "e2"]);

        let (stdout, stderr) = store.get_logs_with_tail(wid, Some(2)).unwrap();
        assert_eq!(stdout, vec!["c", "d"]);
        assert_eq!(stderr, vec!["e1", "e2"]);
    }

    #[test]
    fn log_store_get_nonexistent() {
        let store = WorkloadLogStore::new();
        assert!(store.get_logs(WorkloadId::new()).is_none());
    }

    #[test]
    fn log_store_clear_workload() {
        let mut store = WorkloadLogStore::new();
        let wid1 = WorkloadId::new();
        let wid2 = WorkloadId::new();

        store.append_stdout(wid1, ["log 1"]);
        store.append_stdout(wid2, ["log 2"]);

        assert_eq!(store.len(), 2);
        store.clear(wid1);
        assert_eq!(store.len(), 1);
        assert!(store.get_logs(wid1).is_none());
        assert!(store.get_logs(wid2).is_some());
    }

    #[test]
    fn log_store_clear_all() {
        let mut store = WorkloadLogStore::new();
        store.append_stdout(WorkloadId::new(), ["log 1"]);
        store.append_stdout(WorkloadId::new(), ["log 2"]);

        assert!(!store.is_empty());
        store.clear_all();
        assert!(store.is_empty());
    }

    #[test]
    fn log_store_multiple_workloads() {
        let mut store = WorkloadLogStore::new();
        let wid1 = WorkloadId::new();
        let wid2 = WorkloadId::new();

        store.append_stdout(wid1, ["wl1 stdout"]);
        store.append_stdout(wid2, ["wl2 stdout"]);

        assert_eq!(store.len(), 2);
        assert_eq!(
            store.get_logs(wid1).unwrap().get_stdout(None),
            vec!["wl1 stdout"]
        );
        assert_eq!(
            store.get_logs(wid2).unwrap().get_stdout(None),
            vec!["wl2 stdout"]
        );
    }

    #[test]
    fn log_store_custom_capacity() {
        let mut store = WorkloadLogStore::with_capacity(2);
        let wid = WorkloadId::new();

        store.append_stdout(wid, ["a", "b", "c", "d"]);

        let logs = store.get_logs(wid).unwrap();
        assert_eq!(logs.stdout_len(), 2);
        assert_eq!(logs.get_stdout(None), vec!["c", "d"]);
    }
}
