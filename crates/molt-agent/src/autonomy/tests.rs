//! Unit tests and property-based tests for the autonomy module.

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
    assert!(
        AutonomyMode::Conservative.max_auto_approve() < AutonomyMode::Moderate.max_auto_approve()
    );
    assert!(
        AutonomyMode::Moderate.max_auto_approve() < AutonomyMode::Aggressive.max_auto_approve()
    );
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

    if let JobDecision::CounterOffer {
        proposed_price,
        reason,
    } = decision
    {
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

    let reject: JobDecision =
        serde_json::from_str(r#"{"decision":"reject","reason":"test"}"#).unwrap();
    assert!(reject.is_reject());

    let need: JobDecision =
        serde_json::from_str(r#"{"decision":"need_approval","reason":"check"}"#).unwrap();
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

// ==========================================================================
// PolicyBounds tests
// ==========================================================================

#[test]
fn policy_bounds_default_has_safe_limits() {
    let bounds = PolicyBounds::default();
    assert!(bounds.max_spend_per_action > 0);
    assert!(bounds.max_spend_per_hour > 0);
    assert!(bounds.max_concurrent_jobs > 0);
    assert!(bounds.max_job_duration_secs > 0);
}

#[test]
fn policy_bounds_for_conservative_is_restrictive() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Conservative);
    assert_eq!(bounds.max_spend_per_action, 100);
    assert_eq!(bounds.max_spend_per_hour, 500);
    assert_eq!(bounds.max_concurrent_jobs, 1);
    assert_eq!(bounds.max_job_duration_secs, 600);
    assert!(!bounds.allow_new_counterparties);
    assert!(!bounds.allow_price_negotiation);
}

#[test]
fn policy_bounds_for_moderate_is_balanced() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Moderate);
    assert_eq!(bounds.max_spend_per_action, 10_000);
    assert_eq!(bounds.max_spend_per_hour, 50_000);
    assert_eq!(bounds.max_concurrent_jobs, 5);
    assert_eq!(bounds.max_job_duration_secs, 3600);
    assert!(bounds.allow_new_counterparties);
    assert!(bounds.allow_price_negotiation);
}

#[test]
fn policy_bounds_for_aggressive_is_permissive() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Aggressive);
    assert_eq!(bounds.max_spend_per_action, 1_000_000);
    assert_eq!(bounds.max_spend_per_hour, 10_000_000);
    assert_eq!(bounds.max_concurrent_jobs, 100);
    assert_eq!(bounds.max_job_duration_secs, 86400);
    assert!(bounds.allow_new_counterparties);
    assert!(bounds.allow_price_negotiation);
}

#[test]
fn policy_bounds_check_amount_within_bounds() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Moderate);
    assert!(bounds.is_amount_within_bounds(5_000));
    assert!(bounds.is_amount_within_bounds(10_000));
    assert!(!bounds.is_amount_within_bounds(10_001));
}

#[test]
fn policy_bounds_check_duration_within_bounds() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Moderate);
    assert!(bounds.is_duration_within_bounds(1800));
    assert!(bounds.is_duration_within_bounds(3600));
    assert!(!bounds.is_duration_within_bounds(3601));
}

#[test]
fn policy_bounds_serialization_roundtrip() {
    let bounds = PolicyBounds::for_mode(AutonomyMode::Moderate);
    let json = serde_json::to_string(&bounds).unwrap();
    let parsed: PolicyBounds = serde_json::from_str(&json).unwrap();
    assert_eq!(bounds, parsed);
}

// ==========================================================================
// AutonomyPolicy tests
// ==========================================================================

#[test]
fn autonomy_policy_default_is_conservative() {
    let policy = AutonomyPolicy::default();
    assert_eq!(policy.mode, AutonomyMode::Conservative);
    assert!(policy.require_approval_for_all);
}

