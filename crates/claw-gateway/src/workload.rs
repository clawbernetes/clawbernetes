//! Workload management and state tracking.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use claw_proto::{NodeId, Workload, WorkloadId, WorkloadSpec, WorkloadState, WorkloadStatus};
use thiserror::Error;

/// Errors that can occur during workload management.
#[derive(Debug, Error)]
pub enum WorkloadManagerError {
    /// Workload was not found.
    #[error("workload {0} not found")]
    NotFound(WorkloadId),

    /// Invalid state transition.
    #[error("invalid state transition for workload {0}: cannot transition from {1} to {2}")]
    InvalidTransition(WorkloadId, WorkloadState, WorkloadState),

    /// Workload validation failed.
    #[error("workload validation failed: {0}")]
    ValidationFailed(String),

    /// Cannot cancel workload in current state.
    #[error("cannot cancel workload {0} in state {1}")]
    CannotCancel(WorkloadId, WorkloadState),
}

/// Tracked workload with additional gateway-side metadata.
#[derive(Debug, Clone)]
pub struct TrackedWorkload {
    /// The underlying workload.
    pub workload: Workload,
    /// Node assignment (if scheduled).
    pub assigned_node: Option<NodeId>,
    /// When the workload was submitted to the gateway.
    pub submitted_at: DateTime<Utc>,
    /// Assigned GPU indices (from scheduler).
    pub assigned_gpus: Vec<u32>,
    /// Worker index for parallel workloads.
    pub worker_index: Option<u32>,
    /// Reason for scheduling failure (if any).
    pub schedule_failure: Option<String>,
}

impl TrackedWorkload {
    /// Create a new tracked workload.
    fn new(workload: Workload) -> Self {
        Self {
            workload,
            assigned_node: None,
            submitted_at: Utc::now(),
            assigned_gpus: Vec::new(),
            worker_index: None,
            schedule_failure: None,
        }
    }

    /// Get the workload ID.
    #[must_use]
    pub const fn id(&self) -> WorkloadId {
        self.workload.id
    }

    /// Get the current state.
    #[must_use]
    pub const fn state(&self) -> WorkloadState {
        self.workload.status.state
    }

    /// Check if the workload is assigned to a node.
    #[must_use]
    pub const fn is_assigned(&self) -> bool {
        self.assigned_node.is_some()
    }
}

/// Manager for workload lifecycle and state tracking.
#[derive(Debug, Default)]
pub struct WorkloadManager {
    workloads: HashMap<WorkloadId, TrackedWorkload>,
}

