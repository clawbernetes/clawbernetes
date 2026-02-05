//! End-to-end tests for Secrets and PKI (claw-secrets, claw-pki).
//!
//! These tests verify:
//! 1. Secret creation, retrieval, and deletion
//! 2. Access control policies
//! 3. Audit logging
//! 4. Certificate Authority operations
//! 5. Certificate issuance and validation

use claw_pki::{
    CertificateAuthority, CertificateRequest, CertStore, 
    validate_certificate, validate_chain, is_valid_now,
};
use claw_secrets::{
    AccessPolicy, Accessor, SecretId, SecretKey, SecretStore, WorkloadId,
};

// ============================================================================
// Secret Creation and Retrieval Tests
// ============================================================================

#[test]
fn test_secret_creation_and_retrieval() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    // Create a secret
    let id = SecretId::new("database.password").expect("valid id");
    let value = b"super-secret-password-123";
    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("api-server")]);

    store.put(&id, value, policy).expect("store secret");

    // Retrieve the secret
    let accessor = Accessor::Workload(WorkloadId::new("api-server"));
    let decrypted_value = store.get(&id, &accessor, "testing retrieval").expect("get secret");

    // Value should match (after decryption)
    assert_eq!(decrypted_value.as_bytes(), b"super-secret-password-123");
}

#[test]
fn test_secret_access_denied() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    // Create a secret only accessible by specific workload
    let id = SecretId::new("api.key").expect("valid id");
    let value = b"secret-api-key";
    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("authorized-service")]);

    store.put(&id, value, policy).expect("store secret");

    // Try to access with unauthorized workload
    let unauthorized = Accessor::Workload(WorkloadId::new("unauthorized-service"));
    let result = store.get(&id, &unauthorized, "unauthorized access attempt");
    assert!(result.is_err(), "should deny unauthorized access");
}

#[test]
fn test_secret_not_found() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    let id = SecretId::new("nonexistent.secret").expect("valid id");
    let accessor = Accessor::System;
    let result = store.get(&id, &accessor, "test");
    
    assert!(result.is_err(), "should return error for non-existent secret");
}

#[test]
fn test_secret_update() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    let id = SecretId::new("config.value").expect("valid id");
    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("config-reader")]);

    // Store initial value
    store.put(&id, b"initial-value", policy.clone()).expect("store");

    // Update the value
    store.put(&id, b"updated-value", policy).expect("update");

    // Retrieve and verify it's the new value
    let accessor = Accessor::Workload(WorkloadId::new("config-reader"));
    let value = store.get(&id, &accessor, "read updated").expect("get");
    assert_eq!(value.as_bytes(), b"updated-value");
}

#[test]
fn test_multiple_secrets() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("multi-reader")]);
    let accessor = Accessor::Workload(WorkloadId::new("multi-reader"));

    // Store multiple secrets
    for i in 0..5 {
        let id = SecretId::new(&format!("secret.{}", i)).expect("valid id");
        let value = format!("value-{}", i);
        store.put(&id, value.as_bytes(), policy.clone()).expect("store");
    }

    // Verify all can be retrieved
    for i in 0..5 {
        let id = SecretId::new(&format!("secret.{}", i)).expect("valid id");
        let value = store.get(&id, &accessor, "batch read").expect("get");
        assert_eq!(value.as_bytes(), format!("value-{}", i).as_bytes());
    }
}

#[test]
fn test_audit_log_records_access() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    let id = SecretId::new("audited.secret").expect("valid id");
    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("audit-test")]);

    // Store and access a secret
    store.put(&id, b"audit-me", policy).expect("store");

    let accessor = Accessor::Workload(WorkloadId::new("audit-test"));
    let _ = store.get(&id, &accessor, "audit test access");

    // Audit log should exist (we can at least verify it doesn't panic)
    let _audit_log = store.access_controller().audit_log();
}

