//! `WireGuard` mesh network management for Clawbernetes.
//!
//! This crate provides the core networking infrastructure for Clawbernetes,
//! including:
//!
//! - **IP Allocation**: Manages IP address allocation from region-specific pools
//! - **Mesh Management**: Handles adding/removing nodes and peer relationships
//! - **Peer Discovery**: Enables nodes to discover each other automatically
//!
//! # IP Scheme
//!
//! ```text
//! 10.100.0.0/16    - Clawbernetes mesh
//! 10.100.0.0/24    - Gateway/control plane
//! 10.100.16.0/20   - Region: us-west (16-31)
//! 10.100.32.0/20   - Region: us-east (32-47)
//! 10.100.48.0/20   - Region: eu-west (48-63)
//! 10.100.64.0/20   - Region: asia (64-79)
//! 10.100.128.0/17  - MOLT marketplace providers (128-255)
//!
//! 10.200.{node}.0/24 - Per-node workload subnet
//! ```
//!
//! # Example
//!
//! ```rust
//! use claw_network::{
//!     WireGuardMesh, MeshConfig, MeshNode, Region, WireGuardKey,
//! };
//!
//! // Create a new mesh
//! let config = MeshConfig::default();
//! let mesh = WireGuardMesh::new(config).expect("failed to create mesh");
//!
//! // Allocate an IP for a new node
//! let allocator = mesh.allocator();
//! let mesh_ip = allocator.allocate_node_ip(Region::UsWest).expect("allocation failed");
//!
//! // Create a node (in real usage, key would come from WireGuard)
//! let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
//!     .expect("valid key");
//!
//! let workload_subnet = allocator
//!     .allocate_workload_subnet(claw_network::NodeId::new())
//!     .expect("subnet allocation failed");
//!
//! let node = MeshNode::builder()
//!     .mesh_ip(mesh_ip)
//!     .workload_subnet(workload_subnet)
//!     .wireguard_key(key)
//!     .region(Region::UsWest)
//!     .build()
//!     .expect("failed to build node");
//!
//! // Add node to mesh
//! mesh.add_node(node).expect("failed to add node");
//!
//! // Query topology
//! assert_eq!(mesh.node_count(), 1);
//! ```

pub mod allocation;
pub mod discovery;
pub mod mesh;
pub mod types;

// Re-export main types for convenience
pub use allocation::{AllocationError, AllocationStats, IpAllocator, RegionStats};
pub use discovery::{
    DiscoveryConfig, DiscoveryError, DiscoverySource, MeshDiscovery, MoltDiscoveryProvider,
    PeerInfo, StubMoltDiscovery,
};
pub use mesh::{MeshError, WireGuardMesh};
pub use types::{
    MeshConfig, MeshNode, MeshNodeBuilder, MeshNodeBuilderError, MeshTopology, NodeId, Region,
    WireGuardKey, WireGuardKeyError,
};

