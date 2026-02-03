//! Integration tests for the negotiation flow.
//!
//! Tests multi-provider bidding and selection:
//! 1. Multiple providers bid on a job
//! 2. Buyer agent evaluates offers
//! 3. Negotiation selects best bid based on strategy
//! 4. Tests all three autonomy modes

use chrono::{Duration, Utc};
use molt_agent::{
    AutonomyMode, Bid, BuyerPolicy, JobDecision, JobRequirements as BuyerJobRequirements,
    JobSpec, NegotiationJob, NegotiationPhase, NegotiationState, NegotiationStrategy,
    ProviderOffer, ProviderPolicy, ProviderState,
    evaluate_job, evaluate_job_with_state, evaluate_offer, negotiate_at,
    score_offer, select_best_offer,
};
use uuid::Uuid;

// ============================================================================
// Helper Functions
// ============================================================================

fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

fn make_bid(provider: Uuid, price: u64, reputation: u8, wait_mins: i64) -> Bid {
    let n = now();
    Bid::new(
        provider,
        price,
        n + Duration::minutes(wait_mins),
        n + Duration::hours(1),
        reputation,
    )
}

fn make_provider_offer(
    provider_id: Uuid,
    price: u64,
    reputation: u8,
    duration_secs: u64,
) -> ProviderOffer {
    ProviderOffer::new(provider_id, price, duration_secs, reputation, 300)
}

// ============================================================================
// Multi-Provider Bidding Tests
// ============================================================================

#[test]
fn multiple_providers_submit_bids() {
    let provider1 = Uuid::new_v4();
    let provider2 = Uuid::new_v4();
    let provider3 = Uuid::new_v4();

    let bids = vec![
        make_bid(provider1, 500, 85, 0),  // High rep, immediate
        make_bid(provider2, 400, 70, 30), // Lower price, 30 min wait
        make_bid(provider3, 450, 90, 15), // Highest rep, 15 min wait
    ];

    assert_eq!(bids.len(), 3);
    assert_eq!(bids[0].provider, provider1);
    assert_eq!(bids[1].provider, provider2);
    assert_eq!(bids[2].provider, provider3);
}

#[test]
fn bid_expiration_tracking() {
    let provider = Uuid::new_v4();
    let n = now();

    // Bid that expires in 1 hour
    let bid = Bid::new(provider, 500, n, n + Duration::hours(1), 80);

    assert!(!bid.is_expired(n));
    assert!(!bid.is_expired(n + Duration::minutes(30)));
    assert!(bid.is_expired(n + Duration::hours(1)));
    assert!(bid.is_expired(n + Duration::hours(2)));
}

#[test]
fn bid_availability_tracking() {
    let provider = Uuid::new_v4();
    let n = now();

    // Bid available in 30 minutes
    let bid = Bid::new(
        provider,
        500,
        n + Duration::minutes(30),
        n + Duration::hours(2),
        80,
    );

    assert!(!bid.is_available(n));
    assert!(!bid.is_available(n + Duration::minutes(15)));
    assert!(bid.is_available(n + Duration::minutes(30)));
    assert!(bid.is_available(n + Duration::hours(1)));
}

#[test]
fn bid_wait_time_calculation() {
    let provider = Uuid::new_v4();
    let n = now();

    let bid = Bid::new(
        provider,
        500,
        n + Duration::minutes(45),
        n + Duration::hours(2),
        80,
    );

    assert_eq!(bid.wait_time_secs(n), 45 * 60);
    assert_eq!(bid.wait_time_secs(n + Duration::minutes(30)), 15 * 60);
    assert_eq!(bid.wait_time_secs(n + Duration::minutes(45)), 0);
    assert_eq!(bid.wait_time_secs(n + Duration::hours(1)), 0);
}

// ============================================================================
// Negotiation Strategy Tests
// ============================================================================

#[test]
fn strategy_lowest_price_selects_cheapest() {
    let job = NegotiationJob::new(100, 1000, 50);
    let n = now();

    let bids = vec![
        make_bid(Uuid::new_v4(), 800, 60, 0),
        make_bid(Uuid::new_v4(), 400, 60, 0), // Cheapest
        make_bid(Uuid::new_v4(), 600, 60, 0),
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.price, 400);
}

