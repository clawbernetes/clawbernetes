//! Certificate rotation policy and management.

use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::ca::CertificateAuthority;
use crate::error::{Error, Result};
use crate::store::CertStore;
use crate::types::{Certificate, CertificateId, CertificateRequest, KeyUsage, PrivateKey};

/// Policy for certificate rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Duration before expiry when rotation should occur.
    renew_before_expiry: Duration,
    /// Whether to automatically rotate certificates.
    auto_rotate: bool,
}

impl RotationPolicy {
    /// Creates a new rotation policy.
    ///
    /// # Arguments
    ///
    /// * `renew_before_expiry` - Duration before expiry when rotation should occur.
    /// * `auto_rotate` - Whether to automatically rotate certificates.
    #[must_use]
    pub const fn new(renew_before_expiry: Duration, auto_rotate: bool) -> Self {
        Self {
            renew_before_expiry,
            auto_rotate,
        }
    }

    /// Returns the duration before expiry when rotation should occur.
    #[must_use]
    pub const fn renew_before_expiry(&self) -> Duration {
        self.renew_before_expiry
    }

    /// Returns whether auto-rotation is enabled.
    #[must_use]
    pub const fn auto_rotate(&self) -> bool {
        self.auto_rotate
    }
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            // Renew 30 days before expiry by default
            renew_before_expiry: Duration::from_secs(30 * 24 * 60 * 60),
            auto_rotate: true,
        }
    }
}

/// Checks if a certificate needs rotation based on the policy.
///
/// # Arguments
///
/// * `cert` - The certificate to check.
/// * `policy` - The rotation policy.
///
/// # Returns
///
/// `true` if the certificate should be rotated.
#[must_use]
pub fn check_rotation_needed(cert: &Certificate, policy: &RotationPolicy) -> bool {
    let now = Utc::now();
    let renew_threshold = chrono::Duration::from_std(policy.renew_before_expiry)
        .unwrap_or(chrono::Duration::zero());
    let rotation_time = cert.not_after() - renew_threshold;

    now >= rotation_time
}

/// Rotates a certificate using the CA.
///
/// # Arguments
///
/// * `ca` - The Certificate Authority.
/// * `store` - The certificate store.
/// * `cert_id` - The ID of the certificate to rotate.
///
/// # Returns
///
/// The new certificate and its ID.
///
/// # Errors
///
/// Returns an error if the certificate is not found or rotation fails.
pub fn rotate(
    ca: &CertificateAuthority,
    store: &CertStore,
    cert_id: &CertificateId,
) -> Result<(CertificateId, Certificate, PrivateKey)> {
    let (old_cert, _old_key) = store.get(cert_id)?;

    info!(
        "Rotating certificate: {} (subject: {})",
        cert_id,
        old_cert.subject()
    );

    // Create a new certificate request based on the old certificate
    let request = create_rotation_request(&old_cert)?;

    // Issue a new certificate
    let (new_cert, new_key) = ca.issue(&request)?;

    // Store the new certificate
    let new_id = store.store(new_cert.clone(), new_key.clone())?;

    info!(
        "Certificate rotated: {} -> {} (subject: {})",
        cert_id,
        new_id,
        new_cert.subject()
    );

    Ok((new_id, new_cert, new_key))
}

/// Creates a rotation request based on an existing certificate.
fn create_rotation_request(cert: &Certificate) -> Result<CertificateRequest> {
    // Calculate validity: same duration as original certificate
    let original_duration = cert.not_after() - cert.not_before();
    let validity_days = original_duration.num_days();

    if validity_days <= 0 {
        return Err(Error::Validation(
            "cannot rotate certificate with non-positive validity".into(),
        ));
    }

    // For simplicity, assume server auth if we can't determine
    // In a real implementation, we'd parse the key usage from the certificate
    let request = CertificateRequest {
        subject: cert.subject().to_string(),
        san: cert.san().to_vec(),
        validity_days: u32::try_from(validity_days)
            .map_err(|_| Error::Validation("validity days out of range".into()))?,
        key_usage: vec![KeyUsage::ServerAuth, KeyUsage::ClientAuth],
    };

    request.validate()?;

    Ok(request)
}

