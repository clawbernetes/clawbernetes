//! Autonomy mode definitions and decision-making logic.
//!
//! Defines the spectrum of autonomous behavior for agents:
//! - [`AutonomyMode`] — Conservative, Moderate, or Aggressive
//! - [`JobDecision`] — Outcome of evaluating a job opportunity
//! - [`Decision`] — Trait for mode-specific decision logic

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Autonomy mode controlling how aggressively an agent operates.
///
/// Each mode represents a different risk/reward tradeoff:
/// - `Conservative` — Requires human approval for most decisions
/// - `Moderate` — Balanced automation with guardrails
/// - `Aggressive` — Maximum automation, minimal human intervention
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AutonomyMode {
    /// Low risk tolerance. Requires approval for most non-trivial decisions.
    #[default]
    Conservative,
    /// Medium risk tolerance. Auto-approves within configured thresholds.
    Moderate,
    /// High risk tolerance. Maximizes opportunity capture with minimal checks.
    Aggressive,
}

impl AutonomyMode {
    /// Returns a numeric risk tolerance score (0.0 to 1.0).
    ///
    /// Higher values indicate more aggressive behavior:
    /// - Conservative: 0.25
    /// - Moderate: 0.50
    /// - Aggressive: 0.85
    #[must_use]
    pub const fn risk_tolerance(&self) -> f64 {
        match self {
            Self::Conservative => 0.25,
            Self::Moderate => 0.50,
            Self::Aggressive => 0.85,
        }
    }

    /// Returns the maximum auto-approval amount for this mode (in base units).
    ///
    /// Jobs exceeding this threshold require human approval.
    #[must_use]
    pub const fn max_auto_approve(&self) -> u64 {
        match self {
            Self::Conservative => 100,     // Very low auto-approval
            Self::Moderate => 10_000,      // Reasonable threshold
            Self::Aggressive => 1_000_000, // High autonomy
        }
    }

    /// Returns whether this mode allows counter-offers.
    #[must_use]
    pub const fn allows_counter_offers(&self) -> bool {
        match self {
            Self::Conservative => false,
            Self::Moderate => true,
            Self::Aggressive => true,
        }
    }
}

/// Outcome of evaluating a job opportunity.
///
/// Represents the decision an agent makes when presented with a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum JobDecision {
    /// Accept the job as-is.
    Accept,
    /// Reject the job outright.
    Reject {
        /// Reason for rejection.
        reason: String,
    },
    /// Escalate to human operator for approval.
    NeedApproval {
        /// Why approval is needed.
        reason: String,
    },
    /// Propose alternative terms.
    CounterOffer {
        /// Proposed price in base units.
        proposed_price: u64,
        /// Explanation of the counter-offer.
        reason: String,
    },
}

impl JobDecision {
    /// Creates an Accept decision.
    #[must_use]
    pub const fn accept() -> Self {
        Self::Accept
    }

    /// Creates a Reject decision with the given reason.
    #[must_use]
    pub fn reject(reason: impl Into<String>) -> Self {
        Self::Reject {
            reason: reason.into(),
        }
    }

    /// Creates a `NeedApproval` decision with the given reason.
    #[must_use]
    pub fn need_approval(reason: impl Into<String>) -> Self {
        Self::NeedApproval {
            reason: reason.into(),
        }
    }

    /// Creates a `CounterOffer` decision.
    #[must_use]
    pub fn counter_offer(proposed_price: u64, reason: impl Into<String>) -> Self {
        Self::CounterOffer {
            proposed_price,
            reason: reason.into(),
        }
    }

    /// Returns true if this is an Accept decision.
    #[must_use]
    pub const fn is_accept(&self) -> bool {
        matches!(self, Self::Accept)
    }

    /// Returns true if this is a Reject decision.
    #[must_use]
    pub const fn is_reject(&self) -> bool {
        matches!(self, Self::Reject { .. })
    }

    /// Returns true if this requires human approval.
    #[must_use]
    pub const fn needs_approval(&self) -> bool {
        matches!(self, Self::NeedApproval { .. })
    }

    /// Returns true if this is a counter-offer.
    #[must_use]
    pub const fn is_counter_offer(&self) -> bool {
        matches!(self, Self::CounterOffer { .. })
    }
}

