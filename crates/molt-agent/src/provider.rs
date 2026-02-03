//! Provider agent — advertises capacity, accepts jobs, executes workloads.
//!
//! A provider agent manages compute resources and decides whether to accept
//! incoming job requests based on capacity, price, and autonomy settings.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::autonomy::{AutonomyMode, Decision, DecisionThresholds, JobDecision};

/// Unique identifier for a provider.
pub type ProviderId = Uuid;

/// Unique identifier for a job.
pub type JobId = Uuid;

/// Current state of a provider agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderState {
    /// Unique identifier for this provider.
    pub id: ProviderId,
    /// Total compute capacity (abstract units).
    pub capacity: u64,
    /// Currently allocated capacity.
    pub allocated: u64,
    /// Number of active jobs being executed.
    pub active_jobs: u32,
    /// Maximum concurrent jobs allowed.
    pub max_jobs: u32,
    /// Total earnings accumulated (in base units).
    pub earnings: u64,
}

impl ProviderState {
    /// Creates a new provider state with the given capacity.
    #[must_use]
    pub fn new(capacity: u64, max_jobs: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            capacity,
            allocated: 0,
            active_jobs: 0,
            max_jobs,
            earnings: 0,
        }
    }

    /// Returns available (unallocated) capacity.
    #[must_use]
    pub const fn available_capacity(&self) -> u64 {
        self.capacity.saturating_sub(self.allocated)
    }

    /// Returns true if the provider can accept more jobs.
    #[must_use]
    pub fn can_accept_jobs(&self) -> bool {
        self.active_jobs < self.max_jobs && self.available_capacity() > 0
    }

    /// Returns the utilization ratio (0.0 to 1.0).
    #[must_use]
    pub fn utilization(&self) -> f64 {
        if self.capacity == 0 {
            return 0.0;
        }
        self.allocated as f64 / self.capacity as f64
    }

    /// Allocate capacity for a job. Returns Ok if successful.
    pub fn allocate(&mut self, amount: u64) -> Result<(), ProviderError> {
        if amount > self.available_capacity() {
            return Err(ProviderError::InsufficientCapacity {
                required: amount,
                available: self.available_capacity(),
            });
        }
        if self.active_jobs >= self.max_jobs {
            return Err(ProviderError::MaxJobsReached {
                max: self.max_jobs,
            });
        }
        self.allocated += amount;
        self.active_jobs += 1;
        Ok(())
    }

    /// Release capacity after job completion.
    pub const fn release(&mut self, amount: u64, payment: u64) {
        self.allocated = self.allocated.saturating_sub(amount);
        self.active_jobs = self.active_jobs.saturating_sub(1);
        self.earnings += payment;
    }
}

/// Errors that can occur in provider operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// Not enough capacity available.
    #[error("insufficient capacity: required {required}, available {available}")]
    InsufficientCapacity {
        /// Capacity required.
        required: u64,
        /// Capacity available.
        available: u64,
    },
    /// Maximum concurrent jobs reached.
    #[error("maximum jobs reached: {max}")]
    MaxJobsReached {
        /// Maximum allowed.
        max: u32,
    },
}

/// A job specification that a provider evaluates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
    /// Unique identifier for this job.
    pub id: JobId,
    /// Compute resources required (abstract units).
    pub resources: u64,
    /// Offered price (in base units).
    pub price: u64,
    /// Expected duration in seconds.
    pub duration_secs: u64,
    /// Buyer's reputation score (0-100).
    pub buyer_reputation: u8,
}

impl JobSpec {
    /// Creates a new job specification.
    #[must_use]
    pub fn new(resources: u64, price: u64, duration_secs: u64, buyer_reputation: u8) -> Self {
        Self {
            id: Uuid::new_v4(),
            resources,
            price,
            duration_secs,
            buyer_reputation,
        }
    }

    /// Returns the price per resource unit.
    #[must_use]
    pub fn price_per_unit(&self) -> f64 {
        if self.resources == 0 {
            return 0.0;
        }
        self.price as f64 / self.resources as f64
    }
}

/// Policy constraints for provider decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderPolicy {
    /// Minimum acceptable price per resource unit.
    pub min_price_per_unit: f64,
    /// Decision thresholds based on autonomy mode.
    pub thresholds: DecisionThresholds,
}

impl Default for ProviderPolicy {
    fn default() -> Self {
        Self {
            min_price_per_unit: 1.0,
            thresholds: DecisionThresholds::default(),
        }
    }
}

impl ProviderPolicy {
    /// Create a policy appropriate for the given autonomy mode.
    #[must_use]
    pub fn for_mode(mode: AutonomyMode) -> Self {
        Self {
            min_price_per_unit: match mode {
                AutonomyMode::Conservative => 2.0,
                AutonomyMode::Moderate => 1.0,
                AutonomyMode::Aggressive => 0.5,
            },
            thresholds: DecisionThresholds::for_mode(mode),
        }
    }
}

