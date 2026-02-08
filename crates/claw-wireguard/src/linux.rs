//! Linux kernel `WireGuard` interface using netlink API.
//!
//! Wraps `defguard_wireguard_rs` to implement the `WireGuardInterface` trait
//! using the kernel-level WireGuard implementation via netlink.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use defguard_wireguard_rs::host::Peer as DgPeer;
use defguard_wireguard_rs::net::IpAddrMask;
use defguard_wireguard_rs::{
    InterfaceConfiguration, Kernel, WGApi, WireguardInterfaceApi,
};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::config::{InterfaceConfig, PeerConfig};
use crate::error::{Result, WireGuardError};
use crate::keys::PublicKey;
use crate::types::{InterfaceStatus, PeerStatus};

/// Linux kernel-based `WireGuard` interface manager.
///
/// Uses `defguard_wireguard_rs` with the `Kernel` backend to manage
/// real WireGuard interfaces via the netlink API.
pub struct LinuxWireGuardInterface {
    managed: Arc<RwLock<HashMap<String, WGApi<Kernel>>>>,
}

impl LinuxWireGuardInterface {
    /// Creates a new Linux WireGuard interface manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            managed: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for LinuxWireGuardInterface {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert our `AllowedIp` to defguard's `IpAddrMask`.
fn to_ip_addr_mask(allowed_ip: &crate::types::AllowedIp) -> std::result::Result<IpAddrMask, WireGuardError> {
    IpAddrMask::from_str(&allowed_ip.to_cidr())
        .map_err(|e| WireGuardError::InvalidCidr(format!("{}: {e}", allowed_ip.to_cidr())))
}

/// Convert our `PublicKey` to defguard's `Key`.
fn to_defguard_key(key: &PublicKey) -> std::result::Result<defguard_wireguard_rs::key::Key, WireGuardError> {
    let bytes = key.as_bytes();
    defguard_wireguard_rs::key::Key::try_from(bytes.as_slice())
        .map_err(|e| WireGuardError::InvalidKey(format!("defguard key conversion: {e}")))
}

/// Build a defguard `Peer` from our `PeerConfig`.
fn build_defguard_peer(
    peer: &PeerConfig,
) -> std::result::Result<DgPeer, WireGuardError> {
    let pub_key = to_defguard_key(&peer.public_key)?;
    let mut dg_peer = DgPeer::new(pub_key);

    if let Some(ref endpoint) = peer.endpoint {
        dg_peer.endpoint = Some(*endpoint.address());
    }

    if let Some(keepalive) = peer.persistent_keepalive {
        dg_peer.persistent_keepalive_interval = Some(keepalive);
    }

    if let Some(ref psk) = peer.preshared_key {
        let psk_key = defguard_wireguard_rs::key::Key::try_from(psk.as_bytes().as_slice())
            .map_err(|e| WireGuardError::InvalidKey(format!("preshared key: {e}")))?;
        dg_peer.preshared_key = Some(psk_key);
    }

    for aip in &peer.allowed_ips {
        dg_peer.allowed_ips.push(to_ip_addr_mask(aip)?);
    }

    Ok(dg_peer)
}

impl crate::interface::WireGuardInterface for LinuxWireGuardInterface {
    async fn create(&mut self, name: &str, config: &InterfaceConfig) -> Result<()> {
        let mut managed = self.managed.write().await;

        if managed.contains_key(name) {
            return Err(WireGuardError::InterfaceExists(name.to_string()));
        }

        info!(interface = %name, "creating Linux WireGuard interface");

        let mut api = WGApi::<Kernel>::new(name.to_string())
            .map_err(|e| WireGuardError::InterfaceError(format!("WGApi::new: {e}")))?;

        api.create_interface()
            .map_err(|e| WireGuardError::InterfaceError(format!("create_interface: {e}")))?;

        // Build peers list for InterfaceConfiguration
        let mut dg_peers = Vec::new();
        for peer in &config.peers {
            dg_peers.push(build_defguard_peer(peer)?);
        }

        // Convert addresses to defguard format
        let addresses: Vec<IpAddrMask> = config
            .addresses
            .iter()
            .map(to_ip_addr_mask)
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Assign addresses
        for addr in &addresses {
            api.assign_address(addr)
                .map_err(|e| WireGuardError::InterfaceError(format!("assign_address: {e}")))?;
        }

        // Build InterfaceConfiguration
        let iface_config = InterfaceConfiguration {
            name: name.to_string(),
            prvkey: config.private_key.to_base64(),
            addresses: addresses.clone(),
            port: config.listen_port.unwrap_or(0),
            peers: dg_peers.clone(),
            mtu: config.mtu.map(u32::from),
        };

        api.configure_interface(&iface_config)
            .map_err(|e| WireGuardError::InterfaceError(format!("configure_interface: {e}")))?;

        // Configure peer routing
        if !dg_peers.is_empty() {
            api.configure_peer_routing(&dg_peers)
                .map_err(|e| WireGuardError::InterfaceError(format!("configure_peer_routing: {e}")))?;
        }

        managed.insert(name.to_string(), api);
        info!(interface = %name, "Linux WireGuard interface created");

        Ok(())
    }

