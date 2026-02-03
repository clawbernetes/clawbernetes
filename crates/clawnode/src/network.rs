//! Node networking with WireGuard mesh.
//!
//! This module handles:
//! - WireGuard configuration management
//! - Mesh network participation
//! - Peer discovery and management

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;

use claw_wireguard::{KeyPair, PrivateKey, PublicKey};
use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::NodeError;

/// Configuration for WireGuard networking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireGuardConfig {
    /// WireGuard interface name (e.g., "wg0").
    pub interface_name: String,
    /// Listen port for WireGuard (default: 51820).
    pub listen_port: u16,
    /// Path to private key file, or None to auto-generate.
    pub private_key_path: Option<String>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            interface_name: "claw0".to_string(),
            listen_port: 51820,
            private_key_path: None,
        }
    }
}

/// Configuration for network settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    /// CIDR for mesh network (e.g., "10.100.0.0/16").
    pub mesh_cidr: String,
    /// CIDR for workload network (e.g., "10.200.0.0/16").
    pub workload_cidr: String,
    /// WireGuard configuration.
    #[serde(default)]
    pub wireguard: WireGuardConfig,
    /// Whether networking is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mesh_cidr: "10.100.0.0/16".to_string(),
            workload_cidr: "10.200.0.0/16".to_string(),
            wireguard: WireGuardConfig::default(),
            enabled: true,
        }
    }
}

/// Mesh network configuration received from gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// This node's assigned IP in the mesh.
    pub your_ip: IpAddr,
    /// This node's subnet for workloads.
    pub your_subnet: Ipv4Net,
    /// Existing peers in the mesh.
    pub peers: Vec<MeshNode>,
}

/// A node in the mesh network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshNode {
    /// Node identifier.
    pub node_id: String,
    /// Node's public key (base64).
    pub public_key: String,
    /// Node's mesh IP.
    pub mesh_ip: IpAddr,
    /// Node's workload subnet.
    pub workload_subnet: Ipv4Net,
    /// Node's endpoint (IP:port for WireGuard).
    pub endpoint: Option<String>,
}

/// Interface state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceState {
    /// Interface is down.
    Down,
    /// Interface is up.
    Up,
}

/// A stored peer.
#[derive(Debug, Clone)]
struct StoredPeer {
    node_id: String,
    public_key: String,
    mesh_ip: IpAddr,
    workload_subnet: Ipv4Net,
    endpoint: Option<String>,
}

/// Node networking state.
pub struct NodeNetwork {
    /// This node's mesh IP.
    mesh_ip: IpAddr,
    /// This node's workload subnet.
    workload_subnet: Ipv4Net,
    /// Network configuration.
    config: NetworkConfig,
    /// Our keypair.
    keypair: KeyPair,
    /// Interface state.
    state: InterfaceState,
    /// Stored peers (keyed by public key base64).
    peers: HashMap<String, StoredPeer>,
}

impl NodeNetwork {
    /// Create a new node network (not yet joined to mesh).
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn new(config: &NetworkConfig) -> Result<Self, NodeError> {
        // Parse mesh CIDR to validate
        let mesh_cidr: Ipv4Net = config
            .mesh_cidr
            .parse()
            .map_err(|e| NodeError::Config(format!("invalid mesh_cidr: {e}")))?;

        // Generate or load private key
        let keypair = if let Some(ref path) = config.wireguard.private_key_path {
            Self::load_keypair(path)?
        } else {
            KeyPair::generate()
        };

        // Use a temporary IP until we join the mesh
        let temp_subnet: Ipv4Net = format!("{}/32", mesh_cidr.addr())
            .parse()
            .map_err(|e| NodeError::Config(format!("invalid temp IP: {e}")))?;

        info!(
            interface = %config.wireguard.interface_name,
            public_key = %keypair.public_key().to_base64(),
            "created node network"
        );

        Ok(Self {
            mesh_ip: mesh_cidr.addr().into(),
            workload_subnet: temp_subnet,
            config: config.clone(),
            keypair,
            state: InterfaceState::Down,
            peers: HashMap::new(),
        })
    }

