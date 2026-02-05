//! WireGuard mesh network integration for the gateway.
//!
//! This module provides integration with the `claw-wireguard` crate to enable
//! automatic mesh networking between nodes. When enabled, nodes automatically
//! establish WireGuard tunnels with each other for secure peer-to-peer
//! communication.
//!
//! # Features
//!
//! - **Auto-Peering**: Nodes are automatically added to the mesh on registration
//! - **Peer Distribution**: Peer configs are distributed to nodes via protocol messages
//! - **Topology Support**: Full mesh, hub-spoke, or custom topologies
//! - **Health Tracking**: Mesh connection health is tracked per-node
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                      GatewayServer                            │
//! │  ┌─────────────┐   ┌─────────────┐   ┌─────────────────────┐ │
//! │  │ NodeRegistry │   │ MeshIntegr- │   │ WireGuardManager    │ │
//! │  │             │◄──│ ation       │──►│ (MeshTopology)      │ │
//! │  └─────────────┘   └─────────────┘   └─────────────────────┘ │
//! └──────────────────────────────────────────────────────────────┘
//!                              │
//!                    MeshPeerConfig messages
//!                              │
//!              ┌───────────────┼───────────────┐
//!              ▼               ▼               ▼
//!        ┌──────────┐   ┌──────────┐   ┌──────────┐
//!        │  Node 1  │   │  Node 2  │   │  Node 3  │
//!        │ (wg0)    │◄──│ (wg0)    │──►│ (wg0)    │
//!        └──────────┘   └──────────┘   └──────────┘
//! ```

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use claw_proto::{MeshPeerConfig, NodeId};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::error::{ServerError, ServerResult};

/// Configuration for mesh integration.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Network CIDR for mesh IPs (e.g., "10.100.0.0/16").
    pub network_cidr: String,
    /// Default WireGuard listen port.
    pub listen_port: u16,
    /// Default persistent keepalive interval.
    pub keepalive_secs: u16,
    /// Topology type.
    pub topology: MeshTopologyType,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            network_cidr: "10.100.0.0/16".to_string(),
            listen_port: 51820,
            keepalive_secs: 25,
            topology: MeshTopologyType::FullMesh,
        }
    }
}

impl MeshConfig {
    /// Creates a new mesh config with the given network CIDR.
    #[must_use]
    pub fn new(network_cidr: impl Into<String>) -> Self {
        Self {
            network_cidr: network_cidr.into(),
            ..Self::default()
        }
    }

    /// Sets the listen port.
    #[must_use]
    pub fn with_listen_port(mut self, port: u16) -> Self {
        self.listen_port = port;
        self
    }

    /// Sets the keepalive interval.
    #[must_use]
    pub fn with_keepalive(mut self, secs: u16) -> Self {
        self.keepalive_secs = secs;
        self
    }

    /// Sets the topology type.
    #[must_use]
    pub fn with_topology(mut self, topology: MeshTopologyType) -> Self {
        self.topology = topology;
        self
    }
}

/// Mesh topology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeshTopologyType {
    /// Every node connects to every other node.
    #[default]
    FullMesh,
    /// All nodes connect through central hub(s).
    HubSpoke,
    /// Custom topology defined manually.
    Custom,
}

