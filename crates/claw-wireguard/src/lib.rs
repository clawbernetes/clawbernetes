//! Minimal WireGuard key types for Clawbernetes P2P networking.
//!
//! This crate provides Curve25519 key types used by the MOLT P2P layer
//! for peer identity and tunnel establishment.

pub mod error;
mod keys;

pub use error::WireGuardError;
pub use keys::{generate_keypair, KeyPair, PrivateKey, PublicKey, KEY_SIZE};
