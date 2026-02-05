//! Storage handlers
//!
//! These handlers integrate with claw-storage for volume management.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_storage::{
    AccessMode, EmptyDirConfig, ReclaimPolicy, StorageClass, Volume, VolumeClaim, VolumeId,
    VolumeManager, VolumeStatus, VolumeType,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref VOLUME_MANAGER: VolumeManager = VolumeManager::new();
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct VolumeInfo {
    pub id: String,
    pub capacity_bytes: u64,
    pub status: String,
    pub storage_class: Option<String>,
    pub access_mode: String,
    pub bound_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageClassInfo {
    pub name: String,
    pub provisioner: String,
    pub is_default: bool,
    pub reclaim_policy: String,
    pub parameters: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimInfo {
    pub id: String,
    pub requested_bytes: u64,
    pub storage_class: Option<String>,
    pub bound_volume: Option<String>,
    pub status: String,
}

// ─────────────────────────────────────────────────────────────
// Storage Class Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StorageClassCreateParams {
    pub name: String,
    pub provisioner: String,
    pub is_default: Option<bool>,
    pub reclaim_policy: Option<String>,
    pub parameters: Option<HashMap<String, String>>,
}

/// Create a storage class
pub async fn storage_class_create(params: Value) -> BridgeResult<Value> {
    let params: StorageClassCreateParams = parse_params(params)?;

    let reclaim_policy = match params.reclaim_policy.as_deref() {
        Some("delete") | Some("Delete") => ReclaimPolicy::Delete,
        Some("recycle") | Some("Recycle") => ReclaimPolicy::Recycle,
        _ => ReclaimPolicy::Retain,
    };

    let mut storage_class =
        StorageClass::new(&params.name, &params.provisioner).with_reclaim_policy(reclaim_policy);

    if params.is_default.unwrap_or(false) {
        storage_class = storage_class.as_default();
    }

    if let Some(params_map) = params.parameters {
        for (k, v) in params_map {
            storage_class = storage_class.with_parameter(&k, &v);
        }
    }

    VOLUME_MANAGER
        .register_storage_class(storage_class)
        .map_err(|e| BridgeError::Internal(format!("failed to create storage class: {e}")))?;

    tracing::info!(name = %params.name, "storage class created");

    to_json(serde_json::json!({
        "success": true,
        "name": params.name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct StorageClassListParams {}

/// List storage classes
pub async fn storage_class_list(_params: Value) -> BridgeResult<Value> {
    let classes = VOLUME_MANAGER.list_storage_classes();
    let default_class = VOLUME_MANAGER.default_storage_class();

    let infos: Vec<StorageClassInfo> = classes
        .iter()
        .map(|sc| StorageClassInfo {
            name: sc.name.clone(),
            provisioner: sc.provisioner.clone(),
            is_default: default_class.as_ref().is_some_and(|d| d.name == sc.name),
            reclaim_policy: format!("{:?}", sc.reclaim_policy),
            parameters: sc.parameters.clone(),
        })
        .collect();

    to_json(infos)
}

// ─────────────────────────────────────────────────────────────
// Volume Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct VolumeProvisionParams {
    pub id: String,
    pub capacity_gb: u64,
    pub storage_class: Option<String>,
    pub access_mode: Option<String>,
}

/// Provision a volume
pub async fn volume_provision(params: Value) -> BridgeResult<Value> {
    let params: VolumeProvisionParams = parse_params(params)?;

    let capacity_bytes = params.capacity_gb * 1024 * 1024 * 1024;

    let volume_id = VolumeId::new(&params.id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid volume id: {e}")))?;

    let access_mode = match params.access_mode.as_deref() {
        Some("ReadOnlyMany") | Some("ROX") => AccessMode::ReadOnlyMany,
        Some("ReadWriteMany") | Some("RWX") => AccessMode::ReadWriteMany,
        _ => AccessMode::ReadWriteOnce,
    };

    let volume_type = VolumeType::EmptyDir(EmptyDirConfig::new());
    let mut volume = Volume::new(volume_id.clone(), volume_type, capacity_bytes);
    volume.access_mode = access_mode;
    volume.storage_class = params.storage_class.clone();

    let created_id = VOLUME_MANAGER
        .provision(volume)
        .map_err(|e| BridgeError::Internal(format!("failed to provision volume: {e}")))?;

    tracing::info!(volume_id = %created_id, "volume provisioned");

    to_json(serde_json::json!({
        "volume_id": created_id.to_string(),
        "capacity_gb": params.capacity_gb,
    }))
}

#[derive(Debug, Deserialize)]
pub struct VolumeGetParams {
    pub volume_id: String,
}

/// Get a volume
pub async fn volume_get(params: Value) -> BridgeResult<Value> {
    let params: VolumeGetParams = parse_params(params)?;

    let volume_id = VolumeId::new(&params.volume_id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid volume id: {e}")))?;

    let volume = VOLUME_MANAGER
        .get_volume(&volume_id)
        .ok_or_else(|| BridgeError::NotFound("volume not found".to_string()))?;

    to_json(volume_to_info(&volume))
}

#[derive(Debug, Deserialize)]
pub struct VolumeListParams {
    pub available_only: Option<bool>,
}

/// List volumes
pub async fn volume_list(params: Value) -> BridgeResult<Value> {
    let params: VolumeListParams = parse_params(params)?;

    let volumes = if params.available_only.unwrap_or(false) {
        VOLUME_MANAGER.list_available_volumes()
    } else {
        VOLUME_MANAGER.list_volumes()
    };

    let infos: Vec<VolumeInfo> = volumes.iter().map(volume_to_info).collect();

    to_json(infos)
}

// ─────────────────────────────────────────────────────────────
// Volume Claim Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ClaimCreateParams {
    pub id: String,
    pub requested_gb: u64,
    pub storage_class: Option<String>,
    pub access_mode: Option<String>,
}

/// Create a volume claim
pub async fn claim_create(params: Value) -> BridgeResult<Value> {
    let params: ClaimCreateParams = parse_params(params)?;

    let requested_bytes = params.requested_gb * 1024 * 1024 * 1024;

    let access_mode = match params.access_mode.as_deref() {
        Some("ReadOnlyMany") | Some("ROX") => AccessMode::ReadOnlyMany,
        Some("ReadWriteMany") | Some("RWX") => AccessMode::ReadWriteMany,
        _ => AccessMode::ReadWriteOnce,
    };

    let mut claim = VolumeClaim::new(&params.id, requested_bytes);
    claim.access_mode = access_mode;
    claim.storage_class = params.storage_class.clone();

    let claim_id = VOLUME_MANAGER
        .create_claim(claim)
        .map_err(|e| BridgeError::Internal(format!("failed to create claim: {e}")))?;

    tracing::info!(claim_id = %claim_id, "volume claim created");

    to_json(serde_json::json!({
        "claim_id": claim_id,
        "requested_gb": params.requested_gb,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ClaimListParams {
    pub pending_only: Option<bool>,
}

/// List volume claims
pub async fn claim_list(params: Value) -> BridgeResult<Value> {
    let params: ClaimListParams = parse_params(params)?;

    let claims = if params.pending_only.unwrap_or(false) {
        VOLUME_MANAGER.list_pending_claims()
    } else {
        VOLUME_MANAGER.list_claims()
    };

    let infos: Vec<ClaimInfo> = claims.iter().map(claim_to_info).collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct ClaimBindParams {
    pub volume_id: String,
    pub claim_id: String,
}

/// Bind a volume to a claim
pub async fn claim_bind(params: Value) -> BridgeResult<Value> {
    let params: ClaimBindParams = parse_params(params)?;

    let volume_id = VolumeId::new(&params.volume_id)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid volume id: {e}")))?;

    VOLUME_MANAGER
        .bind(&volume_id, &params.claim_id)
        .map_err(|e| BridgeError::Internal(format!("failed to bind: {e}")))?;

    tracing::info!(volume_id = %params.volume_id, claim_id = %params.claim_id, "volume bound");

    to_json(serde_json::json!({ "success": true }))
}

#[derive(Debug, Deserialize)]
pub struct ReconcileClaimsParams {}

/// Reconcile pending claims
pub async fn reconcile_claims(_params: Value) -> BridgeResult<Value> {
    let bound_count = VOLUME_MANAGER.reconcile_claims();

    to_json(serde_json::json!({ "bound_count": bound_count }))
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn volume_to_info(volume: &Volume) -> VolumeInfo {
    VolumeInfo {
        id: volume.id.to_string(),
        capacity_bytes: volume.capacity,
        status: format!("{:?}", volume.status),
        storage_class: volume.storage_class.clone(),
        access_mode: format!("{:?}", volume.access_mode),
        bound_to: None, // VolumeStatus::Bound is a unit variant
    }
}

fn claim_to_info(claim: &VolumeClaim) -> ClaimInfo {
    ClaimInfo {
        id: claim.id.clone(),
        requested_bytes: claim.requested_capacity,
        storage_class: claim.storage_class.clone(),
        bound_volume: claim.bound_volume.as_ref().map(|v| v.to_string()),
        status: format!("{:?}", claim.status),
    }
}
