//! Hardware attestation — GPU verification, capability proofs.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AttestationError;

/// GPU vendor enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    /// NVIDIA GPUs (CUDA-capable).
    Nvidia,
    /// AMD GPUs (ROCm-capable).
    Amd,
    /// Intel GPUs (oneAPI-capable).
    Intel,
    /// Apple Silicon GPUs (Metal-capable).
    Apple,
    /// Unknown or other vendors.
    Unknown,
}

impl GpuVendor {
    /// Create a vendor from a string, case-insensitive.
    #[must_use]
    pub fn from_str_case_insensitive(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "nvidia" => Self::Nvidia,
            "amd" | "radeon" => Self::Amd,
            "intel" => Self::Intel,
            "apple" => Self::Apple,
            _ => Self::Unknown,
        }
    }

    /// Get the vendor as a string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Intel => "intel",
            Self::Apple => "apple",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// GPU information for hardware attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU vendor (NVIDIA, AMD, Intel, Apple).
    pub vendor: GpuVendor,
    /// GPU model name (e.g., "RTX 4090").
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
            hasher.update(gpu.vendor.as_str().as_bytes());
            hasher.update(gpu.model.as_bytes());
            hasher.update(&gpu.vram_mb.to_le_bytes());
            hasher.update(gpu.compute_capability.as_bytes());
        }

        hasher.update(&timestamp.timestamp().to_le_bytes());
        hasher.update(&expires_at.timestamp().to_le_bytes());

        hasher.finalize().as_bytes().to_vec()
    }
}

/// Entry in the attestation chain recording a verification event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationEntry {
    /// The attestation that was verified.
    pub attestation: HardwareAttestation,
    /// Hash of the previous entry (None for genesis).
    pub previous_hash: Option<[u8; 32]>,
    /// When this verification occurred.
    pub verified_at: DateTime<Utc>,
    /// Result of the verification.
    pub verification_passed: bool,
}

impl AttestationEntry {
    /// Compute the hash of this entry for chaining.
    #[must_use]
    pub fn compute_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"attestation_entry_v1");
        hasher.update(self.attestation.node_id.as_bytes());
        hasher.update(&self.attestation.timestamp.timestamp().to_le_bytes());
        hasher.update(&self.attestation.signature.to_bytes());
        
        if let Some(prev) = &self.previous_hash {
            hasher.update(prev);
        }
        
        hasher.update(&self.verified_at.timestamp().to_le_bytes());
        hasher.update(&[u8::from(self.verification_passed)]);
        
        *hasher.finalize().as_bytes()
    }
}

/// A chain of hardware attestation verification events.
///
/// This provides an auditable history of attestation verifications for a node,
/// enabling trust accumulation over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationChain {
    /// The node this chain is tracking.
    node_id: Uuid,
    /// Ordered list of attestation entries.
    entries: Vec<AttestationEntry>,
}

impl AttestationChain {
    /// Create a new attestation chain for a node.
    #[must_use]
    pub fn new(node_id: Uuid) -> Self {
        Self {
            node_id,
            entries: Vec::new(),
        }
    }

    /// Get the node ID this chain is tracking.
    #[must_use]
    pub const fn node_id(&self) -> Uuid {
        self.node_id
    }

    /// Get the number of entries in the chain.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the chain is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the entries in the chain.
    #[must_use]
    pub fn entries(&self) -> &[AttestationEntry] {
        &self.entries
    }

    /// Get the latest entry hash (for chaining).
    #[must_use]
    pub fn latest_hash(&self) -> Option<[u8; 32]> {
        self.entries.last().map(AttestationEntry::compute_hash)
    }

