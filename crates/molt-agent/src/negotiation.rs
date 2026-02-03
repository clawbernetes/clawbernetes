//! Multi-agent negotiation protocol.
//!
//! Handles bidding, counter-offers, and provider selection for jobs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::provider::ProviderId;

/// Unique identifier for a negotiation session.
pub type NegotiationId = Uuid;

/// A bid from a provider for a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bid {
    /// Unique identifier for this bid.
    pub id: Uuid,
    /// Provider making the bid.
    pub provider: ProviderId,
    /// Bid price (in base units).
    pub price: u64,
    /// When the provider can start the job.
    pub available_at: DateTime<Utc>,
    /// Bid expiration time.
    pub expires_at: DateTime<Utc>,
    /// Provider's reputation at bid time.
    pub reputation: u8,
}

impl Bid {
    /// Creates a new bid.
    #[must_use]
    pub fn new(
        provider: ProviderId,
        price: u64,
        available_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        reputation: u8,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            provider,
            price,
            available_at,
            expires_at,
            reputation,
        }
    }

    /// Returns true if the bid has expired.
    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Returns true if the provider is available now.
    #[must_use]
    pub fn is_available(&self, now: DateTime<Utc>) -> bool {
        now >= self.available_at
    }

    /// Returns the wait time until availability in seconds.
    #[must_use]
    pub fn wait_time_secs(&self, now: DateTime<Utc>) -> i64 {
        (self.available_at - now).num_seconds().max(0)
    }
}

/// A selected bid result from negotiation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedBid {
    /// The winning bid.
    pub bid: Bid,
    /// Score used for selection (higher is better).
    pub score: f64,
    /// Reason for selection.
    pub reason: String,
}

impl SelectedBid {
    /// Creates a new selected bid.
    #[must_use]
    pub fn new(bid: Bid, score: f64, reason: impl Into<String>) -> Self {
        Self {
            bid,
            score,
            reason: reason.into(),
        }
    }
}

/// Strategy for negotiating and selecting bids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegotiationStrategy {
    /// Select the lowest price bid.
    LowestPrice,
    /// Select the highest reputation provider.
    HighestReputation,
    /// Select the bid available soonest.
    FastestAvailability,
    /// Balance price, reputation, and availability.
    Balanced,
}

impl Default for NegotiationStrategy {
    fn default() -> Self {
        Self::Balanced
    }
}

impl NegotiationStrategy {
    /// Score a bid according to this strategy.
    ///
    /// Returns a score where higher is better.
    #[must_use]
    pub fn score_bid(&self, bid: &Bid, max_price: u64, now: DateTime<Utc>) -> f64 {
        match self {
            Self::LowestPrice => {
                if max_price == 0 {
                    0.0
                } else {
                    1.0 - (bid.price as f64 / max_price as f64).min(1.0)
                }
            }
            Self::HighestReputation => bid.reputation as f64 / 100.0,
            Self::FastestAvailability => {
                let wait = bid.wait_time_secs(now) as f64;
                // Invert wait time: 0 wait = score 1.0, higher wait = lower score
                1.0 / (1.0 + wait / 3600.0) // Normalize by 1 hour
            }
            Self::Balanced => {
                // Weighted combination
                let price_score = if max_price == 0 {
                    0.0
                } else {
                    1.0 - (bid.price as f64 / max_price as f64).min(1.0)
                };
                let reputation_score = bid.reputation as f64 / 100.0;
                let wait = bid.wait_time_secs(now) as f64;
                let availability_score = 1.0 / (1.0 + wait / 3600.0);

                price_score * 0.4 + reputation_score * 0.35 + availability_score * 0.25
            }
        }
    }
}

/// Job specification for negotiation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NegotiationJob {
    /// Unique job identifier.
    pub id: Uuid,
    /// Resources required.
    pub resources: u64,
    /// Maximum acceptable price.
    pub max_price: u64,
    /// Minimum acceptable reputation.
    pub min_reputation: u8,
}

impl NegotiationJob {
    /// Creates a new negotiation job specification.
    #[must_use]
    pub fn new(resources: u64, max_price: u64, min_reputation: u8) -> Self {
        Self {
            id: Uuid::new_v4(),
            resources,
            max_price,
            min_reputation,
        }
    }
}

/// Negotiate among bids and select the best one.
///
/// Returns None if no acceptable bid is found.
#[must_use]
pub fn negotiate(
    job: &NegotiationJob,
    bids: &[Bid],
    strategy: NegotiationStrategy,
) -> Option<SelectedBid> {
    negotiate_at(job, bids, strategy, Utc::now())
}