#[test]
fn strategy_highest_reputation_selects_best_rep() {
    let job = NegotiationJob::new(100, 1000, 50);
    let n = now();

    let bids = vec![
        make_bid(Uuid::new_v4(), 500, 70, 0),
        make_bid(Uuid::new_v4(), 500, 95, 0), // Highest rep
        make_bid(Uuid::new_v4(), 500, 60, 0),
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::HighestReputation, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.reputation, 95);
}

#[test]
fn strategy_fastest_availability_selects_immediate() {
    let job = NegotiationJob::new(100, 1000, 50);
    let n = now();

    let bids = vec![
        make_bid(Uuid::new_v4(), 500, 70, 60),  // 1 hour wait
        make_bid(Uuid::new_v4(), 500, 70, 0),   // Immediate
        make_bid(Uuid::new_v4(), 500, 70, 30),  // 30 min wait
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::FastestAvailability, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.wait_time_secs(n), 0);
}

#[test]
fn strategy_balanced_considers_all_factors() {
    let job = NegotiationJob::new(100, 1000, 50);
    let n = now();

    let bids = vec![
        // Cheap but low rep and slow
        make_bid(Uuid::new_v4(), 200, 50, 60),
        // Expensive but high rep and fast
        make_bid(Uuid::new_v4(), 800, 95, 0),
        // Balanced: medium price, good rep, fast
        make_bid(Uuid::new_v4(), 400, 85, 5),
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::Balanced, n);
    assert!(result.is_some());

    // The balanced option should win
    let selected = result.unwrap();
    assert_eq!(selected.bid.price, 400);
    assert_eq!(selected.bid.reputation, 85);
}

#[test]
fn negotiation_filters_expired_bids() {
    let job = NegotiationJob::new(100, 1000, 50);
    let n = now();

    let provider_valid = Uuid::new_v4();

    let mut expired_bid = make_bid(Uuid::new_v4(), 300, 80, 0); // Would be best price
    expired_bid.expires_at = n - Duration::minutes(1); // But it's expired

    let valid_bid = make_bid(provider_valid, 500, 70, 0);

    let bids = vec![expired_bid, valid_bid];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
    assert!(result.is_some());
    
    let selected = result.unwrap();
    assert_eq!(selected.bid.provider, provider_valid);
    assert_eq!(selected.bid.price, 500);
}

#[test]
fn negotiation_filters_over_budget_bids() {
    let job = NegotiationJob::new(100, 500, 50); // Max price 500
    let n = now();

    let provider_valid = Uuid::new_v4();

    let bids = vec![
        make_bid(Uuid::new_v4(), 600, 90, 0), // Over budget
        make_bid(Uuid::new_v4(), 800, 95, 0), // Over budget
        make_bid(provider_valid, 450, 70, 0), // Within budget
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::HighestReputation, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.provider, provider_valid);
}

#[test]
fn negotiation_filters_low_reputation_bids() {
    let job = NegotiationJob::new(100, 1000, 70); // Min reputation 70
    let n = now();

    let provider_valid = Uuid::new_v4();

    let bids = vec![
        make_bid(Uuid::new_v4(), 300, 50, 0), // Low rep
        make_bid(Uuid::new_v4(), 400, 60, 0), // Low rep
        make_bid(provider_valid, 500, 75, 0), // Meets requirement
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.provider, provider_valid);
}

#[test]
fn negotiation_returns_none_when_no_valid_bids() {
    let job = NegotiationJob::new(100, 100, 90); // Very strict requirements
    let n = now();

    let bids = vec![
        make_bid(Uuid::new_v4(), 500, 50, 0), // Over price, low rep
        make_bid(Uuid::new_v4(), 200, 60, 0), // Over price, low rep
    ];

    let result = negotiate_at(&job, &bids, NegotiationStrategy::Balanced, n);
    assert!(result.is_none());
}

