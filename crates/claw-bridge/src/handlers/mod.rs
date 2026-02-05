//! Request handlers
//!
//! Dispatches incoming requests to the appropriate claw-* crate functions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};
use crate::protocol::Response;

pub mod auth;
pub mod cluster;
pub mod deploy;
pub mod molt;
pub mod network;
pub mod observability;
pub mod operations;
pub mod pki;
pub mod secrets;
pub mod service;
pub mod storage;
pub mod tenancy;
pub mod workload;

/// Handle an incoming request
pub async fn handle_request(id: u64, method: &str, params: Value) -> Response {
    let result = dispatch(method, params).await;

    match result {
        Ok(value) => Response::success(id, value),
        Err(e) => Response::error(id, e.code(), e.to_string()),
    }
}

/// Dispatch a method call to the appropriate handler
async fn dispatch(method: &str, params: Value) -> BridgeResult<Value> {
    match method {
        // ─── Cluster Operations ───
        "cluster_status" => cluster::cluster_status(params).await,
        "node_list" => cluster::node_list(params).await,
        "node_get" => cluster::node_get(params).await,
        "node_drain" => cluster::node_drain(params).await,
        "node_cordon" => cluster::node_cordon(params).await,
        "node_uncordon" => cluster::node_uncordon(params).await,

        // ─── Workload Operations ───
        "workload_submit" => workload::workload_submit(params).await,
        "workload_get" => workload::workload_get(params).await,
        "workload_list" => workload::workload_list(params).await,
        "workload_stop" => workload::workload_stop(params).await,
        "workload_scale" => workload::workload_scale(params).await,
        "workload_logs" => workload::workload_logs(params).await,

        // ─── Observability ───
        "metrics_query" => observability::metrics_query(params).await,
        "logs_search" => observability::logs_search(params).await,
        "alert_create" => observability::alert_create(params).await,
        "alert_list" => observability::alert_list(params).await,
        "alert_silence" => observability::alert_silence(params).await,

        // ─── Auth & RBAC ───
        "user_create" => auth::user_create(params).await,
        "user_get" => auth::user_get(params).await,
        "user_list" => auth::user_list(params).await,
        "user_delete" => auth::user_delete(params).await,
        "role_assign" => auth::role_assign(params).await,
        "role_revoke" => auth::role_revoke(params).await,
        "role_list" => auth::role_list(params).await,
        "permission_check" => auth::permission_check(params).await,
        "api_key_generate" => auth::api_key_generate(params).await,
        "api_key_list" => auth::api_key_list(params).await,
        "api_key_revoke" => auth::api_key_revoke(params).await,

        // ─── Deployments ───
        "deploy_intent" => deploy::deploy_intent(params).await,
        "deploy_status" => deploy::deploy_status(params).await,
        "deploy_list" => deploy::deploy_list(params).await,
        "deploy_promote" => deploy::deploy_promote(params).await,
        "deploy_rollback" => deploy::deploy_rollback(params).await,
        "deploy_abort" => deploy::deploy_abort(params).await,

        // ─── Multi-Tenancy ───
        "tenant_create" => tenancy::tenant_create(params).await,
        "tenant_get" => tenancy::tenant_get(params).await,
        "tenant_list" => tenancy::tenant_list(params).await,
        "tenant_delete" => tenancy::tenant_delete(params).await,
        "namespace_create" => tenancy::namespace_create(params).await,
        "namespace_get" => tenancy::namespace_get(params).await,
        "namespace_list" => tenancy::namespace_list(params).await,
        "namespace_delete" => tenancy::namespace_delete(params).await,
        "quota_set" => tenancy::quota_set(params).await,
        "usage_report" => tenancy::usage_report(params).await,

        // ─── Secrets ───
        "secret_put" => secrets::secret_put(params).await,
        "secret_get" => secrets::secret_get(params).await,
        "secret_delete" => secrets::secret_delete(params).await,
        "secret_list" => secrets::secret_list(params).await,
        "secret_rotate" => secrets::secret_rotate(params).await,
        "secret_metadata" => secrets::secret_metadata(params).await,

        // ─── PKI / Certificates ───
        "cert_issue" => pki::cert_issue(params).await,
        "cert_get" => pki::cert_get(params).await,
        "cert_list" => pki::cert_list(params).await,
        "cert_revoke" => pki::cert_revoke(params).await,
        "cert_rotate" => pki::cert_rotate(params).await,
        "ca_status" => pki::ca_status(params).await,

        // ─── Autoscaling ───
        "autoscale_pool_create" => operations::autoscale_pool_create(params).await,
        "autoscale_pool_list" => operations::autoscale_pool_list(params).await,
        "autoscale_evaluate" => operations::autoscale_evaluate(params).await,
        "autoscale_status" => operations::autoscale_status(params).await,

        // ─── Preemption ───
        "preemption_register" => operations::preemption_register(params).await,
        "preemption_request" => operations::preemption_request(params).await,
        "preemption_list" => operations::preemption_list(params).await,

        // ─── Rollback ───
        "rollback_record" => operations::rollback_record(params).await,
        "rollback_plan" => operations::rollback_plan(params).await,
        "rollback_history" => operations::rollback_history(params).await,
        "rollback_trigger_check" => operations::rollback_trigger_check(params).await,

        // ─── Service Discovery ───
        "service_register" => service::service_register(params).await,
        "service_get" => service::service_get(params).await,
        "service_list" => service::service_list(params).await,
        "service_deregister" => service::service_deregister(params).await,
        "endpoint_add" => service::endpoint_add(params).await,
        "endpoint_list" => service::endpoint_list(params).await,
        "endpoint_select" => service::endpoint_select(params).await,

        // ─── Storage ───
        "storage_class_create" => storage::storage_class_create(params).await,
        "storage_class_list" => storage::storage_class_list(params).await,
        "volume_provision" => storage::volume_provision(params).await,
        "volume_get" => storage::volume_get(params).await,
        "volume_list" => storage::volume_list(params).await,
        "claim_create" => storage::claim_create(params).await,
        "claim_list" => storage::claim_list(params).await,
        "claim_bind" => storage::claim_bind(params).await,
        "reconcile_claims" => storage::reconcile_claims(params).await,

        // ─── Network Discovery ───
        "network_scan" => network::network_scan(params).await,
        "credential_profile_create" => network::credential_profile_create(params).await,
        "credential_profile_list" => network::credential_profile_list(params).await,
        "credential_profile_get" => network::credential_profile_get(params).await,
        "node_token_create" => network::node_token_create(params).await,
        "node_token_validate" => network::node_token_validate(params).await,
        "trusted_subnet_add" => network::trusted_subnet_add(params).await,
        "trusted_subnet_list" => network::trusted_subnet_list(params).await,
        "check_trusted" => network::check_trusted(params).await,

        // ─── MOLT Marketplace ───
        "molt_offers" => molt::offers(params).await,
        "molt_offer_create" => molt::offer_create(params).await,
        "molt_order_create" => molt::order_create(params).await,
        "molt_find_matches" => molt::find_matches(params).await,
        "molt_escrow_create" => molt::escrow_create(params).await,
        "molt_escrow_fund" => molt::escrow_fund(params).await,
        "molt_escrow_release" => molt::escrow_release(params).await,
        "molt_escrow_refund" => molt::escrow_refund(params).await,
        "molt_bid" => molt::bid(params).await,
        "molt_spot_prices" => molt::spot_prices(params).await,

        // Unknown method
        _ => Err(BridgeError::MethodNotFound(method.to_string())),
    }
}

// ─────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────

/// Parse params into a typed struct
pub fn parse_params<T: for<'de> Deserialize<'de>>(params: Value) -> BridgeResult<T> {
    serde_json::from_value(params).map_err(|e| BridgeError::InvalidParams(e.to_string()))
}

/// Convert a result to JSON value
pub fn to_json<T: Serialize>(value: T) -> BridgeResult<Value> {
    serde_json::to_value(value).map_err(BridgeError::from)
}
