//! # molt-core
//!
//! MOLT token primitives for the Clawbernetes P2P compute network.
//!
//! This crate provides:
//!
//! - [`Amount`] — Token amount with fixed-point precision
//! - [`Wallet`] — Key management and transaction signing
//! - [`Policy`] — Autonomy policies (Conservative/Moderate/Aggressive)
//! - [`Reputation`] — Provider/buyer reputation scoring

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod amount;
pub mod error;
pub mod policy;
pub mod reputation;
pub mod wallet;

pub use amount::Amount;
pub use error::MoltError;
pub use policy::{AutonomyLevel, Policy, PolicyBuilder};
pub use reputation::{Reputation, Score};
pub use wallet::{PublicKey, Signature, Wallet};