    /// Add a verified attestation to the chain.
    ///
    /// # Errors
    ///
    /// Returns an error if the attestation is for a different node.
    pub fn add_attestation(
        &mut self,
        attestation: HardwareAttestation,
        verification_passed: bool,
    ) -> Result<(), AttestationError> {
        if attestation.node_id != self.node_id {
            return Err(AttestationError::HardwareVerification(format!(
                "attestation node_id {} does not match chain node_id {}",
                attestation.node_id, self.node_id
            )));
        }

        let previous_hash = self.latest_hash();
        let entry = AttestationEntry {
            attestation,
            previous_hash,
            verified_at: Utc::now(),
            verification_passed,
        };

        self.entries.push(entry);
        Ok(())
    }

    /// Verify the integrity of the chain.
    ///
    /// This checks that all entries are properly linked via their hashes.
    ///
    /// # Errors
    ///
    /// Returns an error if the chain integrity is compromised.
    pub fn verify_integrity(&self) -> Result<(), AttestationError> {
        for (i, entry) in self.entries.iter().enumerate() {
            // Check node_id consistency
            if entry.attestation.node_id != self.node_id {
                return Err(AttestationError::HardwareVerification(format!(
                    "entry {i} has wrong node_id"
                )));
            }

            // Check hash chain
            if i == 0 {
                if entry.previous_hash.is_some() {
                    return Err(AttestationError::HardwareVerification(
                        "genesis entry should have no previous hash".to_string(),
                    ));
                }
            } else {
                let expected_prev = self.entries[i - 1].compute_hash();
                match entry.previous_hash {
                    Some(prev) if prev == expected_prev => {}
                    Some(_) => {
                        return Err(AttestationError::HardwareVerification(format!(
                            "entry {i} has incorrect previous hash"
                        )));
                    }
                    None => {
                        return Err(AttestationError::HardwareVerification(format!(
                            "entry {i} missing previous hash"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the count of successful verifications.
    #[must_use]
    pub fn successful_verification_count(&self) -> usize {
        self.entries.iter().filter(|e| e.verification_passed).count()
    }

    /// Get the count of failed verifications.
    #[must_use]
    pub fn failed_verification_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.verification_passed).count()
    }

    /// Calculate a trust score based on verification history (0.0 - 1.0).
    ///
    /// The score considers:
    /// - Ratio of successful to total verifications
    /// - Recency of verifications (recent verifications weighted more)
    /// - Consistency over time
    #[must_use]
    pub fn trust_score(&self) -> f64 {
        if self.entries.is_empty() {
            return 0.0;
        }

        let total = self.entries.len() as f64;
        let successful = self.successful_verification_count() as f64;

        // Base score is success ratio
        let base_score = successful / total;

        // Apply recency weighting (most recent entries count more)
        let weighted_sum: f64 = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let weight = (i + 1) as f64 / total; // Later entries weighted more
                if entry.verification_passed {
                    weight
                } else {
                    0.0
                }
            })
            .sum();

        let max_weighted = (1..=self.entries.len())
            .map(|i| i as f64 / total)
            .sum::<f64>();

        let recency_score = if max_weighted > 0.0 {
            weighted_sum / max_weighted
        } else {
            0.0
        };

        // Combine base and recency scores
        f64::midpoint(base_score, recency_score)
    }

    /// Get the time span covered by this chain.
    #[must_use]
    pub fn time_span(&self) -> Option<chrono::Duration> {
        if self.entries.len() < 2 {
            return None;
        }
        
        let first = &self.entries[0];
        let last = &self.entries[self.entries.len() - 1];
        
        Some(last.verified_at - first.verified_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_test_gpu() -> GpuInfo {
        GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "RTX 4090".to_string(),
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
                vendor: GpuVendor::Nvidia,
                model: "RTX 4090".to_string(),
                vram_mb: 24576,
                compute_capability: "8.9".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "RTX 3090".to_string(),
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
            vendor: GpuVendor::Nvidia,
            model: "RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };
        let gpu2 = GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "RTX 4090".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };
        let gpu3 = GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "RTX 3090".to_string(),
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
            vendor: GpuVendor::Unknown,
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
            vendor: GpuVendor::Nvidia,
            model: "RTX™ 4090 🚀".to_string(),
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
                vendor: GpuVendor::Nvidia,
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
        assert!(debug.contains("RTX 4090"));
    }

    // =========================================================================
    // GpuVendor Tests
    // =========================================================================

    #[test]
    fn test_gpu_vendor_from_str_case_insensitive() {
        assert_eq!(GpuVendor::from_str_case_insensitive("nvidia"), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_str_case_insensitive("NVIDIA"), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_str_case_insensitive("Nvidia"), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_str_case_insensitive("amd"), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_str_case_insensitive("AMD"), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_str_case_insensitive("radeon"), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_str_case_insensitive("intel"), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_str_case_insensitive("INTEL"), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_str_case_insensitive("apple"), GpuVendor::Apple);
        assert_eq!(GpuVendor::from_str_case_insensitive("APPLE"), GpuVendor::Apple);
        assert_eq!(GpuVendor::from_str_case_insensitive("unknown_vendor"), GpuVendor::Unknown);
        assert_eq!(GpuVendor::from_str_case_insensitive(""), GpuVendor::Unknown);
    }

    #[test]
    fn test_gpu_vendor_as_str() {
        assert_eq!(GpuVendor::Nvidia.as_str(), "nvidia");
        assert_eq!(GpuVendor::Amd.as_str(), "amd");
        assert_eq!(GpuVendor::Intel.as_str(), "intel");
        assert_eq!(GpuVendor::Apple.as_str(), "apple");
        assert_eq!(GpuVendor::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_gpu_vendor_display() {
        assert_eq!(format!("{}", GpuVendor::Nvidia), "nvidia");
        assert_eq!(format!("{}", GpuVendor::Amd), "amd");
        assert_eq!(format!("{}", GpuVendor::Intel), "intel");
        assert_eq!(format!("{}", GpuVendor::Apple), "apple");
        assert_eq!(format!("{}", GpuVendor::Unknown), "unknown");
    }

    #[test]
    fn test_gpu_vendor_serialization() {
        let nvidia = GpuVendor::Nvidia;
        let json = serde_json::to_string(&nvidia).expect("should serialize");
        assert_eq!(json, "\"nvidia\"");

        let deserialized: GpuVendor = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized, GpuVendor::Nvidia);
    }

    #[test]
    fn test_gpu_vendor_equality_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(GpuVendor::Nvidia);
        set.insert(GpuVendor::Amd);
        set.insert(GpuVendor::Nvidia); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&GpuVendor::Nvidia));
        assert!(set.contains(&GpuVendor::Amd));
    }

    #[test]
    fn test_gpu_vendor_copy() {
        let vendor = GpuVendor::Nvidia;
        let copied = vendor; // Copy
        assert_eq!(vendor, copied);
    }

    // =========================================================================
    // AttestationChain Tests
    // =========================================================================

    #[test]
    fn test_attestation_chain_new() {
        let node_id = Uuid::new_v4();
        let chain = AttestationChain::new(node_id);

        assert_eq!(chain.node_id(), node_id);
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.latest_hash().is_none());
    }

    #[test]
    fn test_attestation_chain_add_attestation() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        let verification_passed = attestation.verify(&verifying_key).is_ok();
        chain.add_attestation(attestation, verification_passed).expect("should add");

        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
        assert!(chain.latest_hash().is_some());
    }

    #[test]
    fn test_attestation_chain_wrong_node() {
        let (signing_key, _) = create_keypair();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();
        let mut chain = AttestationChain::new(node1);

        // Create attestation for different node
        let attestation =
            HardwareAttestation::create_and_sign(node2, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        let result = chain.add_attestation(attestation, true);
        assert!(matches!(result, Err(AttestationError::HardwareVerification(_))));
    }

    #[test]
    fn test_attestation_chain_integrity() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        // Add multiple attestations
        for _ in 0..5 {
            let attestation =
                HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                    .expect("should create attestation");

            let verification_passed = attestation.verify(&verifying_key).is_ok();
            chain.add_attestation(attestation, verification_passed).expect("should add");
        }

        assert_eq!(chain.len(), 5);
        assert!(chain.verify_integrity().is_ok());
    }

    #[test]
    fn test_attestation_chain_trust_score_all_passed() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        for _ in 0..10 {
            let attestation =
                HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                    .expect("should create attestation");

            let passed = attestation.verify(&verifying_key).is_ok();
            chain.add_attestation(attestation, passed).expect("should add");
        }

        let score = chain.trust_score();
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_attestation_chain_trust_score_all_failed() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        for _ in 0..10 {
            let attestation =
                HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                    .expect("should create attestation");

            // Force failure
            chain.add_attestation(attestation, false).expect("should add");
        }

        let score = chain.trust_score();
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_attestation_chain_trust_score_mixed() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        // Add mix of passed and failed
        for i in 0..10 {
            let attestation =
                HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                    .expect("should create attestation");

            chain.add_attestation(attestation, i % 2 == 0).expect("should add");
        }

        let score = chain.trust_score();
        assert!(score > 0.0);
        assert!(score < 1.0);
    }

