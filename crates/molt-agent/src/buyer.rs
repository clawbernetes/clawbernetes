//! Buyer agent — discovers providers, submits jobs, monitors execution.
//!
//! A buyer agent seeks compute resources and evaluates offers from providers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::autonomy::{AutonomyMode, DecisionThresholds};
use crate::provider::ProviderId;

/// Unique identifier for a buyer.
pub type BuyerId = Uuid;

/// Unique identifier for a pending job request.
pub type RequestId = Uuid;

/// Current state of a buyer agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuyerState {
    /// Unique identifier for this buyer.
    pub id: BuyerId,
    /// Number of pending job requests (awaiting provider).
    pub pending_jobs: u32,
    /// Number of active jobs (being executed).
    pub active_jobs: u32,
    /// Maximum concurrent active jobs allowed.
    pub max_active_jobs: u32,
    /// Total amount spent (in base units).
    pub spent: u64,
    /// Available budget (in base units).
    pub budget: u64,
}

impl BuyerState {
    /// Creates a new buyer state with the given budget.
    #[must_use]
    pub fn new(budget: u64, max_active_jobs: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            pending_jobs: 0,
            active_jobs: 0,
            max_active_jobs,
            spent: 0,
            budget,
        }
    }

    /// Returns remaining budget.
    #[must_use]
    pub const fn remaining_budget(&self) -> u64 {
        self.budget.saturating_sub(self.spent)
    }

    /// Returns true if the buyer can submit more jobs.
    #[must_use]
    pub fn can_submit_jobs(&self) -> bool {
        self.active_jobs < self.max_active_jobs && self.remaining_budget() > 0
    }

    /// Returns total jobs (pending + active).
    #[must_use]
    pub const fn total_jobs(&self) -> u32 {
        self.pending_jobs + self.active_jobs
    }

    /// Record a new pending job submission.
    pub const fn submit_job(&mut self) -> Result<(), BuyerError> {
        if self.active_jobs >= self.max_active_jobs {
            return Err(BuyerError::MaxJobsReached {
                max: self.max_active_jobs,
            });
        }
        self.pending_jobs += 1;
        Ok(())
    }

    /// Transition a job from pending to active (provider accepted).
    pub fn activate_job(&mut self, price: u64) -> Result<(), BuyerError> {
        if self.pending_jobs == 0 {
            return Err(BuyerError::NoPendingJobs);
        }
        if price > self.remaining_budget() {
            return Err(BuyerError::InsufficientBudget {
                required: price,
                available: self.remaining_budget(),
            });
        }
        self.pending_jobs -= 1;
        self.active_jobs += 1;
        self.spent += price;
        Ok(())
    }

    /// Complete an active job.
    pub const fn complete_job(&mut self) {
        self.active_jobs = self.active_jobs.saturating_sub(1);
    }

    /// Cancel a pending job.
    pub const fn cancel_pending(&mut self) {
        self.pending_jobs = self.pending_jobs.saturating_sub(1);
    }
}

/// Errors that can occur in buyer operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuyerError {
    /// Maximum concurrent jobs reached.
    #[error("maximum active jobs reached: {max}")]
    MaxJobsReached {
        /// Maximum allowed.
        max: u32,
    },
    /// No pending jobs to activate.
    #[error("no pending jobs to activate")]
    NoPendingJobs,
    /// Not enough budget available.
    #[error("insufficient budget: required {required}, available {available}")]
    InsufficientBudget {
        /// Amount required.
        required: u64,
        /// Amount available.
        available: u64,
    },
}

/// Requirements for a job that a buyer wants fulfilled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRequirements {
    /// Compute resources needed (abstract units).
    pub resources: u64,
    /// Maximum acceptable price (in base units).
    pub max_price: u64,
    /// Maximum acceptable duration in seconds.
    pub max_duration_secs: u64,
    /// Minimum acceptable provider reputation (0-100).
    pub min_provider_reputation: u8,
}

impl JobRequirements {
    /// Create new job requirements.
    #[must_use]
    pub const fn new(
        resources: u64,
        max_price: u64,
        max_duration_secs: u64,
        min_provider_reputation: u8,
    ) -> Self {
        Self {
            resources,
            max_price,
            max_duration_secs,
            min_provider_reputation,
        }
    }

    /// Returns maximum acceptable price per resource unit.
    #[must_use]
    pub fn max_price_per_unit(&self) -> f64 {
        if self.resources == 0 {
            return 0.0;
        }
        self.max_price as f64 / self.resources as f64
    }
}

