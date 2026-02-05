//! End-to-end tests for Secrets and PKI (claw-secrets, claw-pki).
//!
//! These tests verify:
//! 1. Secret creation, retrieval, and deletion
//! 2. Access control policies
//! 3. Secret rotation
//! 4. Audit logging
//! 5. Certificate Authority operations
//! 6. Certificate issuance and validation
//! 7. Certificate rotation
//! 8. mTLS certificate workflows

use std::net::{IpAddr, Ipv4Addr};
use claw_secrets::{
    AccessPolicy, Accessor, AuditAction, AuditFilter, AuditLog, SecretId, SecretKey,
    SecretMetadata, SecretStore, SecretValue, RotationPolicy as SecretRotationPolicy,
    WorkloadId as SecretWorkloadId, NodeId as SecretNodeId,
};
use claw_pki::{
    Certificate, CertificateAuthority, CertificateRequest, CertStore,
    RotationPolicy as CertRotationPolicy, check_rotation_needed, is_expired, is_valid_now,
    rotate, rotate_all_needed, validate_certificate, validate_chain,
};

// ============================================================================
// Secret Creation and Retrieval Tests
// ============================================================================

#[test]
fn test_secret_creation_and_retrieval() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    // Create a secret
    let id = SecretId::new("database.password").unwrap();
    let value = SecretValue::new(b"super-secret-password-123".to_vec());
    let policy = AccessPolicy::allow_all();

    store.store(&id, value.clone(), policy, &key).unwrap();

    // Retrieve the secret
    let retrieved = store.get(&id, &key).unwrap();
    assert!(retrieved.is_some());

    // Value should match (after decryption)
    let (decrypted_value, _metadata) = retrieved.unwrap();
    assert_eq!(decrypted_value.as_bytes(), b"super-secret-password-123");
}

#[test]
fn test_secret_not_found() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    let id = SecretId::new("nonexistent.secret").unwrap();
    let retrieved = store.get(&id, &key).unwrap();

    assert!(retrieved.is_none());
}

#[test]
fn test_secret_deletion() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    let id = SecretId::new("deletable.secret").unwrap();
    let value = SecretValue::new(b"delete-me".to_vec());

    store.store(&id, value, AccessPolicy::allow_all(), &key).unwrap();

    // Verify it exists
    assert!(store.get(&id, &key).unwrap().is_some());

    // Delete it
    store.delete(&id).unwrap();

    // Should no longer exist
    assert!(store.get(&id, &key).unwrap().is_none());
}

#[test]
fn test_secret_list() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    // Create multiple secrets
    for i in 1..=5 {
        let id = SecretId::new(format!("app.secret.{}", i)).unwrap();
        let value = SecretValue::new(format!("value-{}", i).into_bytes());
        store.store(&id, value, AccessPolicy::allow_all(), &key).unwrap();
    }

    // List all secrets
    let secrets = store.list();
    assert_eq!(secrets.len(), 5);
}

#[test]
fn test_secret_with_different_types() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    // Store various types of secrets
    let api_key = SecretValue::new(b"api-key-abc123".to_vec());
    let cert_pem = SecretValue::new(b"-----BEGIN CERTIFICATE-----\nMIIBkDCC...".to_vec());
    let json_config = SecretValue::new(br#"{"username":"admin","password":"secret"}"#.to_vec());

    store.store(
        &SecretId::new("external.api_key").unwrap(),
        api_key,
        AccessPolicy::allow_all(),
        &key,
    ).unwrap();

    store.store(
        &SecretId::new("tls.certificate").unwrap(),
        cert_pem,
        AccessPolicy::allow_all(),
        &key,
    ).unwrap();

    store.store(
        &SecretId::new("config.json").unwrap(),
        json_config,
        AccessPolicy::allow_all(),
        &key,
    ).unwrap();

    // All should be retrievable
    assert!(store.get(&SecretId::new("external.api_key").unwrap(), &key).unwrap().is_some());
    assert!(store.get(&SecretId::new("tls.certificate").unwrap(), &key).unwrap().is_some());
    assert!(store.get(&SecretId::new("config.json").unwrap(), &key).unwrap().is_some());
}

