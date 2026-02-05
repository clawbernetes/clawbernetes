//! `WireGuard` tunnel manager for lifecycle and mesh operations.
//!
//! This module provides the `WireGuardManager` type for managing `WireGuard`
//! tunnels, including peer management, mesh topology integration, and
//! automatic peering based on node registry changes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::{InterfaceConfig, PeerConfig};
use crate::error::{Result, WireGuardError};
use crate::interface::WireGuardInterface;
use crate::keys::{KeyPair, PrivateKey, PublicKey};
use crate::mesh::{MeshIpAllocator, MeshNode, MeshNodeId, MeshTopology};
use crate::tunnel::{ConnectionState, TunnelPeerStatus, TunnelStatus};
use crate::types::AllowedIp;

/// Configuration for the `WireGuard` manager.
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Default listen port for new interfaces.
    pub default_listen_port: u16,
    /// Default persistent keepalive interval.
    pub default_keepalive: Option<u16>,
    /// Network CIDR for mesh IP allocation.
    pub mesh_network: String,
    /// MTU for `WireGuard` interfaces.
    pub mtu: Option<u16>,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            default_listen_port: 51820,
            default_keepalive: Some(25),
            mesh_network: "10.100.0.0/16".to_string(),
            mtu: Some(1420),
        }
    }
}

impl ManagerConfig {
    /// Creates a new manager config with the given network.
    #[must_use]
    pub fn new(mesh_network: impl Into<String>) -> Self {
        Self {
            mesh_network: mesh_network.into(),
            ..Self::default()
        }
    }

    /// Sets the default listen port.
    #[must_use]
    pub fn with_listen_port(mut self, port: u16) -> Self {
        self.default_listen_port = port;
        self
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

    /// Sets the MTU.
    #[must_use]
    pub fn with_mtu(mut self, mtu: u16) -> Self {
        self.mtu = Some(mtu);
        self
    }
}

/// Internal state for a managed tunnel.
#[derive(Debug)]
struct ManagedTunnel {
    /// The tunnel's key pair.
    keypair: KeyPair,
    /// The tunnel's interface configuration.
    config: InterfaceConfig,
    /// Current tunnel status.
    status: TunnelStatus,
    /// Mesh node ID (if part of a mesh).
    /// Reserved for future mesh-tunnel association.
    #[allow(dead_code)]
    mesh_node_id: Option<MeshNodeId>,
}

/// `WireGuard` tunnel manager.
///
/// Manages the lifecycle of `WireGuard` tunnels, including creation, peer
/// management, and mesh topology integration.
#[derive(Clone)]
pub struct WireGuardManager<I: WireGuardInterface + Clone> {
    /// The underlying `WireGuard` interface implementation.
    interface: I,
    /// Manager configuration.
    config: ManagerConfig,
    /// Managed tunnels by interface name.
    tunnels: Arc<RwLock<HashMap<String, ManagedTunnel>>>,
    /// IP address allocator for mesh networks.
    ip_allocator: Arc<RwLock<MeshIpAllocator>>,
    /// Active mesh topology (if any).
    mesh: Arc<RwLock<Option<MeshTopology>>>,
}

impl<I: WireGuardInterface + Clone> WireGuardManager<I> {
    /// Creates a new `WireGuard` manager.
    ///
    /// # Errors
    ///
    /// Returns an error if the mesh network CIDR is invalid.
    pub fn new(interface: I, config: ManagerConfig) -> Result<Self> {
        let ip_allocator = MeshIpAllocator::new_v4(&config.mesh_network)?;

        Ok(Self {
            interface,
            config,
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            ip_allocator: Arc::new(RwLock::new(ip_allocator)),
            mesh: Arc::new(RwLock::new(None)),
        })
    }