/// An offer from a provider in response to a job request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOffer {
    /// Provider making the offer.
    pub provider_id: ProviderId,
    /// Offered price (in base units).
    pub price: u64,
    /// Estimated completion time in seconds.
    pub estimated_duration_secs: u64,
    /// Provider's reputation score (0-100).
    pub provider_reputation: u8,
    /// Time until offer expires (seconds from now).
    pub expires_in_secs: u64,
}

impl ProviderOffer {
    /// Create a new provider offer.
    #[must_use]
    pub const fn new(
        provider_id: ProviderId,
        price: u64,
        estimated_duration_secs: u64,
        provider_reputation: u8,
        expires_in_secs: u64,
    ) -> Self {
        Self {
            provider_id,
            price,
            estimated_duration_secs,
            provider_reputation,
            expires_in_secs,
        }
    }
}

/// Policy for buyer decision-making.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyerPolicy {
    /// Maximum price multiplier over requirements (e.g., 1.2 = 20% over max).
    pub price_flexibility: f64,
    /// Decision thresholds based on autonomy mode.
    pub thresholds: DecisionThresholds,
    /// Whether to auto-accept the best offer.
    pub auto_accept_best: bool,
}

impl Default for BuyerPolicy {
    fn default() -> Self {
        Self {
            price_flexibility: 1.0,
            thresholds: DecisionThresholds::default(),
            auto_accept_best: false,
        }
    }
}

impl BuyerPolicy {
    /// Create a policy appropriate for the given autonomy mode.
    #[must_use]
    pub fn for_mode(mode: AutonomyMode) -> Self {
        match mode {
            AutonomyMode::Conservative => Self {
                price_flexibility: 1.0,
                thresholds: DecisionThresholds::for_mode(mode),
                auto_accept_best: false,
            },
            AutonomyMode::Moderate => Self {
                price_flexibility: 1.1,
                thresholds: DecisionThresholds::for_mode(mode),
                auto_accept_best: true,
            },
            AutonomyMode::Aggressive => Self {
                price_flexibility: 1.25,
                thresholds: DecisionThresholds::for_mode(mode),
                auto_accept_best: true,
            },
        }
    }
}

/// Evaluate whether an offer meets the buyer's requirements.
///
/// Returns true if the offer should be accepted.
#[must_use]
pub fn evaluate_offer(
    offer: &ProviderOffer,
    requirements: &JobRequirements,
    policy: &BuyerPolicy,
) -> bool {
    // Check price with flexibility
    let max_acceptable = (requirements.max_price as f64 * policy.price_flexibility).ceil() as u64;
    if offer.price > max_acceptable {
        return false;
    }

    // Check duration
    if offer.estimated_duration_secs > requirements.max_duration_secs {
        return false;
    }

    // Check reputation
    if offer.provider_reputation < requirements.min_provider_reputation {
        return false;
    }

    true
}

/// Score an offer for comparison (higher is better).
///
/// Considers price, duration, and reputation.
#[must_use]
pub fn score_offer(offer: &ProviderOffer, requirements: &JobRequirements) -> f64 {
    // Price score: lower is better (invert and normalize)
    let price_ratio = if requirements.max_price > 0 {
        1.0 - (offer.price as f64 / requirements.max_price as f64).min(1.0)
    } else {
        0.0
    };

    // Duration score: lower is better (invert and normalize)
    let duration_ratio = if requirements.max_duration_secs > 0 {
        1.0 - (offer.estimated_duration_secs as f64 / requirements.max_duration_secs as f64).min(1.0)
    } else {
        0.0
    };

    // Reputation score: higher is better (normalize to 0-1)
    let reputation_ratio = f64::from(offer.provider_reputation) / 100.0;

    // Weighted combination
    price_ratio * 0.5 + duration_ratio * 0.2 + reputation_ratio * 0.3
}