/// Trait for mode-specific decision logic.
///
/// Implementors define how an agent evaluates jobs based on its autonomy mode.
pub trait Decision {
    /// The type of job specification this decision maker evaluates.
    type Job;
    /// Policy constraints that influence decisions.
    type Policy;

    /// Evaluate a job and return a decision.
    fn evaluate(&self, job: &Self::Job, mode: AutonomyMode, policy: &Self::Policy) -> JobDecision;
}

/// Thresholds for autonomous decision-making.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionThresholds {
    /// Maximum price (in base units) to auto-accept.
    pub max_auto_accept_price: u64,
    /// Minimum reputation score (0-100) to accept without review.
    pub min_reputation: u8,
    /// Maximum job duration in seconds to accept without review.
    pub max_duration_secs: u64,
}

impl Default for DecisionThresholds {
    fn default() -> Self {
        Self {
            max_auto_accept_price: 1000,
            min_reputation: 50,
            max_duration_secs: 3600, // 1 hour
        }
    }
}

impl DecisionThresholds {
    /// Create thresholds appropriate for the given autonomy mode.
    #[must_use]
    pub const fn for_mode(mode: AutonomyMode) -> Self {
        match mode {
            AutonomyMode::Conservative => Self {
                max_auto_accept_price: 100,
                min_reputation: 80,
                max_duration_secs: 600, // 10 minutes
            },
            AutonomyMode::Moderate => Self {
                max_auto_accept_price: 10_000,
                min_reputation: 50,
                max_duration_secs: 3600, // 1 hour
            },
            AutonomyMode::Aggressive => Self {
                max_auto_accept_price: 1_000_000,
                min_reputation: 20,
                max_duration_secs: 86400, // 24 hours
            },
        }
    }
}

// =============================================================================
// PolicyBounds - Defines numeric bounds for autonomous actions
// =============================================================================

/// Bounds constraining what actions an agent can take autonomously.
///
/// These bounds define the "guardrails" for moderate autonomy mode,
/// specifying maximum amounts, durations, and counterparty requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyBounds {
    /// Maximum amount (in base units) the agent can spend per single action.
    pub max_spend_per_action: u64,
    /// Maximum total spending allowed per hour.
    pub max_spend_per_hour: u64,
    /// Maximum number of concurrent jobs the agent can accept.
    pub max_concurrent_jobs: u32,
    /// Maximum job duration (in seconds) to accept without approval.
    pub max_job_duration_secs: u64,
    /// Minimum reputation score (0-100) for counterparties.
    pub min_counterparty_reputation: u8,
    /// Whether to accept jobs from previously unknown counterparties.
    pub allow_new_counterparties: bool,
    /// Whether the agent can negotiate prices (counter-offers).
    pub allow_price_negotiation: bool,
}

impl Default for PolicyBounds {
    fn default() -> Self {
        Self::for_mode(AutonomyMode::Conservative)
    }
}

impl PolicyBounds {
    /// Create bounds appropriate for the given autonomy mode.
    ///
    /// - **Conservative**: Very restrictive, minimal autonomous action
    /// - **Moderate**: Balanced bounds for typical operation
    /// - **Aggressive**: Permissive bounds for maximum autonomy
    #[must_use]
    pub const fn for_mode(mode: AutonomyMode) -> Self {
        match mode {
            AutonomyMode::Conservative => Self {
                max_spend_per_action: 100,
                max_spend_per_hour: 500,
                max_concurrent_jobs: 1,
                max_job_duration_secs: 600, // 10 minutes
                min_counterparty_reputation: 80,
                allow_new_counterparties: false,
                allow_price_negotiation: false,
            },
            AutonomyMode::Moderate => Self {
                max_spend_per_action: 10_000,
                max_spend_per_hour: 50_000,
                max_concurrent_jobs: 5,
                max_job_duration_secs: 3600, // 1 hour
                min_counterparty_reputation: 50,
                allow_new_counterparties: true,
                allow_price_negotiation: true,
            },
            AutonomyMode::Aggressive => Self {
                max_spend_per_action: 1_000_000,
                max_spend_per_hour: 10_000_000,
                max_concurrent_jobs: 100,
                max_job_duration_secs: 86400, // 24 hours
                min_counterparty_reputation: 20,
                allow_new_counterparties: true,
                allow_price_negotiation: true,
            },
        }
    }

