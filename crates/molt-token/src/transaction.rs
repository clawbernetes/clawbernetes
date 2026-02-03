//! Transaction types for MOLT token operations.

use crate::amount::Amount;
use crate::wallet::Address;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique transaction identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(String);

impl TransactionId {
    /// Create a new random transaction ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from a string.
    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the ID as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transaction status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    /// Transaction is pending (not yet submitted).
    Pending,
    /// Transaction has been submitted to the network.
    Submitted,
    /// Transaction is being processed.
    Processing,
    /// Transaction confirmed on-chain.
    Confirmed,
    /// Transaction finalized (irreversible).
    Finalized,
    /// Transaction failed.
    Failed,
}

impl TransactionStatus {
    /// Check if the transaction is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Confirmed | Self::Finalized | Self::Failed)
    }

    /// Check if the transaction succeeded.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Confirmed | Self::Finalized)
    }
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Submitted => write!(f, "submitted"),
            Self::Processing => write!(f, "processing"),
            Self::Confirmed => write!(f, "confirmed"),
            Self::Finalized => write!(f, "finalized"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Type of transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionType {
    /// Transfer tokens between accounts.
    Transfer,
    /// Create an escrow.
    EscrowCreate,
    /// Release escrowed funds.
    EscrowRelease,
    /// Refund escrowed funds.
    EscrowRefund,
    /// Deposit tokens (from external).
    Deposit,
    /// Withdraw tokens (to external).
    Withdraw,
}

impl fmt::Display for TransactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transfer => write!(f, "transfer"),
            Self::EscrowCreate => write!(f, "escrow_create"),
            Self::EscrowRelease => write!(f, "escrow_release"),
            Self::EscrowRefund => write!(f, "escrow_refund"),
            Self::Deposit => write!(f, "deposit"),
            Self::Withdraw => write!(f, "withdraw"),
        }
    }
}

/// A MOLT token transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique transaction ID.
    pub id: TransactionId,

    /// Transaction type.
    pub tx_type: TransactionType,

    /// Source address.
    pub from: Address,

    /// Destination address (if applicable).
    pub to: Option<Address>,

    /// Amount transferred.
    pub amount: Amount,

    /// Transaction status.
    pub status: TransactionStatus,

    /// Solana signature (when submitted).
    pub signature: Option<String>,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,

    /// Error message (if failed).
    pub error: Option<String>,

    /// Associated escrow ID (if applicable).
    pub escrow_id: Option<String>,
}

impl Transaction {
    /// Create a new transfer transaction.
    #[must_use]
    pub fn transfer(from: Address, to: Address, amount: Amount) -> Self {
        let now = Utc::now();
        Self {
            id: TransactionId::new(),
            tx_type: TransactionType::Transfer,
            from,
            to: Some(to),
            amount,
            status: TransactionStatus::Pending,
            signature: None,
            created_at: now,
            updated_at: now,
            error: None,
            escrow_id: None,
        }
    }

    /// Create a new escrow creation transaction.
    #[must_use]
    pub fn escrow_create(from: Address, amount: Amount, escrow_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: TransactionId::new(),
            tx_type: TransactionType::EscrowCreate,
            from,
            to: None,
            amount,
            status: TransactionStatus::Pending,
            signature: None,
            created_at: now,
            updated_at: now,
            error: None,
            escrow_id: Some(escrow_id),
        }
    }

    /// Create a new escrow release transaction.
    #[must_use]
    pub fn escrow_release(escrow_address: Address, to: Address, amount: Amount, escrow_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: TransactionId::new(),
            tx_type: TransactionType::EscrowRelease,
            from: escrow_address,
            to: Some(to),
            amount,
            status: TransactionStatus::Pending,
            signature: None,
            created_at: now,
            updated_at: now,
            error: None,
            escrow_id: Some(escrow_id),
        }
    }

    /// Mark transaction as submitted.
    pub fn mark_submitted(&mut self, signature: String) {
        self.status = TransactionStatus::Submitted;
        self.signature = Some(signature);
        self.updated_at = Utc::now();
    }

    /// Mark transaction as confirmed.
    pub fn mark_confirmed(&mut self) {
        self.status = TransactionStatus::Confirmed;
        self.updated_at = Utc::now();
    }

    /// Mark transaction as finalized.
    pub fn mark_finalized(&mut self) {
        self.status = TransactionStatus::Finalized;
        self.updated_at = Utc::now();
    }

    /// Mark transaction as failed.
    pub fn mark_failed(&mut self, error: String) {
        self.status = TransactionStatus::Failed;
        self.error = Some(error);
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::Wallet;

    fn test_addresses() -> (Address, Address) {
        let w1 = Wallet::generate().expect("generate wallet 1");
        let w2 = Wallet::generate().expect("generate wallet 2");
        (w1.address().clone(), w2.address().clone())
    }

    #[test]
    fn test_transaction_id_unique() {
        let id1 = TransactionId::new();
        let id2 = TransactionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_transaction_status_terminal() {
        assert!(!TransactionStatus::Pending.is_terminal());
        assert!(!TransactionStatus::Submitted.is_terminal());
        assert!(TransactionStatus::Confirmed.is_terminal());
        assert!(TransactionStatus::Finalized.is_terminal());
        assert!(TransactionStatus::Failed.is_terminal());
    }

    #[test]
    fn test_transaction_status_success() {
        assert!(!TransactionStatus::Pending.is_success());
        assert!(!TransactionStatus::Failed.is_success());
        assert!(TransactionStatus::Confirmed.is_success());
        assert!(TransactionStatus::Finalized.is_success());
    }

    #[test]
    fn test_transfer_transaction() {
        let (from, to) = test_addresses();
        let amount = Amount::molt(10.0);
        
        let tx = Transaction::transfer(from.clone(), to.clone(), amount);
        
        assert_eq!(tx.tx_type, TransactionType::Transfer);
        assert_eq!(tx.from, from);
        assert_eq!(tx.to, Some(to));
        assert_eq!(tx.amount, amount);
        assert_eq!(tx.status, TransactionStatus::Pending);
    }

    #[test]
    fn test_transaction_state_transitions() {
        let (from, to) = test_addresses();
        let mut tx = Transaction::transfer(from, to, Amount::molt(5.0));

        assert_eq!(tx.status, TransactionStatus::Pending);

        tx.mark_submitted("sig123".to_string());
        assert_eq!(tx.status, TransactionStatus::Submitted);
        assert_eq!(tx.signature, Some("sig123".to_string()));

        tx.mark_confirmed();
        assert_eq!(tx.status, TransactionStatus::Confirmed);

        tx.mark_finalized();
        assert_eq!(tx.status, TransactionStatus::Finalized);
    }

    #[test]
    fn test_transaction_failure() {
        let (from, to) = test_addresses();
        let mut tx = Transaction::transfer(from, to, Amount::molt(5.0));

        tx.mark_failed("insufficient funds".to_string());
        assert_eq!(tx.status, TransactionStatus::Failed);
        assert_eq!(tx.error, Some("insufficient funds".to_string()));
    }

    #[test]
    fn test_transaction_serialization() {
        let (from, to) = test_addresses();
        let tx = Transaction::transfer(from, to, Amount::molt(5.0));

        let json = serde_json::to_string(&tx).expect("serialize");
        let parsed: Transaction = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(tx.id, parsed.id);
        assert_eq!(tx.amount, parsed.amount);
    }
}
