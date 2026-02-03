//! Peer discovery for the mesh network.
//!
//! This module provides mechanisms for nodes to discover each other
//! and announce their presence to the mesh.

use std::collections::HashMap;
use std::net::SocketAddr;

use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use tracing::{debug, info};

use crate::types::{MeshNode, NodeId, Region};

/// Errors that can occur during discovery.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DiscoveryError {
    /// Gateway is not configured.
    #[error("gateway endpoint not configured")]
    NoGateway,
    /// Node not found in registry.
    #[error("node {node_id} not found")]
    NodeNotFound {
        /// The node ID that was not found.
        node_id: NodeId,
    },
    /// Discovery is disabled.
    #[error("discovery is disabled")]
    Disabled,
    /// Connection to gateway failed.
    #[error("failed to connect to gateway: {message}")]
    ConnectionFailed {
        /// Error message.
        message: String,
    },
}

/// Configuration for mesh discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Gateway endpoint for centralized discovery.
    pub gateway_endpoint: Option<SocketAddr>,
    /// How long before a peer is considered stale.
    pub peer_timeout: Duration,
    /// Whether to enable MOLT P2P discovery.
    pub molt_enabled: bool,
    /// Announcement interval.
    pub announce_interval: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            gateway_endpoint: None,
            peer_timeout: Duration::minutes(5),
            molt_enabled: false,
            announce_interval: Duration::seconds(30),
        }
    }
}

/// Information about a discovered peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The mesh node information.
    pub node: MeshNode,
    /// When this peer was first discovered.
    pub discovered_at: DateTime<Utc>,
    /// When this peer was last seen.
    pub last_seen: DateTime<Utc>,
    /// How this peer was discovered.
    pub source: DiscoverySource,
}

impl PeerInfo {
    /// Creates new peer info from a node.
    #[must_use]
    pub fn new(node: MeshNode, source: DiscoverySource) -> Self {
        let now = Utc::now();
        Self {
            node,
            discovered_at: now,
            last_seen: now,
            source,
        }
    }

    /// Updates the last seen timestamp.
    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }

    /// Checks if this peer is stale based on the timeout.
    #[must_use]
    pub fn is_stale(&self, timeout: Duration) -> bool {
        Utc::now() - self.last_seen > timeout
    }
}

/// How a peer was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    /// Discovered through the gateway.
    Gateway,
    /// Discovered through MOLT P2P.
    MoltP2p,
    /// Manually configured.
    Static,
    /// Announced by the peer itself.
    Announcement,
}

/// Registry of discovered peers.
#[derive(Debug)]
struct PeerRegistry {
    /// Known peers indexed by node ID.
    peers: HashMap<NodeId, PeerInfo>,
}

impl PeerRegistry {
    fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    fn add_or_update(&mut self, node: MeshNode, source: DiscoverySource) {
        let node_id = node.node_id;
        if let Some(info) = self.peers.get_mut(&node_id) {
            info.touch();
            // Update node info if from more authoritative source
            if source == DiscoverySource::Gateway || source == DiscoverySource::Announcement {
                info.node = node;
            }
        } else {
            self.peers.insert(node_id, PeerInfo::new(node, source));
        }
    }

    fn remove(&mut self, node_id: NodeId) -> Option<PeerInfo> {
        self.peers.remove(&node_id)
    }

    fn get(&self, node_id: NodeId) -> Option<&PeerInfo> {
        self.peers.get(&node_id)
    }

    fn all(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }

    fn by_region(&self, region: Region) -> Vec<PeerInfo> {
        self.peers
            .values()
            .filter(|p| p.node.region == region)
            .cloned()
            .collect()
    }

    fn prune_stale(&mut self, timeout: Duration) -> Vec<PeerInfo> {
        let stale: Vec<NodeId> = self
            .peers
            .iter()
            .filter(|(_, info)| info.is_stale(timeout))
            .map(|(id, _)| *id)
            .collect();

        let mut removed = Vec::with_capacity(stale.len());
        for id in stale {
            if let Some(info) = self.peers.remove(&id) {
                removed.push(info);
            }
        }
        removed
    }

    fn count(&self) -> usize {
        self.peers.len()
    }
}

/// Mesh peer discovery service.
#[derive(Debug)]
pub struct MeshDiscovery {
    /// Discovery configuration.
    config: DiscoveryConfig,
    /// Discovered peers.
    registry: RwLock<PeerRegistry>,
    /// Our own node info (if announced).
    self_node: RwLock<Option<MeshNode>>,
    /// Whether discovery is enabled.
    enabled: bool,
}

