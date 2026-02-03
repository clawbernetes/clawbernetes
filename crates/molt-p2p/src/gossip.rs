//! Gossip protocol for capacity announcements.
//!
//! This module handles broadcasting and receiving capacity announcements
//! from peers in the network.

use crate::error::P2pError;
use crate::protocol::PeerId;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Information about a GPU available for compute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    /// GPU model name (e.g., "RTX 4090", "A100").
    pub model: String,
    /// Video RAM in gigabytes.
    pub vram_gb: u32,
    /// Number of GPUs of this type.
    pub count: u32,
}

/// Pricing information for compute resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pricing {
    /// Cost per GPU-hour in cents.
    pub gpu_hour_cents: u64,
    /// Cost per CPU-hour in cents.
    pub cpu_hour_cents: u64,
}

/// A signed announcement of a peer's available compute capacity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityAnnouncement {
    peer_id: PeerId,
    gpus: Vec<GpuInfo>,
    pricing: Pricing,
    job_types: Vec<String>,
    ttl: Duration,
    created_at: DateTime<Utc>,
    #[serde(with = "signature_serde")]
    signature: Option<Signature>,
}

/// Custom serde for `Option<Signature>` since Signature doesn't impl Serialize/Deserialize.
mod signature_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)]
    pub fn serialize<S>(sig: &Option<Signature>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match sig {
            Some(s) => serializer.serialize_some(&s.to_bytes().to_vec()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Signature>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<Vec<u8>> = Option::deserialize(deserializer)?;
        match opt {
            Some(bytes) => {
                let arr: [u8; 64] = bytes
                    .try_into()
                    .map_err(|_| serde::de::Error::custom("Invalid signature length"))?;
                Ok(Some(
                    Signature::from_bytes(&arr),
                ))
            }
            None => Ok(None),
        }
    }
}

impl CapacityAnnouncement {
    /// Creates a new unsigned capacity announcement.
    #[must_use]
    pub fn new(
        peer_id: PeerId,
        gpus: Vec<GpuInfo>,
        pricing: Pricing,
        job_types: Vec<String>,
        ttl: Duration,
    ) -> Self {
        Self {
            peer_id,
            gpus,
            pricing,
            job_types,
            ttl,
            created_at: Utc::now(),
            signature: None,
        }
    }

    /// Returns the peer ID of the announcer.
    #[must_use]
    pub const fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    /// Returns the available GPUs.
    #[must_use]
    pub fn gpus(&self) -> &[GpuInfo] {
        &self.gpus
    }

    /// Returns the pricing information.
    #[must_use]
    pub const fn pricing(&self) -> &Pricing {
        &self.pricing
    }

    /// Returns the supported job types.
    #[must_use]
    pub fn job_types(&self) -> &[String] {
        &self.job_types
    }

    /// Returns the TTL (time-to-live) duration.
    #[must_use]
    pub const fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Checks if this announcement has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or(Duration::MAX);
        elapsed > self.ttl
    }

    /// Signs this announcement with the given signing key.
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let message = self.signing_message();
        self.signature = Some(signing_key.sign(&message));
    }

    /// Verifies the announcement's signature against the given public key.
    ///
    /// # Errors
    ///
    /// Returns an error if the signature is missing or invalid.
    pub fn verify(&self, verifying_key: &VerifyingKey) -> Result<(), P2pError> {
        let signature = self
            .signature
            .ok_or_else(|| P2pError::Protocol("Missing signature".to_string()))?;

        let message = self.signing_message();
        verifying_key
            .verify(&message, &signature)
            .map_err(|e| P2pError::Protocol(format!("Invalid signature: {e}")))
    }

    /// Constructs the message to be signed (excludes the signature field).
    fn signing_message(&self) -> Vec<u8> {
        let mut msg = Vec::new();
        msg.extend_from_slice(self.peer_id.as_bytes());
        msg.extend_from_slice(&self.ttl.as_secs().to_le_bytes());
        msg.extend_from_slice(
            &self
                .created_at
                .timestamp()
                .to_le_bytes(),
        );
        // Include GPUs, pricing, job_types in the signature
        for gpu in &self.gpus {
            msg.extend_from_slice(gpu.model.as_bytes());
            msg.extend_from_slice(&gpu.vram_gb.to_le_bytes());
            msg.extend_from_slice(&gpu.count.to_le_bytes());
        }
        msg.extend_from_slice(&self.pricing.gpu_hour_cents.to_le_bytes());
        msg.extend_from_slice(&self.pricing.cpu_hour_cents.to_le_bytes());
        for jt in &self.job_types {
            msg.extend_from_slice(jt.as_bytes());
        }
        msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn make_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    #[test]
    fn capacity_announcement_creation() {
        let signing_key = make_signing_key();
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let announcement = CapacityAnnouncement::new(
            peer_id,
            vec![GpuInfo {
                model: "RTX 4090".to_string(),
                vram_gb: 24,
                count: 2,
            }],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec!["inference".to_string(), "training".to_string()],
            Duration::from_secs(300),
        );

        assert_eq!(announcement.peer_id(), peer_id);
        assert_eq!(announcement.gpus().len(), 1);
        assert_eq!(announcement.gpus()[0].model, "RTX 4090");
        assert_eq!(announcement.job_types().len(), 2);
    }

    #[test]
    fn capacity_announcement_sign_and_verify() {
        let signing_key = make_signing_key();
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let mut announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 50,
                cpu_hour_cents: 5,
            },
            vec![],
            Duration::from_secs(60),
        );

        // Sign the announcement
        announcement.sign(&signing_key);

        // Verification should succeed
        assert!(announcement.verify(&signing_key.verifying_key()).is_ok());
    }

    #[test]
    fn capacity_announcement_verify_fails_with_wrong_key() {
        let signing_key = make_signing_key();
        let wrong_key = make_signing_key();
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let mut announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 50,
                cpu_hour_cents: 5,
            },
            vec![],
            Duration::from_secs(60),
        );

        announcement.sign(&signing_key);

        // Verification with wrong key should fail
        assert!(announcement.verify(&wrong_key.verifying_key()).is_err());
    }

    #[test]
    fn capacity_announcement_expiry() {
        let signing_key = make_signing_key();
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        // Very short TTL
        let announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 50,
                cpu_hour_cents: 5,
            },
            vec![],
            Duration::from_millis(1),
        );

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(10));

        assert!(announcement.is_expired());
    }

    #[test]
    fn capacity_announcement_not_expired_within_ttl() {
        let signing_key = make_signing_key();
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());

        let announcement = CapacityAnnouncement::new(
            peer_id,
            vec![],
            Pricing {
                gpu_hour_cents: 50,
                cpu_hour_cents: 5,
            },
            vec![],
            Duration::from_secs(300),
        );

        assert!(!announcement.is_expired());
    }

    #[test]
    fn gpu_info_serialization() {
        let gpu = GpuInfo {
            model: "A100".to_string(),
            vram_gb: 80,
            count: 8,
        };

        let json = serde_json::to_string(&gpu).ok();
        assert!(json.is_some());

        let deserialized: GpuInfo = serde_json::from_str(&json.unwrap()).ok().unwrap();
        assert_eq!(deserialized.model, "A100");
        assert_eq!(deserialized.vram_gb, 80);
        assert_eq!(deserialized.count, 8);
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn gpu_info_serialization_roundtrip(
                model in "[A-Za-z0-9 ]+",
                vram_gb in 1u32..256,
                count in 1u32..64
            ) {
                let gpu = GpuInfo { model, vram_gb, count };
                let json = serde_json::to_string(&gpu).unwrap();
                let deserialized: GpuInfo = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(gpu, deserialized);
            }

            #[test]
            fn pricing_serialization_roundtrip(
                gpu_hour_cents in 0u64..1_000_000,
                cpu_hour_cents in 0u64..100_000
            ) {
                let pricing = Pricing { gpu_hour_cents, cpu_hour_cents };
                let json = serde_json::to_string(&pricing).unwrap();
                let deserialized: Pricing = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(pricing, deserialized);
            }

            #[test]
            fn capacity_announcement_expiry_is_consistent(
                ttl_ms in 1u64..10_000
            ) {
                let signing_key = SigningKey::generate(&mut OsRng);
                let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
                
                let announcement = CapacityAnnouncement::new(
                    peer_id,
                    vec![],
                    Pricing { gpu_hour_cents: 100, cpu_hour_cents: 10 },
                    vec![],
                    Duration::from_millis(ttl_ms),
                );
                
                // Immediately after creation, should not be expired (unless TTL is extremely short)
                if ttl_ms > 1 {
                    prop_assert!(!announcement.is_expired());
                }
            }
        }
    }
}