#[test]
fn autonomy_policy_conservative_requires_approval_for_all() {
    let policy = AutonomyPolicy::conservative();
    assert_eq!(policy.mode, AutonomyMode::Conservative);
    assert!(policy.require_approval_for_all);
    assert!(!policy.bounds.allow_new_counterparties);
}

#[test]
fn autonomy_policy_moderate_executes_within_bounds() {
    let policy = AutonomyPolicy::moderate();
    assert_eq!(policy.mode, AutonomyMode::Moderate);
    assert!(!policy.require_approval_for_all);
    assert!(policy.bounds.allow_price_negotiation);
}

#[test]
fn autonomy_policy_aggressive_maximizes_autonomy() {
    let policy = AutonomyPolicy::aggressive();
    assert_eq!(policy.mode, AutonomyMode::Aggressive);
    assert!(!policy.require_approval_for_all);
    assert!(policy.auto_accept_profitable_jobs);
    assert!(policy.auto_optimize_pricing);
}

#[test]
fn autonomy_policy_with_custom_bounds() {
    let custom_bounds = PolicyBounds {
        max_spend_per_action: 5000,
        max_spend_per_hour: 25000,
        max_concurrent_jobs: 3,
        max_job_duration_secs: 1800,
        min_counterparty_reputation: 60,
        allow_new_counterparties: true,
        allow_price_negotiation: true,
    };
    let policy = AutonomyPolicy::moderate().with_bounds(custom_bounds.clone());
    assert_eq!(policy.bounds, custom_bounds);
}

#[test]
fn autonomy_policy_builder_pattern() {
    let policy = AutonomyPolicy::moderate()
        .with_mode(AutonomyMode::Aggressive)
        .with_approval_required(true)
        .with_auto_accept(false);

    assert_eq!(policy.mode, AutonomyMode::Aggressive);
    assert!(policy.require_approval_for_all);
    assert!(!policy.auto_accept_profitable_jobs);
}

#[test]
fn autonomy_policy_serialization_roundtrip() {
    let policy = AutonomyPolicy::moderate();
    let json = serde_json::to_string(&policy).unwrap();
    let parsed: AutonomyPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(policy, parsed);
}

// ==========================================================================
// PolicyEvaluator tests
// ==========================================================================

#[test]
fn policy_evaluator_conservative_always_needs_approval() {
    let policy = AutonomyPolicy::conservative();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 50,
        duration_secs: 300,
        counterparty_reputation: 95,
    };

    let result = evaluator.evaluate(&action);
    assert_eq!(
        result,
        EvaluationResult::NeedsApproval {
            reason: "conservative mode requires approval for all actions".into(),
        }
    );
}

#[test]
fn policy_evaluator_moderate_approves_within_bounds() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 5_000,
        duration_secs: 1800,
        counterparty_reputation: 70,
    };

    let result = evaluator.evaluate(&action);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn policy_evaluator_moderate_rejects_over_bounds() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 50_000, // Exceeds max_spend_per_action of 10_000
        duration_secs: 1800,
        counterparty_reputation: 70,
    };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

#[test]
fn policy_evaluator_checks_duration_bounds() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 5_000,
        duration_secs: 7200, // Exceeds max_job_duration_secs of 3600
        counterparty_reputation: 70,
    };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

#[test]
fn policy_evaluator_checks_reputation_bounds() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 5_000,
        duration_secs: 1800,
        counterparty_reputation: 30, // Below min_counterparty_reputation of 50
    };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