// ============================================================================
// Access Policy Tests
// ============================================================================

#[test]
fn test_access_policy_workload_restriction() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    // Create a secret with workload-specific access
    let id = SecretId::new("restricted.secret").unwrap();
    let value = SecretValue::new(b"workload-specific".to_vec());

    let policy = AccessPolicy::allow_workloads(vec![
        SecretWorkloadId::new("api-server"),
        SecretWorkloadId::new("worker"),
    ]);

    store.store(&id, value, policy, &key).unwrap();

    // Verify the policy was stored
    let (_, metadata) = store.get(&id, &key).unwrap().unwrap();
    assert!(metadata.policy.allows_workload(&SecretWorkloadId::new("api-server")));
    assert!(metadata.policy.allows_workload(&SecretWorkloadId::new("worker")));
    assert!(!metadata.policy.allows_workload(&SecretWorkloadId::new("unauthorized")));
}

#[test]
fn test_access_policy_node_restriction() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    let id = SecretId::new("node.restricted").unwrap();
    let value = SecretValue::new(b"node-specific".to_vec());

    let policy = AccessPolicy::allow_nodes(vec![
        SecretNodeId::new("node-1"),
        SecretNodeId::new("node-2"),
    ]);

    store.store(&id, value, policy, &key).unwrap();

    let (_, metadata) = store.get(&id, &key).unwrap().unwrap();
    assert!(metadata.policy.allows_node(&SecretNodeId::new("node-1")));
    assert!(!metadata.policy.allows_node(&SecretNodeId::new("node-3")));
}

#[test]
fn test_access_policy_combined() {
    let policy = AccessPolicy::new()
        .with_workloads(vec![SecretWorkloadId::new("app")])
        .with_nodes(vec![SecretNodeId::new("trusted-node")]);

    // Must match both workload and node
    assert!(policy.allows_workload(&SecretWorkloadId::new("app")));
    assert!(policy.allows_node(&SecretNodeId::new("trusted-node")));

    // Doesn't match unauthorized
    assert!(!policy.allows_workload(&SecretWorkloadId::new("other")));
    assert!(!policy.allows_node(&SecretNodeId::new("other-node")));
}

// ============================================================================
// Secret Rotation Tests
// ============================================================================

#[test]
fn test_secret_rotation() {
    let store = SecretStore::new();
    let key = SecretKey::generate();

    let id = SecretId::new("rotating.secret").unwrap();
    let v1 = SecretValue::new(b"version-1".to_vec());

    // Store initial version
    store.store(&id, v1, AccessPolicy::allow_all(), &key).unwrap();

    // Get initial
    let (value1, meta1) = store.get(&id, &key).unwrap().unwrap();
    assert_eq!(value1.as_bytes(), b"version-1");
    let version1 = meta1.version;

    // Update (rotate) the secret
    let v2 = SecretValue::new(b"version-2".to_vec());
    store.update(&id, v2, &key).unwrap();

    // Get updated
    let (value2, meta2) = store.get(&id, &key).unwrap().unwrap();
    assert_eq!(value2.as_bytes(), b"version-2");
    assert!(meta2.version > version1);
}

#[test]
fn test_rotation_policy() {
    // Test rotation policy configuration
    let policy = SecretRotationPolicy::new()
        .with_max_age_days(30)
        .with_warning_days(7);

    assert_eq!(policy.max_age_days(), 30);
    assert_eq!(policy.warning_days(), 7);
}

// ============================================================================
// Audit Logging Tests
// ============================================================================

#[test]
fn test_audit_log_creation() {
    let log = AuditLog::new(1000); // Keep last 1000 entries

    // Record some actions
    log.record(AuditAction::Created {
        secret_id: SecretId::new("test.secret").unwrap(),
        accessor: Accessor::workload("api-server"),
    });

    log.record(AuditAction::Accessed {
        secret_id: SecretId::new("test.secret").unwrap(),
        accessor: Accessor::workload("api-server"),
    });

    log.record(AuditAction::Rotated {
        secret_id: SecretId::new("test.secret").unwrap(),
        accessor: Accessor::system(),
    });

    // Query all entries
    let entries = log.query(AuditFilter::default());
    assert_eq!(entries.len(), 3);
}

