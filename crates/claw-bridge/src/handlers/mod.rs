//! Request handlers
//!
//! Dispatches incoming requests to the appropriate claw-* crate functions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{BridgeError, BridgeResult};
use crate::protocol::Response;

pub mod cluster;
pub mod molt;
pub mod observability;
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
        // Cluster operations
        "cluster_status" => cluster::status(params).await,
        "node_list" => cluster::node_list(params).await,
        "node_get" => cluster::node_get(params).await,
        "node_drain" => cluster::node_drain(params).await,
        "node_cordon" => cluster::node_cordon(params).await,
        "node_uncordon" => cluster::node_uncordon(params).await,

        // Workload operations
        "workload_submit" => workload::submit(params).await,
        "workload_get" => workload::get(params).await,
        "workload_list" => workload::list(params).await,
        "workload_stop" => workload::stop(params).await,
        "workload_scale" => workload::scale(params).await,
        "workload_logs" => workload::logs(params).await,

        // Observability
        "metrics_query" => observability::metrics_query(params).await,
        "logs_search" => observability::logs_search(params).await,
        "alert_create" => observability::alert_create(params).await,
        "alert_list" => observability::alert_list(params).await,
        "alert_silence" => observability::alert_silence(params).await,

        // MOLT marketplace
        "molt_offers" => molt::offers(params).await,
        "molt_offer_create" => molt::offer_create(params).await,
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
