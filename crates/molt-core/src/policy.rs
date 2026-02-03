//! Autonomy policy definitions for MOLT participation.
//!
//! Policies define how agents participate in the MOLT network,
//! including spending limits, job filters, and risk tolerance.

use serde::{Deserialize, Serialize};

use crate::Amount;

/// Defines the level of autonomy an agent has in decision-making.
///
/// Higher autonomy levels allow more aggressive bidding, higher spending,
/// and acceptance of riskier jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub enum AutonomyLevel {
    /// Minimal autonomy. Only accepts low-risk, pre-approved job types.
    /// Requires confirmation for spending above minimal thresholds.
    #[default]
    Conservative,
    /// Balanced autonomy. Accepts most job types within budget.
    /// Can make routine spending decisions independently.
    Moderate,
    /// Maximum autonomy. Accepts any job within technical capability.
    /// Can make significant spending decisions independently.
    Aggressive,
}

/// A policy governing agent behavior in the MOLT network.
///
/// Policies control:
/// - Maximum spending per job
/// - Allowed job types
/// - Pricing rules and strategies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Policy {
    autonomy_level: AutonomyLevel,
    max_spend_per_job: Option<Amount>,
    allowed_job_types: Option<Vec<String>>,
}

impl Policy {
    /// Creates a new policy builder.
    #[must_use]
    pub fn builder() -> PolicyBuilder {
        PolicyBuilder::default()
    }

    /// Returns the autonomy level of this policy.
    #[must_use]
    pub const fn autonomy_level(&self) -> AutonomyLevel {
        self.autonomy_level
    }

    /// Returns the maximum spend per job, if set.
    #[must_use]
    pub const fn max_spend_per_job(&self) -> Option<Amount> {
        self.max_spend_per_job
    }

    /// Checks if the given amount is within the spending limit.
    ///
    /// Returns `true` if no limit is set or if the amount is within the limit.
    #[must_use]
    pub fn allows_spend(&self, amount: Amount) -> bool {
        self.max_spend_per_job.is_none_or(|max| amount <= max)
    }

    /// Checks if a job type is allowed by this policy.
    ///
    /// Returns `true` if no filter is set or if the job type is in the allowed list.
    #[must_use]
    pub fn allows_job_type(&self, job_type: &str) -> bool {
        self.allowed_job_types
            .as_ref()
            .is_none_or(|allowed| allowed.iter().any(|t| t == job_type))
    }
}

/// Builder for constructing [`Policy`] instances.
#[derive(Debug, Clone, Default)]
pub struct PolicyBuilder {
    autonomy_level: AutonomyLevel,
    max_spend_per_job: Option<Amount>,
    allowed_job_types: Option<Vec<String>>,
}

impl PolicyBuilder {
    /// Sets the autonomy level.
    #[must_use]
    pub const fn autonomy_level(mut self, level: AutonomyLevel) -> Self {
        self.autonomy_level = level;
        self
    }

    /// Sets the maximum spend per job.
    #[must_use]
    pub const fn max_spend_per_job(mut self, amount: Amount) -> Self {
        self.max_spend_per_job = Some(amount);
        self
    }

    /// Sets the allowed job types.
    #[must_use]
    pub fn allowed_job_types(mut self, types: Vec<String>) -> Self {
        self.allowed_job_types = Some(types);
        self
    }

    /// Builds the policy.
    #[must_use]
    pub fn build(self) -> Policy {
        Policy {
            autonomy_level: self.autonomy_level,
            max_spend_per_job: self.max_spend_per_job,
            allowed_job_types: self.allowed_job_types,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autonomy_level_default_is_conservative() {
        let level = AutonomyLevel::default();
        assert_eq!(level, AutonomyLevel::Conservative);
    }

    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::Conservative < AutonomyLevel::Moderate);
        assert!(AutonomyLevel::Moderate < AutonomyLevel::Aggressive);
    }

    #[test]
    fn policy_default_is_conservative() {
        let policy = Policy::default();
        assert_eq!(policy.autonomy_level(), AutonomyLevel::Conservative);
    }

    #[test]
    fn policy_with_spending_limit() {
        let policy = Policy::builder()
            .autonomy_level(AutonomyLevel::Moderate)
            .max_spend_per_job(Amount::from_molt(10))
            .build();
        
        assert_eq!(policy.max_spend_per_job(), Some(Amount::from_molt(10)));
    }

    #[test]
    fn policy_allows_job_within_budget() {
        let policy = Policy::builder()
            .max_spend_per_job(Amount::from_molt(100))
            .build();
        
        assert!(policy.allows_spend(Amount::from_molt(50)));
    }

    #[test]
    fn policy_rejects_job_over_budget() {
        let policy = Policy::builder()
            .max_spend_per_job(Amount::from_molt(100))
            .build();
        
        assert!(!policy.allows_spend(Amount::from_molt(150)));
    }

    #[test]
    fn policy_with_job_filter() {
        let policy = Policy::builder()
            .allowed_job_types(vec!["compute".into(), "storage".into()])
            .build();
        
        assert!(policy.allows_job_type("compute"));
        assert!(policy.allows_job_type("storage"));
        assert!(!policy.allows_job_type("network"));
    }

    #[test]
    fn policy_without_filter_allows_all() {
        let policy = Policy::default();
        assert!(policy.allows_job_type("anything"));
    }

    #[test]
    fn policy_serde_roundtrip() {
        let policy = Policy::builder()
            .autonomy_level(AutonomyLevel::Aggressive)
            .max_spend_per_job(Amount::from_molt(500))
            .build();
        
        let json = serde_json::to_string(&policy).unwrap();
        let restored: Policy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy.autonomy_level(), restored.autonomy_level());
    }
}
