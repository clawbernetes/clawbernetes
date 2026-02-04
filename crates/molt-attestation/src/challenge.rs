//! Challenge-response mechanism for attestation replay protection.
//!
//! This module provides protection against replay attacks by requiring attestations
//! to include a challenge from the verifier. The challenge contains:
//! - A random nonce (prevents replay)
//! - A timestamp (ensures freshness)
//! - A verifier ID (binds the attestation to a specific verifier)
//!
//! # Security Model
//!
//! Without challenge-response, an attacker could:
//! 1. Capture a valid attestation from node A to verifier V1
//! 2. Replay that attestation to verifier V2, spoofing node A's identity
//!
//! With challenge-response:
//! 1. Verifier issues a challenge with unique nonce and its ID
//! 2. Node signs the attestation including the challenge
//! 3. Verifier checks: nonce unused, challenge fresh, verifier ID matches
//!
//! # Example
//!
//! ```rust,ignore
//! use molt_attestation::challenge::{AttestationChallenge, ChallengeConfig, NonceCache};
//!
//! // Verifier creates a challenge
//! let challenge = AttestationChallenge::new("verifier-001");
//!
//! // Node creates attestation with challenge
//! let attestation = ChallengedHardwareAttestation::create_and_sign(
//!     node_id, gpus, validity, &signing_key, challenge,
//! )?;
//!
//! // Verifier validates with nonce cache
//! let config = ChallengeConfig::default();
//! let mut cache = NonceCache::new(config.nonce_cache_size);
//! attestation.verify_with_challenge(&verifying_key, "verifier-001", &config, &mut cache)?;
//! ```

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AttestationError;

/// A challenge issued by a verifier for attestation.
///
/// The challenge must be included in the signed attestation data to prevent
/// replay attacks across different verifiers or time periods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationChallenge {
    /// Random nonce to prevent replay (32 bytes, hex-encoded for readability).
    pub nonce: [u8; 32],
    /// When this challenge was issued.
    pub issued_at: DateTime<Utc>,
    /// The verifier that issued this challenge.
    pub verifier_id: String,
}

impl AttestationChallenge {
    /// Create a new challenge for attestation.
    ///
    /// Generates a cryptographically random nonce and records the current timestamp.
    #[must_use]
    pub fn new(verifier_id: impl Into<String>) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);

        Self {
            nonce,
            issued_at: Utc::now(),
            verifier_id: verifier_id.into(),
        }
    }

    /// Create a challenge with a specific nonce (for testing).
    #[cfg(test)]
    pub fn with_nonce(verifier_id: impl Into<String>, nonce: [u8; 32]) -> Self {
        Self {
            nonce,
            issued_at: Utc::now(),
            verifier_id: verifier_id.into(),
        }
    }

    /// Create a challenge with a specific timestamp (for testing).
    #[cfg(test)]
    pub fn with_timestamp(
        verifier_id: impl Into<String>,
        nonce: [u8; 32],
        issued_at: DateTime<Utc>,
    ) -> Self {
        Self {
            nonce,
            issued_at,
            verifier_id: verifier_id.into(),
        }
    }

    /// Get the age of this challenge in seconds.
    #[must_use]
    pub fn age_secs(&self) -> u64 {
        let now = Utc::now();
        let duration = now.signed_duration_since(self.issued_at);
        // If the challenge is from the future, treat it as age 0
        if duration.num_seconds() < 0 {
            0
        } else {
            duration.num_seconds() as u64
        }
    }

    /// Check if this challenge has expired according to the config.
    #[must_use]
    pub fn is_expired(&self, config: &ChallengeConfig) -> bool {
        self.age_secs() > config.max_age_secs
    }

    /// Get the nonce as a hex string for logging/display.
    #[must_use]
    pub fn nonce_hex(&self) -> String {
        self.nonce
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write;
                let _ = write!(acc, "{byte:02x}");
                acc
            })
    }

    /// Serialize the challenge for inclusion in signed message.
    #[must_use]
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 8 + self.verifier_id.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.issued_at.timestamp().to_le_bytes());
        bytes.extend_from_slice(self.verifier_id.as_bytes());
        bytes
    }
}

/// Configuration for challenge verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeConfig {
    /// Maximum age of a challenge in seconds (default: 300 = 5 minutes).
    pub max_age_secs: u64,
    /// Maximum number of nonces to cache for replay detection (default: 10000).
    pub nonce_cache_size: usize,
}

impl Default for ChallengeConfig {
    fn default() -> Self {
        Self {
            max_age_secs: 300,        // 5 minutes
            nonce_cache_size: 10_000, // 10k nonces
        }
    }
}

impl ChallengeConfig {
    /// Create a new config with custom values.
    #[must_use]
    pub fn new(max_age_secs: u64, nonce_cache_size: usize) -> Self {
        Self {
            max_age_secs,
            nonce_cache_size,
        }
    }

    /// Create a config for testing with short timeouts.
    #[cfg(test)]
    pub fn for_testing() -> Self {
        Self {
            max_age_secs: 5,        // 5 seconds
            nonce_cache_size: 100,  // Small cache
        }
    }
}

