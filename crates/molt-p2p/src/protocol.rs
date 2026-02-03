//! Wire protocol definitions for P2P communication.
//!
//! This module defines the core types used in P2P networking:
//! - [`PeerId`]: Unique identifier for peers, derived from WireGuard public keys
//! - [`PeerInfo`]: Metadata about a known peer

use chrono::{DateTime, Utc};
use claw_wireguard::PublicKey as WireGuardPublicKey;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::Ipv4Addr;

/// Unique identifier for a peer in the network.
///
/// A `PeerId` is derived from a WireGuard public key (Curve25519). The bytes stored
/// are the raw 32-byte public key, which can be displayed as base58 for human readability.
///
/// This allows the PeerId to serve double duty: it's both the peer identifier and the
/// WireGuard public key used to establish encrypted tunnels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId {
    bytes: [u8; 32],
}

impl PeerId {
    /// Creates a `PeerId` from a WireGuard public key.
    ///
    /// This is the primary constructor for MOLT network peers.
    #[must_use]
    pub fn from_wireguard_key(key: &WireGuardPublicKey) -> Self {
        Self {
            bytes: *key.as_bytes(),
        }
    }

    /// Creates a `PeerId` from an Ed25519 public key.
    ///
    /// Maintained for backward compatibility.
    #[must_use]
    pub fn from_public_key(key: &VerifyingKey) -> Self {
        Self {
            bytes: key.to_bytes(),
        }
    }

    /// Creates a `PeerId` from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Returns the raw bytes of the peer ID.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Converts the PeerId to a WireGuard public key.
    ///
    /// Since PeerId is derived from a WireGuard key, this conversion is lossless.
    #[must_use]
    pub fn to_wireguard_key(&self) -> WireGuardPublicKey {
        WireGuardPublicKey::from_bytes_array(self.bytes)
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", bs58::encode(&self.bytes).into_string())
    }
}

impl From<WireGuardPublicKey> for PeerId {
    fn from(key: WireGuardPublicKey) -> Self {
        Self::from_wireguard_key(&key)
    }
}

impl From<&WireGuardPublicKey> for PeerId {
    fn from(key: &WireGuardPublicKey) -> Self {
        Self::from_wireguard_key(key)
    }
}

/// Information about a known peer in the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique peer identifier (derived from WireGuard public key).
    peer_id: PeerId,
    /// Known network addresses for this peer.
    addresses: Vec<String>,
    /// Capabilities advertised by this peer.
    capabilities: Vec<String>,
    /// When this peer was last seen.
    last_seen: DateTime<Utc>,
    /// The peer's WireGuard public key (same bytes as peer_id).
    wireguard_key: WireGuardPublicKey,
    /// The peer's assigned mesh IP address (for WireGuard tunnel routing).
    mesh_ip: Option<Ipv4Addr>,
}

impl PeerInfo {
    /// Creates a new `PeerInfo` with the given peer ID, addresses, and capabilities.
    #[must_use]
    pub fn new(peer_id: PeerId, addresses: Vec<String>, capabilities: Vec<String>) -> Self {
        let wireguard_key = peer_id.to_wireguard_key();
        Self {
            peer_id,
            addresses,
            capabilities,
            last_seen: Utc::now(),
            wireguard_key,
            mesh_ip: None,
        }
    }

    /// Creates a new `PeerInfo` from a WireGuard public key.
    #[must_use]
    pub fn from_wireguard_key(
        key: &WireGuardPublicKey,
        addresses: Vec<String>,
        capabilities: Vec<String>,
    ) -> Self {
        let peer_id = PeerId::from_wireguard_key(key);
        Self {
            peer_id,
            addresses,
            capabilities,
            last_seen: Utc::now(),
            wireguard_key: *key,
            mesh_ip: None,
        }
    }

    /// Creates a new `PeerInfo` with a mesh IP.
    #[must_use]
    pub fn with_mesh_ip(mut self, ip: Ipv4Addr) -> Self {
        self.mesh_ip = Some(ip);
        self
    }

    /// Returns the peer's unique identifier.
    #[must_use]
    pub const fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Returns the peer's known network addresses.
    #[must_use]
    pub fn addresses(&self) -> &[String] {
        &self.addresses
    }

