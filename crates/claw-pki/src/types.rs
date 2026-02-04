//! Core PKI types for certificate management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};

/// Unique identifier for a certificate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CertificateId(Uuid);

impl CertificateId {
    /// Creates a new random certificate ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a certificate ID from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for CertificateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CertificateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Key usage purposes for certificates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyUsage {
    /// TLS server authentication.
    ServerAuth,
    /// TLS client authentication.
    ClientAuth,
    /// Code signing.
    CodeSigning,
}

impl KeyUsage {
    /// Returns the OID string for this key usage.
    #[must_use]
    pub const fn oid(&self) -> &'static str {
        match self {
            Self::ServerAuth => "1.3.6.1.5.5.7.3.1",
            Self::ClientAuth => "1.3.6.1.5.5.7.3.2",
            Self::CodeSigning => "1.3.6.1.5.5.7.3.3",
        }
    }
}

/// Subject Alternative Name types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubjectAltName {
    /// DNS name.
    Dns(String),
    /// IP address.
    Ip(std::net::IpAddr),
    /// Email address.
    Email(String),
    /// URI.
    Uri(String),
}

/// Request to issue a new certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    /// Subject common name.
    pub subject: String,
    /// Subject alternative names.
    pub san: Vec<SubjectAltName>,
    /// Validity period in days.
    pub validity_days: u32,
    /// Key usage purposes.
    pub key_usage: Vec<KeyUsage>,
}

impl CertificateRequest {
    /// Creates a new certificate request builder.
    #[must_use]
    pub fn builder(subject: impl Into<String>) -> CertificateRequestBuilder {
        CertificateRequestBuilder {
            subject: subject.into(),
            san: Vec::new(),
            validity_days: 365,
            key_usage: Vec::new(),
        }
    }

    /// Validates the certificate request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.subject.is_empty() {
            return Err(Error::Validation("subject cannot be empty".into()));
        }
        if self.validity_days == 0 {
            return Err(Error::Validation("validity_days must be greater than 0".into()));
        }
        if self.key_usage.is_empty() {
            return Err(Error::Validation("at least one key usage is required".into()));
        }
        Ok(())
    }
}

/// Builder for certificate requests.
#[derive(Debug)]
pub struct CertificateRequestBuilder {
    subject: String,
    san: Vec<SubjectAltName>,
    validity_days: u32,
    key_usage: Vec<KeyUsage>,
}

impl CertificateRequestBuilder {
    /// Adds a DNS subject alternative name.
    #[must_use]
    pub fn dns(mut self, dns: impl Into<String>) -> Self {
        self.san.push(SubjectAltName::Dns(dns.into()));
        self
    }

    /// Adds an IP subject alternative name.
    #[must_use]
    pub fn ip(mut self, ip: std::net::IpAddr) -> Self {
        self.san.push(SubjectAltName::Ip(ip));
        self
    }

    /// Adds an email subject alternative name.
    #[must_use]
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.san.push(SubjectAltName::Email(email.into()));
        self
    }

    /// Adds a URI subject alternative name.
    #[must_use]
    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.san.push(SubjectAltName::Uri(uri.into()));
        self
    }

    /// Sets the validity period in days.
    #[must_use]
    pub const fn validity_days(mut self, days: u32) -> Self {
        self.validity_days = days;
        self
    }

    /// Adds a key usage.
    #[must_use]
    pub fn key_usage(mut self, usage: KeyUsage) -> Self {
        self.key_usage.push(usage);
        self
    }

    /// Adds server authentication key usage.
    #[must_use]
    pub fn server_auth(self) -> Self {
        self.key_usage(KeyUsage::ServerAuth)
    }

    /// Adds client authentication key usage.
    #[must_use]
    pub fn client_auth(self) -> Self {
        self.key_usage(KeyUsage::ClientAuth)
    }

    /// Builds the certificate request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is invalid.
    pub fn build(self) -> Result<CertificateRequest> {
        let request = CertificateRequest {
            subject: self.subject,
            san: self.san,
            validity_days: self.validity_days,
            key_usage: self.key_usage,
        };
        request.validate()?;
        Ok(request)
    }
}