    /// Creates a new tunnel with the given name and private key.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A tunnel with the same name already exists
    /// - The interface creation fails
    pub async fn create_tunnel(
        &mut self,
        name: &str,
        private_key: PrivateKey,
        listen_port: Option<u16>,
    ) -> Result<PublicKey> {
        let tunnels = self.tunnels.read().await;
        if tunnels.contains_key(name) {
            return Err(WireGuardError::InterfaceExists(name.to_string()));
        }
        drop(tunnels);

        let keypair = KeyPair::from_private_key(private_key.clone());
        let public_key = *keypair.public_key();

        let port = listen_port.unwrap_or(self.config.default_listen_port);
        let mut config = InterfaceConfig::new(private_key).with_listen_port(port);

        if let Some(mtu) = self.config.mtu {
            config = config.with_mtu(mtu);
        }

        self.interface.create(name, &config).await?;

        let status = TunnelStatus::new(name, public_key).with_listen_port(port);

        let managed = ManagedTunnel {
            keypair,
            config,
            status,
            mesh_node_id: None,
        };

        self.tunnels.write().await.insert(name.to_string(), managed);

        info!(interface = name, "Created WireGuard tunnel");
        Ok(public_key)
    }

    /// Creates a new tunnel with a generated key pair.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel creation fails.
    pub async fn create_tunnel_with_generated_key(
        &mut self,
        name: &str,
        listen_port: Option<u16>,
    ) -> Result<PublicKey> {
        let keypair = KeyPair::generate();
        let private_key = keypair.into_private_key();
        self.create_tunnel(name, private_key, listen_port).await
    }

    /// Destroys a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist or destruction fails.
    pub async fn destroy_tunnel(&mut self, name: &str) -> Result<()> {
        let mut tunnels = self.tunnels.write().await;
        if tunnels.remove(name).is_none() {
            return Err(WireGuardError::InterfaceNotFound(name.to_string()));
        }
        drop(tunnels);

        self.interface.destroy(name).await?;

        info!(interface = name, "Destroyed WireGuard tunnel");
        Ok(())
    }

    /// Adds a peer to a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tunnel doesn't exist
    /// - The peer already exists
    /// - Adding the peer fails
    pub async fn add_peer(&mut self, interface: &str, peer: &PeerConfig) -> Result<()> {
        {
            let tunnels = self.tunnels.read().await;
            if !tunnels.contains_key(interface) {
                return Err(WireGuardError::InterfaceNotFound(interface.to_string()));
            }
        }

        self.interface.add_peer(interface, peer).await?;

        // Update managed tunnel state
        let mut tunnels = self.tunnels.write().await;
        if let Some(tunnel) = tunnels.get_mut(interface) {
            tunnel.config.peers.push(peer.clone());

            let mut peer_status = TunnelPeerStatus::new(peer.public_key);
            if let Some(endpoint) = &peer.endpoint {
                peer_status = peer_status.with_endpoint(endpoint.clone());
            }
            tunnel.status.peers.push(peer_status);
        }

        debug!(
            interface,
            peer = %peer.public_key.to_base64()[..8],
            "Added peer to tunnel"
        );

        Ok(())
    }

    /// Removes a peer from a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tunnel doesn't exist
    /// - The peer doesn't exist
    /// - Removing the peer fails
    pub async fn remove_peer(&mut self, interface: &str, public_key: &PublicKey) -> Result<()> {
        {
            let tunnels = self.tunnels.read().await;
            if !tunnels.contains_key(interface) {
                return Err(WireGuardError::InterfaceNotFound(interface.to_string()));
            }
        }

        self.interface.remove_peer(interface, public_key).await?;

        // Update managed tunnel state
        let mut tunnels = self.tunnels.write().await;
        if let Some(tunnel) = tunnels.get_mut(interface) {
            tunnel.config.peers.retain(|p| &p.public_key != public_key);
            tunnel.status.peers.retain(|p| &p.public_key != public_key);
        }

        debug!(
            interface,
            peer = %public_key.to_base64()[..8],
            "Removed peer from tunnel"
        );

        Ok(())
    }

