//! Workload definitions and lifecycle management.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ProtoError;
use crate::types::{WorkloadId, WorkloadState};
use crate::validation::{
    validate_env_key, validate_image, validate_resources, ValidationError, ValidationResult,
};

/// Full workload specification for scheduling and execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadSpec {
    /// Container image reference (e.g., "nginx:latest", "gcr.io/project/image:v1").
    pub image: String,
    /// Command to run in the container.
    pub command: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Number of GPUs required.
    pub gpu_count: u32,
    /// Memory limit in megabytes.
    pub memory_mb: u64,
    /// Number of CPU cores.
    pub cpu_cores: u32,
}

impl WorkloadSpec {
    /// Create a new workload spec with required fields.
    #[must_use]
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            command: Vec::new(),
            env: HashMap::new(),
            gpu_count: 0,
            memory_mb: 512,
            cpu_cores: 1,
        }
    }

    /// Set the command.
    #[must_use]
    pub fn with_command(mut self, command: Vec<String>) -> Self {
        self.command = command;
        self
    }

    /// Add an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set GPU count.
    #[must_use]
    pub const fn with_gpu_count(mut self, count: u32) -> Self {
        self.gpu_count = count;
        self
    }

    /// Set memory limit in MB.
    #[must_use]
    pub const fn with_memory_mb(mut self, memory_mb: u64) -> Self {
        self.memory_mb = memory_mb;
        self
    }

    /// Set CPU cores.
    #[must_use]
    pub const fn with_cpu_cores(mut self, cpu_cores: u32) -> Self {
        self.cpu_cores = cpu_cores;
        self
    }

    /// Validate the workload spec.
    ///
    /// # Errors
    ///
    /// Returns a validation error if any field is invalid.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut result = ValidationResult::new();

        // Validate image
        if let Err(e) = validate_image(&self.image) {
            result.add_error(e);
        }

        // Validate resources
        if let Err(e) = validate_resources(self.memory_mb, self.cpu_cores, self.gpu_count) {
            result.add_error(e);
        }

        // Validate environment keys
        for key in self.env.keys() {
            if let Err(e) = validate_env_key(key) {
                result.add_error(e);
            }
        }

        result.into_result()
    }
}

/// Runtime status of a workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadStatus {
    /// Current state of the workload.
    pub state: WorkloadState,
    /// When the workload started executing.
    pub started_at: Option<DateTime<Utc>>,
    /// When the workload finished executing.
    pub finished_at: Option<DateTime<Utc>>,
    /// Exit code if the workload has completed.
    pub exit_code: Option<i32>,
    /// GPU IDs assigned to this workload.
    pub gpu_ids: Vec<u32>,
}

impl WorkloadStatus {
    /// Create a new pending status.
    #[must_use]
    pub const fn pending() -> Self {
        Self {
            state: WorkloadState::Pending,
            started_at: None,
            finished_at: None,
            exit_code: None,
            gpu_ids: Vec::new(),
        }
    }

    /// Create a running status.
    #[must_use]
    pub const fn running(started_at: DateTime<Utc>, gpu_ids: Vec<u32>) -> Self {
        Self {
            state: WorkloadState::Running,
            started_at: Some(started_at),
            finished_at: None,
            exit_code: None,
            gpu_ids,
        }
    }

    /// Create a completed status.
    #[must_use]
    pub const fn completed(
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
        exit_code: i32,
    ) -> Self {
        Self {
            state: WorkloadState::Completed,
            started_at: Some(started_at),
            finished_at: Some(finished_at),
            exit_code: Some(exit_code),
            gpu_ids: Vec::new(),
        }
    }

    /// Create a failed status.
    #[must_use]
    pub const fn failed(
        started_at: Option<DateTime<Utc>>,
        finished_at: DateTime<Utc>,
        exit_code: Option<i32>,
    ) -> Self {
        Self {
            state: WorkloadState::Failed,
            started_at,
            finished_at: Some(finished_at),
            exit_code,
            gpu_ids: Vec::new(),
        }
    }

