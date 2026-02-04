//! Integration tests for the attestation flow.
//!
//! Tests the complete attestation lifecycle:
//! 1. Hardware attestation creation and verification
//! 2. Execution with checkpoint creation
//! 3. Execution attestation with checkpoint chain
//! 4. Attestation verification

use chrono::Duration;
use ed25519_dalek::{SigningKey, VerifyingKey};
use molt_attestation::{
    batch_verify_execution, batch_verify_hardware, Checkpoint, CheckpointChain,
    ExecutionAttestation, ExecutionMetrics, GpuInfo, HardwareAttestation,
    VerificationDetails, verify_execution_attestation, verify_execution_with_data,
    verify_hardware_attestation,
};
use rand::rngs::OsRng;
use rand::RngCore;
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

fn create_test_gpu() -> GpuInfo {
    GpuInfo {
        vendor: molt_attestation::GpuVendor::Nvidia,
        model: "NVIDIA RTX 4090".to_string(),
        vram_mb: 24576,
        compute_capability: "8.9".to_string(),
    }
}

// ============================================================================
// Hardware Attestation Tests
// ============================================================================

#[test]
fn create_hardware_attestation_single_gpu() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let gpus = vec![create_test_gpu()];

    let attestation_result =
        HardwareAttestation::create_and_sign(node_id, gpus.clone(), Duration::hours(24), &signing_key);

    assert!(attestation_result.is_ok());

    let attestation = attestation_result.unwrap();
    assert_eq!(attestation.node_id, node_id);
    assert_eq!(attestation.gpus.len(), 1);
    assert_eq!(attestation.gpus[0].model, "NVIDIA RTX 4090");
    assert!(!attestation.is_expired());

    // Verify
    let verify_result = verify_hardware_attestation(&attestation, &verifying_key);
    assert!(verify_result.is_ok());
    assert!(verify_result.unwrap().valid);
}

#[test]
fn create_hardware_attestation_multiple_gpus() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();

    let gpus = vec![
        GpuInfo {
            vendor: molt_attestation::GpuVendor::Nvidia,
            model: "NVIDIA A100".to_string(),
            vram_mb: 81920,
            compute_capability: "8.0".to_string(),
        },
        GpuInfo {
            vendor: molt_attestation::GpuVendor::Nvidia,
            model: "NVIDIA A100".to_string(),
            vram_mb: 81920,
            compute_capability: "8.0".to_string(),
        },
        GpuInfo {
            vendor: molt_attestation::GpuVendor::Nvidia,
            model: "NVIDIA H100".to_string(),
            vram_mb: 81920,
            compute_capability: "9.0".to_string(),
        },
    ];

    let attestation_result =
        HardwareAttestation::create_and_sign(node_id, gpus.clone(), Duration::hours(24), &signing_key);

    assert!(attestation_result.is_ok());

    let attestation = attestation_result.unwrap();
    assert_eq!(attestation.gpus.len(), 3);

    let verify_result = verify_hardware_attestation(&attestation, &verifying_key);
    assert!(verify_result.is_ok());

    if let VerificationDetails::Hardware { gpu_count, not_expired } = verify_result.unwrap().details {
        assert_eq!(gpu_count, 3);
        assert!(not_expired);
    } else {
        panic!("Expected Hardware verification details");
    }
}

#[test]
fn verify_hardware_attestation_success() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let gpus = vec![create_test_gpu()];

    let attestation =
        HardwareAttestation::create_and_sign(node_id, gpus, Duration::hours(24), &signing_key)
            .expect("should create attestation");

    let result = verify_hardware_attestation(&attestation, &verifying_key);
    assert!(result.is_ok());

    let verification = result.unwrap();
    assert!(verification.valid);
    
    match verification.details {
        VerificationDetails::Hardware { gpu_count, not_expired } => {
            assert_eq!(gpu_count, 1);
            assert!(not_expired);
        }
        _ => panic!("Expected Hardware details"),
    }
}

#[test]
fn verify_hardware_attestation_wrong_key_fails() {
    let (signing_key, _) = create_keypair();
    let (_, wrong_verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let gpus = vec![create_test_gpu()];

    let attestation =
        HardwareAttestation::create_and_sign(node_id, gpus, Duration::hours(24), &signing_key)
            .expect("should create attestation");

    let result = verify_hardware_attestation(&attestation, &wrong_verifying_key);
    assert!(result.is_err());
}

#[test]
fn verify_hardware_attestation_expired_fails() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let gpus = vec![create_test_gpu()];

    // Create already-expired attestation
    let attestation =
        HardwareAttestation::create_and_sign(node_id, gpus, Duration::hours(-1), &signing_key)
            .expect("should create attestation");

    assert!(attestation.is_expired());

    let result = verify_hardware_attestation(&attestation, &verifying_key);
    assert!(result.is_err());
}

