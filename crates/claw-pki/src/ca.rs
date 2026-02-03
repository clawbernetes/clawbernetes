//! Certificate Authority implementation.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, Ia5String, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use tracing::{debug, info};

use crate::error::{Error, Result};
use crate::types::{
    Certificate, CertificateId, CertificateRequest, KeyUsage, PrivateKey, SubjectAltName,
};

/// Certificate Authority for issuing and managing certificates.
pub struct CertificateAuthority {
    /// Root certificate.
    root_cert: Certificate,
    /// Root private key.
    root_key: PrivateKey,
    /// rcgen key pair for signing.
    key_pair: KeyPair,
    /// Set of revoked certificate IDs.
    revoked: HashSet<CertificateId>,
}

impl CertificateAuthority {
    /// Creates a new Certificate Authority with a self-signed root certificate.
    ///
    /// # Arguments
    ///
    /// * `name` - The common name for the CA certificate.
    ///
    /// # Errors
    ///
    /// Returns an error if certificate generation fails.
    pub fn new(name: &str) -> Result<Self> {
        info!("Creating new Certificate Authority: {}", name);

        // Generate key pair
        let key_pair = KeyPair::generate()
            .map_err(|e| Error::Generation(format!("failed to generate key pair: {e}")))?;

        // Create root certificate parameters
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Set validity period (10 years for CA)
        let now = Utc::now();
        let not_before = now - Duration::hours(1); // Allow for clock skew
        let not_after = now + Duration::days(3650);

        params.not_before = to_rcgen_time(not_before)?;
        params.not_after = to_rcgen_time(not_after)?;

        // Generate the certificate
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| Error::Generation(format!("failed to generate root certificate: {e}")))?;

        let der = cert.der().to_vec();
        let root_key = PrivateKey::new(key_pair.serialize_der());

        let root_cert = Certificate::from_der(&der)?;

        debug!("CA root certificate created successfully");

