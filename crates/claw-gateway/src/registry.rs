//! Node fleet registry for tracking registered nodes and their capabilities.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use claw_proto::{NodeCapabilities, NodeId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Expected heartbeat interval from nodes (in seconds).
pub const HEARTBEAT_INTERVAL_SECS: i64 = 30;

/// Node is considered unhealthy after missing this many heartbeat intervals.
pub const UNHEALTHY_THRESHOLD_MISSED: i64 = 2;

/// Node is considered offline after missing this many heartbeat intervals.
pub const OFFLINE_THRESHOLD_MISSED: i64 = 5;

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Node is already registered.
    #[error("node {0} is already registered")]
    AlreadyRegistered(NodeId),

    /// Node is not found in the registry.
    #[error("node {0} not found")]
    NotFound(NodeId),
}

/// Health status of a node based on heartbeat tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeHealthStatus {
    /// Node is healthy and responding to heartbeats.
    Healthy,
    /// Node has missed some heartbeats but is not yet offline.
    Unhealthy,
    /// Node is marked as draining (not accepting new workloads).
    Draining,
    /// Node has not responded for an extended period.
    Offline,
}

impl std::fmt::Display for NodeHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Draining => write!(f, "draining"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// Summary of node health across the fleet.
#[derive(Debug, Clone, Default)]
pub struct HealthSummary {
    /// Number of healthy nodes.
    pub healthy: usize,
    /// Number of unhealthy nodes.
    pub unhealthy: usize,
    /// Number of draining nodes.
    pub draining: usize,
    /// Number of offline nodes.
    pub offline: usize,
}

impl HealthSummary {
    /// Total number of nodes.
    #[must_use]
    pub fn total(&self) -> usize {
        self.healthy + self.unhealthy + self.draining + self.offline
    }
}

/// A node that has been registered with the gateway.
#[derive(Debug, Clone)]
pub struct RegisteredNode {
    /// The node's unique identifier.
    pub id: NodeId,
    /// Human-readable name for the node.
    pub name: String,
    /// The node's capabilities.
    pub capabilities: NodeCapabilities,
    /// When the node was registered.
    pub registered_at: DateTime<Utc>,
    /// Last heartbeat timestamp.
    pub last_heartbeat: DateTime<Utc>,
    /// Whether the node is draining (not accepting new workloads).
    pub draining: bool,
}

impl RegisteredNode {
    /// Create a new registered node.
    fn new(id: NodeId, name: String, capabilities: NodeCapabilities) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            capabilities,
            registered_at: now,
            last_heartbeat: now,
            draining: false,
        }
    }

    /// Update the last heartbeat timestamp.
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
    }

    /// Check if this node has the specified GPU type.
    #[must_use]
    pub fn has_gpu_type(&self, gpu_type: &str) -> bool {
        self.capabilities
            .gpus
            .iter()
            .any(|gpu| gpu.name.contains(gpu_type))
    }

    /// Get the number of GPUs this node has.
    #[must_use]
    pub fn gpu_count(&self) -> usize {
        self.capabilities.gpus.len()
    }

    /// Get the health status of this node.
    #[must_use]
    pub fn health_status(&self) -> NodeHealthStatus {
        if self.draining {
            return NodeHealthStatus::Draining;
        }

        let now = Utc::now();
        let elapsed_secs = (now - self.last_heartbeat).num_seconds();
        let missed_intervals = elapsed_secs / HEARTBEAT_INTERVAL_SECS;

        if missed_intervals >= OFFLINE_THRESHOLD_MISSED {
            NodeHealthStatus::Offline
        } else if missed_intervals >= UNHEALTHY_THRESHOLD_MISSED {
            NodeHealthStatus::Unhealthy
        } else {
            NodeHealthStatus::Healthy
        }
    }

    /// Check if this node is available for new workloads.
    ///
    /// A node is available if it's healthy and not draining.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.health_status() == NodeHealthStatus::Healthy
    }
}

/// Registry for managing the fleet of connected nodes.
#[derive(Debug, Default)]
pub struct NodeRegistry {
    nodes: HashMap<NodeId, RegisteredNode>,
}

impl NodeRegistry {
    /// Create a new empty node registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Register a node with its capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is already registered.
    pub fn register(
        &mut self,
        node_id: NodeId,
        capabilities: NodeCapabilities,
    ) -> Result<(), RegistryError> {
        self.register_with_name(node_id, format!("node-{}", &node_id.to_string()[..8]), capabilities)
    }

