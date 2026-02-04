//! Escrow management for MOLT compute jobs.
//!
//! Escrows hold funds during job execution:
//! - Created when buyer requests compute
//! - Released to provider on job completion
//! - Refunded to buyer on job failure/cancellation
//!
//! ## Fee Calculation Precision
//!
//! Fee rates are stored as **basis points** (1 bp = 0.01%) for precision:
//! - 500 bps = 5% fee
//! - 100 bps = 1% fee
//! - 1 bp = 0.01% fee
//!
//! All fee calculations use integer arithmetic to ensure:
//! - No floating-point precision loss
//! - Deterministic results across platforms
//! - Overflow protection via checked arithmetic
//!
//! ### Precision Guarantees
//!
//! For any valid amount and fee rate:
//! - `fee + payout <= amount` (never overpay)
//! - `fee = floor(amount * fee_rate_bps / 10_000)` (fees round down)
//! - No precision loss for amounts up to `u64::MAX`

use crate::amount::Amount;
use crate::error::{MoltError, Result};
use crate::wallet::Address;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Maximum fee rate in basis points (100% = 10,000 bps).
/// Fees above this are rejected to prevent configuration errors.
pub const MAX_FEE_RATE_BPS: u32 = 10_000;

/// Basis points per percent (100 bps = 1%).
pub const BPS_PER_PERCENT: u32 = 100;

/// Calculate fee in lamports using integer arithmetic.
///
/// Formula: `fee = floor(amount_lamports * fee_rate_bps / 10_000)`
///
/// Uses 128-bit intermediate multiplication to prevent overflow,
/// then safely converts back to u64 (guaranteed to fit since
/// fee_rate_bps <= 10_000 means fee <= amount).
///
/// # Arguments
///
/// * `amount_lamports` - The base amount in lamports
/// * `fee_rate_bps` - Fee rate in basis points (1 bp = 0.01%)
///
/// # Returns
///
/// The calculated fee in lamports, using floor division (rounds down).
///
/// # Precision Guarantees
///
/// - No precision loss: exact integer arithmetic
/// - Overflow-safe: u128 intermediate handles u64::MAX * 10_000
/// - Deterministic: same inputs always produce same output
#[must_use]
pub const fn calculate_fee_lamports(amount_lamports: u64, fee_rate_bps: u32) -> u64 {
    // Use u128 to prevent overflow: u64::MAX * 10_000 fits in u128
    let amount_wide = amount_lamports as u128;
    let rate_wide = fee_rate_bps as u128;
    
    // fee = floor(amount * rate / 10_000)
    // Since rate <= 10_000, result <= amount, so it fits in u64
    let fee_wide = amount_wide * rate_wide / (MAX_FEE_RATE_BPS as u128);
    
    // Safe cast: fee <= amount (which is u64), so this can't overflow
    fee_wide as u64
}

