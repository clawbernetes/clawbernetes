//! End-to-end integration tests for the MOLT flow.
//!
//! Tests the complete lifecycle of a compute job in the MOLT network:
//! 1. Wallet creation and signing
//! 2. Provider capacity announcement
//! 3. Buyer job order creation
//! 4. Order matching
//! 5. Escrow funding
//! 6. Job evaluation with autonomy modes
//! 7. Execution attestation
//! 8. Job settlement
//! 9. Reputation updates

use chrono::{Duration, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use molt_agent::{
    AutonomyMode, JobDecision, JobSpec, ProviderPolicy, ProviderState,
    evaluate_job, evaluate_job_with_state,
};
use molt_attestation::{
    CheckpointChain, ExecutionAttestation, ExecutionMetrics, verify_execution_attestation,
};
use molt_core::{Reputation, Wallet};
use molt_market::{
    CapacityOffer, EscrowAccount, EscrowState, GpuCapacity, JobOrder, JobRequirements,
    JobSettlementInput, OrderBook, settle_job,
};
use molt_p2p::{CapacityAnnouncement, GpuInfo as P2pGpuInfo, PeerId, Pricing};
use rand::rngs::OsRng;
use rand::RngCore;
use std::time::Duration as StdDuration;
use uuid::Uuid;

// ============================================================================
// Helper Functions
// ============================================================================

fn create_keypair() -> (SigningKey, VerifyingKey) {
    let mut secret_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut secret_bytes);
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

// ============================================================================
// Phase 1: Wallet Creation and Signing
// ============================================================================

#[test]
fn wallet_creation_generates_unique_keys() {
    let wallet1 = Wallet::new();
    let wallet2 = Wallet::new();

    // Each wallet should have a unique public key
    assert_ne!(wallet1.public_key(), wallet2.public_key());
}

#[test]
fn wallet_sign_and_verify_roundtrip() {
    let wallet = Wallet::new();
    let public_key = wallet.public_key();
    let message = b"test transaction data";

    let signature = wallet.sign(message);

    // Verification should succeed with correct key and message
    let verify_result = public_key.verify(message, &signature);
    assert!(verify_result.is_ok());
}

#[test]
fn wallet_verify_rejects_tampered_message() {
    let wallet = Wallet::new();
    let public_key = wallet.public_key();
    let original_message = b"original data";
    let tampered_message = b"tampered data";

    let signature = wallet.sign(original_message);

    // Verification should fail with tampered message
    let verify_result = public_key.verify(tampered_message, &signature);
    assert!(verify_result.is_err());
}

#[test]
fn wallet_verify_rejects_wrong_key() {
    let wallet1 = Wallet::new();
    let wallet2 = Wallet::new();
    let message = b"test message";

    let signature = wallet1.sign(message);

    // Verification should fail with wrong public key
    let verify_result = wallet2.public_key().verify(message, &signature);
    assert!(verify_result.is_err());
}

#[test]
fn wallet_serialization_roundtrip() {
    let wallet = Wallet::new();
    let original_pubkey = wallet.public_key();
    let bytes = wallet.to_bytes();

    let restored_result = Wallet::from_bytes(&bytes);
    assert!(restored_result.is_ok());

    let restored = restored_result.unwrap();
    assert_eq!(original_pubkey, restored.public_key());
}

// ============================================================================
// Phase 2: Provider Capacity Announcement
// ============================================================================

#[test]
fn provider_creates_capacity_announcement() {
    let (signing_key, verifying_key) = create_keypair();
    let peer_id = PeerId::from_public_key(&verifying_key);

    let gpus = vec![P2pGpuInfo {
        model: "NVIDIA RTX 4090".to_string(),
        vram_gb: 24,
        count: 2,
    }];

    let pricing = Pricing {
        gpu_hour_cents: 100,
        cpu_hour_cents: 10,
    };

    let mut announcement = CapacityAnnouncement::new(
        peer_id,
        gpus.clone(),
        pricing.clone(),
        vec!["inference".to_string(), "training".to_string()],
        StdDuration::from_secs(3600),
    );

    // Sign the announcement
    announcement.sign(&signing_key);

    // Verify the announcement
    let verify_result = announcement.verify(&verifying_key);
    assert!(verify_result.is_ok());

    // Check announcement properties
    assert_eq!(announcement.peer_id(), peer_id);
    assert_eq!(announcement.gpus().len(), 1);
    assert_eq!(announcement.gpus()[0].model, "NVIDIA RTX 4090");
    assert_eq!(announcement.pricing().gpu_hour_cents, 100);
    assert!(!announcement.is_expired());
}

#[test]
fn capacity_announcement_expires_after_ttl() {
    let (signing_key, verifying_key) = create_keypair();
    let peer_id = PeerId::from_public_key(&verifying_key);

    // Create announcement with very short TTL
    let mut announcement = CapacityAnnouncement::new(
        peer_id,
        vec![],
        Pricing {
            gpu_hour_cents: 50,
            cpu_hour_cents: 5,
        },
        vec![],
        StdDuration::from_millis(1),
    );

    announcement.sign(&signing_key);

    // Wait for expiry
    std::thread::sleep(StdDuration::from_millis(10));

    assert!(announcement.is_expired());
}

#[test]
fn capacity_announcement_verification_fails_with_wrong_key() {
    let (signing_key, _) = create_keypair();
    let (_, wrong_verifying_key) = create_keypair();
    let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

    let mut announcement = CapacityAnnouncement::new(
        peer_id,
        vec![],
        Pricing {
            gpu_hour_cents: 50,
            cpu_hour_cents: 5,
        },
        vec![],
        StdDuration::from_secs(3600),
    );

    announcement.sign(&signing_key);

    // Verification should fail with wrong key
    let verify_result = announcement.verify(&wrong_verifying_key);
    assert!(verify_result.is_err());
}

// ============================================================================
// Phase 3: Buyer Job Order Creation
// ============================================================================

#[test]
fn buyer_creates_job_order() {
    let buyer_wallet = Wallet::new();
    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();

    let requirements = JobRequirements {
        min_gpus: 4,
        gpu_model: Some("A100".to_string()),
        min_memory_gb: 80,
        max_duration_hours: 8,
    };

    let order = JobOrder::new(buyer_id.clone(), requirements.clone(), 1000);

    assert!(!order.id.is_empty());
    assert_eq!(order.buyer, buyer_id);
    assert_eq!(order.requirements.min_gpus, 4);
    assert_eq!(order.requirements.gpu_model, Some("A100".to_string()));
    assert_eq!(order.max_price, 1000);
    assert!(order.created_at > 0);
}

// ============================================================================
// Phase 4: Order Matching
// ============================================================================

#[test]
fn order_matches_compatible_capacity_offer() {
    let mut orderbook = OrderBook::new();

    // Provider creates capacity offer
    let provider_wallet = Wallet::new();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let offer = CapacityOffer::new(
        provider_id.clone(),
        GpuCapacity {
            count: 8,
            model: "A100".to_string(),
            memory_gb: 80,
        },
        50, // 50 tokens per hour
        850, // High reputation
    );
    let offer_id = offer.id.clone();
    orderbook.insert_offer(offer);

    // Buyer creates job order
    let buyer_wallet = Wallet::new();
    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();

    let order = JobOrder::new(
        buyer_id.clone(),
        JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 40,
            max_duration_hours: 8,
        },
        500, // Max price 500 tokens
    );
    let order_id = order.id.clone();
    orderbook.insert_order(order);

    // Find matches
    let matches_result = orderbook.find_matches(&order_id);
    assert!(matches_result.is_ok());

    let matches = matches_result.unwrap();
    assert!(!matches.is_empty());
    assert_eq!(matches[0].offer_id, offer_id);
}

