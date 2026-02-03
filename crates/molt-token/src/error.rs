//! Error types for MOLT token operations.

use thiserror::Error;

/// Result type alias for MOLT operations.
pub type Result<T> = std::result::Result<T, MoltError>;

/// Errors that can occur during MOLT token operations.
#[derive(Debug, Error)]
pub enum MoltError {
    /// Invalid wallet address format.
    #[error("invalid address: {message}")]
    InvalidAddress {
        /// Description of the address error.
        message: String,
    },

    /// Insufficient balance for operation.
    #[error("insufficient balance: have {have} MOLT, need {need} MOLT")]
    InsufficientBalance {
        /// Current balance.
        have: f64,
        /// Required balance.
        need: f64,
    },

    /// Transaction failed.
    #[error("transaction failed: {reason}")]
    TransactionFailed {
        /// Reason for failure.
        reason: String,
    },

    /// Transaction not found.
    #[error("transaction not found: {id}")]
    TransactionNotFound {
        /// Transaction ID.
        id: String,
    },

    /// Escrow error.
    #[error("escrow error: {message}")]
    EscrowError {
        /// Description of the escrow error.
        message: String,
    },

    /// Escrow not found.
    #[error("escrow not found: {id}")]
    EscrowNotFound {
        /// Escrow ID.
        id: String,
    },

    /// Escrow already released or refunded.
    #[error("escrow already finalized: {id} is {state}")]
    EscrowFinalized {
        /// Escrow ID.
        id: String,
        /// Current state.
        state: String,
    },

    /// Network error.
    #[error("network error: {message}")]
    NetworkError {
        /// Description of the network error.
        message: String,
    },

    /// RPC error from Solana.
    #[error("RPC error: {message}")]
    RpcError {
        /// RPC error message.
        message: String,
    },

    /// Invalid amount.
    #[error("invalid amount: {message}")]
    InvalidAmount {
        /// Description of the amount error.
        message: String,
    },

    /// Wallet error.
    #[error("wallet error: {message}")]
    WalletError {
        /// Description of the wallet error.
        message: String,
    },

    /// Signing error.
    #[error("signing error: {message}")]
    SigningError {
        /// Description of the signing error.
        message: String,
    },

    /// Timeout waiting for confirmation.
    #[error("timeout: {operation} did not complete in {timeout_secs} seconds")]
    Timeout {
        /// Operation that timed out.
        operation: String,
        /// Timeout duration.
        timeout_secs: u64,
    },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl MoltError {
    /// Create an invalid address error.
    #[must_use]
    pub fn invalid_address(message: impl Into<String>) -> Self {
        Self::InvalidAddress {
            message: message.into(),
        }
    }

    /// Create an insufficient balance error.
    #[must_use]
    pub fn insufficient_balance(have: f64, need: f64) -> Self {
        Self::InsufficientBalance { have, need }
    }

    /// Create a transaction failed error.
    #[must_use]
    pub fn transaction_failed(reason: impl Into<String>) -> Self {
        Self::TransactionFailed {
            reason: reason.into(),
        }
    }

    /// Create a network error.
    #[must_use]
    pub fn network_error(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }

    /// Create an escrow error.
    #[must_use]
    pub fn escrow_error(message: impl Into<String>) -> Self {
        Self::EscrowError {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insufficient_balance_display() {
        let err = MoltError::insufficient_balance(5.0, 10.0);
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn test_invalid_address_display() {
        let err = MoltError::invalid_address("bad format");
        assert!(err.to_string().contains("bad format"));
    }

    #[test]
    fn test_escrow_finalized_display() {
        let err = MoltError::EscrowFinalized {
            id: "abc123".to_string(),
            state: "released".to_string(),
        };
        assert!(err.to_string().contains("abc123"));
        assert!(err.to_string().contains("released"));
    }
}