    /// Gets the status of a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist.
    pub async fn get_tunnel_status(&self, name: &str) -> Result<TunnelStatus> {
        let tunnels = self.tunnels.read().await;
        let tunnel = tunnels
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;
        Ok(tunnel.status.clone())
    }

    /// Lists all managed tunnels.
    pub async fn list_tunnels(&self) -> Vec<String> {
        self.tunnels.read().await.keys().cloned().collect()
    }

    /// Returns the number of managed tunnels.
    pub async fn tunnel_count(&self) -> usize {
        self.tunnels.read().await.len()
    }

    /// Allocates a mesh IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if no addresses are available.
    pub async fn allocate_mesh_ip(&self) -> Result<IpAddr> {
        self.ip_allocator.write().await.allocate()
    }

    /// Reserves a specific mesh IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if the address is already allocated.
    pub async fn reserve_mesh_ip(&self, ip: IpAddr) -> Result<()> {
        self.ip_allocator.write().await.reserve(ip)
    }

    /// Releases a mesh IP address.
    pub async fn release_mesh_ip(&self, ip: &IpAddr) {
        self.ip_allocator.write().await.release(ip);
    }

    /// Sets the mesh topology.
    pub async fn set_mesh_topology(&self, topology: MeshTopology) {
        *self.mesh.write().await = Some(topology);
    }

    /// Gets the current mesh topology.
    pub async fn get_mesh_topology(&self) -> Option<MeshTopology> {
        self.mesh.read().await.clone()
    }

    /// Registers a node with the mesh and creates its tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No mesh topology is configured
    /// - The node already exists
    /// - Tunnel creation fails
    pub async fn register_mesh_node(
        &mut self,
        node_id: impl Into<MeshNodeId>,
        name: impl Into<String>,
        public_key: PublicKey,
    ) -> Result<IpAddr> {
        let node_id = node_id.into();
        let name = name.into();

        let mesh_ip = self.allocate_mesh_ip().await?;

        let node = MeshNode::new(node_id.clone(), name.clone(), public_key, mesh_ip);

        {
            let mut mesh_guard = self.mesh.write().await;
            let mesh = mesh_guard
                .as_mut()
                .ok_or_else(|| WireGuardError::InvalidConfig("no mesh topology configured".to_string()))?;

            mesh.add_node(node)?;
        }

        info!(
            node_id = %node_id,
            name = %name,
            mesh_ip = %mesh_ip,
            "Registered mesh node"
        );

        Ok(mesh_ip)
    }

    /// Unregisters a node from the mesh.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No mesh topology is configured
    /// - The node doesn't exist
    pub async fn unregister_mesh_node(&mut self, node_id: &MeshNodeId) -> Result<()> {
        let mesh_ip;
        {
            let mut mesh_guard = self.mesh.write().await;
            let mesh = mesh_guard
                .as_mut()
                .ok_or_else(|| WireGuardError::InvalidConfig("no mesh topology configured".to_string()))?;

            let node = mesh.remove_node(node_id)?;
            mesh_ip = node.mesh_ip;
        }

        self.release_mesh_ip(&mesh_ip).await;

        info!(node_id = %node_id, "Unregistered mesh node");
        Ok(())
    }

    /// Gets the peer configurations for a node based on the mesh topology.
    ///
    /// # Errors
    ///
    /// Returns an error if no mesh topology is configured.
    pub async fn get_mesh_peers(&self, node_id: &MeshNodeId) -> Result<Vec<PeerConfig>> {
        let mesh_guard = self.mesh.read().await;
        let mesh = mesh_guard
            .as_ref()
            .ok_or_else(|| WireGuardError::InvalidConfig("no mesh topology configured".to_string()))?;

        Ok(mesh.generate_peer_configs(node_id))
    }