/// Provider decision maker implementing the Decision trait.
#[derive(Debug, Clone, Default)]
pub struct ProviderDecisionMaker {
    /// Current provider state.
    pub state: Option<ProviderState>,
}

impl ProviderDecisionMaker {
    /// Create a new decision maker with the given state.
    #[must_use]
    pub const fn new(state: ProviderState) -> Self {
        Self { state: Some(state) }
    }

    /// Create a decision maker without state (for policy-only evaluation).
    #[must_use]
    pub const fn stateless() -> Self {
        Self { state: None }
    }
}

impl Decision for ProviderDecisionMaker {
    type Job = JobSpec;
    type Policy = ProviderPolicy;

    fn evaluate(&self, job: &Self::Job, mode: AutonomyMode, policy: &Self::Policy) -> JobDecision {
        // Check capacity if we have state
        if let Some(ref state) = self.state {
            if job.resources > state.available_capacity() {
                return JobDecision::reject("insufficient capacity");
            }
            if state.active_jobs >= state.max_jobs {
                return JobDecision::reject("max jobs reached");
            }
        }

        // Check price against minimum
        let price_per_unit = job.price_per_unit();
        if price_per_unit < policy.min_price_per_unit {
            // In aggressive mode, counter-offer instead of reject
            if mode.allows_counter_offers() {
                let min_price = (job.resources as f64 * policy.min_price_per_unit).ceil() as u64;
                return JobDecision::counter_offer(min_price, "price below minimum");
            }
            return JobDecision::reject("price below minimum");
        }

        // Check reputation
        if job.buyer_reputation < policy.thresholds.min_reputation {
            return JobDecision::need_approval("buyer reputation too low");
        }

        // Check duration
        if job.duration_secs > policy.thresholds.max_duration_secs {
            return JobDecision::need_approval("job duration exceeds threshold");
        }

        // Check auto-approval threshold
        if job.price > policy.thresholds.max_auto_accept_price {
            return JobDecision::need_approval("price exceeds auto-approval threshold");
        }

        JobDecision::accept()
    }
}

/// Evaluate a job against provider policy.
///
/// This is a convenience function that creates a stateless decision maker.
#[must_use]
pub fn evaluate_job(job: &JobSpec, mode: AutonomyMode, policy: &ProviderPolicy) -> JobDecision {
    let maker = ProviderDecisionMaker::stateless();
    maker.evaluate(job, mode, policy)
}

