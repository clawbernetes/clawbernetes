//! # molt-token
//!
//! MOLT token (Marketplace Of Liquid Tensors) for the P2P GPU compute marketplace.
//!
//! This crate provides:
//! - Wallet management (keypair generation, balance queries)
//! - Token operations (transfer, escrow, release)
//! - Transaction building and signing
//! - Solana network interaction (devnet/mainnet)
//!
//! ## Token Details
//!
//! - **Name**: MOLT (Marketplace Of Liquid Tensors)
//! - **Network**: Solana (SPL Token)
//! - **Decimals**: 9 (1 MOLT = `1_000_000_000` lamports)
//! - **Use**: Payment for GPU compute on the MOLT network
//!
//! ## Example
//!
//! ```rust,no_run
//! use molt_token::{Wallet, MoltClient, Amount};
//!
//! # async fn example() -> molt_token::Result<()> {
//! // Create or load wallet
//! let wallet = Wallet::generate()?;
//! println!("Address: {}", wallet.address());
//!
//! // Connect to Solana
//! let client = MoltClient::devnet();
//!
//! // Check balance
//! let balance = client.balance(&wallet.address()).await?;
//! println!("Balance: {} MOLT", balance);
//!
//! // Transfer tokens (to another wallet)
//! let recipient = Wallet::generate()?;
//! client.transfer(&wallet, recipient.address(), Amount::molt(10.0)).await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod amount;
pub mod client;
pub mod error;
pub mod escrow;
pub mod transaction;
pub mod wallet;

pub use amount::Amount;
pub use client::{MoltClient, Network};
pub use error::{MoltError, Result};
pub use escrow::{Escrow, EscrowId, EscrowState};
pub use transaction::{Transaction, TransactionId, TransactionStatus};
pub use wallet::{Address, Wallet};

/// MOLT token mint address (Solana SPL token)
pub const MOLT_MINT: &str = "MoLTxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

/// MOLT token decimals
pub const MOLT_DECIMALS: u8 = 9;

/// One MOLT in base units (lamports)
pub const LAMPORTS_PER_MOLT: u64 = 1_000_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MOLT_DECIMALS, 9);
        assert_eq!(LAMPORTS_PER_MOLT, 1_000_000_000);
    }
}