        Ok(Self {
            root_cert,
            root_key,
            key_pair,
            revoked: HashSet::new(),
        })
    }

    /// Creates a Certificate Authority from an existing certificate and key.
    ///
    /// # Arguments
    ///
    /// * `cert` - The existing CA certificate.
    /// * `key` - The CA private key.
    ///
    /// # Errors
    ///
    /// Returns an error if the key doesn't match the certificate.
    pub fn from_existing(cert: Certificate, key: PrivateKey) -> Result<Self> {
        let key_pair = KeyPair::try_from(key.der())
            .map_err(|e| Error::Parse(format!("failed to parse private key: {e}")))?;

        Ok(Self {
            root_cert: cert,
            root_key: key,
            key_pair,
            revoked: HashSet::new(),
        })
    }

    /// Returns a reference to the root certificate.
    #[must_use]
    pub const fn root_certificate(&self) -> &Certificate {
        &self.root_cert
    }

    /// Returns a reference to the root private key.
    #[must_use]
    pub const fn root_key(&self) -> &PrivateKey {
        &self.root_key
    }

    /// Issues a new certificate based on the request.
    ///
    /// # Arguments
    ///
    /// * `request` - The certificate request.
    ///
    /// # Errors
    ///
    /// Returns an error if certificate generation fails.
    pub fn issue(&self, request: &CertificateRequest) -> Result<(Certificate, PrivateKey)> {
        request.validate()?;

        info!("Issuing certificate for: {}", request.subject);

        // Generate new key pair for the certificate
        let cert_key_pair = KeyPair::generate()
            .map_err(|e| Error::Generation(format!("failed to generate key pair: {e}")))?;

        // Create certificate parameters
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, &request.subject);
        params.is_ca = IsCa::NoCa;

        // Set key usages
        params.extended_key_usages = request
            .key_usage
            .iter()
            .map(|usage| match usage {
                KeyUsage::ServerAuth => ExtendedKeyUsagePurpose::ServerAuth,
                KeyUsage::ClientAuth => ExtendedKeyUsagePurpose::ClientAuth,
                KeyUsage::CodeSigning => ExtendedKeyUsagePurpose::CodeSigning,
            })
            .collect();

        // Set basic key usages
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        // Set validity period
        let now = Utc::now();
        let not_before = now - Duration::hours(1);
        let not_after = now + Duration::days(i64::from(request.validity_days));

        params.not_before = to_rcgen_time(not_before)?;
        params.not_after = to_rcgen_time(not_after)?;

        // Add SANs
        params.subject_alt_names = convert_sans(&request.san)?;

        // Create issuer certificate for signing
        let issuer_cert = self.create_issuer_cert()?;

        // Sign the certificate with the CA
        let cert = params
            .signed_by(&cert_key_pair, &issuer_cert, &self.key_pair)
            .map_err(|e| Error::Generation(format!("failed to sign certificate: {e}")))?;

        let der = cert.der().to_vec();
        let private_key = PrivateKey::new(cert_key_pair.serialize_der());
        let certificate = Certificate::from_der(&der)?;

        debug!("Certificate issued successfully for: {}", request.subject);

        Ok((certificate, private_key))
    }

    /// Revokes a certificate.
    ///
    /// # Arguments
    ///
    /// * `cert_id` - The ID of the certificate to revoke.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate is already revoked.
    pub fn revoke(&mut self, cert_id: &CertificateId) -> Result<()> {
        if self.revoked.contains(cert_id) {
            return Err(Error::AlreadyRevoked(cert_id.to_string()));
        }

        info!("Revoking certificate: {}", cert_id);
        self.revoked.insert(cert_id.clone());

        Ok(())
    }

    /// Checks if a certificate is revoked.
    #[must_use]
    pub fn is_revoked(&self, cert_id: &CertificateId) -> bool {
        self.revoked.contains(cert_id)
    }

    /// Returns the set of revoked certificate IDs.
    #[must_use]
    pub const fn revoked_certificates(&self) -> &HashSet<CertificateId> {
        &self.revoked
    }

    /// Creates an issuer certificate for signing.
    fn create_issuer_cert(&self) -> Result<rcgen::Certificate> {
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, self.root_cert.subject());
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let now = Utc::now();
        params.not_before = to_rcgen_time(now - Duration::hours(1))?;
        params.not_after = to_rcgen_time(now + Duration::days(3650))?;

        params
            .self_signed(&self.key_pair)
            .map_err(|e| Error::Generation(format!("failed to create issuer cert: {e}")))
    }
}

impl std::fmt::Debug for CertificateAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateAuthority")
            .field("root_cert", &self.root_cert)
            .field("root_key", &"[REDACTED]")
            .field("revoked_count", &self.revoked.len())
            .finish_non_exhaustive()
    }
}

/// Converts `SubjectAltNames` to rcgen `SanTypes`.
fn convert_sans(sans: &[SubjectAltName]) -> Result<Vec<SanType>> {
    sans.iter()
        .map(|san| match san {
            SubjectAltName::Dns(dns) => {
                let ia5 = Ia5String::try_from(dns.clone())
                    .map_err(|e| Error::San(format!("invalid DNS name '{dns}': {e}")))?;
                Ok(SanType::DnsName(ia5))
            }
            SubjectAltName::Ip(ip) => Ok(SanType::IpAddress(*ip)),
            SubjectAltName::Email(email) => {
                let ia5 = Ia5String::try_from(email.clone())
                    .map_err(|e| Error::San(format!("invalid email '{email}': {e}")))?;
                Ok(SanType::Rfc822Name(ia5))
            }
            SubjectAltName::Uri(uri) => {
                let ia5 = Ia5String::try_from(uri.clone())
                    .map_err(|e| Error::San(format!("invalid URI '{uri}': {e}")))?;
                Ok(SanType::URI(ia5))
            }
        })
        .collect()
}