/// A DER-encoded X.509 certificate with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// DER-encoded certificate bytes.
    der: Vec<u8>,
    /// Certificate validity start time.
    not_before: DateTime<Utc>,
    /// Certificate validity end time.
    not_after: DateTime<Utc>,
    /// Subject common name.
    subject: String,
    /// Issuer common name.
    issuer: String,
    /// Subject alternative names.
    san: Vec<SubjectAltName>,
}

impl Certificate {
    /// Creates a new certificate from raw DER bytes and metadata.
    #[must_use]
    pub const fn new(
        der: Vec<u8>,
        not_before: DateTime<Utc>,
        not_after: DateTime<Utc>,
        subject: String,
        issuer: String,
        san: Vec<SubjectAltName>,
    ) -> Self {
        Self {
            der,
            not_before,
            not_after,
            subject,
            issuer,
            san,
        }
    }

    /// Parses a certificate from DER-encoded bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn from_der(der: &[u8]) -> Result<Self> {
        use x509_parser::prelude::*;

        let (_, cert) = X509Certificate::from_der(der)
            .map_err(|e| Error::Parse(format!("failed to parse certificate: {e}")))?;

        let not_before = DateTime::from_timestamp(cert.validity().not_before.timestamp(), 0)
            .ok_or_else(|| Error::Parse("invalid not_before timestamp".into()))?;
        let not_after = DateTime::from_timestamp(cert.validity().not_after.timestamp(), 0)
            .ok_or_else(|| Error::Parse("invalid not_after timestamp".into()))?;

        let subject = extract_common_name(cert.subject())?;
        let issuer = extract_common_name(cert.issuer())?;
        let san = extract_san(&cert);

        Ok(Self {
            der: der.to_vec(),
            not_before,
            not_after,
            subject,
            issuer,
            san,
        })
    }

    /// Returns the DER-encoded certificate bytes.
    #[must_use]
    pub fn der(&self) -> &[u8] {
        &self.der
    }

    /// Returns the PEM-encoded certificate.
    #[must_use]
    pub fn pem(&self) -> String {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&self.der);
        format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            b64.as_bytes()
                .chunks(64)
                .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    /// Returns the certificate validity start time.
    #[must_use]
    pub const fn not_before(&self) -> DateTime<Utc> {
        self.not_before
    }

    /// Returns the certificate validity end time.
    #[must_use]
    pub const fn not_after(&self) -> DateTime<Utc> {
        self.not_after
    }

    /// Returns the subject common name.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the issuer common name.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the subject alternative names.
    #[must_use]
    pub fn san(&self) -> &[SubjectAltName] {
        &self.san
    }
}

/// Extracts the common name from an X.509 name.
fn extract_common_name(name: &x509_parser::x509::X509Name) -> Result<String> {
    for rdn in name.iter() {
        for attr in rdn.iter() {
            if attr.attr_type() == &x509_parser::oid_registry::OID_X509_COMMON_NAME {
                return attr
                    .as_str()
                    .map(String::from)
                    .map_err(|e| Error::Parse(format!("failed to parse CN: {e}")));
            }
        }
    }
    Err(Error::Parse("common name not found".into()))
}

/// Extracts SANs from a certificate.
fn extract_san(cert: &x509_parser::certificate::X509Certificate) -> Vec<SubjectAltName> {
    let mut sans = Vec::new();

    if let Ok(Some(san_ext)) = cert.subject_alternative_name() {
        for name in &san_ext.value.general_names {
            match name {
                x509_parser::extensions::GeneralName::DNSName(dns) => {
                    sans.push(SubjectAltName::Dns((*dns).to_string()));
                }
                x509_parser::extensions::GeneralName::IPAddress(ip_bytes) => {
                    if let Some(ip) = parse_ip_bytes(ip_bytes) {
                        sans.push(SubjectAltName::Ip(ip));
                    }
                }
                x509_parser::extensions::GeneralName::RFC822Name(email) => {
                    sans.push(SubjectAltName::Email((*email).to_string()));
                }
                x509_parser::extensions::GeneralName::URI(uri) => {
                    sans.push(SubjectAltName::Uri((*uri).to_string()));
                }
                _ => {}
            }
        }
    }

    sans
}

