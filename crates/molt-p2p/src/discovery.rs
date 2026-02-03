//! DHT-based peer discovery.
//!
//! This module provides peer discovery and tracking functionality.

use crate::protocol::{PeerId, PeerInfo};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// A table tracking known peers in the network.
#[derive(Debug, Clone, Default)]
pub struct PeerTable {
    peers: HashMap<PeerId, PeerInfo>,
}

impl PeerTable {
    /// Creates a new empty peer table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    /// Inserts or updates a peer in the table.
    pub fn insert(&mut self, info: PeerInfo) {
        self.peers.insert(info.peer_id(), info);
    }

    /// Retrieves a peer's info by their ID.
    #[must_use]
    pub fn get(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Removes a peer from the table.
    pub fn remove(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    /// Returns the number of peers in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Returns true if the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Returns all peers in the table.
    #[must_use]
    pub fn all_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().collect()
    }

    /// Finds peers that have a specific capability.
    #[must_use]
    pub fn find_by_capability(&self, capability: &str) -> Vec<&PeerInfo> {
        self.peers
            .values()
            .filter(|info| info.capabilities().contains(&capability.to_string()))
            .collect()
    }
}

/// A bootstrap node used for initial network discovery.
#[derive(Debug, Clone)]
pub struct BootstrapNode {
    address: String,
    connected: bool,
    last_connected: Option<DateTime<Utc>>,
}

impl BootstrapNode {
    /// Creates a new bootstrap node with the given address.
    #[must_use]
    pub const fn new(address: String) -> Self {
        Self {
            address,
            connected: false,
            last_connected: None,
        }
    }

    /// Returns the node's address.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns whether the node is currently connected.
    #[must_use]
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// Returns when the node was last connected, if ever.
    #[must_use]
    pub const fn last_connected(&self) -> Option<DateTime<Utc>> {
        self.last_connected
    }

    /// Marks the node as connected and updates the last connected time.
    pub fn mark_connected(&mut self) {
        self.connected = true;
        self.last_connected = Some(Utc::now());
    }

    /// Marks the node as disconnected.
    pub const fn mark_disconnected(&mut self) {
        self.connected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_peer_id() -> PeerId {
        let signing_key = SigningKey::generate(&mut OsRng);
        PeerId::from_public_key(&signing_key.verifying_key())
    }

    #[test]
    fn peer_table_insert_and_get() {
        let mut table = PeerTable::new();
        let peer_id = make_peer_id();

        let info = PeerInfo::new(
            peer_id,
            vec!["/ip4/192.168.1.1/tcp/8080".to_string()],
            vec!["gpu".to_string()],
        );

        table.insert(info.clone());

        let retrieved = table.get(&peer_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().peer_id(), peer_id);
    }

    #[test]
    fn peer_table_remove() {
        let mut table = PeerTable::new();
        let peer_id = make_peer_id();

        let info = PeerInfo::new(peer_id, vec![], vec![]);
        table.insert(info);

        assert!(table.get(&peer_id).is_some());

        table.remove(&peer_id);

        assert!(table.get(&peer_id).is_none());
    }

    #[test]
    fn peer_table_len() {
        let mut table = PeerTable::new();

        assert_eq!(table.len(), 0);
        assert!(table.is_empty());

        table.insert(PeerInfo::new(make_peer_id(), vec![], vec![]));
        table.insert(PeerInfo::new(make_peer_id(), vec![], vec![]));
        table.insert(PeerInfo::new(make_peer_id(), vec![], vec![]));

        assert_eq!(table.len(), 3);
        assert!(!table.is_empty());
    }

    #[test]
    fn peer_table_find_by_capability() {
        let mut table = PeerTable::new();

        let gpu_peer = make_peer_id();
        let cpu_peer = make_peer_id();
        let both_peer = make_peer_id();

        table.insert(PeerInfo::new(
            gpu_peer,
            vec![],
            vec!["gpu".to_string(), "inference".to_string()],
        ));
        table.insert(PeerInfo::new(
            cpu_peer,
            vec![],
            vec!["cpu".to_string()],
        ));
        table.insert(PeerInfo::new(
            both_peer,
            vec![],
            vec!["gpu".to_string(), "cpu".to_string()],
        ));

        let gpu_peers = table.find_by_capability("gpu");
        assert_eq!(gpu_peers.len(), 2);

        let cpu_peers = table.find_by_capability("cpu");
        assert_eq!(cpu_peers.len(), 2);

        let inference_peers = table.find_by_capability("inference");
        assert_eq!(inference_peers.len(), 1);
        assert_eq!(inference_peers[0].peer_id(), gpu_peer);
    }

    #[test]
    fn peer_table_update_existing_peer() {
        let mut table = PeerTable::new();
        let peer_id = make_peer_id();

        let info1 = PeerInfo::new(
            peer_id,
            vec!["/ip4/192.168.1.1/tcp/8080".to_string()],
            vec!["gpu".to_string()],
        );
        table.insert(info1);

        // Insert updated info for same peer
        let info2 = PeerInfo::new(
            peer_id,
            vec![
                "/ip4/192.168.1.1/tcp/8080".to_string(),
                "/ip4/10.0.0.1/tcp/9000".to_string(),
            ],
            vec!["gpu".to_string(), "cpu".to_string()],
        );
        table.insert(info2);

        // Should still have only one peer
        assert_eq!(table.len(), 1);

        // Should have updated capabilities
        let retrieved = table.get(&peer_id).unwrap();
        assert_eq!(retrieved.capabilities().len(), 2);
        assert_eq!(retrieved.addresses().len(), 2);
    }

    #[test]
    fn peer_table_all_peers() {
        let mut table = PeerTable::new();

        let id1 = make_peer_id();
        let id2 = make_peer_id();

        table.insert(PeerInfo::new(id1, vec![], vec![]));
        table.insert(PeerInfo::new(id2, vec![], vec![]));

        let all = table.all_peers();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn bootstrap_node_creation() {
        let address = "/ip4/1.2.3.4/tcp/8080".to_string();
        let node = BootstrapNode::new(address.clone());

        assert_eq!(node.address(), &address);
        assert!(!node.is_connected());
    }

    #[test]
    fn bootstrap_node_mark_connected() {
        let address = "/ip4/1.2.3.4/tcp/8080".to_string();
        let mut node = BootstrapNode::new(address);

        assert!(!node.is_connected());

        node.mark_connected();

        assert!(node.is_connected());
        assert!(node.last_connected().is_some());
    }

    #[test]
    fn bootstrap_node_mark_disconnected() {
        let address = "/ip4/1.2.3.4/tcp/8080".to_string();
        let mut node = BootstrapNode::new(address);

        node.mark_connected();
        assert!(node.is_connected());

        node.mark_disconnected();
        assert!(!node.is_connected());
        // last_connected should still be set
        assert!(node.last_connected().is_some());
    }
}
