//! Wire protocol definitions for P2P communication.
//!
//! This module defines the core types used in P2P networking:
//! - [`PeerId`]: Unique identifier for peers, derived from Ed25519 public keys
//! - [`PeerInfo`]: Metadata about a known peer

use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a peer in the network.
///
/// A `PeerId` is derived from an Ed25519 public key. The bytes stored are the
/// raw 32-byte public key, which can be displayed as base58 for human readability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId {
    bytes: [u8; 32],
}

impl PeerId {
    /// Creates a `PeerId` from an Ed25519 public key.
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
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", bs58::encode(&self.bytes).into_string())
    }
}

/// Information about a known peer in the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    peer_id: PeerId,
    addresses: Vec<String>,
    capabilities: Vec<String>,
    last_seen: DateTime<Utc>,
}

impl PeerInfo {
    /// Creates a new `PeerInfo` with the given peer ID, addresses, and capabilities.
    #[must_use]
    pub fn new(peer_id: PeerId, addresses: Vec<String>, capabilities: Vec<String>) -> Self {
        Self {
            peer_id,
            addresses,
            capabilities,
            last_seen: Utc::now(),
        }
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
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn peer_id_from_public_key_is_deterministic() {
        // Generate a keypair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        // Creating PeerId from the same key should always produce the same ID
        let peer_id_1 = PeerId::from_public_key(&verifying_key);
        let peer_id_2 = PeerId::from_public_key(&verifying_key);

        assert_eq!(peer_id_1, peer_id_2);
    }

    #[test]
    fn peer_id_display_is_base58() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let peer_id = PeerId::from_public_key(&verifying_key);
        let displayed = peer_id.to_string();

        // Base58 alphabet doesn't contain 0, O, I, l
        assert!(!displayed.contains('0'));
        assert!(!displayed.contains('O'));
        assert!(!displayed.contains('I'));
        assert!(!displayed.contains('l'));

        // Should be non-empty
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
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        assert_eq!(peer_id.as_bytes().len(), 32);
    }

    #[test]
    fn peer_id_from_bytes_roundtrip() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let original = PeerId::from_public_key(&signing_key.verifying_key());

        let reconstructed = PeerId::from_bytes(*original.as_bytes());

        assert_eq!(original, reconstructed);
    }

    #[test]
    fn peer_info_creation() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let info = PeerInfo::new(
            peer_id,
            vec!["/ip4/192.168.1.1/tcp/8080".to_string()],
            vec!["gpu".to_string(), "cpu".to_string()],
        );

        assert_eq!(info.peer_id(), peer_id);
        assert_eq!(info.addresses().len(), 1);
        assert_eq!(info.capabilities().len(), 2);
    }

    #[test]
    fn peer_info_last_seen_updates() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let mut info = PeerInfo::new(peer_id, vec![], vec![]);
        let original_last_seen = info.last_seen();

        // Small sleep to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));
        info.touch();

        assert!(info.last_seen() > original_last_seen);
    }

    #[test]
    fn peer_info_add_address() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

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