/// Select the best offer from a list of acceptable offers.
#[must_use]
pub fn select_best_offer<'a>(
    offers: &'a [ProviderOffer],
    requirements: &JobRequirements,
    policy: &BuyerPolicy,
) -> Option<&'a ProviderOffer> {
    offers
        .iter()
        .filter(|o| evaluate_offer(o, requirements, policy))
        .max_by(|a, b| {
            let score_a = score_offer(a, requirements);
            let score_b = score_offer(b, requirements);
            score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // BuyerState tests
    // ==========================================================================

    #[test]
    fn buyer_state_new() {
        let state = BuyerState::new(10_000, 5);
        assert_eq!(state.budget, 10_000);
        assert_eq!(state.max_active_jobs, 5);
        assert_eq!(state.pending_jobs, 0);
        assert_eq!(state.active_jobs, 0);
        assert_eq!(state.spent, 0);
    }

    #[test]
    fn buyer_state_remaining_budget() {
        let mut state = BuyerState::new(10_000, 5);
        assert_eq!(state.remaining_budget(), 10_000);
        
        state.spent = 3000;
        assert_eq!(state.remaining_budget(), 7000);
        
        state.spent = 15_000; // Over budget
        assert_eq!(state.remaining_budget(), 0);
    }

    #[test]
    fn buyer_state_can_submit_jobs() {
        let mut state = BuyerState::new(10_000, 2);
        assert!(state.can_submit_jobs());
        
        state.active_jobs = 2;
        assert!(!state.can_submit_jobs());
        
        state.active_jobs = 1;
        state.spent = 10_000;
        assert!(!state.can_submit_jobs());
    }

    #[test]
    fn buyer_state_total_jobs() {
        let mut state = BuyerState::new(10_000, 5);
        assert_eq!(state.total_jobs(), 0);
        
        state.pending_jobs = 2;
        state.active_jobs = 3;
        assert_eq!(state.total_jobs(), 5);
    }

    #[test]
    fn buyer_state_submit_job_success() {
        let mut state = BuyerState::new(10_000, 5);
        let result = state.submit_job();
        assert!(result.is_ok());
        assert_eq!(state.pending_jobs, 1);
    }

    #[test]
    fn buyer_state_submit_job_max_reached() {
        let mut state = BuyerState::new(10_000, 2);
        state.active_jobs = 2;
        let result = state.submit_job();
        assert!(matches!(result, Err(BuyerError::MaxJobsReached { max: 2 })));
    }

    #[test]
    fn buyer_state_activate_job_success() {
        let mut state = BuyerState::new(10_000, 5);
        state.pending_jobs = 1;
        
        let result = state.activate_job(1000);
        assert!(result.is_ok());
        assert_eq!(state.pending_jobs, 0);
        assert_eq!(state.active_jobs, 1);
        assert_eq!(state.spent, 1000);
    }

    #[test]
    fn buyer_state_activate_job_no_pending() {
        let mut state = BuyerState::new(10_000, 5);
        let result = state.activate_job(1000);
        assert!(matches!(result, Err(BuyerError::NoPendingJobs)));
    }

    #[test]
    fn buyer_state_activate_job_insufficient_budget() {
        let mut state = BuyerState::new(500, 5);
        state.pending_jobs = 1;
        
        let result = state.activate_job(1000);
        assert!(matches!(
            result,
            Err(BuyerError::InsufficientBudget { required: 1000, available: 500 })
        ));
    }

    #[test]
    fn buyer_state_complete_job() {
        let mut state = BuyerState::new(10_000, 5);
        state.active_jobs = 2;
        
        state.complete_job();
        assert_eq!(state.active_jobs, 1);
    }

    #[test]
    fn buyer_state_cancel_pending() {
        let mut state = BuyerState::new(10_000, 5);
        state.pending_jobs = 2;
        
        state.cancel_pending();
        assert_eq!(state.pending_jobs, 1);
    }

    // ==========================================================================
    // JobRequirements tests
    // ==========================================================================

    #[test]
    fn job_requirements_new() {
        let req = JobRequirements::new(100, 500, 3600, 60);
        assert_eq!(req.resources, 100);
        assert_eq!(req.max_price, 500);
        assert_eq!(req.max_duration_secs, 3600);
        assert_eq!(req.min_provider_reputation, 60);
    }

    #[test]
    fn job_requirements_max_price_per_unit() {
        let req = JobRequirements::new(100, 500, 3600, 60);
        assert!((req.max_price_per_unit() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn job_requirements_max_price_per_unit_zero_resources() {
        let req = JobRequirements::new(0, 500, 3600, 60);
        assert!((req.max_price_per_unit() - 0.0).abs() < f64::EPSILON);
    }

    // ==========================================================================
    // ProviderOffer tests
    // ==========================================================================

    #[test]
    fn provider_offer_new() {
        let id = Uuid::new_v4();
        let offer = ProviderOffer::new(id, 400, 1800, 75, 300);
        assert_eq!(offer.provider_id, id);
        assert_eq!(offer.price, 400);
        assert_eq!(offer.estimated_duration_secs, 1800);
        assert_eq!(offer.provider_reputation, 75);
        assert_eq!(offer.expires_in_secs, 300);
    }

    // ==========================================================================
    // BuyerPolicy tests
    // ==========================================================================

    #[test]
    fn buyer_policy_default() {
        let policy = BuyerPolicy::default();
        assert!((policy.price_flexibility - 1.0).abs() < f64::EPSILON);
        assert!(!policy.auto_accept_best);
    }

    #[test]
    fn buyer_policy_for_conservative() {
        let policy = BuyerPolicy::for_mode(AutonomyMode::Conservative);
        assert!((policy.price_flexibility - 1.0).abs() < f64::EPSILON);
        assert!(!policy.auto_accept_best);
    }

    #[test]
    fn buyer_policy_for_moderate() {
        let policy = BuyerPolicy::for_mode(AutonomyMode::Moderate);
        assert!((policy.price_flexibility - 1.1).abs() < f64::EPSILON);
        assert!(policy.auto_accept_best);
    }

    #[test]
    fn buyer_policy_for_aggressive() {
        let policy = BuyerPolicy::for_mode(AutonomyMode::Aggressive);
        assert!((policy.price_flexibility - 1.25).abs() < f64::EPSILON);
        assert!(policy.auto_accept_best);
    }

    // ==========================================================================
    // evaluate_offer tests
    // ==========================================================================

    #[test]
    fn evaluate_offer_accepts_good_offer() {
        let offer = ProviderOffer::new(Uuid::new_v4(), 400, 1800, 75, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        assert!(evaluate_offer(&offer, &requirements, &policy));
    }

    #[test]
    fn evaluate_offer_rejects_high_price() {
        let offer = ProviderOffer::new(Uuid::new_v4(), 600, 1800, 75, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        assert!(!evaluate_offer(&offer, &requirements, &policy));
    }

    #[test]
    fn evaluate_offer_accepts_with_flexibility() {
        let offer = ProviderOffer::new(Uuid::new_v4(), 550, 1800, 75, 300); // 10% over
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::for_mode(AutonomyMode::Moderate); // 10% flexibility
        
        assert!(evaluate_offer(&offer, &requirements, &policy));
    }

    #[test]
    fn evaluate_offer_rejects_long_duration() {
        let offer = ProviderOffer::new(Uuid::new_v4(), 400, 7200, 75, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        assert!(!evaluate_offer(&offer, &requirements, &policy));
    }

    #[test]
    fn evaluate_offer_rejects_low_reputation() {
        let offer = ProviderOffer::new(Uuid::new_v4(), 400, 1800, 50, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        assert!(!evaluate_offer(&offer, &requirements, &policy));
    }

    // ==========================================================================
    // score_offer tests
    // ==========================================================================

    #[test]
    fn score_offer_prefers_lower_price() {
        let low_price = ProviderOffer::new(Uuid::new_v4(), 200, 1800, 75, 300);
        let high_price = ProviderOffer::new(Uuid::new_v4(), 400, 1800, 75, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        
        assert!(score_offer(&low_price, &requirements) > score_offer(&high_price, &requirements));
    }

    #[test]
    fn score_offer_prefers_higher_reputation() {
        let high_rep = ProviderOffer::new(Uuid::new_v4(), 400, 1800, 90, 300);
        let low_rep = ProviderOffer::new(Uuid::new_v4(), 400, 1800, 60, 300);
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        
        assert!(score_offer(&high_rep, &requirements) > score_offer(&low_rep, &requirements));
    }

    // ==========================================================================
    // select_best_offer tests
    // ==========================================================================

    #[test]
    fn select_best_offer_returns_best() {
        let offers = vec![
            ProviderOffer::new(Uuid::new_v4(), 400, 1800, 75, 300),
            ProviderOffer::new(Uuid::new_v4(), 300, 1800, 80, 300), // Best
            ProviderOffer::new(Uuid::new_v4(), 450, 1800, 70, 300),
        ];
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        let best = select_best_offer(&offers, &requirements, &policy);
        assert!(best.is_some());
        assert_eq!(best.unwrap().price, 300);
    }

    #[test]
    fn select_best_offer_filters_unacceptable() {
        let offers = vec![
            ProviderOffer::new(Uuid::new_v4(), 600, 1800, 75, 300), // Too expensive
            ProviderOffer::new(Uuid::new_v4(), 400, 7200, 75, 300), // Too long
            ProviderOffer::new(Uuid::new_v4(), 400, 1800, 40, 300), // Low reputation
        ];
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        let best = select_best_offer(&offers, &requirements, &policy);
        assert!(best.is_none());
    }

    #[test]
    fn select_best_offer_empty_list() {
        let offers: Vec<ProviderOffer> = vec![];
        let requirements = JobRequirements::new(100, 500, 3600, 60);
        let policy = BuyerPolicy::default();
        
        let best = select_best_offer(&offers, &requirements, &policy);
        assert!(best.is_none());
    }

    #[test]
    fn buyer_state_serialization_roundtrip() {
        let state = BuyerState::new(10_000, 5);
        let json = serde_json::to_string(&state).unwrap();
        let parsed: BuyerState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.budget, parsed.budget);
        assert_eq!(state.max_active_jobs, parsed.max_active_jobs);
    }
}