#[test]
fn order_does_not_match_insufficient_capacity() {
    let mut orderbook = OrderBook::new();

    // Provider with only 2 GPUs
    let provider_wallet = Wallet::new();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let offer = CapacityOffer::new(
        provider_id.clone(),
        GpuCapacity {
            count: 2,
            model: "A100".to_string(),
            memory_gb: 80,
        },
        50,
        850,
    );
    orderbook.insert_offer(offer);

    // Buyer needs 8 GPUs
    let buyer_wallet = Wallet::new();
    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();

    let order = JobOrder::new(
        buyer_id.clone(),
        JobRequirements {
            min_gpus: 8,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 40,
            max_duration_hours: 8,
        },
        1000,
    );
    let order_id = order.id.clone();
    orderbook.insert_order(order);

    let matches_result = orderbook.find_matches(&order_id);
    assert!(matches_result.is_ok());
    assert!(matches_result.unwrap().is_empty());
}

#[test]
fn order_does_not_match_wrong_gpu_model() {
    let mut orderbook = OrderBook::new();

    // Provider with RTX 4090
    let provider_wallet = Wallet::new();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let offer = CapacityOffer::new(
        provider_id.clone(),
        GpuCapacity {
            count: 8,
            model: "RTX 4090".to_string(),
            memory_gb: 24,
        },
        30,
        900,
    );
    orderbook.insert_offer(offer);

    // Buyer needs A100
    let buyer_wallet = Wallet::new();
    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();

    let order = JobOrder::new(
        buyer_id.clone(),
        JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 40,
            max_duration_hours: 8,
        },
        1000,
    );
    let order_id = order.id.clone();
    orderbook.insert_order(order);

    let matches_result = orderbook.find_matches(&order_id);
    assert!(matches_result.is_ok());
    assert!(matches_result.unwrap().is_empty());
}