    /// Check if an amount is within the per-action spending limit.
    #[must_use]
    pub const fn is_amount_within_bounds(&self, amount: u64) -> bool {
        amount <= self.max_spend_per_action
    }

    /// Check if a duration is within the allowed job duration.
    #[must_use]
    pub const fn is_duration_within_bounds(&self, duration_secs: u64) -> bool {
        duration_secs <= self.max_job_duration_secs
    }

    /// Check if a reputation score meets the minimum requirement.
    #[must_use]
    pub const fn is_reputation_acceptable(&self, reputation: u8) -> bool {
        reputation >= self.min_counterparty_reputation
    }
}

// =============================================================================
// AutonomyPolicy - Complete policy configuration for an agent
// =============================================================================

/// Complete autonomy policy configuration for an agent.
///
/// Combines the autonomy mode with specific bounds and behavioral flags
/// to fully specify how an agent should operate.
///
/// # Autonomy Modes
///
/// - **Conservative**: Agent suggests actions, user approves every one.
///   The agent never takes autonomous action.
/// - **Moderate**: Agent executes actions within the defined `PolicyBounds`.
///   Actions exceeding bounds require user approval.
/// - **Aggressive**: Full autopilot mode. Agent maximizes earnings with
///   minimal restrictions, only pausing for extreme situations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutonomyPolicy {
    /// The base autonomy mode.
    pub mode: AutonomyMode,
    /// Bounds constraining autonomous actions.
    pub bounds: PolicyBounds,
    /// If true, every action requires explicit user approval (Conservative mode).
    pub require_approval_for_all: bool,
    /// If true, automatically accept jobs that appear profitable.
    pub auto_accept_profitable_jobs: bool,
    /// If true, agent can adjust pricing to optimize earnings.
    pub auto_optimize_pricing: bool,
    /// Decision thresholds for job evaluation.
    pub thresholds: DecisionThresholds,
}

impl Default for AutonomyPolicy {
    fn default() -> Self {
        Self::conservative()
    }
}

impl AutonomyPolicy {
    /// Create a conservative policy: agent suggests, user approves all.
    #[must_use]
    pub fn conservative() -> Self {
        Self {
            mode: AutonomyMode::Conservative,
            bounds: PolicyBounds::for_mode(AutonomyMode::Conservative),
            require_approval_for_all: true,
            auto_accept_profitable_jobs: false,
            auto_optimize_pricing: false,
            thresholds: DecisionThresholds::for_mode(AutonomyMode::Conservative),
        }
    }

    /// Create a moderate policy: agent executes within bounds.
    #[must_use]
    pub fn moderate() -> Self {
        Self {
            mode: AutonomyMode::Moderate,
            bounds: PolicyBounds::for_mode(AutonomyMode::Moderate),
            require_approval_for_all: false,
            auto_accept_profitable_jobs: false,
            auto_optimize_pricing: false,
            thresholds: DecisionThresholds::for_mode(AutonomyMode::Moderate),
        }
    }

    /// Create an aggressive policy: full autopilot, maximize earnings.
    #[must_use]
    pub fn aggressive() -> Self {
        Self {
            mode: AutonomyMode::Aggressive,
            bounds: PolicyBounds::for_mode(AutonomyMode::Aggressive),
            require_approval_for_all: false,
            auto_accept_profitable_jobs: true,
            auto_optimize_pricing: true,
            thresholds: DecisionThresholds::for_mode(AutonomyMode::Aggressive),
        }
    }

    /// Create a policy for the given autonomy mode.
    #[must_use]
    pub fn for_mode(mode: AutonomyMode) -> Self {
        match mode {
            AutonomyMode::Conservative => Self::conservative(),
            AutonomyMode::Moderate => Self::moderate(),
            AutonomyMode::Aggressive => Self::aggressive(),
        }
    }

    /// Builder: replace bounds with custom bounds.
    #[must_use]
    pub fn with_bounds(mut self, bounds: PolicyBounds) -> Self {
        self.bounds = bounds;
        self
    }

    /// Builder: set the autonomy mode.
    #[must_use]
    pub fn with_mode(mut self, mode: AutonomyMode) -> Self {
        self.mode = mode;
        self
    }