    /// Returns the peer's advertised capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// Returns when the peer was last seen.
    #[must_use]
    pub const fn last_seen(&self) -> DateTime<Utc> {
        self.last_seen
    }

    /// Returns the peer's WireGuard public key.
    #[must_use]
    pub const fn wireguard_key(&self) -> &WireGuardPublicKey {
        &self.wireguard_key
    }

    /// Returns the peer's mesh IP address if assigned.
    #[must_use]
    pub const fn mesh_ip(&self) -> Option<Ipv4Addr> {
        self.mesh_ip
    }

    /// Sets the mesh IP address.
    pub fn set_mesh_ip(&mut self, ip: Ipv4Addr) {
        self.mesh_ip = Some(ip);
    }

    /// Updates the last seen timestamp to now.
    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }

    /// Adds a new address if it doesn't already exist.
    pub fn add_address(&mut self, address: String) {
        if !self.addresses.contains(&address) {
            self.addresses.push(address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_wireguard::KeyPair;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    // ==================== WireGuard PeerId Tests ====================

    #[test]
    fn peer_id_from_wireguard_key_is_deterministic() {
        let keypair = KeyPair::generate();
        let public_key = keypair.public_key();

        let peer_id_1 = PeerId::from_wireguard_key(public_key);
        let peer_id_2 = PeerId::from_wireguard_key(public_key);

        assert_eq!(peer_id_1, peer_id_2);
    }

    #[test]
    fn peer_id_from_different_wireguard_keys_differ() {
        let keypair1 = KeyPair::generate();
        let keypair2 = KeyPair::generate();

        let peer_id_1 = PeerId::from_wireguard_key(keypair1.public_key());
        let peer_id_2 = PeerId::from_wireguard_key(keypair2.public_key());

        assert_ne!(peer_id_1, peer_id_2);
    }

    #[test]
    fn peer_id_to_wireguard_key_roundtrip() {
        let keypair = KeyPair::generate();
        let original_key = keypair.public_key();

        let peer_id = PeerId::from_wireguard_key(original_key);
        let recovered_key = peer_id.to_wireguard_key();

        assert_eq!(original_key, &recovered_key);
    }

    #[test]
    fn peer_id_from_wireguard_impl() {
        let keypair = KeyPair::generate();
        let peer_id: PeerId = keypair.public_key().into();
        assert_eq!(peer_id.as_bytes(), keypair.public_key().as_bytes());
    }

    // ==================== Backward Compatibility Tests ====================

    #[test]
    fn peer_id_from_public_key_is_deterministic() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let peer_id_1 = PeerId::from_public_key(&verifying_key);
        let peer_id_2 = PeerId::from_public_key(&verifying_key);

        assert_eq!(peer_id_1, peer_id_2);
    }

    #[test]
    fn peer_id_display_is_base58() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());
        let displayed = peer_id.to_string();

        // Base58 alphabet doesn't contain 0, O, I, l
        assert!(!displayed.contains('0'));
        assert!(!displayed.contains('O'));
        assert!(!displayed.contains('I'));
        assert!(!displayed.contains('l'));
        assert!(!displayed.is_empty());
    }

    #[test]
    fn peer_id_different_keys_produce_different_ids() {
        let key1 = SigningKey::generate(&mut OsRng);
        let key2 = SigningKey::generate(&mut OsRng);

        let peer_id_1 = PeerId::from_public_key(&key1.verifying_key());
        let peer_id_2 = PeerId::from_public_key(&key2.verifying_key());

        assert_ne!(peer_id_1, peer_id_2);
    }

    #[test]
    fn peer_id_as_bytes_returns_32_bytes() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());

        assert_eq!(peer_id.as_bytes().len(), 32);
    }

    #[test]
    fn peer_id_from_bytes_roundtrip() {
        let keypair = KeyPair::generate();
        let original = PeerId::from_wireguard_key(keypair.public_key());

        let reconstructed = PeerId::from_bytes(*original.as_bytes());

        assert_eq!(original, reconstructed);
    }

    // ==================== PeerInfo Tests ====================

    #[test]
    fn peer_info_creation() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());

        let info = PeerInfo::new(
            peer_id,
            vec!["/ip4/192.168.1.1/tcp/8080".to_string()],
            vec!["gpu".to_string(), "cpu".to_string()],
        );

        assert_eq!(info.peer_id(), peer_id);
        assert_eq!(info.addresses().len(), 1);
        assert_eq!(info.capabilities().len(), 2);
        assert_eq!(info.wireguard_key(), keypair.public_key());
        assert!(info.mesh_ip().is_none());
    }

    #[test]
    fn peer_info_from_wireguard_key() {
        let keypair = KeyPair::generate();

        let info = PeerInfo::from_wireguard_key(
            keypair.public_key(),
            vec!["10.0.0.1:51820".to_string()],
            vec!["provider".to_string()],
        );

        assert_eq!(info.wireguard_key(), keypair.public_key());
        assert_eq!(info.peer_id(), PeerId::from_wireguard_key(keypair.public_key()));
    }

    #[test]
    fn peer_info_with_mesh_ip() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());
        let mesh_ip = Ipv4Addr::new(10, 200, 0, 1);

        let info = PeerInfo::new(peer_id, vec![], vec![]).with_mesh_ip(mesh_ip);

        assert_eq!(info.mesh_ip(), Some(mesh_ip));
    }

    #[test]
    fn peer_info_set_mesh_ip() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());
        let mesh_ip = Ipv4Addr::new(10, 200, 0, 42);

        let mut info = PeerInfo::new(peer_id, vec![], vec![]);
        assert!(info.mesh_ip().is_none());

        info.set_mesh_ip(mesh_ip);
        assert_eq!(info.mesh_ip(), Some(mesh_ip));
    }

    #[test]
    fn peer_info_last_seen_updates() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());

        let mut info = PeerInfo::new(peer_id, vec![], vec![]);
        let original_last_seen = info.last_seen();

        std::thread::sleep(std::time::Duration::from_millis(10));
        info.touch();

        assert!(info.last_seen() > original_last_seen);
    }

    #[test]
    fn peer_info_add_address() {
        let keypair = KeyPair::generate();
        let peer_id = PeerId::from_wireguard_key(keypair.public_key());

        let mut info = PeerInfo::new(peer_id, vec![], vec![]);
        assert!(info.addresses().is_empty());

        info.add_address("/ip4/10.0.0.1/tcp/9000".to_string());
        assert_eq!(info.addresses().len(), 1);

        // Adding duplicate should not increase count
        info.add_address("/ip4/10.0.0.1/tcp/9000".to_string());
        assert_eq!(info.addresses().len(), 1);
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn peer_id_from_bytes_roundtrip_prop(bytes in prop::array::uniform32(any::<u8>())) {
                let peer_id = PeerId::from_bytes(bytes);
                prop_assert_eq!(*peer_id.as_bytes(), bytes);
            }

            #[test]
            fn peer_id_display_never_panics(bytes in prop::array::uniform32(any::<u8>())) {
                let peer_id = PeerId::from_bytes(bytes);
                let _display = peer_id.to_string();
            }

            #[test]
            fn peer_id_wireguard_roundtrip(bytes in prop::array::uniform32(any::<u8>())) {
                let wg_key = claw_wireguard::PublicKey::from_bytes_array(bytes);
                let peer_id = PeerId::from_wireguard_key(&wg_key);
                let recovered = peer_id.to_wireguard_key();
                prop_assert_eq!(wg_key, recovered);
            }

            #[test]
            fn peer_info_serialization_roundtrip(
                bytes in prop::array::uniform32(any::<u8>()),
                addresses in prop::collection::vec(".*", 0..5),
                capabilities in prop::collection::vec("[a-z]+", 0..5)
            ) {
                let peer_id = PeerId::from_bytes(bytes);
                let info = PeerInfo::new(peer_id, addresses, capabilities);

                let json = serde_json::to_string(&info).unwrap();
                let deserialized: PeerInfo = serde_json::from_str(&json).unwrap();

                prop_assert_eq!(info.peer_id(), deserialized.peer_id());
            }
        }
    }
}