/// Prelude module for convenient imports.
pub mod prelude {
    pub use crate::allocation::{AllocationError, IpAllocator};
    pub use crate::discovery::{DiscoveryConfig, MeshDiscovery};
    pub use crate::mesh::{MeshError, WireGuardMesh};
    pub use crate::types::{MeshConfig, MeshNode, MeshTopology, NodeId, Region, WireGuardKey};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_workflow() {
        // Create mesh
        let config = MeshConfig::default();
        let mesh = WireGuardMesh::new(config).expect("should create mesh");

        // Get allocator
        let allocator = mesh.allocator();

        // Allocate resources for node 1
        let node1_ip = allocator
            .allocate_node_ip(Region::UsWest)
            .expect("should allocate IP");
        let node1_id = NodeId::new();
        let node1_subnet = allocator
            .allocate_workload_subnet(node1_id)
            .expect("should allocate subnet");

        let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid key");

        let node1 = MeshNode::builder()
            .node_id(node1_id)
            .mesh_ip(node1_ip)
            .workload_subnet(node1_subnet)
            .wireguard_key(key.clone())
            .region(Region::UsWest)
            .build()
            .expect("should build node");

        mesh.add_node(node1).expect("should add node");

        // Allocate resources for node 2
        let node2_ip = allocator
            .allocate_node_ip(Region::UsEast)
            .expect("should allocate IP");
        let node2_id = NodeId::new();
        let node2_subnet = allocator
            .allocate_workload_subnet(node2_id)
            .expect("should allocate subnet");

        let node2 = MeshNode::builder()
            .node_id(node2_id)
            .mesh_ip(node2_ip)
            .workload_subnet(node2_subnet)
            .wireguard_key(key.clone())
            .region(Region::UsEast)
            .build()
            .expect("should build node");

        mesh.add_node(node2).expect("should add node");

        // Verify topology
        assert_eq!(mesh.node_count(), 2);

        // Get peers for node 1
        let peers = mesh.get_peers_for_node(node1_id);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].node_id, node2_id);

        // Remove node 2
        mesh.remove_node(node2_id).expect("should remove node");
        assert_eq!(mesh.node_count(), 1);

        // Verify IPs were released
        assert!(!allocator.is_allocated(node2_ip));
    }

    #[test]
    fn test_gateway_workflow() {
        let config = MeshConfig::default();
        let mesh = WireGuardMesh::new(config).expect("should create mesh");
        let allocator = mesh.allocator();

        // Create gateway node
        let gateway_ip = allocator
            .allocate_node_ip(Region::Gateway)
            .expect("should allocate IP");
        let gateway_id = NodeId::new();
        let gateway_subnet = allocator
            .allocate_workload_subnet(gateway_id)
            .expect("should allocate subnet");

        let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid key");

        let gateway = MeshNode::builder()
            .node_id(gateway_id)
            .mesh_ip(gateway_ip)
            .workload_subnet(gateway_subnet)
            .wireguard_key(key)
            .region(Region::Gateway)
            .build()
            .expect("should build node");

        mesh.add_node(gateway).expect("should add gateway");
        mesh.set_gateway(gateway_id).expect("should set gateway");

        assert_eq!(mesh.gateway(), Some(gateway_id));

        // Cannot remove gateway
        let result = mesh.remove_node(gateway_id);
        assert!(matches!(result, Err(MeshError::CannotRemoveGateway)));

        // Clear gateway first, then remove
        mesh.clear_gateway();
        mesh.remove_node(gateway_id).expect("should remove");
    }

    #[test]
    fn test_discovery_integration() {
        use std::net::SocketAddr;

        // Create discovery service
        let gateway_addr: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(gateway_addr);

        // Create and announce our node
        let key = WireGuardKey::new("YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=")
            .expect("valid key");

        let our_node = MeshNode::builder()
            .mesh_ip("10.100.2.1".parse().expect("valid IP"))
            .workload_subnet("10.200.1.0/24".parse().expect("valid subnet"))
            .wireguard_key(key.clone())
            .region(Region::UsWest)
            .build()
            .expect("should build node");

        discovery.announce_self(&our_node).expect("should announce");

        // Simulate discovering other peers
        let peer1 = MeshNode::builder()
            .mesh_ip("10.100.2.2".parse().expect("valid IP"))
            .workload_subnet("10.200.2.0/24".parse().expect("valid subnet"))
            .wireguard_key(key.clone())
            .region(Region::UsWest)
            .build()
            .expect("should build node");

        let peer2 = MeshNode::builder()
            .mesh_ip("10.100.128.1".parse().expect("valid IP"))
            .workload_subnet("10.200.3.0/24".parse().expect("valid subnet"))
            .wireguard_key(key)
            .region(Region::Molt)
            .build()
            .expect("should build node");

        discovery.register_peer(peer1, DiscoverySource::Gateway);
        discovery.register_peer(peer2, DiscoverySource::MoltP2p);

        // Discover peers
        let peers = discovery.discover_peers().expect("should discover");
        assert_eq!(peers.len(), 2);

        // Check regional peers
        let us_west_peers = discovery.peers_in_region(Region::UsWest);
        assert_eq!(us_west_peers.len(), 1);

        let molt_peers = discovery.peers_in_region(Region::Molt);
        assert_eq!(molt_peers.len(), 1);
    }

    #[test]
    fn test_molt_region_allocation() {
        let allocator = IpAllocator::new().expect("should create allocator");

        // Allocate several MOLT IPs
        let mut ips = Vec::new();
        for _ in 0..10 {
            let ip = allocator
                .allocate_node_ip(Region::Molt)
                .expect("should allocate");
            ips.push(ip);
        }

        // All should be unique and in MOLT range
        for (i, ip) in ips.iter().enumerate() {
            // Check uniqueness
            for (j, other) in ips.iter().enumerate() {
                if i != j {
                    assert_ne!(ip, other);
                }
            }

            // Check range
            if let std::net::IpAddr::V4(v4) = ip {
                let octets = v4.octets();
                assert_eq!(octets[0], 10);
                assert_eq!(octets[1], 100);
                assert!((128..=255).contains(&octets[2]));
            } else {
                panic!("expected IPv4");
            }
        }
    }
}