#[test]
fn negotiation_returns_none_for_empty_bids() {
    let job = NegotiationJob::new(100, 1000, 50);
    let bids: Vec<Bid> = vec![];
    let n = now();

    let result = negotiate_at(&job, &bids, NegotiationStrategy::Balanced, n);
    assert!(result.is_none());
}

// ============================================================================
// Negotiation State Machine Tests
// ============================================================================

#[test]
fn negotiation_state_lifecycle() {
    let job = NegotiationJob::new(100, 1000, 50);
    let deadline = now() + Duration::hours(1);

    let mut state = NegotiationState::new(job.clone(), deadline);

    // Initial state
    assert_eq!(state.phase, NegotiationPhase::CollectingBids);
    assert!(state.is_active());
    assert!(state.selected.is_none());
    assert_eq!(state.bid_count(), 0);

    // Add bids
    let bid1 = make_bid(Uuid::new_v4(), 500, 70, 0);
    let bid2 = make_bid(Uuid::new_v4(), 400, 80, 10);

    let add_result1 = state.add_bid(bid1);
    assert!(add_result1.is_ok());
    assert_eq!(state.bid_count(), 1);

    let add_result2 = state.add_bid(bid2);
    assert!(add_result2.is_ok());
    assert_eq!(state.bid_count(), 2);

    // Finalize
    let finalize_result = state.finalize(NegotiationStrategy::LowestPrice);
    assert!(finalize_result.is_ok());
    assert!(finalize_result.unwrap().is_some());
    assert_eq!(state.phase, NegotiationPhase::Completed);
    assert!(!state.is_active());
    assert!(state.selected.is_some());
}

#[test]
fn negotiation_state_cannot_add_bid_after_finalize() {
    let job = NegotiationJob::new(100, 1000, 50);
    let deadline = now() + Duration::hours(1);

    let mut state = NegotiationState::new(job, deadline);
    state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();
    state.finalize(NegotiationStrategy::Balanced).unwrap();

    // Try to add another bid
    let result = state.add_bid(make_bid(Uuid::new_v4(), 400, 80, 0));
    assert!(result.is_err());
}

#[test]
fn negotiation_state_cannot_finalize_twice() {
    let job = NegotiationJob::new(100, 1000, 50);
    let deadline = now() + Duration::hours(1);

    let mut state = NegotiationState::new(job, deadline);
    state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();
    state.finalize(NegotiationStrategy::Balanced).unwrap();

    // Try to finalize again
    let result = state.finalize(NegotiationStrategy::LowestPrice);
    assert!(result.is_err());
}

#[test]
fn negotiation_state_can_be_cancelled() {
    let job = NegotiationJob::new(100, 1000, 50);
    let deadline = now() + Duration::hours(1);

    let mut state = NegotiationState::new(job, deadline);
    state.add_bid(make_bid(Uuid::new_v4(), 500, 70, 0)).unwrap();

    state.cancel();

    assert_eq!(state.phase, NegotiationPhase::Cancelled);
    assert!(!state.is_active());
}

#[test]
fn negotiation_fails_when_no_valid_bids() {
    let job = NegotiationJob::new(100, 100, 95); // Very strict
    let deadline = now() + Duration::hours(1);

    let mut state = NegotiationState::new(job, deadline);
    state.add_bid(make_bid(Uuid::new_v4(), 500, 50, 0)).unwrap(); // Won't qualify

    let result = state.finalize(NegotiationStrategy::Balanced);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
    assert_eq!(state.phase, NegotiationPhase::Failed);
}

// ============================================================================
// Buyer Agent Tests
// ============================================================================