impl MeshDiscovery {
    /// Creates a new mesh discovery service.
    #[must_use]
    pub fn new(config: DiscoveryConfig) -> Self {
        let enabled = config.gateway_endpoint.is_some() || config.molt_enabled;

        Self {
            config,
            registry: RwLock::new(PeerRegistry::new()),
            self_node: RwLock::new(None),
            enabled,
        }
    }

    /// Creates a discovery service with a gateway endpoint.
    #[must_use]
    pub fn with_gateway(gateway_endpoint: SocketAddr) -> Self {
        Self::new(DiscoveryConfig {
            gateway_endpoint: Some(gateway_endpoint),
            ..Default::default()
        })
    }

    /// Creates a disabled discovery service (for standalone nodes).
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            config: DiscoveryConfig::default(),
            registry: RwLock::new(PeerRegistry::new()),
            self_node: RwLock::new(None),
            enabled: false,
        }
    }

    /// Returns whether discovery is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the gateway endpoint if configured.
    #[must_use]
    pub fn gateway_endpoint(&self) -> Option<SocketAddr> {
        self.config.gateway_endpoint
    }

    /// Announces this node to the network.
    ///
    /// # Errors
    ///
    /// Returns an error if discovery is disabled or gateway unreachable.
    pub fn announce_self(&self, node: &MeshNode) -> Result<(), DiscoveryError> {
        if !self.enabled {
            return Err(DiscoveryError::Disabled);
        }

        info!(
            node_id = %node.node_id,
            mesh_ip = %node.mesh_ip,
            region = %node.region,
            "Announcing self to mesh"
        );

        // Store our own info
        {
            let mut self_node = self.self_node.write();
            *self_node = Some(node.clone());
        }

        // In a real implementation, this would send to the gateway
        // For now, we just store locally
        debug!(
            gateway = ?self.config.gateway_endpoint,
            "Would announce to gateway"
        );

        Ok(())
    }

    /// Discovers peers from all configured sources.
    ///
    /// # Errors
    ///
    /// Returns an error if discovery is disabled.
    pub fn discover_peers(&self) -> Result<Vec<MeshNode>, DiscoveryError> {
        if !self.enabled {
            return Err(DiscoveryError::Disabled);
        }

        // Prune stale peers first
        {
            let mut registry = self.registry.write();
            let pruned = registry.prune_stale(self.config.peer_timeout);
            if !pruned.is_empty() {
                debug!(count = pruned.len(), "Pruned stale peers");
            }
        }

        // Return current known peers
        let registry = self.registry.read();
        let peers: Vec<MeshNode> = registry.all().into_iter().map(|p| p.node).collect();

        debug!(peer_count = peers.len(), "Discovered peers");

        Ok(peers)
    }

    /// Registers a peer (e.g., from gateway response or announcement).
    pub fn register_peer(&self, node: MeshNode, source: DiscoverySource) {
        let node_id = node.node_id;

        // Don't register ourselves
        {
            let self_node = self.self_node.read();
            if let Some(ref sn) = *self_node {
                if sn.node_id == node_id {
                    return;
                }
            }
        }

        let mut registry = self.registry.write();
        registry.add_or_update(node, source);

        debug!(node_id = %node_id, ?source, "Registered peer");
    }

    /// Unregisters a peer.
    pub fn unregister_peer(&self, node_id: NodeId) -> Option<MeshNode> {
        let mut registry = self.registry.write();
        let info = registry.remove(node_id);

        if info.is_some() {
            info!(node_id = %node_id, "Unregistered peer");
        }

        info.map(|i| i.node)
    }

    /// Returns information about a specific peer.
    #[must_use]
    pub fn get_peer(&self, node_id: NodeId) -> Option<PeerInfo> {
        let registry = self.registry.read();
        registry.get(node_id).cloned()
    }

    /// Returns all known peers.
    #[must_use]
    pub fn all_peers(&self) -> Vec<PeerInfo> {
        let registry = self.registry.read();
        registry.all()
    }

    /// Returns peers in a specific region.
    #[must_use]
    pub fn peers_in_region(&self, region: Region) -> Vec<PeerInfo> {
        let registry = self.registry.read();
        registry.by_region(region)
    }

    /// Returns the number of known peers.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        let registry = self.registry.read();
        registry.count()
    }

    /// Updates a peer's last seen timestamp.
    pub fn touch_peer(&self, node_id: NodeId) {
        let mut registry = self.registry.write();
        if let Some(info) = registry.peers.get_mut(&node_id) {
            info.touch();
        }
    }

    /// Prunes stale peers and returns them.
    pub fn prune_stale(&self) -> Vec<PeerInfo> {
        let mut registry = self.registry.write();
        let pruned = registry.prune_stale(self.config.peer_timeout);

        if !pruned.is_empty() {
            info!(count = pruned.len(), "Pruned stale peers");
        }

        pruned
    }

    /// Returns our own node info if announced.
    #[must_use]
    pub fn self_node(&self) -> Option<MeshNode> {
        let self_node = self.self_node.read();
        self_node.clone()
    }
}