    /// Register a node with a custom name and capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is already registered.
    pub fn register_with_name(
        &mut self,
        node_id: NodeId,
        name: impl Into<String>,
        capabilities: NodeCapabilities,
    ) -> Result<(), RegistryError> {
        if self.nodes.contains_key(&node_id) {
            return Err(RegistryError::AlreadyRegistered(node_id));
        }

        let node = RegisteredNode::new(node_id, name.into(), capabilities);
        self.nodes.insert(node_id, node);
        Ok(())
    }

    /// Unregister a node from the registry.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub fn unregister(&mut self, node_id: NodeId) -> Result<(), RegistryError> {
        self.nodes
            .remove(&node_id)
            .map(|_| ())
            .ok_or(RegistryError::NotFound(node_id))
    }

    /// Get a reference to a registered node.
    #[must_use]
    pub fn get_node(&self, node_id: NodeId) -> Option<&RegisteredNode> {
        self.nodes.get(&node_id)
    }

    /// Get a mutable reference to a registered node.
    #[must_use]
    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut RegisteredNode> {
        self.nodes.get_mut(&node_id)
    }

    /// List all registered nodes.
    #[must_use]
    pub fn list_nodes(&self) -> Vec<&RegisteredNode> {
        self.nodes.values().collect()
    }

    /// Find nodes that have the specified GPU type.
    #[must_use]
    pub fn find_by_gpu(&self, gpu_type: &str) -> Vec<&RegisteredNode> {
        self.nodes
            .values()
            .filter(|node| node.has_gpu_type(gpu_type))
            .collect()
    }

    /// Get the number of registered nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Update heartbeat for a node.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub fn heartbeat(&mut self, node_id: NodeId) -> Result<(), RegistryError> {
        self.nodes
            .get_mut(&node_id)
            .map(RegisteredNode::update_heartbeat)
            .ok_or(RegistryError::NotFound(node_id))
    }

    /// Set the draining state of a node.
    ///
    /// When a node is draining, it will not receive new workloads but
    /// existing workloads will continue to run.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub fn set_draining(&mut self, node_id: NodeId, draining: bool) -> Result<(), RegistryError> {
        self.nodes
            .get_mut(&node_id)
            .map(|node| node.draining = draining)
            .ok_or(RegistryError::NotFound(node_id))
    }

    /// Check if a node is draining.
    #[must_use]
    pub fn is_draining(&self, node_id: NodeId) -> Option<bool> {
        self.nodes.get(&node_id).map(|node| node.draining)
    }

    /// Get the health status of a node.
    #[must_use]
    pub fn node_health(&self, node_id: NodeId) -> Option<NodeHealthStatus> {
        self.nodes.get(&node_id).map(|node| node.health_status())
    }

    /// Get all healthy nodes (not unhealthy, offline, or draining).
    #[must_use]
    pub fn healthy_nodes(&self) -> Vec<&RegisteredNode> {
        self.nodes
            .values()
            .filter(|node| node.health_status() == NodeHealthStatus::Healthy)
            .collect()
    }

    /// Get all available nodes (healthy and not draining).
    #[must_use]
    pub fn available_nodes(&self) -> Vec<&RegisteredNode> {
        self.nodes
            .values()
            .filter(|node| node.is_available())
            .collect()
    }

    /// Get a summary of node health across the fleet.
    #[must_use]
    pub fn health_summary(&self) -> HealthSummary {
        let mut summary = HealthSummary::default();
        for node in self.nodes.values() {
            match node.health_status() {
                NodeHealthStatus::Healthy => summary.healthy += 1,
                NodeHealthStatus::Unhealthy => summary.unhealthy += 1,
                NodeHealthStatus::Draining => summary.draining += 1,
                NodeHealthStatus::Offline => summary.offline += 1,
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::GpuCapability;

    // ==================== Helper Functions ====================

    fn make_gpu(name: &str, memory_mib: u64) -> GpuCapability {
        GpuCapability {
            index: 0,
            name: name.to_string(),
            memory_mib,
            uuid: "gpu-uuid-test".to_string(),
        }
    }

    fn make_capabilities_with_gpu(gpu_name: &str) -> NodeCapabilities {
        NodeCapabilities::new(8, 16384).with_gpu(make_gpu(gpu_name, 8192))
    }

    fn make_capabilities() -> NodeCapabilities {
        NodeCapabilities::new(4, 8192)
    }

    // ==================== NodeRegistry Basic Tests ====================

    #[test]
    fn test_registry_new_is_empty() {
        let registry = NodeRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_default_is_empty() {
        let registry = NodeRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_node_success() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        let caps = make_capabilities();

        let result = registry.register(node_id, caps);

        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_register_duplicate_node_fails() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        let caps = make_capabilities();

        registry.register(node_id, caps.clone()).unwrap();
        let result = registry.register(node_id, caps);

        assert!(matches!(result, Err(RegistryError::AlreadyRegistered(_))));
    }

    #[test]
    fn test_register_multiple_nodes() {
        let mut registry = NodeRegistry::new();
        let node1 = NodeId::new();
        let node2 = NodeId::new();

        registry.register(node1, make_capabilities()).unwrap();
        registry.register(node2, make_capabilities()).unwrap();

        assert_eq!(registry.len(), 2);
    }

    // ==================== Unregister Tests ====================

    #[test]
    fn test_unregister_existing_node() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let result = registry.unregister(node_id);

        assert!(result.is_ok());
        assert!(registry.is_empty());
    }

    #[test]
    fn test_unregister_nonexistent_node_fails() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();

        let result = registry.unregister(node_id);

        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    // ==================== Get Node Tests ====================

    #[test]
    fn test_get_node_returns_registered_node() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        let caps = make_capabilities_with_gpu("RTX 4090");
        registry.register(node_id, caps.clone()).unwrap();

        let node = registry.get_node(node_id);

        assert!(node.is_some());
        let node = node.unwrap();
        assert_eq!(node.id, node_id);
        assert_eq!(node.capabilities.cpu_cores, 8);
    }

    #[test]
    fn test_get_node_returns_none_for_unknown() {
        let registry = NodeRegistry::new();
        let node_id = NodeId::new();

        assert!(registry.get_node(node_id).is_none());
    }

    #[test]
    fn test_get_node_mut_allows_modification() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let original_heartbeat = registry.get_node(node_id).unwrap().last_heartbeat;

        // Allow some time to pass
        std::thread::sleep(std::time::Duration::from_millis(10));

        if let Some(node) = registry.get_node_mut(node_id) {
            node.update_heartbeat();
        }

        let new_heartbeat = registry.get_node(node_id).unwrap().last_heartbeat;
        assert!(new_heartbeat >= original_heartbeat);
    }

    // ==================== List Nodes Tests ====================

    #[test]
    fn test_list_nodes_empty_registry() {
        let registry = NodeRegistry::new();
        let nodes = registry.list_nodes();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_list_nodes_returns_all() {
        let mut registry = NodeRegistry::new();
        let node1 = NodeId::new();
        let node2 = NodeId::new();
        let node3 = NodeId::new();

        registry.register(node1, make_capabilities()).unwrap();
        registry.register(node2, make_capabilities()).unwrap();
        registry.register(node3, make_capabilities()).unwrap();

        let nodes = registry.list_nodes();

        assert_eq!(nodes.len(), 3);
    }

    // ==================== Find By GPU Tests ====================

    #[test]
    fn test_find_by_gpu_finds_matching_nodes() {
        let mut registry = NodeRegistry::new();

        let node1 = NodeId::new();
        let node2 = NodeId::new();
        let node3 = NodeId::new();

        registry
            .register(node1, make_capabilities_with_gpu("NVIDIA RTX 4090"))
            .unwrap();
        registry
            .register(node2, make_capabilities_with_gpu("NVIDIA A100"))
            .unwrap();
        registry
            .register(node3, make_capabilities_with_gpu("NVIDIA RTX 4090"))
            .unwrap();

        let rtx_nodes = registry.find_by_gpu("RTX 4090");

        assert_eq!(rtx_nodes.len(), 2);
    }

    #[test]
    fn test_find_by_gpu_returns_empty_when_no_match() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry
            .register(node_id, make_capabilities_with_gpu("NVIDIA RTX 4090"))
            .unwrap();

        let h100_nodes = registry.find_by_gpu("H100");

        assert!(h100_nodes.is_empty());
    }

    #[test]
    fn test_find_by_gpu_partial_match() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry
            .register(node_id, make_capabilities_with_gpu("NVIDIA GeForce RTX 4090 24GB"))
            .unwrap();

        // Should match partial name
        let nodes = registry.find_by_gpu("4090");
        assert_eq!(nodes.len(), 1);

        let nodes = registry.find_by_gpu("RTX");
        assert_eq!(nodes.len(), 1);

        let nodes = registry.find_by_gpu("NVIDIA");
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_find_by_gpu_on_empty_registry() {
        let registry = NodeRegistry::new();
        let nodes = registry.find_by_gpu("RTX 4090");
        assert!(nodes.is_empty());
    }

    // ==================== Heartbeat Tests ====================

    #[test]
    fn test_heartbeat_updates_timestamp() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        let original = registry.get_node(node_id).unwrap().last_heartbeat;

        std::thread::sleep(std::time::Duration::from_millis(10));
        registry.heartbeat(node_id).unwrap();

        let updated = registry.get_node(node_id).unwrap().last_heartbeat;
        assert!(updated >= original);
    }

    #[test]
    fn test_heartbeat_nonexistent_node_fails() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();

        let result = registry.heartbeat(node_id);

        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    // ==================== RegisteredNode Tests ====================

    #[test]
    fn test_registered_node_has_gpu_type() {
        let caps = NodeCapabilities::new(8, 16384)
            .with_gpu(make_gpu("NVIDIA RTX 4090", 24576))
            .with_gpu(make_gpu("NVIDIA RTX 4080", 16384));

        let node = RegisteredNode::new(NodeId::new(), "test".into(), caps);

        assert!(node.has_gpu_type("4090"));
        assert!(node.has_gpu_type("4080"));
        assert!(node.has_gpu_type("RTX"));
        assert!(!node.has_gpu_type("A100"));
    }

    #[test]
    fn test_registered_node_gpu_count() {
        let caps = NodeCapabilities::new(8, 16384)
            .with_gpu(make_gpu("GPU1", 8192))
            .with_gpu(make_gpu("GPU2", 8192));

        let node = RegisteredNode::new(NodeId::new(), "test".into(), caps);

        assert_eq!(node.gpu_count(), 2);
    }

    #[test]
    fn test_registered_node_no_gpus() {
        let caps = NodeCapabilities::new(4, 8192);
        let node = RegisteredNode::new(NodeId::new(), "test".into(), caps);

        assert_eq!(node.gpu_count(), 0);
        assert!(!node.has_gpu_type("any"));
    }

    // ==================== Health Status Tests ====================

    #[test]
    fn test_new_node_is_healthy() {
        let caps = NodeCapabilities::new(4, 8192);
        let node = RegisteredNode::new(NodeId::new(), "test".into(), caps);

        assert_eq!(node.health_status(), NodeHealthStatus::Healthy);
        assert!(node.is_available());
    }

    #[test]
    fn test_draining_node_status() {
        let caps = NodeCapabilities::new(4, 8192);
        let mut node = RegisteredNode::new(NodeId::new(), "test".into(), caps);
        node.draining = true;

        assert_eq!(node.health_status(), NodeHealthStatus::Draining);
        assert!(!node.is_available());
    }

    #[test]
    fn test_health_summary() {
        let mut registry = NodeRegistry::new();

        // Register 3 healthy nodes
        for _ in 0..3 {
            registry.register(NodeId::new(), make_capabilities()).unwrap();
        }

        // Mark one as draining
        let draining_id = NodeId::new();
        registry.register(draining_id, make_capabilities()).unwrap();
        registry.set_draining(draining_id, true).unwrap();

        let summary = registry.health_summary();
        assert_eq!(summary.healthy, 3);
        assert_eq!(summary.draining, 1);
        assert_eq!(summary.unhealthy, 0);
        assert_eq!(summary.offline, 0);
        assert_eq!(summary.total(), 4);
    }

    #[test]
    fn test_available_nodes_excludes_draining() {
        let mut registry = NodeRegistry::new();

        let node1 = NodeId::new();
        let node2 = NodeId::new();
        let node3 = NodeId::new();

        registry.register(node1, make_capabilities()).unwrap();
        registry.register(node2, make_capabilities()).unwrap();
        registry.register(node3, make_capabilities()).unwrap();

        // Mark node2 as draining
        registry.set_draining(node2, true).unwrap();

        let available = registry.available_nodes();
        assert_eq!(available.len(), 2);

        let available_ids: Vec<NodeId> = available.iter().map(|n| n.id).collect();
        assert!(available_ids.contains(&node1));
        assert!(!available_ids.contains(&node2));
        assert!(available_ids.contains(&node3));
    }

    // ==================== Drain Tests ====================

    #[test]
    fn test_set_draining() {
        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        registry.register(node_id, make_capabilities()).unwrap();

        assert!(!registry.is_draining(node_id).unwrap());

        registry.set_draining(node_id, true).unwrap();
        assert!(registry.is_draining(node_id).unwrap());

        registry.set_draining(node_id, false).unwrap();
        assert!(!registry.is_draining(node_id).unwrap());
    }

    #[test]
    fn test_set_draining_unknown_node() {
        let mut registry = NodeRegistry::new();
        let result = registry.set_draining(NodeId::new(), true);
        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    #[test]
    fn test_node_health_returns_none_for_unknown() {
        let registry = NodeRegistry::new();
        assert!(registry.node_health(NodeId::new()).is_none());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(NodeHealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(NodeHealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(NodeHealthStatus::Draining.to_string(), "draining");
        assert_eq!(NodeHealthStatus::Offline.to_string(), "offline");
    }
}