    /// Load keypair from file.
    fn load_keypair(path: &str) -> Result<KeyPair, NodeError> {
        let path = Path::new(path);
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| NodeError::Config(format!("failed to read key file: {e}")))?;
            let private_key = PrivateKey::from_base64(content.trim())
                .map_err(|e| NodeError::Config(format!("invalid private key: {e}")))?;
            Ok(KeyPair::from_private_key(private_key))
        } else {
            // Generate new key and save it
            let keypair = KeyPair::generate();
            std::fs::write(path, keypair.private_key().to_base64())
                .map_err(|e| NodeError::Config(format!("failed to write key file: {e}")))?;
            Ok(keypair)
        }
    }

    /// Join the mesh network with configuration from gateway.
    ///
    /// # Errors
    ///
    /// Returns error if joining fails.
    pub fn join_mesh(&mut self, mesh_config: &MeshConfig) -> Result<(), NodeError> {
        info!(
            mesh_ip = %mesh_config.your_ip,
            subnet = %mesh_config.your_subnet,
            peer_count = mesh_config.peers.len(),
            "joining mesh network"
        );

        // Update our mesh IP and subnet
        self.mesh_ip = mesh_config.your_ip;
        self.workload_subnet = mesh_config.your_subnet;

        // Add all existing peers
        for peer in &mesh_config.peers {
            if let Err(e) = self.add_peer(peer) {
                warn!(
                    node_id = %peer.node_id,
                    error = %e,
                    "failed to add peer during mesh join"
                );
            }
        }

        // Mark interface as up
        self.state = InterfaceState::Up;

        info!("joined mesh network successfully");
        Ok(())
    }

    /// Add a peer to the mesh.
    ///
    /// # Errors
    ///
    /// Returns error if peer configuration is invalid.
    pub fn add_peer(&mut self, peer: &MeshNode) -> Result<(), NodeError> {
        debug!(
            node_id = %peer.node_id,
            mesh_ip = %peer.mesh_ip,
            "adding mesh peer"
        );

        // Validate public key
        let _ = PublicKey::from_base64(&peer.public_key)
            .map_err(|e| NodeError::Config(format!("invalid peer public key: {e}")))?;

        // Validate endpoint if provided
        if let Some(ref endpoint) = peer.endpoint {
            let _: std::net::SocketAddr = endpoint
                .parse()
                .map_err(|e| NodeError::Config(format!("invalid peer endpoint: {e}")))?;
        }

        // Check for duplicate
        if self.peers.contains_key(&peer.public_key) {
            return Err(NodeError::Config(format!(
                "peer already exists: {}",
                peer.node_id
            )));
        }

        // Store peer
        self.peers.insert(
            peer.public_key.clone(),
            StoredPeer {
                node_id: peer.node_id.clone(),
                public_key: peer.public_key.clone(),
                mesh_ip: peer.mesh_ip,
                workload_subnet: peer.workload_subnet,
                endpoint: peer.endpoint.clone(),
            },
        );

        info!(
            node_id = %peer.node_id,
            mesh_ip = %peer.mesh_ip,
            "added mesh peer"
        );

        Ok(())
    }

    /// Remove a peer from the mesh.
    ///
    /// # Errors
    ///
    /// Returns error if peer not found.
    pub fn remove_peer(&mut self, public_key: &str) -> Result<(), NodeError> {
        debug!(public_key = %public_key, "removing mesh peer");

        // Validate public key
        let _ = PublicKey::from_base64(public_key)
            .map_err(|e| NodeError::Config(format!("invalid public key: {e}")))?;

        if self.peers.remove(public_key).is_none() {
            return Err(NodeError::Config(format!(
                "peer not found: {}",
                public_key
            )));
        }

        info!(public_key = %public_key, "removed mesh peer");
        Ok(())
    }

    /// Get this node's mesh IP.
    #[must_use]
    pub fn mesh_ip(&self) -> IpAddr {
        self.mesh_ip
    }

    /// Get this node's workload subnet.
    #[must_use]
    pub fn workload_subnet(&self) -> &Ipv4Net {
        &self.workload_subnet
    }

    /// Get this node's public key.
    #[must_use]
    pub fn public_key(&self) -> &PublicKey {
        self.keypair.public_key()
    }

    /// Get the WireGuard interface name.
    #[must_use]
    pub fn interface_name(&self) -> &str {
        &self.config.wireguard.interface_name
    }

    /// Get the interface state.
    #[must_use]
    pub fn interface_state(&self) -> InterfaceState {
        self.state
    }

    /// Get the number of peers.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Shutdown the network.
    ///
    /// # Errors
    ///
    /// Returns error if shutdown fails.
    pub fn shutdown(&mut self) -> Result<(), NodeError> {
        info!(interface = %self.config.wireguard.interface_name, "shutting down network");
        self.state = InterfaceState::Down;
        Ok(())
    }
}

