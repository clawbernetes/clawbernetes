//! # molt-p2p
//!
//! P2P networking layer for the MOLT compute network.
//!
//! This crate provides:
//!
//! - Peer discovery via DHT
//! - Gossip protocol for capacity announcements
//! - Secure QUIC connections between nodes
//! - NAT traversal and relay support
//!
//! ## Core Types
//!
//! - [`PeerId`]: Unique identifier for peers, derived from Ed25519 public keys
//! - [`PeerInfo`]: Metadata about a known peer
//! - [`CapacityAnnouncement`]: Signed announcement of compute capacity
//! - [`PeerTable`]: Table for tracking known peers

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod connection;
pub mod discovery;
pub mod error;
pub mod gossip;
pub mod message;
pub mod network;
pub mod protocol;

pub use connection::{
    ConnectionHealth, ConnectionPool, ConnectionPoolConfig, ConnectionState, PeerConnection,
    SharedConnectionPool,
};
pub use discovery::{BootstrapNode, PeerTable};
pub use error::P2pError;
pub use gossip::{CapacityAnnouncement, GpuInfo, Pricing};
pub use message::{CapacityRequirements, P2pMessage};
pub use network::{MoltNetwork, NetworkConfig, NetworkState, NetworkStats, ProviderSearchResult};
pub use protocol::{PeerId, PeerInfo};
