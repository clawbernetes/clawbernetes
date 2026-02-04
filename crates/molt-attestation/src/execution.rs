#![allow(clippy::expect_used)]
//! Execution attestation — job completion proofs, checkpoint verification.

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AttestationError;

/// A checkpoint in the execution proving work progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Sequence number of this checkpoint.
    pub sequence: u64,
    /// Blake3 hash of the checkpoint data/state.
    pub hash: [u8; 32],
    /// Timestamp when checkpoint was created.
    pub timestamp: DateTime<Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint with the given data.
    #[must_use] 
    pub fn new(sequence: u64, data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        Self {
            sequence,
            hash: *hash.as_bytes(),
            timestamp: Utc::now(),
        }
    }

    /// Create a checkpoint that chains from a previous checkpoint.
    #[must_use] 
    pub fn chain_from(previous: &Self, data: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&previous.hash);
        hasher.update(data);
        let hash = hasher.finalize();

        Self {
            sequence: previous.sequence + 1,
            hash: *hash.as_bytes(),
            timestamp: Utc::now(),
        }
    }

    /// Verify that this checkpoint correctly chains from the previous one given the data.
    #[must_use] 
    pub fn verify_chain(&self, previous: &Self, data: &[u8]) -> bool {
        if self.sequence != previous.sequence + 1 {
            return false;
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(&previous.hash);
        hasher.update(data);
        let expected_hash = hasher.finalize();

        self.hash == *expected_hash.as_bytes()
    }
}

/// Metrics about the job execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// GPU utilization percentage (0.0 - 100.0).
    pub gpu_utilization: f64,
    /// Memory used in megabytes.
    pub memory_used_mb: u64,
    /// Number of compute operations performed.
    pub compute_ops: u64,
}

impl Default for ExecutionMetrics {
    fn default() -> Self {
        Self {
            gpu_utilization: 0.0,
            memory_used_mb: 0,
            compute_ops: 0,
        }
    }
}

/// Execution attestation proving job completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionAttestation {
    /// Unique job identifier.
    pub job_id: Uuid,
    /// Checkpoints recorded during execution.
    pub checkpoints: Vec<Checkpoint>,
    /// Total execution duration.
    pub duration: Duration,
    /// Execution metrics.
    pub metrics: ExecutionMetrics,
    /// When this attestation was created.
    pub timestamp: DateTime<Utc>,
    /// Ed25519 signature over the attestation data.
    #[serde(with = "signature_serde")]
    pub signature: Signature,
}

mod signature_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(sig: &Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = sig.to_bytes();
        serializer.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        let arr: [u8; 64] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("signature must be 64 bytes"))?;
        Ok(Signature::from_bytes(&arr))
    }
}

impl ExecutionAttestation {
    /// Create and sign a new execution attestation.
    ///
    /// # Errors
    ///
    /// Returns an error if there are no checkpoints or if the checkpoint chain is invalid.
    pub fn create_and_sign(
        job_id: Uuid,
        checkpoints: Vec<Checkpoint>,
        duration: Duration,
        metrics: ExecutionMetrics,
        signing_key: &SigningKey,
    ) -> Result<Self, AttestationError> {
        use ed25519_dalek::Signer;

        if checkpoints.is_empty() {
            return Err(AttestationError::InvalidExecutionProof(
                "at least one checkpoint is required".to_string(),
            ));
        }

        // Verify checkpoint sequence is valid
        for (i, checkpoint) in checkpoints.iter().enumerate() {
            if checkpoint.sequence != i as u64 {
                return Err(AttestationError::CheckpointVerification(format!(
                    "checkpoint sequence mismatch: expected {}, got {}",
                    i, checkpoint.sequence
                )));
            }
        }

        let timestamp = Utc::now();
        let message = Self::create_signing_message(job_id, &checkpoints, duration, &metrics, timestamp);
        let signature = signing_key.sign(&message);

        Ok(Self {
            job_id,
            checkpoints,
            duration,
            metrics,
            timestamp,
            signature,
        })
    }