impl std::fmt::Debug for NodeNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeNetwork")
            .field("interface", &self.config.wireguard.interface_name)
            .field("mesh_ip", &self.mesh_ip)
            .field("workload_subnet", &self.workload_subnet)
            .field("state", &self.state)
            .field("peer_count", &self.peers.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_network_config() -> NetworkConfig {
        NetworkConfig {
            mesh_cidr: "10.100.0.0/16".to_string(),
            workload_cidr: "10.200.0.0/16".to_string(),
            wireguard: WireGuardConfig {
                interface_name: "wg-test".to_string(),
                listen_port: 51820,
                private_key_path: None,
            },
            enabled: true,
        }
    }

    fn test_mesh_config() -> MeshConfig {
        MeshConfig {
            your_ip: "10.100.0.1".parse().unwrap(),
            your_subnet: "10.200.0.0/24".parse().unwrap(),
            peers: vec![],
        }
    }

    fn test_mesh_node() -> MeshNode {
        let keypair = KeyPair::generate();
        MeshNode {
            node_id: "test-node-2".to_string(),
            public_key: keypair.public_key().to_base64(),
            mesh_ip: "10.100.0.2".parse().unwrap(),
            workload_subnet: "10.200.1.0/24".parse().unwrap(),
            endpoint: Some("192.168.1.100:51820".to_string()),
        }
    }

    #[test]
    fn test_new_node_network() {
        let config = test_network_config();
        let network = NodeNetwork::new(&config);

        assert!(network.is_ok());
        let network = network.unwrap();
        assert_eq!(network.interface_name(), "wg-test");
        assert_eq!(network.interface_state(), InterfaceState::Down);
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_new_generates_keypair() {
        let config = test_network_config();
        let network1 = NodeNetwork::new(&config).unwrap();
        let network2 = NodeNetwork::new(&config).unwrap();

        // Each instance should have different keys
        assert_ne!(
            network1.public_key().to_base64(),
            network2.public_key().to_base64()
        );
    }

    #[test]
    fn test_invalid_mesh_cidr_rejected() {
        let mut config = test_network_config();
        config.mesh_cidr = "invalid".to_string();

        let result = NodeNetwork::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_join_mesh() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let mesh_config = test_mesh_config();
        let result = network.join_mesh(&mesh_config);

        assert!(result.is_ok());
        assert_eq!(network.mesh_ip(), mesh_config.your_ip);
        assert_eq!(network.workload_subnet(), &mesh_config.your_subnet);
        assert_eq!(network.interface_state(), InterfaceState::Up);
    }

    #[test]
    fn test_add_peer() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let peer = test_mesh_node();
        let result = network.add_peer(&peer);

        assert!(result.is_ok());
        assert_eq!(network.peer_count(), 1);
    }

    #[test]
    fn test_add_peer_invalid_public_key() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let mut peer = test_mesh_node();
        peer.public_key = "invalid-key".to_string();

        let result = network.add_peer(&peer);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_peer_invalid_endpoint() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let mut peer = test_mesh_node();
        peer.endpoint = Some("not-a-valid-endpoint".to_string());

        let result = network.add_peer(&peer);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_peer() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let peer = test_mesh_node();
        network.add_peer(&peer).unwrap();
        assert_eq!(network.peer_count(), 1);

        let result = network.remove_peer(&peer.public_key);
        assert!(result.is_ok());
        assert_eq!(network.peer_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_peer() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let keypair = KeyPair::generate();
        let result = network.remove_peer(&keypair.public_key().to_base64());

        assert!(result.is_err());
    }

    #[test]
    fn test_join_mesh_with_peers() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let mut mesh_config = test_mesh_config();
        mesh_config.peers = vec![test_mesh_node()];

        let result = network.join_mesh(&mesh_config);

        assert!(result.is_ok());
        assert_eq!(network.peer_count(), 1);
    }

    #[test]
    fn test_shutdown() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        // Join mesh first to bring interface up
        let mesh_config = test_mesh_config();
        network.join_mesh(&mesh_config).unwrap();
        assert_eq!(network.interface_state(), InterfaceState::Up);

        // Shutdown
        let result = network.shutdown();
        assert!(result.is_ok());
        assert_eq!(network.interface_state(), InterfaceState::Down);
    }

    #[test]
    fn test_wireguard_config_default() {
        let config = WireGuardConfig::default();
        assert_eq!(config.interface_name, "claw0");
        assert_eq!(config.listen_port, 51820);
        assert!(config.private_key_path.is_none());
    }

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.mesh_cidr, "10.100.0.0/16");
        assert_eq!(config.workload_cidr, "10.200.0.0/16");
        assert!(config.enabled);
    }

    #[test]
    fn test_mesh_config_serialization() {
        let config = test_mesh_config();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: MeshConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.your_ip, parsed.your_ip);
        assert_eq!(config.your_subnet, parsed.your_subnet);
    }

    #[test]
    fn test_mesh_node_serialization() {
        let node = test_mesh_node();
        let json = serde_json::to_string(&node).unwrap();
        let parsed: MeshNode = serde_json::from_str(&json).unwrap();

        assert_eq!(node.node_id, parsed.node_id);
        assert_eq!(node.public_key, parsed.public_key);
        assert_eq!(node.mesh_ip, parsed.mesh_ip);
    }

    #[test]
    fn test_add_duplicate_peer_rejected() {
        let config = test_network_config();
        let mut network = NodeNetwork::new(&config).unwrap();

        let peer = test_mesh_node();
        network.add_peer(&peer).unwrap();

        // Adding same peer again should fail
        let result = network.add_peer(&peer);
        assert!(result.is_err());
    }
}
