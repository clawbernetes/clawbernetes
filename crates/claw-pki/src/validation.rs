//! Certificate validation utilities.

use chrono::Utc;
use tracing::debug;
use x509_parser::prelude::*;

use crate::error::{Error, Result};
use crate::types::Certificate;

/// Validates a certificate against its issuing CA certificate.
///
/// This performs the following checks:
/// - The certificate is not expired
/// - The certificate is not yet valid (`not_before` check)
/// - The certificate was signed by the CA
/// - The issuer matches the CA's subject
///
/// # Arguments
///
/// * `cert` - The certificate to validate.
/// * `ca_cert` - The CA certificate that should have issued this certificate.
///
/// # Errors
///
/// Returns an error if validation fails.
pub fn validate_certificate(cert: &Certificate, ca_cert: &Certificate) -> Result<()> {
    debug!("Validating certificate: {}", cert.subject());

    // Check expiration
    if is_expired(cert) {
        return Err(Error::Expired);
    }

    // Check not_before
    if is_not_yet_valid(cert) {
        return Err(Error::NotYetValid);
    }

    // Check issuer matches CA subject
    if cert.issuer() != ca_cert.subject() {
        return Err(Error::Validation(format!(
            "issuer '{}' does not match CA subject '{}'",
            cert.issuer(),
            ca_cert.subject()
        )));
    }

    // Verify signature
    verify_signature(cert, ca_cert)?;

    debug!("Certificate validated successfully: {}", cert.subject());

    Ok(())
}

/// Validates a certificate chain.
///
/// The chain should be ordered from end-entity to root CA.
/// Each certificate should be signed by the next certificate in the chain.
///
/// # Arguments
///
/// * `chain` - The certificate chain (end-entity first, root last).
///
/// # Errors
///
/// Returns an error if the chain is invalid.
pub fn validate_chain(chain: &[Certificate]) -> Result<()> {
    if chain.is_empty() {
        return Err(Error::InvalidChain("empty certificate chain".into()));
    }

    if chain.len() == 1 {
        // Single certificate must be self-signed (root)
        let cert = &chain[0];
        if cert.issuer() != cert.subject() {
            return Err(Error::InvalidChain(
                "single certificate in chain must be self-signed".into(),
            ));
        }
        return validate_self_signed(cert);
    }

    // Validate each certificate against its issuer
    for i in 0..chain.len() - 1 {
        let cert = &chain[i];
        let issuer = &chain[i + 1];

        validate_certificate(cert, issuer)?;
    }

    // Validate root certificate is self-signed
    let root = &chain[chain.len() - 1];
    validate_self_signed(root)?;

    Ok(())
}

/// Checks if a certificate is expired.
///
/// # Arguments
///
/// * `cert` - The certificate to check.
///
/// # Returns
///
/// `true` if the certificate has expired.
#[must_use]
pub fn is_expired(cert: &Certificate) -> bool {
    cert.not_after() < Utc::now()
}

/// Checks if a certificate is not yet valid.
///
/// # Arguments
///
/// * `cert` - The certificate to check.
///
/// # Returns
///
/// `true` if the certificate is not yet valid.
#[must_use]
pub fn is_not_yet_valid(cert: &Certificate) -> bool {
    cert.not_before() > Utc::now()
}

/// Checks if a certificate is currently valid (not expired and `not_before` has passed).
///
/// # Arguments
///
/// * `cert` - The certificate to check.
///
/// # Returns
///
/// `true` if the certificate is currently valid.
#[must_use]
pub fn is_valid_now(cert: &Certificate) -> bool {
    !is_expired(cert) && !is_not_yet_valid(cert)
}

/// Calculates the remaining validity period.
///
/// # Arguments
///
/// * `cert` - The certificate to check.
///
/// # Returns
///
/// The duration until expiry, or None if already expired.
#[must_use]
pub fn remaining_validity(cert: &Certificate) -> Option<chrono::Duration> {
    let now = Utc::now();
    if cert.not_after() > now {
        Some(cert.not_after() - now)
    } else {
        None
    }
}

/// Validates a self-signed certificate.
fn validate_self_signed(cert: &Certificate) -> Result<()> {
    if cert.issuer() != cert.subject() {
        return Err(Error::Validation("certificate is not self-signed".into()));
    }

    // For self-signed, verify signature against itself
    verify_signature(cert, cert)?;

    Ok(())
}

