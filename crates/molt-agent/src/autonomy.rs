//! Autonomy mode definitions and decision-making logic.
//!
//! Defines the spectrum of autonomous behavior for agents:
//! - [`AutonomyMode`] — Conservative, Moderate, or Aggressive
//! - [`JobDecision`] — Outcome of evaluating a job opportunity
//! - [`Decision`] — Trait for mode-specific decision logic

use serde::{Deserialize, Serialize};

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
            Self::Conservative => 100,      // Very low auto-approval
            Self::Moderate => 10_000,       // Reasonable threshold
            Self::Aggressive => 1_000_000,  // High autonomy
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

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // AutonomyMode enum tests
    // ==========================================================================

    #[test]
    fn autonomy_mode_conservative_is_default() {
        let mode = AutonomyMode::default();
        assert_eq!(mode, AutonomyMode::Conservative);
    }

    #[test]
    fn autonomy_mode_has_three_variants() {
        let modes = [
            AutonomyMode::Conservative,
            AutonomyMode::Moderate,
            AutonomyMode::Aggressive,
        ];
        assert_eq!(modes.len(), 3);
    }

    #[test]
    fn autonomy_mode_serializes_to_lowercase() {
        let conservative = serde_json::to_string(&AutonomyMode::Conservative).unwrap();
        let moderate = serde_json::to_string(&AutonomyMode::Moderate).unwrap();
        let aggressive = serde_json::to_string(&AutonomyMode::Aggressive).unwrap();

        assert_eq!(conservative, "\"conservative\"");
        assert_eq!(moderate, "\"moderate\"");
        assert_eq!(aggressive, "\"aggressive\"");
    }

    #[test]
    fn autonomy_mode_deserializes_from_lowercase() {
        let conservative: AutonomyMode = serde_json::from_str("\"conservative\"").unwrap();
        let moderate: AutonomyMode = serde_json::from_str("\"moderate\"").unwrap();
        let aggressive: AutonomyMode = serde_json::from_str("\"aggressive\"").unwrap();

        assert_eq!(conservative, AutonomyMode::Conservative);
        assert_eq!(moderate, AutonomyMode::Moderate);
        assert_eq!(aggressive, AutonomyMode::Aggressive);
    }

    #[test]
    fn autonomy_mode_risk_tolerance_ordering() {
        assert!(AutonomyMode::Conservative.risk_tolerance() < AutonomyMode::Moderate.risk_tolerance());
        assert!(AutonomyMode::Moderate.risk_tolerance() < AutonomyMode::Aggressive.risk_tolerance());
    }

    #[test]
    fn autonomy_mode_is_copy_and_clone() {
        let mode = AutonomyMode::Moderate;
        let copied = mode;
        let cloned = mode.clone();
        assert_eq!(mode, copied);
        assert_eq!(mode, cloned);
    }

    #[test]
    fn autonomy_mode_max_auto_approve_ordering() {
        assert!(AutonomyMode::Conservative.max_auto_approve() < AutonomyMode::Moderate.max_auto_approve());
        assert!(AutonomyMode::Moderate.max_auto_approve() < AutonomyMode::Aggressive.max_auto_approve());
    }

    #[test]
    fn autonomy_mode_counter_offers() {
        assert!(!AutonomyMode::Conservative.allows_counter_offers());
        assert!(AutonomyMode::Moderate.allows_counter_offers());
        assert!(AutonomyMode::Aggressive.allows_counter_offers());
    }

    // ==========================================================================
    // JobDecision enum tests
    // ==========================================================================

    #[test]
    fn job_decision_accept_constructor() {
        let decision = JobDecision::accept();
        assert!(decision.is_accept());
        assert!(!decision.is_reject());
        assert!(!decision.needs_approval());
        assert!(!decision.is_counter_offer());
    }

    #[test]
    fn job_decision_reject_constructor() {
        let decision = JobDecision::reject("insufficient resources");
        assert!(decision.is_reject());
        assert!(!decision.is_accept());
        
        if let JobDecision::Reject { reason } = decision {
            assert_eq!(reason, "insufficient resources");
        } else {
            panic!("expected Reject variant");
        }
    }

    #[test]
    fn job_decision_need_approval_constructor() {
        let decision = JobDecision::need_approval("price exceeds threshold");
        assert!(decision.needs_approval());
        assert!(!decision.is_accept());
        
        if let JobDecision::NeedApproval { reason } = decision {
            assert_eq!(reason, "price exceeds threshold");
        } else {
            panic!("expected NeedApproval variant");
        }
    }

    #[test]
    fn job_decision_counter_offer_constructor() {
        let decision = JobDecision::counter_offer(1500, "price too low");
        assert!(decision.is_counter_offer());
        assert!(!decision.is_accept());
        
        if let JobDecision::CounterOffer { proposed_price, reason } = decision {
            assert_eq!(proposed_price, 1500);
            assert_eq!(reason, "price too low");
        } else {
            panic!("expected CounterOffer variant");
        }
    }

    #[test]
    fn job_decision_serializes_with_tag() {
        let accept = serde_json::to_string(&JobDecision::accept()).unwrap();
        assert!(accept.contains("\"decision\":\"accept\""));

        let reject = serde_json::to_string(&JobDecision::reject("test")).unwrap();
        assert!(reject.contains("\"decision\":\"reject\""));
        assert!(reject.contains("\"reason\":\"test\""));

        let counter = serde_json::to_string(&JobDecision::counter_offer(100, "low")).unwrap();
        assert!(counter.contains("\"decision\":\"counter_offer\""));
        assert!(counter.contains("\"proposed_price\":100"));
    }

    #[test]
    fn job_decision_deserializes_from_tag() {
        let accept: JobDecision = serde_json::from_str(r#"{"decision":"accept"}"#).unwrap();
        assert!(accept.is_accept());

        let reject: JobDecision = serde_json::from_str(r#"{"decision":"reject","reason":"test"}"#).unwrap();
        assert!(reject.is_reject());

        let need: JobDecision = serde_json::from_str(r#"{"decision":"need_approval","reason":"check"}"#).unwrap();
        assert!(need.needs_approval());
    }

    // ==========================================================================
    // DecisionThresholds tests
    // ==========================================================================

    #[test]
    fn decision_thresholds_default() {
        let thresholds = DecisionThresholds::default();
        assert_eq!(thresholds.max_auto_accept_price, 1000);
        assert_eq!(thresholds.min_reputation, 50);
        assert_eq!(thresholds.max_duration_secs, 3600);
    }

    #[test]
    fn decision_thresholds_for_conservative_mode() {
        let thresholds = DecisionThresholds::for_mode(AutonomyMode::Conservative);
        assert_eq!(thresholds.max_auto_accept_price, 100);
        assert_eq!(thresholds.min_reputation, 80);
        assert_eq!(thresholds.max_duration_secs, 600);
    }

    #[test]
    fn decision_thresholds_for_moderate_mode() {
        let thresholds = DecisionThresholds::for_mode(AutonomyMode::Moderate);
        assert_eq!(thresholds.max_auto_accept_price, 10_000);
        assert_eq!(thresholds.min_reputation, 50);
        assert_eq!(thresholds.max_duration_secs, 3600);
    }

    #[test]
    fn decision_thresholds_for_aggressive_mode() {
        let thresholds = DecisionThresholds::for_mode(AutonomyMode::Aggressive);
        assert_eq!(thresholds.max_auto_accept_price, 1_000_000);
        assert_eq!(thresholds.min_reputation, 20);
        assert_eq!(thresholds.max_duration_secs, 86400);
    }

    #[test]
    fn decision_thresholds_serialization_roundtrip() {
        let original = DecisionThresholds {
            max_auto_accept_price: 5000,
            min_reputation: 75,
            max_duration_secs: 7200,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: DecisionThresholds = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }
}
