//! WireGuard interface management for Clawbernetes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod error;
mod interface;
mod keys;
mod types;

pub use config::{InterfaceConfig, InterfaceConfigBuilder, PeerConfig, PeerConfigBuilder};
pub use error::{Result, WireGuardError};
pub use interface::{InterfaceState, WireGuardInterface};
pub use keys::{generate_keypair, public_key_from_private, KeyPair, PrivateKey, PublicKey, KEY_SIZE};
pub use types::{AllowedIp, Endpoint, InterfaceStatus, PeerStatus, PresharedKey, WireGuardPeer};