    /// Verify the signature of this attestation.
    ///
    /// # Errors
    ///
    /// Returns `AttestationError::SignatureVerification` if the signature is invalid.
    pub fn verify_signature(&self, public_key: &VerifyingKey) -> Result<(), AttestationError> {
        use ed25519_dalek::Verifier;

        let message = Self::create_signing_message(
            self.job_id,
            &self.checkpoints,
            self.duration,
            &self.metrics,
            self.timestamp,
        );
        public_key
            .verify(&message, &self.signature)
            .map_err(|_| AttestationError::SignatureVerification)
    }

    /// Get the final checkpoint hash (the end state of execution).
    #[must_use] 
    pub fn final_checkpoint_hash(&self) -> Option<[u8; 32]> {
        self.checkpoints.last().map(|c| c.hash)
    }

    /// Get the number of checkpoints.
    #[must_use] 
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    fn create_signing_message(
        job_id: Uuid,
        checkpoints: &[Checkpoint],
        duration: Duration,
        metrics: &ExecutionMetrics,
        timestamp: DateTime<Utc>,
    ) -> Vec<u8> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"execution_attestation_v1");
        hasher.update(job_id.as_bytes());

        for checkpoint in checkpoints {
            hasher.update(&checkpoint.sequence.to_le_bytes());
            hasher.update(&checkpoint.hash);
            hasher.update(&checkpoint.timestamp.timestamp().to_le_bytes());
        }

        hasher.update(&duration.num_milliseconds().to_le_bytes());
        hasher.update(&metrics.gpu_utilization.to_le_bytes());
        hasher.update(&metrics.memory_used_mb.to_le_bytes());
        hasher.update(&metrics.compute_ops.to_le_bytes());
        hasher.update(&timestamp.timestamp().to_le_bytes());

        hasher.finalize().as_bytes().to_vec()
    }
}

/// Builder for creating checkpoint chains during execution.
#[derive(Debug)]
pub struct CheckpointChain {
    checkpoints: Vec<Checkpoint>,
}

impl CheckpointChain {
    /// Start a new checkpoint chain with initial data.
    #[must_use] 
    pub fn new(initial_data: &[u8]) -> Self {
        let checkpoint = Checkpoint::new(0, initial_data);
        Self {
            checkpoints: vec![checkpoint],
        }
    }

    /// Add a new checkpoint to the chain.
    pub fn add_checkpoint(&mut self, data: &[u8]) {
        let previous = self.checkpoints.last().expect("chain always has at least one checkpoint");
        let next = Checkpoint::chain_from(previous, data);
        self.checkpoints.push(next);
    }

    /// Consume the chain and return the checkpoints.
    #[must_use] 
    pub fn into_checkpoints(self) -> Vec<Checkpoint> {
        self.checkpoints
    }