// ============================================================================
// PKI / Certificate Authority Tests
// ============================================================================

#[test]
fn test_ca_creation() {
    let ca = CertificateAuthority::new("Test CA").expect("create CA");
    
    let root_cert = ca.root_certificate();
    // Root cert subject should match
    assert_eq!(root_cert.subject(), "Test CA");
    // Root cert should be self-signed (issuer = subject)
    assert_eq!(root_cert.issuer(), "Test CA");
}

#[test]
fn test_certificate_issuance() {
    let ca = CertificateAuthority::new("Test CA").expect("create CA");

    let request = CertificateRequest::builder("test-service")
        .dns("test-service.local")
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");

    let (cert, _key) = ca.issue(&request).expect("issue certificate");
    
    // Issued cert should have correct subject
    assert_eq!(cert.subject(), "test-service");
    // Issued cert should have CA as issuer
    assert_eq!(cert.issuer(), "Test CA");
}

#[test]
fn test_certificate_validation() {
    let ca = CertificateAuthority::new("Test CA").expect("create CA");

    let request = CertificateRequest::builder("valid-service")
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");

    let (cert, _key) = ca.issue(&request).expect("issue certificate");

    // Validate the certificate
    let result = validate_certificate(&cert, ca.root_certificate());
    assert!(result.is_ok(), "certificate should be valid");
    
    // Also check is_valid_now
    assert!(is_valid_now(&cert), "certificate should be valid now");
}

#[test]
fn test_certificate_chain_validation() {
    let ca = CertificateAuthority::new("Root CA").expect("create CA");

    let request = CertificateRequest::builder("leaf-service")
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");

    let (cert, _key) = ca.issue(&request).expect("issue certificate");

    // Build and validate chain
    let chain = vec![cert.clone(), ca.root_certificate().clone()];
    let result = validate_chain(&chain);
    assert!(result.is_ok(), "certificate chain should be valid");
}

#[test]
fn test_cert_store_operations() {
    let ca = CertificateAuthority::new("Store CA").expect("create CA");

    let store = CertStore::new();

    // Issue and store multiple certificates
    for i in 0..3 {
        let request = CertificateRequest::builder(&format!("service-{}", i))
            .validity_days(30)
            .server_auth()
            .build()
            .expect("build request");
        let (cert, key) = ca.issue(&request).expect("issue");
        store.store(cert, key).expect("store cert");
    }

    // Verify count
    let all_certs = store.list_all();
    assert_eq!(all_certs.len(), 3);
}

#[test]
fn test_cert_store_lookup() {
    let ca = CertificateAuthority::new("Lookup CA").expect("create CA");

    let store = CertStore::new();

    let request = CertificateRequest::builder("lookup-service")
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");
    let (cert, key) = ca.issue(&request).expect("issue");
    
    // Store returns the ID
    let cert_id = store.store(cert, key).expect("store cert");

    // Lookup by ID - get returns Result, not Option
    let found = store.get(&cert_id);
    assert!(found.is_ok(), "should find certificate by ID");
    
    let (retrieved_cert, _key) = found.unwrap();
    assert_eq!(retrieved_cert.subject(), "lookup-service");
}

#[test]
fn test_mtls_certificate_workflow() {
    let ca = CertificateAuthority::new("mTLS CA").expect("create CA");

    // Issue server certificate with both server and client auth
    let server_req = CertificateRequest::builder("server")
        .dns("server.example.com")
        .validity_days(30)
        .server_auth()
        .client_auth()
        .build()
        .expect("build server request");
    let (server_cert, _server_key) = ca.issue(&server_req).expect("issue server cert");

    // Issue client certificate
    let client_req = CertificateRequest::builder("client")
        .dns("client.example.com")
        .validity_days(30)
        .client_auth()
        .build()
        .expect("build client request");
    let (client_cert, _client_key) = ca.issue(&client_req).expect("issue client cert");

    // Both should be valid
    assert!(validate_certificate(&server_cert, ca.root_certificate()).is_ok());
    assert!(validate_certificate(&client_cert, ca.root_certificate()).is_ok());
}