/// Cache of used nonces for replay detection.
///
/// Uses a bounded set to prevent memory exhaustion. When the cache is full,
/// old entries are evicted (using a simple FIFO-like approach via Vec).
#[derive(Debug)]
pub struct NonceCache {
    /// Set of used nonces for O(1) lookup.
    nonces: HashSet<[u8; 32]>,
    /// Order of insertion for eviction (FIFO).
    order: Vec<[u8; 32]>,
    /// Maximum cache size.
    max_size: usize,
}

impl NonceCache {
    /// Create a new nonce cache with the given maximum size.
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self {
            nonces: HashSet::with_capacity(max_size),
            order: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Check if a nonce has been used.
    #[must_use]
    pub fn contains(&self, nonce: &[u8; 32]) -> bool {
        self.nonces.contains(nonce)
    }

    /// Mark a nonce as used, evicting old entries if needed.
    ///
    /// Returns `true` if the nonce was newly added, `false` if it was already present.
    pub fn insert(&mut self, nonce: [u8; 32]) -> bool {
        if self.nonces.contains(&nonce) {
            return false;
        }

        // Evict oldest if at capacity
        while self.order.len() >= self.max_size {
            if let Some(oldest) = self.order.first().copied() {
                self.order.remove(0);
                self.nonces.remove(&oldest);
            }
        }

        self.nonces.insert(nonce);
        self.order.push(nonce);
        true
    }

    /// Get the current number of cached nonces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nonces.len()
    }

    /// Check if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nonces.is_empty()
    }

    /// Clear all cached nonces.
    pub fn clear(&mut self) {
        self.nonces.clear();
        self.order.clear();
    }
}

