//! Secrets management handlers
//!
//! These handlers integrate with claw-secrets for encrypted secrets storage.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_secrets::{AccessPolicy, Accessor, SecretId, SecretKey, SecretStore, WorkloadId};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref SECRET_STORE: Arc<SecretStore> = Arc::new(SecretStore::new(SecretKey::generate()));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SecretInfo {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: u64,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SecretPutParams {
    pub id: String,
    pub value: String,
    pub allowed_workloads: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SecretPutResult {
    pub success: bool,
    pub id: String,
}

/// Store a secret
pub async fn secret_put(params: Value) -> BridgeResult<Value> {
    let params: SecretPutParams = parse_params(params)?;

    let secret_id = SecretId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid secret id: {e}")))?;

    let policy = if let Some(workloads) = &params.allowed_workloads {
        let wids: Vec<WorkloadId> = workloads.iter().map(|w| WorkloadId::new(w)).collect();
        AccessPolicy::allow_workloads(wids)
    } else {
        AccessPolicy::new() // Empty policy = no restrictions
    };

    SECRET_STORE
        .put(&secret_id, params.value.as_bytes(), policy)
        .map_err(|e| BridgeError::Internal(format!("failed to store secret: {e}")))?;

    tracing::info!(secret_id = %params.id, "secret stored");

    to_json(SecretPutResult {
        success: true,
        id: params.id,
    })
}

#[derive(Debug, Deserialize)]
pub struct SecretGetParams {
    pub id: String,
    pub workload_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SecretGetResult {
    pub id: String,
    pub value: String,
}

/// Get a secret value
pub async fn secret_get(params: Value) -> BridgeResult<Value> {
    let params: SecretGetParams = parse_params(params)?;

    let secret_id = SecretId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid secret id: {e}")))?;

    let reason = params.reason.as_deref().unwrap_or("bridge access");

    let value = if let Some(wid_str) = &params.workload_id {
        let workload_id = WorkloadId::new(wid_str);
        SECRET_STORE
            .get_for_workload(&secret_id, &workload_id, reason)
            .map_err(|e| BridgeError::NotFound(format!("secret not found or access denied: {e}")))?
    } else {
        SECRET_STORE
            .get(&secret_id, &Accessor::System, reason)
            .map_err(|e| BridgeError::NotFound(format!("secret not found: {e}")))?
    };

    let value_str = String::from_utf8(value.into_bytes())
        .map_err(|_| BridgeError::Internal("secret is not valid UTF-8".to_string()))?;

    to_json(SecretGetResult {
        id: params.id,
        value: value_str,
    })
}

#[derive(Debug, Deserialize)]
pub struct SecretDeleteParams {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
}

/// Delete a secret
pub async fn secret_delete(params: Value) -> BridgeResult<Value> {
    let params: SecretDeleteParams = parse_params(params)?;

    let secret_id = SecretId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid secret id: {e}")))?;

    SECRET_STORE
        .delete(&secret_id)
        .map_err(|e| BridgeError::NotFound(format!("secret not found: {e}")))?;

    tracing::info!(secret_id = %params.id, "secret deleted");

    to_json(DeleteResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct SecretListParams {}

/// List all secret IDs
pub async fn secret_list(_params: Value) -> BridgeResult<Value> {
    let ids: Vec<String> = SECRET_STORE.list().iter().map(|id| id.to_string()).collect();

    to_json(ids)
}

#[derive(Debug, Deserialize)]
pub struct SecretRotateParams {
    pub id: String,
    pub new_value: String,
}

/// Rotate a secret to a new value
pub async fn secret_rotate(params: Value) -> BridgeResult<Value> {
    let params: SecretRotateParams = parse_params(params)?;

    let secret_id = SecretId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid secret id: {e}")))?;

    SECRET_STORE
        .rotate(&secret_id, params.new_value.as_bytes())
        .map_err(|e| BridgeError::Internal(format!("failed to rotate secret: {e}")))?;

    tracing::info!(secret_id = %params.id, "secret rotated");

    to_json(DeleteResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct SecretMetadataParams {
    pub id: String,
}

/// Get secret metadata (without the value)
pub async fn secret_metadata(params: Value) -> BridgeResult<Value> {
    let params: SecretMetadataParams = parse_params(params)?;

    let secret_id = SecretId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid secret id: {e}")))?;

    let metadata = SECRET_STORE
        .metadata(&secret_id)
        .map_err(|e| BridgeError::NotFound(format!("secret not found: {e}")))?;

    let info = SecretInfo {
        id: params.id,
        created_at: metadata.created_at.timestamp_millis(),
        updated_at: metadata.updated_at.timestamp_millis(),
        version: metadata.version,
    };

    to_json(info)
}