#[test]
fn buyer_evaluates_provider_offers() {
    let buyer_requirements = BuyerJobRequirements::new(100, 1000, 3600, 60);
    let policy = BuyerPolicy::default();

    let good_offer = make_provider_offer(Uuid::new_v4(), 500, 80, 1800);
    let expensive_offer = make_provider_offer(Uuid::new_v4(), 1500, 90, 1800);
    let low_rep_offer = make_provider_offer(Uuid::new_v4(), 400, 40, 1800);
    let slow_offer = make_provider_offer(Uuid::new_v4(), 400, 80, 7200); // Over max duration

    assert!(evaluate_offer(&good_offer, &buyer_requirements, &policy));
    assert!(!evaluate_offer(&expensive_offer, &buyer_requirements, &policy));
    assert!(!evaluate_offer(&low_rep_offer, &buyer_requirements, &policy));
    assert!(!evaluate_offer(&slow_offer, &buyer_requirements, &policy));
}

#[test]
fn buyer_scores_offers() {
    let requirements = BuyerJobRequirements::new(100, 1000, 3600, 50);

    let cheap_low_rep = make_provider_offer(Uuid::new_v4(), 200, 50, 1800);
    let expensive_high_rep = make_provider_offer(Uuid::new_v4(), 900, 95, 1800);
    let balanced = make_provider_offer(Uuid::new_v4(), 500, 80, 1800);

    let score_cheap = score_offer(&cheap_low_rep, &requirements);
    let score_expensive = score_offer(&expensive_high_rep, &requirements);
    let score_balanced = score_offer(&balanced, &requirements);

    // All should be positive scores (they all qualify)
    assert!(score_cheap > 0.0);
    assert!(score_expensive > 0.0);
    assert!(score_balanced > 0.0);
}

#[test]
fn buyer_selects_best_offer() {
    let requirements = BuyerJobRequirements::new(100, 1000, 3600, 50);
    let policy = BuyerPolicy::default();

    let offers = vec![
        make_provider_offer(Uuid::new_v4(), 800, 60, 1800),
        make_provider_offer(Uuid::new_v4(), 500, 85, 1800), // Best balanced
        make_provider_offer(Uuid::new_v4(), 600, 70, 1800),
    ];

    let result = select_best_offer(&offers, &requirements, &policy);
    assert!(result.is_some());

    let best = result.unwrap();
    assert_eq!(best.price, 500);
    assert_eq!(best.provider_reputation, 85);
}

#[test]
fn buyer_returns_none_when_no_offers_qualify() {
    let requirements = BuyerJobRequirements::new(1000, 100, 1800, 99);
    let policy = BuyerPolicy::default();

    let offers = vec![
        make_provider_offer(Uuid::new_v4(), 500, 80, 3600), // Over price, under rep, over duration
        make_provider_offer(Uuid::new_v4(), 600, 70, 3600),
    ];

    let result = select_best_offer(&offers, &requirements, &policy);
    assert!(result.is_none());
}

// ============================================================================
// Autonomy Mode Tests - Provider Side
// ============================================================================

#[test]
fn autonomy_conservative_requires_approval_for_most() {
    let state = ProviderState::new(1000, 10);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);

    // Job that would be auto-accepted in other modes
    let job = JobSpec::new(100, 200, 600, 80);
    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Conservative, &policy);

    // Conservative should need approval (price 200 > threshold 100)
    assert!(decision.needs_approval());
}

#[test]
fn autonomy_conservative_rejects_low_price() {
    let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);

    // Job with low price (0.5 per unit, conservative wants 2.0)
    let job = JobSpec::new(100, 50, 600, 80);
    let decision = evaluate_job(&job, AutonomyMode::Conservative, &policy);

    // Conservative doesn't counter-offer, just rejects
    assert!(decision.is_reject());
}

#[test]
fn autonomy_moderate_accepts_reasonable_jobs() {
    let state = ProviderState::new(1000, 10);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    // Reasonable job
    let job = JobSpec::new(100, 200, 600, 80);
    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);

    assert!(decision.is_accept());
}

#[test]
fn autonomy_moderate_counter_offers_on_low_price() {
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    // Job with low price
    let job = JobSpec::new(100, 50, 600, 80);
    let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);

    assert!(decision.is_counter_offer());
    if let JobDecision::CounterOffer { proposed_price, .. } = decision {
        // 100 resources * 1.0 min price = 100
        assert_eq!(proposed_price, 100);
    }
}