    /// Synchronizes a tunnel's peers with the mesh topology.
    ///
    /// This adds missing peers and removes peers that are no longer in the mesh.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tunnel doesn't exist
    /// - No mesh topology is configured
    /// - Peer operations fail
    pub async fn sync_mesh_peers(&mut self, interface: &str, node_id: &MeshNodeId) -> Result<SyncResult> {
        // Get expected peers from mesh
        let expected_peers = self.get_mesh_peers(node_id).await?;
        let expected_keys: std::collections::HashSet<_> = expected_peers
            .iter()
            .map(|p| p.public_key.to_base64())
            .collect();

        // Get current peers
        let current_status = self.get_tunnel_status(interface).await?;
        let current_keys: std::collections::HashSet<_> = current_status
            .peers
            .iter()
            .map(|p| p.public_key.to_base64())
            .collect();

        let mut result = SyncResult::default();

        // Add missing peers
        for peer in &expected_peers {
            let key = peer.public_key.to_base64();
            if !current_keys.contains(&key) {
                match self.add_peer(interface, peer).await {
                    Ok(()) => result.added += 1,
                    Err(e) => {
                        warn!(error = %e, peer = %key[..8], "Failed to add peer");
                        result.errors += 1;
                    }
                }
            }
        }

        // Remove stale peers
        for peer in &current_status.peers {
            let key = peer.public_key.to_base64();
            if !expected_keys.contains(&key) {
                match self.remove_peer(interface, &peer.public_key).await {
                    Ok(()) => result.removed += 1,
                    Err(e) => {
                        warn!(error = %e, peer = %key[..8], "Failed to remove peer");
                        result.errors += 1;
                    }
                }
            }
        }

        debug!(
            interface,
            added = result.added,
            removed = result.removed,
            errors = result.errors,
            "Synchronized mesh peers"
        );

        Ok(result)
    }

    /// Gets the public key for a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist.
    pub async fn get_tunnel_public_key(&self, name: &str) -> Result<PublicKey> {
        let tunnels = self.tunnels.read().await;
        let tunnel = tunnels
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;
        Ok(*tunnel.keypair.public_key())
    }

    /// Records a successful handshake with a peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel or peer doesn't exist.
    pub async fn record_peer_handshake(
        &self,
        interface: &str,
        public_key: &PublicKey,
    ) -> Result<()> {
        let mut tunnels = self.tunnels.write().await;
        let tunnel = tunnels
            .get_mut(interface)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(interface.to_string()))?;

        let peer = tunnel
            .status
            .find_peer_mut(public_key)
            .ok_or_else(|| WireGuardError::PeerNotFound(public_key.to_base64()))?;

        peer.record_handshake();
        Ok(())
    }

    /// Updates the health status of all peers in a tunnel.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist.
    pub async fn update_tunnel_health(&self, name: &str) -> Result<()> {
        let mut tunnels = self.tunnels.write().await;
        let tunnel = tunnels
            .get_mut(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        for peer in &mut tunnel.status.peers {
            peer.update_health();
        }
        tunnel.status.update_state();

        Ok(())
    }

    /// Gets the manager configuration.
    #[must_use]
    pub fn config(&self) -> &ManagerConfig {
        &self.config
    }

    /// Checks if a tunnel exists.
    pub async fn tunnel_exists(&self, name: &str) -> bool {
        self.tunnels.read().await.contains_key(name)
    }

    /// Updates tunnel status from the underlying interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the tunnel doesn't exist or status retrieval fails.
    pub async fn refresh_tunnel_status(&mut self, name: &str) -> Result<()> {
        let interface_status = self.interface.get_status(name).await?;

        let mut tunnels = self.tunnels.write().await;
        let tunnel = tunnels
            .get_mut(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        // Update peer traffic stats from interface
        for iface_peer in &interface_status.peers {
            if let Some(status_peer) = tunnel.status.find_peer_mut(&iface_peer.public_key) {
                // Set absolute values (interface reports totals, not deltas)
                status_peer.rx_bytes = iface_peer.rx_bytes;
                status_peer.tx_bytes = iface_peer.tx_bytes;

                // Update handshake if present
                if iface_peer.last_handshake.is_some()
                    && status_peer.state != ConnectionState::Connected {
                        status_peer.record_handshake();
                    }
            }
        }

        tunnel.status.update_state();
        Ok(())
    }
}

/// Result of a mesh peer synchronization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncResult {
    /// Number of peers added.
    pub added: usize,
    /// Number of peers removed.
    pub removed: usize,
    /// Number of errors encountered.
    pub errors: usize,
}

impl SyncResult {
    /// Returns whether the sync was successful (no errors).
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.errors == 0
    }

