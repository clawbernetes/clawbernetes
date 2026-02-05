//! Cluster management handlers
//!
//! These handlers integrate with claw-gateway's NodeRegistry for fleet management.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_gateway::{NodeRegistry, RegisteredNode};
use claw_proto::NodeId;

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State (would be injected in production)
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref NODE_REGISTRY: Arc<RwLock<NodeRegistry>> = Arc::new(RwLock::new(NodeRegistry::new()));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ClusterStatus {
    pub healthy: bool,
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub unhealthy_nodes: usize,
    pub draining_nodes: usize,
    pub offline_nodes: usize,
    pub total_gpus: usize,
    pub available_gpus: usize,
    pub pending_workloads: usize,
    pub running_workloads: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub gpu_count: usize,
    pub gpu_type: Option<String>,
    pub cpu_cores: u32,
    pub memory_gb: u64,
    pub draining: bool,
    pub last_heartbeat: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDetails {
    pub id: String,
    pub name: String,
    pub status: String,
    pub gpus: Vec<GpuInfo>,
    pub cpu_cores: u32,
    pub memory_gb: u64,
    pub draining: bool,
    pub registered_at: i64,
    pub last_heartbeat: i64,
    pub workloads: Vec<String>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub vram_mb: u64,
    pub available: bool,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

/// Get overall cluster status
pub async fn cluster_status(_params: Value) -> BridgeResult<Value> {
    let registry = NODE_REGISTRY.read();
    let summary = registry.health_summary();

    // Count GPUs
    let mut total_gpus = 0;
    let mut available_gpus = 0;

    for node in registry.list_nodes() {
        let gpu_count = node.gpu_count();
        total_gpus += gpu_count;
        if node.is_available() {
            available_gpus += gpu_count;
        }
    }

    let status = ClusterStatus {
        healthy: summary.unhealthy == 0 && summary.offline == 0,
        total_nodes: summary.total(),
        healthy_nodes: summary.healthy,
        unhealthy_nodes: summary.unhealthy,
        draining_nodes: summary.draining,
        offline_nodes: summary.offline,
        total_gpus,
        available_gpus,
        pending_workloads: 0, // TODO: integrate with WorkloadManager
        running_workloads: 0,
    };

    to_json(status)
}

#[derive(Debug, Deserialize)]
pub struct NodeListParams {
    pub status: Option<String>,
    pub gpu_type: Option<String>,
    pub labels: Option<HashMap<String, String>>,
}

/// List nodes with optional filtering
pub async fn node_list(params: Value) -> BridgeResult<Value> {
    let params: NodeListParams = parse_params(params)?;
    let registry = NODE_REGISTRY.read();

    let nodes: Vec<NodeSummary> = registry
        .list_nodes()
        .iter()
        .filter(|node| {
            // Filter by status
            if let Some(status) = &params.status {
                let node_status = node.health_status().to_string();
                if &node_status != status {
                    return false;
                }
            }

            // Filter by GPU type
            if let Some(gpu_type) = &params.gpu_type {
                if !node.has_gpu_type(gpu_type) {
                    return false;
                }
            }

            true
        })
        .map(|node| node_to_summary(node))
        .collect();

    to_json(nodes)
}

#[derive(Debug, Deserialize)]
pub struct NodeGetParams {
    pub node_id: String,
}

/// Get detailed node information
pub async fn node_get(params: Value) -> BridgeResult<Value> {
    let params: NodeGetParams = parse_params(params)?;

    let node_id = NodeId::parse(&params.node_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid node_id: {}", params.node_id)))?;

    let registry = NODE_REGISTRY.read();
    let node = registry
        .get_node(node_id)
        .ok_or_else(|| BridgeError::NotFound(format!("node {} not found", params.node_id)))?;

    let details = node_to_details(node);
    to_json(details)
}

#[derive(Debug, Deserialize)]
pub struct NodeDrainParams {
    pub node_id: String,
    pub force: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct NodeDrainResult {
    pub success: bool,
    pub migrated_workloads: Vec<String>,
}

/// Drain a node (migrate workloads off)
pub async fn node_drain(params: Value) -> BridgeResult<Value> {
    let params: NodeDrainParams = parse_params(params)?;

    let node_id = NodeId::parse(&params.node_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid node_id: {}", params.node_id)))?;

    let mut registry = NODE_REGISTRY.write();
    registry
        .set_draining(node_id, true)
        .map_err(|e| BridgeError::NotFound(e.to_string()))?;

    tracing::info!(node_id = %params.node_id, "node marked as draining");

    // TODO: Actually migrate workloads via WorkloadManager

    let result = NodeDrainResult {
        success: true,
        migrated_workloads: vec![],
    };

    to_json(result)
}

#[derive(Debug, Deserialize)]
pub struct NodeCordonParams {
    pub node_id: String,
}

#[derive(Debug, Serialize)]
pub struct NodeCordonResult {
    pub success: bool,
}

/// Cordon a node (prevent new scheduling)
pub async fn node_cordon(params: Value) -> BridgeResult<Value> {
    let params: NodeCordonParams = parse_params(params)?;

    let node_id = NodeId::parse(&params.node_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid node_id: {}", params.node_id)))?;

    let mut registry = NODE_REGISTRY.write();
    registry
        .set_draining(node_id, true)
        .map_err(|e| BridgeError::NotFound(e.to_string()))?;

    tracing::info!(node_id = %params.node_id, "node cordoned");

    to_json(NodeCordonResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct NodeUncordonParams {
    pub node_id: String,
}

/// Uncordon a node (allow scheduling)
pub async fn node_uncordon(params: Value) -> BridgeResult<Value> {
    let params: NodeUncordonParams = parse_params(params)?;

    let node_id = NodeId::parse(&params.node_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid node_id: {}", params.node_id)))?;

    let mut registry = NODE_REGISTRY.write();
    registry
        .set_draining(node_id, false)
        .map_err(|e| BridgeError::NotFound(e.to_string()))?;

    tracing::info!(node_id = %params.node_id, "node uncordoned");

    to_json(NodeCordonResult { success: true })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn node_to_summary(node: &RegisteredNode) -> NodeSummary {
    let gpu_type = node
        .capabilities
        .gpus
        .first()
        .map(|g| g.name.clone());

    NodeSummary {
        id: node.id.to_string(),
        name: node.name.clone(),
        status: node.health_status().to_string(),
        gpu_count: node.gpu_count(),
        gpu_type,
        cpu_cores: node.capabilities.cpu_cores,
        memory_gb: node.capabilities.memory_mib / 1024,
        draining: node.draining,
        last_heartbeat: node.last_heartbeat.timestamp_millis(),
    }
}

fn node_to_details(node: &RegisteredNode) -> NodeDetails {
    let gpus: Vec<GpuInfo> = node
        .capabilities
        .gpus
        .iter()
        .map(|gpu| GpuInfo {
            index: gpu.index,
            name: gpu.name.clone(),
            vram_mb: gpu.memory_mib,
            available: true, // TODO: Track actual availability
        })
        .collect();

    NodeDetails {
        id: node.id.to_string(),
        name: node.name.clone(),
        status: node.health_status().to_string(),
        gpus,
        cpu_cores: node.capabilities.cpu_cores,
        memory_gb: node.capabilities.memory_mib / 1024,
        draining: node.draining,
        registered_at: node.registered_at.timestamp_millis(),
        last_heartbeat: node.last_heartbeat.timestamp_millis(),
        workloads: vec![], // TODO: Track from WorkloadManager
        labels: if node.capabilities.labels.is_empty() {
            None
        } else {
            Some(node.capabilities.labels.clone())
        },
    }
}
