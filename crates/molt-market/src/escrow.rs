//! Escrow management for MOLT payments.
//!
//! Provides secure payment escrow with state machine transitions
//! for job lifecycle management.

use serde::{Deserialize, Serialize};

use crate::error::MarketError;

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
}

impl EscrowState {
    /// Checks if a transition to the target state is valid.
    #[must_use] 
    pub const fn can_transition_to(&self, target: &Self) -> bool {
        use EscrowState::{Created, Funded, Released, Refunded, Disputed};

        matches!(
            (self, target),
            (Created, Funded) | (Funded | Disputed, Released) |
(Funded | Disputed, Refunded) | (Funded, Disputed)
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
}

impl EscrowAccount {
    /// Creates a new escrow account in the Created state.
    #[must_use] 
    pub const fn new(job_id: String, buyer: String, provider: String, amount: u64) -> Self {
        Self {
            job_id,
            buyer,
            provider,
            amount,
            state: EscrowState::Created,
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

    /// Funds the escrow account (buyer deposits tokens).
    pub fn fund(&mut self) -> Result<(), MarketError> {
        self.transition_to(EscrowState::Funded)
    }

    /// Releases funds to the provider (job completed successfully).
    pub fn release(&mut self) -> Result<(), MarketError> {
        self.transition_to(EscrowState::Released)
    }

    /// Refunds the buyer (job cancelled or failed).
    pub fn refund(&mut self) -> Result<(), MarketError> {
        self.transition_to(EscrowState::Refunded)
    }

    /// Puts the account into dispute resolution.
    pub fn dispute(&mut self) -> Result<(), MarketError> {
        self.transition_to(EscrowState::Disputed)
    }

    /// Returns true if the escrow is in a terminal state.
    #[must_use] 
    pub const fn is_finalized(&self) -> bool {
        matches!(self.state, EscrowState::Released | EscrowState::Refunded)
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
    }

    #[test]
    fn escrow_fund_transition() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        let result = account.fund();
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

        account.fund().unwrap();
        let result = account.release();
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

        let result = account.release();
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

        account.fund().unwrap();
        let result = account.refund();
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

        account.fund().unwrap();
        let result = account.dispute();
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
        account.fund().unwrap();
        assert!(!account.is_finalized());
        account.release().unwrap();
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

        account.fund().unwrap();
        let result = account.fund();
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

        let result = account.refund();
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

        let result = account.dispute();
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

        account.fund().unwrap();
        account.dispute().unwrap();
        
        // Can release from disputed state
        let result = account.release();
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

        account.fund().unwrap();
        account.dispute().unwrap();
        
        // Can refund from disputed state
        let result = account.refund();
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

        account.fund().unwrap();
        account.refund().unwrap();
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

        account.fund().unwrap();
        account.release().unwrap();
        
        // Cannot do anything from released state
        assert!(account.fund().is_err());
        assert!(account.refund().is_err());
        assert!(account.dispute().is_err());
    }

    #[test]
    fn escrow_cannot_transition_from_refunded() {
        let mut account = EscrowAccount::new(
            "job-123".to_string(),
            "buyer-1".to_string(),
            "provider-1".to_string(),
            1000,
        );

        account.fund().unwrap();
        account.refund().unwrap();
        
        // Cannot do anything from refunded state
        assert!(account.fund().is_err());
        assert!(account.release().is_err());
        assert!(account.dispute().is_err());
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
        assert!(EscrowState::Disputed.can_transition_to(&EscrowState::Released));
        assert!(EscrowState::Disputed.can_transition_to(&EscrowState::Refunded));
    }

    #[test]
    fn escrow_all_invalid_transitions() {
        // From Created - can only go to Funded
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Refunded));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Created.can_transition_to(&EscrowState::Created));

        // From Released - terminal state
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Created));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Refunded));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Released.can_transition_to(&EscrowState::Released));

        // From Refunded - terminal state
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Created));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Funded));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Released));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Disputed));
        assert!(!EscrowState::Refunded.can_transition_to(&EscrowState::Refunded));
    }
}