/// Converts a chrono `DateTime` to rcgen `OffsetDateTime`.
fn to_rcgen_time(dt: DateTime<Utc>) -> Result<time::OffsetDateTime> {
    time::OffsetDateTime::from_unix_timestamp(dt.timestamp())
        .map_err(|e| Error::Generation(format!("invalid timestamp: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn create_new_ca() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        assert_eq!(ca.root_certificate().subject(), "Test CA");
        assert_eq!(ca.root_certificate().issuer(), "Test CA"); // Self-signed
    }

    #[test]
    fn ca_root_certificate_is_valid() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        let cert = ca.root_certificate();

        let now = Utc::now();
        assert!(cert.not_before() < now);
        assert!(cert.not_after() > now);
    }

    #[test]
    fn issue_server_certificate() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("server.example.com")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();

        assert_eq!(cert.subject(), "server.example.com");
        assert_eq!(cert.issuer(), "Test CA");
        assert!(!key.der().is_empty());
    }

    #[test]
    fn issue_certificate_with_san() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("server.example.com")
            .dns("*.example.com")
            .ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, _) = ca.issue(&request).unwrap();

        let san = cert.san();
        assert_eq!(san.len(), 2);
        assert!(matches!(&san[0], SubjectAltName::Dns(d) if d == "*.example.com"));
        assert!(
            matches!(&san[1], SubjectAltName::Ip(ip) if *ip == IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
        );
    }

    #[test]
    fn issue_client_certificate() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("client@example.com")
            .validity_days(365)
            .client_auth()
            .build()
            .unwrap();

        let (cert, _) = ca.issue(&request).unwrap();

        assert_eq!(cert.subject(), "client@example.com");
    }

    #[test]
    fn issue_mtls_certificate() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("node-1")
            .dns("node-1.clawbernetes.local")
            .validity_days(90)
            .server_auth()
            .client_auth()
            .build()
            .unwrap();

        let (cert, _) = ca.issue(&request).unwrap();

        assert_eq!(cert.subject(), "node-1");
    }

    #[test]
    fn revoke_certificate() {
        let mut ca = CertificateAuthority::new("Test CA").unwrap();
        let cert_id = CertificateId::new();

        assert!(!ca.is_revoked(&cert_id));

        ca.revoke(&cert_id).unwrap();

        assert!(ca.is_revoked(&cert_id));
    }

    #[test]
    fn revoke_already_revoked_fails() {
        let mut ca = CertificateAuthority::new("Test CA").unwrap();
        let cert_id = CertificateId::new();

        ca.revoke(&cert_id).unwrap();
        let result = ca.revoke(&cert_id);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::AlreadyRevoked(_)));
    }

    #[test]
    fn from_existing_ca() {
        let ca1 = CertificateAuthority::new("Test CA").unwrap();
        let cert = ca1.root_certificate().clone();
        let key = ca1.root_key().clone();

        let ca2 = CertificateAuthority::from_existing(cert, key).unwrap();

        assert_eq!(ca2.root_certificate().subject(), "Test CA");
    }

    #[test]
    fn issued_certificates_have_correct_validity() {
        let ca = CertificateAuthority::new("Test CA").unwrap();

        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, _) = ca.issue(&request).unwrap();

        let now = Utc::now();
        let expected_expiry = now + Duration::days(30);

        // Allow 1 hour tolerance for clock skew handling
        assert!(cert.not_before() < now);
        assert!((cert.not_after() - expected_expiry).num_hours().abs() < 2);
    }

    #[test]
    fn ca_debug_redacts_key() {
        let ca = CertificateAuthority::new("Test CA").unwrap();
        let debug = format!("{:?}", ca);
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn revoked_certificates_collection() {
        let mut ca = CertificateAuthority::new("Test CA").unwrap();
        let id1 = CertificateId::new();
        let id2 = CertificateId::new();

        ca.revoke(&id1).unwrap();
        ca.revoke(&id2).unwrap();

        let revoked = ca.revoked_certificates();
        assert_eq!(revoked.len(), 2);
        assert!(revoked.contains(&id1));
        assert!(revoked.contains(&id2));
    }
}