/// Negotiate among bids at a specific time (for testing).
#[must_use]
pub fn negotiate_at(
    job: &NegotiationJob,
    bids: &[Bid],
    strategy: NegotiationStrategy,
    now: DateTime<Utc>,
) -> Option<SelectedBid> {
    // Filter valid bids
    let valid_bids: Vec<&Bid> = bids
        .iter()
        .filter(|b| !b.is_expired(now))
        .filter(|b| b.price <= job.max_price)
        .filter(|b| b.reputation >= job.min_reputation)
        .collect();

    if valid_bids.is_empty() {
        return None;
    }

    // Score and select best
    let best = valid_bids
        .iter()
        .map(|b| {
            let score = strategy.score_bid(b, job.max_price, now);
            (*b, score)
        })
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    best.map(|(bid, score)| {
        let reason = match strategy {
            NegotiationStrategy::LowestPrice => "lowest price bid",
            NegotiationStrategy::HighestReputation => "highest reputation provider",
            NegotiationStrategy::FastestAvailability => "fastest availability",
            NegotiationStrategy::Balanced => "best balanced score",
        };
        SelectedBid::new(bid.clone(), score, reason)
    })
}

/// State of an ongoing negotiation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NegotiationState {
    /// Negotiation session ID.
    pub id: NegotiationId,
    /// Job being negotiated.
    pub job: NegotiationJob,
    /// Collected bids.
    pub bids: Vec<Bid>,
    /// Current phase.
    pub phase: NegotiationPhase,
    /// Selected bid (if any).
    pub selected: Option<SelectedBid>,
    /// When negotiation started.
    pub started_at: DateTime<Utc>,
    /// Deadline for bids.
    pub deadline: DateTime<Utc>,
}

/// Phase of a negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegotiationPhase {
    /// Collecting bids from providers.
    CollectingBids,
    /// Evaluating bids.
    Evaluating,
    /// Negotiation completed with selection.
    Completed,
    /// Negotiation failed (no valid bids).
    Failed,
    /// Negotiation cancelled.
    Cancelled,
}

impl NegotiationState {
    /// Creates a new negotiation state.
    #[must_use]
    pub fn new(job: NegotiationJob, deadline: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            job,
            bids: Vec::new(),
            phase: NegotiationPhase::CollectingBids,
            selected: None,
            started_at: Utc::now(),
            deadline,
        }
    }

    /// Add a bid to the negotiation.
    pub fn add_bid(&mut self, bid: Bid) -> Result<(), NegotiationError> {
        if self.phase != NegotiationPhase::CollectingBids {
            return Err(NegotiationError::NotCollectingBids);
        }
        self.bids.push(bid);
        Ok(())
    }

    /// Finalize negotiation and select the best bid.
    pub fn finalize(&mut self, strategy: NegotiationStrategy) -> Result<Option<&SelectedBid>, NegotiationError> {
        if self.phase != NegotiationPhase::CollectingBids {
            return Err(NegotiationError::AlreadyFinalized);
        }

        self.phase = NegotiationPhase::Evaluating;
        self.selected = negotiate(&self.job, &self.bids, strategy);

        if self.selected.is_some() {
            self.phase = NegotiationPhase::Completed;
        } else {
            self.phase = NegotiationPhase::Failed;
        }

        Ok(self.selected.as_ref())
    }

    /// Cancel the negotiation.
    pub fn cancel(&mut self) {
        self.phase = NegotiationPhase::Cancelled;
    }

    /// Returns true if negotiation is still active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(
            self.phase,
            NegotiationPhase::CollectingBids | NegotiationPhase::Evaluating
        )
    }

    /// Returns the number of bids collected.
    #[must_use]
    pub fn bid_count(&self) -> usize {
        self.bids.len()
    }
}