    /// Builder: set whether approval is required for all actions.
    #[must_use]
    pub const fn with_approval_required(mut self, required: bool) -> Self {
        self.require_approval_for_all = required;
        self
    }

    /// Builder: set whether to auto-accept profitable jobs.
    #[must_use]
    pub const fn with_auto_accept(mut self, auto_accept: bool) -> Self {
        self.auto_accept_profitable_jobs = auto_accept;
        self
    }

    /// Builder: set whether to auto-optimize pricing.
    #[must_use]
    pub const fn with_auto_pricing(mut self, auto_pricing: bool) -> Self {
        self.auto_optimize_pricing = auto_pricing;
        self
    }
}

// =============================================================================
// ProposedAction - Actions the agent might take
// =============================================================================

/// An action the agent proposes to take.
///
/// These are evaluated against the policy to determine if they can
/// be executed autonomously or require user approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProposedAction {
    /// Accept a job from a buyer.
    AcceptJob {
        /// Price offered for the job (in base units).
        price: u64,
        /// Expected job duration in seconds.
        duration_secs: u64,
        /// Reputation score of the counterparty (0-100).
        counterparty_reputation: u8,
    },
    /// Submit a bid for a job.
    SubmitBid {
        /// Bid amount in base units.
        amount: u64,
    },
    /// Make a counter-offer on a job.
    CounterOffer {
        /// Original offered price.
        original_price: u64,
        /// Price we're proposing.
        proposed_price: u64,
    },
    /// Accept a new counterparty we haven't worked with before.
    AcceptNewCounterparty {
        /// Reputation score of the new counterparty.
        reputation: u8,
    },
}

// =============================================================================
// EvaluationResult - Outcome of policy evaluation
// =============================================================================

/// Result of evaluating a proposed action against a policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum EvaluationResult {
    /// Action is approved for autonomous execution.
    Approved,
    /// Action requires human approval.
    NeedsApproval {
        /// Reason why approval is needed.
        reason: String,
    },
    /// Action is rejected outright.
    Rejected {
        /// Reason for rejection.
        reason: String,
    },
}

impl EvaluationResult {
    /// Returns true if the action is approved.
    #[must_use]
    pub const fn is_approved(&self) -> bool {
        matches!(self, Self::Approved)
    }

    /// Returns true if the action needs approval.
    #[must_use]
    pub const fn needs_approval(&self) -> bool {
        matches!(self, Self::NeedsApproval { .. })
    }

    /// Returns true if the action is rejected.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }
}

// =============================================================================
// PolicyEvaluator - Evaluates actions against a policy
// =============================================================================

/// Evaluates proposed actions against an autonomy policy.
///
/// The evaluator applies the policy rules to determine if an action
/// can be executed autonomously or requires user approval.
#[derive(Debug, Clone)]
pub struct PolicyEvaluator {
    policy: AutonomyPolicy,
}

impl PolicyEvaluator {
    /// Create a new evaluator with the given policy.
    #[must_use]
    pub const fn new(policy: AutonomyPolicy) -> Self {
        Self { policy }
    }

    /// Get a reference to the current policy.
    #[must_use]
    pub const fn policy(&self) -> &AutonomyPolicy {
        &self.policy
    }

    /// Evaluate a proposed action against the policy.
    ///
    /// Returns whether the action can be executed autonomously,
    /// needs user approval, or should be rejected.
    #[must_use]
    pub fn evaluate(&self, action: &ProposedAction) -> EvaluationResult {
        // Conservative mode: always require approval
        if self.policy.require_approval_for_all {
            return EvaluationResult::NeedsApproval {
                reason: "conservative mode requires approval for all actions".into(),
            };
        }

        match action {
            ProposedAction::AcceptJob {
                price,
                duration_secs,
                counterparty_reputation,
            } => self.evaluate_accept_job(*price, *duration_secs, *counterparty_reputation),

            ProposedAction::SubmitBid { amount } => self.evaluate_spend(*amount),

            ProposedAction::CounterOffer {
                original_price: _,
                proposed_price,
            } => {
                if !self.policy.bounds.allow_price_negotiation {
                    return EvaluationResult::NeedsApproval {
                        reason: "price negotiation not allowed by policy".into(),
                    };
                }
                self.evaluate_spend(*proposed_price)
            }

            ProposedAction::AcceptNewCounterparty { reputation } => {
                if !self.policy.bounds.allow_new_counterparties {
                    return EvaluationResult::NeedsApproval {
                        reason: "new counterparties not allowed by policy".into(),
                    };
                }
                if !self.policy.bounds.is_reputation_acceptable(*reputation) {
                    return EvaluationResult::NeedsApproval {
                        reason: format!(
                            "counterparty reputation {} below minimum {}",
                            reputation, self.policy.bounds.min_counterparty_reputation
                        ),
                    };
                }
                EvaluationResult::Approved
            }
        }
    }