#[test]
fn hardware_attestation_serialization_preserves_validity() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let gpus = vec![create_test_gpu()];

    let attestation =
        HardwareAttestation::create_and_sign(node_id, gpus, Duration::hours(24), &signing_key)
            .expect("should create attestation");

    // Serialize and deserialize
    let json = serde_json::to_string(&attestation).expect("should serialize");
    let deserialized: HardwareAttestation =
        serde_json::from_str(&json).expect("should deserialize");

    // Verification should still work
    let result = verify_hardware_attestation(&deserialized, &verifying_key);
    assert!(result.is_ok());
    assert!(result.unwrap().valid);
}

// ============================================================================
// Checkpoint Tests
// ============================================================================

#[test]
fn checkpoint_creation_computes_hash() {
    let data = b"initial state";
    let checkpoint = Checkpoint::new(0, data);

    assert_eq!(checkpoint.sequence, 0);
    assert_eq!(checkpoint.hash, *blake3::hash(data).as_bytes());
}

#[test]
fn checkpoint_chaining_links_hashes() {
    let first = Checkpoint::new(0, b"first state");
    let second = Checkpoint::chain_from(&first, b"second state");

    assert_eq!(second.sequence, 1);

    // Verify the chain relationship
    assert!(second.verify_chain(&first, b"second state"));
}

#[test]
fn checkpoint_chain_verification_fails_with_wrong_data() {
    let first = Checkpoint::new(0, b"first state");
    let second = Checkpoint::chain_from(&first, b"second state");

    // Should fail with wrong data
    assert!(!second.verify_chain(&first, b"wrong data"));
}

#[test]
fn checkpoint_chain_verification_fails_with_wrong_sequence() {
    let first = Checkpoint::new(0, b"first state");
    let mut second = Checkpoint::chain_from(&first, b"second state");
    second.sequence = 5; // Wrong sequence

    assert!(!second.verify_chain(&first, b"second state"));
}

#[test]
fn checkpoint_chain_builder_creates_valid_chain() {
    let mut chain = CheckpointChain::new(b"initial");
    assert_eq!(chain.len(), 1);

    chain.add_checkpoint(b"step 1");
    chain.add_checkpoint(b"step 2");
    chain.add_checkpoint(b"step 3");
    chain.add_checkpoint(b"final");

    assert_eq!(chain.len(), 5);
    assert!(!chain.is_empty());

    let checkpoints = chain.into_checkpoints();
    assert_eq!(checkpoints.len(), 5);

    // Verify sequence numbers
    for (i, checkpoint) in checkpoints.iter().enumerate() {
        assert_eq!(checkpoint.sequence, i as u64);
    }
}

// ============================================================================
// Execution Attestation Tests
// ============================================================================

#[test]
fn create_execution_attestation_with_checkpoints() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let mut chain = CheckpointChain::new(b"job started");
    chain.add_checkpoint(b"progress 25%");
    chain.add_checkpoint(b"progress 50%");
    chain.add_checkpoint(b"progress 75%");
    chain.add_checkpoint(b"job complete");

    let metrics = ExecutionMetrics {
        gpu_utilization: 95.0,
        memory_used_mb: 32768,
        compute_ops: 5_000_000,
    };

    let attestation_result = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(7200),
        metrics.clone(),
        &signing_key,
    );

    assert!(attestation_result.is_ok());

    let attestation = attestation_result.unwrap();
    assert_eq!(attestation.job_id, job_id);
    assert_eq!(attestation.checkpoint_count(), 5);
    assert!(attestation.final_checkpoint_hash().is_some());
    assert_eq!(attestation.metrics, metrics);

    // Verify
    let verify_result = verify_execution_attestation(&attestation, &verifying_key);
    assert!(verify_result.is_ok());
    assert!(verify_result.unwrap().valid);
}

#[test]
fn create_execution_attestation_requires_checkpoints() {
    let (signing_key, _) = create_keypair();
    let job_id = Uuid::new_v4();

    let result = ExecutionAttestation::create_and_sign(
        job_id,
        vec![], // No checkpoints
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    );

    assert!(result.is_err());
}

#[test]
fn execution_attestation_validates_checkpoint_sequence() {
    let (signing_key, _) = create_keypair();
    let job_id = Uuid::new_v4();

    // Create checkpoint with wrong sequence
    let mut bad_checkpoint = Checkpoint::new(0, b"data");
    bad_checkpoint.sequence = 5; // Should be 0

    let result = ExecutionAttestation::create_and_sign(
        job_id,
        vec![bad_checkpoint],
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    );

    assert!(result.is_err());
}