#[test]
fn test_audit_log_filtering() {
    let log = AuditLog::new(1000);

    // Create audit entries for different secrets
    for i in 1..=5 {
        log.record(AuditAction::Created {
            secret_id: SecretId::new(format!("secret.{}", i)).unwrap(),
            accessor: Accessor::workload("api-server"),
        });
    }

    log.record(AuditAction::Accessed {
        secret_id: SecretId::new("secret.1").unwrap(),
        accessor: Accessor::workload("worker"),
    });

    // Filter by secret ID
    let filter = AuditFilter::for_secret(&SecretId::new("secret.1").unwrap());
    let entries = log.query(filter);
    assert_eq!(entries.len(), 2); // Created + Accessed

    // Filter by accessor
    let filter = AuditFilter::for_accessor(&Accessor::workload("worker"));
    let entries = log.query(filter);
    assert_eq!(entries.len(), 1);
}

// ============================================================================
// PKI: Certificate Authority Tests
// ============================================================================

#[test]
fn test_ca_creation() {
    let ca = CertificateAuthority::new("Clawbernetes Test CA").unwrap();

    let root_cert = ca.root_certificate();
    assert_eq!(root_cert.subject(), "Clawbernetes Test CA");
    assert!(!is_expired(root_cert));
}

#[test]
fn test_ca_persistence() {
    // Create CA
    let ca1 = CertificateAuthority::new("Persistent CA").unwrap();
    let root_cert = ca1.root_certificate().clone();
    let root_key = ca1.root_key().clone();

    // Issue a certificate
    let request = CertificateRequest::builder("test-service")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();
    let (cert1, _) = ca1.issue(&request).unwrap();

    // Recreate CA from existing cert/key
    let ca2 = CertificateAuthority::from_existing(root_cert, root_key).unwrap();

    // Both CAs should validate the same certificates
    validate_certificate(&cert1, ca1.root_certificate()).unwrap();
    validate_certificate(&cert1, ca2.root_certificate()).unwrap();
}

// ============================================================================
// PKI: Certificate Issuance Tests
// ============================================================================

#[test]
fn test_server_certificate_issuance() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    let request = CertificateRequest::builder("gateway.clawbernetes.local")
        .dns("gateway.clawbernetes.local")
        .dns("*.gateway.clawbernetes.local")
        .ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)))
        .validity_days(90)
        .server_auth()
        .build()
        .unwrap();

    let (cert, key) = ca.issue(&request).unwrap();

    // Verify certificate properties
    assert_eq!(cert.subject(), "gateway.clawbernetes.local");
    assert_eq!(cert.issuer(), "Test CA");
    assert!(!is_expired(&cert));
    assert!(is_valid_now(&cert));
    assert!(!key.pem().is_empty());
}

#[test]
fn test_client_certificate_issuance() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    let request = CertificateRequest::builder("node-worker-1")
        .dns("node-worker-1.clawbernetes.local")
        .validity_days(30)
        .client_auth()
        .build()
        .unwrap();

    let (cert, _key) = ca.issue(&request).unwrap();

    assert_eq!(cert.subject(), "node-worker-1");
    validate_certificate(&cert, ca.root_certificate()).unwrap();
}

#[test]
fn test_mtls_certificate_issuance() {
    let ca = CertificateAuthority::new("mTLS CA").unwrap();

    // Server certificate with both usages
    let server_request = CertificateRequest::builder("server")
        .dns("server.local")
        .validity_days(30)
        .server_auth()
        .client_auth() // mTLS requires both
        .build()
        .unwrap();

    let (server_cert, _) = ca.issue(&server_request).unwrap();

    // Client certificate with both usages
    let client_request = CertificateRequest::builder("client")
        .validity_days(30)
        .client_auth()
        .server_auth() // mTLS requires both
        .build()
        .unwrap();

    let (client_cert, _) = ca.issue(&client_request).unwrap();

    // Both should be valid
    validate_certificate(&server_cert, ca.root_certificate()).unwrap();
    validate_certificate(&client_cert, ca.root_certificate()).unwrap();
}