    /// Evaluate an AcceptJob action.
    fn evaluate_accept_job(
        &self,
        price: u64,
        duration_secs: u64,
        counterparty_reputation: u8,
    ) -> EvaluationResult {
        // Check price bounds
        if !self.policy.bounds.is_amount_within_bounds(price) {
            return EvaluationResult::NeedsApproval {
                reason: format!(
                    "price {} exceeds maximum {}",
                    price, self.policy.bounds.max_spend_per_action
                ),
            };
        }

        // Check duration bounds
        if !self.policy.bounds.is_duration_within_bounds(duration_secs) {
            return EvaluationResult::NeedsApproval {
                reason: format!(
                    "duration {} exceeds maximum {}",
                    duration_secs, self.policy.bounds.max_job_duration_secs
                ),
            };
        }

        // Check reputation
        if !self
            .policy
            .bounds
            .is_reputation_acceptable(counterparty_reputation)
        {
            return EvaluationResult::NeedsApproval {
                reason: format!(
                    "counterparty reputation {} below minimum {}",
                    counterparty_reputation, self.policy.bounds.min_counterparty_reputation
                ),
            };
        }

        EvaluationResult::Approved
    }

    /// Evaluate a spending action (bid, payment, etc.)
    fn evaluate_spend(&self, amount: u64) -> EvaluationResult {
        if !self.policy.bounds.is_amount_within_bounds(amount) {
            return EvaluationResult::NeedsApproval {
                reason: format!(
                    "amount {} exceeds maximum {}",
                    amount, self.policy.bounds.max_spend_per_action
                ),
            };
        }
        EvaluationResult::Approved
    }
}

// =============================================================================
// SpendingTracker - Tracks spending for rate limiting
// =============================================================================

/// Tracks spending over time for hourly budget enforcement.
///
/// This is used to ensure the agent doesn't exceed its hourly spending limit.
#[derive(Debug, Clone)]
pub struct SpendingTracker {
    hourly_budget: u64,
    spent_this_hour: u64,
}

/// Error when spending would exceed budget.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("spending {amount} would exceed budget (remaining: {remaining})")]
pub struct BudgetExceededError {
    /// Amount attempting to spend.
    pub amount: u64,
    /// Remaining budget.
    pub remaining: u64,
}

impl SpendingTracker {
    /// Create a new tracker with the given hourly budget.
    #[must_use]
    pub const fn new(hourly_budget: u64) -> Self {
        Self {
            hourly_budget,
            spent_this_hour: 0,
        }
    }

    /// Get the amount spent this hour.
    #[must_use]
    pub const fn spent_this_hour(&self) -> u64 {
        self.spent_this_hour
    }

    /// Get the remaining budget for this hour.
    #[must_use]
    pub const fn remaining_budget(&self) -> u64 {
        self.hourly_budget.saturating_sub(self.spent_this_hour)
    }

    /// Check if we can afford to spend the given amount.
    #[must_use]
    pub const fn can_afford(&self, amount: u64) -> bool {
        amount <= self.remaining_budget()
    }

    /// Record a spend, returning error if it would exceed budget.
    pub fn record_spend(&mut self, amount: u64) -> Result<(), BudgetExceededError> {
        if !self.can_afford(amount) {
            return Err(BudgetExceededError {
                amount,
                remaining: self.remaining_budget(),
            });
        }
        self.spent_this_hour = self.spent_this_hour.saturating_add(amount);
        Ok(())
    }

    /// Reset the hourly spending counter (call at the start of each hour).
    pub fn reset_hour(&mut self) {
        self.spent_this_hour = 0;
    }

    /// Update the hourly budget.
    pub fn set_hourly_budget(&mut self, budget: u64) {
        self.hourly_budget = budget;
    }
}
