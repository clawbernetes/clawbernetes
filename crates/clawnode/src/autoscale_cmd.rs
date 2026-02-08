//! Autoscaling command handlers
//!
//! Provides 4 commands (requires `autoscaler` feature):
//! `autoscale.create`, `autoscale.status`, `autoscale.adjust`, `autoscale.delete`

use crate::commands::{CommandError, CommandRequest};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route an autoscale.* command.
pub async fn handle_autoscale_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "autoscale.create" => handle_autoscale_create(state, request.params).await,
        "autoscale.status" => handle_autoscale_status(state, request.params).await,
        "autoscale.adjust" => handle_autoscale_adjust(state, request.params).await,
        "autoscale.delete" => handle_autoscale_delete(state, request.params).await,
        _ => Err(format!("unknown autoscale command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct AutoscaleCreateParams {
    target: String,
    #[serde(rename = "minReplicas", default = "default_min")]
    min_replicas: u32,
    #[serde(rename = "maxReplicas", default = "default_max")]
    max_replicas: u32,
    #[serde(default = "default_policy")]
    policy: String,
    // Policy-specific params
    #[serde(rename = "targetUtilization")]
    target_utilization: Option<f64>,
    tolerance: Option<f64>,
    #[serde(rename = "targetQueueDepth")]
    target_queue_depth: Option<u32>,
    #[serde(rename = "upThreshold")]
    up_threshold: Option<u32>,
    #[serde(rename = "downThreshold")]
    down_threshold: Option<u32>,
}

fn default_min() -> u32 {
    1
}

fn default_max() -> u32 {
    10
}

fn default_policy() -> String {
    "target_utilization".to_string()
}

async fn handle_autoscale_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleCreateParams = serde_json::from_value(params)?;

    info!(
        target = %params.target,
        min = params.min_replicas,
        max = params.max_replicas,
        policy = %params.policy,
        "creating autoscaler"
    );

    let policy_type = match params.policy.as_str() {
        "target_utilization" => {
            let target = params.target_utilization.unwrap_or(70.0);
            let tolerance = params.tolerance.unwrap_or(10.0);
            claw_autoscaler::ScalingPolicyType::TargetUtilization {
                target_percent: target,
                tolerance_percent: tolerance,
            }
        }
        "queue_depth" => claw_autoscaler::ScalingPolicyType::QueueDepth {
            target_jobs_per_node: params.target_queue_depth.unwrap_or(5),
            scale_up_threshold: params.up_threshold.unwrap_or(10),
            scale_down_threshold: params.down_threshold.unwrap_or(2),
        },
        other => {
            return Err(format!(
                "unknown policy: {other} (use target_utilization/queue_depth)"
            )
            .into())
        }
    };

    let policy_name = format!("{}-policy", params.target);
    let policy = claw_autoscaler::ScalingPolicy::builder(&policy_name, &policy_name)
        .min_nodes(params.min_replicas)
        .max_nodes(params.max_replicas)
        .policy_type(policy_type)
        .build()
        .map_err(|e| format!("invalid policy: {e}"))?;

    let pool =
        claw_autoscaler::NodePool::new(&params.target, &params.target, policy)
            .map_err(|e| format!("invalid pool: {e}"))?;

    state
        .autoscaler_manager
        .register_pool(pool)
        .map_err(|e| format!("register failed: {e}"))?;

    Ok(json!({
        "target": params.target,
        "minReplicas": params.min_replicas,
        "maxReplicas": params.max_replicas,
        "policy": params.policy,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct AutoscaleIdentifyParams {
    target: String,
}

async fn handle_autoscale_status(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleIdentifyParams = serde_json::from_value(params)?;

    let pool_id = claw_autoscaler::PoolId::new(&params.target);
    let pool = state
        .autoscaler_manager
        .get_pool(&pool_id)
        .ok_or_else(|| format!("autoscaler '{}' not found", params.target))?;

    let status = state.autoscaler_manager.status();

    Ok(json!({
        "target": params.target,
        "currentNodes": pool.node_count(),
        "totalGpus": pool.total_gpu_count(),
        "minNodes": pool.policy.min_nodes,
        "maxNodes": pool.policy.max_nodes,
        "enabled": status.enabled,
        "pendingActions": status.pending_actions,
    }))
}

#[derive(Debug, Deserialize)]
struct AutoscaleAdjustParams {
    target: String,
    #[serde(rename = "minReplicas")]
    min_replicas: Option<u32>,
    #[serde(rename = "maxReplicas")]
    max_replicas: Option<u32>,
    policy: Option<String>,
    #[serde(rename = "targetUtilization")]
    target_utilization: Option<f64>,
    tolerance: Option<f64>,
}

async fn handle_autoscale_adjust(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleAdjustParams = serde_json::from_value(params)?;

    info!(target = %params.target, "adjusting autoscaler");

    let pool_id = claw_autoscaler::PoolId::new(&params.target);

    // Get current pool to merge params
    let current = state
        .autoscaler_manager
        .get_pool(&pool_id)
        .ok_or_else(|| format!("autoscaler '{}' not found", params.target))?;

    let min = params.min_replicas.unwrap_or(current.policy.min_nodes);
    let max = params.max_replicas.unwrap_or(current.policy.max_nodes);

    let target = params.target_utilization.unwrap_or(70.0);
    let tolerance = params.tolerance.unwrap_or(10.0);

    let policy_name = format!("{}-policy", params.target);
    let new_policy = claw_autoscaler::ScalingPolicy::builder(&policy_name, &policy_name)
        .min_nodes(min)
        .max_nodes(max)
        .target_utilization(target, tolerance)
        .build()
        .map_err(|e| format!("invalid policy: {e}"))?;

    state
        .autoscaler_manager
        .set_policy(&pool_id, new_policy)
        .map_err(|e| format!("set policy failed: {e}"))?;

    Ok(json!({
        "target": params.target,
        "minReplicas": min,
        "maxReplicas": max,
        "success": true,
    }))
}

async fn handle_autoscale_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleIdentifyParams = serde_json::from_value(params)?;

    info!(target = %params.target, "deleting autoscaler");

    let pool_id = claw_autoscaler::PoolId::new(&params.target);

    state
        .autoscaler_manager
        .unregister_pool(&pool_id)
        .map_err(|e| format!("unregister failed: {e}"))?;

    Ok(json!({
        "target": params.target,
        "deleted": true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NodeConfig;

    fn test_state() -> SharedState {
        let mut config = NodeConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        config.state_path = dir.path().to_path_buf();
        std::mem::forget(dir);
        SharedState::new(config)
    }

    #[tokio::test]
    async fn test_autoscale_create_and_status() {
        let state = test_state();

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({
                    "target": "gpu-pool",
                    "minReplicas": 2,
                    "maxReplicas": 20,
                    "policy": "target_utilization",
                    "targetUtilization": 70.0,
                    "tolerance": 10.0,
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.status".to_string(),
                params: json!({"target": "gpu-pool"}),
            },
        )
        .await
        .expect("status");
        assert_eq!(result["minNodes"], 2);
        assert_eq!(result["maxNodes"], 20);
    }

    #[tokio::test]
    async fn test_autoscale_adjust() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({
                    "target": "adjust-pool",
                    "minReplicas": 1,
                    "maxReplicas": 10,
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.adjust".to_string(),
                params: json!({
                    "target": "adjust-pool",
                    "minReplicas": 3,
                    "maxReplicas": 15,
                }),
            },
        )
        .await
        .expect("adjust");
        assert_eq!(result["minReplicas"], 3);
        assert_eq!(result["maxReplicas"], 15);
    }

    #[tokio::test]
    async fn test_autoscale_delete() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({
                    "target": "del-pool",
                    "minReplicas": 1,
                    "maxReplicas": 5,
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.delete".to_string(),
                params: json!({"target": "del-pool"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);
    }
}
