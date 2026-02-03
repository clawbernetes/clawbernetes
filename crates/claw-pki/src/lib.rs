//! PKI (Public Key Infrastructure) for Clawbernetes.
#![forbid(unsafe_code)]
//!
//! This crate provides certificate authority functionality for agent-managed
//! certificate issuance, mTLS support, and automatic certificate rotation.
//!
//! # Overview
//!
//! The `claw-pki` crate enables:
//! - Creating and managing a Certificate Authority (CA)
//! - Issuing certificates for workloads (server auth, client auth, mTLS)
//! - Automatic certificate rotation before expiry
//! - Certificate validation and chain verification
//!
//! # Example
//!
//! ```
//! use claw_pki::{CertificateAuthority, CertificateRequest, CertStore};
//!
//! // Create a new CA
//! let ca = CertificateAuthority::new("Clawbernetes Root CA").unwrap();
//!
//! // Create a certificate store
//! let store = CertStore::new();
//!
//! // Issue a certificate for a node
//! let request = CertificateRequest::builder("node-1.clawbernetes.local")
//!     .dns("node-1.clawbernetes.local")
//!     .validity_days(90)
//!     .server_auth()
//!     .client_auth()  // For mTLS
//!     .build()
//!     .unwrap();
//!
//! let (cert, key) = ca.issue(&request).unwrap();
//!
//! // Store the certificate
//! let cert_id = store.store(cert, key).unwrap();
//! ```
//!
//! # Modules
//!
//! - [`ca`] - Certificate Authority implementation
//! - [`store`] - Certificate storage
//! - [`rotation`] - Certificate rotation policy and automation
//! - [`validation`] - Certificate validation utilities
//! - [`types`] - Core types (Certificate, `PrivateKey`, etc.)
//! - [`error`] - Error types

pub mod ca;
pub mod error;
pub mod rotation;
pub mod store;
pub mod types;
pub mod validation;

// Re-export commonly used types at crate root
pub use ca::CertificateAuthority;
pub use error::{Error, Result};
pub use rotation::{check_rotation_needed, rotate, rotate_all_needed, RotationPolicy};
pub use store::CertStore;
pub use types::{
    Certificate, CertificateId, CertificateRequest, CertificateRequestBuilder, KeyUsage,
    PrivateKey, SubjectAltName,
};
pub use validation::{is_expired, is_not_yet_valid, is_valid_now, validate_certificate, validate_chain, remaining_validity};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn full_workflow_test() {
        // 1. Create CA
        let ca = CertificateAuthority::new("Clawbernetes Root CA").unwrap();
        assert_eq!(ca.root_certificate().subject(), "Clawbernetes Root CA");

        // 2. Create store
        let store = CertStore::new();

        // 3. Issue a server certificate
        let server_request = CertificateRequest::builder("gateway.clawbernetes.local")
            .dns("gateway.clawbernetes.local")
            .dns("*.gateway.clawbernetes.local")
            .ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)))
            .validity_days(90)
            .server_auth()
            .build()
            .unwrap();

        let (server_cert, server_key) = ca.issue(&server_request).unwrap();

        // Verify the certificate
        assert_eq!(server_cert.subject(), "gateway.clawbernetes.local");
        assert_eq!(server_cert.issuer(), "Clawbernetes Root CA");
        assert!(!is_expired(&server_cert));

        // Store it
        let server_id = store.store(server_cert.clone(), server_key).unwrap();

        // 4. Issue a client certificate (for mTLS)
        let client_request = CertificateRequest::builder("node-1")
            .dns("node-1.clawbernetes.local")
            .validity_days(90)
            .client_auth()
            .build()
            .unwrap();

        let (client_cert, client_key) = ca.issue(&client_request).unwrap();
        let _client_id = store.store(client_cert.clone(), client_key).unwrap();

        // 5. Validate certificates
        validate_certificate(&server_cert, ca.root_certificate()).unwrap();
        validate_certificate(&client_cert, ca.root_certificate()).unwrap();

        // 6. Verify chain
        let chain = vec![server_cert.clone(), ca.root_certificate().clone()];
        validate_chain(&chain).unwrap();

        // 7. Check rotation (should not be needed for fresh certs)
        let policy = RotationPolicy::default();
        assert!(!check_rotation_needed(&server_cert, &policy));

        // 8. Retrieve from store
        let (retrieved_cert, _) = store.get(&server_id).unwrap();
        assert_eq!(retrieved_cert.subject(), "gateway.clawbernetes.local");

        // 9. PEM export
        let pem = retrieved_cert.pem();
        assert!(pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn mtls_workflow() {
        let ca = CertificateAuthority::new("mTLS CA").unwrap();

        // Server certificate
        let server_request = CertificateRequest::builder("server")
            .dns("server.local")
            .validity_days(30)
            .server_auth()
            .client_auth()  // mTLS requires both
            .build()
            .unwrap();

        let (server_cert, _) = ca.issue(&server_request).unwrap();

        // Client certificate
        let client_request = CertificateRequest::builder("client")
            .validity_days(30)
            .client_auth()
            .server_auth()  // mTLS requires both
            .build()
            .unwrap();

        let (client_cert, _) = ca.issue(&client_request).unwrap();

        // Both should be valid
        validate_certificate(&server_cert, ca.root_certificate()).unwrap();
        validate_certificate(&client_cert, ca.root_certificate()).unwrap();
    }

    #[test]
    fn certificate_rotation_workflow() {
        let ca = CertificateAuthority::new("Rotation CA").unwrap();
        let store = CertStore::new();

        // Create a certificate
        let request = CertificateRequest::builder("rotate-me")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();
        let cert_id = store.store(cert, key).unwrap();

        // Rotate it (even though it doesn't need rotation yet)
        let (new_id, new_cert, _) = rotate(&ca, &store, &cert_id).unwrap();

        assert_ne!(cert_id, new_id);
        assert_eq!(new_cert.subject(), "rotate-me");

        // Both certificates should be in the store
        assert!(store.contains(&cert_id));
        assert!(store.contains(&new_id));
    }

    #[test]
    fn ca_persistence() {
        // Create CA and get its cert/key
        let ca1 = CertificateAuthority::new("Persistent CA").unwrap();
        let root_cert = ca1.root_certificate().clone();
        let root_key = ca1.root_key().clone();

        // Issue a certificate
        let request = CertificateRequest::builder("test")
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();
        let (cert1, _) = ca1.issue(&request).unwrap();

        // Recreate CA from existing cert/key
        let ca2 = CertificateAuthority::from_existing(root_cert, root_key).unwrap();

        // Issue another certificate
        let (cert2, _) = ca2.issue(&request).unwrap();

        // Both certificates should validate against the same CA
        validate_certificate(&cert1, ca2.root_certificate()).unwrap();
        validate_certificate(&cert2, ca2.root_certificate()).unwrap();
    }
}
