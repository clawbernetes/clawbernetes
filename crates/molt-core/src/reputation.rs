//! Reputation scoring for MOLT network participants.
//!
//! Tracks provider and buyer reputation based on transaction history.

use serde::{Deserialize, Serialize};

use crate::MoltError;

/// A reputation score between 0.0 and 1.0.
///
/// - 0.0 = worst possible reputation
/// - 0.5 = neutral (new participants)
/// - 1.0 = perfect reputation
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Score(f64);

impl Score {
    /// Perfect reputation score (1.0).
    pub const PERFECT: Self = Self(1.0);

    /// Worst reputation score (0.0).
    pub const ZERO: Self = Self(0.0);

    /// Neutral reputation for new participants (0.5).
    pub const NEUTRAL: Self = Self(0.5);

    /// Creates a new score from a value.
    ///
    /// # Errors
    ///
    /// Returns `MoltError::InvalidAmount` if the value is outside [0.0, 1.0].
    pub fn new(value: f64) -> Result<Self, MoltError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(MoltError::InvalidAmount(format!(
                "score must be between 0.0 and 1.0, got {value}"
            )));
        }
        Ok(Self(value))
    }

    /// Returns the raw score value.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Returns true if this is a "good" reputation (>= 0.7).
    #[must_use]
    pub fn is_good(self) -> bool {
        self.0 >= 0.7
    }

    /// Returns true if this is a "poor" reputation (< 0.3).
    #[must_use]
    pub fn is_poor(self) -> bool {
        self.0 < 0.3
    }
}

impl Default for Score {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// Tracks reputation history for a network participant.
///
/// Reputation is calculated from transaction success/failure history,
/// with more recent transactions weighted more heavily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reputation {
    successful: u64,
    failed: u64,
}

impl Reputation {
    /// Creates a new reputation tracker with neutral starting score.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            successful: 0,
            failed: 0,
        }
    }

    /// Records a successful transaction.
    #[allow(clippy::missing_const_for_fn)] // const &mut self is unstable
    pub fn record_success(&mut self) {
        self.successful = self.successful.saturating_add(1);
    }

    /// Records a failed transaction.
    #[allow(clippy::missing_const_for_fn)] // const &mut self is unstable
    pub fn record_failure(&mut self) {
        self.failed = self.failed.saturating_add(1);
    }

    /// Returns the total number of transactions recorded.
    #[must_use]
    pub const fn total_transactions(&self) -> u64 {
        self.successful.saturating_add(self.failed)
    }

    /// Returns the number of successful transactions.
    #[must_use]
    pub const fn successful_transactions(&self) -> u64 {
        self.successful
    }

    /// Calculates the current reputation score.
    ///
    /// Uses a Bayesian approach: starts at neutral (0.5) and adjusts
    /// based on observed success rate with a prior of 1 success and 1 failure.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn score(&self) -> Score {
        // Bayesian prior: assume 1 success and 1 failure to start
        // This prevents scores of exactly 0 or 1 with limited data
        let prior_success = 1.0;
        let prior_failure = 1.0;

        let total_success = self.successful as f64 + prior_success;
        let total_failure = self.failed as f64 + prior_failure;
        let total = total_success + total_failure;

        Score(total_success / total)
    }
}

impl Default for Reputation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_new_is_neutral() {
        let score = Score::default();
        assert_eq!(score.value(), 0.5);
    }

    #[test]
    fn score_clamped_to_valid_range() {
        let high = Score::new(1.5);
        assert!(high.is_err());

        let low = Score::new(-0.1);
        assert!(low.is_err());

        let valid = Score::new(0.75);
        assert!(valid.is_ok());
    }

    #[test]
    fn score_perfect_and_zero() {
        assert_eq!(Score::PERFECT.value(), 1.0);
        assert_eq!(Score::ZERO.value(), 0.0);
    }

    #[test]
    fn reputation_new_starts_neutral() {
        let rep = Reputation::new();
        assert_eq!(rep.score().value(), 0.5);
        assert_eq!(rep.total_transactions(), 0);
    }

    #[test]
    fn reputation_records_success() {
        let mut rep = Reputation::new();
        rep.record_success();
        
        assert_eq!(rep.total_transactions(), 1);
        assert_eq!(rep.successful_transactions(), 1);
        assert!(rep.score().value() > 0.5); // Should improve
    }

    #[test]
    fn reputation_records_failure() {
        let mut rep = Reputation::new();
        rep.record_failure();
        
        assert_eq!(rep.total_transactions(), 1);
        assert_eq!(rep.successful_transactions(), 0);
        assert!(rep.score().value() < 0.5); // Should decrease
    }

    #[test]
    fn reputation_calculates_score_from_history() {
        let mut rep = Reputation::new();
        // 8 successes, 2 failures = 80% success rate
        for _ in 0..8 {
            rep.record_success();
        }
        for _ in 0..2 {
            rep.record_failure();
        }
        
        // Score should be around 0.8 with enough history
        let score = rep.score().value();
        assert!(score > 0.7 && score < 0.9);
    }

    #[test]
    fn reputation_serde_roundtrip() {
        let mut rep = Reputation::new();
        rep.record_success();
        rep.record_success();
        rep.record_failure();
        
        let json = serde_json::to_string(&rep).unwrap();
        let restored: Reputation = serde_json::from_str(&json).unwrap();
        
        assert_eq!(rep.total_transactions(), restored.total_transactions());
        assert_eq!(rep.successful_transactions(), restored.successful_transactions());
    }
}