#[test]
fn verify_execution_attestation_success() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let mut chain = CheckpointChain::new(b"initial");
    chain.add_checkpoint(b"step 1");
    chain.add_checkpoint(b"final");

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    )
    .expect("should create attestation");

    let result = verify_execution_attestation(&attestation, &verifying_key);
    assert!(result.is_ok());

    let verification = result.unwrap();
    assert!(verification.valid);

    match verification.details {
        VerificationDetails::Execution { checkpoint_count, final_hash } => {
            assert_eq!(checkpoint_count, 3);
            assert!(final_hash.is_some());
        }
        _ => panic!("Expected Execution details"),
    }
}

#[test]
fn verify_execution_attestation_wrong_key_fails() {
    let (signing_key, _) = create_keypair();
    let (_, wrong_verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let mut chain = CheckpointChain::new(b"initial");
    chain.add_checkpoint(b"final");

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    )
    .expect("should create attestation");

    let result = verify_execution_attestation(&attestation, &wrong_verifying_key);
    assert!(result.is_err());
}

#[test]
fn verify_execution_with_original_data() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    // Create attestation with known data
    let checkpoint_data = vec![
        b"initial state".to_vec(),
        b"intermediate state".to_vec(),
        b"final state".to_vec(),
    ];

    let mut chain = CheckpointChain::new(&checkpoint_data[0]);
    chain.add_checkpoint(&checkpoint_data[1]);
    chain.add_checkpoint(&checkpoint_data[2]);

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    )
    .expect("should create attestation");

    // Verify with original data
    let result = verify_execution_with_data(&attestation, &verifying_key, &checkpoint_data);
    assert!(result.is_ok());
    assert!(result.unwrap().valid);
}

#[test]
fn verify_execution_with_wrong_data_fails() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let original_data = vec![
        b"initial".to_vec(),
        b"step".to_vec(),
        b"final".to_vec(),
    ];

    let mut chain = CheckpointChain::new(&original_data[0]);
    chain.add_checkpoint(&original_data[1]);
    chain.add_checkpoint(&original_data[2]);

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    )
    .expect("should create attestation");

    // Provide wrong data
    let wrong_data = vec![
        b"initial".to_vec(),
        b"TAMPERED".to_vec(), // Different!
        b"final".to_vec(),
    ];

    let result = verify_execution_with_data(&attestation, &verifying_key, &wrong_data);
    assert!(result.is_err());
}

#[test]
fn verify_execution_with_mismatched_data_count_fails() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let mut chain = CheckpointChain::new(b"initial");
    chain.add_checkpoint(b"step");
    chain.add_checkpoint(b"final");

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics::default(),
        &signing_key,
    )
    .expect("should create attestation");

    // Provide wrong number of data items
    let wrong_data = vec![b"only one".to_vec()];

    let result = verify_execution_with_data(&attestation, &verifying_key, &wrong_data);
    assert!(result.is_err());
}

#[test]
fn execution_attestation_serialization_preserves_validity() {
    let (signing_key, verifying_key) = create_keypair();
    let job_id = Uuid::new_v4();

    let mut chain = CheckpointChain::new(b"initial");
    chain.add_checkpoint(b"final");

    let attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::seconds(3600),
        ExecutionMetrics {
            gpu_utilization: 75.5,
            memory_used_mb: 8192,
            compute_ops: 100_000,
        },
        &signing_key,
    )
    .expect("should create attestation");

    // Serialize and deserialize
    let json = serde_json::to_string(&attestation).expect("should serialize");
    let deserialized: ExecutionAttestation =
        serde_json::from_str(&json).expect("should deserialize");

    // Verification should still work
    let result = verify_execution_attestation(&deserialized, &verifying_key);
    assert!(result.is_ok());
    assert!(result.unwrap().valid);
}

// ============================================================================
// Batch Verification Tests
// ============================================================================

#[test]
fn batch_verify_hardware_attestations() {
    let (key1, vk1) = create_keypair();
    let (key2, vk2) = create_keypair();
    let (_, wrong_key) = create_keypair();

    let att1 = HardwareAttestation::create_and_sign(
        Uuid::new_v4(),
        vec![create_test_gpu()],
        Duration::hours(24),
        &key1,
    )
    .expect("should create");

    let att2 = HardwareAttestation::create_and_sign(
        Uuid::new_v4(),
        vec![create_test_gpu()],
        Duration::hours(24),
        &key2,
    )
    .expect("should create");

    // Expired attestation
    let att3 = HardwareAttestation::create_and_sign(
        Uuid::new_v4(),
        vec![create_test_gpu()],
        Duration::hours(-1),
        &key1,
    )
    .expect("should create");

    let attestations = vec![
        (&att1, &vk1),        // Valid
        (&att2, &vk2),        // Valid
        (&att3, &vk1),        // Expired
        (&att1, &wrong_key),  // Wrong key
    ];

    let results = batch_verify_hardware(&attestations);

    assert_eq!(results.len(), 4);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_err()); // Expired
    assert!(results[3].is_err()); // Wrong key
}

