//! `WireGuard` mesh network management.
//!
//! This module provides the core mesh management functionality,
//! including adding/removing nodes and computing peer relationships.

use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::allocation::{AllocationError, IpAllocator};
use crate::types::{MeshConfig, MeshNode, MeshTopology, NodeId, Region};

/// Errors that can occur during mesh operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MeshError {
    /// Node already exists in the mesh.
    #[error("node {node_id} already exists in mesh")]
    NodeAlreadyExists {
        /// The node ID that already exists.
        node_id: NodeId,
    },
    /// Node not found in the mesh.
    #[error("node {node_id} not found in mesh")]
    NodeNotFound {
        /// The node ID that was not found.
        node_id: NodeId,
    },
    /// IP allocation failed.
    #[error("IP allocation failed: {0}")]
    AllocationFailed(#[from] AllocationError),
    /// Cannot remove the gateway node.
    #[error("cannot remove gateway node")]
    CannotRemoveGateway,
}

/// Internal state of the mesh.
#[derive(Debug)]
struct MeshState {
    topology: MeshTopology,
}

/// `WireGuard` mesh network manager.
#[derive(Debug)]
pub struct WireGuardMesh {
    /// Mesh configuration.
    config: MeshConfig,
    /// IP allocator for mesh and workload IPs.
    allocator: Arc<IpAllocator>,
    /// Internal mesh state.
    state: RwLock<MeshState>,
}

impl WireGuardMesh {
    /// Creates a new `WireGuard` mesh with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the IP allocator cannot be created.
    pub fn new(config: MeshConfig) -> Result<Self, MeshError> {
        let allocator = IpAllocator::new()?;

        Ok(Self {
            config,
            allocator: Arc::new(allocator),
            state: RwLock::new(MeshState {
                topology: MeshTopology::new(),
            }),
        })
    }

    /// Returns the mesh configuration.
    #[must_use]
    pub fn config(&self) -> &MeshConfig {
        &self.config
    }

    /// Returns a reference to the IP allocator.
    #[must_use]
    pub fn allocator(&self) -> &Arc<IpAllocator> {
        &self.allocator
    }

    /// Adds a node to the mesh.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The node already exists
    pub fn add_node(&self, node: MeshNode) -> Result<(), MeshError> {
        let mut state = self.state.write();

        if state.topology.nodes.contains_key(&node.node_id) {
            return Err(MeshError::NodeAlreadyExists {
                node_id: node.node_id,
            });
        }

        info!(
            node_id = %node.node_id,
            mesh_ip = %node.mesh_ip,
            region = %node.region,
            "Adding node to mesh"
        );

        state.topology.nodes.insert(node.node_id, node);

        Ok(())
    }

    /// Removes a node from the mesh.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The node doesn't exist
    /// - The node is the gateway
    pub fn remove_node(&self, node_id: NodeId) -> Result<MeshNode, MeshError> {
        let mut state = self.state.write();

        // Check if it's the gateway
        if state.topology.gateway == Some(node_id) {
            return Err(MeshError::CannotRemoveGateway);
        }

        let node = state
            .topology
            .nodes
            .remove(&node_id)
            .ok_or(MeshError::NodeNotFound { node_id })?;

        info!(
            node_id = %node_id,
            mesh_ip = %node.mesh_ip,
            "Removed node from mesh"
        );

        // Release the IP back to the pool
        if let Err(e) = self.allocator.release_ip(node.mesh_ip) {
            warn!(
                node_id = %node_id,
                mesh_ip = %node.mesh_ip,
                error = %e,
                "Failed to release mesh IP"
            );
        }

        // Release workload subnet
        if let Err(e) = self.allocator.release_workload_subnet(node_id) {
            warn!(
                node_id = %node_id,
                error = %e,
                "Failed to release workload subnet"
            );
        }

        Ok(node)
    }

    /// Gets a node by ID.
    #[must_use]
    pub fn get_node(&self, node_id: NodeId) -> Option<MeshNode> {
        let state = self.state.read();
        state.topology.nodes.get(&node_id).cloned()
    }

    /// Sets the gateway node.
    ///
    /// # Errors
    ///
    /// Returns an error if the node doesn't exist.
    pub fn set_gateway(&self, node_id: NodeId) -> Result<(), MeshError> {
        let mut state = self.state.write();

        if !state.topology.nodes.contains_key(&node_id) {
            return Err(MeshError::NodeNotFound { node_id });
        }

        debug!(node_id = %node_id, "Setting gateway node");
        state.topology.gateway = Some(node_id);

        Ok(())
    }

    /// Clears the gateway designation.
    pub fn clear_gateway(&self) {
        let mut state = self.state.write();
        state.topology.gateway = None;
    }

    /// Returns the current gateway node ID.
    #[must_use]
    pub fn gateway(&self) -> Option<NodeId> {
        let state = self.state.read();
        state.topology.gateway
    }

