//! Workload lifecycle events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::WorkloadId;

/// Event types for workload lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadEventKind {
    /// Workload was created and queued.
    Created,
    /// Workload started executing.
    Started,
    /// Workload is running.
    Running,
    /// Workload completed successfully.
    Completed,
    /// Workload failed with an error.
    Failed,
    /// Workload was stopped by request.
    Stopped,
}

impl std::fmt::Display for WorkloadEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Created => "created",
            Self::Started => "started",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        };
        write!(f, "{s}")
    }
}

/// Metadata associated with an event.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventMetadata {
    /// Optional message describing the event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional exit code (for completion/failure events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Optional error details (for failure events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// GPU IDs involved in the event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpu_ids: Vec<u32>,
    /// Node ID where the event occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

impl EventMetadata {
    /// Create new empty metadata.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the message.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set the exit code.
    #[must_use]
    pub const fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    /// Set the error details.
    #[must_use]
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set GPU IDs.
    #[must_use]
    pub fn with_gpu_ids(mut self, gpu_ids: Vec<u32>) -> Self {
        self.gpu_ids = gpu_ids;
        self
    }

    /// Set node ID.
    #[must_use]
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }
}

/// A workload lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadEvent {
    /// The workload this event is for.
    pub workload_id: WorkloadId,
    /// The kind of event.
    pub kind: WorkloadEventKind,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: EventMetadata,
}

impl WorkloadEvent {
    /// Create a new event.
    #[must_use]
    pub fn new(workload_id: WorkloadId, kind: WorkloadEventKind) -> Self {
        Self {
            workload_id,
            kind,
            timestamp: Utc::now(),
            metadata: EventMetadata::new(),
        }
    }

    /// Create an event with a specific timestamp.
    #[must_use]
    pub fn at(workload_id: WorkloadId, kind: WorkloadEventKind, timestamp: DateTime<Utc>) -> Self {
        Self {
            workload_id,
            kind,
            timestamp,
            metadata: EventMetadata::new(),
        }
    }

    /// Create a "created" event.
    #[must_use]
    pub fn created(workload_id: WorkloadId) -> Self {
        Self::new(workload_id, WorkloadEventKind::Created)
    }

    /// Create a "started" event.
    #[must_use]
    pub fn started(workload_id: WorkloadId) -> Self {
        Self::new(workload_id, WorkloadEventKind::Started)
    }

    /// Create a "running" event.
    #[must_use]
    pub fn running(workload_id: WorkloadId) -> Self {
        Self::new(workload_id, WorkloadEventKind::Running)
    }

    /// Create a "completed" event.
    #[must_use]
    pub fn completed(workload_id: WorkloadId, exit_code: i32) -> Self {
        Self {
            workload_id,
            kind: WorkloadEventKind::Completed,
            timestamp: Utc::now(),
            metadata: EventMetadata::new().with_exit_code(exit_code),
        }
    }

    /// Create a "failed" event.
    #[must_use]
    pub fn failed(workload_id: WorkloadId, error: impl Into<String>) -> Self {
        Self {
            workload_id,
            kind: WorkloadEventKind::Failed,
            timestamp: Utc::now(),
            metadata: EventMetadata::new().with_error(error),
        }
    }

    /// Create a "stopped" event.
    #[must_use]
    pub fn stopped(workload_id: WorkloadId) -> Self {
        Self::new(workload_id, WorkloadEventKind::Stopped)
    }

    /// Add metadata to the event.
    #[must_use]
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a message to the metadata.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.metadata.message = Some(message.into());
        self
    }

    /// Add GPU IDs to the metadata.
    #[must_use]
    pub fn with_gpu_ids(mut self, gpu_ids: Vec<u32>) -> Self {
        self.metadata.gpu_ids = gpu_ids;
        self
    }

    /// Add node ID to the metadata.
    #[must_use]
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.metadata.node_id = Some(node_id.into());
        self
    }

    /// Serialize the event to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, crate::ProtoError> {
        serde_json::to_string(self).map_err(|e| crate::ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize an event from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, crate::ProtoError> {
        serde_json::from_str(json).map_err(|e| crate::ProtoError::Decoding(e.to_string()))
    }
}

/// Ordered sequence of events for a workload.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventLog {
    events: Vec<WorkloadEvent>,
}

impl EventLog {
    /// Create a new empty event log.
    #[must_use]
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Add an event to the log.
    pub fn push(&mut self, event: WorkloadEvent) {
        self.events.push(event);
    }

