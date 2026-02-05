//! Cluster management handlers
//!
//! These handlers integrate with claw-discovery and claw-compute to provide
//! cluster-level operations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ClusterStatus {
    pub name: String,
    pub healthy: bool,
    pub nodes: NodeCounts,
    pub gpus: GpuCounts,
    pub workloads: WorkloadCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeCounts {
    pub total: u32,
    pub ready: u32,
    pub not_ready: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuCounts {
    pub total: u32,
    pub available: u32,
    pub allocated: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadCounts {
    pub running: u32,
    pub pending: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub labels: HashMap<String, String>,
    pub gpus: Vec<GpuInfo>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub platform: String,
    pub region: Option<String>,
    pub zone: Option<String>,
    pub connected_at: u64,
    pub last_heartbeat: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub id: String,
    pub model: String,
    pub vendor: String,
    pub memory_mb: u64,
    pub compute_capability: Option<String>,
    pub utilization_percent: Option<f32>,
    pub temperature_celsius: Option<f32>,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StatusParams {
    pub cluster: Option<String>,
}

/// Get cluster status
pub async fn status(params: Value) -> BridgeResult<Value> {
    let _params: StatusParams = parse_params(params)?;

    // TODO: Aggregate from actual crates
    // For now, return mock data that matches the expected interface
    let status = ClusterStatus {
        name: "default".to_string(),
        healthy: true,
        nodes: NodeCounts {
            total: 0,
            ready: 0,
            not_ready: 0,
        },
        gpus: GpuCounts {
            total: 0,
            available: 0,
            allocated: 0,
        },
        workloads: WorkloadCounts {
            running: 0,
            pending: 0,
            failed: 0,
        },
    };

    to_json(status)
}

#[derive(Debug, Deserialize)]
pub struct NodeListParams {
    pub status: Option<String>,
    pub labels: Option<HashMap<String, String>>,
    pub gpu_model: Option<String>,
}

/// List nodes in the cluster
pub async fn node_list(params: Value) -> BridgeResult<Value> {
    let _params: NodeListParams = parse_params(params)?;

    // TODO: Get from claw-discovery service registry
    let nodes: Vec<NodeInfo> = vec![];

    to_json(nodes)
}

#[derive(Debug, Deserialize)]
pub struct NodeGetParams {
    pub node_id: String,
}

/// Get detailed node information
pub async fn node_get(params: Value) -> BridgeResult<Value> {
    let params: NodeGetParams = parse_params(params)?;

    // TODO: Look up in claw-discovery
    Err(BridgeError::NotFound(format!(
        "node not found: {}",
        params.node_id
    )))
}

#[derive(Debug, Deserialize)]
pub struct NodeDrainParams {
    pub node_id: String,
    pub grace_period_seconds: Option<u32>,
    pub force: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct NodeDrainResult {
    pub success: bool,
    pub migrated_workloads: u32,
}

/// Drain a node (migrate all workloads off)
pub async fn node_drain(params: Value) -> BridgeResult<Value> {
    let params: NodeDrainParams = parse_params(params)?;

    // TODO: Implement via claw-preemption + workload migration
    tracing::info!(
        node_id = %params.node_id,
        force = params.force.unwrap_or(false),
        "draining node"
    );

    to_json(NodeDrainResult {
        success: true,
        migrated_workloads: 0,
    })
}

#[derive(Debug, Deserialize)]
pub struct NodeCordonParams {
    pub node_id: String,
}

#[derive(Debug, Serialize)]
pub struct NodeCordonResult {
    pub success: bool,
}

/// Cordon a node (prevent new workload scheduling)
pub async fn node_cordon(params: Value) -> BridgeResult<Value> {
    let params: NodeCordonParams = parse_params(params)?;

    // TODO: Update node state in discovery
    tracing::info!(node_id = %params.node_id, "cordoning node");

    to_json(NodeCordonResult { success: true })
}

/// Uncordon a node (allow new workload scheduling)
pub async fn node_uncordon(params: Value) -> BridgeResult<Value> {
    let params: NodeCordonParams = parse_params(params)?;

    // TODO: Update node state in discovery
    tracing::info!(node_id = %params.node_id, "uncordoning node");

    to_json(NodeCordonResult { success: true })
}
