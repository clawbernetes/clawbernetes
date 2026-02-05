//! Mesh network topology types and algorithms.
//!
//! This module provides types for defining `WireGuard` mesh network topologies,
//! including full mesh (where every node connects to every other node) and
//! hub-spoke patterns.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::config::PeerConfig;
use crate::error::{Result, WireGuardError};
use crate::keys::PublicKey;
use crate::types::{AllowedIp, Endpoint};

/// Maximum nodes for automatic full mesh topology.
/// Beyond this, hub-spoke or manual configuration is recommended.
pub const MAX_FULL_MESH_NODES: usize = 50;

/// Mesh topology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TopologyType {
    /// Every node connects to every other node.
    #[default]
    FullMesh,
    /// All nodes connect through a central hub.
    HubSpoke,
    /// Custom topology defined manually.
    Custom,
}


impl fmt::Display for TopologyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FullMesh => write!(f, "full_mesh"),
            Self::HubSpoke => write!(f, "hub_spoke"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Unique identifier for a mesh node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeshNodeId(String);

impl MeshNodeId {
    /// Creates a new mesh node ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MeshNodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for MeshNodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MeshNodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A node in the mesh network.
#[derive(Debug, Clone)]
pub struct MeshNode {
    /// Unique identifier for this node.
    pub id: MeshNodeId,
    /// Human-readable name.
    pub name: String,
    /// The node's `WireGuard` public key.
    pub public_key: PublicKey,
    /// The node's `WireGuard` endpoint (external IP:port).
    pub endpoint: Option<Endpoint>,
    /// The node's mesh-internal IP address.
    pub mesh_ip: IpAddr,
    /// Whether this node acts as a hub (for hub-spoke topology).
    pub is_hub: bool,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

impl MeshNode {
    /// Creates a new mesh node.
    #[must_use]
    pub fn new(
        id: impl Into<MeshNodeId>,
        name: impl Into<String>,
        public_key: PublicKey,
        mesh_ip: IpAddr,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            public_key,
            endpoint: None,
            mesh_ip,
            is_hub: false,
            metadata: HashMap::new(),
        }
    }

    /// Sets the endpoint for this node.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Marks this node as a hub.
    #[must_use]
    pub fn as_hub(mut self) -> Self {
        self.is_hub = true;
        self
    }

    /// Adds metadata to this node.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Converts this mesh node to a `WireGuard` peer config.
    #[must_use]
    pub fn to_peer_config(&self, keepalive: Option<u16>) -> PeerConfig {
        let mut config = PeerConfig::new(self.public_key);
        
        // Add the mesh IP as allowed
        if let Ok(allowed) = AllowedIp::from_cidr(&format!("{}/32", self.mesh_ip)) {
            config.allowed_ips.push(allowed);
        }
        
        config.endpoint.clone_from(&self.endpoint);
        config.persistent_keepalive = keepalive;
        config
    }
}

/// Definition of a mesh network topology.
#[derive(Debug, Clone)]
pub struct MeshTopology {
    /// The type of topology.
    pub topology_type: TopologyType,
    /// All nodes in the mesh.
    nodes: HashMap<MeshNodeId, MeshNode>,
    /// Explicit connections (for custom topologies).
    /// Maps from node ID to set of connected peer IDs.
    connections: HashMap<MeshNodeId, HashSet<MeshNodeId>>,
    /// The mesh network CIDR (e.g., "10.100.0.0/16").
    pub network_cidr: String,
    /// Default persistent keepalive interval.
    pub default_keepalive: Option<u16>,
}

impl MeshTopology {
    /// Creates a new full mesh topology.
    #[must_use]
    pub fn full_mesh(network_cidr: impl Into<String>) -> Self {
        Self {
            topology_type: TopologyType::FullMesh,
            nodes: HashMap::new(),
            connections: HashMap::new(),
            network_cidr: network_cidr.into(),
            default_keepalive: Some(25),
        }
    }

    /// Creates a new hub-spoke topology.
    #[must_use]
    pub fn hub_spoke(network_cidr: impl Into<String>) -> Self {
        Self {
            topology_type: TopologyType::HubSpoke,
            nodes: HashMap::new(),
            connections: HashMap::new(),
            network_cidr: network_cidr.into(),
            default_keepalive: Some(25),
        }
    }

    /// Creates a new custom topology.
    #[must_use]
    pub fn custom(network_cidr: impl Into<String>) -> Self {
        Self {
            topology_type: TopologyType::Custom,
            nodes: HashMap::new(),
            connections: HashMap::new(),
            network_cidr: network_cidr.into(),
            default_keepalive: Some(25),
        }
    }

    /// Sets the default keepalive interval.
    #[must_use]
    pub fn with_keepalive(mut self, seconds: u16) -> Self {
        self.default_keepalive = Some(seconds);
        self
    }

    /// Disables keepalive.
    #[must_use]
    pub fn without_keepalive(mut self) -> Self {
        self.default_keepalive = None;
        self
    }

    /// Adds a node to the topology.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A node with the same ID already exists
    /// - Adding would exceed `MAX_FULL_MESH_NODES` for full mesh
    pub fn add_node(&mut self, node: MeshNode) -> Result<()> {
        if self.nodes.contains_key(&node.id) {
            return Err(WireGuardError::PeerExists(node.id.to_string()));
        }

        if self.topology_type == TopologyType::FullMesh && self.nodes.len() >= MAX_FULL_MESH_NODES {
            return Err(WireGuardError::InvalidConfig(format!(
                "full mesh topology limited to {MAX_FULL_MESH_NODES} nodes"
            )));
        }

        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Removes a node from the topology.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub fn remove_node(&mut self, node_id: &MeshNodeId) -> Result<MeshNode> {
        let node = self
            .nodes
            .remove(node_id)
            .ok_or_else(|| WireGuardError::PeerNotFound(node_id.to_string()))?;

        // Remove from connections
        self.connections.remove(node_id);
        for peers in self.connections.values_mut() {
            peers.remove(node_id);
        }

        Ok(node)
    }

    /// Gets a node by ID.
    #[must_use]
    pub fn get_node(&self, node_id: &MeshNodeId) -> Option<&MeshNode> {
        self.nodes.get(node_id)
    }

    /// Gets a mutable reference to a node by ID.
    #[must_use]
    pub fn get_node_mut(&mut self, node_id: &MeshNodeId) -> Option<&mut MeshNode> {
        self.nodes.get_mut(node_id)
    }

    /// Returns all nodes in the topology.
    #[must_use]
    pub fn nodes(&self) -> Vec<&MeshNode> {
        self.nodes.values().collect()
    }

    /// Returns the number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether the topology is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Adds an explicit connection between two nodes (for custom topology).
    ///
    /// # Errors
    ///
    /// Returns an error if either node doesn't exist.
    pub fn add_connection(&mut self, from: &MeshNodeId, to: &MeshNodeId) -> Result<()> {
        if !self.nodes.contains_key(from) {
            return Err(WireGuardError::PeerNotFound(from.to_string()));
        }
        if !self.nodes.contains_key(to) {
            return Err(WireGuardError::PeerNotFound(to.to_string()));
        }

        // Add bidirectional connection
        self.connections
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.connections
            .entry(to.clone())
            .or_default()
            .insert(from.clone());

        Ok(())
    }

    /// Removes a connection between two nodes.
    pub fn remove_connection(&mut self, from: &MeshNodeId, to: &MeshNodeId) {
        if let Some(peers) = self.connections.get_mut(from) {
            peers.remove(to);
        }
        if let Some(peers) = self.connections.get_mut(to) {
            peers.remove(from);
        }
    }

    /// Gets the peers for a specific node based on the topology type.
    #[must_use]
    pub fn get_peers_for(&self, node_id: &MeshNodeId) -> Vec<&MeshNode> {
        match self.topology_type {
            TopologyType::FullMesh => {
                // In full mesh, every node connects to every other node
                self.nodes
                    .values()
                    .filter(|n| &n.id != node_id)
                    .collect()
            }
            TopologyType::HubSpoke => {
                let Some(node) = self.nodes.get(node_id) else {
                    return Vec::new();
                };

                if node.is_hub {
                    // Hub connects to all spoke nodes
                    self.nodes
                        .values()
                        .filter(|n| &n.id != node_id && !n.is_hub)
                        .collect()
                } else {
                    // Spoke connects only to hubs
                    self.nodes.values().filter(|n| n.is_hub).collect()
                }
            }
            TopologyType::Custom => {
                // Use explicit connections
                self.connections
                    .get(node_id)
                    .map(|peer_ids| {
                        peer_ids
                            .iter()
                            .filter_map(|pid| self.nodes.get(pid))
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }

    /// Generates peer configs for a specific node.
    #[must_use]
    pub fn generate_peer_configs(&self, node_id: &MeshNodeId) -> Vec<PeerConfig> {
        self.get_peers_for(node_id)
            .into_iter()
            .map(|peer| peer.to_peer_config(self.default_keepalive))
            .collect()
    }

    /// Returns the number of connections in the mesh.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        match self.topology_type {
            TopologyType::FullMesh => {
                let n = self.nodes.len();
                if n < 2 {
                    0
                } else {
                    n * (n - 1) / 2
                }
            }
            TopologyType::HubSpoke => {
                let hubs = self.nodes.values().filter(|n| n.is_hub).count();
                let spokes = self.nodes.len() - hubs;
                hubs * spokes
            }
            TopologyType::Custom => {
                // Count unique connections (bidirectional counted once)
                let mut seen = HashSet::new();
                for (from, peers) in &self.connections {
                    for to in peers {
                        let key = if from.as_str() < to.as_str() {
                            (from.clone(), to.clone())
                        } else {
                            (to.clone(), from.clone())
                        };
                        seen.insert(key);
                    }
                }
                seen.len()
            }
        }
    }

    /// Finds a hub node (for hub-spoke topology).
    #[must_use]
    pub fn find_hub(&self) -> Option<&MeshNode> {
        self.nodes.values().find(|n| n.is_hub)
    }

    /// Returns all hub nodes.
    #[must_use]
    pub fn hubs(&self) -> Vec<&MeshNode> {
        self.nodes.values().filter(|n| n.is_hub).collect()
    }

    /// Validates the topology.
    ///
    /// # Errors
    ///
    /// Returns an error if the topology is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.nodes.is_empty() {
            return Err(WireGuardError::InvalidConfig(
                "topology has no nodes".to_string(),
            ));
        }

        if self.topology_type == TopologyType::HubSpoke {
            let hub_count = self.nodes.values().filter(|n| n.is_hub).count();
            if hub_count == 0 {
                return Err(WireGuardError::InvalidConfig(
                    "hub-spoke topology requires at least one hub".to_string(),
                ));
            }
        }

        // Check for duplicate public keys
        let mut seen_keys = HashSet::new();
        for node in self.nodes.values() {
            let key_b64 = node.public_key.to_base64();
            if seen_keys.contains(&key_b64) {
                return Err(WireGuardError::InvalidConfig(format!(
                    "duplicate public key found: {}",
                    &key_b64[..8]
                )));
            }
            seen_keys.insert(key_b64);
        }

        // Check for duplicate mesh IPs
        let mut seen_ips = HashSet::new();
        for node in self.nodes.values() {
            if seen_ips.contains(&node.mesh_ip) {
                return Err(WireGuardError::InvalidConfig(format!(
                    "duplicate mesh IP found: {}",
                    node.mesh_ip
                )));
            }
            seen_ips.insert(node.mesh_ip);
        }

        Ok(())
    }
}

/// IP address allocator for mesh networks.
#[derive(Debug, Clone)]
pub struct MeshIpAllocator {
    /// Base network address.
    base: IpAddr,
    /// Next available host number.
    next_host: u32,
    /// Allocated addresses.
    allocated: HashSet<IpAddr>,
}

impl MeshIpAllocator {
    /// Creates a new allocator for an IPv4 network.
    ///
    /// # Errors
    ///
    /// Returns an error if the CIDR is invalid.
    pub fn new_v4(network_cidr: &str) -> Result<Self> {
        let network = network_cidr
            .parse::<ipnet::IpNet>()
            .map_err(|e| WireGuardError::InvalidCidr(e.to_string()))?;

        let base = match network.network() {
            IpAddr::V4(v4) => IpAddr::V4(v4),
            IpAddr::V6(_) => {
                return Err(WireGuardError::InvalidConfig(
                    "expected IPv4 network".to_string(),
                ))
            }
        };

        Ok(Self {
            base,
            next_host: 1, // Start at .1
            allocated: HashSet::new(),
        })
    }

    /// Allocates the next available IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if no more addresses are available.
    pub fn allocate(&mut self) -> Result<IpAddr> {
        let IpAddr::V4(base_v4) = self.base else {
            return Err(WireGuardError::InvalidConfig(
                "IPv6 not supported yet".to_string(),
            ));
        };

        let base_u32 = u32::from(base_v4);

        // Try to find an available address
        for _ in 0..256 {
            let candidate = IpAddr::V4(std::net::Ipv4Addr::from(
                base_u32.saturating_add(self.next_host),
            ));
            self.next_host = self.next_host.saturating_add(1);

            if !self.allocated.contains(&candidate) {
                self.allocated.insert(candidate);
                return Ok(candidate);
            }
        }

        Err(WireGuardError::InvalidConfig(
            "no more IP addresses available".to_string(),
        ))
    }

    /// Reserves a specific IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if the address is already allocated.
    pub fn reserve(&mut self, ip: IpAddr) -> Result<()> {
        if self.allocated.contains(&ip) {
            return Err(WireGuardError::InvalidConfig(format!(
                "IP address {ip} already allocated"
            )));
        }
        self.allocated.insert(ip);
        Ok(())
    }

    /// Releases an allocated IP address.
    pub fn release(&mut self, ip: &IpAddr) {
        self.allocated.remove(ip);
    }

    /// Returns the number of allocated addresses.
    #[must_use]
    pub fn allocated_count(&self) -> usize {
        self.allocated.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::PrivateKey;

    fn test_public_key() -> PublicKey {
        PrivateKey::generate().public_key()
    }

    fn test_node(id: &str, ip: &str) -> MeshNode {
        let pk = test_public_key();
        let mesh_ip: IpAddr = ip.parse().expect("valid ip");
        MeshNode::new(id, format!("Node {}", id), pk, mesh_ip)
    }

    // ==================== TopologyType Tests ====================

    #[test]
    fn topology_type_default() {
        assert_eq!(TopologyType::default(), TopologyType::FullMesh);
    }

    #[test]
    fn topology_type_display() {
        assert_eq!(TopologyType::FullMesh.to_string(), "full_mesh");
        assert_eq!(TopologyType::HubSpoke.to_string(), "hub_spoke");
        assert_eq!(TopologyType::Custom.to_string(), "custom");
    }

    // ==================== MeshNodeId Tests ====================

    #[test]
    fn mesh_node_id_new() {
        let id = MeshNodeId::new("node-1");
        assert_eq!(id.as_str(), "node-1");
        assert_eq!(id.to_string(), "node-1");
    }

    #[test]
    fn mesh_node_id_from_string() {
        let id: MeshNodeId = "node-2".into();
        assert_eq!(id.as_str(), "node-2");
    }

    // ==================== MeshNode Tests ====================

    #[test]
    fn mesh_node_new() {
        let pk = test_public_key();
        let ip: IpAddr = "10.0.0.1".parse().expect("valid ip");
        let node = MeshNode::new("node-1", "Node 1", pk, ip);

        assert_eq!(node.id.as_str(), "node-1");
        assert_eq!(node.name, "Node 1");
        assert_eq!(node.mesh_ip, ip);
        assert!(!node.is_hub);
        assert!(node.endpoint.is_none());
    }

    #[test]
    fn mesh_node_with_endpoint() {
        let pk = test_public_key();
        let ip: IpAddr = "10.0.0.1".parse().expect("valid ip");
        let endpoint: Endpoint = "192.168.1.1:51820".parse().expect("valid endpoint");

        let node = MeshNode::new("node-1", "Node 1", pk, ip).with_endpoint(endpoint.clone());

        assert_eq!(node.endpoint, Some(endpoint));
    }

    #[test]
    fn mesh_node_as_hub() {
        let node = test_node("hub", "10.0.0.1").as_hub();
        assert!(node.is_hub);
    }

    #[test]
    fn mesh_node_with_metadata() {
        let node = test_node("node-1", "10.0.0.1")
            .with_metadata("region", "us-west")
            .with_metadata("tier", "primary");

        assert_eq!(node.metadata.get("region"), Some(&"us-west".to_string()));
        assert_eq!(node.metadata.get("tier"), Some(&"primary".to_string()));
    }

    #[test]
    fn mesh_node_to_peer_config() {
        let endpoint: Endpoint = "192.168.1.1:51820".parse().expect("valid endpoint");
        let node = test_node("node-1", "10.0.0.1").with_endpoint(endpoint.clone());

        let config = node.to_peer_config(Some(25));

        assert_eq!(config.public_key, node.public_key);
        assert_eq!(config.endpoint, Some(endpoint));
        assert_eq!(config.persistent_keepalive, Some(25));
        assert_eq!(config.allowed_ips.len(), 1);
    }

    // ==================== MeshTopology Tests ====================

    #[test]
    fn mesh_topology_full_mesh_new() {
        let topo = MeshTopology::full_mesh("10.0.0.0/24");

        assert_eq!(topo.topology_type, TopologyType::FullMesh);
        assert_eq!(topo.network_cidr, "10.0.0.0/24");
        assert!(topo.is_empty());
        assert_eq!(topo.default_keepalive, Some(25));
    }

    #[test]
    fn mesh_topology_hub_spoke_new() {
        let topo = MeshTopology::hub_spoke("10.0.0.0/24");
        assert_eq!(topo.topology_type, TopologyType::HubSpoke);
    }

    #[test]
    fn mesh_topology_custom_new() {
        let topo = MeshTopology::custom("10.0.0.0/24");
        assert_eq!(topo.topology_type, TopologyType::Custom);
    }

    #[test]
    fn mesh_topology_with_keepalive() {
        let topo = MeshTopology::full_mesh("10.0.0.0/24").with_keepalive(30);
        assert_eq!(topo.default_keepalive, Some(30));
    }

    #[test]
    fn mesh_topology_without_keepalive() {
        let topo = MeshTopology::full_mesh("10.0.0.0/24").without_keepalive();
        assert_eq!(topo.default_keepalive, None);
    }

    #[test]
    fn mesh_topology_add_node() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let node = test_node("node-1", "10.0.0.1");

        assert!(topo.add_node(node).is_ok());
        assert_eq!(topo.node_count(), 1);
        assert!(!topo.is_empty());
    }

    #[test]
    fn mesh_topology_add_duplicate_node_fails() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-1", "10.0.0.2");

        assert!(topo.add_node(node1).is_ok());
        assert!(topo.add_node(node2).is_err());
    }

    #[test]
    fn mesh_topology_remove_node() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let node = test_node("node-1", "10.0.0.1");
        let node_id = node.id.clone();

        topo.add_node(node).expect("add node");
        let removed = topo.remove_node(&node_id);

        assert!(removed.is_ok());
        assert!(topo.is_empty());
    }

    #[test]
    fn mesh_topology_remove_nonexistent_node() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let result = topo.remove_node(&MeshNodeId::new("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn mesh_topology_get_node() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let node = test_node("node-1", "10.0.0.1");
        let node_id = node.id.clone();

        topo.add_node(node).expect("add node");

        assert!(topo.get_node(&node_id).is_some());
        assert!(topo.get_node(&MeshNodeId::new("nonexistent")).is_none());
    }

    #[test]
    fn mesh_topology_full_mesh_peers() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");

        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-2", "10.0.0.2");
        let node3 = test_node("node-3", "10.0.0.3");

        let id1 = node1.id.clone();
        let id2 = node2.id.clone();

        topo.add_node(node1).expect("add node1");
        topo.add_node(node2).expect("add node2");
        topo.add_node(node3).expect("add node3");

        // Each node should have 2 peers in full mesh
        assert_eq!(topo.get_peers_for(&id1).len(), 2);
        assert_eq!(topo.get_peers_for(&id2).len(), 2);
    }

    #[test]
    fn mesh_topology_hub_spoke_peers() {
        let mut topo = MeshTopology::hub_spoke("10.0.0.0/24");

        let hub = test_node("hub", "10.0.0.1").as_hub();
        let spoke1 = test_node("spoke-1", "10.0.0.2");
        let spoke2 = test_node("spoke-2", "10.0.0.3");

        let hub_id = hub.id.clone();
        let spoke1_id = spoke1.id.clone();

        topo.add_node(hub).expect("add hub");
        topo.add_node(spoke1).expect("add spoke1");
        topo.add_node(spoke2).expect("add spoke2");

        // Hub should have 2 peers (both spokes)
        assert_eq!(topo.get_peers_for(&hub_id).len(), 2);

        // Spoke should have 1 peer (the hub)
        assert_eq!(topo.get_peers_for(&spoke1_id).len(), 1);
    }

    #[test]
    fn mesh_topology_custom_peers() {
        let mut topo = MeshTopology::custom("10.0.0.0/24");

        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-2", "10.0.0.2");
        let node3 = test_node("node-3", "10.0.0.3");

        let id1 = node1.id.clone();
        let id2 = node2.id.clone();
        let id3 = node3.id.clone();

        topo.add_node(node1).expect("add node1");
        topo.add_node(node2).expect("add node2");
        topo.add_node(node3).expect("add node3");

        // No connections initially
        assert!(topo.get_peers_for(&id1).is_empty());

        // Add connection 1 <-> 2
        topo.add_connection(&id1, &id2).expect("add connection");

        assert_eq!(topo.get_peers_for(&id1).len(), 1);
        assert_eq!(topo.get_peers_for(&id2).len(), 1);
        assert!(topo.get_peers_for(&id3).is_empty());
    }

    #[test]
    fn mesh_topology_connection_count_full_mesh() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");

        // 0 nodes = 0 connections
        assert_eq!(topo.connection_count(), 0);

        topo.add_node(test_node("node-1", "10.0.0.1")).expect("add");
        // 1 node = 0 connections
        assert_eq!(topo.connection_count(), 0);

        topo.add_node(test_node("node-2", "10.0.0.2")).expect("add");
        // 2 nodes = 1 connection
        assert_eq!(topo.connection_count(), 1);

        topo.add_node(test_node("node-3", "10.0.0.3")).expect("add");
        // 3 nodes = 3 connections
        assert_eq!(topo.connection_count(), 3);

        topo.add_node(test_node("node-4", "10.0.0.4")).expect("add");
        // 4 nodes = 6 connections
        assert_eq!(topo.connection_count(), 6);
    }

    #[test]
    fn mesh_topology_connection_count_hub_spoke() {
        let mut topo = MeshTopology::hub_spoke("10.0.0.0/24");

        topo.add_node(test_node("hub", "10.0.0.1").as_hub())
            .expect("add");
        topo.add_node(test_node("spoke-1", "10.0.0.2"))
            .expect("add");
        topo.add_node(test_node("spoke-2", "10.0.0.3"))
            .expect("add");
        topo.add_node(test_node("spoke-3", "10.0.0.4"))
            .expect("add");

        // 1 hub, 3 spokes = 3 connections
        assert_eq!(topo.connection_count(), 3);
    }

    #[test]
    fn mesh_topology_remove_connection() {
        let mut topo = MeshTopology::custom("10.0.0.0/24");

        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-2", "10.0.0.2");

        let id1 = node1.id.clone();
        let id2 = node2.id.clone();

        topo.add_node(node1).expect("add");
        topo.add_node(node2).expect("add");
        topo.add_connection(&id1, &id2).expect("add connection");

        assert_eq!(topo.connection_count(), 1);

        topo.remove_connection(&id1, &id2);
        assert_eq!(topo.connection_count(), 0);
    }

    #[test]
    fn mesh_topology_find_hub() {
        let mut topo = MeshTopology::hub_spoke("10.0.0.0/24");

        assert!(topo.find_hub().is_none());

        topo.add_node(test_node("hub", "10.0.0.1").as_hub())
            .expect("add");

        assert!(topo.find_hub().is_some());
    }

    #[test]
    fn mesh_topology_hubs() {
        let mut topo = MeshTopology::hub_spoke("10.0.0.0/24");

        topo.add_node(test_node("hub1", "10.0.0.1").as_hub())
            .expect("add");
        topo.add_node(test_node("hub2", "10.0.0.2").as_hub())
            .expect("add");
        topo.add_node(test_node("spoke", "10.0.0.3"))
            .expect("add");

        assert_eq!(topo.hubs().len(), 2);
    }

    #[test]
    fn mesh_topology_generate_peer_configs() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");

        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-2", "10.0.0.2");

        let id1 = node1.id.clone();

        topo.add_node(node1).expect("add");
        topo.add_node(node2).expect("add");

        let configs = topo.generate_peer_configs(&id1);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].persistent_keepalive, Some(25));
    }

    #[test]
    fn mesh_topology_validate_empty() {
        let topo = MeshTopology::full_mesh("10.0.0.0/24");
        assert!(topo.validate().is_err());
    }

    #[test]
    fn mesh_topology_validate_hub_spoke_no_hub() {
        let mut topo = MeshTopology::hub_spoke("10.0.0.0/24");
        topo.add_node(test_node("spoke", "10.0.0.1")).expect("add");

        assert!(topo.validate().is_err());
    }

    #[test]
    fn mesh_topology_validate_duplicate_ip() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");

        let pk1 = test_public_key();
        let pk2 = test_public_key();
        let ip: IpAddr = "10.0.0.1".parse().expect("valid ip");

        let node1 = MeshNode::new("node-1", "Node 1", pk1, ip);
        let node2 = MeshNode::new("node-2", "Node 2", pk2, ip);

        topo.add_node(node1).expect("add");
        topo.add_node(node2).expect("add");

        assert!(topo.validate().is_err());
    }

    #[test]
    fn mesh_topology_validate_success() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        topo.add_node(test_node("node-1", "10.0.0.1")).expect("add");
        topo.add_node(test_node("node-2", "10.0.0.2")).expect("add");

        assert!(topo.validate().is_ok());
    }

    // ==================== MeshIpAllocator Tests ====================

    #[test]
    fn mesh_ip_allocator_new() {
        let alloc = MeshIpAllocator::new_v4("10.0.0.0/24");
        assert!(alloc.is_ok());
        assert_eq!(alloc.expect("valid").allocated_count(), 0);
    }

    #[test]
    fn mesh_ip_allocator_invalid_cidr() {
        let alloc = MeshIpAllocator::new_v4("invalid");
        assert!(alloc.is_err());
    }

    #[test]
    fn mesh_ip_allocator_allocate() {
        let mut alloc = MeshIpAllocator::new_v4("10.0.0.0/24").expect("valid");

        let ip1 = alloc.allocate().expect("allocate");
        let ip2 = alloc.allocate().expect("allocate");

        assert_ne!(ip1, ip2);
        assert_eq!(alloc.allocated_count(), 2);
    }

    #[test]
    fn mesh_ip_allocator_reserve() {
        let mut alloc = MeshIpAllocator::new_v4("10.0.0.0/24").expect("valid");
        let ip: IpAddr = "10.0.0.100".parse().expect("valid ip");

        assert!(alloc.reserve(ip).is_ok());
        assert_eq!(alloc.allocated_count(), 1);

        // Can't reserve twice
        assert!(alloc.reserve(ip).is_err());
    }

    #[test]
    fn mesh_ip_allocator_release() {
        let mut alloc = MeshIpAllocator::new_v4("10.0.0.0/24").expect("valid");

        let ip = alloc.allocate().expect("allocate");
        assert_eq!(alloc.allocated_count(), 1);

        alloc.release(&ip);
        assert_eq!(alloc.allocated_count(), 0);
    }

    #[test]
    fn mesh_topology_nodes() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");

        topo.add_node(test_node("node-1", "10.0.0.1")).expect("add");
        topo.add_node(test_node("node-2", "10.0.0.2")).expect("add");

        let nodes = topo.nodes();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn mesh_topology_get_node_mut() {
        let mut topo = MeshTopology::full_mesh("10.0.0.0/24");
        let node = test_node("node-1", "10.0.0.1");
        let node_id = node.id.clone();

        topo.add_node(node).expect("add");

        let node_mut = topo.get_node_mut(&node_id);
        assert!(node_mut.is_some());

        if let Some(n) = node_mut {
            n.name = "Updated Name".to_string();
        }

        assert_eq!(topo.get_node(&node_id).expect("node").name, "Updated Name");
    }

    #[test]
    fn mesh_topology_add_connection_invalid_nodes() {
        let mut topo = MeshTopology::custom("10.0.0.0/24");
        let id1 = MeshNodeId::new("node-1");
        let id2 = MeshNodeId::new("node-2");

        // Neither node exists
        assert!(topo.add_connection(&id1, &id2).is_err());

        // Only one node exists
        topo.add_node(test_node("node-1", "10.0.0.1")).expect("add");
        assert!(topo.add_connection(&id1, &id2).is_err());
    }

    #[test]
    fn mesh_topology_remove_node_clears_connections() {
        let mut topo = MeshTopology::custom("10.0.0.0/24");

        let node1 = test_node("node-1", "10.0.0.1");
        let node2 = test_node("node-2", "10.0.0.2");
        let node3 = test_node("node-3", "10.0.0.3");

        let id1 = node1.id.clone();
        let id2 = node2.id.clone();
        let id3 = node3.id.clone();

        topo.add_node(node1).expect("add");
        topo.add_node(node2).expect("add");
        topo.add_node(node3).expect("add");

        topo.add_connection(&id1, &id2).expect("connect");
        topo.add_connection(&id2, &id3).expect("connect");

        assert_eq!(topo.connection_count(), 2);

        // Remove node2 should clear connections involving it
        topo.remove_node(&id2).expect("remove");

        assert_eq!(topo.connection_count(), 0);
        assert!(topo.get_peers_for(&id1).is_empty());
    }
}
