//! Proof verification logic.
//!
//! This module provides unified verification functions for hardware and execution attestations.

use ed25519_dalek::VerifyingKey;

use crate::error::AttestationError;
use crate::execution::ExecutionAttestation;
use crate::hardware::HardwareAttestation;

/// Verification result containing details about the verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationResult {
    /// Whether the verification succeeded.
    pub valid: bool,
    /// Details about what was verified.
    pub details: VerificationDetails,
}

/// Details about what was verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationDetails {
    /// Hardware attestation verification details.
    Hardware {
        /// Number of GPUs in the attestation.
        gpu_count: usize,
        /// Whether the attestation is currently valid (not expired).
        not_expired: bool,
    },
    /// Execution attestation verification details.
    Execution {
        /// Number of checkpoints verified.
        checkpoint_count: usize,
        /// Final checkpoint hash.
        final_hash: Option<[u8; 32]>,
    },
}

/// Verify a hardware attestation.
///
/// This function verifies:
/// 1. The Ed25519 signature is valid for the given public key
/// 2. The attestation has not expired
///
/// # Errors
///
/// Returns `AttestationError::SignatureVerification` if the signature is invalid.
/// Returns `AttestationError::Expired` if the attestation has expired.
///
/// # Examples
///
/// ```ignore
/// let result = verify_hardware_attestation(&attestation, &public_key)?;
/// assert!(result.valid);
/// ```
pub fn verify_hardware_attestation(
    attestation: &HardwareAttestation,
    public_key: &VerifyingKey,
) -> Result<VerificationResult, AttestationError> {
    // Check expiry first (cheaper than signature verification)
    let not_expired = !attestation.is_expired();
    if !not_expired {
        return Err(AttestationError::Expired);
    }

    // Verify signature
    attestation.verify_signature(public_key)?;

    Ok(VerificationResult {
        valid: true,
        details: VerificationDetails::Hardware {
            gpu_count: attestation.gpus.len(),
            not_expired,
        },
    })
}

/// Verify an execution attestation.
///
/// This function verifies:
/// 1. The Ed25519 signature is valid for the given public key
/// 2. The checkpoint chain has valid sequence numbers
///
/// # Errors
///
/// Returns `AttestationError::SignatureVerification` if the signature is invalid.
///
/// # Examples
///
/// ```ignore
/// let result = verify_execution_attestation(&attestation, &public_key)?;
/// assert!(result.valid);
/// ```
pub fn verify_execution_attestation(
    attestation: &ExecutionAttestation,
    public_key: &VerifyingKey,
) -> Result<VerificationResult, AttestationError> {
    // Verify checkpoint sequence integrity
    for (i, checkpoint) in attestation.checkpoints.iter().enumerate() {
        if checkpoint.sequence != i as u64 {
            return Err(AttestationError::CheckpointVerification(format!(
                "checkpoint sequence mismatch at index {}: expected {}, got {}",
                i, i, checkpoint.sequence
            )));
        }
    }

    // Verify signature
    attestation.verify_signature(public_key)?;

    Ok(VerificationResult {
        valid: true,
        details: VerificationDetails::Execution {
            checkpoint_count: attestation.checkpoint_count(),
            final_hash: attestation.final_checkpoint_hash(),
        },
    })
}