    /// Get all events.
    #[must_use]
    pub fn events(&self) -> &[WorkloadEvent] {
        &self.events
    }

    /// Get the latest event, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&WorkloadEvent> {
        self.events.last()
    }

    /// Get events of a specific kind.
    #[must_use]
    pub fn events_of_kind(&self, kind: WorkloadEventKind) -> Vec<&WorkloadEvent> {
        self.events.iter().filter(|e| e.kind == kind).collect()
    }

    /// Check if the log contains an event of a specific kind.
    #[must_use]
    pub fn has_event(&self, kind: WorkloadEventKind) -> bool {
        self.events.iter().any(|e| e.kind == kind)
    }

    /// Get the number of events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get events within a time range.
    #[must_use]
    pub fn events_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&WorkloadEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== WorkloadEventKind Tests ====================

    #[test]
    fn test_event_kind_display() {
        assert_eq!(WorkloadEventKind::Created.to_string(), "created");
        assert_eq!(WorkloadEventKind::Started.to_string(), "started");
        assert_eq!(WorkloadEventKind::Running.to_string(), "running");
        assert_eq!(WorkloadEventKind::Completed.to_string(), "completed");
        assert_eq!(WorkloadEventKind::Failed.to_string(), "failed");
        assert_eq!(WorkloadEventKind::Stopped.to_string(), "stopped");
    }

    #[test]
    fn test_event_kind_serialization() {
        let kind = WorkloadEventKind::Running;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"running\"");

        let deserialized: WorkloadEventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, deserialized);
    }

    // ==================== EventMetadata Tests ====================

    #[test]
    fn test_event_metadata_default() {
        let meta = EventMetadata::default();
        assert!(meta.message.is_none());
        assert!(meta.exit_code.is_none());
        assert!(meta.error.is_none());
        assert!(meta.gpu_ids.is_empty());
        assert!(meta.node_id.is_none());
    }

    #[test]
    fn test_event_metadata_builder() {
        let meta = EventMetadata::new()
            .with_message("Process started")
            .with_exit_code(0)
            .with_gpu_ids(vec![0, 1])
            .with_node_id("node-123");

        assert_eq!(meta.message, Some("Process started".to_string()));
        assert_eq!(meta.exit_code, Some(0));
        assert_eq!(meta.gpu_ids, vec![0, 1]);
        assert_eq!(meta.node_id, Some("node-123".to_string()));
    }

    #[test]
    fn test_event_metadata_with_error() {
        let meta = EventMetadata::new().with_error("OOM killed");
        assert_eq!(meta.error, Some("OOM killed".to_string()));
    }

    // ==================== WorkloadEvent Tests ====================

    #[test]
    fn test_workload_event_new() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::new(id, WorkloadEventKind::Created);

        assert_eq!(event.workload_id, id);
        assert_eq!(event.kind, WorkloadEventKind::Created);
        assert!(event.metadata.message.is_none());
    }

    #[test]
    fn test_workload_event_at() {
        let id = WorkloadId::new();
        let ts = Utc::now() - chrono::Duration::hours(1);
        let event = WorkloadEvent::at(id, WorkloadEventKind::Started, ts);

        assert_eq!(event.timestamp, ts);
    }

    #[test]
    fn test_workload_event_created() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::created(id);
        assert_eq!(event.kind, WorkloadEventKind::Created);
    }

    #[test]
    fn test_workload_event_started() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::started(id);
        assert_eq!(event.kind, WorkloadEventKind::Started);
    }

    #[test]
    fn test_workload_event_running() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::running(id);
        assert_eq!(event.kind, WorkloadEventKind::Running);
    }

    #[test]
    fn test_workload_event_completed() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::completed(id, 0);

        assert_eq!(event.kind, WorkloadEventKind::Completed);
        assert_eq!(event.metadata.exit_code, Some(0));
    }

    #[test]
    fn test_workload_event_failed() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::failed(id, "Connection timeout");

        assert_eq!(event.kind, WorkloadEventKind::Failed);
        assert_eq!(event.metadata.error, Some("Connection timeout".to_string()));
    }

    #[test]
    fn test_workload_event_stopped() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::stopped(id);
        assert_eq!(event.kind, WorkloadEventKind::Stopped);
    }

    #[test]
    fn test_workload_event_with_metadata() {
        let id = WorkloadId::new();
        let meta = EventMetadata::new()
            .with_message("Ready to serve")
            .with_gpu_ids(vec![0]);

        let event = WorkloadEvent::running(id).with_metadata(meta);

        assert_eq!(event.metadata.message, Some("Ready to serve".to_string()));
        assert_eq!(event.metadata.gpu_ids, vec![0]);
    }

    #[test]
    fn test_workload_event_with_message() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::started(id).with_message("Container pulled");

        assert_eq!(event.metadata.message, Some("Container pulled".to_string()));
    }

    #[test]
    fn test_workload_event_with_gpu_ids() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::running(id).with_gpu_ids(vec![0, 1, 2]);

        assert_eq!(event.metadata.gpu_ids, vec![0, 1, 2]);
    }

    #[test]
    fn test_workload_event_with_node_id() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::started(id).with_node_id("node-abc-123");

        assert_eq!(event.metadata.node_id, Some("node-abc-123".to_string()));
    }

    #[test]
    fn test_workload_event_json_round_trip() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::completed(id, 0)
            .with_message("Success")
            .with_gpu_ids(vec![0]);

        let json = event.to_json().unwrap();
        let restored = WorkloadEvent::from_json(&json).unwrap();

        assert_eq!(event.workload_id, restored.workload_id);
        assert_eq!(event.kind, restored.kind);
        assert_eq!(event.metadata.exit_code, restored.metadata.exit_code);
        assert_eq!(event.metadata.message, restored.metadata.message);
    }

    #[test]
    fn test_workload_event_json_contains_fields() {
        let id = WorkloadId::new();
        let event = WorkloadEvent::failed(id, "OOM");

        let json = event.to_json().unwrap();
        assert!(json.contains("\"kind\":\"failed\""));
        assert!(json.contains("\"error\":\"OOM\""));
    }

    // ==================== EventLog Tests ====================

    #[test]
    fn test_event_log_new() {
        let log = EventLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_event_log_push() {
        let id = WorkloadId::new();
        let mut log = EventLog::new();

        log.push(WorkloadEvent::created(id));
        log.push(WorkloadEvent::started(id));

        assert_eq!(log.len(), 2);
        assert!(!log.is_empty());
    }

    #[test]
    fn test_event_log_latest() {
        let id = WorkloadId::new();
        let mut log = EventLog::new();

        assert!(log.latest().is_none());

        log.push(WorkloadEvent::created(id));
        log.push(WorkloadEvent::running(id));

        let latest = log.latest().unwrap();
        assert_eq!(latest.kind, WorkloadEventKind::Running);
    }

    #[test]
    fn test_event_log_events_of_kind() {
        let id = WorkloadId::new();
        let mut log = EventLog::new();

        log.push(WorkloadEvent::created(id));
        log.push(WorkloadEvent::started(id));
        log.push(WorkloadEvent::running(id));
        log.push(WorkloadEvent::running(id)); // Two running events

        let running = log.events_of_kind(WorkloadEventKind::Running);
        assert_eq!(running.len(), 2);
    }

    #[test]
    fn test_event_log_has_event() {
        let id = WorkloadId::new();
        let mut log = EventLog::new();

        log.push(WorkloadEvent::created(id));

        assert!(log.has_event(WorkloadEventKind::Created));
        assert!(!log.has_event(WorkloadEventKind::Completed));
    }

    #[test]
    fn test_event_log_events_between() {
        let id = WorkloadId::new();
        let now = Utc::now();
        let mut log = EventLog::new();

        let old_event = WorkloadEvent::at(id, WorkloadEventKind::Created, now - chrono::Duration::hours(2));
        let recent_event = WorkloadEvent::at(id, WorkloadEventKind::Started, now - chrono::Duration::minutes(30));
        let current_event = WorkloadEvent::at(id, WorkloadEventKind::Running, now);

        log.push(old_event);
        log.push(recent_event);
        log.push(current_event);

        let range_start = now - chrono::Duration::hours(1);
        let range_end = now;
        let in_range = log.events_between(range_start, range_end);

        assert_eq!(in_range.len(), 2); // recent and current, not old
    }

    #[test]
    fn test_event_log_events() {
        let id = WorkloadId::new();
        let mut log = EventLog::new();

        log.push(WorkloadEvent::created(id));
        log.push(WorkloadEvent::started(id));

        let events = log.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, WorkloadEventKind::Created);
        assert_eq!(events[1].kind, WorkloadEventKind::Started);
    }
}
