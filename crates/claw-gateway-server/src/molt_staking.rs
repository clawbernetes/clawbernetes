//! MOLT staking for compute providers.
//!
//! Providers stake MOLT tokens to:
//! - Demonstrate commitment and skin-in-the-game
//! - Be eligible for job assignments
//! - Earn staking rewards
//!
//! ## Staking Tiers
//!
//! | Tier     | Min Stake    | Max Jobs | Priority |
//! |----------|--------------|----------|----------|
//! | Bronze   | 100 MOLT     | 10       | Low      |
//! | Silver   | 1,000 MOLT   | 50       | Medium   |
//! | Gold     | 10,000 MOLT  | 200      | High     |
//! | Platinum | 100,000 MOLT | Unlimited| Highest  |

use chrono::{DateTime, Utc};
use molt_token::Amount;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Staking tier based on amount staked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingTier {
    /// No stake - cannot provide compute.
    None,
    /// Bronze tier: 100+ MOLT.
    Bronze,
    /// Silver tier: 1,000+ MOLT.
    Silver,
    /// Gold tier: 10,000+ MOLT.
    Gold,
    /// Platinum tier: 100,000+ MOLT.
    Platinum,
}

impl StakingTier {
    /// Minimum stake for Bronze tier (100 MOLT).
    pub const BRONZE_MIN: u64 = 100_000_000_000; // 100 MOLT in lamports

    /// Minimum stake for Silver tier (1,000 MOLT).
    pub const SILVER_MIN: u64 = 1_000_000_000_000; // 1,000 MOLT

    /// Minimum stake for Gold tier (10,000 MOLT).
    pub const GOLD_MIN: u64 = 10_000_000_000_000; // 10,000 MOLT

    /// Minimum stake for Platinum tier (100,000 MOLT).
    pub const PLATINUM_MIN: u64 = 100_000_000_000_000; // 100,000 MOLT

    /// Determine tier from staked amount.
    #[must_use]
    pub fn from_amount(lamports: u64) -> Self {
        if lamports >= Self::PLATINUM_MIN {
            Self::Platinum
        } else if lamports >= Self::GOLD_MIN {
            Self::Gold
        } else if lamports >= Self::SILVER_MIN {
            Self::Silver
        } else if lamports >= Self::BRONZE_MIN {
            Self::Bronze
        } else {
            Self::None
        }
    }

    /// Get the maximum concurrent jobs for this tier.
    #[must_use]
    pub const fn max_concurrent_jobs(&self) -> Option<usize> {
        match self {
            Self::None => Some(0),
            Self::Bronze => Some(10),
            Self::Silver => Some(50),
            Self::Gold => Some(200),
            Self::Platinum => None, // Unlimited
        }
    }

    /// Get the priority multiplier for job assignment.
    #[must_use]
    pub const fn priority_multiplier(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::Bronze => 1,
            Self::Silver => 2,
            Self::Gold => 4,
            Self::Platinum => 8,
        }
    }

    /// Check if this tier allows providing compute.
    #[must_use]
    pub const fn can_provide(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Get the minimum stake for this tier.
    #[must_use]
    pub const fn min_stake(&self) -> u64 {
        match self {
            Self::None => 0,
            Self::Bronze => Self::BRONZE_MIN,
            Self::Silver => Self::SILVER_MIN,
            Self::Gold => Self::GOLD_MIN,
            Self::Platinum => Self::PLATINUM_MIN,
        }
    }

    /// Get the next tier up, if any.
    #[must_use]
    pub const fn next_tier(&self) -> Option<Self> {
        match self {
            Self::None => Some(Self::Bronze),
            Self::Bronze => Some(Self::Silver),
            Self::Silver => Some(Self::Gold),
            Self::Gold => Some(Self::Platinum),
            Self::Platinum => None,
        }
    }
}

impl std::fmt::Display for StakingTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Bronze => write!(f, "bronze"),
            Self::Silver => write!(f, "silver"),
            Self::Gold => write!(f, "gold"),
            Self::Platinum => write!(f, "platinum"),
        }
    }
}

/// Current staking state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingState {
    /// Amount currently staked.
    pub staked_amount: u64,
    /// Current tier.
    pub tier: StakingTier,
    /// When the stake was last updated.
    pub last_updated: DateTime<Utc>,
    /// Pending unstake amount (in cooldown).
    pub pending_unstake: u64,
    /// When pending unstake becomes available.
    pub unstake_available_at: Option<DateTime<Utc>>,
}

impl Default for StakingState {
    fn default() -> Self {
        Self {
            staked_amount: 0,
            tier: StakingTier::None,
            last_updated: Utc::now(),
            pending_unstake: 0,
            unstake_available_at: None,
        }
    }
}

impl StakingState {
    /// Create a new staking state with the given amount.
    #[must_use]
    pub fn new(staked_amount: u64) -> Self {
        Self {
            staked_amount,
            tier: StakingTier::from_amount(staked_amount),
            last_updated: Utc::now(),
            pending_unstake: 0,
            unstake_available_at: None,
        }
    }

    /// Update the staked amount.
    pub fn set_staked(&mut self, amount: u64) {
        self.staked_amount = amount;
        self.tier = StakingTier::from_amount(amount);
        self.last_updated = Utc::now();
        debug!(amount, tier = %self.tier, "Updated staking state");
    }

    /// Add to pending unstake.
    pub fn request_unstake(&mut self, amount: u64, cooldown: chrono::Duration) {
        self.pending_unstake = self.pending_unstake.saturating_add(amount);
        self.unstake_available_at = Some(Utc::now() + cooldown);
        info!(
            amount,
            available_at = ?self.unstake_available_at,
            "Unstake requested"
        );
    }

