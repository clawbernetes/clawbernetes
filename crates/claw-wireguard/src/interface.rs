//! WireGuard interface management.
//!
//! This module provides traits and implementations for managing WireGuard
//! network interfaces.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::{InterfaceConfig, PeerConfig};
use crate::error::{Result, WireGuardError};
use crate::keys::PublicKey;
use crate::types::{InterfaceStatus, PeerStatus};

/// Trait for managing WireGuard interfaces.
#[allow(async_fn_in_trait)]
pub trait WireGuardInterface {
    /// Creates a new WireGuard interface.
    async fn create(&mut self, name: &str, config: &InterfaceConfig) -> Result<()>;

    /// Destroys a WireGuard interface.
    async fn destroy(&mut self, name: &str) -> Result<()>;

    /// Adds a peer to an existing interface.
    async fn add_peer(&mut self, name: &str, peer: &PeerConfig) -> Result<()>;

    /// Removes a peer from an interface.
    async fn remove_peer(&mut self, name: &str, public_key: &PublicKey) -> Result<()>;

    /// Gets the status of an interface.
    async fn get_status(&self, name: &str) -> Result<InterfaceStatus>;

    /// Lists all managed interfaces.
    async fn list_interfaces(&self) -> Result<Vec<String>>;

    /// Checks if an interface exists.
    async fn interface_exists(&self, name: &str) -> bool;
}

/// Internal state for a fake interface.
#[derive(Clone, Debug)]
struct FakeInterfaceData {
    config: InterfaceConfig,
    peers: HashMap<String, FakePeerData>,
}

/// Internal state for a fake peer.
#[derive(Clone, Debug)]
struct FakePeerData {
    config: PeerConfig,
    rx_bytes: u64,
    tx_bytes: u64,
    last_handshake: Option<u64>,
}

/// State of an interface (for external use).
#[derive(Clone, Debug)]
pub struct InterfaceState {
    /// Interface name.
    pub name: String,
    /// Whether the interface is up.
    pub is_up: bool,
    /// Number of peers.
    pub peer_count: usize,
}

/// A fake WireGuard interface for testing.
#[derive(Clone)]
pub struct FakeWireGuardInterface {
    interfaces: Arc<RwLock<HashMap<String, FakeInterfaceData>>>,
}

impl FakeWireGuardInterface {
    /// Creates a new fake WireGuard interface manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            interfaces: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Simulates traffic on a peer.
    pub async fn simulate_traffic(
        &self,
        interface_name: &str,
        public_key: &PublicKey,
        rx_bytes: u64,
        tx_bytes: u64,
    ) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;
        let interface = interfaces
            .get_mut(interface_name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(interface_name.to_string()))?;

        let key_b64 = public_key.to_base64();
        let peer_data = interface
            .peers
            .get_mut(&key_b64)
            .ok_or_else(|| WireGuardError::PeerNotFound(key_b64))?;

        peer_data.rx_bytes = peer_data.rx_bytes.saturating_add(rx_bytes);
        peer_data.tx_bytes = peer_data.tx_bytes.saturating_add(tx_bytes);

        Ok(())
    }

    /// Simulates a handshake with a peer.
    pub async fn simulate_handshake(
        &self,
        interface_name: &str,
        public_key: &PublicKey,
        timestamp: u64,
    ) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;
        let interface = interfaces
            .get_mut(interface_name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(interface_name.to_string()))?;

        let key_b64 = public_key.to_base64();
        let peer_data = interface
            .peers
            .get_mut(&key_b64)
            .ok_or_else(|| WireGuardError::PeerNotFound(key_b64))?;

        peer_data.last_handshake = Some(timestamp);

        Ok(())
    }

    /// Gets the number of interfaces.
    pub async fn interface_count(&self) -> usize {
        self.interfaces.read().await.len()
    }

    /// Gets the state of an interface.
    pub async fn get_interface_state(&self, name: &str) -> Result<InterfaceState> {
        let interfaces = self.interfaces.read().await;
        let interface = interfaces
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        Ok(InterfaceState {
            name: name.to_string(),
            is_up: true,
            peer_count: interface.peers.len(),
        })
    }
}

impl Default for FakeWireGuardInterface {
    fn default() -> Self {
        Self::new()
    }
}

impl WireGuardInterface for FakeWireGuardInterface {
    async fn create(&mut self, name: &str, config: &InterfaceConfig) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;

        if interfaces.contains_key(name) {
            return Err(WireGuardError::InterfaceExists(name.to_string()));
        }

        let mut peers = HashMap::new();
        for peer in &config.peers {
            let key_b64 = peer.public_key.to_base64();
            peers.insert(
                key_b64,
                FakePeerData {
                    config: peer.clone(),
                    rx_bytes: 0,
                    tx_bytes: 0,
                    last_handshake: None,
                },
            );
        }

        interfaces.insert(
            name.to_string(),
            FakeInterfaceData {
                config: config.clone(),
                peers,
            },
        );