// ============================================================================
// Combined Secrets + PKI Tests
// ============================================================================

#[test]
fn test_ca_private_key_stored_as_secret() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    let _ca = CertificateAuthority::new("Secure CA").expect("create CA");

    // Store CA private key as a secret (simulated)
    let key_id = SecretId::new("ca.private_key").expect("valid id");
    let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("cert-manager")]);
    
    // Simulated private key data
    let fake_private_key = b"-----BEGIN PRIVATE KEY-----\nfake-key-data\n-----END PRIVATE KEY-----";
    store.put(&key_id, fake_private_key, policy).expect("store CA key");

    // Verify it can be retrieved by authorized service
    let accessor = Accessor::Workload(WorkloadId::new("cert-manager"));
    let retrieved = store.get(&key_id, &accessor, "CA key rotation").expect("get");
    assert!(retrieved.as_bytes().starts_with(b"-----BEGIN PRIVATE KEY-----"));
}

#[test]
fn test_workload_certificate_workflow() {
    // This test simulates the full workflow:
    // 1. CA is created
    // 2. Workload requests a certificate
    // 3. Certificate is issued and stored in cert store
    // 4. Workload can retrieve its certificate

    let ca = CertificateAuthority::new("Workload CA").expect("create CA");
    let cert_store = CertStore::new();

    // Request certificate for a workload
    let request = CertificateRequest::builder("ml-training-job")
        .dns("ml-training.workloads.local")
        .validity_days(7)  // Short-lived for workloads
        .server_auth()
        .client_auth()  // mTLS
        .build()
        .expect("build request");

    // Issue certificate
    let (cert, key) = ca.issue(&request).expect("issue cert");

    // Store in cert store
    let cert_id = cert_store.store(cert.clone(), key).expect("store cert");

    // Verify certificate is valid
    assert!(validate_certificate(&cert, ca.root_certificate()).is_ok());

    // Verify we can retrieve it
    let (retrieved, _) = cert_store.get(&cert_id).expect("get cert");
    assert_eq!(retrieved.subject(), "ml-training-job");
}

#[test]
fn test_node_identity_workflow() {
    // Test for node identity certificates
    let ca = CertificateAuthority::new("Node CA").expect("create CA");
    let cert_store = CertStore::new();

    // Issue certificates for multiple nodes
    let node_names = vec!["node-1", "node-2", "node-3"];
    let mut cert_ids = Vec::new();

    for node in &node_names {
        let request = CertificateRequest::builder(*node)
            .dns(&format!("{}.cluster.local", node))
            .validity_days(365)
            .server_auth()
            .client_auth()
            .build()
            .expect("build request");

        let (cert, key) = ca.issue(&request).expect("issue");
        let id = cert_store.store(cert, key).expect("store");
        cert_ids.push(id);
    }

    // All nodes should have valid certificates
    for (i, id) in cert_ids.iter().enumerate() {
        let (cert, _) = cert_store.get(id).expect("get");
        assert_eq!(cert.subject(), node_names[i]);
        assert!(validate_certificate(&cert, ca.root_certificate()).is_ok());
    }
}

#[test]
fn test_secret_store_with_system_access() {
    let master_key = SecretKey::generate();
    let store = SecretStore::new(master_key);

    // Create a secret (System accessor always has access regardless of policy)
    let id = SecretId::new("system.config").expect("valid id");
    // Empty policy - System accessor bypasses policy checks
    let policy = AccessPolicy::new();

    store.put(&id, b"system-level-secret", policy).expect("store");

    // System accessor should have access
    let accessor = Accessor::System;
    let value = store.get(&id, &accessor, "system read").expect("get");
    assert_eq!(value.as_bytes(), b"system-level-secret");
}