    /// Check if unstake is available.
    #[must_use]
    pub fn can_withdraw_unstake(&self) -> bool {
        self.pending_unstake > 0
            && self
                .unstake_available_at
                .map_or(true, |t| Utc::now() >= t)
    }

    /// Clear pending unstake (after withdrawal).
    pub fn clear_pending_unstake(&mut self) {
        self.pending_unstake = 0;
        self.unstake_available_at = None;
    }

    /// Get total locked (staked + pending unstake).
    #[must_use]
    pub fn total_locked(&self) -> u64 {
        self.staked_amount.saturating_add(self.pending_unstake)
    }

    /// Amount needed to reach next tier.
    #[must_use]
    pub fn amount_to_next_tier(&self) -> Option<u64> {
        self.tier
            .next_tier()
            .map(|next| next.min_stake().saturating_sub(self.staked_amount))
    }
}

/// Staking tracker for the gateway.
#[derive(Debug, Default)]
pub struct StakingTracker {
    /// Current staking state.
    state: StakingState,
    /// Unstake cooldown period.
    cooldown: chrono::Duration,
}

impl StakingTracker {
    /// Default unstake cooldown (7 days).
    pub const DEFAULT_COOLDOWN_DAYS: i64 = 7;

    /// Create a new staking tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: StakingState::default(),
            cooldown: chrono::Duration::days(Self::DEFAULT_COOLDOWN_DAYS),
        }
    }

    /// Create with custom cooldown period.
    #[must_use]
    pub fn with_cooldown(cooldown: chrono::Duration) -> Self {
        Self {
            state: StakingState::default(),
            cooldown,
        }
    }

    /// Get current staking state.
    #[must_use]
    pub fn state(&self) -> &StakingState {
        &self.state
    }

    /// Get current tier.
    #[must_use]
    pub fn tier(&self) -> StakingTier {
        self.state.tier
    }

    /// Get staked amount.
    #[must_use]
    pub fn staked_amount(&self) -> u64 {
        self.state.staked_amount
    }

    /// Update staked amount (from on-chain query).
    pub fn update_staked(&mut self, amount: u64) {
        self.state.set_staked(amount);
    }

    /// Request unstake.
    pub fn request_unstake(&mut self, amount: u64) {
        let max_unstake = self.state.staked_amount.saturating_sub(self.state.pending_unstake);
        let actual = amount.min(max_unstake);
        if actual > 0 {
            self.state.request_unstake(actual, self.cooldown);
        }
    }

    /// Check if can provide compute.
    #[must_use]
    pub fn can_provide(&self) -> bool {
        self.state.tier.can_provide()
    }

    /// Get max concurrent jobs allowed.
    #[must_use]
    pub fn max_jobs(&self) -> Option<usize> {
        self.state.tier.max_concurrent_jobs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_amount() {
        assert_eq!(StakingTier::from_amount(0), StakingTier::None);
        assert_eq!(StakingTier::from_amount(99_999_999_999), StakingTier::None);
        assert_eq!(
            StakingTier::from_amount(100_000_000_000),
            StakingTier::Bronze
        );
        assert_eq!(
            StakingTier::from_amount(1_000_000_000_000),
            StakingTier::Silver
        );
        assert_eq!(
            StakingTier::from_amount(10_000_000_000_000),
            StakingTier::Gold
        );
        assert_eq!(
            StakingTier::from_amount(100_000_000_000_000),
            StakingTier::Platinum
        );
    }

    #[test]
    fn test_tier_max_jobs() {
        assert_eq!(StakingTier::None.max_concurrent_jobs(), Some(0));
        assert_eq!(StakingTier::Bronze.max_concurrent_jobs(), Some(10));
        assert_eq!(StakingTier::Silver.max_concurrent_jobs(), Some(50));
        assert_eq!(StakingTier::Gold.max_concurrent_jobs(), Some(200));
        assert_eq!(StakingTier::Platinum.max_concurrent_jobs(), None);
    }

    #[test]
    fn test_tier_can_provide() {
        assert!(!StakingTier::None.can_provide());
        assert!(StakingTier::Bronze.can_provide());
        assert!(StakingTier::Platinum.can_provide());
    }

    #[test]
    fn test_staking_state_new() {
        let state = StakingState::new(1_000_000_000_000);
        assert_eq!(state.tier, StakingTier::Silver);
        assert_eq!(state.staked_amount, 1_000_000_000_000);
    }

    #[test]
    fn test_amount_to_next_tier() {
        let state = StakingState::new(500_000_000_000); // 500 MOLT (Bronze)
        let to_silver = state.amount_to_next_tier().unwrap();
        assert_eq!(to_silver, 500_000_000_000); // Need 500 more to reach Silver
    }

    #[test]
    fn test_staking_tracker() {
        let mut tracker = StakingTracker::new();

        assert!(!tracker.can_provide());
        assert_eq!(tracker.tier(), StakingTier::None);

        tracker.update_staked(100_000_000_000);
        assert!(tracker.can_provide());
        assert_eq!(tracker.tier(), StakingTier::Bronze);
        assert_eq!(tracker.max_jobs(), Some(10));
    }

    #[test]
    fn test_unstake_request() {
        let mut tracker = StakingTracker::with_cooldown(chrono::Duration::seconds(1));
        tracker.update_staked(1_000_000_000_000);

        tracker.request_unstake(500_000_000_000);

        let state = tracker.state();
        assert_eq!(state.pending_unstake, 500_000_000_000);
        assert!(state.unstake_available_at.is_some());
    }
}