    async fn destroy(&mut self, name: &str) -> Result<()> {
        let mut managed = self.managed.write().await;

        let api = managed
            .remove(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        info!(interface = %name, "destroying Linux WireGuard interface");

        api.remove_interface()
            .map_err(|e| WireGuardError::InterfaceError(format!("remove_interface: {e}")))?;

        Ok(())
    }

    async fn add_peer(&mut self, name: &str, peer: &PeerConfig) -> Result<()> {
        let managed = self.managed.read().await;

        let api = managed
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let pub_key_b64 = peer.public_key.to_base64();
        debug!(interface = %name, peer = %pub_key_b64, "adding peer");

        // Check if peer already exists
        let host = api
            .read_interface_data()
            .map_err(|e| WireGuardError::InterfaceError(format!("read_interface_data: {e}")))?;

        let dg_key = to_defguard_key(&peer.public_key)?;
        if host.peers.contains_key(&dg_key) {
            return Err(WireGuardError::PeerExists(pub_key_b64));
        }

        let dg_peer = build_defguard_peer(peer)?;

        api.configure_peer(&dg_peer)
            .map_err(|e| WireGuardError::InterfaceError(format!("configure_peer: {e}")))?;

        api.configure_peer_routing(&[dg_peer])
            .map_err(|e| WireGuardError::InterfaceError(format!("configure_peer_routing: {e}")))?;

        Ok(())
    }

    async fn remove_peer(&mut self, name: &str, public_key: &PublicKey) -> Result<()> {
        let managed = self.managed.read().await;

        let api = managed
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let dg_key = to_defguard_key(public_key)?;
        let pub_key_b64 = public_key.to_base64();

        // Check peer exists
        let host = api
            .read_interface_data()
            .map_err(|e| WireGuardError::InterfaceError(format!("read_interface_data: {e}")))?;

        if !host.peers.contains_key(&dg_key) {
            return Err(WireGuardError::PeerNotFound(pub_key_b64));
        }

        debug!(interface = %name, peer = %pub_key_b64, "removing peer");

        api.remove_peer(&dg_key)
            .map_err(|e| WireGuardError::InterfaceError(format!("remove_peer: {e}")))?;

        Ok(())
    }

    async fn get_status(&self, name: &str) -> Result<InterfaceStatus> {
        let managed = self.managed.read().await;

        let api = managed
            .get(name)
            .ok_or_else(|| WireGuardError::InterfaceNotFound(name.to_string()))?;

        let host = api
            .read_interface_data()
            .map_err(|e| WireGuardError::InterfaceError(format!("read_interface_data: {e}")))?;

        // Derive public key from private key if available
        let public_key = if let Some(ref priv_key) = host.private_key {
            let pub_key = priv_key.public_key();
            let bytes: [u8; 32] = pub_key.as_array();
            PublicKey::from_bytes(&bytes)
                .unwrap_or_else(|_| PublicKey::from_bytes(&[0u8; 32]).expect("zero key"))
        } else {
            PublicKey::from_bytes(&[0u8; 32]).expect("zero key")
        };

        let peers: Vec<PeerStatus> = host
            .peers
            .values()
            .map(|dg_peer| {
                let peer_pub_bytes: [u8; 32] = dg_peer.public_key.as_array();
                let peer_pub = PublicKey::from_bytes(&peer_pub_bytes)
                    .unwrap_or_else(|_| PublicKey::from_bytes(&[0u8; 32]).expect("zero key"));

                let endpoint = dg_peer.endpoint.map(|addr| crate::types::Endpoint::new(addr));

                let allowed_ips: Vec<crate::types::AllowedIp> = dg_peer
                    .allowed_ips
                    .iter()
                    .filter_map(|aip| crate::types::AllowedIp::from_cidr(&aip.to_string()).ok())
                    .collect();

                let last_handshake = dg_peer.last_handshake.and_then(|t| {
                    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
                });

                PeerStatus {
                    public_key: peer_pub,
                    endpoint,
                    allowed_ips,
                    last_handshake,
                    rx_bytes: dg_peer.rx_bytes,
                    tx_bytes: dg_peer.tx_bytes,
                }
            })
            .collect();

        let listen_port = if host.listen_port > 0 {
            Some(host.listen_port as u16)
        } else {
            None
        };

        Ok(InterfaceStatus {
            name: name.to_string(),
            public_key,
            listen_port,
            peers,
        })
    }

    async fn list_interfaces(&self) -> Result<Vec<String>> {
        let managed = self.managed.read().await;
        Ok(managed.keys().cloned().collect())
    }

    async fn interface_exists(&self, name: &str) -> bool {
        let managed = self.managed.read().await;
        managed.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::WireGuardInterface;
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
        peer.allowed_ips
            .push(AllowedIp::from_cidr("10.0.0.2/32").expect("valid cidr"));
        peer
    }

    // Integration tests requiring root + WireGuard kernel module
    // Run with: sudo cargo test -p claw-wireguard --features linux -- --ignored

    #[tokio::test]
    #[ignore = "requires root and WireGuard kernel module"]
    async fn linux_create_and_destroy_interface() {
        let mut iface = LinuxWireGuardInterface::new();
        let config = test_config();

        iface.create("clawtest0", &config).await.expect("create");
        assert!(iface.interface_exists("clawtest0").await);

        let status = iface.get_status("clawtest0").await.expect("status");
        assert_eq!(status.name, "clawtest0");
        assert_eq!(status.listen_port, Some(51820));

        iface.destroy("clawtest0").await.expect("destroy");
        assert!(!iface.interface_exists("clawtest0").await);
    }

    #[tokio::test]
    #[ignore = "requires root and WireGuard kernel module"]
    async fn linux_add_and_remove_peer() {
        let mut iface = LinuxWireGuardInterface::new();
        let config = test_config();

        iface.create("clawtest1", &config).await.expect("create");

        let peer = test_peer();
        iface
            .add_peer("clawtest1", &peer)
            .await
            .expect("add peer");

        let status = iface.get_status("clawtest1").await.expect("status");
        assert_eq!(status.peers.len(), 1);

        iface
            .remove_peer("clawtest1", &peer.public_key)
            .await
            .expect("remove peer");

        let status = iface.get_status("clawtest1").await.expect("status");
        assert_eq!(status.peers.len(), 0);

        iface.destroy("clawtest1").await.expect("destroy");
    }

    #[tokio::test]
    #[ignore = "requires root and WireGuard kernel module"]
    async fn linux_duplicate_create_fails() {
        let mut iface = LinuxWireGuardInterface::new();
        let config = test_config();

        iface.create("clawtest2", &config).await.expect("create");

        let result = iface.create("clawtest2", &config).await;
        assert!(matches!(result, Err(WireGuardError::InterfaceExists(_))));

        iface.destroy("clawtest2").await.expect("destroy");
    }

    #[tokio::test]
    #[ignore = "requires root and WireGuard kernel module"]
    async fn linux_duplicate_peer_fails() {
        let mut iface = LinuxWireGuardInterface::new();
        let config = test_config();

        iface.create("clawtest3", &config).await.expect("create");

        let peer = test_peer();
        iface
            .add_peer("clawtest3", &peer)
            .await
            .expect("add peer");

        let result = iface.add_peer("clawtest3", &peer).await;
        assert!(matches!(result, Err(WireGuardError::PeerExists(_))));

        iface.destroy("clawtest3").await.expect("destroy");
    }

    #[tokio::test]
    #[ignore = "requires root and WireGuard kernel module"]
    async fn linux_list_interfaces() {
        let mut iface = LinuxWireGuardInterface::new();

        iface.create("clawtest4", &test_config()).await.expect("create");

        let list = iface.list_interfaces().await.expect("list");
        assert!(list.contains(&"clawtest4".to_string()));

        iface.destroy("clawtest4").await.expect("destroy");
    }
}