#[test]
fn batch_verify_execution_attestations() {
    let (key1, vk1) = create_keypair();
    let (key2, vk2) = create_keypair();
    let (_, wrong_key) = create_keypair();

    let create_attestation = |key: &SigningKey| {
        let mut chain = CheckpointChain::new(b"init");
        chain.add_checkpoint(b"done");
        ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            key,
        )
        .expect("should create")
    };

    let att1 = create_attestation(&key1);
    let att2 = create_attestation(&key2);

    let attestations = vec![
        (&att1, &vk1),        // Valid
        (&att2, &vk2),        // Valid
        (&att1, &wrong_key),  // Wrong key
    ];

    let results = batch_verify_execution(&attestations);

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_ok());
    assert!(results[2].is_err());
}

// ============================================================================
// Full Attestation Flow Test
// ============================================================================

#[test]
fn full_attestation_flow_end_to_end() {
    let (signing_key, verifying_key) = create_keypair();
    let node_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();

    // Step 1: Create hardware attestation
    let gpus = vec![
        GpuInfo {
            vendor: molt_attestation::GpuVendor::Nvidia,
            model: "NVIDIA A100".to_string(),
            vram_mb: 81920,
            compute_capability: "8.0".to_string(),
        },
        GpuInfo {
            vendor: molt_attestation::GpuVendor::Nvidia,
            model: "NVIDIA A100".to_string(),
            vram_mb: 81920,
            compute_capability: "8.0".to_string(),
        },
    ];

    let hw_attestation =
        HardwareAttestation::create_and_sign(node_id, gpus, Duration::hours(24), &signing_key)
            .expect("should create hardware attestation");

    // Step 2: Verify hardware attestation
    let hw_verify = verify_hardware_attestation(&hw_attestation, &verifying_key);
    assert!(hw_verify.is_ok());
    assert!(hw_verify.unwrap().valid);

    // Step 3: Execute job and create checkpoints
    let checkpoint_data = vec![
        b"job_id:".iter().chain(job_id.as_bytes()).copied().collect::<Vec<u8>>(),
        b"loading model...".to_vec(),
        b"training epoch 1/10".to_vec(),
        b"training epoch 5/10".to_vec(),
        b"training epoch 10/10".to_vec(),
        b"saving model checkpoint".to_vec(),
        b"job complete: accuracy=0.95".to_vec(),
    ];

    let mut chain = CheckpointChain::new(&checkpoint_data[0]);
    for data in checkpoint_data.iter().skip(1) {
        chain.add_checkpoint(data);
    }

    // Step 4: Create execution attestation with checkpoint chain
    let metrics = ExecutionMetrics {
        gpu_utilization: 98.5,
        memory_used_mb: 156000, // ~150GB across 2 GPUs
        compute_ops: 1_000_000_000,
    };

    let exec_attestation = ExecutionAttestation::create_and_sign(
        job_id,
        chain.into_checkpoints(),
        Duration::hours(8),
        metrics,
        &signing_key,
    )
    .expect("should create execution attestation");

    // Step 5: Verify execution attestation
    let exec_verify = verify_execution_attestation(&exec_attestation, &verifying_key);
    assert!(exec_verify.is_ok());
    let exec_result = exec_verify.unwrap();
    assert!(exec_result.valid);

    match exec_result.details {
        VerificationDetails::Execution { checkpoint_count, final_hash } => {
            assert_eq!(checkpoint_count, 7);
            assert!(final_hash.is_some());
        }
        _ => panic!("Expected Execution details"),
    }

    // Step 6: Verify with original checkpoint data
    let full_verify = verify_execution_with_data(&exec_attestation, &verifying_key, &checkpoint_data);
    assert!(full_verify.is_ok());
    assert!(full_verify.unwrap().valid);

    // Step 7: Verify both attestations link to same provider
    assert_eq!(hw_attestation.node_id, node_id);
    assert_eq!(exec_attestation.job_id, job_id);

    // The final checkpoint hash can be used for result verification
    let final_hash = exec_attestation.final_checkpoint_hash();
    assert!(final_hash.is_some());
}
