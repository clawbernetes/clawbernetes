//! `WireGuard` interface management for Clawbernetes.
//!
//! This crate provides `WireGuard` tunnel management, mesh networking,
//! and secure peer-to-peer connectivity for Clawbernetes clusters.
//!
//! # Features
//!
//! - **Tunnel Management**: Create, configure, and destroy `WireGuard` interfaces
//! - **Key Management**: Generate and manage Curve25519 key pairs
//! - **Mesh Networking**: Full mesh, hub-spoke, and custom topologies
//! - **Health Monitoring**: Track peer connectivity and tunnel health
//! - **Auto-Peering**: Automatic peer discovery and configuration
//!
//! # Example
//!
//! ```rust,ignore
//! use claw_wireguard::{WireGuardManager, ManagerConfig, MeshTopology};
//! use claw_wireguard::interface::FakeWireGuardInterface;
//!
//! # async fn example() -> claw_wireguard::Result<()> {
//! // Create a manager with a fake interface (for testing)
//! let iface = FakeWireGuardInterface::new();
//! let config = ManagerConfig::default();
//! let mut manager = WireGuardManager::new(iface, config)?;
//!
//! // Create a tunnel with a generated key
//! let public_key = manager.create_tunnel_with_generated_key("wg0", None).await?;
//!
//! // Set up a mesh topology
//! let mesh = MeshTopology::full_mesh("10.100.0.0/16");
//! manager.set_mesh_topology(mesh).await;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod error;
mod interface;
mod keys;
#[cfg(feature = "linux")]
pub mod linux;
mod manager;
mod mesh;
mod tunnel;
mod types;

pub use config::{
    generate_wg_config, parse_wg_config, InterfaceConfig, InterfaceConfigBuilder, PeerConfig,
    PeerConfigBuilder,
};
pub use error::{Result, WireGuardError};
pub use interface::{FakeWireGuardInterface, InterfaceState, WireGuardInterface};
#[cfg(feature = "linux")]
pub use linux::LinuxWireGuardInterface;
pub use keys::{generate_keypair, public_key_from_private, KeyPair, PrivateKey, PublicKey, KEY_SIZE};
pub use manager::{ManagerConfig, MeshPeerBuilder, SyncResult, WireGuardManager};
pub use mesh::{
    MeshIpAllocator, MeshNode, MeshNodeId, MeshTopology, TopologyType, MAX_FULL_MESH_NODES,
};
pub use tunnel::{
    ConnectionState, PeerHealth, TunnelHealthSummary, TunnelPeerStatus, TunnelStatus,
    DEFAULT_HANDSHAKE_TIMEOUT_SECS, DEFAULT_KEEPALIVE_SECS,
};
pub use types::{AllowedIp, Endpoint, InterfaceStatus, PeerStatus, PresharedKey, WireGuardPeer};