/// Integration point for MOLT P2P discovery.
pub trait MoltDiscoveryProvider: Send + Sync {
    /// Discovers MOLT marketplace nodes.
    fn discover_molt_nodes(&self) -> Result<Vec<MeshNode>, DiscoveryError>;

    /// Announces to MOLT network.
    fn announce_to_molt(&self, node: &MeshNode) -> Result<(), DiscoveryError>;
}

/// Stub implementation for MOLT discovery (to be implemented with molt-p2p).
#[derive(Debug, Default)]
pub struct StubMoltDiscovery;

impl MoltDiscoveryProvider for StubMoltDiscovery {
    fn discover_molt_nodes(&self) -> Result<Vec<MeshNode>, DiscoveryError> {
        // Stub - returns empty list
        Ok(Vec::new())
    }

    fn announce_to_molt(&self, _node: &MeshNode) -> Result<(), DiscoveryError> {
        // Stub - does nothing
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

    fn test_node(region: Region, mesh_ip: &str) -> MeshNode {
        MeshNode::builder()
            .mesh_ip(mesh_ip.parse().expect("valid IP"))
            .workload_subnet("10.200.1.0/24".parse().expect("valid subnet"))
            .wireguard_key(test_key())
            .region(region)
            .build()
            .expect("should build node")
    }

    // ==================== CREATION TESTS ====================

    #[test]
    fn test_discovery_creates_with_gateway() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);

