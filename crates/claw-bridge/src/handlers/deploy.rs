//! Deployment handlers
//!
//! These handlers integrate with claw-deploy for intent-based deployments.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use claw_deploy::{
    parse_intent, DeploymentExecutor, DeploymentId, DeploymentIntent, DeploymentState,
    StrategyHint,
};

use crate::error::{BridgeError, BridgeResult};
use crate::handlers::{parse_params, to_json};

// ─────────────────────────────────────────────────────────────
// Global State
// ─────────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    static ref DEPLOY_EXECUTOR: Arc<RwLock<DeploymentExecutor>> = Arc::new(RwLock::new(DeploymentExecutor::new()));
}

// ─────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentInfo {
    pub id: String,
    pub image: String,
    pub state: String,
    pub strategy: String,
    pub healthy_replicas: u32,
    pub total_replicas: u32,
    pub message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ─────────────────────────────────────────────────────────────
// Deploy Intent (Natural Language)
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeployIntentParams {
    /// Natural language deployment command OR image name
    pub intent: String,
    /// Optional: number of replicas (overrides parsed intent)
    pub replicas: Option<u32>,
    /// Optional: GPUs per replica (overrides parsed intent)
    pub gpus: Option<u32>,
    /// Optional: strategy hint (canary, blue-green, rolling, immediate)
    pub strategy: Option<String>,
    /// Optional: canary percentage (for canary strategy)
    pub canary_percentage: Option<u8>,
}

/// Start a deployment from natural language intent
pub async fn deploy_intent(params: Value) -> BridgeResult<Value> {
    let params: DeployIntentParams = parse_params(params)?;

    // Try to parse as natural language first
    let mut intent = match parse_intent(&params.intent) {
        Ok(parsed) => parsed,
        Err(_) => {
            // Fall back to treating it as just an image name
            DeploymentIntent::new(&params.intent)
        }
    };

    // Apply overrides
    if let Some(replicas) = params.replicas {
        intent = intent.with_replicas(replicas);
    }
    if let Some(gpus) = params.gpus {
        intent = intent.with_gpus(gpus);
    }

    // Parse strategy hint
    if let Some(strategy_str) = &params.strategy {
        let hint = parse_strategy_hint(strategy_str, params.canary_percentage)?;
        intent = intent.with_strategy_hint(hint);
    }

    // Validate
    intent
        .validate()
        .map_err(|e| BridgeError::InvalidParams(format!("invalid deployment intent: {e}")))?;

    // Start deployment
    let executor = DEPLOY_EXECUTOR.write();
    let deployment_id = executor
        .start(&intent)
        .map_err(|e| BridgeError::Internal(format!("failed to start deployment: {e}")))?;

    // Get status
    let status = executor
        .get_status(&deployment_id)
        .map_err(|e| BridgeError::Internal(format!("failed to get status: {e}")))?;

    tracing::info!(
        deployment_id = %deployment_id,
        image = %intent.image,
        replicas = intent.replicas,
        "deployment started"
    );

    let info = DeploymentInfo {
        id: deployment_id.to_string(),
        image: status.image.clone(),
        state: format!("{:?}", status.state).to_lowercase(),
        strategy: format!("{:?}", status.strategy),
        healthy_replicas: status.healthy_replicas,
        total_replicas: status.total_replicas,
        message: status.message.clone(),
        created_at: status.created_at.timestamp_millis(),
        updated_at: status.updated_at.timestamp_millis(),
    };

    to_json(info)
}

// ─────────────────────────────────────────────────────────────
// Deployment Status
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeployStatusParams {
    pub deployment_id: String,
}

/// Get deployment status
pub async fn deploy_status(params: Value) -> BridgeResult<Value> {
    let params: DeployStatusParams = parse_params(params)?;

    let deployment_id = DeploymentId::parse(&params.deployment_id)
        .map_err(|_| BridgeError::InvalidParams("invalid deployment_id".to_string()))?;

    let executor = DEPLOY_EXECUTOR.read();
    let status = executor
        .get_status(&deployment_id)
        .map_err(|e| BridgeError::NotFound(format!("deployment not found: {e}")))?;

    let info = DeploymentInfo {
        id: deployment_id.to_string(),
        image: status.image.clone(),
        state: format!("{:?}", status.state).to_lowercase(),
        strategy: format!("{:?}", status.strategy),
        healthy_replicas: status.healthy_replicas,
        total_replicas: status.total_replicas,
        message: status.message.clone(),
        created_at: status.created_at.timestamp_millis(),
        updated_at: status.updated_at.timestamp_millis(),
    };

    to_json(info)
}

// ─────────────────────────────────────────────────────────────
// Deployment List
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeployListParams {
    pub state: Option<String>,
    pub limit: Option<u32>,
}