/// Verify an execution attestation including checkpoint hash chain verification.
///
/// This is a more thorough verification that requires the original checkpoint data
/// to verify the hash chain integrity.
///
/// # Errors
///
/// Returns `AttestationError::CheckpointVerification` if the hash chain is invalid.
/// Returns `AttestationError::SignatureVerification` if the signature is invalid.
pub fn verify_execution_with_data(
    attestation: &ExecutionAttestation,
    public_key: &VerifyingKey,
    checkpoint_data: &[Vec<u8>],
) -> Result<VerificationResult, AttestationError> {
    // First do basic verification
    verify_execution_attestation(attestation, public_key)?;

    // Verify we have the right amount of data
    if checkpoint_data.len() != attestation.checkpoints.len() {
        return Err(AttestationError::CheckpointVerification(format!(
            "checkpoint data count mismatch: expected {}, got {}",
            attestation.checkpoints.len(),
            checkpoint_data.len()
        )));
    }

    // Verify first checkpoint hash
    if !attestation.checkpoints.is_empty() {
        let expected_first = blake3::hash(&checkpoint_data[0]);
        if attestation.checkpoints[0].hash != *expected_first.as_bytes() {
            return Err(AttestationError::CheckpointVerification(
                "first checkpoint hash mismatch".to_string(),
            ));
        }
    }

    // Verify checkpoint chain
    for i in 1..attestation.checkpoints.len() {
        let verified = attestation.checkpoints[i]
            .verify_chain(&attestation.checkpoints[i - 1], &checkpoint_data[i]);
        if !verified {
            return Err(AttestationError::CheckpointVerification(format!(
                "checkpoint chain verification failed at index {i}"
            )));
        }
    }

    Ok(VerificationResult {
        valid: true,
        details: VerificationDetails::Execution {
            checkpoint_count: attestation.checkpoint_count(),
            final_hash: attestation.final_checkpoint_hash(),
        },
    })
}

/// Batch verify multiple hardware attestations.
///
/// Returns a vector of results, one for each attestation.
/// Continues verifying even if some attestations fail.
#[must_use] 
pub fn batch_verify_hardware(
    attestations: &[(&HardwareAttestation, &VerifyingKey)],
) -> Vec<Result<VerificationResult, AttestationError>> {
    attestations
        .iter()
        .map(|(attestation, public_key)| verify_hardware_attestation(attestation, public_key))
        .collect()
}