/// Evaluate a job with full provider state consideration.
#[must_use]
pub fn evaluate_job_with_state(
    job: &JobSpec,
    state: &ProviderState,
    mode: AutonomyMode,
    policy: &ProviderPolicy,
) -> JobDecision {
    let maker = ProviderDecisionMaker::new(state.clone());
    maker.evaluate(job, mode, policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // ProviderState tests
    // ==========================================================================

    #[test]
    fn provider_state_new() {
        let state = ProviderState::new(1000, 10);
        assert_eq!(state.capacity, 1000);
        assert_eq!(state.allocated, 0);
        assert_eq!(state.active_jobs, 0);
        assert_eq!(state.max_jobs, 10);
        assert_eq!(state.earnings, 0);
    }

    #[test]
    fn provider_state_available_capacity() {
        let mut state = ProviderState::new(1000, 10);
        assert_eq!(state.available_capacity(), 1000);
        
        state.allocated = 300;
        assert_eq!(state.available_capacity(), 700);
        
        state.allocated = 1000;
        assert_eq!(state.available_capacity(), 0);
    }

    #[test]
    fn provider_state_can_accept_jobs() {
        let mut state = ProviderState::new(1000, 2);
        assert!(state.can_accept_jobs());
        
        state.active_jobs = 2;
        assert!(!state.can_accept_jobs());
        
        state.active_jobs = 1;
        state.allocated = 1000;
        assert!(!state.can_accept_jobs());
    }

    #[test]
    fn provider_state_utilization() {
        let mut state = ProviderState::new(1000, 10);
        assert!((state.utilization() - 0.0).abs() < f64::EPSILON);
        
        state.allocated = 500;
        assert!((state.utilization() - 0.5).abs() < f64::EPSILON);
        
        state.allocated = 1000;
        assert!((state.utilization() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn provider_state_utilization_zero_capacity() {
        let state = ProviderState::new(0, 10);
        assert!((state.utilization() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn provider_state_allocate_success() {
        let mut state = ProviderState::new(1000, 10);
        let result = state.allocate(300);
        assert!(result.is_ok());
        assert_eq!(state.allocated, 300);
        assert_eq!(state.active_jobs, 1);
    }

    #[test]
    fn provider_state_allocate_insufficient_capacity() {
        let mut state = ProviderState::new(100, 10);
        let result = state.allocate(200);
        assert!(matches!(
            result,
            Err(ProviderError::InsufficientCapacity { required: 200, available: 100 })
        ));
    }

    #[test]
    fn provider_state_allocate_max_jobs_reached() {
        let mut state = ProviderState::new(1000, 2);
        state.active_jobs = 2;
        let result = state.allocate(100);
        assert!(matches!(
            result,
            Err(ProviderError::MaxJobsReached { max: 2 })
        ));
    }

    #[test]
    fn provider_state_release() {
        let mut state = ProviderState::new(1000, 10);
        state.allocated = 300;
        state.active_jobs = 1;
        
        state.release(300, 500);
        
        assert_eq!(state.allocated, 0);
        assert_eq!(state.active_jobs, 0);
        assert_eq!(state.earnings, 500);
    }

    // ==========================================================================
    // JobSpec tests
    // ==========================================================================

    #[test]
    fn job_spec_new() {
        let job = JobSpec::new(100, 200, 3600, 80);
        assert_eq!(job.resources, 100);
        assert_eq!(job.price, 200);
        assert_eq!(job.duration_secs, 3600);
        assert_eq!(job.buyer_reputation, 80);
    }

    #[test]
    fn job_spec_price_per_unit() {
        let job = JobSpec::new(100, 200, 3600, 80);
        assert!((job.price_per_unit() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn job_spec_price_per_unit_zero_resources() {
        let job = JobSpec::new(0, 200, 3600, 80);
        assert!((job.price_per_unit() - 0.0).abs() < f64::EPSILON);
    }

    // ==========================================================================
    // ProviderPolicy tests
    // ==========================================================================

    #[test]
    fn provider_policy_default() {
        let policy = ProviderPolicy::default();
        assert!((policy.min_price_per_unit - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn provider_policy_for_conservative() {
        let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);
        assert!((policy.min_price_per_unit - 2.0).abs() < f64::EPSILON);
        assert_eq!(policy.thresholds.max_auto_accept_price, 100);
    }

    #[test]
    fn provider_policy_for_aggressive() {
        let policy = ProviderPolicy::for_mode(AutonomyMode::Aggressive);
        assert!((policy.min_price_per_unit - 0.5).abs() < f64::EPSILON);
        assert_eq!(policy.thresholds.max_auto_accept_price, 1_000_000);
    }

    // ==========================================================================
    // evaluate_job tests
    // ==========================================================================

    #[test]
    fn evaluate_job_accepts_good_job() {
        let job = JobSpec::new(100, 200, 600, 80);
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
        assert!(decision.is_accept());
    }

    #[test]
    fn evaluate_job_rejects_low_price_conservative() {
        let job = JobSpec::new(100, 50, 600, 80); // 0.5 per unit
        let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);
        
        let decision = evaluate_job(&job, AutonomyMode::Conservative, &policy);
        assert!(decision.is_reject());
    }

    #[test]
    fn evaluate_job_counter_offers_low_price_moderate() {
        let job = JobSpec::new(100, 50, 600, 80); // 0.5 per unit, policy wants 1.0
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
        assert!(decision.is_counter_offer());
        
        if let JobDecision::CounterOffer { proposed_price, .. } = decision {
            assert_eq!(proposed_price, 100); // 100 resources * 1.0 min price
        }
    }

    #[test]
    fn evaluate_job_needs_approval_low_reputation() {
        let job = JobSpec::new(100, 200, 600, 30); // reputation 30, threshold 50
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
        assert!(decision.needs_approval());
    }

    #[test]
    fn evaluate_job_needs_approval_long_duration() {
        let job = JobSpec::new(100, 200, 7200, 80); // 2 hours, threshold 1 hour
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
        assert!(decision.needs_approval());
    }

    #[test]
    fn evaluate_job_needs_approval_high_price() {
        let job = JobSpec::new(100, 50_000, 600, 80); // 50k, threshold 10k
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);
        assert!(decision.needs_approval());
    }

    #[test]
    fn evaluate_job_with_state_rejects_insufficient_capacity() {
        let state = ProviderState::new(50, 10);
        let job = JobSpec::new(100, 200, 600, 80); // needs 100, only 50 available
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);
        assert!(decision.is_reject());
    }

    #[test]
    fn evaluate_job_with_state_rejects_max_jobs() {
        let mut state = ProviderState::new(1000, 2);
        state.active_jobs = 2;
        let job = JobSpec::new(100, 200, 600, 80);
        let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
        
        let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);
        assert!(decision.is_reject());
    }

    #[test]
    fn provider_state_serialization_roundtrip() {
        let state = ProviderState::new(1000, 10);
        let json = serde_json::to_string(&state).unwrap();
        let parsed: ProviderState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.capacity, parsed.capacity);
        assert_eq!(state.max_jobs, parsed.max_jobs);
    }
}