    /// Transition to a new state.
    ///
    /// # Errors
    ///
    /// Returns an error if the state transition is invalid.
    pub fn transition_to(&mut self, new_state: WorkloadState) -> Result<(), ProtoError> {
        if !is_valid_transition(self.state, new_state) {
            return Err(ProtoError::Validation(format!(
                "invalid state transition from {} to {}",
                self.state, new_state
            )));
        }

        // Update timestamps based on transition
        match new_state {
            WorkloadState::Running => {
                if self.started_at.is_none() {
                    self.started_at = Some(Utc::now());
                }
            }
            WorkloadState::Completed | WorkloadState::Failed | WorkloadState::Stopped => {
                self.finished_at = Some(Utc::now());
            }
            _ => {}
        }

        self.state = new_state;
        Ok(())
    }

    /// Set the exit code.
    pub const fn set_exit_code(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    /// Set assigned GPU IDs.
    pub fn set_gpu_ids(&mut self, gpu_ids: Vec<u32>) {
        self.gpu_ids = gpu_ids;
    }

    /// Calculate duration if both start and finish times are set.
    #[must_use]
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(finish)) => Some(finish - start),
            _ => None,
        }
    }
}

impl Default for WorkloadStatus {
    fn default() -> Self {
        Self::pending()
    }
}

/// Check if a state transition is valid.
#[must_use]
pub const fn is_valid_transition(from: WorkloadState, to: WorkloadState) -> bool {
    matches!(
        (from, to),
        // From Pending
        (WorkloadState::Pending, WorkloadState::Starting | WorkloadState::Failed | WorkloadState::Stopped | WorkloadState::Pending)
            // From Starting
            | (WorkloadState::Starting, WorkloadState::Running | WorkloadState::Failed | WorkloadState::Stopped)
            // From Running
            | (WorkloadState::Running, WorkloadState::Stopping | WorkloadState::Completed | WorkloadState::Failed | WorkloadState::Running)
            // From Stopping
            | (WorkloadState::Stopping, WorkloadState::Stopped | WorkloadState::Failed)
    )
}

/// Full workload record combining ID, spec, and status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workload {
    /// Unique workload identifier.
    pub id: WorkloadId,
    /// Workload specification.
    pub spec: WorkloadSpec,
    /// Current status.
    pub status: WorkloadStatus,
    /// When the workload was created.
    pub created_at: DateTime<Utc>,
    /// Optional workload name.
    pub name: Option<String>,
}

impl Workload {
    /// Create a new workload with the given spec.
    #[must_use]
    pub fn new(spec: WorkloadSpec) -> Self {
        Self {
            id: WorkloadId::new(),
            spec,
            status: WorkloadStatus::pending(),
            created_at: Utc::now(),
            name: None,
        }
    }

    /// Create a new workload with a specific ID.
    #[must_use]
    pub fn with_id(id: WorkloadId, spec: WorkloadSpec) -> Self {
        Self {
            id,
            spec,
            status: WorkloadStatus::pending(),
            created_at: Utc::now(),
            name: None,
        }
    }

    /// Set the workload name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Check if the workload is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.status.state.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== WorkloadSpec Tests ====================

    #[test]
    fn test_workload_spec_new() {
        let spec = WorkloadSpec::new("nginx:latest");
        assert_eq!(spec.image, "nginx:latest");
        assert!(spec.command.is_empty());
        assert!(spec.env.is_empty());
        assert_eq!(spec.gpu_count, 0);
        assert_eq!(spec.memory_mb, 512);
        assert_eq!(spec.cpu_cores, 1);
    }