impl WorkloadManager {
    /// Create a new workload manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            workloads: HashMap::new(),
        }
    }

    /// Submit a new workload for scheduling.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload spec is invalid.
    pub fn submit(&mut self, spec: WorkloadSpec) -> Result<WorkloadId, WorkloadManagerError> {
        spec.validate()
            .map_err(|e| WorkloadManagerError::ValidationFailed(e.to_string()))?;

        let workload = Workload::new(spec);
        let id = workload.id;
        let tracked = TrackedWorkload::new(workload);
        self.workloads.insert(id, tracked);

        Ok(id)
    }

    /// Get the status of a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found.
    pub fn get_status(&self, id: WorkloadId) -> Result<WorkloadStatus, WorkloadManagerError> {
        self.workloads
            .get(&id)
            .map(|tw| tw.workload.status.clone())
            .ok_or(WorkloadManagerError::NotFound(id))
    }

    /// Get a reference to a tracked workload.
    #[must_use]
    pub fn get_workload(&self, id: WorkloadId) -> Option<&TrackedWorkload> {
        self.workloads.get(&id)
    }

    /// Get a mutable reference to a tracked workload.
    #[must_use]
    pub fn get_workload_mut(&mut self, id: WorkloadId) -> Option<&mut TrackedWorkload> {
        self.workloads.get_mut(&id)
    }

    /// Cancel a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or cannot be cancelled.
    pub fn cancel(&mut self, id: WorkloadId) -> Result<(), WorkloadManagerError> {
        let tracked = self
            .workloads
            .get_mut(&id)
            .ok_or(WorkloadManagerError::NotFound(id))?;

        let current_state = tracked.state();

        // Can only cancel from non-terminal states
        if current_state.is_terminal() {
            return Err(WorkloadManagerError::CannotCancel(id, current_state));
        }

        // Transition to stopping or stopped depending on current state
        let new_state = match current_state {
            WorkloadState::Pending => WorkloadState::Stopped,
            WorkloadState::Starting | WorkloadState::Running => WorkloadState::Stopping,
            WorkloadState::Stopping => return Ok(()), // Already stopping
            _ => return Err(WorkloadManagerError::CannotCancel(id, current_state)),
        };

        tracked
            .workload
            .status
            .transition_to(new_state)
            .map_err(|_| WorkloadManagerError::InvalidTransition(id, current_state, new_state))
    }

    /// Assign a workload to a node.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found.
    pub fn assign_to_node(
        &mut self,
        workload_id: WorkloadId,
        node_id: NodeId,
    ) -> Result<(), WorkloadManagerError> {
        let tracked = self
            .workloads
            .get_mut(&workload_id)
            .ok_or(WorkloadManagerError::NotFound(workload_id))?;

        tracked.assigned_node = Some(node_id);
        Ok(())
    }

    /// Update the state of a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if the workload is not found or the transition is invalid.
    pub fn update_state(
        &mut self,
        id: WorkloadId,
        new_state: WorkloadState,
    ) -> Result<(), WorkloadManagerError> {
        let tracked = self
            .workloads
            .get_mut(&id)
            .ok_or(WorkloadManagerError::NotFound(id))?;

        let current = tracked.state();
        tracked
            .workload
            .status
            .transition_to(new_state)
            .map_err(|_| WorkloadManagerError::InvalidTransition(id, current, new_state))
    }

    /// List all workloads.
    #[must_use]
    pub fn list_workloads(&self) -> Vec<&TrackedWorkload> {
        self.workloads.values().collect()
    }

    /// List workloads in a specific state.
    #[must_use]
    pub fn list_by_state(&self, state: WorkloadState) -> Vec<&TrackedWorkload> {
        self.workloads
            .values()
            .filter(|tw| tw.state() == state)
            .collect()
    }

    /// List workloads assigned to a specific node.
    #[must_use]
    pub fn list_by_node(&self, node_id: NodeId) -> Vec<&TrackedWorkload> {
        self.workloads
            .values()
            .filter(|tw| tw.assigned_node == Some(node_id))
            .collect()
    }

    /// List pending workloads that need scheduling.
    #[must_use]
    pub fn pending_workloads(&self) -> Vec<&TrackedWorkload> {
        self.workloads
            .values()
            .filter(|tw| tw.state() == WorkloadState::Pending && !tw.is_assigned())
            .collect()
    }

    /// Get the number of managed workloads.
    #[must_use]
    pub fn len(&self) -> usize {
        self.workloads.len()
    }

    /// Check if there are no managed workloads.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.workloads.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Helper Functions ====================

    fn make_valid_spec() -> WorkloadSpec {
        WorkloadSpec::new("nginx:latest")
            .with_memory_mb(1024)
            .with_cpu_cores(2)
    }

    fn make_gpu_spec(gpu_count: u32) -> WorkloadSpec {
        WorkloadSpec::new("pytorch:latest")
            .with_gpu_count(gpu_count)
            .with_memory_mb(8192)
            .with_cpu_cores(4)
    }

    // ==================== WorkloadManager Basic Tests ====================

    #[test]
    fn test_manager_new_is_empty() {
        let manager = WorkloadManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_manager_default_is_empty() {
        let manager = WorkloadManager::default();
        assert!(manager.is_empty());
    }

    // ==================== Submit Tests ====================

    #[test]
    fn test_submit_valid_spec_returns_id() {
        let mut manager = WorkloadManager::new();
        let spec = make_valid_spec();

        let result = manager.submit(spec);

        assert!(result.is_ok());
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_submit_creates_pending_workload() {
        let mut manager = WorkloadManager::new();
        let spec = make_valid_spec();

        let id = manager.submit(spec).unwrap();
        let workload = manager.get_workload(id).unwrap();

        assert_eq!(workload.state(), WorkloadState::Pending);
        assert!(!workload.is_assigned());
    }

    #[test]
    fn test_submit_invalid_spec_fails() {
        let mut manager = WorkloadManager::new();
        let spec = WorkloadSpec::new(""); // Empty image is invalid

        let result = manager.submit(spec);

        assert!(matches!(
            result,
            Err(WorkloadManagerError::ValidationFailed(_))
        ));
        assert!(manager.is_empty());
    }

    #[test]
    fn test_submit_multiple_workloads() {
        let mut manager = WorkloadManager::new();

        let id1 = manager.submit(make_valid_spec()).unwrap();
        let id2 = manager.submit(make_gpu_spec(1)).unwrap();

        assert_eq!(manager.len(), 2);
        assert_ne!(id1, id2);
    }

    // ==================== Get Status Tests ====================

    #[test]
    fn test_get_status_existing_workload() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        let status = manager.get_status(id);

        assert!(status.is_ok());
        assert_eq!(status.unwrap().state, WorkloadState::Pending);
    }

    #[test]
    fn test_get_status_nonexistent_workload() {
        let manager = WorkloadManager::new();
        let fake_id = WorkloadId::new();

        let result = manager.get_status(fake_id);

        assert!(matches!(result, Err(WorkloadManagerError::NotFound(_))));
    }

    // ==================== Cancel Tests ====================

    #[test]
    fn test_cancel_pending_workload() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        let result = manager.cancel(id);

        assert!(result.is_ok());
        let status = manager.get_status(id).unwrap();
        assert_eq!(status.state, WorkloadState::Stopped);
    }

    #[test]
    fn test_cancel_running_workload() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        // Transition to running
        manager.update_state(id, WorkloadState::Starting).unwrap();
        manager.update_state(id, WorkloadState::Running).unwrap();

        let result = manager.cancel(id);

        assert!(result.is_ok());
        let status = manager.get_status(id).unwrap();
        assert_eq!(status.state, WorkloadState::Stopping);
    }

    #[test]
    fn test_cancel_completed_workload_fails() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        // Transition to completed
        manager.update_state(id, WorkloadState::Starting).unwrap();
        manager.update_state(id, WorkloadState::Running).unwrap();
        manager.update_state(id, WorkloadState::Completed).unwrap();

        let result = manager.cancel(id);

        assert!(matches!(result, Err(WorkloadManagerError::CannotCancel(_, _))));
    }

    #[test]
    fn test_cancel_nonexistent_workload_fails() {
        let mut manager = WorkloadManager::new();
        let fake_id = WorkloadId::new();

        let result = manager.cancel(fake_id);

        assert!(matches!(result, Err(WorkloadManagerError::NotFound(_))));
    }

    // ==================== Assignment Tests ====================

    #[test]
    fn test_assign_to_node() {
        let mut manager = WorkloadManager::new();
        let workload_id = manager.submit(make_valid_spec()).unwrap();
        let node_id = NodeId::new();

        let result = manager.assign_to_node(workload_id, node_id);

        assert!(result.is_ok());
        let workload = manager.get_workload(workload_id).unwrap();
        assert_eq!(workload.assigned_node, Some(node_id));
        assert!(workload.is_assigned());
    }

    #[test]
    fn test_assign_nonexistent_workload_fails() {
        let mut manager = WorkloadManager::new();
        let fake_id = WorkloadId::new();
        let node_id = NodeId::new();

        let result = manager.assign_to_node(fake_id, node_id);

        assert!(matches!(result, Err(WorkloadManagerError::NotFound(_))));
    }

    // ==================== State Update Tests ====================

    #[test]
    fn test_update_state_valid_transition() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        let result = manager.update_state(id, WorkloadState::Starting);

        assert!(result.is_ok());
        assert_eq!(manager.get_status(id).unwrap().state, WorkloadState::Starting);
    }

    #[test]
    fn test_update_state_invalid_transition() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        // Cannot go directly from Pending to Completed
        let result = manager.update_state(id, WorkloadState::Completed);

        assert!(matches!(
            result,
            Err(WorkloadManagerError::InvalidTransition(_, _, _))
        ));
    }

    #[test]
    fn test_update_state_full_lifecycle() {
        let mut manager = WorkloadManager::new();
        let id = manager.submit(make_valid_spec()).unwrap();

        assert!(manager.update_state(id, WorkloadState::Starting).is_ok());
        assert!(manager.update_state(id, WorkloadState::Running).is_ok());
        assert!(manager.update_state(id, WorkloadState::Completed).is_ok());

        let status = manager.get_status(id).unwrap();
        assert_eq!(status.state, WorkloadState::Completed);
        assert!(status.state.is_terminal());
    }

    // ==================== List Tests ====================

    #[test]
    fn test_list_workloads_empty() {
        let manager = WorkloadManager::new();
        assert!(manager.list_workloads().is_empty());
    }

    #[test]
    fn test_list_workloads_returns_all() {
        let mut manager = WorkloadManager::new();
        manager.submit(make_valid_spec()).unwrap();
        manager.submit(make_valid_spec()).unwrap();
        manager.submit(make_valid_spec()).unwrap();

        assert_eq!(manager.list_workloads().len(), 3);
    }

    #[test]
    fn test_list_by_state() {
        let mut manager = WorkloadManager::new();
        let id1 = manager.submit(make_valid_spec()).unwrap();
        let id2 = manager.submit(make_valid_spec()).unwrap();
        let _id3 = manager.submit(make_valid_spec()).unwrap();

        manager.update_state(id1, WorkloadState::Starting).unwrap();
        manager.update_state(id2, WorkloadState::Starting).unwrap();
        manager.update_state(id2, WorkloadState::Running).unwrap();

        let pending = manager.list_by_state(WorkloadState::Pending);
        let starting = manager.list_by_state(WorkloadState::Starting);
        let running = manager.list_by_state(WorkloadState::Running);

        assert_eq!(pending.len(), 1);
        assert_eq!(starting.len(), 1);
        assert_eq!(running.len(), 1);
    }

    #[test]
    fn test_list_by_node() {
        let mut manager = WorkloadManager::new();
        let node1 = NodeId::new();
        let node2 = NodeId::new();

        let id1 = manager.submit(make_valid_spec()).unwrap();
        let id2 = manager.submit(make_valid_spec()).unwrap();
        let _id3 = manager.submit(make_valid_spec()).unwrap(); // Unassigned

        manager.assign_to_node(id1, node1).unwrap();
        manager.assign_to_node(id2, node1).unwrap();

        let node1_workloads = manager.list_by_node(node1);
        let node2_workloads = manager.list_by_node(node2);

        assert_eq!(node1_workloads.len(), 2);
        assert_eq!(node2_workloads.len(), 0);
    }

    #[test]
    fn test_pending_workloads() {
        let mut manager = WorkloadManager::new();
        let node_id = NodeId::new();

        let id1 = manager.submit(make_valid_spec()).unwrap();
        let id2 = manager.submit(make_valid_spec()).unwrap();
        let id3 = manager.submit(make_valid_spec()).unwrap();

        // Assign one
        manager.assign_to_node(id1, node_id).unwrap();

        // Start another
        manager.update_state(id2, WorkloadState::Starting).unwrap();

        // Only id3 should be pending and unassigned
        let pending = manager.pending_workloads();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id(), id3);
    }

    // ==================== TrackedWorkload Tests ====================

    #[test]
    fn test_tracked_workload_accessors() {
        let spec = make_valid_spec();
        let workload = Workload::new(spec);
        let id = workload.id;
        let tracked = TrackedWorkload::new(workload);

        assert_eq!(tracked.id(), id);
        assert_eq!(tracked.state(), WorkloadState::Pending);
        assert!(!tracked.is_assigned());
    }
}