// ============================================================================
// Phase 5: Escrow Funding
// ============================================================================

#[test]
fn escrow_lifecycle_fund_and_release() {
    let buyer_wallet = Wallet::new();
    let provider_wallet = Wallet::new();

    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let mut escrow = EscrowAccount::new(
        "job-001".to_string(),
        buyer_id.clone(),
        provider_id.clone(),
        500,
    );

    // Initial state should be Created
    assert_eq!(escrow.state, EscrowState::Created);
    assert!(!escrow.is_finalized());

    // Fund the escrow
    let fund_result = escrow.fund(&buyer_id);
    assert!(fund_result.is_ok());
    assert_eq!(escrow.state, EscrowState::Funded);

    // Release funds to provider
    let release_result = escrow.release(&buyer_id);
    assert!(release_result.is_ok());
    assert_eq!(escrow.state, EscrowState::Released);
    assert!(escrow.is_finalized());
}

#[test]
fn escrow_lifecycle_fund_and_refund() {
    let buyer_wallet = Wallet::new();
    let provider_wallet = Wallet::new();

    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let mut escrow = EscrowAccount::new(
        "job-002".to_string(),
        buyer_id.clone(),
        provider_id.clone(),
        500,
    );

    let fund_result = escrow.fund(&buyer_id);
    assert!(fund_result.is_ok());

    // Refund buyer (job cancelled)
    let refund_result = escrow.refund(&provider_id);
    assert!(refund_result.is_ok());
    assert_eq!(escrow.state, EscrowState::Refunded);
    assert!(escrow.is_finalized());
}

#[test]
fn escrow_cannot_release_before_funding() {
    let buyer_wallet = Wallet::new();
    let provider_wallet = Wallet::new();

    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let mut escrow = EscrowAccount::new(
        "job-003".to_string(),
        buyer_id.clone(),
        provider_id.clone(),
        500,
    );

    // Cannot release without funding first
    let release_result = escrow.release(&buyer_id);
    assert!(release_result.is_err());
}

#[test]
fn escrow_dispute_and_resolution() {
    let buyer_wallet = Wallet::new();
    let provider_wallet = Wallet::new();

    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    let mut escrow = EscrowAccount::new(
        "job-004".to_string(),
        buyer_id.clone(),
        provider_id.clone(),
        500,
    );

    let fund_result = escrow.fund(&buyer_id);
    assert!(fund_result.is_ok());

    // Enter dispute
    let dispute_result = escrow.dispute(&buyer_id);
    assert!(dispute_result.is_ok());
    assert_eq!(escrow.state, EscrowState::Disputed);

    // Resolve dispute in favor of provider
    let resolve_result = escrow.release(&buyer_id);
    assert!(resolve_result.is_ok());
    assert_eq!(escrow.state, EscrowState::Released);
}

// ============================================================================
// Phase 6: Provider Job Evaluation with Autonomy Modes
// ============================================================================

#[test]
fn provider_evaluates_job_conservative_mode() {
    let state = ProviderState::new(1000, 10);
    let job = JobSpec::new(100, 200, 600, 80);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);

    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Conservative, &policy);

    // Conservative mode has stricter thresholds
    // Price per unit is 2.0, which meets conservative min (2.0)
    // But price (200) exceeds conservative max_auto_accept_price (100)
    assert!(decision.needs_approval());
}

#[test]
fn provider_evaluates_job_moderate_mode_accepts() {
    let state = ProviderState::new(1000, 10);
    let job = JobSpec::new(100, 200, 600, 80);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);

    // Moderate mode should accept: good price (2.0 per unit), good reputation (80), short duration
    assert!(decision.is_accept());
}

#[test]
fn provider_evaluates_job_aggressive_mode_accepts() {
    let state = ProviderState::new(1000, 10);
    let job = JobSpec::new(100, 200, 3600, 50);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Aggressive);

    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Aggressive, &policy);

    // Aggressive mode has loose thresholds, should accept
    assert!(decision.is_accept());
}

#[test]
fn provider_rejects_job_insufficient_capacity() {
    let state = ProviderState::new(50, 10); // Only 50 capacity
    let job = JobSpec::new(100, 200, 600, 80); // Needs 100 capacity
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    let decision = evaluate_job_with_state(&job, &state, AutonomyMode::Moderate, &policy);

    assert!(decision.is_reject());
}