/// List deployments
pub async fn deploy_list(params: Value) -> BridgeResult<Value> {
    let params: DeployListParams = parse_params(params)?;

    let executor = DEPLOY_EXECUTOR.read();

    let deployments = if let Some(state_str) = &params.state {
        let state = parse_deployment_state(state_str)?;
        executor
            .list_by_state(state)
            .map_err(|e| BridgeError::Internal(format!("failed to list: {e}")))?
    } else {
        executor
            .list()
            .map_err(|e| BridgeError::Internal(format!("failed to list: {e}")))?
    };

    let limit = params.limit.unwrap_or(100) as usize;

    let infos: Vec<DeploymentInfo> = deployments
        .into_iter()
        .take(limit)
        .map(|(id, status)| DeploymentInfo {
            id: id.to_string(),
            image: status.image.clone(),
            state: format!("{:?}", status.state).to_lowercase(),
            strategy: format!("{:?}", status.strategy),
            healthy_replicas: status.healthy_replicas,
            total_replicas: status.total_replicas,
            message: status.message.clone(),
            created_at: status.created_at.timestamp_millis(),
            updated_at: status.updated_at.timestamp_millis(),
        })
        .collect();

    to_json(infos)
}

// ─────────────────────────────────────────────────────────────
// Promote & Rollback
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeployPromoteParams {
    pub deployment_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeployActionResult {
    pub success: bool,
    pub new_state: String,
}

/// Promote a canary deployment to full
pub async fn deploy_promote(params: Value) -> BridgeResult<Value> {
    let params: DeployPromoteParams = parse_params(params)?;

    let deployment_id = DeploymentId::parse(&params.deployment_id)
        .map_err(|_| BridgeError::InvalidParams("invalid deployment_id".to_string()))?;

    let executor = DEPLOY_EXECUTOR.write();
    executor
        .promote(&deployment_id)
        .map_err(|e| BridgeError::Internal(format!("failed to promote: {e}")))?;

    let status = executor
        .get_status(&deployment_id)
        .map_err(|e| BridgeError::Internal(format!("failed to get status: {e}")))?;

    tracing::info!(deployment_id = %deployment_id, "deployment promoted");

    to_json(DeployActionResult {
        success: true,
        new_state: format!("{:?}", status.state).to_lowercase(),
    })
}

#[derive(Debug, Deserialize)]
pub struct DeployRollbackParams {
    pub deployment_id: String,
    pub reason: Option<String>,
}

/// Rollback a deployment
pub async fn deploy_rollback(params: Value) -> BridgeResult<Value> {
    let params: DeployRollbackParams = parse_params(params)?;

    let deployment_id = DeploymentId::parse(&params.deployment_id)
        .map_err(|_| BridgeError::InvalidParams("invalid deployment_id".to_string()))?;

    let reason = params.reason.as_deref().unwrap_or("manual rollback");

    let executor = DEPLOY_EXECUTOR.write();
    executor
        .rollback(&deployment_id, reason)
        .map_err(|e| BridgeError::Internal(format!("failed to rollback: {e}")))?;

    let status = executor
        .get_status(&deployment_id)
        .map_err(|e| BridgeError::Internal(format!("failed to get status: {e}")))?;

    tracing::info!(deployment_id = %deployment_id, reason = %reason, "deployment rolled back");

    to_json(DeployActionResult {
        success: true,
        new_state: format!("{:?}", status.state).to_lowercase(),
    })
}

#[derive(Debug, Deserialize)]
pub struct DeployAbortParams {
    pub deployment_id: String,
}

/// Abort a deployment in progress
pub async fn deploy_abort(params: Value) -> BridgeResult<Value> {
    let params: DeployAbortParams = parse_params(params)?;

    let deployment_id = DeploymentId::parse(&params.deployment_id)
        .map_err(|_| BridgeError::InvalidParams("invalid deployment_id".to_string()))?;

    let executor = DEPLOY_EXECUTOR.write();
    executor
        .rollback(&deployment_id, "deployment aborted")
        .map_err(|e| BridgeError::Internal(format!("failed to abort: {e}")))?;

    tracing::info!(deployment_id = %deployment_id, "deployment aborted");

    to_json(DeployActionResult {
        success: true,
        new_state: "aborted".to_string(),
    })
}

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

fn parse_strategy_hint(s: &str, canary_pct: Option<u8>) -> BridgeResult<StrategyHint> {
    match s.to_lowercase().as_str() {
        "immediate" => Ok(StrategyHint::Immediate),
        "canary" => Ok(StrategyHint::Canary {
            percentage: canary_pct.unwrap_or(10),
        }),
        "blue-green" | "bluegreen" => Ok(StrategyHint::BlueGreen),
        "rolling" => Ok(StrategyHint::Rolling { batch_size: 1 }),
        _ => Err(BridgeError::InvalidParams(format!(
            "unknown strategy: {s}. Use: immediate, canary, blue-green, rolling"
        ))),
    }
}

fn parse_deployment_state(s: &str) -> BridgeResult<DeploymentState> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(DeploymentState::Pending),
        "deploying" => Ok(DeploymentState::Deploying),
        "canary" => Ok(DeploymentState::Canary),
        "promoting" => Ok(DeploymentState::Promoting),
        "rollingback" | "rolling_back" => Ok(DeploymentState::RollingBack),
        "complete" | "completed" => Ok(DeploymentState::Complete),
        "failed" => Ok(DeploymentState::Failed),
        _ => Err(BridgeError::InvalidParams(format!(
            "unknown deployment state: {s}"
        ))),
    }
}
