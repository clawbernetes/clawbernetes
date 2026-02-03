//! Node state management.
//!
//! Tracks active workloads, GPU allocations, and connection state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::NodeError;

/// State of the gateway connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GatewayConnectionState {
    /// Not connected to gateway.
    #[default]
    Disconnected,
    /// Currently connecting.
    Connecting,
    /// Connected and operational.
    Connected,
    /// Connection failed, will retry.
    Reconnecting,
}

/// Information about a running workload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadInfo {
    /// Unique workload identifier.
    pub id: Uuid,
    /// Container image.
    pub image: String,
    /// Allocated GPU IDs.
    pub gpu_ids: Vec<u32>,
    /// Start timestamp.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Container ID assigned by runtime (if started).
    pub container_id: Option<String>,
}

impl WorkloadInfo {
    /// Create a new workload info.
    #[must_use]
    pub fn new(id: Uuid, image: impl Into<String>, gpu_ids: Vec<u32>) -> Self {
        Self {
            id,
            image: image.into(),
            gpu_ids,
            started_at: chrono::Utc::now(),
            container_id: None,
        }
    }

    /// Set the container ID.
    #[must_use]
    pub fn with_container_id(mut self, container_id: impl Into<String>) -> Self {
        self.container_id = Some(container_id.into());
        self
    }
}

/// Current state of the node.
#[derive(Debug, Default)]
pub struct NodeState {
    /// Gateway connection state.
    pub connection: GatewayConnectionState,
    /// Active workloads by ID.
    pub workloads: HashMap<Uuid, WorkloadInfo>,
    /// Allocated GPU IDs.
    pub allocated_gpus: Vec<u32>,
    /// Total available GPU count on this node.
    pub total_gpus: u32,
}

impl NodeState {
    /// Create a new empty node state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new node state with specified GPU count.
    #[must_use]
    pub fn with_gpus(total_gpus: u32) -> Self {
        Self {
            total_gpus,
            ..Self::default()
        }
    }

    /// Check if a GPU is allocated.
    #[must_use]
    pub fn is_gpu_allocated(&self, gpu_id: u32) -> bool {
        self.allocated_gpus.contains(&gpu_id)
    }

    /// Get count of active workloads.
    #[must_use]
    pub fn workload_count(&self) -> usize {
        self.workloads.len()
    }

    /// Get the number of available (unallocated) GPUs.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // GPU count won't exceed u32::MAX
    pub fn available_gpu_count(&self) -> u32 {
        self.total_gpus.saturating_sub(self.allocated_gpus.len() as u32)
    }

    /// Add a workload to the state.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::WorkloadExists`] if a workload with the same ID already exists.
    pub fn add_workload(&mut self, info: WorkloadInfo) -> Result<(), NodeError> {
        if self.workloads.contains_key(&info.id) {
            return Err(NodeError::WorkloadExists(info.id));
        }

        // Track the GPU allocations from this workload
        for &gpu_id in &info.gpu_ids {
            if !self.allocated_gpus.contains(&gpu_id) {
                self.allocated_gpus.push(gpu_id);
            }
        }

        self.workloads.insert(info.id, info);
        Ok(())
    }

    /// Remove a workload from the state.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::WorkloadNotFound`] if the workload does not exist.
    pub fn remove_workload(&mut self, id: Uuid) -> Result<WorkloadInfo, NodeError> {
        let info = self
            .workloads
            .remove(&id)
            .ok_or(NodeError::WorkloadNotFound(id))?;

        // Release the GPUs that were allocated to this workload
        for gpu_id in &info.gpu_ids {
            self.allocated_gpus.retain(|&g| g != *gpu_id);
        }

        Ok(info)
    }

    /// Get a reference to a workload by ID.
    #[must_use]
    pub fn get_workload(&self, id: Uuid) -> Option<&WorkloadInfo> {
        self.workloads.get(&id)
    }

    /// Get a mutable reference to a workload by ID.
    #[must_use]
    pub fn get_workload_mut(&mut self, id: Uuid) -> Option<&mut WorkloadInfo> {
        self.workloads.get_mut(&id)
    }

