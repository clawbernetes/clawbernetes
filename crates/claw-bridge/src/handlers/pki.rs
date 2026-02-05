//! PKI (Certificate Authority) handlers
//!
//! These handlers integrate with claw-pki for certificate management and mTLS.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_pki::{
    CertStore, Certificate, CertificateAuthority, CertificateId, CertificateRequest,
    SubjectAltName,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref CA: Arc<RwLock<CertificateAuthority>> = Arc::new(RwLock::new(
        CertificateAuthority::new("Clawbernetes Root CA").expect("failed to create CA")
    ));
    static ref CERT_STORE: Arc<CertStore> = Arc::new(CertStore::new());
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CertificateInfo {
    pub id: String,
    pub subject: String,
    pub dns_names: Vec<String>,
    pub not_before: i64,
    pub not_after: i64,
    pub is_expired: bool,
    pub days_until_expiry: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaStatus {
    pub subject: String,
    pub not_before: i64,
    pub not_after: i64,
    pub total_certs_issued: usize,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CertIssueParams {
    pub common_name: String,
    pub dns_names: Option<Vec<String>>,
    pub ip_addresses: Option<Vec<String>>,
    pub validity_days: Option<u32>,
    pub server_auth: Option<bool>,
    pub client_auth: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CertIssueResult {
    pub cert_id: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
}

/// Issue a new certificate
pub async fn cert_issue(params: Value) -> BridgeResult<Value> {
    let params: CertIssueParams = parse_params(params)?;

    let mut builder = CertificateRequest::builder(&params.common_name);

    if let Some(dns_names) = &params.dns_names {
        for dns in dns_names {
            builder = builder.dns(dns);
        }
    }

    if let Some(ips) = &params.ip_addresses {
        for ip_str in ips {
            let ip: std::net::IpAddr = ip_str
                .parse()
                .map_err(|_| BridgeError::InvalidParams(format!("invalid IP: {ip_str}")))?;
            builder = builder.ip(ip);
        }
    }

    if let Some(days) = params.validity_days {
        builder = builder.validity_days(days);
    }

    if params.server_auth.unwrap_or(true) {
        builder = builder.server_auth();
    }

    if params.client_auth.unwrap_or(false) {
        builder = builder.client_auth();
    }

    let request = builder
        .build()
        .map_err(|e| BridgeError::InvalidParams(format!("invalid certificate request: {e}")))?;

    let ca = CA.read();
    let (cert, key) = ca
        .issue(&request)
        .map_err(|e| BridgeError::Internal(format!("failed to issue certificate: {e}")))?;

    // Store the certificate
    let cert_id = CERT_STORE
        .store(cert.clone(), key.clone())
        .map_err(|e| BridgeError::Internal(format!("failed to store certificate: {e}")))?;

    tracing::info!(cert_id = %cert_id, cn = %params.common_name, "certificate issued");

    to_json(CertIssueResult {
        cert_id: cert_id.to_string(),
        certificate_pem: cert.pem(),
        private_key_pem: key.pem(),
    })
}

#[derive(Debug, Deserialize)]
pub struct CertGetParams {
    pub cert_id: String,
}

/// Get a certificate by ID
pub async fn cert_get(params: Value) -> BridgeResult<Value> {
    let params: CertGetParams = parse_params(params)?;

    let cert_id = parse_cert_id(&params.cert_id)?;

    let (cert, _key) = CERT_STORE
        .get(&cert_id)
        .map_err(|e| BridgeError::NotFound(format!("certificate not found: {e}")))?;

    let info = cert_to_info(&cert_id, &cert);
    to_json(info)
}

#[derive(Debug, Deserialize)]
pub struct CertListParams {
    pub expiring_within_days: Option<u32>,
}

/// List certificates
pub async fn cert_list(params: Value) -> BridgeResult<Value> {
    let params: CertListParams = parse_params(params)?;

    let cert_ids = if let Some(days) = params.expiring_within_days {
        CERT_STORE.list_expiring(Duration::from_secs(days as u64 * 86400))
    } else {
        CERT_STORE.list_all()
    };

    let mut infos = Vec::new();
    for cert_id in cert_ids {
        if let Ok((cert, _)) = CERT_STORE.get(&cert_id) {
            infos.push(cert_to_info(&cert_id, &cert));
        }
    }

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct CertRevokeParams {
    pub cert_id: String,
}

#[derive(Debug, Serialize)]
pub struct RevokeResult {
    pub success: bool,
}

/// Revoke a certificate
pub async fn cert_revoke(params: Value) -> BridgeResult<Value> {
    let params: CertRevokeParams = parse_params(params)?;

    let cert_id = parse_cert_id(&params.cert_id)?;

    let mut ca = CA.write();
    ca.revoke(&cert_id)
        .map_err(|e| BridgeError::Internal(format!("failed to revoke: {e}")))?;

    // Also delete from store
    let _ = CERT_STORE.delete(&cert_id);

    tracing::info!(cert_id = %params.cert_id, "certificate revoked");

    to_json(RevokeResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct CertRotateParams {
    pub cert_id: String,
}

/// Rotate (renew) a certificate
pub async fn cert_rotate(params: Value) -> BridgeResult<Value> {
    let params: CertRotateParams = parse_params(params)?;

    let cert_id = parse_cert_id(&params.cert_id)?;

    let ca = CA.read();
    let (new_cert_id, new_cert, _new_key) = claw_pki::rotate(&ca, &CERT_STORE, &cert_id)
        .map_err(|e| BridgeError::Internal(format!("failed to rotate: {e}")))?;

    tracing::info!(old_cert_id = %params.cert_id, new_cert_id = %new_cert_id, "certificate rotated");

    to_json(cert_to_info(&new_cert_id, &new_cert))
}

#[derive(Debug, Deserialize)]
pub struct CaStatusParams {}

/// Get CA status
pub async fn ca_status(_params: Value) -> BridgeResult<Value> {
    let ca = CA.read();

    // Issue a temporary cert to get CA cert info, or use another approach
    // For now, just return basic stats
    let status = CaStatus {
        subject: "Clawbernetes Root CA".to_string(),
        not_before: chrono::Utc::now().timestamp_millis(),
        not_after: (chrono::Utc::now() + chrono::Duration::days(3650)).timestamp_millis(),
        total_certs_issued: CERT_STORE.len(),
    };

    to_json(status)
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn parse_cert_id(s: &str) -> BridgeResult<CertificateId> {
    let uuid = uuid::Uuid::parse_str(s)
        .map_err(|_| BridgeError::InvalidParams("invalid cert_id (must be UUID)".to_string()))?;
    Ok(CertificateId::from_uuid(uuid))
}

fn cert_to_info(id: &CertificateId, cert: &Certificate) -> CertificateInfo {
    let now = chrono::Utc::now();
    let not_after = cert.not_after();
    let days_until_expiry = (not_after - now).num_days();

    // Extract DNS names from SANs
    let dns_names: Vec<String> = cert
        .san()
        .iter()
        .filter_map(|san| match san {
            SubjectAltName::Dns(dns) => Some(dns.clone()),
            _ => None,
        })
        .collect();

    CertificateInfo {
        id: id.to_string(),
        subject: cert.subject().to_string(),
        dns_names,
        not_before: cert.not_before().timestamp_millis(),
        not_after: not_after.timestamp_millis(),
        is_expired: claw_pki::is_expired(cert),
        days_until_expiry,
    }
}