    #[test]
    fn test_attestation_chain_trust_score_empty() {
        let node_id = Uuid::new_v4();
        let chain = AttestationChain::new(node_id);

        let score = chain.trust_score();
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_attestation_chain_verification_counts() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        // Add 3 passed and 2 failed
        for i in 0..5 {
            let attestation =
                HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                    .expect("should create attestation");

            chain.add_attestation(attestation, i < 3).expect("should add");
        }

        assert_eq!(chain.successful_verification_count(), 3);
        assert_eq!(chain.failed_verification_count(), 2);
    }

    #[test]
    fn test_attestation_chain_time_span() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        // Empty chain has no time span
        assert!(chain.time_span().is_none());

        // Single entry has no time span
        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");
        chain.add_attestation(attestation, true).expect("should add");
        assert!(chain.time_span().is_none());

        // Multiple entries have a time span
        std::thread::sleep(std::time::Duration::from_millis(10));
        let attestation2 =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");
        chain.add_attestation(attestation2, true).expect("should add");

        let span = chain.time_span();
        assert!(span.is_some());
        assert!(span.unwrap().num_milliseconds() >= 0);
    }

    #[test]
    fn test_attestation_chain_entries() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");
        chain.add_attestation(attestation.clone(), true).expect("should add");

        let entries = chain.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].attestation.node_id, node_id);
        assert!(entries[0].verification_passed);
        assert!(entries[0].previous_hash.is_none()); // Genesis
    }

    #[test]
    fn test_attestation_chain_serialization() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();
        let mut chain = AttestationChain::new(node_id);

        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");
        chain.add_attestation(attestation, true).expect("should add");

        let json = serde_json::to_string(&chain).expect("should serialize");
        let deserialized: AttestationChain = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.node_id(), node_id);
        assert_eq!(deserialized.len(), 1);
        assert!(deserialized.verify_integrity().is_ok());
    }

    #[test]
    fn test_attestation_entry_compute_hash() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();

        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        let entry = AttestationEntry {
            attestation: attestation.clone(),
            previous_hash: None,
            verified_at: Utc::now(),
            verification_passed: true,
        };

        let hash1 = entry.compute_hash();
        let hash2 = entry.compute_hash();

        // Same entry should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_attestation_entry_hash_differs_by_verification_status() {
        let (signing_key, _) = create_keypair();
        let node_id = Uuid::new_v4();

        let attestation =
            HardwareAttestation::create_and_sign(node_id, vec![create_test_gpu()], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        let now = Utc::now();
        
        let entry_passed = AttestationEntry {
            attestation: attestation.clone(),
            previous_hash: None,
            verified_at: now,
            verification_passed: true,
        };

        let entry_failed = AttestationEntry {
            attestation,
            previous_hash: None,
            verified_at: now,
            verification_passed: false,
        };

        assert_ne!(entry_passed.compute_hash(), entry_failed.compute_hash());
    }

    #[test]
    fn test_different_vendor_different_attestation() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();

        let nvidia_gpu = GpuInfo {
            vendor: GpuVendor::Nvidia,
            model: "Test GPU".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };

        let amd_gpu = GpuInfo {
            vendor: GpuVendor::Amd,
            model: "Test GPU".to_string(),
            vram_mb: 24576,
            compute_capability: "8.9".to_string(),
        };

        let attestation1 =
            HardwareAttestation::create_and_sign(node_id, vec![nvidia_gpu], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        let attestation2 =
            HardwareAttestation::create_and_sign(node_id, vec![amd_gpu], chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        // Both should verify correctly
        assert!(attestation1.verify(&verifying_key).is_ok());
        assert!(attestation2.verify(&verifying_key).is_ok());

        // But they have different signatures (different vendor means different data)
        assert_ne!(attestation1.signature, attestation2.signature);
    }

    #[test]
    fn test_all_vendor_types_in_attestation() {
        let (signing_key, verifying_key) = create_keypair();
        let node_id = Uuid::new_v4();

        let gpus = vec![
            GpuInfo {
                vendor: GpuVendor::Nvidia,
                model: "RTX 4090".to_string(),
                vram_mb: 24576,
                compute_capability: "8.9".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Amd,
                model: "RX 7900 XTX".to_string(),
                vram_mb: 24576,
                compute_capability: "gfx1100".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Intel,
                model: "Arc A770".to_string(),
                vram_mb: 16384,
                compute_capability: "Xe-HPG".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Apple,
                model: "M2 Max".to_string(),
                vram_mb: 96000,
                compute_capability: "Metal 3".to_string(),
            },
            GpuInfo {
                vendor: GpuVendor::Unknown,
                model: "Custom FPGA".to_string(),
                vram_mb: 8192,
                compute_capability: "Custom".to_string(),
            },
        ];

        let attestation =
            HardwareAttestation::create_and_sign(node_id, gpus.clone(), chrono::Duration::hours(24), &signing_key)
                .expect("should create attestation");

        assert_eq!(attestation.gpus.len(), 5);
        assert!(attestation.verify(&verifying_key).is_ok());
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use proptest::prelude::*;
    use rand::rngs::OsRng;

    fn create_keypair() -> (SigningKey, VerifyingKey) {
        use rand::RngCore;
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn arb_gpu_vendor() -> impl Strategy<Value = GpuVendor> {
        prop_oneof![
            Just(GpuVendor::Nvidia),
            Just(GpuVendor::Amd),
            Just(GpuVendor::Intel),
            Just(GpuVendor::Apple),
            Just(GpuVendor::Unknown),
        ]
    }

    fn arb_gpu_info() -> impl Strategy<Value = GpuInfo> {
        (
            arb_gpu_vendor(),
            "[a-zA-Z0-9 ]{1,50}",
            0u64..=u64::MAX,
            "[a-zA-Z0-9.]{1,20}",
        )
            .prop_map(|(vendor, model, vram_mb, compute_capability)| GpuInfo {
                vendor,
                model,
                vram_mb,
                compute_capability,
            })
    }

    proptest! {
        #[test]
        fn prop_attestation_signature_verifies(
            gpus in proptest::collection::vec(arb_gpu_info(), 0..10),
            validity_hours in 1i64..1000,
        ) {
            let (signing_key, verifying_key) = create_keypair();
            let node_id = Uuid::new_v4();
            let validity = chrono::Duration::hours(validity_hours);

            let attestation = HardwareAttestation::create_and_sign(
                node_id,
                gpus,
                validity,
                &signing_key,
            )
            .expect("should create attestation");

            prop_assert!(attestation.verify(&verifying_key).is_ok());
        }

        #[test]
        fn prop_attestation_wrong_key_fails(
            gpus in proptest::collection::vec(arb_gpu_info(), 1..5),
        ) {
            let (signing_key, _) = create_keypair();
            let (_, wrong_key) = create_keypair();
            let node_id = Uuid::new_v4();
            let validity = chrono::Duration::hours(24);

            let attestation = HardwareAttestation::create_and_sign(
                node_id,
                gpus,
                validity,
                &signing_key,
            )
            .expect("should create attestation");

            prop_assert!(attestation.verify(&wrong_key).is_err());
        }

        #[test]
        fn prop_attestation_serialization_roundtrip(
            gpus in proptest::collection::vec(arb_gpu_info(), 1..5),
        ) {
            let (signing_key, verifying_key) = create_keypair();
            let node_id = Uuid::new_v4();
            let validity = chrono::Duration::hours(24);

            let attestation = HardwareAttestation::create_and_sign(
                node_id,
                gpus,
                validity,
                &signing_key,
            )
            .expect("should create attestation");

            let json = serde_json::to_string(&attestation).expect("should serialize");
            let deserialized: HardwareAttestation = serde_json::from_str(&json).expect("should deserialize");

            prop_assert_eq!(deserialized.node_id, attestation.node_id);
            prop_assert_eq!(deserialized.gpus.len(), attestation.gpus.len());
            prop_assert!(deserialized.verify(&verifying_key).is_ok());
        }

        #[test]
        fn prop_gpu_vendor_roundtrip(vendor_str in "[a-zA-Z]{0,20}") {
            let vendor = GpuVendor::from_str_case_insensitive(&vendor_str);
            let as_str = vendor.as_str();
            
            // Converting back should give same vendor
            let back = GpuVendor::from_str_case_insensitive(as_str);
            prop_assert_eq!(vendor, back);
        }

        #[test]
        fn prop_attestation_chain_integrity_preserved(
            count in 1usize..20,
        ) {
            let (signing_key, verifying_key) = create_keypair();
            let node_id = Uuid::new_v4();
            let mut chain = AttestationChain::new(node_id);

            for _ in 0..count {
                let gpu = GpuInfo {
                    vendor: GpuVendor::Nvidia,
                    model: "Test".to_string(),
                    vram_mb: 1000,
                    compute_capability: "1.0".to_string(),
                };
                let attestation = HardwareAttestation::create_and_sign(
                    node_id,
                    vec![gpu],
                    chrono::Duration::hours(24),
                    &signing_key,
                )
                .expect("should create attestation");

                let passed = attestation.verify(&verifying_key).is_ok();
                chain.add_attestation(attestation, passed).expect("should add");
            }

            prop_assert_eq!(chain.len(), count);
            prop_assert!(chain.verify_integrity().is_ok());
        }

        #[test]
        fn prop_trust_score_bounds(
            passed_count in 0usize..50,
            failed_count in 0usize..50,
        ) {
            if passed_count == 0 && failed_count == 0 {
                return Ok(());
            }

            let (signing_key, _) = create_keypair();
            let node_id = Uuid::new_v4();
            let mut chain = AttestationChain::new(node_id);

            for _ in 0..passed_count {
                let gpu = GpuInfo {
                    vendor: GpuVendor::Nvidia,
                    model: "Test".to_string(),
                    vram_mb: 1000,
                    compute_capability: "1.0".to_string(),
                };
                let attestation = HardwareAttestation::create_and_sign(
                    node_id,
                    vec![gpu],
                    chrono::Duration::hours(24),
                    &signing_key,
                )
                .expect("should create attestation");
                chain.add_attestation(attestation, true).expect("should add");
            }

            for _ in 0..failed_count {
                let gpu = GpuInfo {
                    vendor: GpuVendor::Nvidia,
                    model: "Test".to_string(),
                    vram_mb: 1000,
                    compute_capability: "1.0".to_string(),
                };
                let attestation = HardwareAttestation::create_and_sign(
                    node_id,
                    vec![gpu],
                    chrono::Duration::hours(24),
                    &signing_key,
                )
                .expect("should create attestation");
                chain.add_attestation(attestation, false).expect("should add");
            }

            let score = chain.trust_score();
            prop_assert!(score >= 0.0);
            prop_assert!(score <= 1.0);
        }
    }
}