#[test]
fn provider_counter_offers_on_low_price() {
    let job = JobSpec::new(100, 50, 600, 80); // 0.5 per unit, too low
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);

    let decision = evaluate_job(&job, AutonomyMode::Moderate, &policy);

    // Moderate mode should counter-offer
    assert!(decision.is_counter_offer());

    if let JobDecision::CounterOffer { proposed_price, .. } = decision {
        assert_eq!(proposed_price, 100); // 100 resources * 1.0 min price
    }
}

#[test]
fn provider_rejects_low_price_in_conservative_mode() {
    let job = JobSpec::new(100, 50, 600, 80); // 0.5 per unit
    let policy = ProviderPolicy::for_mode(AutonomyMode::Conservative);

    let decision = evaluate_job(&job, AutonomyMode::Conservative, &policy);

    // Conservative mode doesn't allow counter-offers, should reject
    assert!(decision.is_reject());
}

// ============================================================================
// Phase 7: Execution Attestation
// ============================================================================

#[test]
fn provider_creates_execution_attestation() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    // Simulate job execution with checkpoints
    let mut chain = CheckpointChain::new(b"initial state");
    chain.add_checkpoint(b"processing step 1");
    chain.add_checkpoint(b"processing step 2");
    chain.add_checkpoint(b"final state");

    let metrics = ExecutionMetrics {
        gpu_utilization: 85.5,
        memory_used_mb: 16384,
        compute_ops: 1_000_000,
    };

    let attestation_result = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        metrics,
        &signing_key,
    );

    assert!(attestation_result.is_ok());

    let attestation = attestation_result.unwrap();
    assert_eq!(attestation.job_id, job_id);
    assert_eq!(attestation.checkpoint_count(), 4);
    assert!(attestation.final_checkpoint_hash().is_some());

    // Verify the attestation
    let verify_result = verify_execution_attestation(&attestation, &verifying_key);
    assert!(verify_result.is_ok());
    assert!(verify_result.unwrap().valid);
}

// ============================================================================
// Phase 8: Job Settlement
// ============================================================================

#[test]
fn job_settles_correctly() {
    let input = JobSettlementInput {
        job_id: "job-005".to_string(),
        start_time: 1000,
        end_time: 4600, // 1 hour later
        rate_per_hour: 100,
        escrow_amount: 500,
    };

    let result = settle_job(&input);
    assert!(result.is_ok());

    let settlement = result.unwrap();
    assert_eq!(settlement.job_id, "job-005");
    assert_eq!(settlement.duration_seconds, 3600);
    assert_eq!(settlement.amount_paid, 100); // 1 hour * 100/hour
    assert!(settlement.success);
}

#[test]
fn job_settlement_caps_at_escrow_amount() {
    let input = JobSettlementInput {
        job_id: "job-006".to_string(),
        start_time: 0,
        end_time: 36000, // 10 hours
        rate_per_hour: 100,
        escrow_amount: 500, // Only 500 escrowed
    };

    let result = settle_job(&input);
    assert!(result.is_ok());

    let settlement = result.unwrap();
    // Should cap at escrow amount
    assert_eq!(settlement.amount_paid, 500);
}

#[test]
fn job_settlement_rejects_invalid_times() {
    let input = JobSettlementInput {
        job_id: "job-007".to_string(),
        start_time: 5000,
        end_time: 1000, // End before start
        rate_per_hour: 100,
        escrow_amount: 500,
    };

    let result = settle_job(&input);
    assert!(result.is_err());
}

// ============================================================================
// Phase 9: Reputation Updates
// ============================================================================

#[test]
fn reputation_updates_on_success() {
    let mut reputation = Reputation::new();

    // Initial neutral score
    let initial_score = reputation.score().value();
    assert!((initial_score - 0.5).abs() < f64::EPSILON);

    // Record successful job
    reputation.record_success();

    let new_score = reputation.score().value();
    assert!(new_score > initial_score);
    assert_eq!(reputation.total_transactions(), 1);
    assert_eq!(reputation.successful_transactions(), 1);
}

#[test]
fn reputation_updates_on_failure() {
    let mut reputation = Reputation::new();

    let initial_score = reputation.score().value();

    // Record failed job
    reputation.record_failure();

    let new_score = reputation.score().value();
    assert!(new_score < initial_score);
    assert_eq!(reputation.total_transactions(), 1);
    assert_eq!(reputation.successful_transactions(), 0);
}

