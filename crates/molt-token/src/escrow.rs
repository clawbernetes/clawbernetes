//! Escrow management for MOLT compute jobs.
//!
//! Escrows hold funds during job execution:
//! - Created when buyer requests compute
//! - Released to provider on job completion
//! - Refunded to buyer on job failure/cancellation

use crate::amount::Amount;
use crate::error::{MoltError, Result};
use crate::wallet::Address;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique escrow identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EscrowId(String);

impl EscrowId {
    /// Create a new random escrow ID.
    #[must_use]
    pub fn new() -> Self {
        Self(format!("escrow-{}", Uuid::new_v4()))
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

impl Default for EscrowId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EscrowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Escrow state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowState {
    /// Escrow is being created (funds being transferred).
    Creating,
    /// Escrow is active (funds held).
    Active,
    /// Escrow is being released to provider.
    Releasing,
    /// Escrow released to provider (job completed).
    Released,
    /// Escrow is being refunded to buyer.
    Refunding,
    /// Escrow refunded to buyer (job failed/cancelled).
    Refunded,
    /// Escrow is in dispute.
    Disputed,
    /// Escrow expired (timeout).
    Expired,
}

impl EscrowState {
    /// Check if the escrow is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Released | Self::Refunded | Self::Expired)
    }

    /// Check if the escrow can be released.
    #[must_use]
    pub const fn can_release(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the escrow can be refunded.
    #[must_use]
    pub const fn can_refund(&self) -> bool {
        matches!(self, Self::Active | Self::Disputed)
    }

    /// Check if the escrow can be disputed.
    #[must_use]
    pub const fn can_dispute(&self) -> bool {
        matches!(self, Self::Active | Self::Releasing | Self::Refunding)
    }
}

impl fmt::Display for EscrowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Creating => write!(f, "creating"),
            Self::Active => write!(f, "active"),
            Self::Releasing => write!(f, "releasing"),
            Self::Released => write!(f, "released"),
            Self::Refunding => write!(f, "refunding"),
            Self::Refunded => write!(f, "refunded"),
            Self::Disputed => write!(f, "disputed"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

/// An escrow for MOLT payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escrow {
    /// Unique escrow ID.
    pub id: EscrowId,

    /// Buyer (funds source).
    pub buyer: Address,

    /// Provider (funds destination on release).
    pub provider: Address,

    /// Amount held in escrow.
    pub amount: Amount,

    /// Network fee (percentage taken on release).
    pub fee_rate: f64,

    /// Current state.
    pub state: EscrowState,

    /// Associated job ID.
    pub job_id: String,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,

    /// Expiration timestamp.
    pub expires_at: DateTime<Utc>,

    /// Release transaction signature (when released).
    pub release_signature: Option<String>,

    /// Refund transaction signature (when refunded).
    pub refund_signature: Option<String>,

    /// Dispute reason (if disputed).
    pub dispute_reason: Option<String>,
}

impl Escrow {
    /// Default escrow duration (24 hours).
    pub const DEFAULT_DURATION_HOURS: i64 = 24;

    /// Default network fee rate (5%).
    pub const DEFAULT_FEE_RATE: f64 = 0.05;

    /// Create a new escrow.
    #[must_use]
    pub fn new(
        buyer: Address,
        provider: Address,
        amount: Amount,
        job_id: String,
        duration_hours: Option<i64>,
    ) -> Self {
        let now = Utc::now();
        let duration = Duration::hours(duration_hours.unwrap_or(Self::DEFAULT_DURATION_HOURS));

        Self {
            id: EscrowId::new(),
            buyer,
            provider,
            amount,
            fee_rate: Self::DEFAULT_FEE_RATE,
            state: EscrowState::Creating,
            job_id,
            created_at: now,
            updated_at: now,
            expires_at: now + duration,
            release_signature: None,
            refund_signature: None,
            dispute_reason: None,
        }
    }

    /// Calculate the provider payout (amount - fees).
    #[must_use]
    pub fn provider_payout(&self) -> Amount {
        let fee_lamports = (self.amount.lamports() as f64 * self.fee_rate) as u64;
        Amount::from_lamports(self.amount.lamports().saturating_sub(fee_lamports))
    }

    /// Calculate the network fee.
    #[must_use]
    pub fn network_fee(&self) -> Amount {
        let fee_lamports = (self.amount.lamports() as f64 * self.fee_rate) as u64;
        Amount::from_lamports(fee_lamports)
    }

    /// Check if the escrow has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Mark escrow as active.
    ///
    /// # Errors
    ///
    /// Returns error if escrow is not in creating state.
    pub fn activate(&mut self) -> Result<()> {
        if self.state != EscrowState::Creating {
            return Err(MoltError::EscrowError {
                message: format!("cannot activate escrow in state {}", self.state),
            });
        }
        self.state = EscrowState::Active;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Start releasing the escrow to provider.
    ///
    /// # Errors
    ///
    /// Returns error if escrow cannot be released.
    pub fn start_release(&mut self) -> Result<()> {
        if !self.state.can_release() {
            return Err(MoltError::EscrowFinalized {
                id: self.id.to_string(),
                state: self.state.to_string(),
            });
        }
        self.state = EscrowState::Releasing;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete release to provider.
    ///
    /// # Errors
    ///
    /// Returns error if not in releasing state.
    pub fn complete_release(&mut self, signature: String) -> Result<()> {
        if self.state != EscrowState::Releasing {
            return Err(MoltError::EscrowError {
                message: format!("cannot complete release in state {}", self.state),
            });
        }
        self.state = EscrowState::Released;
        self.release_signature = Some(signature);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Start refunding the escrow to buyer.
    ///
    /// # Errors
    ///
    /// Returns error if escrow cannot be refunded.
    pub fn start_refund(&mut self) -> Result<()> {
        if !self.state.can_refund() {
            return Err(MoltError::EscrowFinalized {
                id: self.id.to_string(),
                state: self.state.to_string(),
            });
        }
        self.state = EscrowState::Refunding;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Complete refund to buyer.
    ///
    /// # Errors
    ///
    /// Returns error if not in refunding state.
    pub fn complete_refund(&mut self, signature: String) -> Result<()> {
        if self.state != EscrowState::Refunding {
            return Err(MoltError::EscrowError {
                message: format!("cannot complete refund in state {}", self.state),
            });
        }
        self.state = EscrowState::Refunded;
        self.refund_signature = Some(signature);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Open a dispute.
    ///
    /// # Errors
    ///
    /// Returns error if escrow cannot be disputed.
    pub fn dispute(&mut self, reason: String) -> Result<()> {
        if !self.state.can_dispute() {
            return Err(MoltError::EscrowError {
                message: format!("cannot dispute escrow in state {}", self.state),
            });
        }
        self.state = EscrowState::Disputed;
        self.dispute_reason = Some(reason);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark as expired.
    pub fn mark_expired(&mut self) {
        self.state = EscrowState::Expired;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::Wallet;

    fn test_addresses() -> (Address, Address) {
        let buyer = Wallet::generate().expect("wallet 1").address().clone();
        let provider = Wallet::generate().expect("wallet 2").address().clone();
        (buyer, provider)
    }

    #[test]
    fn test_escrow_id_unique() {
        let id1 = EscrowId::new();
        let id2 = EscrowId::new();
        assert_ne!(id1, id2);
        assert!(id1.as_str().starts_with("escrow-"));
    }

    #[test]
    fn test_escrow_creation() {
        let (buyer, provider) = test_addresses();
        let escrow = Escrow::new(
            buyer.clone(),
            provider.clone(),
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        assert_eq!(escrow.buyer, buyer);
        assert_eq!(escrow.provider, provider);
        assert_eq!(escrow.amount, Amount::molt(10.0));
        assert_eq!(escrow.state, EscrowState::Creating);
    }

    #[test]
    fn test_escrow_fee_calculation() {
        let (buyer, provider) = test_addresses();
        let escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(100.0),
            "job-123".to_string(),
            None,
        );

        let payout = escrow.provider_payout();
        let fee = escrow.network_fee();

        // 5% fee
        assert!((fee.as_molt() - 5.0).abs() < 0.001);
        assert!((payout.as_molt() - 95.0).abs() < 0.001);
    }

    #[test]
    fn test_escrow_lifecycle_release() {
        let (buyer, provider) = test_addresses();
        let mut escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        // Creating -> Active
        escrow.activate().expect("should activate");
        assert_eq!(escrow.state, EscrowState::Active);

        // Active -> Releasing
        escrow.start_release().expect("should start release");
        assert_eq!(escrow.state, EscrowState::Releasing);

        // Releasing -> Released
        escrow.complete_release("sig123".to_string()).expect("should complete");
        assert_eq!(escrow.state, EscrowState::Released);
        assert!(escrow.state.is_terminal());
    }

    #[test]
    fn test_escrow_lifecycle_refund() {
        let (buyer, provider) = test_addresses();
        let mut escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        escrow.activate().expect("should activate");

        // Active -> Refunding
        escrow.start_refund().expect("should start refund");
        assert_eq!(escrow.state, EscrowState::Refunding);

        // Refunding -> Refunded
        escrow.complete_refund("sig456".to_string()).expect("should complete");
        assert_eq!(escrow.state, EscrowState::Refunded);
        assert!(escrow.state.is_terminal());
    }

    #[test]
    fn test_escrow_dispute() {
        let (buyer, provider) = test_addresses();
        let mut escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        escrow.activate().expect("should activate");
        escrow.dispute("provider didn't complete job".to_string()).expect("should dispute");

        assert_eq!(escrow.state, EscrowState::Disputed);
        assert_eq!(escrow.dispute_reason, Some("provider didn't complete job".to_string()));
    }

    #[test]
    fn test_cannot_release_finalized() {
        let (buyer, provider) = test_addresses();
        let mut escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        escrow.activate().unwrap();
        escrow.start_refund().unwrap();
        escrow.complete_refund("sig".to_string()).unwrap();

        // Cannot release after refund
        let result = escrow.start_release();
        assert!(result.is_err());
    }

    #[test]
    fn test_escrow_serialization() {
        let (buyer, provider) = test_addresses();
        let escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(10.0),
            "job-123".to_string(),
            None,
        );

        let json = serde_json::to_string(&escrow).expect("serialize");
        let parsed: Escrow = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(escrow.id, parsed.id);
        assert_eq!(escrow.amount, parsed.amount);
    }
}