/// Calculate fee and payout together, ensuring they sum correctly.
///
/// Returns `(fee, payout)` where `fee + payout == amount` for valid rates,
/// or `fee + payout <= amount` in edge cases.
///
/// # Arguments
///
/// * `amount_lamports` - The base amount in lamports
/// * `fee_rate_bps` - Fee rate in basis points (1 bp = 0.01%)
///
/// # Returns
///
/// Tuple of `(fee_lamports, payout_lamports)`.
#[must_use]
pub const fn calculate_fee_and_payout(amount_lamports: u64, fee_rate_bps: u32) -> (u64, u64) {
    let fee = calculate_fee_lamports(amount_lamports, fee_rate_bps);
    // Saturating sub is defensive; shouldn't be needed since fee <= amount
    let payout = amount_lamports.saturating_sub(fee);
    (fee, payout)
}

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

    /// Network fee rate in basis points (1 bp = 0.01%).
    /// 
    /// Examples:
    /// - 500 bps = 5% fee
    /// - 100 bps = 1% fee
    /// - 50 bps = 0.5% fee
    pub fee_rate_bps: u32,

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

    /// Default network fee rate in basis points (500 bps = 5%).
    pub const DEFAULT_FEE_RATE_BPS: u32 = 500;

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
            fee_rate_bps: Self::DEFAULT_FEE_RATE_BPS,
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

    /// Create a new escrow with a custom fee rate.
    ///
    /// # Arguments
    ///
    /// * `fee_rate_bps` - Fee rate in basis points (1 bp = 0.01%, max 10,000 = 100%)
    ///
    /// # Returns
    ///
    /// Returns `None` if fee rate exceeds `MAX_FEE_RATE_BPS` (10,000).
    #[must_use]
    pub fn with_fee_rate(
        buyer: Address,
        provider: Address,
        amount: Amount,
        job_id: String,
        duration_hours: Option<i64>,
        fee_rate_bps: u32,
    ) -> Option<Self> {
        if fee_rate_bps > MAX_FEE_RATE_BPS {
            return None;
        }
        
        let mut escrow = Self::new(buyer, provider, amount, job_id, duration_hours);
        escrow.fee_rate_bps = fee_rate_bps;
        Some(escrow)
    }

    /// Calculate the network fee using integer arithmetic.
    ///
    /// Formula: `fee = floor(amount * fee_rate_bps / 10_000)`
    ///
    /// Uses 128-bit intermediate to prevent overflow for large amounts.
    ///
    /// # Precision Guarantees
    ///
    /// - Fees always round down (floor division)
    /// - No precision loss for any valid u64 amount
    /// - Overflow-safe for all inputs
    #[must_use]
    pub fn network_fee(&self) -> Amount {
        let fee_lamports = calculate_fee_lamports(self.amount.lamports(), self.fee_rate_bps);
        Amount::from_lamports(fee_lamports)
    }

    /// Calculate the provider payout (amount - fees).
    ///
    /// # Precision Guarantees
    ///
    /// - `payout = amount - fee`
    /// - `payout + fee <= amount` (never overpay)
    /// - Uses saturating subtraction (can't underflow)
    #[must_use]
    pub fn provider_payout(&self) -> Amount {
        let fee_lamports = calculate_fee_lamports(self.amount.lamports(), self.fee_rate_bps);
        Amount::from_lamports(self.amount.lamports().saturating_sub(fee_lamports))
    }

    /// Get the fee rate as a percentage (for display purposes only).
    ///
    /// Note: Internal calculations always use basis points to avoid precision loss.
    #[must_use]
    pub fn fee_rate_percent(&self) -> f64 {
        f64::from(self.fee_rate_bps) / f64::from(BPS_PER_PERCENT)
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
    use crate::LAMPORTS_PER_MOLT;

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
        assert_eq!(escrow.fee_rate_bps, Escrow::DEFAULT_FEE_RATE_BPS);
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

        // 5% fee (500 bps)
        assert!((fee.as_molt() - 5.0).abs() < 0.001);
        assert!((payout.as_molt() - 95.0).abs() < 0.001);
        
        // Verify fee + payout == amount (no loss)
        assert_eq!(fee.lamports() + payout.lamports(), escrow.amount.lamports());
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

        escrow.activate().expect("activate");
        escrow.start_refund().expect("start refund");
        escrow.complete_refund("sig".to_string()).expect("complete refund");

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
        assert_eq!(escrow.fee_rate_bps, parsed.fee_rate_bps);
    }

    // =========================================================================
    // Fee Calculation Precision Tests (MED-11 Fix)
    // =========================================================================

    #[test]
    fn test_calculate_fee_lamports_basic() {
        // 5% of 100 MOLT = 5 MOLT
        let amount = 100 * LAMPORTS_PER_MOLT;
        let fee = calculate_fee_lamports(amount, 500); // 500 bps = 5%
        assert_eq!(fee, 5 * LAMPORTS_PER_MOLT);
    }

    #[test]
    fn test_calculate_fee_lamports_zero_amount() {
        let fee = calculate_fee_lamports(0, 500);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_calculate_fee_lamports_zero_rate() {
        let fee = calculate_fee_lamports(1_000_000, 0);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_calculate_fee_lamports_full_rate() {
        // 100% fee = 10,000 bps
        let amount = 1_000_000u64;
        let fee = calculate_fee_lamports(amount, MAX_FEE_RATE_BPS);
        assert_eq!(fee, amount);
    }

    #[test]
    fn test_calculate_fee_lamports_very_small_amount() {
        // 1 lamport with 5% fee -> 0 (rounds down)
        let fee = calculate_fee_lamports(1, 500);
        assert_eq!(fee, 0);
        
        // 199 lamports with 5% fee -> 9 (floor(199 * 500 / 10000) = 9)
        let fee = calculate_fee_lamports(199, 500);
        assert_eq!(fee, 9);
        
        // 200 lamports with 5% fee -> 10 (exact)
        let fee = calculate_fee_lamports(200, 500);
        assert_eq!(fee, 10);
    }

    #[test]
    fn test_calculate_fee_lamports_very_large_amount() {
        // Test with u64::MAX - should not overflow
        let fee = calculate_fee_lamports(u64::MAX, 500); // 5%
        // 5% of u64::MAX should be approximately u64::MAX / 20
        assert!(fee > 0);
        assert!(fee <= u64::MAX / 20 + 1);
        
        // Verify it doesn't panic or overflow with max rate
        let max_fee = calculate_fee_lamports(u64::MAX, MAX_FEE_RATE_BPS);
        assert_eq!(max_fee, u64::MAX); // 100% fee = full amount
    }

    #[test]
    fn test_calculate_fee_and_payout_invariant() {
        // fee + payout should always equal amount for any inputs
        let test_cases = [
            (0u64, 0u32),
            (0, 500),
            (1, 500),
            (100, 500),
            (LAMPORTS_PER_MOLT, 500),
            (100 * LAMPORTS_PER_MOLT, 500),
            (u64::MAX, 500),
            (1_000_000, 0),
            (1_000_000, 1),
            (1_000_000, 10_000),
        ];
        
        for (amount, rate) in test_cases {
            let (fee, payout) = calculate_fee_and_payout(amount, rate);
            assert_eq!(
                fee.saturating_add(payout), amount,
                "fee + payout != amount for ({}, {}): {} + {} != {}",
                amount, rate, fee, payout, amount
            );
        }
    }

    #[test]
    fn test_fee_precision_no_floating_point_loss() {
        // This test verifies that our integer arithmetic produces
        // the same or better results than floating point would
        
        let (buyer, provider) = test_addresses();
        
        // Test edge case that would have precision issues with f64
        // Amount that when multiplied by 0.05 gives a non-integer
        let amount_lamports = 33u64; // 33 * 0.05 = 1.65
        let escrow = Escrow::with_fee_rate(
            buyer,
            provider,
            Amount::from_lamports(amount_lamports),
            "job-1".to_string(),
            None,
            500, // 5%
        ).expect("valid fee rate");
        
        let fee = escrow.network_fee();
        let payout = escrow.provider_payout();
        
        // Integer math: floor(33 * 500 / 10000) = floor(1.65) = 1
        assert_eq!(fee.lamports(), 1);
        assert_eq!(payout.lamports(), 32);
        
        // Verify no loss: fee + payout == original amount
        assert_eq!(fee.lamports() + payout.lamports(), amount_lamports);
    }

    #[test]
    fn test_with_fee_rate_valid() {
        let (buyer, provider) = test_addresses();
        
        let escrow = Escrow::with_fee_rate(
            buyer.clone(),
            provider.clone(),
            Amount::molt(100.0),
            "job-1".to_string(),
            None,
            100, // 1%
        );
        assert!(escrow.is_some());
        let escrow = escrow.expect("valid");
        assert_eq!(escrow.fee_rate_bps, 100);
        
        // Verify 1% fee
        let fee = escrow.network_fee();
        assert_eq!(fee.lamports(), LAMPORTS_PER_MOLT); // 1% of 100 = 1
    }

    #[test]
    fn test_with_fee_rate_max() {
        let (buyer, provider) = test_addresses();
        
        // 100% fee is valid (though unusual)
        let escrow = Escrow::with_fee_rate(
            buyer,
            provider,
            Amount::molt(100.0),
            "job-1".to_string(),
            None,
            10_000, // 100%
        );
        assert!(escrow.is_some());
        let escrow = escrow.expect("valid");
        
        // 100% fee means provider gets nothing
        assert_eq!(escrow.network_fee().lamports(), escrow.amount.lamports());
        assert_eq!(escrow.provider_payout().lamports(), 0);
    }

    #[test]
    fn test_with_fee_rate_invalid() {
        let (buyer, provider) = test_addresses();
        
        // Over 100% is invalid
        let escrow = Escrow::with_fee_rate(
            buyer,
            provider,
            Amount::molt(100.0),
            "job-1".to_string(),
            None,
            10_001, // 100.01% - invalid
        );
        assert!(escrow.is_none());
    }

    #[test]
    fn test_fee_rate_percent_conversion() {
        let (buyer, provider) = test_addresses();
        let escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(100.0),
            "job-1".to_string(),
            None,
        );
        
        // 500 bps = 5.0%
        let percent = escrow.fee_rate_percent();
        assert!((percent - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fee_calculation_various_rates() {
        let test_cases = [
            (100 * LAMPORTS_PER_MOLT, 500, 5 * LAMPORTS_PER_MOLT),   // 5%
            (100 * LAMPORTS_PER_MOLT, 100, LAMPORTS_PER_MOLT),       // 1%
            (100 * LAMPORTS_PER_MOLT, 250, LAMPORTS_PER_MOLT * 5/2), // 2.5%
            (100 * LAMPORTS_PER_MOLT, 1, LAMPORTS_PER_MOLT / 100),   // 0.01%
            (1000, 500, 50),                                          // 5% of 1000
            (999, 500, 49),                                           // 5% of 999 = 49.95 -> 49
        ];
        
        for (amount, rate, expected_fee) in test_cases {
            let fee = calculate_fee_lamports(amount, rate);
            assert_eq!(
                fee, expected_fee,
                "Fee mismatch for amount={}, rate={}: got {}, expected {}",
                amount, rate, fee, expected_fee
            );
        }
    }

    #[test]
    fn test_basis_points_constants() {
        assert_eq!(MAX_FEE_RATE_BPS, 10_000); // 100%
        assert_eq!(BPS_PER_PERCENT, 100);     // 100 bps = 1%
        assert_eq!(Escrow::DEFAULT_FEE_RATE_BPS, 500); // 5%
    }

    #[test]
    fn test_fee_floor_division() {
        // Verify that fees always round down (floor), favoring the user
        // This is important for security - we never take more than calculated
        
        // 1 lamport at 1% should give 0 fee (not rounded up to 1)
        assert_eq!(calculate_fee_lamports(1, 100), 0);
        
        // 99 lamports at 1% should give 0 fee (99 * 100 / 10000 = 0.99 -> 0)
        assert_eq!(calculate_fee_lamports(99, 100), 0);
        
        // 100 lamports at 1% should give exactly 1 fee
        assert_eq!(calculate_fee_lamports(100, 100), 1);
        
        // 101 lamports at 1% should still give 1 fee (1.01 -> 1)
        assert_eq!(calculate_fee_lamports(101, 100), 1);
    }

    #[test]
    fn test_escrow_default_fee_rate() {
        let (buyer, provider) = test_addresses();
        let escrow = Escrow::new(
            buyer,
            provider,
            Amount::molt(100.0),
            "job-1".to_string(),
            None,
        );
        
        // Default should be 5% (500 bps)
        assert_eq!(escrow.fee_rate_bps, 500);
        
        // Verify calculation matches
        let fee = escrow.network_fee();
        let expected_fee = Amount::molt(5.0);
        assert_eq!(fee.lamports(), expected_fee.lamports());
    }
}