/// Parses IP address bytes into an `IpAddr`.
fn parse_ip_bytes(bytes: &[u8]) -> Option<std::net::IpAddr> {
    match bytes.len() {
        4 => {
            let octets: [u8; 4] = bytes.try_into().ok()?;
            Some(std::net::IpAddr::V4(std::net::Ipv4Addr::from(octets)))
        }
        16 => {
            let octets: [u8; 16] = bytes.try_into().ok()?;
            Some(std::net::IpAddr::V6(std::net::Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

/// A private key with secure memory handling.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey {
    /// DER-encoded private key bytes.
    der: Vec<u8>,
}

impl PrivateKey {
    /// Creates a new private key from DER-encoded bytes.
    #[must_use]
    pub const fn new(der: Vec<u8>) -> Self {
        Self { der }
    }

    /// Returns the DER-encoded private key bytes.
    #[must_use]
    pub fn der(&self) -> &[u8] {
        &self.der
    }

    /// Returns the PEM-encoded private key.
    #[must_use]
    pub fn pem(&self) -> String {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&self.der);
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            b64.as_bytes()
                .chunks(64)
                .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateKey")
            .field("der", &"[REDACTED]")
            .finish()
    }
}

impl Clone for PrivateKey {
    fn clone(&self) -> Self {
        Self {
            der: self.der.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn certificate_id_is_unique() {
        let id1 = CertificateId::new();
        let id2 = CertificateId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn certificate_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = CertificateId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    #[test]
    fn certificate_id_display() {
        let uuid = Uuid::new_v4();
        let id = CertificateId::from_uuid(uuid);
        assert_eq!(id.to_string(), uuid.to_string());
    }

    #[test]
    fn certificate_id_serialization() {
        let id = CertificateId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: CertificateId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn key_usage_oid() {
        assert_eq!(KeyUsage::ServerAuth.oid(), "1.3.6.1.5.5.7.3.1");
        assert_eq!(KeyUsage::ClientAuth.oid(), "1.3.6.1.5.5.7.3.2");
        assert_eq!(KeyUsage::CodeSigning.oid(), "1.3.6.1.5.5.7.3.3");
    }

    #[test]
    fn certificate_request_builder_basic() {
        let request = CertificateRequest::builder("test.example.com")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        assert_eq!(request.subject, "test.example.com");
        assert_eq!(request.validity_days, 30);
        assert!(request.key_usage.contains(&KeyUsage::ServerAuth));
    }

    #[test]
    fn certificate_request_builder_with_san() {
        let request = CertificateRequest::builder("test.example.com")
            .dns("*.example.com")
            .ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))
            .email("admin@example.com")
            .uri("https://example.com")
            .server_auth()
            .client_auth()
            .build()
            .unwrap();

        assert_eq!(request.san.len(), 4);
        assert!(matches!(&request.san[0], SubjectAltName::Dns(s) if s == "*.example.com"));
        assert!(matches!(&request.san[1], SubjectAltName::Ip(ip) if *ip == IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(matches!(&request.san[2], SubjectAltName::Email(e) if e == "admin@example.com"));
        assert!(matches!(&request.san[3], SubjectAltName::Uri(u) if u == "https://example.com"));
    }

    #[test]
    fn certificate_request_validation_empty_subject() {
        let result = CertificateRequest::builder("")
            .server_auth()
            .build();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn certificate_request_validation_zero_validity() {
        let result = CertificateRequest::builder("test")
            .validity_days(0)
            .server_auth()
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn certificate_request_validation_no_key_usage() {
        let result = CertificateRequest::builder("test").build();
        assert!(result.is_err());
    }

    #[test]
    fn private_key_debug_redacted() {
        let key = PrivateKey::new(vec![1, 2, 3, 4]);
        let debug = format!("{:?}", key);
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("1"));
    }

    #[test]
    fn private_key_clone() {
        let key = PrivateKey::new(vec![1, 2, 3, 4]);
        let cloned = key.clone();
        assert_eq!(key.der(), cloned.der());
    }

    #[test]
    fn private_key_pem_format() {
        let key = PrivateKey::new(vec![1, 2, 3, 4]);
        let pem = key.pem();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        assert!(pem.ends_with("-----END PRIVATE KEY-----\n"));
    }

    #[test]
    fn certificate_pem_format() {
        let cert = Certificate::new(
            vec![1, 2, 3, 4],
            Utc::now(),
            Utc::now(),
            "subject".into(),
            "issuer".into(),
            vec![],
        );
        let pem = cert.pem();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(pem.ends_with("-----END CERTIFICATE-----\n"));
    }

    #[test]
    fn certificate_accessors() {
        let now = Utc::now();
        let later = now + chrono::Duration::days(365);
        let san = vec![SubjectAltName::Dns("example.com".into())];

        let cert = Certificate::new(
            vec![1, 2, 3],
            now,
            later,
            "my-subject".into(),
            "my-issuer".into(),
            san.clone(),
        );

        assert_eq!(cert.der(), &[1, 2, 3]);
        assert_eq!(cert.not_before(), now);
        assert_eq!(cert.not_after(), later);
        assert_eq!(cert.subject(), "my-subject");
        assert_eq!(cert.issuer(), "my-issuer");
        assert_eq!(cert.san().len(), 1);
    }

    #[test]
    fn subject_alt_name_serialization() {
        let san = SubjectAltName::Dns("example.com".into());
        let json = serde_json::to_string(&san).unwrap();
        let deserialized: SubjectAltName = serde_json::from_str(&json).unwrap();
        assert_eq!(san, deserialized);
    }

    #[test]
    fn key_usage_serialization() {
        let usage = KeyUsage::ServerAuth;
        let json = serde_json::to_string(&usage).unwrap();
        let deserialized: KeyUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(usage, deserialized);
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn certificate_id_default() {
        let id = CertificateId::default();
        // Default should create a new unique ID
        assert!(!id.as_uuid().is_nil());
    }

    #[test]
    fn certificate_id_equality() {
        let uuid = Uuid::new_v4();
        let id1 = CertificateId::from_uuid(uuid);
        let id2 = CertificateId::from_uuid(uuid);
        assert_eq!(id1, id2);
    }

    #[test]
    fn certificate_id_hash() {
        use std::collections::HashSet;
        let id1 = CertificateId::new();
        let id2 = CertificateId::new();
        let id3 = CertificateId::from_uuid(*id1.as_uuid());

        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        set.insert(id3); // Same as id1

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn key_usage_all_oids() {
        assert_eq!(KeyUsage::ServerAuth.oid(), "1.3.6.1.5.5.7.3.1");
        assert_eq!(KeyUsage::ClientAuth.oid(), "1.3.6.1.5.5.7.3.2");
        assert_eq!(KeyUsage::CodeSigning.oid(), "1.3.6.1.5.5.7.3.3");
    }

    #[test]
    fn key_usage_equality() {
        assert_eq!(KeyUsage::ServerAuth, KeyUsage::ServerAuth);
        assert_ne!(KeyUsage::ServerAuth, KeyUsage::ClientAuth);
    }

    #[test]
    fn key_usage_clone() {
        let usage = KeyUsage::CodeSigning;
        let cloned = usage.clone();
        assert_eq!(usage, cloned);
    }

    #[test]
    fn key_usage_debug() {
        let usage = KeyUsage::ServerAuth;
        let debug = format!("{:?}", usage);
        assert!(debug.contains("ServerAuth"));
    }

    #[test]
    fn subject_alt_name_dns() {
        let san = SubjectAltName::Dns("example.com".into());
        assert!(matches!(san, SubjectAltName::Dns(s) if s == "example.com"));
    }

    #[test]
    fn subject_alt_name_ip() {
        let san = SubjectAltName::Ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(matches!(san, SubjectAltName::Ip(ip) if ip == IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    }

    #[test]
    fn subject_alt_name_email() {
        let san = SubjectAltName::Email("test@example.com".into());
        assert!(matches!(san, SubjectAltName::Email(e) if e == "test@example.com"));
    }

    #[test]
    fn subject_alt_name_uri() {
        let san = SubjectAltName::Uri("https://example.com".into());
        assert!(matches!(san, SubjectAltName::Uri(u) if u == "https://example.com"));
    }

    #[test]
    fn subject_alt_name_equality() {
        let san1 = SubjectAltName::Dns("example.com".into());
        let san2 = SubjectAltName::Dns("example.com".into());
        let san3 = SubjectAltName::Dns("other.com".into());
        assert_eq!(san1, san2);
        assert_ne!(san1, san3);
    }

    #[test]
    fn subject_alt_name_clone() {
        let san = SubjectAltName::Dns("example.com".into());
        let cloned = san.clone();
        assert_eq!(san, cloned);
    }

    #[test]
    fn certificate_request_builder_all_key_usages() {
        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .server_auth()
            .client_auth()
            .key_usage(KeyUsage::CodeSigning)
            .build()
            .unwrap();

        assert_eq!(request.key_usage.len(), 3);
        assert!(request.key_usage.contains(&KeyUsage::ServerAuth));
        assert!(request.key_usage.contains(&KeyUsage::ClientAuth));
        assert!(request.key_usage.contains(&KeyUsage::CodeSigning));
    }

    #[test]
    fn certificate_request_builder_multiple_dns() {
        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .dns("example.com")
            .dns("www.example.com")
            .dns("api.example.com")
            .server_auth()
            .build()
            .unwrap();

        let dns_count = request.san.iter().filter(|s| matches!(s, SubjectAltName::Dns(_))).count();
        assert_eq!(dns_count, 3);
    }

    #[test]
    fn certificate_request_builder_multiple_ips() {
        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)))
            .ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
            .server_auth()
            .build()
            .unwrap();

        let ip_count = request.san.iter().filter(|s| matches!(s, SubjectAltName::Ip(_))).count();
        assert_eq!(ip_count, 2);
    }

    #[test]
    fn certificate_request_default_validity() {
        let request = CertificateRequest::builder("test")
            .server_auth()
            .build()
            .unwrap();

        // Default validity should be set (usually 365 days)
        assert!(request.validity_days > 0);
    }

    #[test]
    fn private_key_der_access() {
        let der_data = vec![1, 2, 3, 4, 5];
        let key = PrivateKey::new(der_data.clone());
        assert_eq!(key.der(), &der_data);
    }

    #[test]
    fn certificate_debug() {
        let cert = Certificate::new(
            vec![1, 2, 3],
            Utc::now(),
            Utc::now(),
            "subject".into(),
            "issuer".into(),
            vec![],
        );
        let debug = format!("{:?}", cert);
        assert!(debug.contains("Certificate"));
    }

    #[test]
    fn certificate_clone() {
        let cert = Certificate::new(
            vec![1, 2, 3],
            Utc::now(),
            Utc::now(),
            "subject".into(),
            "issuer".into(),
            vec![],
        );
        let cloned = cert.clone();
        assert_eq!(cert.subject(), cloned.subject());
        assert_eq!(cert.der(), cloned.der());
    }

    #[test]
    fn certificate_request_serialization() {
        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .dns("example.com")
            .server_auth()
            .build()
            .unwrap();

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CertificateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request.subject, deserialized.subject);
        assert_eq!(request.validity_days, deserialized.validity_days);
    }

    #[test]
    fn certificate_with_empty_san() {
        let cert = Certificate::new(
            vec![1, 2, 3],
            Utc::now(),
            Utc::now(),
            "subject".into(),
            "issuer".into(),
            vec![],
        );
        assert!(cert.san().is_empty());
    }

    #[test]
    fn certificate_with_multiple_san() {
        let san = vec![
            SubjectAltName::Dns("example.com".into()),
            SubjectAltName::Ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
            SubjectAltName::Email("admin@example.com".into()),
        ];
        let cert = Certificate::new(
            vec![1, 2, 3],
            Utc::now(),
            Utc::now(),
            "subject".into(),
            "issuer".into(),
            san,
        );
        assert_eq!(cert.san().len(), 3);
    }
}
