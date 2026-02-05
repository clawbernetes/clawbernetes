//! Operations handlers
//!
//! These handlers integrate with claw-autoscaler, claw-preemption, and claw-rollback.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_autoscaler::{
    AutoscalerManager, InMemoryMetricsProvider, NodePool, PoolId, ScaleDirection, ScalingPolicy,
};
use claw_preemption::{
    NoOpEvictionHandler, PreemptionCandidate, PreemptionManager, PreemptionRequest,
    PriorityClass, ResourceRequirements, WorkloadId as PreemptWorkloadId,
};
use claw_rollback::{
    DeploymentHistory, DeploymentId, DeploymentSnapshot, DeploymentSpec, Metrics,
    RollbackExecutor, TriggerEvaluator,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref AUTOSCALER: AutoscalerManager<InMemoryMetricsProvider> = 
        AutoscalerManager::new(InMemoryMetricsProvider::new());
    static ref PREEMPTION_MANAGER: PreemptionManager<NoOpEvictionHandler> =
        PreemptionManager::with_defaults(NoOpEvictionHandler::new());
    static ref ROLLBACK_HISTORY: RwLock<DeploymentHistory> =
        RwLock::new(DeploymentHistory::new(100).expect("valid history size"));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PoolInfo {
    pub id: String,
    pub name: String,
    pub node_count: usize,
    pub min_nodes: u32,
    pub max_nodes: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScaleActionInfo {
    pub pool_id: String,
    pub direction: String,
    pub current_nodes: u32,
    pub target_nodes: u32,
    pub reason: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreemptionCandidateInfo {
    pub workload_id: String,
    pub priority: u16,
    pub priority_class: String,
    pub gpus: u32,
}

// ─────────────────────────────────────────────────────────────
// Autoscaler Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AutoscalePoolCreateParams {
    pub id: String,
    pub name: String,
    pub min_nodes: u32,
    pub max_nodes: u32,
    pub target_utilization: Option<f64>,
}

/// Create an autoscaling pool
pub async fn autoscale_pool_create(params: Value) -> BridgeResult<Value> {
    let params: AutoscalePoolCreateParams = parse_params(params)?;

    let policy = ScalingPolicy::builder(&format!("{}-policy", params.id), "Auto-generated policy")
        .min_nodes(params.min_nodes)
        .max_nodes(params.max_nodes)
        .target_utilization(params.target_utilization.unwrap_or(70.0), 10.0)
        .build()
        .map_err(|e| BridgeError::InvalidParams(format!("invalid policy: {e}")))?;

    let pool = NodePool::new(&params.id, &params.name, policy)
        .map_err(|e| BridgeError::InvalidParams(format!("invalid pool: {e}")))?;

    AUTOSCALER
        .register_pool(pool)
        .map_err(|e| BridgeError::Internal(format!("failed to register pool: {e}")))?;

    tracing::info!(pool_id = %params.id, "autoscale pool created");

    to_json(PoolInfo {
        id: params.id,
        name: params.name,
        node_count: 0,
        min_nodes: params.min_nodes,
        max_nodes: params.max_nodes,
    })
}

#[derive(Debug, Deserialize)]
pub struct AutoscalePoolListParams {}

/// List autoscaling pools
pub async fn autoscale_pool_list(_params: Value) -> BridgeResult<Value> {
    let pools = AUTOSCALER.list_pools();

    let infos: Vec<PoolInfo> = pools
        .iter()
        .map(|p| PoolInfo {
            id: p.id.to_string(),
            name: p.name.clone(),
            node_count: p.nodes.len(),
            min_nodes: p.policy.min_nodes,
            max_nodes: p.policy.max_nodes,
        })
        .collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct AutoscaleEvaluateParams {}

/// Evaluate autoscaling and get recommendations
pub async fn autoscale_evaluate(_params: Value) -> BridgeResult<Value> {
    let actions = AUTOSCALER
        .evaluate()
        .map_err(|e| BridgeError::Internal(format!("evaluation failed: {e}")))?;

    let infos: Vec<ScaleActionInfo> = actions
        .iter()
        .map(|a| ScaleActionInfo {
            pool_id: a.pool_id.to_string(),
            direction: match a.recommendation.direction {
                ScaleDirection::Up => "up".to_string(),
                ScaleDirection::Down => "down".to_string(),
                ScaleDirection::None => "none".to_string(),
            },
            current_nodes: a.recommendation.current_nodes,
            target_nodes: a.recommendation.target_nodes,
            reason: a.recommendation.reason.clone(),
            confidence: a.recommendation.confidence,
        })
        .collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct AutoscaleStatusParams {}

/// Get autoscaler status
pub async fn autoscale_status(_params: Value) -> BridgeResult<Value> {
    let status = AUTOSCALER.status();

    to_json(serde_json::json!({
        "enabled": status.enabled,
        "pool_count": status.pool_count,
        "pending_actions": status.pending_actions,
        "last_evaluation": status.last_evaluation.map(|t| t.timestamp_millis()),
    }))
}

// ─────────────────────────────────────────────────────────────
// Preemption Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PreemptionRegisterParams {
    pub workload_id: String,
    pub priority_class: Option<String>,
    pub gpus: Option<u32>,
    pub memory_gb: Option<u64>,
}

/// Register a workload for preemption tracking
pub async fn preemption_register(params: Value) -> BridgeResult<Value> {
    let params: PreemptionRegisterParams = parse_params(params)?;

    let workload_id = PreemptWorkloadId::new(&params.workload_id);

    let priority_class = match params.priority_class.as_deref() {
        Some("system-critical") => PriorityClass::system_critical(),
        Some("high-priority") => PriorityClass::high_priority(),
        Some("spot") => PriorityClass::spot(),
        Some("preemptible") => PriorityClass::preemptible(),
        _ => PriorityClass::default(),
    };

    let mut candidate = PreemptionCandidate::new(workload_id, priority_class);

    if params.gpus.is_some() || params.memory_gb.is_some() {
        let mut resources = ResourceRequirements::new();
        if let Some(gpus) = params.gpus {
            resources = resources.with_gpus(gpus);
        }
        if let Some(mem) = params.memory_gb {
            resources = resources.with_memory_gb(mem);
        }
        candidate = candidate.with_resources(resources);
    }

    PREEMPTION_MANAGER.register_workload(candidate);

    tracing::info!(workload_id = %params.workload_id, "workload registered for preemption");

    to_json(serde_json::json!({ "success": true }))
}

#[derive(Debug, Deserialize)]
pub struct PreemptionRequestParams {
    pub gpus_needed: u32,
    pub memory_gb_needed: Option<u64>,
    pub requester_priority: Option<String>,
}

/// Request preemption to free resources
pub async fn preemption_request(params: Value) -> BridgeResult<Value> {
    let params: PreemptionRequestParams = parse_params(params)?;

    let mut resources = ResourceRequirements::new().with_gpus(params.gpus_needed);
    if let Some(mem) = params.memory_gb_needed {
        resources = resources.with_memory_gb(mem);
    }

    let priority_class = match params.requester_priority.as_deref() {
        Some("system-critical") => PriorityClass::system_critical(),
        Some("high-priority") => PriorityClass::high_priority(),
        Some("spot") => PriorityClass::spot(),
        Some("preemptible") => PriorityClass::preemptible(),
        _ => PriorityClass::default(),
    };

    let request = PreemptionRequest::new(resources, priority_class);

    let result = PREEMPTION_MANAGER
        .request_preemption(&request)
        .map_err(|e| BridgeError::Internal(format!("preemption failed: {e}")))?;

    to_json(serde_json::json!({
        "evicted_count": result.evicted_workloads.len(),
        "freed_gpus": result.freed_resources.gpus,
        "freed_memory_bytes": result.freed_resources.memory_bytes,
        "total_cost": result.total_cost,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PreemptionListParams {}

/// List preemptible workloads
pub async fn preemption_list(_params: Value) -> BridgeResult<Value> {
    let workloads = PREEMPTION_MANAGER.preemptible_workloads();

    let infos: Vec<PreemptionCandidateInfo> = workloads
        .iter()
        .map(|w| PreemptionCandidateInfo {
            workload_id: w.workload_id.to_string(),
            priority: w.priority_class.value,
            priority_class: w.priority_class.name.clone(),
            gpus: w.resources.gpus,
        })
        .collect();

    to_json(infos)
}

// ─────────────────────────────────────────────────────────────
// Rollback Handlers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RollbackRecordParams {
    pub deployment_id: String,
    pub name: String,
    pub image: String,
}

/// Record a deployment snapshot for rollback
pub async fn rollback_record(params: Value) -> BridgeResult<Value> {
    let params: RollbackRecordParams = parse_params(params)?;

    let deployment_id = DeploymentId::new(&params.deployment_id);
    let spec = DeploymentSpec::new(&params.name, &params.image);
    let snapshot = DeploymentSnapshot::new(deployment_id.clone(), spec);

    let mut history = ROLLBACK_HISTORY.write();
    history.record(snapshot);

    tracing::info!(deployment_id = %params.deployment_id, "deployment recorded for rollback");

    to_json(serde_json::json!({ "success": true, "deployment_id": params.deployment_id }))
}

#[derive(Debug, Deserialize)]
pub struct RollbackPlanParams {
    pub deployment_id: String,
    pub target_version: Option<String>,
}

/// Plan a rollback
pub async fn rollback_plan(params: Value) -> BridgeResult<Value> {
    let params: RollbackPlanParams = parse_params(params)?;

    let deployment_id = DeploymentId::new(&params.deployment_id);

    let history = ROLLBACK_HISTORY.read();
    let executor = RollbackExecutor::new(history.clone());

    let target = params.target_version.map(|v| DeploymentId::new(&v));

    let plan = executor
        .plan_rollback(&deployment_id, target.as_ref())
        .map_err(|e| BridgeError::Internal(format!("failed to plan rollback: {e}")))?;

    to_json(serde_json::json!({
        "id": plan.id.to_string(),
        "from_name": plan.from.spec.name,
        "to_name": plan.to.spec.name,
        "strategy": format!("{:?}", plan.strategy),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RollbackHistoryParams {
    pub limit: Option<usize>,
}

/// Get rollback history
pub async fn rollback_history(params: Value) -> BridgeResult<Value> {
    let params: RollbackHistoryParams = parse_params(params)?;

    let history = ROLLBACK_HISTORY.read();
    let snapshots = history.list_recent(params.limit.unwrap_or(20));

    let infos: Vec<_> = snapshots
        .iter()
        .map(|s| {
            serde_json::json!({
                "deployment_id": s.id.to_string(),
                "name": s.spec.name,
                "image": s.spec.image,
                "timestamp": s.timestamp.timestamp_millis(),
            })
        })
        .collect();

    to_json(infos)
}

#[derive(Debug, Deserialize)]
pub struct RollbackTriggerCheckParams {
    pub error_rate: Option<f64>,
    pub p99_latency_ms: Option<f64>,
}

/// Check if rollback should be triggered based on metrics
pub async fn rollback_trigger_check(params: Value) -> BridgeResult<Value> {
    let params: RollbackTriggerCheckParams = parse_params(params)?;

    let evaluator = TriggerEvaluator::new();
    let triggers = claw_rollback::DefaultTriggers::all(100.0); // 100ms baseline

    let mut metrics = Metrics::new();
    if let Some(err) = params.error_rate {
        metrics = metrics.with_error_rate(err);
    }
    if let Some(lat) = params.p99_latency_ms {
        metrics = metrics.with_p99_latency_ms(lat);
    }

    let triggered = evaluator.evaluate_all(&triggers, &metrics);

    to_json(serde_json::json!({
        "should_rollback": triggered.is_some(),
        "trigger_reason": triggered.map(|t| t.description()),
    }))
}