#[test]
fn autonomy_moderate_needs_approval_for_high_value() {
    let state = ProviderState::new(1000, 10);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    // High value job (exceeds 10k threshold)
    let job = JobSpec::new(100, 50_000, 600, 80);
    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);

    assert!(decision.needs_approval());
}

#[test]
fn autonomy_aggressive_accepts_most_jobs() {
    let state = ProviderState::new(1000, 10);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Aggressive);

    // Job that would need approval in other modes
    let job = JobSpec::new(100, 50_000, 7200, 50);
    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Aggressive, &policy);

    assert!(decision.is_accept());
}

#[test]
fn autonomy_aggressive_counter_offers_instead_of_reject() {
    let policy = ProviderPolicy::for_mode(AutonomyMode::Aggressive);

    // Job with very low price
    let job = JobSpec::new(100, 30, 600, 80);
    let decision = evaluate_job(&job, AutonomyMode::Aggressive, &policy);

    // Aggressive mode should counter-offer
    assert!(decision.is_counter_offer());
    if let JobDecision::CounterOffer { proposed_price, .. } = decision {
        // 100 resources * 0.5 min price = 50
        assert_eq!(proposed_price, 50);
    }
}

#[test]
fn autonomy_aggressive_has_low_reputation_threshold() {
    let state = ProviderState::new(1000, 10);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Aggressive);

    // Job with low reputation buyer (25, threshold is 20)
    let job = JobSpec::new(100, 200, 600, 25);
    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Aggressive, &policy);

    assert!(decision.is_accept());
}

// ============================================================================
// Full Negotiation Flow Test
// ============================================================================

#[test]
fn full_negotiation_flow_end_to_end() {
    // Step 1: Create negotiation job
    let job = NegotiationJob::new(
        500,   // resources
        5000,  // max price
        60,    // min reputation
    );

    // Step 2: Simulate multiple providers evaluating the job
    let providers = vec![
        (Uuid::new_v4(), ProviderState::new(1000, 10), AutonomyMode::Conservative),
        (Uuid::new_v4(), ProviderState::new(800, 8), AutonomyMode::Moderate),
        (Uuid::new_v4(), ProviderState::new(1200, 15), AutonomyMode::Aggressive),
    ];

    let job_spec = JobSpec::new(500, 3000, 7200, 75);

    let mut _provider_decisions = Vec::new();
    for (id, state, mode) in &providers {
        let policy = ProviderPolicy::for_mode(*mode);
        let decision = evaluate_job_with_state(&job_spec, state, *mode, &policy);
        _provider_decisions.push((*id, decision.clone()));

        // Track which providers will bid
        match decision {
            JobDecision::Accept => {
                // Provider will submit a bid
            }
            JobDecision::CounterOffer { .. } => {
                // Provider will submit a counter-offer as their bid
            }
            JobDecision::NeedApproval { .. } => {
                // Provider needs human approval before bidding
            }
            JobDecision::Reject { .. } => {
                // Provider won't bid
            }
        }
    }

    // Step 3: Collect bids from interested providers
    let n = now();
    let bids = vec![
        make_bid(providers[0].0, 4000, 85, 30), // Conservative, high price
        make_bid(providers[1].0, 3000, 75, 10), // Moderate, balanced
        make_bid(providers[2].0, 2500, 65, 0),  // Aggressive, cheap and fast
    ];

    // Step 4: Create negotiation state and add bids
    let deadline = n + Duration::hours(1);
    let mut negotiation = NegotiationState::new(job.clone(), deadline);

    for bid in bids {
        let add_result = negotiation.add_bid(bid);
        assert!(add_result.is_ok());
    }

    assert_eq!(negotiation.bid_count(), 3);

    // Step 5: Test different strategies
    let strategies = vec![
        NegotiationStrategy::LowestPrice,
        NegotiationStrategy::HighestReputation,
        NegotiationStrategy::FastestAvailability,
        NegotiationStrategy::Balanced,
    ];

    for strategy in &strategies {
        let result = negotiate_at(&job, &negotiation.bids, *strategy, n);
        assert!(result.is_some(), "Strategy {:?} should find a bid", strategy);
    }

    // Step 6: Finalize with balanced strategy
    let finalize_result = negotiation.finalize(NegotiationStrategy::Balanced);
    assert!(finalize_result.is_ok());

    let selected = finalize_result.unwrap();
    assert!(selected.is_some());

    let winning_bid = selected.unwrap();
    assert!(winning_bid.bid.price <= job.max_price);
    assert!(winning_bid.bid.reputation >= job.min_reputation);

    // Step 7: Verify final state
    assert_eq!(negotiation.phase, NegotiationPhase::Completed);
    assert!(!negotiation.is_active());
}

