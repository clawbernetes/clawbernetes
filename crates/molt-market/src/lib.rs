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
//! - Payment service integrating MOLT token
//!
//! ## Example
//!
//! ```rust,no_run
//! use molt_market::{PaymentService, OrderBook, JobOrder, JobRequirements};
//! use molt_token::{Wallet, Amount};
//!
//! # async fn example() -> Result<(), molt_market::MarketError> {
//! // Create payment service
//! let payment = PaymentService::devnet();
//!
//! // Create buyer wallet and fund it
//! let buyer = Wallet::generate()?;
//! payment.airdrop(buyer.address(), Amount::molt(100.0)).await?;
//!
//! // Create escrow for job
//! let provider = Wallet::generate()?;
//! let escrow = payment.create_escrow(
//!     &buyer,
//!     provider.address(),
//!     Amount::molt(50.0),
//!     "job-123".into(),
//! ).await?;
//!
//! // On job completion, release to provider
//! payment.release_escrow(&escrow.id).await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod escrow;
pub mod orderbook;
pub mod payment;
pub mod settlement;

pub use error::MarketError;
pub use escrow::{EscrowAccount, EscrowState};
pub use orderbook::{CapacityOffer, GpuCapacity, JobOrder, JobRequirements, OrderBook, OrderMatch};
pub use payment::{PaymentService, TokenEscrow};
pub use settlement::{calculate_payment, settle_job, JobSettlementInput, SettlementResult};