impl std::fmt::Display for MeshTopologyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FullMesh => write!(f, "full_mesh"),
            Self::HubSpoke => write!(f, "hub_spoke"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// State of a mesh node.
#[derive(Debug, Clone)]
pub struct MeshNodeState {
    /// Node ID.
    pub node_id: NodeId,
    /// Node name.
    pub name: String,
    /// Assigned mesh IP address.
    pub mesh_ip: IpAddr,
    /// WireGuard public key.
    pub public_key: String,
    /// External endpoint (for direct connections).
    pub endpoint: Option<String>,
    /// Whether this node is a hub (for hub-spoke topology).
    pub is_hub: bool,
    /// When the node joined the mesh.
    pub joined_at: DateTime<Utc>,
    /// Number of confirmed peer connections.
    pub connected_peers: u32,
    /// Last time mesh status was confirmed.
    pub last_mesh_ready: Option<DateTime<Utc>>,
}

impl MeshNodeState {
    /// Creates a new mesh node state.
    fn new(
        node_id: NodeId,
        name: String,
        mesh_ip: IpAddr,
        public_key: String,
        endpoint: Option<String>,
    ) -> Self {
        Self {
            node_id,
            name,
            mesh_ip,
            public_key,
            endpoint,
            is_hub: false,
            joined_at: Utc::now(),
            connected_peers: 0,
            last_mesh_ready: None,
        }
    }

    /// Updates the mesh ready status.
    fn update_mesh_ready(&mut self, peer_count: u32) {
        self.connected_peers = peer_count;
        self.last_mesh_ready = Some(Utc::now());
    }
}

/// IP address allocator for mesh networks.
#[derive(Debug)]
struct IpAllocator {
    /// Base network address (as u32 for easy arithmetic).
    base: u32,
    /// Next host number to allocate.
    next_host: u32,
    /// Allocated IP addresses.
    allocated: std::collections::HashSet<IpAddr>,
}

impl IpAllocator {
    /// Creates a new IP allocator for the given network.
    fn new(network_cidr: &str) -> ServerResult<Self> {
        let network: ipnet::IpNet = network_cidr
            .parse()
            .map_err(|e: ipnet::AddrParseError| ServerError::Internal(format!("invalid CIDR: {e}")))?;

        let base = match network.network() {
            IpAddr::V4(v4) => u32::from(v4),
            IpAddr::V6(_) => {
                return Err(ServerError::Internal("IPv6 not supported".to_string()))
            }
        };

        Ok(Self {
            base,
            next_host: 1, // Start at .1
            allocated: std::collections::HashSet::new(),
        })
    }

    /// Allocates the next available IP address.
    fn allocate(&mut self) -> ServerResult<IpAddr> {
        // Try to find an available address (limit search to prevent infinite loop)
        for _ in 0..65536 {
            let candidate = IpAddr::V4(std::net::Ipv4Addr::from(
                self.base.saturating_add(self.next_host),
            ));
            self.next_host = self.next_host.saturating_add(1);

            if !self.allocated.contains(&candidate) {
                self.allocated.insert(candidate);
                return Ok(candidate);
            }
        }

        Err(ServerError::Internal(
            "no more IP addresses available".to_string(),
        ))
    }

    /// Releases an allocated IP address.
    fn release(&mut self, ip: &IpAddr) {
        self.allocated.remove(ip);
    }

    /// Returns the number of allocated addresses.
    fn allocated_count(&self) -> usize {
        self.allocated.len()
    }
}

/// Mesh network integration for the gateway.
///
/// Manages WireGuard mesh topology and peer configuration distribution.
#[derive(Debug)]
pub struct MeshIntegration {
    /// Configuration.
    config: MeshConfig,
    /// Mesh nodes indexed by node ID.
    nodes: RwLock<HashMap<NodeId, MeshNodeState>>,
    /// IP address allocator.
    ip_allocator: Mutex<IpAllocator>,
}

impl MeshIntegration {
    /// Creates a new mesh integration with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the network CIDR is invalid.
    pub fn new(config: MeshConfig) -> ServerResult<Self> {
        let ip_allocator = IpAllocator::new(&config.network_cidr)?;

        Ok(Self {
            config,
            nodes: RwLock::new(HashMap::new()),
            ip_allocator: Mutex::new(ip_allocator),
        })
    }

    /// Creates a new mesh integration with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails.
    pub fn with_defaults() -> ServerResult<Self> {
        Self::new(MeshConfig::default())
    }

    /// Gets the configuration.
    #[must_use]
    pub fn config(&self) -> &MeshConfig {
        &self.config
    }

    /// Checks if mesh networking is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        true
    }

    /// Gets the number of nodes in the mesh.
    pub async fn node_count(&self) -> usize {
        self.nodes.read().await.len()
    }

    /// Gets the number of connections in the mesh.
    pub async fn connection_count(&self) -> usize {
        let nodes = self.nodes.read().await;
        let n = nodes.len();
        match self.config.topology {
            MeshTopologyType::FullMesh => {
                if n < 2 { 0 } else { n * (n - 1) / 2 }
            }
            MeshTopologyType::HubSpoke => {
                let hubs = nodes.values().filter(|n| n.is_hub).count();
                let spokes = n - hubs;
                hubs * spokes
            }
            MeshTopologyType::Custom => 0, // Would need explicit tracking
        }
    }

    /// Registers a node with the mesh.
    ///
    /// Returns the assigned mesh IP and the peer configurations to send to the node.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The node is already registered
    /// - IP allocation fails
    pub async fn register_node(
        &self,
        node_id: NodeId,
        name: &str,
        public_key: &str,
        endpoint: Option<String>,
    ) -> ServerResult<(IpAddr, Vec<MeshPeerConfig>)> {
        // Check if already registered
        {
            let nodes = self.nodes.read().await;
            if nodes.contains_key(&node_id) {
                return Err(ServerError::Internal(format!(
                    "node {node_id} already in mesh"
                )));
            }
        }

        // Allocate IP
        let mesh_ip = self.ip_allocator.lock().await.allocate()?;

        // Create node state
        let node_state = MeshNodeState::new(
            node_id,
            name.to_string(),
            mesh_ip,
            public_key.to_string(),
            endpoint,
        );

        // Get peer configs for this node (all other nodes)
        let peer_configs = {
            let nodes = self.nodes.read().await;
            self.generate_peer_configs_for_topology(&nodes, node_id)
        };

        // Add to mesh
        self.nodes.write().await.insert(node_id, node_state);

        info!(
            node_id = %node_id,
            name = %name,
            mesh_ip = %mesh_ip,
            peer_count = peer_configs.len(),
            "Registered node in mesh"
        );

        Ok((mesh_ip, peer_configs))
    }

    /// Unregisters a node from the mesh.
    ///
    /// Returns the public key of the removed node (for notifying other nodes).
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not found.
    pub async fn unregister_node(&self, node_id: NodeId) -> ServerResult<String> {
        let node = {
            let mut nodes = self.nodes.write().await;
            nodes
                .remove(&node_id)
                .ok_or_else(|| ServerError::Internal(format!("node {node_id} not in mesh")))?
        };

        // Release the IP
        self.ip_allocator.lock().await.release(&node.mesh_ip);

        info!(
            node_id = %node_id,
            mesh_ip = %node.mesh_ip,
            "Unregistered node from mesh"
        );

        Ok(node.public_key)
    }

    /// Gets a node's mesh state.
    pub async fn get_node(&self, node_id: NodeId) -> Option<MeshNodeState> {
        self.nodes.read().await.get(&node_id).cloned()
    }

    /// Lists all mesh nodes.
    pub async fn list_nodes(&self) -> Vec<MeshNodeState> {
        self.nodes.read().await.values().cloned().collect()
    }

    /// Gets peer configs for a node to add (new peers since registration).
    pub async fn get_peers_for_node(&self, node_id: NodeId) -> Vec<MeshPeerConfig> {
        let nodes = self.nodes.read().await;
        self.generate_peer_configs_for_topology(&nodes, node_id)
    }

    /// Handles a mesh ready message from a node.
    pub async fn handle_mesh_ready(
        &self,
        node_id: NodeId,
        peer_count: u32,
        error: Option<&str>,
    ) {
        let mut nodes = self.nodes.write().await;
        if let Some(node) = nodes.get_mut(&node_id) {
            node.update_mesh_ready(peer_count);
            if let Some(err) = error {
                warn!(
                    node_id = %node_id,
                    peer_count = peer_count,
                    error = %err,
                    "Node mesh ready with errors"
                );
            } else {
                debug!(
                    node_id = %node_id,
                    peer_count = peer_count,
                    "Node mesh ready"
                );
            }
        }
    }

    /// Gets the public keys of all current mesh nodes (for peer removal notifications).
    pub async fn all_public_keys(&self) -> Vec<String> {
        self.nodes
            .read()
            .await
            .values()
            .map(|n| n.public_key.clone())
            .collect()
    }

    /// Gets all node IDs in the mesh.
    pub async fn all_node_ids(&self) -> Vec<NodeId> {
        self.nodes.read().await.keys().copied().collect()
    }

    /// Generates peer configs based on topology.
    fn generate_peer_configs_for_topology(
        &self,
        nodes: &HashMap<NodeId, MeshNodeState>,
        for_node: NodeId,
    ) -> Vec<MeshPeerConfig> {
        match self.config.topology {
            MeshTopologyType::FullMesh => {
                // Connect to all other nodes
                nodes
                    .values()
                    .filter(|n| n.node_id != for_node)
                    .map(|n| self.node_to_peer_config(n))
                    .collect()
            }
            MeshTopologyType::HubSpoke => {
                // Check if this node is a hub
                let is_hub = nodes.get(&for_node).is_some_and(|n| n.is_hub);

                if is_hub {
                    // Hub connects to all non-hub nodes
                    nodes
                        .values()
                        .filter(|n| n.node_id != for_node && !n.is_hub)
                        .map(|n| self.node_to_peer_config(n))
                        .collect()
                } else {
                    // Spoke connects only to hubs
                    nodes
                        .values()
                        .filter(|n| n.is_hub)
                        .map(|n| self.node_to_peer_config(n))
                        .collect()
                }
            }
            MeshTopologyType::Custom => {
                // For custom topology, would need explicit connection tracking
                // For now, default to no automatic connections
                Vec::new()
            }
        }
    }

    /// Converts a mesh node state to a peer config message.
    fn node_to_peer_config(&self, node: &MeshNodeState) -> MeshPeerConfig {
        let mut config = MeshPeerConfig::new(&node.public_key, node.mesh_ip.to_string())
            .with_keepalive(self.config.keepalive_secs)
            .with_allowed_ip(format!("{}/32", node.mesh_ip));

        if let Some(ref endpoint) = node.endpoint {
            config = config.with_endpoint(endpoint);
        }

        config
    }

    /// Gets mesh status summary.
    pub async fn status(&self) -> MeshStatus {
        let nodes = self.nodes.read().await;
        let allocated = self.ip_allocator.lock().await.allocated_count();

        MeshStatus {
            enabled: true,
            node_count: nodes.len() as u32,
            connection_count: match self.config.topology {
                MeshTopologyType::FullMesh => {
                    let n = nodes.len();
                    if n < 2 { 0 } else { (n * (n - 1) / 2) as u32 }
                }
                MeshTopologyType::HubSpoke => {
                    let hubs = nodes.values().filter(|n| n.is_hub).count();
                    let spokes = nodes.len() - hubs;
                    (hubs * spokes) as u32
                }
                MeshTopologyType::Custom => 0,
            },
            network_cidr: self.config.network_cidr.clone(),
            topology_type: self.config.topology.to_string(),
            allocated_ips: allocated as u32,
        }
    }
}