    /// Get the current number of checkpoints.
    #[must_use] 
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Check if the chain is empty (it never should be after construction).
    #[must_use] 
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_keypair() -> (SigningKey, VerifyingKey) {
        use rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn test_checkpoint_creation() {
        let data = b"test data";
        let checkpoint = Checkpoint::new(0, data);

        assert_eq!(checkpoint.sequence, 0);
        assert_eq!(checkpoint.hash, *blake3::hash(data).as_bytes());
    }

    #[test]
    fn test_checkpoint_chaining() {
        let first = Checkpoint::new(0, b"first");
        let second = Checkpoint::chain_from(&first, b"second");

        assert_eq!(second.sequence, 1);
        assert!(second.verify_chain(&first, b"second"));
    }

    #[test]
    fn test_checkpoint_chain_verification_fails_with_wrong_data() {
        let first = Checkpoint::new(0, b"first");
        let second = Checkpoint::chain_from(&first, b"second");

        assert!(!second.verify_chain(&first, b"wrong data"));
    }

    #[test]
    fn test_checkpoint_chain_verification_fails_with_wrong_sequence() {
        let first = Checkpoint::new(0, b"first");
        let mut second = Checkpoint::chain_from(&first, b"second");
        second.sequence = 5; // Wrong sequence

        assert!(!second.verify_chain(&first, b"second"));
    }

    #[test]
    fn test_checkpoint_chain_builder() {
        let mut chain = CheckpointChain::new(b"initial");
        assert_eq!(chain.len(), 1);

        chain.add_checkpoint(b"step1");
        chain.add_checkpoint(b"step2");
        chain.add_checkpoint(b"final");

        assert_eq!(chain.len(), 4);

        let checkpoints = chain.into_checkpoints();
        assert_eq!(checkpoints.len(), 4);

        // Verify the chain
        for i in 1..checkpoints.len() {
            assert_eq!(checkpoints[i].sequence, i as u64);
        }
    }

    #[test]
    fn test_execution_attestation_creation() {
        let (signing_key, _) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");
        let checkpoints = chain.into_checkpoints();

        let metrics = ExecutionMetrics {
            gpu_utilization: 85.5,
            memory_used_mb: 4096,
            compute_ops: 1_000_000,
        };

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            checkpoints,
            Duration::seconds(3600),
            metrics.clone(),
            &signing_key,
        )
        .expect("should create attestation");

        assert_eq!(attestation.job_id, job_id);
        assert_eq!(attestation.checkpoint_count(), 2);
        assert_eq!(attestation.metrics, metrics);
    }

    #[test]
    fn test_execution_attestation_requires_checkpoints() {
        let (signing_key, _) = create_keypair();
        let job_id = Uuid::new_v4();

        let result = ExecutionAttestation::create_and_sign(
            job_id,
            vec![], // No checkpoints
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        );

        assert!(matches!(
            result,
            Err(AttestationError::InvalidExecutionProof(_))
        ));
    }

