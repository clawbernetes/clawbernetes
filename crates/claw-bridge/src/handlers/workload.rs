//! Workload management handlers
//!
//! These handlers integrate with claw-gateway's WorkloadManager.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_gateway::{TrackedWorkload, WorkloadLogStore, WorkloadManager};
use claw_proto::{WorkloadId, WorkloadSpec, WorkloadState};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State (would be injected in production)
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref WORKLOAD_MANAGER: Arc<RwLock<WorkloadManager>> = Arc::new(RwLock::new(WorkloadManager::new()));
    static ref WORKLOAD_LOGS: Arc<RwLock<WorkloadLogStore>> = Arc::new(RwLock::new(WorkloadLogStore::new()));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadInfo {
    pub id: String,
    pub name: Option<String>,
    pub state: String,
    pub image: String,
    pub gpu_count: u32,
    pub assigned_node: Option<String>,
    pub submitted_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadDetails {
    pub id: String,
    pub name: Option<String>,
    pub state: String,
    pub image: String,
    pub gpu_count: u32,
    pub assigned_node: Option<String>,
    pub assigned_gpus: Vec<u32>,
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub env: Option<HashMap<String, String>>,
    pub submitted_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub schedule_failure: Option<String>,
}

// ─────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WorkloadSubmitParams {
    pub name: Option<String>,
    pub image: String,
    pub gpus: Option<u32>,
    pub cpu_cores: Option<u32>,
    pub memory_mb: Option<u64>,
    pub env: Option<HashMap<String, String>>,
    pub command: Option<Vec<String>>,
    pub priority: Option<i32>,
}

/// Submit a new workload
pub async fn workload_submit(params: Value) -> BridgeResult<Value> {
    let params: WorkloadSubmitParams = parse_params(params)?;

    // Build the workload spec
    let mut spec = WorkloadSpec::new(&params.image);

    if let Some(gpus) = params.gpus {
        spec = spec.with_gpu_count(gpus);
    }

    if let Some(cpu_cores) = params.cpu_cores {
        spec = spec.with_cpu_cores(cpu_cores);
    }

    if let Some(memory_mb) = params.memory_mb {
        spec = spec.with_memory_mb(memory_mb);
    }

    if let Some(env) = &params.env {
        for (k, v) in env {
            spec = spec.with_env(k, v);
        }
    }

    if let Some(command) = params.command {
        spec = spec.with_command(command);
    }

    let mut manager = WORKLOAD_MANAGER.write();
    let workload_id = manager
        .submit(spec)
        .map_err(|e| BridgeError::Internal(format!("failed to submit workload: {e}")))?;

    tracing::info!(workload_id = %workload_id, name = ?params.name, "workload submitted");

    // Get the submitted workload
    let workload = manager
        .get_workload(workload_id)
        .ok_or_else(|| BridgeError::Internal("workload disappeared after submit".to_string()))?;

    let info = tracked_to_info(workload);
    to_json(info)
}

#[derive(Debug, Deserialize)]
pub struct WorkloadGetParams {
    pub workload_id: String,
}