    /// Allocate GPUs for a workload.
    ///
    /// Returns the list of allocated GPU IDs.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError::InsufficientGpus`] if not enough GPUs are available.
    pub fn allocate_gpus(&mut self, count: u32) -> Result<Vec<u32>, NodeError> {
        if count == 0 {
            return Ok(Vec::new());
        }

        let available = self.available_gpu_count();
        if count > available {
            return Err(NodeError::InsufficientGpus {
                requested: count,
                available,
            });
        }

        // Find unallocated GPU IDs
        let mut allocated = Vec::with_capacity(count as usize);
        for gpu_id in 0..self.total_gpus {
            if !self.is_gpu_allocated(gpu_id) {
                allocated.push(gpu_id);
                self.allocated_gpus.push(gpu_id);
                if allocated.len() == count as usize {
                    break;
                }
            }
        }

        Ok(allocated)
    }

    /// Release previously allocated GPUs.
    ///
    /// Any GPU IDs that are not currently allocated are silently ignored.
    pub fn release_gpus(&mut self, gpu_ids: &[u32]) {
        for &gpu_id in gpu_ids {
            self.allocated_gpus.retain(|&g| g != gpu_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic State Tests ====================

    #[test]
    fn test_node_state_default() {
        let state = NodeState::new();
        assert_eq!(state.connection, GatewayConnectionState::Disconnected);
        assert!(state.workloads.is_empty());
        assert!(state.allocated_gpus.is_empty());
        assert_eq!(state.total_gpus, 0);
    }

    #[test]
    fn test_node_state_with_gpus() {
        let state = NodeState::with_gpus(4);
        assert_eq!(state.total_gpus, 4);
        assert_eq!(state.available_gpu_count(), 4);
    }

    #[test]
    fn test_gateway_connection_state_default() {
        let state = GatewayConnectionState::default();
        assert_eq!(state, GatewayConnectionState::Disconnected);
    }

    #[test]
    fn test_is_gpu_allocated() {
        let mut state = NodeState::new();
        assert!(!state.is_gpu_allocated(0));
        state.allocated_gpus.push(0);
        assert!(state.is_gpu_allocated(0));
        assert!(!state.is_gpu_allocated(1));
    }

    // ==================== WorkloadInfo Tests ====================

    #[test]
    fn test_workload_info_new() {
        let id = Uuid::new_v4();
        let info = WorkloadInfo::new(id, "nginx:latest", vec![0, 1]);

        assert_eq!(info.id, id);
        assert_eq!(info.image, "nginx:latest");
        assert_eq!(info.gpu_ids, vec![0, 1]);
        assert!(info.container_id.is_none());
    }

    #[test]
    fn test_workload_info_with_container_id() {
        let id = Uuid::new_v4();
        let info = WorkloadInfo::new(id, "nginx:latest", vec![0])
            .with_container_id("container-123");

        assert_eq!(info.container_id, Some("container-123".to_string()));
    }

    // ==================== add_workload Tests ====================

    #[test]
    fn test_add_workload_success() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();
        let info = WorkloadInfo::new(id, "nginx:latest", vec![0, 1]);

        let result = state.add_workload(info);

        assert!(result.is_ok());
        assert_eq!(state.workload_count(), 1);
        assert!(state.is_gpu_allocated(0));
        assert!(state.is_gpu_allocated(1));
    }

    #[test]
    fn test_add_workload_duplicate_fails() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();
        let info1 = WorkloadInfo::new(id, "nginx:latest", vec![0]);
        let info2 = WorkloadInfo::new(id, "redis:latest", vec![1]);

        state.add_workload(info1).expect("first add should succeed");
        let result = state.add_workload(info2);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadExists(err_id) if err_id == id));
    }

    #[test]
    fn test_add_multiple_workloads() {
        let mut state = NodeState::with_gpus(4);
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        state
            .add_workload(WorkloadInfo::new(id1, "nginx:latest", vec![0]))
            .expect("add should succeed");
        state
            .add_workload(WorkloadInfo::new(id2, "redis:latest", vec![1]))
            .expect("add should succeed");

        assert_eq!(state.workload_count(), 2);
        assert!(state.is_gpu_allocated(0));
        assert!(state.is_gpu_allocated(1));
        assert!(!state.is_gpu_allocated(2));
    }

    // ==================== remove_workload Tests ====================

    #[test]
    fn test_remove_workload_success() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();
        state
            .add_workload(WorkloadInfo::new(id, "nginx:latest", vec![0, 1]))
            .expect("add should succeed");

        let result = state.remove_workload(id);

        assert!(result.is_ok());
        let removed = result.unwrap();
        assert_eq!(removed.id, id);
        assert_eq!(state.workload_count(), 0);
        // GPUs should be released
        assert!(!state.is_gpu_allocated(0));
        assert!(!state.is_gpu_allocated(1));
    }

    #[test]
    fn test_remove_workload_not_found() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();

        let result = state.remove_workload(id);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::WorkloadNotFound(err_id) if err_id == id));
    }

    #[test]
    fn test_remove_workload_releases_gpus() {
        let mut state = NodeState::with_gpus(4);
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        state
            .add_workload(WorkloadInfo::new(id1, "nginx:latest", vec![0, 1]))
            .expect("add should succeed");
        state
            .add_workload(WorkloadInfo::new(id2, "redis:latest", vec![2]))
            .expect("add should succeed");

        state.remove_workload(id1).expect("remove should succeed");

        // GPU 0 and 1 should be released, GPU 2 should still be allocated
        assert!(!state.is_gpu_allocated(0));
        assert!(!state.is_gpu_allocated(1));
        assert!(state.is_gpu_allocated(2));
    }

    // ==================== get_workload Tests ====================

    #[test]
    fn test_get_workload_exists() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();
        state
            .add_workload(WorkloadInfo::new(id, "nginx:latest", vec![0]))
            .expect("add should succeed");

        let workload = state.get_workload(id);

        assert!(workload.is_some());
        assert_eq!(workload.unwrap().image, "nginx:latest");
    }

    #[test]
    fn test_get_workload_not_found() {
        let state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();

        let workload = state.get_workload(id);

        assert!(workload.is_none());
    }

    #[test]
    fn test_get_workload_mut() {
        let mut state = NodeState::with_gpus(4);
        let id = Uuid::new_v4();
        state
            .add_workload(WorkloadInfo::new(id, "nginx:latest", vec![0]))
            .expect("add should succeed");

        let workload = state.get_workload_mut(id);
        assert!(workload.is_some());

        workload.unwrap().container_id = Some("container-abc".to_string());

        assert_eq!(
            state.get_workload(id).unwrap().container_id,
            Some("container-abc".to_string())
        );
    }

    // ==================== allocate_gpus Tests ====================

    #[test]
    fn test_allocate_gpus_success() {
        let mut state = NodeState::with_gpus(4);

        let result = state.allocate_gpus(2);

        assert!(result.is_ok());
        let allocated = result.unwrap();
        assert_eq!(allocated.len(), 2);
        assert_eq!(state.available_gpu_count(), 2);
        assert!(state.is_gpu_allocated(allocated[0]));
        assert!(state.is_gpu_allocated(allocated[1]));
    }

    #[test]
    fn test_allocate_gpus_all() {
        let mut state = NodeState::with_gpus(4);

        let result = state.allocate_gpus(4);

        assert!(result.is_ok());
        let allocated = result.unwrap();
        assert_eq!(allocated.len(), 4);
        assert_eq!(state.available_gpu_count(), 0);
    }

    #[test]
    fn test_allocate_gpus_insufficient() {
        let mut state = NodeState::with_gpus(2);

        let result = state.allocate_gpus(3);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NodeError::InsufficientGpus {
                requested: 3,
                available: 2
            }
        ));
    }

    #[test]
    fn test_allocate_gpus_none_available() {
        let mut state = NodeState::with_gpus(2);
        state.allocate_gpus(2).expect("allocate all");

        let result = state.allocate_gpus(1);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NodeError::InsufficientGpus {
                requested: 1,
                available: 0
            }
        ));
    }

    #[test]
    fn test_allocate_zero_gpus() {
        let mut state = NodeState::with_gpus(4);

        let result = state.allocate_gpus(0);

        assert!(result.is_ok());
        let allocated = result.unwrap();
        assert!(allocated.is_empty());
        assert_eq!(state.available_gpu_count(), 4);
    }

    #[test]
    fn test_allocate_gpus_sequential() {
        let mut state = NodeState::with_gpus(4);

        let first = state.allocate_gpus(1).expect("first allocation");
        let second = state.allocate_gpus(1).expect("second allocation");

        // Should get different GPUs
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0], second[0]);
    }

    // ==================== release_gpus Tests ====================

    #[test]
    fn test_release_gpus_success() {
        let mut state = NodeState::with_gpus(4);
        let allocated = state.allocate_gpus(2).expect("allocate");

        state.release_gpus(&allocated);

        assert_eq!(state.available_gpu_count(), 4);
        assert!(!state.is_gpu_allocated(allocated[0]));
        assert!(!state.is_gpu_allocated(allocated[1]));
    }

    #[test]
    fn test_release_gpus_partial() {
        let mut state = NodeState::with_gpus(4);
        let allocated = state.allocate_gpus(3).expect("allocate");

        // Only release the first GPU
        state.release_gpus(&allocated[..1]);

        assert_eq!(state.available_gpu_count(), 2);
        assert!(!state.is_gpu_allocated(allocated[0]));
        assert!(state.is_gpu_allocated(allocated[1]));
        assert!(state.is_gpu_allocated(allocated[2]));
    }

    #[test]
    fn test_release_gpus_not_allocated() {
        let mut state = NodeState::with_gpus(4);

        // Releasing GPUs that aren't allocated should be a no-op
        state.release_gpus(&[0, 1, 2]);

        assert_eq!(state.available_gpu_count(), 4);
    }

    #[test]
    fn test_release_gpus_empty() {
        let mut state = NodeState::with_gpus(4);
        state.allocate_gpus(2).expect("allocate");

        state.release_gpus(&[]);

        // Should have no effect
        assert_eq!(state.available_gpu_count(), 2);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn test_full_workload_lifecycle() {
        let mut state = NodeState::with_gpus(4);

        // Allocate GPUs
        let gpus = state.allocate_gpus(2).expect("allocate");

        // Add workload
        let id = Uuid::new_v4();
        let info = WorkloadInfo::new(id, "training-job:latest", gpus.clone())
            .with_container_id("container-xyz");
        state.add_workload(info).expect("add workload");

        // Verify state
        assert_eq!(state.workload_count(), 1);
        assert_eq!(state.available_gpu_count(), 2);
        let workload = state.get_workload(id).expect("should exist");
        assert_eq!(workload.container_id, Some("container-xyz".to_string()));

        // Remove workload (GPUs were already tracked by add_workload)
        let removed = state.remove_workload(id).expect("remove");
        assert_eq!(removed.gpu_ids, gpus);

        // GPUs should now be available (released by remove_workload)
        assert_eq!(state.available_gpu_count(), 4);
    }

    #[test]
    fn test_multiple_workloads_with_gpus() {
        let mut state = NodeState::with_gpus(8);

        // Workload 1: 2 GPUs
        let gpus1 = state.allocate_gpus(2).expect("allocate");
        let id1 = Uuid::new_v4();
        // Don't add GPUs twice - they're already allocated
        state
            .add_workload(WorkloadInfo::new(id1, "job-a", vec![]))
            .expect("add");

        // Workload 2: 3 GPUs
        let _gpus2 = state.allocate_gpus(3).expect("allocate");
        let id2 = Uuid::new_v4();
        state
            .add_workload(WorkloadInfo::new(id2, "job-b", vec![]))
            .expect("add");

        assert_eq!(state.available_gpu_count(), 3);
        assert_eq!(state.workload_count(), 2);

        // Remove workload 1
        state.remove_workload(id1).expect("remove");
        // Release GPUs manually since they weren't in the workload info
        state.release_gpus(&gpus1);

        assert_eq!(state.available_gpu_count(), 5);
        assert_eq!(state.workload_count(), 1);
    }

    #[test]
    fn test_available_gpu_count_accuracy() {
        let mut state = NodeState::with_gpus(4);

        assert_eq!(state.available_gpu_count(), 4);

        state.allocate_gpus(1).expect("allocate");
        assert_eq!(state.available_gpu_count(), 3);

        state.allocate_gpus(2).expect("allocate");
        assert_eq!(state.available_gpu_count(), 1);

        state.release_gpus(&[0]);
        assert_eq!(state.available_gpu_count(), 2);

        state.release_gpus(&[1, 2]);
        assert_eq!(state.available_gpu_count(), 4);
    }
}