/// Errors during negotiation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NegotiationError {
    /// Cannot add bids when not in collecting phase.
    #[error("negotiation is not in collecting bids phase")]
    NotCollectingBids,
    /// Cannot finalize an already finalized negotiation.
    #[error("negotiation already finalized")]
    AlreadyFinalized,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn make_bid(provider: ProviderId, price: u64, reputation: u8, wait_mins: i64) -> Bid {
        let n = now();
        Bid::new(
            provider,
            price,
            n + Duration::minutes(wait_mins),
            n + Duration::hours(1),
            reputation,
        )
    }

    // ==========================================================================
    // Bid tests
    // ==========================================================================

    #[test]
    fn bid_new() {
        let provider = Uuid::new_v4();
        let n = now();
        let bid = Bid::new(provider, 1000, n, n + Duration::hours(1), 80);
        
        assert_eq!(bid.provider, provider);
        assert_eq!(bid.price, 1000);
        assert_eq!(bid.reputation, 80);
    }

    #[test]
    fn bid_is_expired() {
        let n = now();
        let bid = Bid::new(Uuid::new_v4(), 1000, n, n + Duration::hours(1), 80);
        
        assert!(!bid.is_expired(n));
        assert!(!bid.is_expired(n + Duration::minutes(30)));
        assert!(bid.is_expired(n + Duration::hours(1)));
        assert!(bid.is_expired(n + Duration::hours(2)));
    }

    #[test]
    fn bid_is_available() {
        let n = now();
        let bid = Bid::new(Uuid::new_v4(), 1000, n + Duration::minutes(30), n + Duration::hours(1), 80);
        
        assert!(!bid.is_available(n));
        assert!(bid.is_available(n + Duration::minutes(30)));
        assert!(bid.is_available(n + Duration::hours(1)));
    }

    #[test]
    fn bid_wait_time() {
        let n = now();
        let bid = Bid::new(Uuid::new_v4(), 1000, n + Duration::minutes(30), n + Duration::hours(1), 80);
        
        assert_eq!(bid.wait_time_secs(n), 30 * 60);
        assert_eq!(bid.wait_time_secs(n + Duration::minutes(30)), 0);
        assert_eq!(bid.wait_time_secs(n + Duration::hours(1)), 0);
    }

    // ==========================================================================
    // NegotiationStrategy tests
    // ==========================================================================

    #[test]
    fn strategy_lowest_price() {
        let n = now();
        let low = make_bid(Uuid::new_v4(), 100, 50, 0);
        let high = make_bid(Uuid::new_v4(), 500, 50, 0);
        
        let low_score = NegotiationStrategy::LowestPrice.score_bid(&low, 1000, n);
        let high_score = NegotiationStrategy::LowestPrice.score_bid(&high, 1000, n);
        
        assert!(low_score > high_score);
    }

    #[test]
    fn strategy_highest_reputation() {
        let n = now();
        let high_rep = make_bid(Uuid::new_v4(), 500, 90, 0);
        let low_rep = make_bid(Uuid::new_v4(), 500, 50, 0);
        
        let high_score = NegotiationStrategy::HighestReputation.score_bid(&high_rep, 1000, n);
        let low_score = NegotiationStrategy::HighestReputation.score_bid(&low_rep, 1000, n);
        
        assert!(high_score > low_score);
    }

    #[test]
    fn strategy_fastest_availability() {
        let n = now();
        let fast = make_bid(Uuid::new_v4(), 500, 50, 0);  // Available now
        let slow = make_bid(Uuid::new_v4(), 500, 50, 60); // 1 hour wait
        
        let fast_score = NegotiationStrategy::FastestAvailability.score_bid(&fast, 1000, n);
        let slow_score = NegotiationStrategy::FastestAvailability.score_bid(&slow, 1000, n);
        
        assert!(fast_score > slow_score);
    }

    #[test]
    fn strategy_balanced() {
        let n = now();
        // Good balance: medium price, high rep, available soon
        let balanced = make_bid(Uuid::new_v4(), 300, 80, 5);
        // Cheap but bad rep and slow
        let cheap = make_bid(Uuid::new_v4(), 100, 30, 60);
        
        let balanced_score = NegotiationStrategy::Balanced.score_bid(&balanced, 1000, n);
        let cheap_score = NegotiationStrategy::Balanced.score_bid(&cheap, 1000, n);
        
        // Balanced should generally win due to better reputation
        assert!(balanced_score > cheap_score);
    }

    // ==========================================================================
    // negotiate tests
    // ==========================================================================

    #[test]
    fn negotiate_selects_lowest_price() {
        let job = NegotiationJob::new(100, 1000, 50);
        let n = now();
        
        let bids = vec![
            make_bid(Uuid::new_v4(), 800, 60, 0),
            make_bid(Uuid::new_v4(), 500, 60, 0), // Cheapest
            make_bid(Uuid::new_v4(), 700, 60, 0),
        ];
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
        assert!(result.is_some());
        assert_eq!(result.unwrap().bid.price, 500);
    }

    #[test]
    fn negotiate_selects_highest_reputation() {
        let job = NegotiationJob::new(100, 1000, 50);
        let n = now();
        
        let bids = vec![
            make_bid(Uuid::new_v4(), 600, 70, 0),
            make_bid(Uuid::new_v4(), 600, 90, 0), // Highest rep
            make_bid(Uuid::new_v4(), 600, 60, 0),
        ];
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::HighestReputation, n);
        assert!(result.is_some());
        assert_eq!(result.unwrap().bid.reputation, 90);
    }

    #[test]
    fn negotiate_filters_expired_bids() {
        let job = NegotiationJob::new(100, 1000, 50);
        let n = now();
        
        let mut expired = make_bid(Uuid::new_v4(), 400, 60, 0);
        expired.expires_at = n - Duration::minutes(1); // Already expired
        
        let valid = make_bid(Uuid::new_v4(), 500, 60, 0);
        
        let bids = vec![expired, valid.clone()];
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
        assert!(result.is_some());
        assert_eq!(result.unwrap().bid.price, 500); // Should pick valid, not expired
    }

    #[test]
    fn negotiate_filters_over_price() {
        let job = NegotiationJob::new(100, 500, 50);
        let n = now();
        
        let bids = vec![
            make_bid(Uuid::new_v4(), 600, 80, 0), // Over max
            make_bid(Uuid::new_v4(), 700, 90, 0), // Over max
        ];
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
        assert!(result.is_none());
    }

    #[test]
    fn negotiate_filters_low_reputation() {
        let job = NegotiationJob::new(100, 1000, 70);
        let n = now();
        
        let bids = vec![
            make_bid(Uuid::new_v4(), 400, 50, 0), // Below min rep
            make_bid(Uuid::new_v4(), 400, 60, 0), // Below min rep
        ];
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
        assert!(result.is_none());
    }

    #[test]
    fn negotiate_empty_bids() {
        let job = NegotiationJob::new(100, 1000, 50);
        let bids: Vec<Bid> = vec![];
        let n = now();
        
        let result = negotiate_at(&job, &bids, NegotiationStrategy::Balanced, n);
        assert!(result.is_none());
    }

    // ==========================================================================
    // NegotiationState tests
    // ==========================================================================

    #[test]
    fn negotiation_state_new() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let state = NegotiationState::new(job.clone(), deadline);
        
        assert_eq!(state.job, job);
        assert_eq!(state.phase, NegotiationPhase::CollectingBids);
        assert!(state.selected.is_none());
        assert_eq!(state.bid_count(), 0);
        assert!(state.is_active());
    }

    #[test]
    fn negotiation_state_add_bid() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        let bid = make_bid(Uuid::new_v4(), 500, 70, 0);
        let result = state.add_bid(bid);
        
        assert!(result.is_ok());
        assert_eq!(state.bid_count(), 1);
    }

    #[test]
    fn negotiation_state_finalize_success() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();
        state.add_bid(make_bid(Uuid::new_v4(), 400, 80, 0)).unwrap();
        
        let result = state.finalize(NegotiationStrategy::LowestPrice);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
        assert_eq!(state.phase, NegotiationPhase::Completed);
        assert!(!state.is_active());
    }

    #[test]
    fn negotiation_state_finalize_no_valid_bids() {
        let job = NegotiationJob::new(100, 100, 90); // Very strict requirements
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        // Add bids that don't meet requirements
        state.add_bid(make_bid(Uuid::new_v4(), 500, 50, 0)).unwrap(); // Over price, low rep
        
        let result = state.finalize(NegotiationStrategy::Balanced);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        assert_eq!(state.phase, NegotiationPhase::Failed);
    }

    #[test]
    fn negotiation_state_cannot_add_after_finalize() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();
        state.finalize(NegotiationStrategy::Balanced).unwrap();
        
        let result = state.add_bid(make_bid(Uuid::new_v4(), 400, 80, 0));
        assert!(matches!(result, Err(NegotiationError::NotCollectingBids)));
    }

    #[test]
    fn negotiation_state_cannot_finalize_twice() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();
        state.finalize(NegotiationStrategy::Balanced).unwrap();
        
        let result = state.finalize(NegotiationStrategy::LowestPrice);
        assert!(matches!(result, Err(NegotiationError::AlreadyFinalized)));
    }

    #[test]
    fn negotiation_state_cancel() {
        let job = NegotiationJob::new(100, 1000, 50);
        let deadline = now() + Duration::hours(1);
        let mut state = NegotiationState::new(job, deadline);
        
        state.cancel();
        assert_eq!(state.phase, NegotiationPhase::Cancelled);
        assert!(!state.is_active());
    }

    #[test]
    fn negotiation_job_serialization_roundtrip() {
        let job = NegotiationJob::new(100, 1000, 50);
        let json = serde_json::to_string(&job).unwrap();
        let parsed: NegotiationJob = serde_json::from_str(&json).unwrap();
        assert_eq!(job.resources, parsed.resources);
        assert_eq!(job.max_price, parsed.max_price);
    }

    #[test]
    fn bid_serialization_roundtrip() {
        let bid = make_bid(Uuid::new_v4(), 500, 70, 10);
        let json = serde_json::to_string(&bid).unwrap();
        let parsed: Bid = serde_json::from_str(&json).unwrap();
        assert_eq!(bid.price, parsed.price);
        assert_eq!(bid.reputation, parsed.reputation);
    }
}