#[test]
fn policy_evaluator_aggressive_auto_approves() {
    let policy = AutonomyPolicy::aggressive();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptJob {
        price: 500_000,
        duration_secs: 43200,
        counterparty_reputation: 25,
    };

    let result = evaluator.evaluate(&action);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn policy_evaluator_rejects_spending_action() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::SubmitBid {
        amount: 50_000, // Exceeds bounds
    };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

#[test]
fn policy_evaluator_handles_counter_offer() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::CounterOffer {
        original_price: 500,
        proposed_price: 1000,
    };

    let result = evaluator.evaluate(&action);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn policy_evaluator_rejects_counter_offer_when_disabled() {
    let policy = AutonomyPolicy::conservative();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::CounterOffer {
        original_price: 500,
        proposed_price: 1000,
    };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

#[test]
fn policy_evaluator_evaluates_new_counterparty() {
    let policy = AutonomyPolicy::moderate();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptNewCounterparty { reputation: 60 };

    let result = evaluator.evaluate(&action);
    assert_eq!(result, EvaluationResult::Approved);
}

#[test]
fn policy_evaluator_rejects_new_counterparty_conservative() {
    let policy = AutonomyPolicy::conservative();
    let evaluator = PolicyEvaluator::new(policy);

    let action = ProposedAction::AcceptNewCounterparty { reputation: 90 };

    let result = evaluator.evaluate(&action);
    assert!(matches!(result, EvaluationResult::NeedsApproval { .. }));
}

// ==========================================================================
// SpendingTracker tests
// ==========================================================================

#[test]
fn spending_tracker_new_has_zero_spent() {
    let tracker = SpendingTracker::new(10_000);
    assert_eq!(tracker.spent_this_hour(), 0);
    assert_eq!(tracker.remaining_budget(), 10_000);
}

#[test]
fn spending_tracker_records_spending() {
    let mut tracker = SpendingTracker::new(10_000);
    assert!(tracker.record_spend(1000).is_ok());
    assert_eq!(tracker.spent_this_hour(), 1000);
    assert_eq!(tracker.remaining_budget(), 9000);
}

#[test]
fn spending_tracker_rejects_over_budget() {
    let mut tracker = SpendingTracker::new(10_000);
    let result = tracker.record_spend(15_000);
    assert!(result.is_err());
    assert_eq!(tracker.spent_this_hour(), 0);
}

#[test]
fn spending_tracker_can_afford() {
    let mut tracker = SpendingTracker::new(10_000);
    assert!(tracker.can_afford(5_000));
    assert!(tracker.can_afford(10_000));
    assert!(!tracker.can_afford(10_001));

    let _ = tracker.record_spend(6_000);
    assert!(tracker.can_afford(4_000));
    assert!(!tracker.can_afford(4_001));
}

#[test]
fn spending_tracker_reset_updates_timestamp() {
    let mut tracker = SpendingTracker::new(10_000);
    let _ = tracker.record_spend(5_000);

    let ts_before = tracker.hour_start_timestamp();
    tracker.reset_hour();
    let ts_after = tracker.hour_start_timestamp();

    assert_eq!(tracker.spent_this_hour(), 0);
    // Timestamp should be updated (or same if fast enough)
    assert!(ts_after >= ts_before);
}

#[test]
fn spending_tracker_hourly_budget_getter() {
    let tracker = SpendingTracker::new(50_000);
    assert_eq!(tracker.hourly_budget(), 50_000);
}

// ==========================================================================
// SpendingTrackerState tests
// ==========================================================================

#[test]
fn spending_tracker_state_serialization_roundtrip() {
    let state = SpendingTrackerState {
        hourly_budget: 10_000,
        spent_this_hour: 2_500,
        hour_start_timestamp: 1700000000,
        schema_version: 1,
    };

    let json = serde_json::to_string(&state).unwrap();
    let parsed: SpendingTrackerState = serde_json::from_str(&json).unwrap();

    assert_eq!(state, parsed);
}

#[test]
fn spending_tracker_state_default_schema_version() {
    // JSON without schema_version should get default value
    let json = r#"{"hourly_budget":5000,"spent_this_hour":1000,"hour_start_timestamp":1700000000}"#;
    let state: SpendingTrackerState = serde_json::from_str(json).unwrap();
    assert_eq!(state.schema_version, 1);
}

#[test]
fn spending_tracker_to_state_captures_all_fields() {
    let mut tracker = SpendingTracker::new(20_000);
    let _ = tracker.record_spend(5_000);

    let state = tracker.to_state();

    assert_eq!(state.hourly_budget, 20_000);
    assert_eq!(state.spent_this_hour, 5_000);
    assert!(state.hour_start_timestamp > 0);
    assert_eq!(state.schema_version, SpendingTrackerState::CURRENT_SCHEMA_VERSION);
}

#[test]
fn spending_tracker_from_state_restores_values() {
    let state = SpendingTrackerState {
        hourly_budget: 15_000,
        spent_this_hour: 3_000,
        // Use current time so it doesn't expire
        hour_start_timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        schema_version: 1,
    };

    let tracker = SpendingTracker::from_state(state);

    assert_eq!(tracker.hourly_budget(), 15_000);
    assert_eq!(tracker.spent_this_hour(), 3_000);
    assert_eq!(tracker.remaining_budget(), 12_000);
}

#[test]
fn spending_tracker_from_state_expired_resets_spending() {
    let state = SpendingTrackerState {
        hourly_budget: 10_000,
        spent_this_hour: 5_000,
        // Timestamp from 2 hours ago
        hour_start_timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().saturating_sub(7200))
            .unwrap_or(0),
        schema_version: 1,
    };

    let tracker = SpendingTracker::from_state(state);

    // Should have reset because hour expired
    assert_eq!(tracker.spent_this_hour(), 0);
    assert_eq!(tracker.remaining_budget(), 10_000);
}

// ==========================================================================
// SpendingTracker persistence file tests
// ==========================================================================

#[test]
fn spending_tracker_save_and_load_roundtrip() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join(format!("spending_test_{}.json", std::process::id()));

    // Create and save tracker
    let mut tracker = SpendingTracker::new(25_000);
    let _ = tracker.record_spend(7_500);

    let save_result = tracker.save_state(&state_path);
    assert!(save_result.is_ok(), "save_state should succeed");

    // Load and verify
    let (loaded, was_loaded) = SpendingTracker::load_state(&state_path, 1000);

    assert!(was_loaded, "should have loaded from file");
    assert_eq!(loaded.hourly_budget(), 25_000);
    // Spending should be preserved (hour hasn't expired)
    assert_eq!(loaded.spent_this_hour(), 7_500);

    // Cleanup
    let _ = std::fs::remove_file(&state_path);
}