/// Mesh status information.
#[derive(Debug, Clone)]
pub struct MeshStatus {
    /// Whether mesh is enabled.
    pub enabled: bool,
    /// Number of nodes in mesh.
    pub node_count: u32,
    /// Number of connections.
    pub connection_count: u32,
    /// Network CIDR.
    pub network_cidr: String,
    /// Topology type.
    pub topology_type: String,
    /// Number of allocated IPs.
    pub allocated_ips: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== MeshConfig Tests ====================

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.network_cidr, "10.100.0.0/16");
        assert_eq!(config.listen_port, 51820);
        assert_eq!(config.keepalive_secs, 25);
        assert_eq!(config.topology, MeshTopologyType::FullMesh);
    }

    #[test]
    fn test_mesh_config_new() {
        let config = MeshConfig::new("192.168.0.0/24");
        assert_eq!(config.network_cidr, "192.168.0.0/24");
    }

    #[test]
    fn test_mesh_config_builders() {
        let config = MeshConfig::new("10.0.0.0/8")
            .with_listen_port(51821)
            .with_keepalive(30)
            .with_topology(MeshTopologyType::HubSpoke);

        assert_eq!(config.listen_port, 51821);
        assert_eq!(config.keepalive_secs, 30);
        assert_eq!(config.topology, MeshTopologyType::HubSpoke);
    }

    // ==================== MeshTopologyType Tests ====================

    #[test]
    fn test_mesh_topology_type_display() {
        assert_eq!(MeshTopologyType::FullMesh.to_string(), "full_mesh");
        assert_eq!(MeshTopologyType::HubSpoke.to_string(), "hub_spoke");
        assert_eq!(MeshTopologyType::Custom.to_string(), "custom");
    }

    #[test]
    fn test_mesh_topology_type_default() {
        assert_eq!(MeshTopologyType::default(), MeshTopologyType::FullMesh);
    }

    // ==================== IpAllocator Tests ====================

    #[test]
    fn test_ip_allocator_new() {
        let alloc = IpAllocator::new("10.100.0.0/16");
        assert!(alloc.is_ok());
        assert_eq!(alloc.unwrap().allocated_count(), 0);
    }

    #[test]
    fn test_ip_allocator_invalid_cidr() {
        let alloc = IpAllocator::new("invalid");
        assert!(alloc.is_err());
    }

    #[test]
    fn test_ip_allocator_allocate() {
        let mut alloc = IpAllocator::new("10.100.0.0/16").unwrap();

        let ip1 = alloc.allocate().unwrap();
        let ip2 = alloc.allocate().unwrap();

        assert_ne!(ip1, ip2);
        assert_eq!(alloc.allocated_count(), 2);
    }

    #[test]
    fn test_ip_allocator_release() {
        let mut alloc = IpAllocator::new("10.100.0.0/16").unwrap();

        let ip = alloc.allocate().unwrap();
        assert_eq!(alloc.allocated_count(), 1);

        alloc.release(&ip);
        assert_eq!(alloc.allocated_count(), 0);
    }

    // ==================== MeshNodeState Tests ====================

    #[test]
    fn test_mesh_node_state_new() {
        let node_id = NodeId::new();
        let ip: IpAddr = "10.100.0.5".parse().unwrap();

        let state = MeshNodeState::new(
            node_id,
            "test-node".to_string(),
            ip,
            "test-public-key".to_string(),
            Some("192.168.1.100:51820".to_string()),
        );

        assert_eq!(state.node_id, node_id);
        assert_eq!(state.name, "test-node");
        assert_eq!(state.mesh_ip, ip);
        assert_eq!(state.public_key, "test-public-key");
        assert!(!state.is_hub);
        assert_eq!(state.connected_peers, 0);
        assert!(state.last_mesh_ready.is_none());
    }

    #[test]
    fn test_mesh_node_state_update_mesh_ready() {
        let node_id = NodeId::new();
        let ip: IpAddr = "10.100.0.5".parse().unwrap();

        let mut state = MeshNodeState::new(
            node_id,
            "test-node".to_string(),
            ip,
            "test-key".to_string(),
            None,
        );

        state.update_mesh_ready(3);

        assert_eq!(state.connected_peers, 3);
        assert!(state.last_mesh_ready.is_some());
    }

    // ==================== MeshIntegration Tests ====================

    #[tokio::test]
    async fn test_mesh_integration_new() {
        let config = MeshConfig::default();
        let mesh = MeshIntegration::new(config);

        assert!(mesh.is_ok());
        let mesh = mesh.unwrap();
        assert!(mesh.is_enabled());
        assert_eq!(mesh.node_count().await, 0);
    }

    #[tokio::test]
    async fn test_mesh_integration_with_defaults() {
        let mesh = MeshIntegration::with_defaults();
        assert!(mesh.is_ok());
    }

    #[tokio::test]
    async fn test_mesh_integration_register_node() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node_id = NodeId::new();

        let result = mesh
            .register_node(node_id, "test-node", "test-public-key", None)
            .await;

        assert!(result.is_ok());
        let (ip, peers) = result.unwrap();

        // First node gets an IP but no peers
        assert!(ip.to_string().starts_with("10.100."));
        assert!(peers.is_empty());
        assert_eq!(mesh.node_count().await, 1);
    }

    #[tokio::test]
    async fn test_mesh_integration_register_multiple_nodes() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node1 = NodeId::new();
        let node2 = NodeId::new();

        // Register first node
        let (ip1, peers1) = mesh
            .register_node(node1, "node-1", "key1", None)
            .await
            .unwrap();

        assert!(peers1.is_empty());

        // Register second node - should get first node as peer
        let (ip2, peers2) = mesh
            .register_node(node2, "node-2", "key2", None)
            .await
            .unwrap();

        assert_ne!(ip1, ip2);
        assert_eq!(peers2.len(), 1);
        assert_eq!(peers2[0].public_key, "key1");
        assert_eq!(mesh.node_count().await, 2);
    }

    #[tokio::test]
    async fn test_mesh_integration_register_duplicate_fails() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node_id = NodeId::new();

        mesh.register_node(node_id, "test-node", "key", None)
            .await
            .unwrap();

        let result = mesh
            .register_node(node_id, "test-node", "key", None)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mesh_integration_unregister_node() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node_id = NodeId::new();

        mesh.register_node(node_id, "test-node", "test-key", None)
            .await
            .unwrap();

        let result = mesh.unregister_node(node_id).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-key");
        assert_eq!(mesh.node_count().await, 0);
    }

    #[tokio::test]
    async fn test_mesh_integration_unregister_unknown_fails() {
        let mesh = MeshIntegration::with_defaults().unwrap();

        let result = mesh.unregister_node(NodeId::new()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mesh_integration_get_node() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node_id = NodeId::new();

        mesh.register_node(node_id, "test-node", "key", None)
            .await
            .unwrap();

        let node = mesh.get_node(node_id).await;

        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "test-node");
    }

    #[tokio::test]
    async fn test_mesh_integration_list_nodes() {
        let mesh = MeshIntegration::with_defaults().unwrap();

        mesh.register_node(NodeId::new(), "node-1", "key1", None)
            .await
            .unwrap();
        mesh.register_node(NodeId::new(), "node-2", "key2", None)
            .await
            .unwrap();

        let nodes = mesh.list_nodes().await;

        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_mesh_integration_handle_mesh_ready() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node_id = NodeId::new();

        mesh.register_node(node_id, "test-node", "key", None)
            .await
            .unwrap();

        mesh.handle_mesh_ready(node_id, 5, None).await;

        let node = mesh.get_node(node_id).await.unwrap();
        assert_eq!(node.connected_peers, 5);
        assert!(node.last_mesh_ready.is_some());
    }

    #[tokio::test]
    async fn test_mesh_integration_connection_count_full_mesh() {
        let mesh = MeshIntegration::with_defaults().unwrap();

        // 0 nodes = 0 connections
        assert_eq!(mesh.connection_count().await, 0);

        mesh.register_node(NodeId::new(), "n1", "k1", None)
            .await
            .unwrap();
        // 1 node = 0 connections
        assert_eq!(mesh.connection_count().await, 0);

        mesh.register_node(NodeId::new(), "n2", "k2", None)
            .await
            .unwrap();
        // 2 nodes = 1 connection
        assert_eq!(mesh.connection_count().await, 1);

        mesh.register_node(NodeId::new(), "n3", "k3", None)
            .await
            .unwrap();
        // 3 nodes = 3 connections
        assert_eq!(mesh.connection_count().await, 3);
    }

    #[tokio::test]
    async fn test_mesh_integration_status() {
        let mesh = MeshIntegration::with_defaults().unwrap();

        mesh.register_node(NodeId::new(), "n1", "k1", None)
            .await
            .unwrap();
        mesh.register_node(NodeId::new(), "n2", "k2", None)
            .await
            .unwrap();

        let status = mesh.status().await;

        assert!(status.enabled);
        assert_eq!(status.node_count, 2);
        assert_eq!(status.connection_count, 1);
        assert_eq!(status.topology_type, "full_mesh");
        assert_eq!(status.allocated_ips, 2);
    }

    #[tokio::test]
    async fn test_mesh_integration_all_node_ids() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node1 = NodeId::new();
        let node2 = NodeId::new();

        mesh.register_node(node1, "n1", "k1", None).await.unwrap();
        mesh.register_node(node2, "n2", "k2", None).await.unwrap();

        let ids = mesh.all_node_ids().await;

        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&node1));
        assert!(ids.contains(&node2));
    }

    #[tokio::test]
    async fn test_mesh_integration_all_public_keys() {
        let mesh = MeshIntegration::with_defaults().unwrap();

        mesh.register_node(NodeId::new(), "n1", "key1", None)
            .await
            .unwrap();
        mesh.register_node(NodeId::new(), "n2", "key2", None)
            .await
            .unwrap();

        let keys = mesh.all_public_keys().await;

        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }

    #[tokio::test]
    async fn test_mesh_peer_config_with_endpoint() {
        let mesh = MeshIntegration::with_defaults().unwrap();
        let node1 = NodeId::new();
        let node2 = NodeId::new();

        mesh.register_node(node1, "n1", "k1", Some("192.168.1.100:51820".to_string()))
            .await
            .unwrap();

        let (_, peers) = mesh.register_node(node2, "n2", "k2", None).await.unwrap();

        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].endpoint, Some("192.168.1.100:51820".to_string()));
    }
}