        assert!(discovery.is_enabled());
        assert_eq!(discovery.gateway_endpoint(), Some(endpoint));
    }

    #[test]
    fn test_discovery_creates_disabled() {
        let discovery = MeshDiscovery::disabled();

        assert!(!discovery.is_enabled());
        assert!(discovery.gateway_endpoint().is_none());
    }

    #[test]
    fn test_discovery_with_molt_enabled() {
        let config = DiscoveryConfig {
            gateway_endpoint: None,
            molt_enabled: true,
            ..Default::default()
        };
        let discovery = MeshDiscovery::new(config);

        assert!(discovery.is_enabled());
    }

    // ==================== ANNOUNCE TESTS ====================

    #[test]
    fn test_announce_self_stores_node() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");

        let result = discovery.announce_self(&node);
        assert!(result.is_ok());

        let self_node = discovery.self_node();
        assert!(self_node.is_some());
        assert_eq!(self_node.expect("should exist").node_id, node.node_id);
    }

    #[test]
    fn test_announce_self_disabled_fails() {
        let discovery = MeshDiscovery::disabled();
        let node = test_node(Region::UsWest, "10.100.2.1");

        let result = discovery.announce_self(&node);
        assert!(matches!(result, Err(DiscoveryError::Disabled)));
    }

    // ==================== REGISTER TESTS ====================

    #[test]
    fn test_register_peer_adds_to_registry() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");
        let node_id = node.node_id;

        discovery.register_peer(node, DiscoverySource::Gateway);

        assert_eq!(discovery.peer_count(), 1);

        let peer = discovery.get_peer(node_id);
        assert!(peer.is_some());
        assert_eq!(peer.expect("should exist").source, DiscoverySource::Gateway);
    }

    #[test]
    fn test_register_self_is_ignored() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");

        discovery.announce_self(&node).expect("should announce");

        // Try to register ourselves as a peer
        discovery.register_peer(node.clone(), DiscoverySource::Gateway);

        // Should not be in peer list
        assert_eq!(discovery.peer_count(), 0);
    }

    #[test]
    fn test_register_updates_existing_peer() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");
        let node_id = node.node_id;

        discovery.register_peer(node.clone(), DiscoverySource::Static);

        let info1 = discovery.get_peer(node_id).expect("should exist");
        let first_seen = info1.last_seen;

        // Small delay
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Register again
        discovery.register_peer(node, DiscoverySource::Gateway);

        let info2 = discovery.get_peer(node_id).expect("should exist");
        assert!(info2.last_seen > first_seen);
        // Source should still be original since we updated
        assert_eq!(discovery.peer_count(), 1);
    }

    // ==================== UNREGISTER TESTS ====================

    #[test]
    fn test_unregister_peer_removes_from_registry() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");
        let node_id = node.node_id;

        discovery.register_peer(node, DiscoverySource::Gateway);
        assert_eq!(discovery.peer_count(), 1);

        let removed = discovery.unregister_peer(node_id);
        assert!(removed.is_some());
        assert_eq!(discovery.peer_count(), 0);
    }

    #[test]
    fn test_unregister_nonexistent_returns_none() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);

        let removed = discovery.unregister_peer(NodeId::new());
        assert!(removed.is_none());
    }

    // ==================== DISCOVER TESTS ====================

    #[test]
    fn test_discover_peers_returns_registered() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);

        let node1 = test_node(Region::UsWest, "10.100.2.1");
        let node2 = test_node(Region::UsEast, "10.100.18.1");

        discovery.register_peer(node1, DiscoverySource::Gateway);
        discovery.register_peer(node2, DiscoverySource::Gateway);

        let peers = discovery.discover_peers().expect("should succeed");
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_discover_peers_disabled_fails() {
        let discovery = MeshDiscovery::disabled();

        let result = discovery.discover_peers();
        assert!(matches!(result, Err(DiscoveryError::Disabled)));
    }

    // ==================== REGION TESTS ====================

    #[test]
    fn test_peers_in_region() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);

        let node1 = test_node(Region::UsWest, "10.100.2.1");
        let node2 = test_node(Region::UsWest, "10.100.2.2");
        let node3 = test_node(Region::UsEast, "10.100.18.1");

        discovery.register_peer(node1, DiscoverySource::Gateway);
        discovery.register_peer(node2, DiscoverySource::Gateway);
        discovery.register_peer(node3, DiscoverySource::Gateway);

        let us_west = discovery.peers_in_region(Region::UsWest);
        assert_eq!(us_west.len(), 2);

        let us_east = discovery.peers_in_region(Region::UsEast);
        assert_eq!(us_east.len(), 1);

        let molt = discovery.peers_in_region(Region::Molt);
        assert_eq!(molt.len(), 0);
    }

    // ==================== STALE PEER TESTS ====================

    #[test]
    fn test_peer_info_is_stale() {
        let node = test_node(Region::UsWest, "10.100.2.1");
        let mut info = PeerInfo::new(node, DiscoverySource::Gateway);

        // Not stale with 5 minute timeout
        assert!(!info.is_stale(Duration::minutes(5)));

        // Artificially age the peer
        info.last_seen = Utc::now() - Duration::minutes(10);

        // Now it's stale
        assert!(info.is_stale(Duration::minutes(5)));
    }

    #[test]
    fn test_touch_peer_updates_last_seen() {
        let endpoint: SocketAddr = "10.100.0.1:51820".parse().expect("valid addr");
        let discovery = MeshDiscovery::with_gateway(endpoint);
        let node = test_node(Region::UsWest, "10.100.2.1");
        let node_id = node.node_id;

        discovery.register_peer(node, DiscoverySource::Gateway);

        let info1 = discovery.get_peer(node_id).expect("should exist");
        let first_seen = info1.last_seen;

        std::thread::sleep(std::time::Duration::from_millis(10));

        discovery.touch_peer(node_id);

        let info2 = discovery.get_peer(node_id).expect("should exist");
        assert!(info2.last_seen > first_seen);
    }

    #[test]
    fn test_prune_stale_removes_old_peers() {
        let config = DiscoveryConfig {
            gateway_endpoint: Some("10.100.0.1:51820".parse().expect("valid addr")),
            peer_timeout: Duration::milliseconds(50),
            ..Default::default()
        };
        let discovery = MeshDiscovery::new(config);

        let node = test_node(Region::UsWest, "10.100.2.1");
        discovery.register_peer(node, DiscoverySource::Gateway);

        assert_eq!(discovery.peer_count(), 1);

        // Wait for peer to become stale
        std::thread::sleep(std::time::Duration::from_millis(100));

        let pruned = discovery.prune_stale();
        assert_eq!(pruned.len(), 1);
        assert_eq!(discovery.peer_count(), 0);
    }

    // ==================== MOLT PROVIDER TESTS ====================

    #[test]
    fn test_stub_molt_discovery() {
        let stub = StubMoltDiscovery;

        let nodes = stub.discover_molt_nodes();
        assert!(nodes.is_ok());
        assert!(nodes.expect("should succeed").is_empty());

        let node = test_node(Region::Molt, "10.100.128.1");
        let result = stub.announce_to_molt(&node);
        assert!(result.is_ok());
    }
}