/// Verifies that a certificate was signed by the given issuer.
fn verify_signature(cert: &Certificate, issuer: &Certificate) -> Result<()> {
    let (_, parsed_cert) = X509Certificate::from_der(cert.der())
        .map_err(|e| Error::Parse(format!("failed to parse certificate: {e}")))?;

    let (_, parsed_issuer) = X509Certificate::from_der(issuer.der())
        .map_err(|e| Error::Parse(format!("failed to parse issuer certificate: {e}")))?;

    // Get the public key from the issuer
    let issuer_public_key = parsed_issuer.public_key();

    // Verify the signature using the x509-parser API
    parsed_cert
        .verify_signature(Some(issuer_public_key))
        .map_err(|e| {
            Error::SignatureVerification(format!(
                "signature verification failed for '{}': {:?}",
                cert.subject(),
                e
            ))
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::CertificateAuthority;
    use crate::types::CertificateRequest;
    use chrono::Duration;

    fn create_test_cert(subject: &str, validity_days: i64, issuer: &str) -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now - Duration::hours(1),
            now + Duration::days(validity_days),
            subject.into(),
            issuer.into(),
            vec![],
        )
    }

    fn create_expired_cert() -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now - Duration::days(60),
            now - Duration::days(30),
            "expired".into(),
            "Test CA".into(),
            vec![],
        )
    }

    fn create_not_yet_valid_cert() -> Certificate {
        let now = Utc::now();
        Certificate::new(
            vec![1, 2, 3],
            now + Duration::days(30),
            now + Duration::days(60),
            "future".into(),
            "Test CA".into(),
            vec![],
        )
    }

    #[test]
    fn is_expired_true_for_expired_cert() {
        let cert = create_expired_cert();
        assert!(is_expired(&cert));
    }

    #[test]
    fn is_expired_false_for_valid_cert() {
        let cert = create_test_cert("test", 30, "CA");
        assert!(!is_expired(&cert));
    }

    #[test]
    fn is_not_yet_valid_true_for_future_cert() {
        let cert = create_not_yet_valid_cert();
        assert!(is_not_yet_valid(&cert));
    }

    #[test]
    fn is_not_yet_valid_false_for_current_cert() {
        let cert = create_test_cert("test", 30, "CA");
        assert!(!is_not_yet_valid(&cert));
    }

    #[test]
    fn is_valid_now_for_current_cert() {
        let cert = create_test_cert("test", 30, "CA");
        assert!(is_valid_now(&cert));
    }

    #[test]
    fn is_valid_now_false_for_expired() {
        let cert = create_expired_cert();
        assert!(!is_valid_now(&cert));
    }

    #[test]
    fn is_valid_now_false_for_future() {
        let cert = create_not_yet_valid_cert();
        assert!(!is_valid_now(&cert));
    }

    #[test]
    fn remaining_validity_some_for_valid() {
        let cert = create_test_cert("test", 30, "CA");
        let remaining = remaining_validity(&cert);
        assert!(remaining.is_some());
        assert!(remaining.unwrap().num_days() >= 29);
    }

    #[test]
    fn remaining_validity_none_for_expired() {
        let cert = create_expired_cert();
        assert!(remaining_validity(&cert).is_none());
    }

    #[test]
    fn validate_certificate_with_real_ca() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("test.example.com")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, _) = ca.issue(&request).unwrap();

        let result = validate_certificate(&cert, ca.root_certificate());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_certificate_wrong_issuer() {
        let ca1 = CertificateAuthority::new("CA One").unwrap();
        let ca2 = CertificateAuthority::new("CA Two").unwrap();

        let request = CertificateRequest::builder("test.example.com")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, _) = ca1.issue(&request).unwrap();

        // Validate against wrong CA
        let result = validate_certificate(&cert, ca2.root_certificate());
        assert!(result.is_err());
    }

    #[test]
    fn validate_chain_single_self_signed() {
        let ca = CertificateAuthority::new("Root CA").unwrap();
        let chain = vec![ca.root_certificate().clone()];

        let result = validate_chain(&chain);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_chain_two_certs() {
        let ca = CertificateAuthority::new("Root CA").unwrap();

        let request = CertificateRequest::builder("end-entity")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (end_entity, _) = ca.issue(&request).unwrap();
        let chain = vec![end_entity, ca.root_certificate().clone()];

        let result = validate_chain(&chain);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_chain_empty() {
        let result = validate_chain(&[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidChain(_)));
    }

    #[test]
    fn validate_self_signed_ca() {
        let ca = CertificateAuthority::new("Self-Signed CA").unwrap();
        let result = validate_self_signed(ca.root_certificate());
        assert!(result.is_ok());
    }
}