#[test]
fn negotiation_with_all_autonomy_modes_represented() {
    let n = now();

    // Job that tests different mode behaviors
    let job = NegotiationJob::new(200, 2000, 50);

    // Provider bids representing different autonomy behaviors:
    // Conservative: High price, high reputation, careful
    // Moderate: Balanced approach
    // Aggressive: Low price, fast availability, lower requirements

    let conservative_provider = Uuid::new_v4();
    let moderate_provider = Uuid::new_v4();
    let aggressive_provider = Uuid::new_v4();

    let bids = vec![
        // Conservative provider: high price, high rep, needs setup time
        Bid::new(
            conservative_provider,
            1800,
            n + Duration::minutes(60),
            n + Duration::hours(4),
            92,
        ),
        // Moderate provider: balanced
        Bid::new(
            moderate_provider,
            1200,
            n + Duration::minutes(20),
            n + Duration::hours(2),
            78,
        ),
        // Aggressive provider: cheap, fast, lower rep
        Bid::new(
            aggressive_provider,
            800,
            n,
            n + Duration::hours(1),
            55,
        ),
    ];

    // Test: Lowest price strategy picks aggressive provider
    let result = negotiate_at(&job, &bids, NegotiationStrategy::LowestPrice, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.provider, aggressive_provider);

    // Test: Highest reputation strategy picks conservative provider
    let result = negotiate_at(&job, &bids, NegotiationStrategy::HighestReputation, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.provider, conservative_provider);

    // Test: Fastest availability picks aggressive provider (available now)
    let result = negotiate_at(&job, &bids, NegotiationStrategy::FastestAvailability, n);
    assert!(result.is_some());
    assert_eq!(result.unwrap().bid.provider, aggressive_provider);

    // Test: Balanced strategy - should pick moderate or aggressive
    // depending on weight tuning
    let result = negotiate_at(&job, &bids, NegotiationStrategy::Balanced, n);
    assert!(result.is_some());
    // The balanced strategy should find a good middle ground
    let selected = result.unwrap();
    assert!(selected.score > 0.0);
}

// ============================================================================
// Serialization Tests
// ============================================================================

#[test]
fn bid_serialization_roundtrip() {
    let bid = make_bid(Uuid::new_v4(), 1000, 80, 15);

    let json = serde_json::to_string(&bid).expect("should serialize");
    let deserialized: Bid = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(bid.provider, deserialized.provider);
    assert_eq!(bid.price, deserialized.price);
    assert_eq!(bid.reputation, deserialized.reputation);
}

#[test]
fn negotiation_job_serialization_roundtrip() {
    let job = NegotiationJob::new(500, 5000, 70);

    let json = serde_json::to_string(&job).expect("should serialize");
    let deserialized: NegotiationJob = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(job.resources, deserialized.resources);
    assert_eq!(job.max_price, deserialized.max_price);
    assert_eq!(job.min_reputation, deserialized.min_reputation);
}

#[test]
fn strategy_serialization_roundtrip() {
    let strategies = vec![
        NegotiationStrategy::LowestPrice,
        NegotiationStrategy::HighestReputation,
        NegotiationStrategy::FastestAvailability,
        NegotiationStrategy::Balanced,
    ];

    for strategy in strategies {
        let json = serde_json::to_string(&strategy).expect("should serialize");
        let deserialized: NegotiationStrategy =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(strategy, deserialized);
    }
}