    #[test]
    fn test_execution_attestation_verify_signature() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        assert!(attestation.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_execution_attestation_wrong_key() {
        let (signing_key, _) = create_keypair();
        let (_, wrong_verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        let result = attestation.verify_signature(&wrong_verifying_key);
        assert!(matches!(result, Err(AttestationError::SignatureVerification)));
    }

    #[test]
    fn test_final_checkpoint_hash() {
        let (signing_key, _) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");
        chain.add_checkpoint(b"final");
        let checkpoints = chain.into_checkpoints();
        let expected_final_hash = checkpoints.last().unwrap().hash;

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            checkpoints,
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        assert_eq!(attestation.final_checkpoint_hash(), Some(expected_final_hash));
    }

    #[test]
    fn test_execution_attestation_serialization() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics {
                gpu_utilization: 75.0,
                memory_used_mb: 2048,
                compute_ops: 500_000,
            },
            &signing_key,
        )
        .expect("should create attestation");

        let json = serde_json::to_string(&attestation).expect("should serialize");
        let deserialized: ExecutionAttestation =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.job_id, attestation.job_id);
        assert_eq!(deserialized.checkpoint_count(), attestation.checkpoint_count());
        assert!(deserialized.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_checkpoint_sequence_validation() {
        let (signing_key, _) = create_keypair();
        let job_id = Uuid::new_v4();

        // Create checkpoints with wrong sequence
        let mut checkpoint = Checkpoint::new(0, b"initial");
        checkpoint.sequence = 5; // Wrong!

        let result = ExecutionAttestation::create_and_sign(
            job_id,
            vec![checkpoint],
            Duration::seconds(3600),
            ExecutionMetrics::default(),
            &signing_key,
        );

        assert!(matches!(
            result,
            Err(AttestationError::CheckpointVerification(_))
        ));
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn test_checkpoint_hash_is_deterministic() {
        let data = b"test data";
        let checkpoint1 = Checkpoint::new(0, data);
        let checkpoint2 = Checkpoint::new(0, data);

        assert_eq!(checkpoint1.hash, checkpoint2.hash);
    }

    #[test]
    fn test_checkpoint_hash_differs_for_different_data() {
        let checkpoint1 = Checkpoint::new(0, b"data1");
        let checkpoint2 = Checkpoint::new(0, b"data2");

        assert_ne!(checkpoint1.hash, checkpoint2.hash);
    }

    #[test]
    fn test_checkpoint_chain_from_preserves_sequence() {
        let first = Checkpoint::new(0, b"first");
        let second = Checkpoint::chain_from(&first, b"second");
        let third = Checkpoint::chain_from(&second, b"third");

        assert_eq!(first.sequence, 0);
        assert_eq!(second.sequence, 1);
        assert_eq!(third.sequence, 2);
    }

    #[test]
    fn test_checkpoint_chain_hash_includes_previous() {
        let first = Checkpoint::new(0, b"data");
        let second1 = Checkpoint::chain_from(&first, b"same");
        
        // Different first checkpoint
        let alt_first = Checkpoint::new(0, b"different");
        let second2 = Checkpoint::chain_from(&alt_first, b"same");

        // Same data but different chain = different hash
        assert_ne!(second1.hash, second2.hash);
    }

    #[test]
    fn test_execution_metrics_equality() {
        let metrics1 = ExecutionMetrics {
            gpu_utilization: 50.0,
            memory_used_mb: 1024,
            compute_ops: 1000,
        };
        let metrics2 = ExecutionMetrics {
            gpu_utilization: 50.0,
            memory_used_mb: 1024,
            compute_ops: 1000,
        };
        let metrics3 = ExecutionMetrics {
            gpu_utilization: 60.0,
            memory_used_mb: 1024,
            compute_ops: 1000,
        };

        assert_eq!(metrics1, metrics2);
        assert_ne!(metrics1, metrics3);
    }

    #[test]
    fn test_execution_metrics_default() {
        let metrics = ExecutionMetrics::default();
        assert!((metrics.gpu_utilization - 0.0).abs() < f64::EPSILON);
        assert_eq!(metrics.memory_used_mb, 0);
        assert_eq!(metrics.compute_ops, 0);
    }

    #[test]
    fn test_execution_metrics_clone() {
        let metrics = ExecutionMetrics {
            gpu_utilization: 75.5,
            memory_used_mb: 4096,
            compute_ops: 999999,
        };
        let cloned = metrics.clone();
        assert_eq!(metrics, cloned);
    }

    #[test]
    fn test_checkpoint_chain_length() {
        let mut chain = CheckpointChain::new(b"initial");
        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());

        chain.add_checkpoint(b"step1");
        assert_eq!(chain.len(), 2);

        chain.add_checkpoint(b"step2");
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn test_checkpoint_chain_into_checkpoints() {
        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");
        chain.add_checkpoint(b"step2");

        let checkpoints = chain.into_checkpoints();
        assert_eq!(checkpoints.len(), 3);
        
        for (i, checkpoint) in checkpoints.iter().enumerate() {
            assert_eq!(checkpoint.sequence, i as u64);
        }
    }

    #[test]
    fn test_long_checkpoint_chain() {
        let mut chain = CheckpointChain::new(b"start");
        
        for i in 0..100 {
            chain.add_checkpoint(format!("step_{}", i).as_bytes());
        }

        assert_eq!(chain.len(), 101);
        
        let checkpoints = chain.into_checkpoints();
        
        // Verify all sequences are correct
        for (i, checkpoint) in checkpoints.iter().enumerate() {
            assert_eq!(checkpoint.sequence, i as u64);
        }
    }

    #[test]
    fn test_execution_attestation_duration() {
        let (signing_key, _) = create_keypair();
        let job_id = Uuid::new_v4();
        let duration = Duration::hours(5);

        let chain = CheckpointChain::new(b"data");
        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            duration,
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        assert_eq!(attestation.duration.num_hours(), 5);
    }

    #[test]
    fn test_execution_attestation_zero_duration() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let chain = CheckpointChain::new(b"data");
        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::zero(),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        assert!(attestation.verify_signature(&verifying_key).is_ok());
        assert_eq!(attestation.duration.num_seconds(), 0);
    }

    #[test]
    fn test_execution_attestation_large_metrics() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let metrics = ExecutionMetrics {
            gpu_utilization: 100.0,
            memory_used_mb: u64::MAX,
            compute_ops: u64::MAX,
        };

        let chain = CheckpointChain::new(b"data");
        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(1),
            metrics,
            &signing_key,
        )
        .expect("should create attestation");