#[test]
fn reputation_reflects_history() {
    let mut reputation = Reputation::new();

    // 8 successes, 2 failures = 80% success rate
    for _ in 0..8 {
        reputation.record_success();
    }
    for _ in 0..2 {
        reputation.record_failure();
    }

    let score = reputation.score().value();
    // With Bayesian prior (1 success, 1 failure), expected around 0.75
    assert!(score > 0.7 && score < 0.85);
    assert!(reputation.score().is_good());
}

// ============================================================================
// Full End-to-End Flow Test
// ============================================================================

#[test]
fn full_molt_flow_end_to_end() {
    // Step 1: Create buyer and provider wallets
    let buyer_wallet = Wallet::new();
    let provider_wallet = Wallet::new();
    let (provider_signing_key, provider_verifying_key) = create_keypair();

    let buyer_id = bs58::encode(buyer_wallet.public_key().as_bytes()).into_string();
    let provider_id = bs58::encode(provider_wallet.public_key().as_bytes()).into_string();

    // Step 2: Provider creates and signs capacity announcement
    let peer_id = PeerId::from_public_key(&provider_verifying_key);
    let mut announcement = CapacityAnnouncement::new(
        peer_id,
        vec![P2pGpuInfo {
            model: "A100".to_string(),
            vram_gb: 80,
            count: 8,
        }],
        Pricing {
            gpu_hour_cents: 50,
            cpu_hour_cents: 5,
        },
        vec!["training".to_string()],
        StdDuration::from_secs(3600),
    );
    announcement.sign(&provider_signing_key);
    assert!(announcement.verify(&provider_verifying_key).is_ok());

    // Step 3: Buyer creates job order
    let order = JobOrder::new(
        buyer_id.clone(),
        JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 40,
            max_duration_hours: 2,
        },
        200,
    );

    // Step 4: Match order to provider in orderbook
    let mut orderbook = OrderBook::new();
    let offer = CapacityOffer::new(
        provider_id.clone(),
        GpuCapacity {
            count: 8,
            model: "A100".to_string(),
            memory_gb: 80,
        },
        50,
        850,
    );
    orderbook.insert_offer(offer);
    orderbook.insert_order(order.clone());

    let matches = orderbook.find_matches(&order.id);
    assert!(matches.is_ok());
    assert!(!matches.as_ref().unwrap().is_empty());

    // Step 5: Create and fund escrow
    let mut escrow = EscrowAccount::new(
        order.id.clone(),
        buyer_id.clone(),
        provider_id.clone(),
        200,
    );
    assert!(escrow.fund(&buyer_id).is_ok());

    // Step 6: Provider evaluates job with autonomy mode
    let provider_state = ProviderState::new(1000, 10);
    let job_spec = JobSpec::new(400, 200, 7200, 80);
    let policy = ProviderPolicy::for_mode(AutonomyMode::Moderate);
    let decision = evaluate_job_with_state(&job_spec, &provider_state, AutonomyMode::Moderate, &policy);

    // Job should be accepted or need approval (depends on thresholds)
    assert!(!decision.is_reject());

    // Step 7: Provider executes job and creates attestation
    let mut chain = CheckpointChain::new(b"job started");
    chain.add_checkpoint(b"epoch 1 complete");
    chain.add_checkpoint(b"epoch 2 complete");
    chain.add_checkpoint(b"training complete");

    let attestation = ExecutionAttestation::create_and_sign(
        Uuid::parse_str(&order.id).unwrap_or_else(|_| Uuid::new_v4()),
        chain.into_checkpoints(),
        Duration::seconds(7200),
        ExecutionMetrics {
            gpu_utilization: 92.5,
            memory_used_mb: 65536,
            compute_ops: 10_000_000,
        },
        &provider_signing_key,
    );
    assert!(attestation.is_ok());

    let attestation = attestation.unwrap();
    assert!(verify_execution_attestation(&attestation, &provider_verifying_key).is_ok());

    // Step 8: Settle job and release escrow
    let settlement_input = JobSettlementInput {
        job_id: order.id.clone(),
        start_time: Utc::now().timestamp() - 7200,
        end_time: Utc::now().timestamp(),
        rate_per_hour: 100,
        escrow_amount: 200,
    };
    let settlement = settle_job(&settlement_input);
    assert!(settlement.is_ok());

    assert!(escrow.release(&buyer_id).is_ok());
    assert!(escrow.is_finalized());

    // Step 9: Update reputation
    let mut provider_reputation = Reputation::new();
    provider_reputation.record_success();

    assert!(provider_reputation.score().value() > 0.5);
    assert_eq!(provider_reputation.successful_transactions(), 1);
}