#[test]
fn test_certificate_pem_export() {
    let ca = CertificateAuthority::new("Export CA").expect("create CA");

    let request = CertificateRequest::builder("export-test")
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");

    let (cert, _key) = ca.issue(&request).expect("issue");

    // Export to PEM
    let pem = cert.pem();
    
    // Verify PEM format
    assert!(pem.contains("-----BEGIN CERTIFICATE-----"));
    assert!(pem.contains("-----END CERTIFICATE-----"));
}

#[test]
fn test_certificate_dns_san() {
    let ca = CertificateAuthority::new("SAN CA").expect("create CA");

    // Create certificate with multiple DNS SANs
    let request = CertificateRequest::builder("multi-san")
        .dns("service.local")
        .dns("service.example.com")
        .dns("*.service.local")  // Wildcard
        .validity_days(30)
        .server_auth()
        .build()
        .expect("build request");

    let (cert, _key) = ca.issue(&request).expect("issue");

    // Verify SANs are present
    let sans = cert.san();
    assert!(!sans.is_empty(), "should have SANs");
}

#[test]
fn test_full_secrets_and_pki_integration() {
    // Complete integration test combining secrets and PKI

    // 1. Create secrets store for sensitive data
    let master_key = SecretKey::generate();
    let secrets = SecretStore::new(master_key);

    // 2. Create CA
    let ca = CertificateAuthority::new("Integration CA").expect("create CA");
    let certs = CertStore::new();

    // 3. Store CA root key as a secret
    let ca_key_id = SecretId::new("pki.ca.root.key").expect("valid id");
    let ca_key_policy = AccessPolicy::allow_workloads(vec![WorkloadId::new("pki-controller")]);
    secrets.put(&ca_key_id, b"[simulated CA key bytes]", ca_key_policy).expect("store CA key");

    // 4. Issue certificates for services
    let services = vec![
        ("api-gateway", vec!["api.example.com", "gateway.example.com"]),
        ("auth-service", vec!["auth.example.com"]),
        ("data-service", vec!["data.example.com", "*.data.example.com"]),
    ];

    for (service, domains) in services {
        let mut builder = CertificateRequest::builder(service);
        for domain in domains {
            builder = builder.dns(domain);
        }
        let request = builder
            .validity_days(90)
            .server_auth()
            .client_auth()
            .build()
            .expect("build request");

        let (cert, key) = ca.issue(&request).expect("issue");
        
        // Store cert
        let cert_id = certs.store(cert.clone(), key).expect("store");

        // Verify certificate
        assert!(validate_certificate(&cert, ca.root_certificate()).is_ok());
        assert!(certs.get(&cert_id).is_ok());
    }

    // 5. Store service credentials as secrets
    for service in ["api-gateway", "auth-service", "data-service"] {
        let db_secret_id = SecretId::new(&format!("{}.db.password", service)).expect("valid id");
        let policy = AccessPolicy::allow_workloads(vec![WorkloadId::new(service)]);
        secrets.put(&db_secret_id, format!("db-pass-for-{}", service).as_bytes(), policy).expect("store");
    }

    // 6. Verify service can access its own secrets
    let accessor = Accessor::Workload(WorkloadId::new("api-gateway"));
    let secret_id = SecretId::new("api-gateway.db.password").expect("valid id");
    let db_pass = secrets.get(&secret_id, &accessor, "database connection").expect("get");
    assert_eq!(db_pass.as_bytes(), b"db-pass-for-api-gateway");

    // 7. Verify service cannot access another service's secrets
    let wrong_accessor = Accessor::Workload(WorkloadId::new("api-gateway"));
    let other_secret_id = SecretId::new("auth-service.db.password").expect("valid id");
    let result = secrets.get(&other_secret_id, &wrong_accessor, "unauthorized");
    assert!(result.is_err(), "should not access other service's secrets");

    // Verify final state
    assert_eq!(certs.list_all().len(), 3);
}
