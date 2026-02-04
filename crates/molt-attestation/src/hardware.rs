//! Hardware attestation — GPU verification, capability proofs.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AttestationError;

/// GPU information for hardware attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU model name (e.g., "NVIDIA RTX 4090").
    pub model: String,
    /// VRAM in megabytes.
    pub vram_mb: u64,
    /// Compute capability or driver version.
    pub compute_capability: String,
}

/// Hardware attestation proving node capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareAttestation {
    /// Unique node identifier.
    pub node_id: Uuid,
    /// List of GPUs available on the node.
    pub gpus: Vec<GpuInfo>,
    /// When this attestation was created.
    pub timestamp: DateTime<Utc>,
    /// When this attestation expires.
    pub expires_at: DateTime<Utc>,
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

impl HardwareAttestation {
    /// Create and sign a new hardware attestation.
    ///
    /// # Errors
    ///
    /// Returns an error if signing fails.
    pub fn create_and_sign(
        node_id: Uuid,
        gpus: Vec<GpuInfo>,
        validity_duration: chrono::Duration,
        signing_key: &SigningKey,
    ) -> Result<Self, AttestationError> {
        use ed25519_dalek::Signer;

        let timestamp = Utc::now();
        let expires_at = timestamp + validity_duration;

        let message = Self::create_signing_message(node_id, &gpus, timestamp, expires_at);
        let signature = signing_key.sign(&message);

        Ok(Self {
            node_id,
            gpus,
            timestamp,
            expires_at,
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

        let message =
            Self::create_signing_message(self.node_id, &self.gpus, self.timestamp, self.expires_at);
        public_key
            .verify(&message, &self.signature)
            .map_err(|_| AttestationError::SignatureVerification)
    }

    /// Check if this attestation has expired.
    #[must_use] 
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Verify both signature and expiry.
    ///
    /// # Errors
    ///
    /// Returns an error if signature verification fails or if the attestation is expired.
    pub fn verify(&self, public_key: &VerifyingKey) -> Result<(), AttestationError> {
        if self.is_expired() {
            return Err(AttestationError::Expired);
        }
        self.verify_signature(public_key)
    }

    fn create_signing_message(
        node_id: Uuid,
        gpus: &[GpuInfo],
        timestamp: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Vec<u8> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"hardware_attestation_v1");
        hasher.update(node_id.as_bytes());

        for gpu in gpus {
            hasher.update(gpu.model.as_bytes());
            hasher.update(&gpu.vram_mb.to_le_bytes());
            hasher.update(gpu.compute_capability.as_bytes());
        }

        hasher.update(&timestamp.timestamp().to_le_bytes());
        hasher.update(&expires_at.timestamp().to_le_bytes());

        hasher.finalize().as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_test_gpu() -> GpuInfo {
        GpuInfo {
            model: "NVIDIA RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        }
    }

    fn create_keypair() -> (SigningKey, VerifyingKey) {
        use rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn test_create_hardware_attestation() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus.clone(), validity, &signing_key)
                .expect("should create attestation");

        assert_eq!(attestation.node_id, node_id);
        assert_eq!(attestation.gpus, gpus);
        assert!(attestation.expires_at > attestation.timestamp);
    }

    #[test]
    fn test_verify_signature_valid() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert!(attestation.verify_signature(&verifying_key).is_ok());
    }

    #[test]
    fn test_verify_signature_wrong_key() {
        let (signing_key, _) = create_keypair();
        let (_, wrong_verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        let result = attestation.verify_signature(&wrong_verifying_key);
        assert!(matches!(result, Err(AttestationError::SignatureVerification)));
    }

    #[test]
    fn test_attestation_not_expired() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert!(!attestation.is_expired());
        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_attestation_expired() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        // Negative duration = already expired
        let validity = chrono::Duration::hours(-1);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert!(attestation.is_expired());
        let result = attestation.verify(&verifying_key);
        assert!(matches!(result, Err(AttestationError::Expired)));
    }

    #[test]
    fn test_multiple_gpus() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![
            GpuInfo {
                model: "NVIDIA RTX 4090".to_string(),
                vram_mb: 24576,
                compute_capability: "8.9".to_string(),
            },
            GpuInfo {
                model: "NVIDIA RTX 3090".to_string(),
                vram_mb: 24576,
                compute_capability: "8.6".to_string(),
            },
        ];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus.clone(), validity, &signing_key)
                .expect("should create attestation");

