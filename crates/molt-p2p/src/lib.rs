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
//! - **Eclipse attack mitigation** via peer diversity enforcement
//!
//! ## Core Types
//!
//! - [`PeerId`]: Unique identifier for peers, derived from Ed25519 public keys
//! - [`PeerInfo`]: Metadata about a known peer
//! - [`CapacityAnnouncement`]: Signed announcement of compute capacity
//! - [`PeerTable`]: Table for tracking known peers
//! - [`PeerDiversityConfig`]: Configuration for peer diversity requirements
//! - [`PeerDiversityTracker`]: Tracks peer diversity to prevent eclipse attacks

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod connection;
pub mod discovery;
pub mod diversity;
pub mod error;
pub mod gossip;
pub mod message;
pub mod nat;
pub mod network;
pub mod protocol;
pub mod tunnel;

pub use connection::{
    ActiveConnection, ConnectionHealth, ConnectionPool, ConnectionPoolConfig, ConnectionState,
    MessageChannel, PeerConnection, SharedConnectionPool,
};
pub use discovery::{BootstrapNode, PeerTable};
pub use diversity::{
    Asn, DiversityResult, DiversityStats, GeoRegion, Ipv4Subnet, Ipv6Prefix,
    PeerDiversityConfig, PeerDiversityInfo, PeerDiversityTracker,
};
pub use error::P2pError;
pub use gossip::{
    BroadcastConfig, BroadcastResult, CapacityAnnouncement, GossipBroadcaster, GossipEvent,
    GossipMessage, GossipNode, GossipQuery, GpuInfo, LocalGossipNode, MessageId, NodeState,
    Pricing, QueryFilter,
};
pub use message::{CapacityRequirements, P2pMessage};
pub use nat::{EndpointDiscovery, NatTraversal, NatType};
pub use network::{MoltNetwork, NetworkConfig, NetworkState, NetworkStats, ProviderSearchResult};
pub use protocol::{PeerId, PeerInfo};
pub use tunnel::{BandwidthStats, Job, JobId, JobTunnel, TunnelManager, TunnelState};