        assert!(attestation.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_single_checkpoint_attestation() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let chain = CheckpointChain::new(b"only_one");
        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(60),
            ExecutionMetrics::default(),
            &signing_key,
        )
        .expect("should create attestation");

        assert_eq!(attestation.checkpoint_count(), 1);
        assert!(attestation.final_checkpoint_hash().is_some());
        assert!(attestation.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_checkpoint_timestamp_increases() {
        let mut chain = CheckpointChain::new(b"initial");
        
        // Add checkpoints with small delays
        std::thread::sleep(std::time::Duration::from_millis(10));
        chain.add_checkpoint(b"step1");
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        chain.add_checkpoint(b"step2");

        let checkpoints = chain.into_checkpoints();
        
        // Each checkpoint should have a timestamp >= the previous
        for i in 1..checkpoints.len() {
            assert!(checkpoints[i].timestamp >= checkpoints[i - 1].timestamp);
        }
    }

    #[test]
    fn test_execution_attestation_json_bytes_serialization() {
        let (signing_key, verifying_key) = create_keypair();
        let job_id = Uuid::new_v4();

        let mut chain = CheckpointChain::new(b"initial");
        chain.add_checkpoint(b"step1");

        let attestation = ExecutionAttestation::create_and_sign(
            job_id,
            chain.into_checkpoints(),
            Duration::seconds(3600),
            ExecutionMetrics {
                gpu_utilization: 50.0,
                memory_used_mb: 1024,
                compute_ops: 10000,
            },
            &signing_key,
        )
        .expect("should create attestation");

        let bytes = serde_json::to_vec(&attestation).expect("should serialize");
        let deserialized: ExecutionAttestation =
            serde_json::from_slice(&bytes).expect("should deserialize");

        assert_eq!(deserialized.job_id, attestation.job_id);
        assert!(deserialized.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_checkpoint_debug_format() {
        let checkpoint = Checkpoint::new(0, b"test");
        let debug = format!("{:?}", checkpoint);
        assert!(debug.contains("Checkpoint"));
        assert!(debug.contains("sequence: 0"));
    }

    #[test]
    fn test_checkpoint_chain_debug_format() {
        let chain = CheckpointChain::new(b"initial");
        let debug = format!("{:?}", chain);
        assert!(debug.contains("CheckpointChain"));
    }

    #[test]
    fn test_execution_metrics_debug_format() {
        let metrics = ExecutionMetrics {
            gpu_utilization: 75.0,
            memory_used_mb: 4096,
            compute_ops: 1000000,
        };
        let debug = format!("{:?}", metrics);
        assert!(debug.contains("ExecutionMetrics"));
    }

    #[test]
    fn test_verify_chain_with_correct_data() {
        let first = Checkpoint::new(0, b"first");
        let second = Checkpoint::chain_from(&first, b"second");
        
        assert!(second.verify_chain(&first, b"second"));
    }

    #[test]
    fn test_verify_chain_with_empty_data() {
        let first = Checkpoint::new(0, b"");
        let second = Checkpoint::chain_from(&first, b"");
        
        assert!(second.verify_chain(&first, b""));
    }

    #[test]
    fn test_verify_chain_with_large_data() {
        let large_data = vec![0u8; 1024 * 1024]; // 1MB
        let first = Checkpoint::new(0, &large_data);
        let second = Checkpoint::chain_from(&first, &large_data);
        
        assert!(second.verify_chain(&first, &large_data));
    }
}