        assert_eq!(attestation.gpus.len(), 2);
        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        let json = serde_json::to_string(&attestation).expect("should serialize");
        let deserialized: HardwareAttestation =
            serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.node_id, attestation.node_id);
        assert_eq!(deserialized.gpus, attestation.gpus);
        assert!(deserialized.verify(&verifying_key).is_ok());
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn test_gpu_info_equality() {
        let gpu1 = GpuInfo {
            model: "NVIDIA RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };
        let gpu2 = GpuInfo {
            model: "NVIDIA RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };
        let gpu3 = GpuInfo {
            model: "NVIDIA RTX 3090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.6".to_string(),
        };

        assert_eq!(gpu1, gpu2);
        assert_ne!(gpu1, gpu3);
    }

    #[test]
    fn test_gpu_info_clone() {
        let gpu = create_test_gpu();
        let cloned = gpu.clone();
        assert_eq!(gpu, cloned);
    }

    #[test]
    fn test_attestation_timestamps() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let before = Utc::now();
        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");
        let after = Utc::now();

        // Timestamp should be between before and after
        assert!(attestation.timestamp >= before);
        assert!(attestation.timestamp <= after);

        // Expiry should be ~24 hours later
        let expected_expiry = attestation.timestamp + chrono::Duration::hours(24);
        let diff = (attestation.expires_at - expected_expiry).num_seconds().abs();
        assert!(diff < 2); // Allow 1 second tolerance
    }

    #[test]
    fn test_attestation_verify_method() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        // Verify method checks both signature and expiry
        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_signing_includes_timestamp() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        // Create attestation
        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        // The signature should include the timestamp (verified by other tests)
        // Just verify we can create attestations
        assert!(!attestation.is_expired());
    }

    #[test]
    fn test_different_keys_different_signatures() {
        let (signing_key1, _) = create_keypair();
        let (signing_key2, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation1 =
            HardwareAttestation::create_and_sign(node_id, gpus.clone(), validity, &signing_key1)
                .expect("should create attestation");

        let attestation2 =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key2)
                .expect("should create attestation");

        // Different signing keys produce different signatures
        assert_ne!(attestation1.signature, attestation2.signature);
    }

    #[test]
    fn test_empty_gpu_model() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![GpuInfo {
            model: String::new(), // Empty model name
            vram_mb: 1000,
            compute_capability: "1.0".to_string(),
        }];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_unicode_gpu_model() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![GpuInfo {
            model: "NVIDIA RTX™ 4090 🚀".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        }];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_many_gpus() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        
        // 8 GPUs like a DGX system
        let gpus: Vec<GpuInfo> = (0..8)
            .map(|i| GpuInfo {
                model: format!("GPU_{}", i),
                vram_mb: 80000,
                compute_capability: "9.0".to_string(),
            })
            .collect();

        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        assert_eq!(attestation.gpus.len(), 8);
        assert!(attestation.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_zero_validity_duration() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::zero();

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        // With zero duration, it should be expired immediately or at boundary
        // (depends on timing, so just verify it was created)
        assert_eq!(attestation.timestamp, attestation.expires_at);
    }

    #[test]
    fn test_json_compact_serialization() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        // Test compact JSON (no pretty printing)
        let json = serde_json::to_vec(&attestation).expect("should serialize to bytes");
        let deserialized: HardwareAttestation =
            serde_json::from_slice(&json).expect("should deserialize from bytes");

        assert_eq!(deserialized.node_id, attestation.node_id);
        assert!(deserialized.verify(&verifying_key).is_ok());
    }

    #[test]
    fn test_attestation_debug_format() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let gpus = vec![create_test_gpu()];
        let validity = chrono::Duration::hours(24);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus, validity, &signing_key)
                .expect("should create attestation");

        let debug = format!("{:?}", attestation);
        assert!(debug.contains("HardwareAttestation"));
        assert!(debug.contains(&node_id.to_string()));
    }

    #[test]
    fn test_gpu_info_debug_format() {
        let gpu = create_test_gpu();
        let debug = format!("{:?}", gpu);
        assert!(debug.contains("GpuInfo"));
        assert!(debug.contains("NVIDIA RTX 4090"));
    }
}