#[test]
fn spending_tracker_load_missing_file_returns_fresh() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join("nonexistent_spending_state.json");

    // Ensure file doesn't exist
    let _ = std::fs::remove_file(&state_path);

    let (tracker, was_loaded) = SpendingTracker::load_state(&state_path, 50_000);

    assert!(!was_loaded, "should not have loaded from missing file");
    assert_eq!(tracker.hourly_budget(), 50_000);
    assert_eq!(tracker.spent_this_hour(), 0);
}

#[test]
fn spending_tracker_load_corrupt_file_returns_fresh() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join(format!("corrupt_spending_{}.json", std::process::id()));

    // Write corrupt JSON
    std::fs::write(&state_path, "{ invalid json }").unwrap();

    let (tracker, was_loaded) = SpendingTracker::load_state(&state_path, 30_000);

    assert!(!was_loaded, "should not have loaded from corrupt file");
    assert_eq!(tracker.hourly_budget(), 30_000);
    assert_eq!(tracker.spent_this_hour(), 0);

    // Cleanup
    let _ = std::fs::remove_file(&state_path);
}

#[test]
fn spending_tracker_load_invalid_state_returns_fresh() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join(format!("invalid_spending_{}.json", std::process::id()));

    // Write state where spent > budget (invalid)
    let invalid_state = SpendingTrackerState {
        hourly_budget: 1000,
        spent_this_hour: 5000, // More than budget!
        hour_start_timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        schema_version: 1,
    };
    let json = serde_json::to_string(&invalid_state).unwrap();
    std::fs::write(&state_path, json).unwrap();

    let (tracker, was_loaded) = SpendingTracker::load_state(&state_path, 20_000);

    assert!(!was_loaded, "should not have loaded invalid state");
    assert_eq!(tracker.hourly_budget(), 20_000);
    assert_eq!(tracker.spent_this_hour(), 0);

    // Cleanup
    let _ = std::fs::remove_file(&state_path);
}

