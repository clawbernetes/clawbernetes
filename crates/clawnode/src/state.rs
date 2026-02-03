//! Node state management.
//!
//! Tracks active workloads, GPU allocations, and connection state.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// State of the gateway connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayConnectionState {
    /// Not connected to gateway.
    Disconnected,
    /// Currently connecting.
    Connecting,
    /// Connected and operational.
    Connected,
    /// Connection failed, will retry.
    Reconnecting,
}

impl Default for GatewayConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
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
}

impl NodeState {
    /// Create a new empty node state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_state_default() {
        let state = NodeState::new();
        assert_eq!(state.connection, GatewayConnectionState::Disconnected);
        assert!(state.workloads.is_empty());
        assert!(state.allocated_gpus.is_empty());
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
}
