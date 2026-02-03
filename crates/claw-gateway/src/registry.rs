//! Node fleet registry for tracking registered nodes and their capabilities.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use claw_proto::{NodeCapabilities, NodeId};
use thiserror::Error;

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

/// A node that has been registered with the gateway.
#[derive(Debug, Clone)]
pub struct RegisteredNode {
    /// The node's unique identifier.
    pub id: NodeId,
    /// The node's capabilities.
    pub capabilities: NodeCapabilities,
    /// When the node was registered.
    pub registered_at: DateTime<Utc>,
    /// Last heartbeat timestamp.
    pub last_heartbeat: DateTime<Utc>,
}

impl RegisteredNode {
    /// Create a new registered node.
    fn new(id: NodeId, capabilities: NodeCapabilities) -> Self {
        let now = Utc::now();
        Self {
            id,
            capabilities,
            registered_at: now,
            last_heartbeat: now,
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
        if self.nodes.contains_key(&node_id) {
            return Err(RegistryError::AlreadyRegistered(node_id));
        }

        let node = RegisteredNode::new(node_id, capabilities);
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

        let node = RegisteredNode::new(NodeId::new(), caps);

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

        let node = RegisteredNode::new(NodeId::new(), caps);

        assert_eq!(node.gpu_count(), 2);
    }

    #[test]
    fn test_registered_node_no_gpus() {
        let caps = NodeCapabilities::new(4, 8192);
        let node = RegisteredNode::new(NodeId::new(), caps);

        assert_eq!(node.gpu_count(), 0);
        assert!(!node.has_gpu_type("any"));
    }
}