#[test]
fn test_certificate_with_multiple_sans() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    let request = CertificateRequest::builder("multi-san-service")
        .dns("api.example.com")
        .dns("api.example.org")
        .dns("internal-api.local")
        .ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)))
        .ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)))
        .validity_days(90)
        .server_auth()
        .build()
        .unwrap();

    let (cert, _) = ca.issue(&request).unwrap();

    assert_eq!(cert.subject(), "multi-san-service");
    validate_certificate(&cert, ca.root_certificate()).unwrap();
}

// ============================================================================
// PKI: Certificate Validation Tests
// ============================================================================

#[test]
fn test_certificate_chain_validation() {
    let ca = CertificateAuthority::new("Root CA").unwrap();

    let request = CertificateRequest::builder("leaf-cert")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();

    let (leaf_cert, _) = ca.issue(&request).unwrap();

    // Validate the chain
    let chain = vec![leaf_cert.clone(), ca.root_certificate().clone()];
    validate_chain(&chain).unwrap();
}

#[test]
fn test_certificate_from_wrong_ca_fails() {
    let ca1 = CertificateAuthority::new("CA 1").unwrap();
    let ca2 = CertificateAuthority::new("CA 2").unwrap();

    // Issue cert from CA1
    let request = CertificateRequest::builder("test")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();

    let (cert, _) = ca1.issue(&request).unwrap();

    // Validation against CA2 should fail
    let result = validate_certificate(&cert, ca2.root_certificate());
    assert!(result.is_err());
}

#[test]
fn test_certificate_validity_period() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    // Issue a certificate with short validity
    let request = CertificateRequest::builder("short-lived")
        .validity_days(1)
        .server_auth()
        .build()
        .unwrap();

    let (cert, _) = ca.issue(&request).unwrap();

    // Should be valid now
    assert!(is_valid_now(&cert));
    assert!(!is_expired(&cert));
}

// ============================================================================
// PKI: Certificate Storage Tests
// ============================================================================

#[test]
fn test_certificate_store() {
    let ca = CertificateAuthority::new("Test CA").unwrap();
    let store = CertStore::new();

    // Issue and store a certificate
    let request = CertificateRequest::builder("stored-cert")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();

    let (cert, key) = ca.issue(&request).unwrap();
    let cert_id = store.store(cert.clone(), key).unwrap();

    // Retrieve the certificate
    let (retrieved_cert, _key) = store.get(&cert_id).unwrap();
    assert_eq!(retrieved_cert.subject(), "stored-cert");

    // Check if certificate exists
    assert!(store.contains(&cert_id));

    // List all certificates
    let all_certs = store.list();
    assert_eq!(all_certs.len(), 1);
}

#[test]
fn test_certificate_store_multiple() {
    let ca = CertificateAuthority::new("Test CA").unwrap();
    let store = CertStore::new();

    // Store multiple certificates
    for i in 1..=5 {
        let request = CertificateRequest::builder(format!("service-{}", i))
            .validity_days(30)
            .server_auth()
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();
        store.store(cert, key).unwrap();
    }

    assert_eq!(store.list().len(), 5);
}

#[test]
fn test_certificate_pem_export() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    let request = CertificateRequest::builder("exportable")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();

    let (cert, key) = ca.issue(&request).unwrap();

    // Export to PEM
    let cert_pem = cert.pem();
    let key_pem = key.pem();

    assert!(cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(key_pem.contains("BEGIN") && key_pem.contains("KEY"));
}

// ============================================================================
// PKI: Certificate Rotation Tests
// ============================================================================

#[test]
fn test_certificate_rotation() {
    let ca = CertificateAuthority::new("Rotation CA").unwrap();
    let store = CertStore::new();

    // Create initial certificate
    let request = CertificateRequest::builder("rotate-me")
        .validity_days(30)
        .server_auth()
        .build()
        .unwrap();

    let (cert, key) = ca.issue(&request).unwrap();
    let cert_id = store.store(cert, key).unwrap();

    // Rotate the certificate
    let (new_id, new_cert, _new_key) = rotate(&ca, &store, &cert_id).unwrap();

    // New certificate should be different
    assert_ne!(cert_id, new_id);
    assert_eq!(new_cert.subject(), "rotate-me");

    // Both certificates should be in the store
    assert!(store.contains(&cert_id));
    assert!(store.contains(&new_id));
}