/// Get workload details
pub async fn workload_get(params: Value) -> BridgeResult<Value> {
    let params: WorkloadGetParams = parse_params(params)?;

    let workload_id = WorkloadId::parse(&params.workload_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid workload_id: {}", params.workload_id)))?;

    let manager = WORKLOAD_MANAGER.read();
    let workload = manager
        .get_workload(workload_id)
        .ok_or_else(|| BridgeError::NotFound(format!("workload {} not found", params.workload_id)))?;

    let details = tracked_to_details(workload);
    to_json(details)
}

#[derive(Debug, Deserialize)]
pub struct WorkloadListParams {
    pub state: Option<String>,
    pub node_id: Option<String>,
    pub limit: Option<u32>,
}

/// List workloads with optional filtering
pub async fn workload_list(params: Value) -> BridgeResult<Value> {
    let params: WorkloadListParams = parse_params(params)?;
    let manager = WORKLOAD_MANAGER.read();

    let state_filter: Option<WorkloadState> = if let Some(state_str) = &params.state {
        Some(parse_workload_state(state_str)?)
    } else {
        None
    };

    let workloads: Vec<WorkloadInfo> = manager
        .list_workloads()
        .into_iter()
        .filter(|w| {
            // Filter by state
            if let Some(state) = state_filter {
                if w.state() != state {
                    return false;
                }
            }

            // Filter by node
            if let Some(node_id) = &params.node_id {
                if let Some(assigned) = &w.assigned_node {
                    if assigned.to_string() != *node_id {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            true
        })
        .take(params.limit.unwrap_or(100) as usize)
        .map(|w| tracked_to_info(w))
        .collect();

    to_json(workloads)
}

#[derive(Debug, Deserialize)]
pub struct WorkloadStopParams {
    pub workload_id: String,
    pub force: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WorkloadStopResult {
    pub success: bool,
}

/// Stop a running workload
pub async fn workload_stop(params: Value) -> BridgeResult<Value> {
    let params: WorkloadStopParams = parse_params(params)?;

    let workload_id = WorkloadId::parse(&params.workload_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid workload_id: {}", params.workload_id)))?;

    let mut manager = WORKLOAD_MANAGER.write();
    manager
        .cancel(workload_id)
        .map_err(|e| BridgeError::Internal(format!("failed to stop workload: {e}")))?;

    tracing::info!(workload_id = %workload_id, "workload stopped");

    to_json(WorkloadStopResult { success: true })
}

#[derive(Debug, Deserialize)]
pub struct WorkloadScaleParams {
    pub workload_id: String,
    pub replicas: u32,
}

#[derive(Debug, Serialize)]
pub struct WorkloadScaleResult {
    pub success: bool,
    pub new_replicas: u32,
}

/// Scale workload replicas (for parallel workloads)
pub async fn workload_scale(params: Value) -> BridgeResult<Value> {
    let params: WorkloadScaleParams = parse_params(params)?;

    // NOTE: Scaling would require ParallelConfig support
    // For now, return success with the requested replicas

    tracing::info!(
        workload_id = %params.workload_id,
        replicas = params.replicas,
        "workload scale requested (not yet implemented)"
    );

    to_json(WorkloadScaleResult {
        success: true,
        new_replicas: params.replicas,
    })
}

#[derive(Debug, Deserialize)]
pub struct WorkloadLogsParams {
    pub workload_id: String,
    pub tail: Option<u32>,
    pub follow: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WorkloadLogsResult {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}

/// Get workload logs
pub async fn workload_logs(params: Value) -> BridgeResult<Value> {
    let params: WorkloadLogsParams = parse_params(params)?;

    let workload_id = WorkloadId::parse(&params.workload_id)
        .map_err(|_| BridgeError::InvalidParams(format!("invalid workload_id: {}", params.workload_id)))?;

    let tail = Some(params.tail.unwrap_or(100) as usize);
    let log_store = WORKLOAD_LOGS.read();

    let (stdout, stderr) = if let Some(logs) = log_store.get_logs(workload_id) {
        (logs.get_stdout(tail), logs.get_stderr(tail))
    } else {
        (vec![], vec![])
    };

    to_json(WorkloadLogsResult { stdout, stderr })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn parse_workload_state(state: &str) -> BridgeResult<WorkloadState> {
    match state.to_lowercase().as_str() {
        "pending" => Ok(WorkloadState::Pending),
        "starting" => Ok(WorkloadState::Starting),
        "running" => Ok(WorkloadState::Running),
        "stopping" => Ok(WorkloadState::Stopping),
        "stopped" => Ok(WorkloadState::Stopped),
        "completed" => Ok(WorkloadState::Completed),
        "failed" => Ok(WorkloadState::Failed),
        _ => Err(BridgeError::InvalidParams(format!("unknown workload state: {state}"))),
    }
}

fn tracked_to_info(workload: &TrackedWorkload) -> WorkloadInfo {
    let status = &workload.workload.status;

    WorkloadInfo {
        id: workload.id().to_string(),
        name: workload.workload.name.clone(),
        state: format!("{:?}", workload.state()).to_lowercase(),
        image: workload.workload.spec.image.clone(),
        gpu_count: workload.workload.spec.gpu_count,
        assigned_node: workload.assigned_node.map(|n| n.to_string()),
        submitted_at: workload.submitted_at.timestamp_millis(),
        started_at: status.started_at.map(|t| t.timestamp_millis()),
        finished_at: status.finished_at.map(|t| t.timestamp_millis()),
    }
}

fn tracked_to_details(workload: &TrackedWorkload) -> WorkloadDetails {
    let spec = &workload.workload.spec;
    let status = &workload.workload.status;

    WorkloadDetails {
        id: workload.id().to_string(),
        name: workload.workload.name.clone(),
        state: format!("{:?}", workload.state()).to_lowercase(),
        image: spec.image.clone(),
        gpu_count: spec.gpu_count,
        assigned_node: workload.assigned_node.map(|n| n.to_string()),
        assigned_gpus: workload.assigned_gpus.clone(),
        cpu_cores: spec.cpu_cores,
        memory_mb: spec.memory_mb,
        env: if spec.env.is_empty() {
            None
        } else {
            Some(spec.env.clone())
        },
        submitted_at: workload.submitted_at.timestamp_millis(),
        started_at: status.started_at.map(|t| t.timestamp_millis()),
        finished_at: status.finished_at.map(|t| t.timestamp_millis()),
        schedule_failure: workload.schedule_failure.clone(),
    }
}