#[test]
fn spending_tracker_try_load_returns_error_for_missing() {
    let state_path = std::path::Path::new("/nonexistent/path/state.json");
    let result = SpendingTracker::try_load_state(state_path);
    assert!(result.is_err());
}

#[test]
fn spending_tracker_try_load_returns_error_for_corrupt() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join(format!("try_corrupt_{}.json", std::process::id()));

    std::fs::write(&state_path, "not valid json at all").unwrap();

    let result = SpendingTracker::try_load_state(&state_path);
    assert!(result.is_err());

    // Cleanup
    let _ = std::fs::remove_file(&state_path);
}

#[test]
fn spending_tracker_save_creates_parent_dirs() {
    let temp_dir = std::env::temp_dir();
    let nested_path = temp_dir.join(format!("nested_{}", std::process::id()))
        .join("subdir")
        .join("state.json");

    let tracker = SpendingTracker::new(10_000);
    let result = tracker.save_state(&nested_path);

    assert!(result.is_ok(), "should create parent directories");
    assert!(nested_path.exists(), "file should exist");

    // Cleanup
    if let Some(parent) = nested_path.parent() {
        let _ = std::fs::remove_dir_all(parent.parent().unwrap_or(parent));
    }
}

#[test]
fn spending_tracker_limits_enforced_after_simulated_restart() {
    let temp_dir = std::env::temp_dir();
    let state_path = temp_dir.join(format!("restart_test_{}.json", std::process::id()));

    // Simulate first run: spend most of budget
    {
        let mut tracker = SpendingTracker::new(10_000);
        let _ = tracker.record_spend(8_000);
        assert_eq!(tracker.remaining_budget(), 2_000);
        tracker.save_state(&state_path).unwrap();
    }

    // Simulate restart: load state and verify limits
    {
        let (mut tracker, was_loaded) = SpendingTracker::load_state(&state_path, 10_000);
        assert!(was_loaded);

        // Should only be able to spend remaining 2_000
        assert!(!tracker.can_afford(3_000));
        assert!(tracker.can_afford(2_000));

        // Try to exceed budget
        let result = tracker.record_spend(5_000);
        assert!(result.is_err());

        // Should be able to spend within remaining
        let result = tracker.record_spend(1_500);
        assert!(result.is_ok());
        assert_eq!(tracker.remaining_budget(), 500);
    }

    // Cleanup
    let _ = std::fs::remove_file(&state_path);
}