    /// Returns peers for a given node (all other nodes in the mesh).
    ///
    /// In a full mesh topology, every node is a peer of every other node.
    #[must_use]
    pub fn get_peers_for_node(&self, node_id: NodeId) -> Vec<MeshNode> {
        let state = self.state.read();

        state
            .topology
            .nodes
            .iter()
            .filter(|(id, _)| **id != node_id)
            .map(|(_, node)| node.clone())
            .collect()
    }

    /// Returns peers in a specific region for a given node.
    #[must_use]
    pub fn get_regional_peers(&self, node_id: NodeId, region: Region) -> Vec<MeshNode> {
        let state = self.state.read();

        state
            .topology
            .nodes
            .iter()
            .filter(|(id, node)| **id != node_id && node.region == region)
            .map(|(_, node)| node.clone())
            .collect()
    }

    /// Returns a clone of the current topology.
    #[must_use]
    pub fn get_topology(&self) -> MeshTopology {
        let state = self.state.read();
        state.topology.clone()
    }

    /// Returns a reference to the topology (via closure to avoid holding lock).
    pub fn with_topology<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&MeshTopology) -> R,
    {
        let state = self.state.read();
        f(&state.topology)
    }

    /// Returns the number of nodes in the mesh.
    #[must_use]
    pub fn node_count(&self) -> usize {
        let state = self.state.read();
        state.topology.node_count()
    }

    /// Checks if a node exists in the mesh.
    #[must_use]
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        let state = self.state.read();
        state.topology.nodes.contains_key(&node_id)
    }

    /// Returns all node IDs in the mesh.
    #[must_use]
    pub fn node_ids(&self) -> Vec<NodeId> {
        let state = self.state.read();
        state.topology.nodes.keys().copied().collect()
    }

    /// Updates a node's last seen timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error if the node doesn't exist.
    pub fn touch_node(&self, node_id: NodeId) -> Result<(), MeshError> {
        let mut state = self.state.write();

        let node = state
            .topology
            .nodes
            .get_mut(&node_id)
            .ok_or(MeshError::NodeNotFound { node_id })?;

        node.last_seen = chrono::Utc::now();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WireGuardKey;

    fn test_key() -> WireGuardKey {
        WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid test key")
    }

    fn test_node(region: Region, mesh_ip: &str, workload_subnet: &str) -> MeshNode {
        MeshNode::builder()
            .mesh_ip(mesh_ip.parse().expect("valid IP"))
            .workload_subnet(workload_subnet.parse().expect("valid subnet"))
            .wireguard_key(test_key())
            .region(region)
            .build()
            .expect("should build node")
    }

    // ==================== CREATION TESTS ====================

    #[test]
    fn test_mesh_creates_with_default_config() {
        let mesh = WireGuardMesh::new(MeshConfig::default());
        assert!(mesh.is_ok());
    }

    #[test]
    fn test_mesh_starts_empty() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        assert_eq!(mesh.node_count(), 0);
        assert!(mesh.gateway().is_none());
    }

    // ==================== ADD NODE TESTS ====================

    #[test]
    fn test_add_node_succeeds() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");

        let result = mesh.add_node(node);
        assert!(result.is_ok());
        assert_eq!(mesh.node_count(), 1);
    }

    #[test]
    fn test_add_duplicate_node_fails() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node_id = node.node_id;

        mesh.add_node(node.clone()).expect("first add should succeed");
        let result = mesh.add_node(node);

        assert!(matches!(
            result,
            Err(MeshError::NodeAlreadyExists { node_id: id }) if id == node_id
        ));
    }

    #[test]
    fn test_add_multiple_nodes() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsEast, "10.100.18.1", "10.200.2.0/24");
        let node3 = test_node(Region::Molt, "10.100.128.1", "10.200.3.0/24");

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");
        mesh.add_node(node3).expect("should add");

        assert_eq!(mesh.node_count(), 3);
    }

    // ==================== REMOVE NODE TESTS ====================

    #[test]
    fn test_remove_node_succeeds() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node_id = node.node_id;

        mesh.add_node(node).expect("should add");
        assert_eq!(mesh.node_count(), 1);

        let removed = mesh.remove_node(node_id);
        assert!(removed.is_ok());
        assert_eq!(mesh.node_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_node_fails() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node_id = NodeId::new();

        let result = mesh.remove_node(node_id);
        assert!(matches!(result, Err(MeshError::NodeNotFound { .. })));
    }

    #[test]
    fn test_remove_gateway_node_fails() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::Gateway, "10.100.0.1", "10.200.1.0/24");
        let node_id = node.node_id;

        mesh.add_node(node).expect("should add");
        mesh.set_gateway(node_id).expect("should set gateway");

        let result = mesh.remove_node(node_id);
        assert!(matches!(result, Err(MeshError::CannotRemoveGateway)));
    }

    // ==================== GATEWAY TESTS ====================

    #[test]
    fn test_set_gateway() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::Gateway, "10.100.0.1", "10.200.1.0/24");
        let node_id = node.node_id;

        mesh.add_node(node).expect("should add");
        mesh.set_gateway(node_id).expect("should set gateway");

        assert_eq!(mesh.gateway(), Some(node_id));
    }

    #[test]
    fn test_set_gateway_nonexistent_node_fails() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node_id = NodeId::new();

        let result = mesh.set_gateway(node_id);
        assert!(matches!(result, Err(MeshError::NodeNotFound { .. })));
    }

    #[test]
    fn test_clear_gateway() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::Gateway, "10.100.0.1", "10.200.1.0/24");
        let node_id = node.node_id;

        mesh.add_node(node).expect("should add");
        mesh.set_gateway(node_id).expect("should set gateway");
        assert!(mesh.gateway().is_some());

        mesh.clear_gateway();
        assert!(mesh.gateway().is_none());
    }

    // ==================== PEER TESTS ====================

    #[test]
    fn test_get_peers_for_node_returns_all_others() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsWest, "10.100.2.2", "10.200.2.0/24");
        let node3 = test_node(Region::UsEast, "10.100.18.1", "10.200.3.0/24");

        let node1_id = node1.node_id;

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");
        mesh.add_node(node3).expect("should add");

        let peers = mesh.get_peers_for_node(node1_id);
        assert_eq!(peers.len(), 2);

        // Should not include node1 itself
        assert!(!peers.iter().any(|p| p.node_id == node1_id));
    }

    #[test]
    fn test_get_peers_for_nonexistent_node_returns_all() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsEast, "10.100.18.1", "10.200.2.0/24");

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");

        // Query for nonexistent node - returns all nodes
        let peers = mesh.get_peers_for_node(NodeId::new());
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_get_regional_peers() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsWest, "10.100.2.2", "10.200.2.0/24");
        let node3 = test_node(Region::UsEast, "10.100.18.1", "10.200.3.0/24");

        let node1_id = node1.node_id;

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");
        mesh.add_node(node3).expect("should add");

        let us_west_peers = mesh.get_regional_peers(node1_id, Region::UsWest);
        assert_eq!(us_west_peers.len(), 1);

        let us_east_peers = mesh.get_regional_peers(node1_id, Region::UsEast);
        assert_eq!(us_east_peers.len(), 1);
    }

    // ==================== TOPOLOGY TESTS ====================

    #[test]
    fn test_get_topology_returns_clone() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");

        mesh.add_node(node).expect("should add");

        let topology = mesh.get_topology();
        assert_eq!(topology.node_count(), 1);
    }

    #[test]
    fn test_with_topology_closure() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsEast, "10.100.18.1", "10.200.2.0/24");

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");

        let count = mesh.with_topology(|t| t.node_count());
        assert_eq!(count, 2);

        let us_west_count =
            mesh.with_topology(|t| t.nodes_in_region(Region::UsWest).count());
        assert_eq!(us_west_count, 1);
    }

    // ==================== GET NODE TESTS ====================

    #[test]
    fn test_get_node_returns_clone() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node_id = node.node_id;
        let mesh_ip = node.mesh_ip;

        mesh.add_node(node).expect("should add");

        let retrieved = mesh.get_node(node_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.expect("should exist").mesh_ip, mesh_ip);
    }

    #[test]
    fn test_get_nonexistent_node_returns_none() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let result = mesh.get_node(NodeId::new());
        assert!(result.is_none());
    }

    // ==================== TOUCH NODE TESTS ====================

    #[test]
    fn test_touch_node_updates_last_seen() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node_id = node.node_id;
        let original_last_seen = node.last_seen;

        mesh.add_node(node).expect("should add");

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        mesh.touch_node(node_id).expect("should touch");

        let updated = mesh.get_node(node_id).expect("should exist");
        assert!(updated.last_seen > original_last_seen);
    }

    #[test]
    fn test_touch_nonexistent_node_fails() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let result = mesh.touch_node(NodeId::new());
        assert!(matches!(result, Err(MeshError::NodeNotFound { .. })));
    }

    // ==================== CONTAINS/NODE_IDS TESTS ====================

    #[test]
    fn test_contains_node() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");
        let node = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node_id = node.node_id;

        assert!(!mesh.contains_node(node_id));

        mesh.add_node(node).expect("should add");

        assert!(mesh.contains_node(node_id));
    }

    #[test]
    fn test_node_ids_returns_all_ids() {
        let mesh = WireGuardMesh::new(MeshConfig::default()).expect("should create mesh");

        let node1 = test_node(Region::UsWest, "10.100.2.1", "10.200.1.0/24");
        let node2 = test_node(Region::UsEast, "10.100.18.1", "10.200.2.0/24");

        let id1 = node1.node_id;
        let id2 = node2.node_id;

        mesh.add_node(node1).expect("should add");
        mesh.add_node(node2).expect("should add");

        let ids = mesh.node_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }
}
