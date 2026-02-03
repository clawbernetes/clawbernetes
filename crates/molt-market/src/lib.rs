//! # molt-market
//!
//! Decentralized marketplace protocol for MOLT compute network.
//!
//! This crate provides:
//!
//! - Order book for job matching
//! - Escrow management for payments
//! - Settlement logic for completed jobs
//! - Reputation updates based on job outcomes

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod escrow;
pub mod orderbook;
pub mod settlement;

pub use error::MarketError;
pub use escrow::{EscrowAccount, EscrowState};
pub use orderbook::{CapacityOffer, GpuCapacity, JobOrder, JobRequirements, OrderBook, OrderMatch};
pub use settlement::{calculate_payment, settle_job, JobSettlementInput, SettlementResult};