// ==========================================================================
// Property-based tests with proptest
// ==========================================================================

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        fn arb_autonomy_mode()(mode in 0u8..3) -> AutonomyMode {
            match mode {
                0 => AutonomyMode::Conservative,
                1 => AutonomyMode::Moderate,
                _ => AutonomyMode::Aggressive,
            }
        }
    }

    prop_compose! {
        fn arb_policy_bounds()(
            max_spend_per_action in 1u64..1_000_000,
            max_spend_per_hour in 1u64..10_000_000,
            max_concurrent_jobs in 1u32..100,
            max_job_duration_secs in 60u64..86400,
            min_counterparty_reputation in 0u8..100,
            allow_new_counterparties in any::<bool>(),
            allow_price_negotiation in any::<bool>(),
        ) -> PolicyBounds {
            PolicyBounds {
                max_spend_per_action,
                max_spend_per_hour,
                max_concurrent_jobs,
                max_job_duration_secs,
                min_counterparty_reputation,
                allow_new_counterparties,
                allow_price_negotiation,
            }
        }
    }

    prop_compose! {
        fn arb_proposed_action()(
            variant in 0u8..4,
            price in 0u64..1_000_000,
            duration in 0u64..100_000,
            reputation in 0u8..100,
        ) -> ProposedAction {
            match variant {
                0 => ProposedAction::AcceptJob {
                    price,
                    duration_secs: duration,
                    counterparty_reputation: reputation,
                },
                1 => ProposedAction::SubmitBid { amount: price },
                2 => ProposedAction::CounterOffer {
                    original_price: price,
                    proposed_price: price.saturating_add(100),
                },
                _ => ProposedAction::AcceptNewCounterparty { reputation },
            }
        }
    }

    proptest! {
        #[test]
        fn policy_bounds_amount_check_is_consistent(
            bounds in arb_policy_bounds(),
            amount in 0u64..2_000_000,
        ) {
            let within = bounds.is_amount_within_bounds(amount);
            prop_assert_eq!(within, amount <= bounds.max_spend_per_action);
        }

        #[test]
        fn policy_bounds_duration_check_is_consistent(
            bounds in arb_policy_bounds(),
            duration in 0u64..100_000,
        ) {
            let within = bounds.is_duration_within_bounds(duration);
            prop_assert_eq!(within, duration <= bounds.max_job_duration_secs);
        }

        #[test]
        fn autonomy_mode_risk_tolerance_is_bounded(mode in arb_autonomy_mode()) {
            let tolerance = mode.risk_tolerance();
            prop_assert!(tolerance >= 0.0);
            prop_assert!(tolerance <= 1.0);
        }

        #[test]
        fn conservative_policy_always_needs_approval(action in arb_proposed_action()) {
            let policy = AutonomyPolicy::conservative();
            let evaluator = PolicyEvaluator::new(policy);
            let result = evaluator.evaluate(&action);
            prop_assert!(result.needs_approval(), "Conservative mode should always need approval");
        }

        #[test]
        fn spending_tracker_maintains_invariant(
            budget in 1000u64..1_000_000,
            spends in prop::collection::vec(1u64..1000, 0..20),
        ) {
            let mut tracker = SpendingTracker::new(budget);
            let mut total_spent = 0u64;

            for spend in spends {
                if tracker.can_afford(spend) {
                    let _ = tracker.record_spend(spend);
                    total_spent = total_spent.saturating_add(spend);
                }
            }

            prop_assert_eq!(tracker.spent_this_hour(), total_spent);
            prop_assert!(tracker.spent_this_hour() <= budget);
            prop_assert_eq!(tracker.remaining_budget(), budget.saturating_sub(total_spent));
        }

        #[test]
        fn policy_serialization_roundtrip_preserves_data(mode in arb_autonomy_mode()) {
            let policy = AutonomyPolicy::for_mode(mode);
            let json = serde_json::to_string(&policy).unwrap();
            let parsed: AutonomyPolicy = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(policy, parsed);
        }

        #[test]
        fn bounds_for_mode_ordering(_mode in arb_autonomy_mode()) {
            // This test verifies the ordering invariant holds regardless of which mode we're testing
            let conservative = PolicyBounds::for_mode(AutonomyMode::Conservative);
            let moderate = PolicyBounds::for_mode(AutonomyMode::Moderate);
            let aggressive = PolicyBounds::for_mode(AutonomyMode::Aggressive);

            // Conservative should be most restrictive
            prop_assert!(conservative.max_spend_per_action <= moderate.max_spend_per_action);
            prop_assert!(moderate.max_spend_per_action <= aggressive.max_spend_per_action);

            prop_assert!(conservative.max_concurrent_jobs <= moderate.max_concurrent_jobs);
            prop_assert!(moderate.max_concurrent_jobs <= aggressive.max_concurrent_jobs);
        }
    }
}