/// Rotates all certificates that need rotation according to the policy.
///
/// # Arguments
///
/// * `ca` - The Certificate Authority.
/// * `store` - The certificate store.
/// * `policy` - The rotation policy.
///
/// # Returns
///
/// A list of (`old_id`, `new_id`) pairs for rotated certificates.
///
/// # Errors
///
/// Returns an error if rotation fails for any certificate.
pub fn rotate_all_needed(
    ca: &CertificateAuthority,
    store: &CertStore,
    policy: &RotationPolicy,
) -> Result<Vec<(CertificateId, CertificateId)>> {
    if !policy.auto_rotate() {
        return Ok(Vec::new());
    }

    let expiring = store.list_expiring(policy.renew_before_expiry());
    let mut rotated = Vec::with_capacity(expiring.len());

    for cert_id in expiring {
        // Skip the CA root certificate if it's in the store
        if let Ok(cert) = store.get_certificate(&cert_id) {
            if cert.subject() == cert.issuer() {
                // Self-signed (CA) certificate, skip
                continue;
            }
        }

        let (new_id, _, _) = rotate(ca, store, &cert_id)?;
        rotated.push((cert_id, new_id));
    }

    Ok(rotated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn create_test_cert(subject: &str, validity_days: i64) -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now,
            now + ChronoDuration::days(validity_days),
            subject.into(),
            "Test CA".into(),
            vec![],
        )
    }

    fn create_expiring_cert(subject: &str, expires_in_days: i64) -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now - ChronoDuration::days(365 - expires_in_days),
            now + ChronoDuration::days(expires_in_days),
            subject.into(),
            "Test CA".into(),
            vec![],
        )
    }

    #[test]
    fn rotation_policy_default() {
        let policy = RotationPolicy::default();
        assert_eq!(
            policy.renew_before_expiry(),
            Duration::from_secs(30 * 24 * 60 * 60)
        );
        assert!(policy.auto_rotate());
    }

    #[test]
    fn rotation_policy_custom() {
        let policy = RotationPolicy::new(Duration::from_secs(7 * 24 * 60 * 60), false);
        assert_eq!(
            policy.renew_before_expiry(),
            Duration::from_secs(7 * 24 * 60 * 60)
        );
        assert!(!policy.auto_rotate());
    }

    #[test]
    fn check_rotation_needed_not_needed() {
        let cert = create_test_cert("test.example.com", 365);
        let policy = RotationPolicy::default(); // 30 days before expiry

        assert!(!check_rotation_needed(&cert, &policy));
    }

    #[test]
    fn check_rotation_needed_needed() {
        let cert = create_expiring_cert("test.example.com", 10);
        let policy = RotationPolicy::default(); // 30 days before expiry

        assert!(check_rotation_needed(&cert, &policy));
    }

    #[test]
    fn check_rotation_needed_at_boundary() {
        let cert = create_expiring_cert("test.example.com", 30);
        let policy = RotationPolicy::new(Duration::from_secs(30 * 24 * 60 * 60), true);

        // At exactly 30 days, should trigger rotation
        assert!(check_rotation_needed(&cert, &policy));
    }

    #[test]
    fn rotate_certificate_with_real_ca() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        let store = CertStore::new();

        // Create and store a certificate
        let request = CertificateRequest::builder("test.example.com")
            .validity_days(90)
            .server_auth()
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();
        let cert_id = store.store(cert, key).unwrap();

        // Rotate it
        let (new_id, new_cert, _new_key) = rotate(&ca, &store, &cert_id).unwrap();

        assert_ne!(new_id, cert_id);
        assert_eq!(new_cert.subject(), "test.example.com");
        assert_eq!(new_cert.issuer(), "Test CA");
    }

    #[test]
    fn rotate_nonexistent_certificate() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        let store = CertStore::new();
        let cert_id = CertificateId::new();

        let result = rotate(&ca, &store, &cert_id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NotFound(_)));
    }

    #[test]
    fn rotate_all_needed_with_auto_rotate_disabled() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        let store = CertStore::new();
        let policy = RotationPolicy::new(Duration::from_secs(30 * 24 * 60 * 60), false);

        let rotated = rotate_all_needed(&ca, &store, &policy).unwrap();
        assert!(rotated.is_empty());
    }

    #[test]
    fn rotation_policy_serialization() {
        let policy = RotationPolicy::new(Duration::from_secs(86400), true);
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: RotationPolicy = serde_json::from_str(&json).unwrap();

        assert_eq!(
            policy.renew_before_expiry(),
            deserialized.renew_before_expiry()
        );
        assert_eq!(policy.auto_rotate(), deserialized.auto_rotate());
    }

    #[test]
    fn create_rotation_request_preserves_subject() {
        let cert = create_test_cert("preserve-me.example.com", 90);
        let request = create_rotation_request(&cert).unwrap();

        assert_eq!(request.subject, "preserve-me.example.com");
        assert_eq!(request.validity_days, 90);
    }
}
