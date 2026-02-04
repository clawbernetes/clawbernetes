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

    /// Payment/token operation failed.
    #[error("payment error: {0}")]
    PaymentError(String),

    /// Wallet generation error.
    #[error("wallet error: {0}")]
    WalletError(String),

    /// Unauthorized operation attempt.
    /// 
    /// SECURITY: This error is returned when a caller attempts an operation
    /// they are not authorized to perform (e.g., releasing escrow without
    /// being the buyer).
    #[error("unauthorized: {action} requires {required_role}, but caller is {caller_role}")]
    Unauthorized {
        /// The action that was attempted.
        action: String,
        /// The role required to perform this action.
        required_role: String,
        /// The actual role of the caller.
        caller_role: String,
    },

    /// Escrow has expired due to timeout.
    /// 
    /// SECURITY: This error is returned when an operation is attempted on an
    /// escrow that has exceeded its timeout duration. Expired escrows can only
    /// be finalized via the `expire()` method which auto-refunds the buyer.
    #[error("escrow expired: {job_id} timed out after {timeout_days} days")]
    EscrowExpired {
        /// The job ID of the expired escrow.
        job_id: String,
        /// The timeout duration in days.
        timeout_days: u64,
    },
}

impl From<molt_token::MoltError> for MarketError {
    fn from(e: molt_token::MoltError) -> Self {
        Self::PaymentError(e.to_string())
    }
}
