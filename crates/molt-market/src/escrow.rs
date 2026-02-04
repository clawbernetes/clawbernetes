//! Escrow management for MOLT payments.
//!
//! Provides secure payment escrow with state machine transitions
//! for job lifecycle management.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::MarketError;

/// Default timeout duration for escrow accounts (7 days).
pub const DEFAULT_TIMEOUT_DAYS: i64 = 7;

/// Custom serde module for `chrono::Duration` (serializes as seconds).
mod chrono_duration_serde {
    use chrono::Duration;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(duration.num_seconds())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = i64::deserialize(deserializer)?;
        Ok(Duration::seconds(seconds))
    }
}

/// The state of an escrow account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscrowState {
    /// Account created but not yet funded.
    Created,
    /// Funds deposited and locked.
    Funded,
    /// Funds released to provider (job completed successfully).
    Released,
    /// Funds returned to buyer (job cancelled or failed).
    Refunded,
    /// Account in dispute resolution.
    Disputed,
    /// Escrow expired due to timeout - funds auto-refunded to buyer.
    /// 
    /// SECURITY: This state is reached when the escrow exceeds its timeout
    /// duration without being resolved. This prevents indefinite fund locking.
    Expired,
}

impl EscrowState {
    /// Checks if a transition to the target state is valid.
    #[must_use] 
    pub const fn can_transition_to(&self, target: &Self) -> bool {
        use EscrowState::{Created, Funded, Released, Refunded, Disputed, Expired};

        matches!(
            (self, target),
            (Created, Funded) 
            | (Funded | Disputed, Released)
            | (Funded | Disputed, Refunded) 
            | (Funded, Disputed)
            // Expired can be reached from Funded or Disputed (timeout during active escrow)
            | (Funded | Disputed, Expired)
        )
    }
}

impl std::fmt::Display for EscrowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "Created"),
            Self::Funded => write!(f, "Funded"),
            Self::Released => write!(f, "Released"),
            Self::Refunded => write!(f, "Refunded"),
            Self::Disputed => write!(f, "Disputed"),
            Self::Expired => write!(f, "Expired"),
        }
    }
}

/// An escrow account holding funds for a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowAccount {
    /// The job this escrow is for.
    pub job_id: String,
    /// The buyer who deposited funds.
    pub buyer: String,
    /// The provider who will receive funds on completion.
    pub provider: String,
    /// Amount held in escrow (in tokens).
    pub amount: u64,
    /// Current state of the escrow.
    pub state: EscrowState,
    /// Who initiated the dispute (if any).
    /// SECURITY: Tracks accountability for dispute resolution.
    pub disputed_by: Option<String>,
    /// Timestamp when the escrow was created.
    /// 
    /// SECURITY: Used for timeout calculation to prevent indefinite fund locking.
    pub created_at: DateTime<Utc>,
    /// Duration after which the escrow expires if not resolved.
    /// 
    /// SECURITY: Configurable timeout (default 7 days) ensures funds cannot
    /// be locked indefinitely if neither party acts.
    #[serde(with = "chrono_duration_serde")]
    pub timeout_duration: Duration,
}

impl EscrowAccount {
    /// Creates a new escrow account in the Created state with default timeout.
    /// 
    /// # Arguments
    /// * `job_id` - The job identifier
    /// * `buyer` - The buyer's address/identifier
    /// * `provider` - The provider's address/identifier  
    /// * `amount` - Amount to be held in escrow
    /// 
    /// # Default Timeout
    /// Uses `DEFAULT_TIMEOUT_DAYS` (7 days) for the timeout duration.
    #[must_use] 
    pub fn new(job_id: String, buyer: String, provider: String, amount: u64) -> Self {
        Self::with_timeout(job_id, buyer, provider, amount, Duration::days(DEFAULT_TIMEOUT_DAYS))
    }

    /// Creates a new escrow account with a custom timeout duration.
    /// 
    /// # Arguments
    /// * `job_id` - The job identifier
    /// * `buyer` - The buyer's address/identifier
    /// * `provider` - The provider's address/identifier
    /// * `amount` - Amount to be held in escrow
    /// * `timeout` - Duration after which the escrow expires
    /// 
    /// # Security
    /// The timeout prevents funds from being locked indefinitely.
    /// After expiration, the escrow can only be finalized via `expire()`.
    #[must_use]
    pub fn with_timeout(
        job_id: String,
        buyer: String,
        provider: String,
        amount: u64,
        timeout: Duration,
    ) -> Self {
        Self {
            job_id,
            buyer,
            provider,
            amount,
            state: EscrowState::Created,
            disputed_by: None,
            created_at: Utc::now(),
            timeout_duration: timeout,
        }
    }