/// Batch verify multiple execution attestations.
///
/// Returns a vector of results, one for each attestation.
/// Continues verifying even if some attestations fail.
#[must_use] 
pub fn batch_verify_execution(
    attestations: &[(&ExecutionAttestation, &VerifyingKey)],
) -> Vec<Result<VerificationResult, AttestationError>> {
    attestations
        .iter()
        .map(|(attestation, public_key)| verify_execution_attestation(attestation, public_key))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{CheckpointChain, ExecutionMetrics};
    use crate::hardware::GpuInfo;
    use chrono::Duration;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use uuid::Uuid;

    fn create_keypair() -> (SigningKey, VerifyingKey) {
        use rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn create_test_hardware_attestation(
        signing_key: &SigningKey,
        validity_hours: i64,
    ) -> HardwareAttestation {
        use crate::hardware::GpuVendor;
        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };

        HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![gpu],
            Duration::hours(validity_hours),
            signing_key,
        )
        .expect("should create attestation")
    }

    fn create_test_execution_attestation(signing_key: &SigningKey) -> ExecutionAttestation {
        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");
        chain.add_checkpoint(b"final");

        ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            signing_key,
        )
        .expect("should create attestation")
    }

    #[test]
    fn test_verify_hardware_attestation_valid() {
        let (signing_key, verifying_key) = create_keypair();
        let attestation = create_test_hardware_attestation(&signing_key, 24);

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");

        assert!(result.valid);
        match result.details {
            VerificationDetails::Hardware {
                gpu_count,
                not_expired,
            } => {
                assert_eq!(gpu_count, 1);
                assert!(not_expired);
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_verify_hardware_attestation_expired() {
        let (signing_key, verifying_key) = create_keypair();
        let attestation = create_test_hardware_attestation(&signing_key, -1); // Already expired

        let result = verify_hardware_attestation(&attestation, &verifying_key);
        assert!(matches!(result, Err(AttestationError::Expired)));
    }

    #[test]
    fn test_verify_hardware_attestation_wrong_key() {
        let (signing_key, _) = create_keypair();
        let (_, wrong_key) = create_keypair();
        let attestation = create_test_hardware_attestation(&signing_key, 24);

        let result = verify_hardware_attestation(&attestation, &wrong_key);
        assert!(matches!(
            result,
            Err(AttestationError::SignatureVerification)
        ));
    }

    #[test]
    fn test_verify_execution_attestation_valid() {
        let (signing_key, verifying_key) = create_keypair();
        let attestation = create_test_execution_attestation(&signing_key);

        let result =
            verify_execution_attestation(&attestation, &verifying_key).expect("should verify");

        assert!(result.valid);
        match result.details {
            VerificationDetails::Execution {
                checkpoint_count,
                final_hash,
            } => {
                assert_eq!(checkpoint_count, 3);
                assert!(final_hash.is_some());
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_verify_execution_attestation_wrong_key() {
        let (signing_key, _) = create_keypair();
        let (_, wrong_key) = create_keypair();
        let attestation = create_test_execution_attestation(&signing_key);

        let result = verify_execution_attestation(&attestation, &wrong_key);
        assert!(matches!(
            result,
            Err(AttestationError::SignatureVerification)
        ));
    }

    #[test]
    fn test_verify_execution_with_data_valid() {
        let (signing_key, verifying_key) = create_keypair();

        // Create attestation with known data
        let data = vec![
            b"initial".to_vec(),
            b"step1".to_vec(),
            b"final".to_vec(),
        ];

        let mut chain = CheckpointChain::new(&data[0]);
        chain.add_checkpoint(&data[1]);
        chain.add_checkpoint(&data[2]);

        let attestation = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_execution_with_data(&attestation, &verifying_key, &data).expect("should verify");

        assert!(result.valid);
    }

    #[test]
    fn test_verify_execution_with_data_wrong_data() {
        let (signing_key, verifying_key) = create_keypair();

        let data = vec![
            b"initial".to_vec(),
            b"step1".to_vec(),
            b"final".to_vec(),
        ];

        let mut chain = CheckpointChain::new(&data[0]);
        chain.add_checkpoint(&data[1]);
        chain.add_checkpoint(&data[2]);

        let attestation = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        // Provide wrong data
        let wrong_data = vec![
            b"initial".to_vec(),
            b"wrong".to_vec(), // Different!
            b"final".to_vec(),
        ];

        let result = verify_execution_with_data(&attestation, &verifying_key, &wrong_data);
        assert!(matches!(
            result,
            Err(AttestationError::CheckpointVerification(_))
        ));
    }

    #[test]
    fn test_verify_execution_with_data_count_mismatch() {
        let (signing_key, verifying_key) = create_keypair();
        let attestation = create_test_execution_attestation(&signing_key);

        // Provide wrong number of data items
        let wrong_data = vec![b"only one".to_vec()];

        let result = verify_execution_with_data(&attestation, &verifying_key, &wrong_data);
        assert!(matches!(
            result,
            Err(AttestationError::CheckpointVerification(_))
        ));
    }

    #[test]
    fn test_batch_verify_hardware() {
        let (signing_key1, verifying_key1) = create_keypair();
        let (signing_key2, verifying_key2) = create_keypair();
        let (_, wrong_key) = create_keypair();

        let attestation1 = create_test_hardware_attestation(&signing_key1, 24);
        let attestation2 = create_test_hardware_attestation(&signing_key2, 24);
        let attestation3 = create_test_hardware_attestation(&signing_key1, -1); // Expired

        let attestations = vec![
            (&attestation1, &verifying_key1), // Valid
            (&attestation2, &verifying_key2), // Valid
            (&attestation3, &verifying_key1), // Expired
            (&attestation1, &wrong_key),      // Wrong key
        ];

        let results = batch_verify_hardware(&attestations);

        assert_eq!(results.len(), 4);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert!(matches!(results[2], Err(AttestationError::Expired)));
        assert!(matches!(
            results[3],
            Err(AttestationError::SignatureVerification)
        ));
    }

    #[test]
    fn test_batch_verify_execution() {
        let (signing_key1, verifying_key1) = create_keypair();
        let (signing_key2, verifying_key2) = create_keypair();
        let (_, wrong_key) = create_keypair();

        let attestation1 = create_test_execution_attestation(&signing_key1);
        let attestation2 = create_test_execution_attestation(&signing_key2);

        let attestations = vec![
            (&attestation1, &verifying_key1), // Valid
            (&attestation2, &verifying_key2), // Valid
            (&attestation1, &wrong_key),      // Wrong key
        ];

        let results = batch_verify_execution(&attestations);

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert!(matches!(
            results[2],
            Err(AttestationError::SignatureVerification)
        ));
    }

    #[test]
    fn test_verification_result_equality() {
        let result1 = VerificationResult {
            valid: true,
            details: VerificationDetails::Hardware {
                gpu_count: 2,
                not_expired: true,
            },
        };

        let result2 = VerificationResult {
            valid: true,
            details: VerificationDetails::Hardware {
                gpu_count: 2,
                not_expired: true,
            },
        };

        assert_eq!(result1, result2);
    }

    // =========================================================================
    // Security Edge Case Tests
    // =========================================================================

    #[test]
    fn test_empty_gpu_list_attestation() {
        let (signing_key, verifying_key) = create_keypair();

        // Create attestation with no GPUs
        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![], // Empty GPU list
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");

        assert!(result.valid);
        match result.details {
            VerificationDetails::Hardware { gpu_count, .. } => {
                assert_eq!(gpu_count, 0);
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_multi_gpu_attestation() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        let gpus = vec![
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "H100".to_string(),
                vram_mb: 80000,
                compute_capability: "9.0".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "H100".to_string(),
                vram_mb: 80000,
                compute_capability: "9.0".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "A100".to_string(),
                vram_mb: 40000,
                compute_capability: "8.0".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "A100".to_string(),
                vram_mb: 40000,
                compute_capability: "8.0".to_string(),
            },
        ];

        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            gpus,
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");

        assert!(result.valid);
        match result.details {
            VerificationDetails::Hardware { gpu_count, .. } => {
                assert_eq!(gpu_count, 4);
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_very_long_validity_period() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        // 10 years validity
        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU".to_string(),
                vram_mb: 1000,
                compute_capability: "1.0".to_string(),
            }],
            Duration::days(3650),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
    }

    #[test]
    fn test_attestation_expiry_boundary() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        // Create attestation that expires in 1 second
        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU".to_string(),
                vram_mb: 1000,
                compute_capability: "1.0".to_string(),
            }],
            Duration::seconds(1),
            &signing_key,
        )
        .expect("should create attestation");

        // Should still be valid immediately
        let result = verify_hardware_attestation(&attestation, &verifying_key);
        assert!(result.is_ok());
    }

    #[test]
    fn test_replay_attack_detection_different_node() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        // Create attestation for node1
        let attestation = HardwareAttestation::create_and_sign(
            node1,
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU".to_string(),
                vram_mb: 1000,
                compute_capability: "1.0".to_string(),
            }],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        // Attestation is valid but for node1, not node2
        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
        assert_eq!(attestation.node_id, node1);
        assert_ne!(attestation.node_id, node2);
    }

    #[test]
    fn test_checkpoint_chain_integrity() {
        let (signing_key, verifying_key) = create_keypair();

        // Create a proper checkpoint chain
        let mut chain = CheckpointChain::new(b"initial_data");
        chain.add_checkpoint(b"step_1");
        chain.add_checkpoint(b"step_2");
        chain.add_checkpoint(b"final_step");

        let attestation = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        // Verification should succeed for properly chained checkpoints
        let result =
            verify_execution_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
        match result.details {
            VerificationDetails::Execution {
                checkpoint_count, ..
            } => {
                assert_eq!(checkpoint_count, 4);
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_empty_checkpoint_chain_rejected() {
        let (signing_key, _verifying_key) = create_keypair();

        // Empty checkpoints should be rejected
        let result = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![],
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        );

        // Creation should fail - at least one checkpoint is required
        assert!(result.is_err());
    }

    #[test]
    fn test_large_checkpoint_chain() {
        let (signing_key, verifying_key) = create_keypair();

        // Create a large checkpoint chain
        let mut chain = CheckpointChain::new(b"start");
        for i in 0..100 {
            chain.add_checkpoint(&format!("step_{}", i).into_bytes());
        }

        let attestation = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_execution_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
        match result.details {
            VerificationDetails::Execution {
                checkpoint_count, ..
            } => {
                assert_eq!(checkpoint_count, 101); // 1 initial + 100 steps
            }
            _ => panic!("unexpected details type"),
        }
    }

    #[test]
    fn test_execution_metrics_preserved() {
        let (signing_key, verifying_key) = create_keypair();

        let metrics = ExecutionMetrics {
            gpu_utilization: 85.5,
            memory_used_mb: 32768,
            compute_ops: 1_000_000_000,
        };

        let chain = CheckpointChain::new(b"data");
        let attestation = ExecutionAttestation::create_and_sign(
            Uuid::new_v4(),
            chain.into_checkpoints(),
            Duration::seconds(3600),
            metrics,
            &signing_key,
        )
        .expect("should create attestation");

        // Verify the attestation
        let result =
            verify_execution_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);

        // Verify metrics are preserved
        assert!((attestation.metrics.gpu_utilization - 85.5).abs() < f64::EPSILON);
        assert_eq!(attestation.metrics.memory_used_mb, 32768);
        assert_eq!(attestation.metrics.compute_ops, 1_000_000_000);
    }

    #[test]
    fn test_special_characters_in_gpu_model() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        let gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "RTX™ 4090 \"Super\" <special>".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9+".to_string(),
        };

        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![gpu],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
    }

    #[test]
    fn test_zero_vram_gpu() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        let gpu = GpuInfo {
            vendor: GpuVendor::Unknown,
            model: "Virtual GPU".to_string(),
            vram_mb: 0, // Edge case: zero VRAM
            compute_capability: "0.0".to_string(),
        };

        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![gpu],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
    }

    #[test]
    fn test_max_vram_gpu() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        let gpu = GpuInfo {
            vendor: GpuVendor::Unknown,
            model: "Future GPU".to_string(),
            vram_mb: u64::MAX, // Maximum possible VRAM
            compute_capability: "99.9".to_string(),
        };

        let attestation = HardwareAttestation::create_and_sign(
            Uuid::new_v4(),
            vec![gpu],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
    }

    #[test]
    fn test_concurrent_attestations_same_node() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();

        // Create multiple attestations for the same node
        let attestation1 = HardwareAttestation::create_and_sign(
            node_id,
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU v1".to_string(),
                vram_mb: 1000,
                compute_capability: "1.0".to_string(),
            }],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let attestation2 = HardwareAttestation::create_and_sign(
            node_id,
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU v2".to_string(), // Different config
                vram_mb: 2000,
                compute_capability: "2.0".to_string(),
            }],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        // Both should be valid independently
        let result1 =
            verify_hardware_attestation(&attestation1, &verifying_key).expect("should verify");
        let result2 =
            verify_hardware_attestation(&attestation2, &verifying_key).expect("should verify");

        assert!(result1.valid);
        assert!(result2.valid);

        // But they have different timestamps
        assert!(attestation2.timestamp >= attestation1.timestamp);
    }

    #[test]
    fn test_nil_uuid_node_id() {
        use crate::hardware::GpuVendor;
        let (signing_key, verifying_key) = create_keypair();

        // Use nil UUID (all zeros)
        let attestation = HardwareAttestation::create_and_sign(
            Uuid::nil(),
            vec![GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "GPU".to_string(),
                vram_mb: 1000,
                compute_capability: "1.0".to_string(),
            }],
            Duration::hours(24),
            &signing_key,
        )
        .expect("should create attestation");

        let result =
            verify_hardware_attestation(&attestation, &verifying_key).expect("should verify");
        assert!(result.valid);
    }
}