/// Verify a challenge for use with an attestation.
///
/// # Errors
///
/// Returns `AttestationError::ChallengeExpired` if the challenge is too old.
/// Returns `AttestationError::NonceReplay` if the nonce has been used before.
/// Returns `AttestationError::VerifierMismatch` if the verifier ID doesn't match.
pub fn verify_challenge(
    challenge: &AttestationChallenge,
    expected_verifier_id: &str,
    config: &ChallengeConfig,
    nonce_cache: &mut NonceCache,
) -> Result<(), AttestationError> {
    // Check verifier ID first (cheapest check)
    if challenge.verifier_id != expected_verifier_id {
        return Err(AttestationError::VerifierMismatch {
            expected: expected_verifier_id.to_string(),
            actual: challenge.verifier_id.clone(),
        });
    }

    // Check freshness
    let age = challenge.age_secs();
    if age > config.max_age_secs {
        return Err(AttestationError::ChallengeExpired {
            age_secs: age,
            max_age_secs: config.max_age_secs,
        });
    }

    // Check for replay (nonce already used)
    if nonce_cache.contains(&challenge.nonce) {
        return Err(AttestationError::NonceReplay);
    }

    // Mark nonce as used
    nonce_cache.insert(challenge.nonce);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // TDD Tests - Challenge-Response Mechanism
    // =========================================================================

    #[test]
    fn test_challenge_creation() {
        let challenge = AttestationChallenge::new("verifier-001");

        assert_eq!(challenge.verifier_id, "verifier-001");
        assert!(challenge.age_secs() < 2); // Should be very recent
        assert_eq!(challenge.nonce.len(), 32);
    }

    #[test]
    fn test_challenge_nonces_are_unique() {
        let c1 = AttestationChallenge::new("v1");
        let c2 = AttestationChallenge::new("v1");

        // Two challenges should have different nonces
        assert_ne!(c1.nonce, c2.nonce);
    }

    #[test]
    fn test_challenge_signing_bytes_include_all_fields() {
        let nonce = [42u8; 32];
        let challenge = AttestationChallenge::with_nonce("verifier", nonce);

        let bytes = challenge.to_signing_bytes();

        // Should contain nonce (32) + timestamp (8) + verifier_id
        assert!(bytes.len() >= 32 + 8);
        assert!(bytes.starts_with(&nonce));
    }

    #[test]
    fn test_challenge_config_default() {
        let config = ChallengeConfig::default();

        assert_eq!(config.max_age_secs, 300); // 5 minutes
        assert_eq!(config.nonce_cache_size, 10_000);
    }

    #[test]
    fn test_nonce_cache_insert_and_contains() {
        let mut cache = NonceCache::new(100);
        let nonce = [1u8; 32];

        assert!(!cache.contains(&nonce));
        assert!(cache.insert(nonce)); // First insert returns true
        assert!(cache.contains(&nonce));
        assert!(!cache.insert(nonce)); // Second insert returns false (duplicate)
    }

    #[test]
    fn test_nonce_cache_eviction() {
        let mut cache = NonceCache::new(3);

        let n1 = [1u8; 32];
        let n2 = [2u8; 32];
        let n3 = [3u8; 32];
        let n4 = [4u8; 32];

        cache.insert(n1);
        cache.insert(n2);
        cache.insert(n3);

        assert_eq!(cache.len(), 3);
        assert!(cache.contains(&n1));

        // Insert n4, should evict n1 (oldest)
        cache.insert(n4);

        assert_eq!(cache.len(), 3);
        assert!(!cache.contains(&n1)); // Evicted
        assert!(cache.contains(&n2));
        assert!(cache.contains(&n3));
        assert!(cache.contains(&n4));
    }

    #[test]
    fn test_verify_challenge_fresh_valid() {
        let challenge = AttestationChallenge::new("verifier-001");
        let config = ChallengeConfig::default();
        let mut cache = NonceCache::new(100);

        let result = verify_challenge(&challenge, "verifier-001", &config, &mut cache);

        assert!(result.is_ok());
        // Nonce should now be in cache
        assert!(cache.contains(&challenge.nonce));
    }

    #[test]
    fn test_verify_challenge_replay_fails() {
        let nonce = [99u8; 32];
        let challenge = AttestationChallenge::with_nonce("verifier-001", nonce);
        let config = ChallengeConfig::default();
        let mut cache = NonceCache::new(100);

        // First verification succeeds
        let result1 = verify_challenge(&challenge, "verifier-001", &config, &mut cache);
        assert!(result1.is_ok());

        // Second verification with same nonce fails (replay)
        let challenge2 = AttestationChallenge::with_nonce("verifier-001", nonce);
        let result2 = verify_challenge(&challenge2, "verifier-001", &config, &mut cache);

        assert!(matches!(result2, Err(AttestationError::NonceReplay)));
    }

    #[test]
    fn test_verify_challenge_expired_fails() {
        let nonce = [1u8; 32];
        // Create challenge from 10 minutes ago
        let old_time = Utc::now() - chrono::Duration::minutes(10);
        let challenge = AttestationChallenge::with_timestamp("verifier-001", nonce, old_time);

        let config = ChallengeConfig::default(); // max_age = 5 minutes
        let mut cache = NonceCache::new(100);

        let result = verify_challenge(&challenge, "verifier-001", &config, &mut cache);

        match result {
            Err(AttestationError::ChallengeExpired { age_secs, max_age_secs }) => {
                assert!(age_secs >= 600); // At least 10 minutes
                assert_eq!(max_age_secs, 300);
            }
            _ => panic!("expected ChallengeExpired error"),
        }
    }

    #[test]
    fn test_verify_challenge_wrong_verifier_fails() {
        let challenge = AttestationChallenge::new("verifier-001");
        let config = ChallengeConfig::default();
        let mut cache = NonceCache::new(100);

        // Try to verify with wrong verifier ID
        let result = verify_challenge(&challenge, "verifier-002", &config, &mut cache);

        match result {
            Err(AttestationError::VerifierMismatch { expected, actual }) => {
                assert_eq!(expected, "verifier-002");
                assert_eq!(actual, "verifier-001");
            }
            _ => panic!("expected VerifierMismatch error"),
        }

        // Nonce should NOT be in cache (verification failed early)
        assert!(!cache.contains(&challenge.nonce));
    }

    #[test]
    fn test_challenge_hex_nonce() {
        let nonce = [0xAB; 32];
        let challenge = AttestationChallenge::with_nonce("v", nonce);

        let hex = challenge.nonce_hex();

        assert_eq!(hex.len(), 64); // 32 bytes = 64 hex chars
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_nonce_cache_clear() {
        let mut cache = NonceCache::new(100);
        cache.insert([1u8; 32]);
        cache.insert([2u8; 32]);

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_challenge_serialization_roundtrip() {
        let challenge = AttestationChallenge::new("test-verifier");

        let json = serde_json::to_string(&challenge).expect("serialize");
        let deserialized: AttestationChallenge = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(challenge.nonce, deserialized.nonce);
        assert_eq!(challenge.verifier_id, deserialized.verifier_id);
        // Timestamps may have slight differences due to serialization precision
    }

    #[test]
    fn test_challenge_age_future_timestamp() {
        let nonce = [1u8; 32];
        let future_time = Utc::now() + chrono::Duration::hours(1);
        let challenge = AttestationChallenge::with_timestamp("v", nonce, future_time);

        // Future challenge should have age 0 (not negative)
        assert_eq!(challenge.age_secs(), 0);
    }

    #[test]
    fn test_verify_order_verifier_before_freshness() {
        // If verifier ID is wrong, we should fail before checking cache
        // This ensures we don't pollute the cache with invalid requests
        let nonce = [1u8; 32];
        let old_time = Utc::now() - chrono::Duration::hours(1);
        let challenge = AttestationChallenge::with_timestamp("wrong-verifier", nonce, old_time);

        let config = ChallengeConfig::default();
        let mut cache = NonceCache::new(100);

        let result = verify_challenge(&challenge, "correct-verifier", &config, &mut cache);

        // Should fail with VerifierMismatch, not ChallengeExpired
        assert!(matches!(result, Err(AttestationError::VerifierMismatch { .. })));
        // Cache should not be modified
        assert!(!cache.contains(&nonce));
    }
}
