//! Error types for molt-market.

use thiserror::Error;

/// Errors that can occur in marketplace operations.
#[derive(Debug, Error)]
pub enum MarketError {
    /// Insufficient funds for operation.
    #[error("insufficient funds: required {required}, available {available}")]
    InsufficientFunds {
        /// Amount required for the operation.
        required: u64,
        /// Amount currently available.
        available: u64,
    },

    /// Order not found.
    #[error("order not found: {0}")]
    OrderNotFound(String),

    /// Escrow error.
    #[error("escrow error: {0}")]
    Escrow(String),

    /// Settlement failed.
    #[error("settlement failed: {0}")]
    Settlement(String),

    /// Invalid order state transition.
    #[error("invalid state transition: {from} -> {to}")]
    InvalidStateTransition {
        /// The current state.
        from: String,
        /// The attempted target state.
        to: String,
    },
}