#[test]
fn test_rotation_policy_check() {
    let ca = CertificateAuthority::new("Test CA").unwrap();

    // Default policy: rotate when < 30% validity remains
    let policy = CertRotationPolicy::default();

    // Fresh certificate - should NOT need rotation
    let request = CertificateRequest::builder("fresh-cert")
        .validity_days(90)
        .server_auth()
        .build()
        .unwrap();

    let (fresh_cert, _) = ca.issue(&request).unwrap();
    assert!(!check_rotation_needed(&fresh_cert, &policy));
}

#[test]
fn test_bulk_rotation() {
    let ca = CertificateAuthority::new("Bulk CA").unwrap();
    let store = CertStore::new();

    // Create multiple certificates
    for i in 1..=3 {
        let request = CertificateRequest::builder(format!("bulk-{}", i))
            .validity_days(90)
            .server_auth()
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();
        store.store(cert, key).unwrap();
    }

    // Rotate all that need it
    let policy = CertRotationPolicy::default();
    let rotated = rotate_all_needed(&ca, &store, &policy).unwrap();

    // Fresh certs should not need rotation
    assert_eq!(rotated.len(), 0);
}

// ============================================================================
// Integration: Secrets + PKI Workflow
// ============================================================================

#[test]
fn test_certificate_as_secret() {
    // Generate a certificate
    let ca = CertificateAuthority::new("Test CA").unwrap();
    let request = CertificateRequest::builder("api-gateway")
        .dns("api.example.com")
        .validity_days(90)
        .server_auth()
        .build()
        .unwrap();

    let (cert, key) = ca.issue(&request).unwrap();

    // Store certificate and key as secrets
    let secret_store = SecretStore::new();
    let encryption_key = SecretKey::generate();

    // Store certificate
    secret_store.store(
        &SecretId::new("tls.api-gateway.crt").unwrap(),
        SecretValue::new(cert.pem().into_bytes()),
        AccessPolicy::allow_workloads(vec![SecretWorkloadId::new("api-gateway")]),
        &encryption_key,
    ).unwrap();

    // Store private key
    secret_store.store(
        &SecretId::new("tls.api-gateway.key").unwrap(),
        SecretValue::new(key.pem().into_bytes()),
        AccessPolicy::allow_workloads(vec![SecretWorkloadId::new("api-gateway")]),
        &encryption_key,
    ).unwrap();

    // Retrieve and verify
    let (cert_secret, _) = secret_store
        .get(&SecretId::new("tls.api-gateway.crt").unwrap(), &encryption_key)
        .unwrap()
        .unwrap();

    let cert_pem = String::from_utf8(cert_secret.as_bytes().to_vec()).unwrap();
    assert!(cert_pem.contains("BEGIN CERTIFICATE"));
}

#[test]
fn test_full_pki_workflow() {
    // 1. Create CA
    let ca = CertificateAuthority::new("Clawbernetes Root CA").unwrap();
    let root_cert = ca.root_certificate();
    assert!(!is_expired(root_cert));

    // 2. Create cert store
    let store = CertStore::new();

    // 3. Issue certificates for different services
    let services = ["api-gateway", "scheduler", "controller", "node-agent"];

    for service in services {
        let request = CertificateRequest::builder(service)
            .dns(format!("{}.clawbernetes.local", service))
            .validity_days(90)
            .server_auth()
            .client_auth() // For mTLS
            .build()
            .unwrap();

        let (cert, key) = ca.issue(&request).unwrap();

        // Validate each certificate
        validate_certificate(&cert, ca.root_certificate()).unwrap();

        // Store it
        store.store(cert, key).unwrap();
    }

    assert_eq!(store.list().len(), 4);

    // 4. Verify chain for each
    for (cert_id, cert) in store.list() {
        let chain = vec![cert.clone(), ca.root_certificate().clone()];
        validate_chain(&chain).unwrap();
    }

    // 5. Check rotation status
    let policy = CertRotationPolicy::default();
    for (_, cert) in store.list() {
        // Fresh certs should not need rotation
        assert!(!check_rotation_needed(&cert, &policy));
    }
}