    /// Attempts to transition to a new state.
    fn transition_to(&mut self, target: EscrowState) -> Result<(), MarketError> {
        if self.state.can_transition_to(&target) {
            self.state = target;
            Ok(())
        } else {
            Err(MarketError::InvalidStateTransition {
                from: self.state.to_string(),
                to: target.to_string(),
            })
        }
    }

    /// Checks if the escrow has exceeded its timeout duration.
    /// 
    /// # Returns
    /// `true` if current time exceeds `created_at + timeout_duration`
    /// 
    /// # Security
    /// This check is used to prevent operations on expired escrows
    /// and to determine when automatic refund should occur.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.created_at + self.timeout_duration
    }

    /// Checks if the escrow would be expired at a given time.
    /// 
    /// # Arguments
    /// * `at_time` - The time to check expiration against
    /// 
    /// # Returns
    /// `true` if `at_time` exceeds `created_at + timeout_duration`
    /// 
    /// # Use Case
    /// Useful for testing and for checking future expiration.
    #[must_use]
    pub fn is_expired_at(&self, at_time: DateTime<Utc>) -> bool {
        at_time > self.created_at + self.timeout_duration
    }

    /// Returns the expiration time for this escrow.
    #[must_use]
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.created_at + self.timeout_duration
    }

    /// Expires the escrow, automatically refunding funds to the buyer.
    /// 
    /// # Security
    /// - Can only be called on Funded or Disputed escrows that have timed out
    /// - Automatically transitions to Expired state
    /// - Funds are logically returned to buyer (caller must handle actual transfer)
    /// - Anyone can call this method once timeout is reached (permissionless)
    /// 
    /// # Errors
    /// - Returns `EscrowExpired` if already expired
    /// - Returns `InvalidStateTransition` if not in Funded/Disputed state
    /// - Returns error if timeout has not been reached
    pub fn expire(&mut self) -> Result<(), MarketError> {
        self.expire_at(Utc::now())
    }

    /// Expires the escrow at a specific time (for testing).
    /// 
    /// # Arguments
    /// * `at_time` - The time to use for expiration check
    /// 
    /// # Errors
    /// Same as `expire()`
    pub fn expire_at(&mut self, at_time: DateTime<Utc>) -> Result<(), MarketError> {
        // Already expired?
        if self.state == EscrowState::Expired {
            return Err(MarketError::EscrowExpired {
                job_id: self.job_id.clone(),
                timeout_days: self.timeout_duration.num_days() as u64,
            });
        }

        // Check if timeout reached
        if !self.is_expired_at(at_time) {
            return Err(MarketError::Escrow(
                "escrow has not timed out yet".to_string(),
            ));
        }

        // Transition to Expired (only valid from Funded or Disputed)
        self.transition_to(EscrowState::Expired)
    }

    /// Helper to check if escrow is expired and return error if so.
    fn check_not_expired(&self) -> Result<(), MarketError> {
        if self.state == EscrowState::Expired {
            return Err(MarketError::EscrowExpired {
                job_id: self.job_id.clone(),
                timeout_days: self.timeout_duration.num_days() as u64,
            });
        }
        if self.is_expired() && matches!(self.state, EscrowState::Funded | EscrowState::Disputed) {
            return Err(MarketError::EscrowExpired {
                job_id: self.job_id.clone(),
                timeout_days: self.timeout_duration.num_days() as u64,
            });
        }
        Ok(())
    }

    /// Funds the escrow account (buyer deposits tokens).
    /// 
    /// # Security
    /// Only the buyer can fund the escrow.
    /// 
    /// # Errors
    /// Returns `EscrowExpired` if the escrow has timed out.
    pub fn fund(&mut self, caller: &str) -> Result<(), MarketError> {
        self.check_not_expired()?;
        if caller != self.buyer {
            return Err(MarketError::Unauthorized {
                action: "fund escrow".to_string(),
                required_role: "buyer".to_string(),
                caller_role: if caller == self.provider { "provider" } else { "unknown" }.to_string(),
            });
        }
        self.transition_to(EscrowState::Funded)
    }

    /// Releases funds to the provider (job completed successfully).
    /// 
    /// # Security
    /// Only the buyer can release funds (confirming job completion).
    /// This prevents providers from unilaterally claiming payment.
    /// 
    /// # Errors
    /// Returns `EscrowExpired` if the escrow has timed out.
    pub fn release(&mut self, caller: &str) -> Result<(), MarketError> {
        self.check_not_expired()?;
        if caller != self.buyer {
            return Err(MarketError::Unauthorized {
                action: "release escrow".to_string(),
                required_role: "buyer".to_string(),
                caller_role: if caller == self.provider { "provider" } else { "unknown" }.to_string(),
            });
        }
        self.transition_to(EscrowState::Released)
    }

    /// Refunds the buyer (job cancelled or failed).
    /// 
    /// # Security
    /// Only the provider can initiate a refund (acknowledging failure).
    /// This prevents buyers from unilaterally reclaiming funds.
    /// 
    /// # Errors
    /// Returns `EscrowExpired` if the escrow has timed out.
    pub fn refund(&mut self, caller: &str) -> Result<(), MarketError> {
        self.check_not_expired()?;
        if caller != self.provider {
            return Err(MarketError::Unauthorized {
                action: "refund escrow".to_string(),
                required_role: "provider".to_string(),
                caller_role: if caller == self.buyer { "buyer" } else { "unknown" }.to_string(),
            });
        }
        self.transition_to(EscrowState::Refunded)
    }

    /// Puts the account into dispute resolution.
    /// 
    /// # Security
    /// Either buyer or provider can initiate a dispute.
    /// The initiator is recorded for accountability.
    /// 
    /// # Errors
    /// Returns `EscrowExpired` if the escrow has timed out.
    pub fn dispute(&mut self, caller: &str) -> Result<(), MarketError> {
        self.check_not_expired()?;
        if caller != self.buyer && caller != self.provider {
            return Err(MarketError::Unauthorized {
                action: "dispute escrow".to_string(),
                required_role: "buyer or provider".to_string(),
                caller_role: "unknown".to_string(),
            });
        }
        self.disputed_by = Some(caller.to_string());
        self.transition_to(EscrowState::Disputed)
    }

    /// Returns true if the escrow is in a terminal state.
    /// 
    /// Terminal states are: Released, Refunded, or Expired.
    #[must_use] 
    pub const fn is_finalized(&self) -> bool {
        matches!(self.state, EscrowState::Released | EscrowState::Refunded | EscrowState::Expired)
    }

    /// Returns the timeout duration for this escrow.
    #[must_use]
    pub const fn timeout_duration(&self) -> Duration {
        self.timeout_duration
    }

    /// Returns when this escrow was created.
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escrow_state_transitions() {
        // Created -> Funded -> Released is valid
        assert!(EscrowState::Created.can_transition_to(&EscrowState::Funded));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Released));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Refunded));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Disputed));

        // Invalid transitions
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Released));
    }

    #[test]
    fn escrow_account_creation() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        assert_eq!(account.job_id, "job-123");
        assert_eq!(account.buyer, "buyer-1");
        assert_eq!(account.provider, "provider-1");
        assert_eq!(account.amount, 1000);
        assert_eq!(account.state, EscrowState::Created);
        // Verify timeout fields are set
        assert_eq!(account.timeout_duration.num_days(), DEFAULT_TIMEOUT_DAYS);
        assert!(account.created_at <= Utc::now());
    }

    #[test]
    fn escrow_fund_transition() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let result = account.fund("buyer-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Funded);
    }

    #[test]
    fn escrow_release_after_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        let result = account.release("buyer-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Released);
    }

    #[test]
    fn escrow_cannot_release_before_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let result = account.release("buyer-1");
        assert!(result.is_err());
    }

    #[test]
    fn escrow_refund_after_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        let result = account.refund("provider-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Refunded);
    }

    #[test]
    fn escrow_dispute_after_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        let result = account.dispute("buyer-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Disputed);
    }

    #[test]
    fn escrow_is_finalized() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        assert!(!account.is_finalized());
        account.fund("buyer-1").unwrap();
        assert!(!account.is_finalized());
        account.release("buyer-1").unwrap();
        assert!(account.is_finalized());
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn escrow_state_display() {
        assert_eq!(EscrowState::Created.to_string(), "Created");
        assert_eq!(EscrowState::Funded.to_string(), "Funded");
        assert_eq!(EscrowState::Released.to_string(), "Released");
        assert_eq!(EscrowState::Refunded.to_string(), "Refunded");
        assert_eq!(EscrowState::Disputed.to_string(), "Disputed");
        assert_eq!(EscrowState::Expired.to_string(), "Expired");
    }

    #[test]
    fn escrow_state_clone() {
        let state = EscrowState::Funded;
        let cloned = state;
        assert_eq!(state, cloned);
    }

    #[test]
    fn escrow_state_serialization() {
        let state = EscrowState::Funded;
        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: EscrowState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, deserialized);
    }

    #[test]
    fn escrow_cannot_fund_twice() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        let result = account.fund("buyer-1");
        assert!(result.is_err());
    }

    #[test]
    fn escrow_cannot_refund_before_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let result = account.refund("provider-1");
        assert!(result.is_err());
    }

    #[test]
    fn escrow_cannot_dispute_before_funded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let result = account.dispute("buyer-1");
        assert!(result.is_err());
    }

    #[test]
    fn escrow_release_from_disputed() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        account.dispute("buyer-1").unwrap();
        
        // Can release from disputed state
        let result = account.release("buyer-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Released);
    }

    #[test]
    fn escrow_refund_from_disputed() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        account.dispute("buyer-1").unwrap();
        
        // Can refund from disputed state
        let result = account.refund("provider-1");
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Refunded);
    }

    #[test]
    fn escrow_finalized_after_refund() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        account.refund("provider-1").unwrap();
        assert!(account.is_finalized());
    }

    #[test]
    fn escrow_cannot_transition_from_released() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        account.release("buyer-1").unwrap();
        
        // Cannot do anything from released state
        assert!(account.fund("buyer-1").is_err());
        assert!(account.refund("provider-1").is_err());
        assert!(account.dispute("buyer-1").is_err());
    }

    #[test]
    fn escrow_cannot_transition_from_refunded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund("buyer-1").unwrap();
        account.refund("provider-1").unwrap();
        
        // Cannot do anything from refunded state
        assert!(account.fund("buyer-1").is_err());
        assert!(account.release("buyer-1").is_err());
        assert!(account.dispute("buyer-1").is_err());
    }

    #[test]
    fn escrow_account_serialization() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let json = serde_json::to_string(&account).expect("serialize");
        let deserialized: EscrowAccount = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(account.job_id, deserialized.job_id);
        assert_eq!(account.buyer, deserialized.buyer);
        assert_eq!(account.provider, deserialized.provider);
        assert_eq!(account.amount, deserialized.amount);
        assert_eq!(account.state, deserialized.state);
        // Verify timeout fields survive serialization
        assert_eq!(
            account.timeout_duration.num_seconds(),
            deserialized.timeout_duration.num_seconds()
        );
    }

    #[test]
    fn escrow_account_clone() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let cloned = account.clone();
        assert_eq!(account.job_id, cloned.job_id);
        assert_eq!(account.state, cloned.state);
        assert_eq!(account.created_at, cloned.created_at);
        assert_eq!(account.timeout_duration, cloned.timeout_duration);
    }

    #[test]
    fn escrow_zero_amount() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            0,
        );

        assert_eq!(account.amount, 0);
    }

    #[test]
    fn escrow_large_amount() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            u64::MAX,
        );

        assert_eq!(account.amount, u64::MAX);
    }

    #[test]
    fn escrow_account_debug() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        let debug = format!("{:?}", account);
        assert!(debug.contains("EscrowAccount"));
        assert!(debug.contains("job-123"));
    }

    #[test]
    fn escrow_state_debug() {
        let state = EscrowState::Funded;
        let debug = format!("{:?}", state);
        assert!(debug.contains("Funded"));
    }

    #[test]
    fn escrow_all_valid_transitions() {
        // Test all valid state transitions
        assert!(EscrowState::Created.can_transition_to(&EscrowState::Funded));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Released));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Refunded));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Disputed));
        assert!(EscrowState::Funded.can_transition_to(&EscrowState::Expired));
        assert!(EscrowState::Disputed.can_transition_to(&EscrowState::Released));
        assert!(EscrowState::Disputed.can_transition_to(&EscrowState::Refunded));
        assert!(EscrowState::Disputed.can_transition_to(&EscrowState::Expired));
    }

    #[test]
    fn escrow_all_invalid_transitions() {
        // From Created - can only go to Funded
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Refunded));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Expired));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Created));

        // From Released - terminal state
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Created));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Refunded));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Expired));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Released));

        // From Refunded - terminal state
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Created));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Expired));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Refunded));

        // From Expired - terminal state
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Created));
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Refunded));
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Expired.can_transition_to(&EscrowState::Expired));
    }

    // =========================================================================
    // Authorization Tests (CRIT-01 Security Fix)
    // =========================================================================

    #[test]
    fn fund_requires_buyer_authorization() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        // Provider cannot fund
        let result = account.fund("provider-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::Unauthorized { .. })));

        // Unknown party cannot fund
        let result = account.fund("attacker");
        assert!(result.is_err());

        // Buyer can fund
        let result = account.fund("buyer-1");
        assert!(result.is_ok());
    }

    #[test]
    fn release_requires_buyer_authorization() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        account.fund("buyer-1").unwrap();

        // Provider cannot release (would allow self-payment)
        let result = account.release("provider-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::Unauthorized { .. })));

        // Unknown party cannot release
        let result = account.release("attacker");
        assert!(result.is_err());

        // Buyer can release
        let result = account.release("buyer-1");
        assert!(result.is_ok());
    }

    #[test]
    fn refund_requires_provider_authorization() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        account.fund("buyer-1").unwrap();

        // Buyer cannot refund (would allow taking money back without provider consent)
        let result = account.refund("buyer-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::Unauthorized { .. })));

        // Unknown party cannot refund
        let result = account.refund("attacker");
        assert!(result.is_err());

        // Provider can refund
        let result = account.refund("provider-1");
        assert!(result.is_ok());
    }

    #[test]
    fn dispute_allows_buyer_or_provider() {
        let mut account1 = EscrowAccount::new(
            "job-1".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        account1.fund("buyer-1").unwrap();

        // Unknown party cannot dispute
        let result = account1.dispute("attacker");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::Unauthorized { .. })));

        // Buyer can dispute
        let result = account1.dispute("buyer-1");
        assert!(result.is_ok());
        assert_eq!(account1.disputed_by, Some("buyer-1".to_string()));

        // Provider can also dispute (separate account)
        let mut account2 = EscrowAccount::new(
            "job-2".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        account2.fund("buyer-1").unwrap();
        let result = account2.dispute("provider-1");
        assert!(result.is_ok());
        assert_eq!(account2.disputed_by, Some("provider-1".to_string()));
    }

    #[test]
    fn authorization_error_contains_details() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );
        account.fund("buyer-1").unwrap();

        let result = account.release("provider-1");
        match result {
            Err(MarketError::Unauthorized { action, required_role, caller_role }) => {
                assert_eq!(action, "release escrow");
                assert_eq!(required_role, "buyer");
                assert_eq!(caller_role, "provider");
            }
            _ => panic!("Expected Unauthorized error"),
        }
    }

    // =========================================================================
    // Timeout Tests (LOW-01 Security Fix)
    // =========================================================================

    #[test]
    fn escrow_has_default_timeout() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        assert_eq!(account.timeout_duration.num_days(), DEFAULT_TIMEOUT_DAYS);
    }

    #[test]
    fn escrow_with_custom_timeout() {
        let account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(14),
        );

        assert_eq!(account.timeout_duration.num_days(), 14);
    }

    #[test]
    fn escrow_is_not_expired_when_fresh() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        assert!(!account.is_expired());
    }

    #[test]
    fn escrow_is_expired_after_timeout() {
        let account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(7),
        );

        // Check expiration at a time 8 days in the future
        let future_time = Utc::now() + Duration::days(8);
        assert!(account.is_expired_at(future_time));
    }

    #[test]
    fn escrow_expires_at_returns_correct_time() {
        let account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let expected = account.created_at + account.timeout_duration;
        assert_eq!(account.expires_at(), expected);
    }

    #[test]
    fn escrow_expire_transitions_to_expired_state() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1), // Very short timeout
        );
        account.fund("buyer-1").unwrap();

        // Expire at a time past the timeout
        let future_time = account.created_at + Duration::seconds(2);
        let result = account.expire_at(future_time);
        
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Expired);
        assert!(account.is_finalized());
    }

    #[test]
    fn escrow_expire_from_disputed_state() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();
        account.dispute("buyer-1").unwrap();

        let future_time = account.created_at + Duration::seconds(2);
        let result = account.expire_at(future_time);
        
        assert!(result.is_ok());
        assert_eq!(account.state, EscrowState::Expired);
    }

    #[test]
    fn escrow_cannot_expire_before_timeout() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(7),
        );
        account.fund("buyer-1").unwrap();

        // Try to expire immediately (before timeout)
        let result = account.expire_at(account.created_at + Duration::seconds(1));
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::Escrow(_))));
    }

    #[test]
    fn escrow_cannot_expire_from_created_state() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );

        let future_time = account.created_at + Duration::seconds(2);
        let result = account.expire_at(future_time);
        
        // Should fail because Created -> Expired is not a valid transition
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::InvalidStateTransition { .. })));
    }

    #[test]
    fn escrow_cannot_expire_twice() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();

        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();

        // Try to expire again
        let result = account.expire_at(future_time);
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::EscrowExpired { .. })));
    }

    #[test]
    fn expired_escrow_blocks_release() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();
        
        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();

        let result = account.release("buyer-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::EscrowExpired { .. })));
    }

    #[test]
    fn expired_escrow_blocks_refund() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();
        
        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();

        let result = account.refund("provider-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::EscrowExpired { .. })));
    }

    #[test]
    fn expired_escrow_blocks_dispute() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();
        
        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();

        let result = account.dispute("buyer-1");
        assert!(result.is_err());
        assert!(matches!(result, Err(MarketError::EscrowExpired { .. })));
    }

    #[test]
    fn escrow_expired_error_contains_details() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(7),
        );
        account.fund("buyer-1").unwrap();
        
        let future_time = account.created_at + Duration::days(8);
        account.expire_at(future_time).unwrap();

        let result = account.release("buyer-1");
        match result {
            Err(MarketError::EscrowExpired { job_id, timeout_days }) => {
                assert_eq!(job_id, "job-123");
                assert_eq!(timeout_days, 7);
            }
            _ => panic!("Expected EscrowExpired error"),
        }
    }

    #[test]
    fn escrow_serialization_with_timeout() {
        let account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(14),
        );

        let json = serde_json::to_string(&account).expect("serialize");
        let deserialized: EscrowAccount = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(account.job_id, deserialized.job_id);
        assert_eq!(account.timeout_duration.num_days(), deserialized.timeout_duration.num_days());
        // Note: created_at may have slight precision differences, just check it's close
        assert!(
            (account.created_at - deserialized.created_at).num_seconds().abs() < 1
        );
    }

    #[test]
    fn escrow_finalized_after_expired() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );

        assert!(!account.is_finalized());
        account.fund("buyer-1").unwrap();
        assert!(!account.is_finalized());
        
        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();
        assert!(account.is_finalized());
    }

    #[test]
    fn escrow_cannot_transition_from_expired() {
        let mut account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::seconds(1),
        );
        account.fund("buyer-1").unwrap();
        
        let future_time = account.created_at + Duration::seconds(2);
        account.expire_at(future_time).unwrap();

        // Cannot do anything from expired state
        assert!(account.fund("buyer-1").is_err());
        assert!(account.release("buyer-1").is_err());
        assert!(account.refund("provider-1").is_err());
        assert!(account.dispute("buyer-1").is_err());
    }

    #[test]
    fn escrow_timeout_accessors() {
        let account = EscrowAccount::with_timeout(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
            Duration::days(14),
        );

        assert_eq!(account.timeout_duration().num_days(), 14);
        assert!(account.created_at() <= Utc::now());
    }

    #[test]
    fn expired_state_serialization() {
        let state = EscrowState::Expired;
        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: EscrowState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, deserialized);
    }

    #[test]
    fn expired_state_debug() {
        let state = EscrowState::Expired;
        let debug = format!("{:?}", state);
        assert!(debug.contains("Expired"));
    }
}