    #[test]
    fn test_workload_spec_builder() {
        let spec = WorkloadSpec::new("python:3.11")
            .with_command(vec!["python".into(), "app.py".into()])
            .with_env("DEBUG", "true")
            .with_env("PORT", "8080")
            .with_gpu_count(2)
            .with_memory_mb(4096)
            .with_cpu_cores(4);

        assert_eq!(spec.image, "python:3.11");
        assert_eq!(spec.command, vec!["python", "app.py"]);
        assert_eq!(spec.env.get("DEBUG"), Some(&"true".to_string()));
        assert_eq!(spec.env.get("PORT"), Some(&"8080".to_string()));
        assert_eq!(spec.gpu_count, 2);
        assert_eq!(spec.memory_mb, 4096);
        assert_eq!(spec.cpu_cores, 4);
    }

    #[test]
    fn test_workload_spec_validate_valid() {
        let spec = WorkloadSpec::new("nginx:latest")
            .with_env("MY_VAR", "value")
            .with_memory_mb(1024)
            .with_cpu_cores(2)
            .with_gpu_count(1);

        assert!(spec.validate().is_ok());
    }

    #[test]
    fn test_workload_spec_validate_empty_image() {
        let spec = WorkloadSpec::new("");
        let result = spec.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "image");
    }

    #[test]
    fn test_workload_spec_validate_invalid_image() {
        let spec = WorkloadSpec::new("nginx latest"); // has whitespace
        let result = spec.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_workload_spec_validate_invalid_env_key() {
        let mut spec = WorkloadSpec::new("nginx:latest");
        spec.env.insert("123INVALID".into(), "value".into());

        let result = spec.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "env");
    }

    #[test]
    fn test_workload_spec_validate_resource_limits() {
        let spec = WorkloadSpec::new("nginx:latest").with_gpu_count(100); // exceeds MAX_GPU_COUNT
        let result = spec.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_workload_spec_serialization() {
        let spec = WorkloadSpec::new("nginx:latest")
            .with_env("KEY", "value")
            .with_gpu_count(1);

        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: WorkloadSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, deserialized);
    }

    // ==================== WorkloadStatus Tests ====================

    #[test]
    fn test_workload_status_pending() {
        let status = WorkloadStatus::pending();
        assert_eq!(status.state, WorkloadState::Pending);
        assert!(status.started_at.is_none());
        assert!(status.finished_at.is_none());
        assert!(status.exit_code.is_none());
        assert!(status.gpu_ids.is_empty());
    }

    #[test]
    fn test_workload_status_running() {
        let now = Utc::now();
        let status = WorkloadStatus::running(now, vec![0, 1]);
        assert_eq!(status.state, WorkloadState::Running);
        assert_eq!(status.started_at, Some(now));
        assert_eq!(status.gpu_ids, vec![0, 1]);
    }

    #[test]
    fn test_workload_status_completed() {
        let start = Utc::now();
        let finish = start + chrono::Duration::seconds(60);
        let status = WorkloadStatus::completed(start, finish, 0);

        assert_eq!(status.state, WorkloadState::Completed);
        assert_eq!(status.exit_code, Some(0));
        assert!(status.finished_at.is_some());
    }

    #[test]
    fn test_workload_status_failed() {
        let start = Utc::now();
        let finish = start + chrono::Duration::seconds(30);
        let status = WorkloadStatus::failed(Some(start), finish, Some(1));

        assert_eq!(status.state, WorkloadState::Failed);
        assert_eq!(status.exit_code, Some(1));
    }

    #[test]
    fn test_workload_status_duration() {
        let start = Utc::now();
        let finish = start + chrono::Duration::seconds(120);
        let status = WorkloadStatus::completed(start, finish, 0);

        let duration = status.duration().unwrap();
        assert_eq!(duration.num_seconds(), 120);
    }

    #[test]
    fn test_workload_status_duration_none_when_not_finished() {
        let status = WorkloadStatus::running(Utc::now(), vec![]);
        assert!(status.duration().is_none());
    }

    // ==================== State Transition Tests ====================

    #[test]
    fn test_valid_transition_pending_to_starting() {
        assert!(is_valid_transition(
            WorkloadState::Pending,
            WorkloadState::Starting
        ));
    }

    #[test]
    fn test_valid_transition_starting_to_running() {
        assert!(is_valid_transition(
            WorkloadState::Starting,
            WorkloadState::Running
        ));
    }

    #[test]
    fn test_valid_transition_running_to_completed() {
        assert!(is_valid_transition(
            WorkloadState::Running,
            WorkloadState::Completed
        ));
    }

    #[test]
    fn test_valid_transition_running_to_failed() {
        assert!(is_valid_transition(
            WorkloadState::Running,
            WorkloadState::Failed
        ));
    }

    #[test]
    fn test_valid_transition_running_to_stopping() {
        assert!(is_valid_transition(
            WorkloadState::Running,
            WorkloadState::Stopping
        ));
    }

    #[test]
    fn test_valid_transition_stopping_to_stopped() {
        assert!(is_valid_transition(
            WorkloadState::Stopping,
            WorkloadState::Stopped
        ));
    }

    #[test]
    fn test_invalid_transition_pending_to_completed() {
        assert!(!is_valid_transition(
            WorkloadState::Pending,
            WorkloadState::Completed
        ));
    }

    #[test]
    fn test_invalid_transition_completed_to_running() {
        assert!(!is_valid_transition(
            WorkloadState::Completed,
            WorkloadState::Running
        ));
    }

    #[test]
    fn test_invalid_transition_from_terminal() {
        // Terminal states should not transition to anything else
        assert!(!is_valid_transition(
            WorkloadState::Completed,
            WorkloadState::Pending
        ));
        assert!(!is_valid_transition(
            WorkloadState::Failed,
            WorkloadState::Running
        ));
        assert!(!is_valid_transition(
            WorkloadState::Stopped,
            WorkloadState::Starting
        ));
    }

    #[test]
    fn test_status_transition_updates_timestamps() {
        let mut status = WorkloadStatus::pending();
        assert!(status.transition_to(WorkloadState::Starting).is_ok());
        assert!(status.transition_to(WorkloadState::Running).is_ok());
        assert!(status.started_at.is_some());

        assert!(status.transition_to(WorkloadState::Completed).is_ok());
        assert!(status.finished_at.is_some());
    }

    #[test]
    fn test_status_transition_error_on_invalid() {
        let mut status = WorkloadStatus::pending();
        let result = status.transition_to(WorkloadState::Completed);
        assert!(result.is_err());
    }

    // ==================== Workload Tests ====================

    #[test]
    fn test_workload_new() {
        let spec = WorkloadSpec::new("nginx:latest");
        let workload = Workload::new(spec.clone());

        assert_eq!(workload.spec, spec);
        assert_eq!(workload.status.state, WorkloadState::Pending);
        assert!(workload.name.is_none());
    }

    #[test]
    fn test_workload_with_name() {
        let spec = WorkloadSpec::new("nginx:latest");
        let workload = Workload::new(spec).with_name("my-web-server");

        assert_eq!(workload.name, Some("my-web-server".to_string()));
    }

    #[test]
    fn test_workload_is_terminal() {
        let spec = WorkloadSpec::new("nginx:latest");
        let mut workload = Workload::new(spec);

        assert!(!workload.is_terminal());

        workload.status.state = WorkloadState::Completed;
        assert!(workload.is_terminal());
    }

    #[test]
    fn test_workload_serialization() {
        let spec = WorkloadSpec::new("nginx:latest");
        let workload = Workload::new(spec).with_name("test");

        let json = serde_json::to_string(&workload).unwrap();
        let deserialized: Workload = serde_json::from_str(&json).unwrap();

        assert_eq!(workload.id, deserialized.id);
        assert_eq!(workload.spec, deserialized.spec);
        assert_eq!(workload.name, deserialized.name);
    }
}