        Ok(())
    }

    async fn destroy(&mut self, name: &str) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;

        if interfaces.remove(name).is_none() {
            return Err(WireGuardError::InterfaceNotFound(name.to_string()));
        }

        Ok(())
    }

    async fn add_peer(&mut self, name: &str, peer: &PeerConfig) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;

        let interface = interfaces
            .get_mut(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let key_b64 = peer.public_key.to_base64();

        if interface.peers.contains_key(&key_b64) {
            return Err(WireGuardError::PeerExists(key_b64));
        }

        interface.peers.insert(
            key_b64,
            FakePeerData {
                config: peer.clone(),
                rx_bytes: 0,
                tx_bytes: 0,
                last_handshake: None,
            },
        );

        Ok(())
    }

    async fn remove_peer(&mut self, name: &str, public_key: &PublicKey) -> Result<()> {
        let mut interfaces = self.interfaces.write().await;

        let interface = interfaces
            .get_mut(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let key_b64 = public_key.to_base64();

        if interface.peers.remove(&key_b64).is_none() {
            return Err(WireGuardError::PeerNotFound(key_b64));
        }

        Ok(())
    }

    async fn get_status(&self, name: &str) -> Result<InterfaceStatus> {
        let interfaces = self.interfaces.read().await;

        let interface = interfaces
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let public_key = interface.config.private_key.public_key();

        let peers: Vec<PeerStatus> = interface
            .peers
            .values()
            .map(|data| PeerStatus {
                public_key: data.config.public_key,
                endpoint: data.config.endpoint.clone(),
                allowed_ips: data.config.allowed_ips.clone(),
                last_handshake: data.last_handshake,
                rx_bytes: data.rx_bytes,
                tx_bytes: data.tx_bytes,
            })
            .collect();

        Ok(InterfaceStatus {
            name: name.to_string(),
            public_key,
            listen_port: interface.config.listen_port,
            peers,
        })
    }

    async fn list_interfaces(&self) -> Result<Vec<String>> {
        let interfaces = self.interfaces.read().await;
        Ok(interfaces.keys().cloned().collect())
    }

    async fn interface_exists(&self, name: &str) -> bool {
        let interfaces = self.interfaces.read().await;
        interfaces.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{generate_keypair, PrivateKey, KEY_SIZE};
    use crate::types::AllowedIp;

    fn test_private_key() -> PrivateKey {
        PrivateKey::from_bytes(&[1u8; KEY_SIZE]).expect("valid key")
    }

    fn test_config() -> InterfaceConfig {
        InterfaceConfig::new(test_private_key())
            .with_listen_port(51820)
            .with_address(AllowedIp::from_cidr("10.0.0.1/24").expect("valid cidr"))
    }

    fn test_peer() -> PeerConfig {
        let (_, public_key) = generate_keypair();
        let mut peer = PeerConfig::new(public_key);
        peer.allowed_ips.push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));
        peer
    }

    #[tokio::test]
    async fn create_interface() {
        let mut iface = FakeWireGuardInterface::new();
        let result = iface.create("wg0", &test_config()).await;
        assert!(result.is_ok());
        assert!(iface.interface_exists("wg0").await);
    }

    #[tokio::test]
    async fn create_interface_duplicate_fails() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("first create");

        let result = iface.create("wg0", &test_config()).await;
        assert!(matches!(result, Err(WireGuardError::InterfaceExists(_))));
    }

    #[tokio::test]
    async fn destroy_interface() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let result = iface.destroy("wg0").await;
        assert!(result.is_ok());
        assert!(!iface.interface_exists("wg0").await);
    }

    #[tokio::test]
    async fn destroy_nonexistent_fails() {
        let mut iface = FakeWireGuardInterface::new();
        let result = iface.destroy("wg0").await;
        assert!(matches!(result, Err(WireGuardError::InterfaceNotFound(_))));
    }

    #[tokio::test]
    async fn add_peer() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let result = iface.add_peer("wg0", &test_peer()).await;
        assert!(result.is_ok());

        let status = iface.get_status("wg0").await.expect("status");
        assert_eq!(status.peers.len(), 1);
    }

    #[tokio::test]
    async fn add_peer_duplicate_fails() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let peer = test_peer();
        iface.add_peer("wg0", &peer).await.expect("first add");

        let result = iface.add_peer("wg0", &peer).await;
        assert!(matches!(result, Err(WireGuardError::PeerExists(_))));
    }

    #[tokio::test]
    async fn remove_peer() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let peer = test_peer();
        iface.add_peer("wg0", &peer).await.expect("add");

        let result = iface.remove_peer("wg0", &peer.public_key).await;
        assert!(result.is_ok());

        let status = iface.get_status("wg0").await.expect("status");
        assert_eq!(status.peers.len(), 0);
    }

    #[tokio::test]
    async fn get_status() {
        let mut iface = FakeWireGuardInterface::new();
        let config = test_config();
        iface.create("wg0", &config).await.expect("create");

        let status = iface.get_status("wg0").await.expect("status");

        assert_eq!(status.name, "wg0");
        assert_eq!(status.listen_port, Some(51820));
    }

    #[tokio::test]
    async fn list_interfaces() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create wg0");
        iface.create("wg1", &test_config()).await.expect("create wg1");

        let list = iface.list_interfaces().await.expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.contains(&"wg0".to_string()));
        assert!(list.contains(&"wg1".to_string()));
    }

    #[tokio::test]
    async fn simulate_traffic() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let peer = test_peer();
        iface.add_peer("wg0", &peer).await.expect("add");

        iface
            .simulate_traffic("wg0", &peer.public_key, 1000, 500)
            .await
            .expect("traffic");

        let status = iface.get_status("wg0").await.expect("status");
        assert_eq!(status.peers[0].rx_bytes, 1000);
        assert_eq!(status.peers[0].tx_bytes, 500);
    }

    #[tokio::test]
    async fn simulate_handshake() {
        let mut iface = FakeWireGuardInterface::new();
        iface.create("wg0", &test_config()).await.expect("create");

        let peer = test_peer();
        iface.add_peer("wg0", &peer).await.expect("add");

        let timestamp = 1700000000;
        iface
            .simulate_handshake("wg0", &peer.public_key, timestamp)
            .await
            .expect("handshake");

        let status = iface.get_status("wg0").await.expect("status");
        assert_eq!(status.peers[0].last_handshake, Some(timestamp));
    }
}