    /// Returns whether any changes were made.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.added > 0 || self.removed > 0
    }
}

/// Builder for creating mesh peer configurations.
#[derive(Debug)]
pub struct MeshPeerBuilder {
    /// Public key of the peer.
    public_key: PublicKey,
    /// Mesh IP of the peer.
    mesh_ip: Option<IpAddr>,
    /// Optional endpoint.
    endpoint: Option<crate::types::Endpoint>,
    /// Persistent keepalive interval.
    keepalive: Option<u16>,
    /// Additional allowed IPs.
    additional_allowed_ips: Vec<AllowedIp>,
}

impl MeshPeerBuilder {
    /// Creates a new builder with the given public key.
    #[must_use]
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            public_key,
            mesh_ip: None,
            endpoint: None,
            keepalive: None,
            additional_allowed_ips: Vec::new(),
        }
    }

    /// Sets the mesh IP.
    #[must_use]
    pub fn mesh_ip(mut self, ip: IpAddr) -> Self {
        self.mesh_ip = Some(ip);
        self
    }

    /// Sets the endpoint.
    #[must_use]
    pub fn endpoint(mut self, endpoint: crate::types::Endpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Sets the keepalive interval.
    #[must_use]
    pub fn keepalive(mut self, seconds: u16) -> Self {
        self.keepalive = Some(seconds);
        self
    }

    /// Adds an allowed IP.
    #[must_use]
    pub fn allowed_ip(mut self, ip: AllowedIp) -> Self {
        self.additional_allowed_ips.push(ip);
        self
    }

    /// Builds the peer configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<PeerConfig> {
        let mesh_ip = self
            .mesh_ip
            .ok_or_else(|| WireGuardError::InvalidConfig("mesh IP is required".to_string()))?;

        let mut config = PeerConfig::new(self.public_key);
        config.endpoint = self.endpoint;
        config.persistent_keepalive = self.keepalive;

        // Add mesh IP as allowed
        config
            .allowed_ips
            .push(AllowedIp::from_cidr(&format!("{mesh_ip}/32"))?);

        // Add additional allowed IPs
        config.allowed_ips.extend(self.additional_allowed_ips);

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::FakeWireGuardInterface;
    use crate::keys::generate_keypair;
    use crate::mesh::TopologyType;

    async fn test_manager() -> WireGuardManager<FakeWireGuardInterface> {
        let iface = FakeWireGuardInterface::new();
        let config = ManagerConfig::default();
        WireGuardManager::new(iface, config).expect("valid manager")
    }

    // ==================== ManagerConfig Tests ====================

    #[test]
    fn manager_config_default_and_builders() {
        // Test default values
        let config = ManagerConfig::default();
        assert_eq!(config.default_listen_port, 51820);
        assert_eq!(config.default_keepalive, Some(25));
        assert_eq!(config.mesh_network, "10.100.0.0/16");
        assert_eq!(config.mtu, Some(1420));

        // Test new() constructor
        let config = ManagerConfig::new("192.168.0.0/24");
        assert_eq!(config.mesh_network, "192.168.0.0/24");

        // Test builder methods
        let config = ManagerConfig::default()
            .with_listen_port(51821)
            .with_keepalive(30)
            .with_mtu(1400)
            .without_keepalive();
        assert_eq!(config.default_listen_port, 51821);
        assert_eq!(config.default_keepalive, None);
        assert_eq!(config.mtu, Some(1400));
    }

    // ==================== WireGuardManager Tests ====================

    #[tokio::test]
    async fn manager_new() {
        let manager = test_manager().await;
        assert_eq!(manager.tunnel_count().await, 0);
    }

    #[tokio::test]
    async fn manager_invalid_network() {
        let iface = FakeWireGuardInterface::new();
        let config = ManagerConfig::new("invalid");
        let result = WireGuardManager::new(iface, config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn manager_create_tunnel() {
        let mut manager = test_manager().await;
        let (private_key, _) = generate_keypair();

        let result = manager.create_tunnel("wg0", private_key, None).await;

        assert!(result.is_ok());
        assert_eq!(manager.tunnel_count().await, 1);
        assert!(manager.tunnel_exists("wg0").await);
    }

    #[tokio::test]
    async fn manager_create_tunnel_with_generated_key() {
        let mut manager = test_manager().await;

        let result = manager.create_tunnel_with_generated_key("wg0", None).await;

        assert!(result.is_ok());
        assert!(manager.tunnel_exists("wg0").await);
    }

    #[tokio::test]
    async fn manager_create_tunnel_custom_port() {
        let mut manager = test_manager().await;

        let public_key = manager
            .create_tunnel_with_generated_key("wg0", Some(51821))
            .await
            .expect("create tunnel");

        let status = manager.get_tunnel_status("wg0").await.expect("status");
        assert_eq!(status.listen_port, Some(51821));
        assert_eq!(status.local_public_key, public_key);
    }

    #[tokio::test]
    async fn manager_create_duplicate_tunnel() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("first create");

        let result = manager.create_tunnel_with_generated_key("wg0", None).await;
        assert!(matches!(result, Err(WireGuardError::InterfaceExists(_))));
    }

    #[tokio::test]
    async fn manager_destroy_tunnel() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let result = manager.destroy_tunnel("wg0").await;
        assert!(result.is_ok());
        assert!(!manager.tunnel_exists("wg0").await);
    }

    #[tokio::test]
    async fn manager_destroy_nonexistent_tunnel() {
        let mut manager = test_manager().await;
        let result = manager.destroy_tunnel("wg0").await;
        assert!(matches!(result, Err(WireGuardError::InterfaceNotFound(_))));
    }

    #[tokio::test]
    async fn manager_add_peer() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let (_, peer_public) = generate_keypair();
        let mut peer = PeerConfig::new(peer_public);
        peer.allowed_ips
            .push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));

        let result = manager.add_peer("wg0", &peer).await;
        assert!(result.is_ok());

        let status = manager.get_tunnel_status("wg0").await.expect("status");
        assert_eq!(status.peers.len(), 1);
    }

    #[tokio::test]
    async fn manager_add_peer_nonexistent_tunnel() {
        let mut manager = test_manager().await;

        let (_, peer_public) = generate_keypair();
        let peer = PeerConfig::new(peer_public);

        let result = manager.add_peer("wg0", &peer).await;
        assert!(matches!(result, Err(WireGuardError::InterfaceNotFound(_))));
    }

    #[tokio::test]
    async fn manager_remove_peer() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let (_, peer_public) = generate_keypair();
        let mut peer = PeerConfig::new(peer_public);
        peer.allowed_ips
            .push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));

        manager.add_peer("wg0", &peer).await.expect("add peer");

        let result = manager.remove_peer("wg0", &peer_public).await;
        assert!(result.is_ok());

        let status = manager.get_tunnel_status("wg0").await.expect("status");
        assert!(status.peers.is_empty());
    }

    #[tokio::test]
    async fn manager_list_tunnels() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create wg0");
        manager
            .create_tunnel_with_generated_key("wg1", None)
            .await
            .expect("create wg1");

        let tunnels = manager.list_tunnels().await;
        assert_eq!(tunnels.len(), 2);
        assert!(tunnels.contains(&"wg0".to_string()));
        assert!(tunnels.contains(&"wg1".to_string()));
    }

    #[tokio::test]
    async fn manager_get_tunnel_public_key() {
        let mut manager = test_manager().await;

        let public_key = manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let retrieved = manager.get_tunnel_public_key("wg0").await.expect("get key");
        assert_eq!(public_key, retrieved);
    }

    #[tokio::test]
    async fn manager_mesh_ip_allocation() {
        let manager = test_manager().await;

        // Allocate IPs
        let ip1 = manager.allocate_mesh_ip().await.expect("allocate");
        let ip2 = manager.allocate_mesh_ip().await.expect("allocate");
        assert_ne!(ip1, ip2);

        // Reserve IP
        let ip: IpAddr = "10.100.0.100".parse().expect("valid ip");
        assert!(manager.reserve_mesh_ip(ip).await.is_ok());
        assert!(manager.reserve_mesh_ip(ip).await.is_err());

        // Release and re-reserve
        manager.release_mesh_ip(&ip).await;
        assert!(manager.reserve_mesh_ip(ip).await.is_ok());
    }

    // ==================== Mesh Integration Tests ====================

    #[tokio::test]
    async fn manager_set_mesh_topology() {
        let manager = test_manager().await;

        assert!(manager.get_mesh_topology().await.is_none());

        let topo = MeshTopology::full_mesh("10.100.0.0/16");
        manager.set_mesh_topology(topo).await;

        let retrieved = manager.get_mesh_topology().await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.expect("topo").topology_type, TopologyType::FullMesh);
    }

    #[tokio::test]
    async fn manager_register_mesh_node() {
        let mut manager = test_manager().await;

        let topo = MeshTopology::full_mesh("10.100.0.0/16");
        manager.set_mesh_topology(topo).await;

        let (_, public_key) = generate_keypair();
        let mesh_ip = manager
            .register_mesh_node("node-1", "Node 1", public_key)
            .await
            .expect("register");

        assert!(mesh_ip.to_string().starts_with("10.100."));

        let topo = manager.get_mesh_topology().await.expect("topo");
        assert!(topo.get_node(&MeshNodeId::new("node-1")).is_some());
    }

    #[tokio::test]
    async fn manager_register_mesh_node_no_topology() {
        let mut manager = test_manager().await;

        let (_, public_key) = generate_keypair();
        let result = manager
            .register_mesh_node("node-1", "Node 1", public_key)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn manager_unregister_mesh_node() {
        let mut manager = test_manager().await;

        let topo = MeshTopology::full_mesh("10.100.0.0/16");
        manager.set_mesh_topology(topo).await;

        let (_, public_key) = generate_keypair();
        manager
            .register_mesh_node("node-1", "Node 1", public_key)
            .await
            .expect("register");

        let result = manager.unregister_mesh_node(&MeshNodeId::new("node-1")).await;
        assert!(result.is_ok());

        let topo = manager.get_mesh_topology().await.expect("topo");
        assert!(topo.get_node(&MeshNodeId::new("node-1")).is_none());
    }

    #[tokio::test]
    async fn manager_get_mesh_peers() {
        let mut manager = test_manager().await;

        let topo = MeshTopology::full_mesh("10.100.0.0/16");
        manager.set_mesh_topology(topo).await;

        let (_, pk1) = generate_keypair();
        let (_, pk2) = generate_keypair();

        manager
            .register_mesh_node("node-1", "Node 1", pk1)
            .await
            .expect("register");
        manager
            .register_mesh_node("node-2", "Node 2", pk2)
            .await
            .expect("register");

        let peers = manager
            .get_mesh_peers(&MeshNodeId::new("node-1"))
            .await
            .expect("peers");

        assert_eq!(peers.len(), 1);
    }

    #[tokio::test]
    async fn manager_sync_mesh_peers() {
        let mut manager = test_manager().await;

        // Create tunnel and mesh
        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let topo = MeshTopology::full_mesh("10.100.0.0/16");
        manager.set_mesh_topology(topo).await;

        // Get tunnel public key and register as a mesh node
        let tunnel_pk = manager.get_tunnel_public_key("wg0").await.expect("pk");
        manager
            .register_mesh_node("node-1", "Node 1", tunnel_pk)
            .await
            .expect("register");

        // Register another node
        let (_, pk2) = generate_keypair();
        manager
            .register_mesh_node("node-2", "Node 2", pk2)
            .await
            .expect("register");

        // Sync should add the peer
        let result = manager
            .sync_mesh_peers("wg0", &MeshNodeId::new("node-1"))
            .await
            .expect("sync");

        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 0);
        assert!(result.is_success());
        assert!(result.has_changes());
    }

    #[tokio::test]
    async fn manager_record_peer_handshake() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let (_, peer_public) = generate_keypair();
        let mut peer = PeerConfig::new(peer_public);
        peer.allowed_ips
            .push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));

        manager.add_peer("wg0", &peer).await.expect("add peer");

        let result = manager.record_peer_handshake("wg0", &peer_public).await;
        assert!(result.is_ok());

        let status = manager.get_tunnel_status("wg0").await.expect("status");
        assert!(status.peers[0].is_connected());
    }

    #[tokio::test]
    async fn manager_update_tunnel_health() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let (_, peer_public) = generate_keypair();
        let mut peer = PeerConfig::new(peer_public);
        peer.allowed_ips
            .push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));

        manager.add_peer("wg0", &peer).await.expect("add peer");

        let result = manager.update_tunnel_health("wg0").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn manager_config_accessor() {
        let manager = test_manager().await;
        let config = manager.config();
        assert_eq!(config.default_listen_port, 51820);
    }

    // ==================== SyncResult Tests ====================

    #[test]
    fn sync_result_states() {
        let result = SyncResult::default();
        assert!(result.is_success());
        assert!(!result.has_changes());

        let result = SyncResult { added: 2, removed: 1, errors: 0 };
        assert!(result.is_success());
        assert!(result.has_changes());

        let result = SyncResult { added: 1, removed: 0, errors: 1 };
        assert!(!result.is_success());
    }

    // ==================== MeshPeerBuilder Tests ====================

    #[test]
    fn mesh_peer_builder_new() {
        let (_, pk) = generate_keypair();
        let builder = MeshPeerBuilder::new(pk);
        assert!(builder.mesh_ip.is_none());
    }

    #[test]
    fn mesh_peer_builder_complete() {
        let (_, pk) = generate_keypair();
        let ip: IpAddr = "10.0.0.2".parse().expect("valid ip");
        let endpoint: crate::types::Endpoint = "192.168.1.1:51820".parse().expect("valid endpoint");

        let config = MeshPeerBuilder::new(pk)
            .mesh_ip(ip)
            .endpoint(endpoint.clone())
            .keepalive(25)
            .build()
            .expect("build");

        assert_eq!(config.public_key, pk);
        assert_eq!(config.endpoint, Some(endpoint));
        assert_eq!(config.persistent_keepalive, Some(25));
        assert_eq!(config.allowed_ips.len(), 1);
    }

    #[test]
    fn mesh_peer_builder_additional_allowed_ips() {
        let (_, pk) = generate_keypair();
        let ip: IpAddr = "10.0.0.2".parse().expect("valid ip");

        let config = MeshPeerBuilder::new(pk)
            .mesh_ip(ip)
            .allowed_ip(AllowedIp::from_cidr("192.168.0.0/24").expect("valid"))
            .build()
            .expect("build");

        assert_eq!(config.allowed_ips.len(), 2);
    }

    #[test]
    fn mesh_peer_builder_missing_mesh_ip() {
        let (_, pk) = generate_keypair();
        let result = MeshPeerBuilder::new(pk).build();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn manager_refresh_tunnel_status() {
        let mut manager = test_manager().await;

        manager
            .create_tunnel_with_generated_key("wg0", None)
            .await
            .expect("create");

        let result = manager.refresh_tunnel_status("wg0").await;
        assert!(result.is_ok());
    }
}
