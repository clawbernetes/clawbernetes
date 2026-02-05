//! Multi-tenancy handlers
//!
//! These handlers integrate with claw-tenancy for tenant and namespace management.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_tenancy::{NamespaceId, NamespaceManager, ResourceQuota, Tenant, TenantId};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref NAMESPACE_MANAGER: Arc<NamespaceManager> = Arc::new(NamespaceManager::new());
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub created_at: i64,
    pub namespace_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamespaceInfo {
    pub id: String,
    pub name: String,
    pub tenant_id: String,
    pub quota: QuotaInfo,
    pub usage: UsageInfo,
    pub labels: HashMap<String, String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaInfo {
    pub max_gpus: Option<u32>,
    pub gpu_hours: Option<f64>,
    pub memory_mib: Option<u64>,
    pub max_workloads: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageInfo {
    pub gpus_in_use: u32,
    pub gpu_hours_used: f64,
    pub memory_mib_used: u64,
    pub max_utilization_percent: Option<f64>,
}

// ─────────────────────────────────────────────────────────────
// Tenant Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TenantCreateParams {
    pub name: String,
    pub default_quota: Option<QuotaParams>,
}

#[derive(Debug, Deserialize)]
pub struct QuotaParams {
    pub max_gpus: Option<u32>,
    pub gpu_hours: Option<f64>,
    pub memory_mib: Option<u64>,
    pub max_workloads: Option<u32>,
}

/// Create a new tenant
pub async fn tenant_create(params: Value) -> BridgeResult<Value> {
    let params: TenantCreateParams = parse_params(params)?;

    let tenant = if let Some(quota_params) = &params.default_quota {
        let quota = build_quota(quota_params);
        NAMESPACE_MANAGER
            .create_tenant_with_config(&params.name, quota.clone(), quota)
            .map_err(|e| BridgeError::Internal(format!("failed to create tenant: {e}")))?
    } else {
        NAMESPACE_MANAGER
            .create_tenant(&params.name)
            .map_err(|e| BridgeError::Internal(format!("failed to create tenant: {e}")))?
    };

    tracing::info!(tenant_id = %tenant.id, name = %params.name, "tenant created");
    to_json(tenant_to_info(&tenant))
}

#[derive(Debug, Deserialize)]
pub struct TenantGetParams {
    pub tenant_id: Option<String>,
    pub name: Option<String>,
}

/// Get tenant by ID or name
pub async fn tenant_get(params: Value) -> BridgeResult<Value> {
    let params: TenantGetParams = parse_params(params)?;

    let tenant = if let Some(id_str) = &params.tenant_id {
        let tenant_id = TenantId::parse(id_str)
            .map_err(|_| BridgeError::InvalidParams(format!("invalid tenant_id: {}", id_str)))?;
        NAMESPACE_MANAGER.get_tenant(tenant_id)
    } else if let Some(name) = &params.name {
        NAMESPACE_MANAGER.get_tenant_by_name(name)
    } else {
        return Err(BridgeError::InvalidParams(
            "must provide tenant_id or name".to_string(),
        ));
    };

    let tenant = tenant.ok_or_else(|| BridgeError::NotFound("tenant not found".to_string()))?;

    to_json(tenant_to_info(&tenant))
}

#[derive(Debug, Deserialize)]
pub struct TenantListParams {}

/// List all tenants
pub async fn tenant_list(_params: Value) -> BridgeResult<Value> {
    let tenants = NAMESPACE_MANAGER.list_tenants();

    let infos: Vec<TenantInfo> = tenants.into_iter().map(|t| tenant_to_info(&t)).collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct TenantDeleteParams {
    pub tenant_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub success: bool,
}

/// Delete a tenant
pub async fn tenant_delete(params: Value) -> BridgeResult<Value> {
    let params: TenantDeleteParams = parse_params(params)?;

    let tenant_id = TenantId::parse(&params.tenant_id)
        .map_err(|_| BridgeError::InvalidParams("invalid tenant_id".to_string()))?;

    NAMESPACE_MANAGER
        .delete_tenant(tenant_id)
        .map_err(|e| BridgeError::Internal(format!("failed to delete tenant: {e}")))?;

    tracing::info!(tenant_id = %params.tenant_id, "tenant deleted");
    to_json(DeleteResult { success: true })
}

// ─────────────────────────────────────────────────────────────
// Namespace Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NamespaceCreateParams {
    pub tenant_id: String,
    pub name: String,
    pub quota: Option<QuotaParams>,
}

/// Create a namespace
pub async fn namespace_create(params: Value) -> BridgeResult<Value> {
    let params: NamespaceCreateParams = parse_params(params)?;

    let tenant_id = TenantId::parse(&params.tenant_id)
        .map_err(|_| BridgeError::InvalidParams("invalid tenant_id".to_string()))?;

    let namespace = if let Some(quota_params) = &params.quota {
        let quota = build_quota(quota_params);
        NAMESPACE_MANAGER
            .create_namespace_with_quota(tenant_id, &params.name, quota)
            .map_err(|e| BridgeError::Internal(format!("failed to create namespace: {e}")))?
    } else {
        NAMESPACE_MANAGER
            .create_namespace(tenant_id, &params.name)
            .map_err(|e| BridgeError::Internal(format!("failed to create namespace: {e}")))?
    };

    tracing::info!(
        namespace_id = %namespace.id,
        tenant_id = %params.tenant_id,
        name = %params.name,
        "namespace created"
    );

    to_json(namespace_to_info(&namespace))
}

#[derive(Debug, Deserialize)]
pub struct NamespaceGetParams {
    pub namespace_id: Option<String>,
    pub tenant_id: Option<String>,
    pub name: Option<String>,
}

/// Get namespace by ID or by tenant + name
pub async fn namespace_get(params: Value) -> BridgeResult<Value> {
    let params: NamespaceGetParams = parse_params(params)?;

    let namespace = if let Some(id_str) = &params.namespace_id {
        let namespace_id = NamespaceId::parse(id_str)
            .map_err(|_| BridgeError::InvalidParams("invalid namespace_id".to_string()))?;
        NAMESPACE_MANAGER.get_namespace(namespace_id)
    } else if let (Some(tenant_str), Some(name)) = (&params.tenant_id, &params.name) {
        let tenant_id = TenantId::parse(tenant_str)
            .map_err(|_| BridgeError::InvalidParams("invalid tenant_id".to_string()))?;
        NAMESPACE_MANAGER.get_namespace_by_name(tenant_id, name)
    } else {
        return Err(BridgeError::InvalidParams(
            "must provide namespace_id OR (tenant_id + name)".to_string(),
        ));
    };

    let namespace =
        namespace.ok_or_else(|| BridgeError::NotFound("namespace not found".to_string()))?;

    to_json(namespace_to_info(&namespace))
}

#[derive(Debug, Deserialize)]
pub struct NamespaceListParams {
    pub tenant_id: Option<String>,
}

/// List namespaces
pub async fn namespace_list(params: Value) -> BridgeResult<Value> {
    let params: NamespaceListParams = parse_params(params)?;

    let namespaces = if let Some(tenant_str) = &params.tenant_id {
        let tenant_id = TenantId::parse(tenant_str)
            .map_err(|_| BridgeError::InvalidParams("invalid tenant_id".to_string()))?;
        NAMESPACE_MANAGER.list_namespaces(tenant_id)
    } else {
        NAMESPACE_MANAGER.list_all_namespaces()
    };

    let infos: Vec<NamespaceInfo> = namespaces.iter().map(namespace_to_info).collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct NamespaceDeleteParams {
    pub namespace_id: String,
}

/// Delete a namespace
pub async fn namespace_delete(params: Value) -> BridgeResult<Value> {
    let params: NamespaceDeleteParams = parse_params(params)?;

    let namespace_id = NamespaceId::parse(&params.namespace_id)
        .map_err(|_| BridgeError::InvalidParams("invalid namespace_id".to_string()))?;

    NAMESPACE_MANAGER
        .delete_namespace(namespace_id)
        .map_err(|e| BridgeError::Internal(format!("failed to delete namespace: {e}")))?;

    tracing::info!(namespace_id = %params.namespace_id, "namespace deleted");
    to_json(DeleteResult { success: true })
}

// ─────────────────────────────────────────────────────────────
// Quota Management
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QuotaSetParams {
    pub namespace_id: String,
    pub quota: QuotaParams,
}

/// Set quota for a namespace
pub async fn quota_set(params: Value) -> BridgeResult<Value> {
    let params: QuotaSetParams = parse_params(params)?;

    let namespace_id = NamespaceId::parse(&params.namespace_id)
        .map_err(|_| BridgeError::InvalidParams("invalid namespace_id".to_string()))?;

    let mut namespace = NAMESPACE_MANAGER
        .get_namespace(namespace_id)
        .ok_or_else(|| BridgeError::NotFound("namespace not found".to_string()))?;

    // Update quota
    let new_quota = build_quota(&params.quota);
    namespace.quota = new_quota;

    NAMESPACE_MANAGER
        .update_namespace(namespace.clone())
        .map_err(|e| BridgeError::Internal(format!("failed to update namespace: {e}")))?;

    tracing::info!(namespace_id = %params.namespace_id, "quota updated");
    to_json(namespace_to_info(&namespace))
}

#[derive(Debug, Deserialize)]
pub struct UsageReportParams {
    pub tenant_id: Option<String>,
    pub namespace_id: Option<String>,
    pub threshold_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct UsageReport {
    pub namespaces: Vec<NamespaceInfo>,
    pub over_threshold: Vec<String>,
    pub total_gpus_in_use: u32,
    pub total_gpu_hours_used: f64,
}

/// Get usage report
pub async fn usage_report(params: Value) -> BridgeResult<Value> {
    let params: UsageReportParams = parse_params(params)?;

    let namespaces = if let Some(ns_id_str) = &params.namespace_id {
        let namespace_id = NamespaceId::parse(ns_id_str)
            .map_err(|_| BridgeError::InvalidParams("invalid namespace_id".to_string()))?;
        NAMESPACE_MANAGER
            .get_namespace(namespace_id)
            .map(|ns| vec![ns])
            .unwrap_or_default()
    } else if let Some(tenant_str) = &params.tenant_id {
        let tenant_id = TenantId::parse(tenant_str)
            .map_err(|_| BridgeError::InvalidParams("invalid tenant_id".to_string()))?;
        NAMESPACE_MANAGER.list_namespaces(tenant_id)
    } else {
        NAMESPACE_MANAGER.list_all_namespaces()
    };

    let threshold = params.threshold_percent.unwrap_or(80.0);
    let over_threshold = NAMESPACE_MANAGER.find_namespaces_over_threshold(threshold);

    let mut total_gpus_in_use = 0u32;
    let mut total_gpu_hours_used = 0f64;

    let infos: Vec<NamespaceInfo> = namespaces
        .iter()
        .map(|ns| {
            total_gpus_in_use += ns.usage.gpus_in_use;
            total_gpu_hours_used += ns.usage.gpu_hours_used;
            namespace_to_info(ns)
        })
        .collect();

    let over_ids: Vec<String> = over_threshold.iter().map(|ns| ns.id.to_string()).collect();

    to_json(UsageReport {
        namespaces: infos,
        over_threshold: over_ids,
        total_gpus_in_use,
        total_gpu_hours_used,
    })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn build_quota(params: &QuotaParams) -> ResourceQuota {
    let mut quota = ResourceQuota::new();
    if let Some(max_gpus) = params.max_gpus {
        quota = quota.with_max_gpus(max_gpus);
    }
    if let Some(gpu_hours) = params.gpu_hours {
        quota = quota.with_gpu_hours(gpu_hours);
    }
    if let Some(memory_mib) = params.memory_mib {
        quota = quota.with_memory_mib(memory_mib);
    }
    if let Some(max_workloads) = params.max_workloads {
        quota = quota.with_max_workloads(max_workloads);
    }
    quota
}

fn tenant_to_info(tenant: &Tenant) -> TenantInfo {
    TenantInfo {
        id: tenant.id.to_string(),
        name: tenant.name.clone(),
        display_name: tenant.display_name.clone(),
        created_at: tenant.created_at.timestamp_millis(),
        namespace_count: tenant.namespace_count(),
    }
}

fn namespace_to_info(ns: &claw_tenancy::Namespace) -> NamespaceInfo {
    let utilization = ns.usage.utilization(&ns.quota);

    NamespaceInfo {
        id: ns.id.to_string(),
        name: ns.name.clone(),
        tenant_id: ns.tenant_id.to_string(),
        quota: QuotaInfo {
            max_gpus: ns.quota.max_gpus,
            gpu_hours: ns.quota.gpu_hours,
            memory_mib: ns.quota.memory_mib,
            max_workloads: ns.quota.max_workloads,
        },
        usage: UsageInfo {
            gpus_in_use: ns.usage.gpus_in_use,
            gpu_hours_used: ns.usage.gpu_hours_used,
            memory_mib_used: ns.usage.memory_mib_used,
            max_utilization_percent: utilization.max_utilization(),
        },
        labels: ns.labels.clone(),
        created_at: ns.created_at.timestamp_millis(),
    }
}
