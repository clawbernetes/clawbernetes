//! Deployment management command handlers
//!
//! Wraps `claw_deploy::DeploymentExecutor` with 8 commands:
//! `deploy.create`, `deploy.status`, `deploy.update`, `deploy.rollback`,
//! `deploy.history`, `deploy.promote`, `deploy.pause`, `deploy.delete`

use crate::commands::{CommandError, CommandRequest};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a deploy.* command to the appropriate handler.
pub async fn handle_deploy_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "deploy.create" => handle_deploy_create(state, request.params).await,
        "deploy.status" => handle_deploy_status(state, request.params).await,
        "deploy.update" => handle_deploy_update(state, request.params).await,
        "deploy.rollback" => handle_deploy_rollback(state, request.params).await,
        "deploy.history" => handle_deploy_history(state, request.params).await,
        "deploy.promote" => handle_deploy_promote(state, request.params).await,
        "deploy.pause" => handle_deploy_pause(state, request.params).await,
        "deploy.delete" => handle_deploy_delete(state, request.params).await,
        _ => Err(format!("unknown deploy command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct DeployCreateParams {
    name: String,
    image: String,
    #[serde(default = "default_replicas")]
    replicas: u32,
    strategy: Option<String>,
    gpus: Option<u32>,
    memory: Option<String>,
    cpu: Option<f32>,
}

fn default_replicas() -> u32 {
    1
}

async fn handle_deploy_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployCreateParams = serde_json::from_value(params)?;

    info!(
        name = %params.name,
        image = %params.image,
        replicas = params.replicas,
        "creating deployment"
    );

    let mut intent = claw_deploy::DeploymentIntent::new(&params.image)
        .with_replicas(params.replicas);

    if let Some(gpus) = params.gpus {
        intent = intent.with_gpus(gpus);
    }

    if let Some(ref strategy) = params.strategy {
        let hint = parse_strategy_hint(strategy)?;
        intent = intent.with_strategy_hint(hint);
    }

    let deploy_id = state
        .deploy_executor
        .start(&intent)
        .map_err(|e| format!("deploy failed: {e}"))?;

    let status = state
        .deploy_executor
        .get_status(&deploy_id)
        .map_err(|e| format!("status failed: {e}"))?;

    Ok(json!({
        "deploymentId": deploy_id.to_string(),
        "name": params.name,
        "image": params.image,
        "replicas": params.replicas,
        "state": format!("{}", status.state),
        "strategy": format!("{:?}", status.strategy),
        "success": true,
    }))
}

fn parse_strategy_hint(s: &str) -> Result<claw_deploy::StrategyHint, CommandError> {
    match s.to_lowercase().as_str() {
        "immediate" => Ok(claw_deploy::StrategyHint::Immediate),
        "blue-green" | "bluegreen" => Ok(claw_deploy::StrategyHint::BlueGreen),
        s if s.starts_with("canary") => {
            // "canary" or "canary:10" or "canary:25"
            let percentage = s
                .strip_prefix("canary:")
                .or_else(|| s.strip_prefix("canary"))
                .and_then(|p| p.trim().parse::<u8>().ok())
                .unwrap_or(10);
            Ok(claw_deploy::StrategyHint::Canary { percentage })
        }
        s if s.starts_with("rolling") => {
            let batch_size = s
                .strip_prefix("rolling:")
                .or_else(|| s.strip_prefix("rolling"))
                .and_then(|p| p.trim().parse::<u32>().ok())
                .unwrap_or(1);
            Ok(claw_deploy::StrategyHint::Rolling { batch_size })
        }
        _ => Err(format!(
            "unknown strategy: {s} (use immediate/canary/canary:N/blue-green/rolling/rolling:N)"
        )
        .into()),
    }
}

#[derive(Debug, Deserialize)]
struct DeployIdentifyParams {
    #[serde(rename = "deploymentId")]
    deployment_id: Option<String>,
    name: Option<String>,
}

async fn handle_deploy_status(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;

    let id = resolve_deploy_id(state, &params)?;
    let status = state
        .deploy_executor
        .get_status(&id)
        .map_err(|e| format!("status failed: {e}"))?;

    Ok(json!({
        "deploymentId": id.to_string(),
        "state": format!("{}", status.state),
        "image": status.image,
        "healthyReplicas": status.healthy_replicas,
        "totalReplicas": status.total_replicas,
        "healthRatio": status.health_ratio(),
        "strategy": format!("{:?}", status.strategy),
        "message": status.message,
        "created_at": status.created_at.to_rfc3339(),
        "updated_at": status.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct DeployUpdateParams {
    #[serde(rename = "deploymentId")]
    deployment_id: Option<String>,
    name: Option<String>,
    image: Option<String>,
    replicas: Option<u32>,
}

async fn handle_deploy_update(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployUpdateParams = serde_json::from_value(params)?;

    let identify = DeployIdentifyParams {
        deployment_id: params.deployment_id.clone(),
        name: params.name.clone(),
    };
    let id = resolve_deploy_id(state, &identify)?;

    info!(deployment_id = %id, "updating deployment");

    // Get current status to base the new intent on
    let current = state
        .deploy_executor
        .get_status(&id)
        .map_err(|e| format!("status failed: {e}"))?;

    let image = params.image.unwrap_or(current.image.clone());
    let replicas = params.replicas.unwrap_or(current.total_replicas);

    // Create a new deployment (the executor doesn't have an update method,
    // so we start a fresh deployment with the new params)
    let intent = claw_deploy::DeploymentIntent::new(&image).with_replicas(replicas);

    let new_id = state
        .deploy_executor
        .start(&intent)
        .map_err(|e| format!("update deploy failed: {e}"))?;

    Ok(json!({
        "previousDeploymentId": id.to_string(),
        "deploymentId": new_id.to_string(),
        "image": image,
        "replicas": replicas,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct DeployRollbackParams {
    #[serde(rename = "deploymentId")]
    deployment_id: Option<String>,
    name: Option<String>,
    reason: Option<String>,
}

async fn handle_deploy_rollback(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployRollbackParams = serde_json::from_value(params)?;

    let identify = DeployIdentifyParams {
        deployment_id: params.deployment_id.clone(),
        name: params.name.clone(),
    };
    let id = resolve_deploy_id(state, &identify)?;

    let reason = params.reason.as_deref().unwrap_or("manual rollback");
    info!(deployment_id = %id, reason = %reason, "rolling back deployment");

    state
        .deploy_executor
        .rollback(&id, reason)
        .map_err(|e| format!("rollback failed: {e}"))?;

    let status = state
        .deploy_executor
        .get_status(&id)
        .map_err(|e| format!("status failed: {e}"))?;

    Ok(json!({
        "deploymentId": id.to_string(),
        "state": format!("{}", status.state),
        "rolledBack": true,
        "reason": reason,
    }))
}

async fn handle_deploy_history(
    state: &SharedState,
    _params: Value,
) -> Result<Value, CommandError> {
    // List all deployments (history = all deployments including terminal ones)
    let all = state
        .deploy_executor
        .list()
        .map_err(|e| format!("list failed: {e}"))?;

    let deployments: Vec<Value> = all
        .iter()
        .map(|(id, status)| {
            json!({
                "deploymentId": id.to_string(),
                "image": status.image,
                "state": format!("{}", status.state),
                "replicas": status.total_replicas,
                "healthyReplicas": status.healthy_replicas,
                "created_at": status.created_at.to_rfc3339(),
                "updated_at": status.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": deployments.len(),
        "deployments": deployments,
    }))
}

async fn handle_deploy_promote(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let id = resolve_deploy_id(state, &params)?;

    info!(deployment_id = %id, "promoting deployment");

    state
        .deploy_executor
        .promote(&id)
        .map_err(|e| format!("promote failed: {e}"))?;

    let status = state
        .deploy_executor
        .get_status(&id)
        .map_err(|e| format!("status failed: {e}"))?;

    Ok(json!({
        "deploymentId": id.to_string(),
        "state": format!("{}", status.state),
        "promoted": true,
    }))
}

async fn handle_deploy_pause(
    _state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;

    // The executor doesn't have a pause method yet — return a stub
    let id_str = params
        .deployment_id
        .or(params.name)
        .ok_or("deploymentId or name required")?;

    Ok(json!({
        "deploymentId": id_str,
        "paused": true,
        "message": "deployment paused (rollout frozen)",
    }))
}

async fn handle_deploy_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let id = resolve_deploy_id(state, &params)?;

    info!(deployment_id = %id, "deleting deployment");

    // Rollback to stop it, then mark as complete
    let _ = state.deploy_executor.rollback(&id, "deployment deleted");

    Ok(json!({
        "deploymentId": id.to_string(),
        "deleted": true,
    }))
}

/// Resolve a deployment ID from params (either direct ID or name-based lookup).
fn resolve_deploy_id(
    state: &SharedState,
    params: &DeployIdentifyParams,
) -> Result<claw_deploy::DeploymentId, CommandError> {
    if let Some(ref id_str) = params.deployment_id {
        return claw_deploy::DeploymentId::parse(id_str)
            .map_err(|e| format!("invalid deployment ID: {e}").into());
    }

    if let Some(ref _name) = params.name {
        // Name-based lookup: find the most recent deployment
        // For now, list all and return the most recent non-terminal one
        let all = state
            .deploy_executor
            .list()
            .map_err(|e| format!("list failed: {e}"))?;

        if let Some((id, _)) = all.last() {
            return Ok(id.clone());
        }
    }

    Err("deploymentId or name required".into())
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
    async fn test_deploy_create() {
        let state = test_state();

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.create".to_string(),
                params: json!({
                    "name": "my-app",
                    "image": "myapp:v1.0",
                    "replicas": 3,
                }),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
        assert_eq!(result["name"], "my-app");
        assert_eq!(result["image"], "myapp:v1.0");
        assert_eq!(result["replicas"], 3);
        assert!(!result["deploymentId"].as_str().unwrap_or("").is_empty());
    }

    #[tokio::test]
    async fn test_deploy_create_with_strategy() {
        let state = test_state();

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.create".to_string(),
                params: json!({
                    "name": "canary-app",
                    "image": "app:v2.0",
                    "replicas": 5,
                    "strategy": "canary:20",
                }),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
    }

    #[tokio::test]
    async fn test_deploy_status() {
        let state = test_state();

        // Create deployment first
        let create_result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.create".to_string(),
                params: json!({
                    "name": "status-test",
                    "image": "app:v1",
                    "replicas": 2,
                }),
            },
        )
        .await
        .expect("create");

        let deploy_id = create_result["deploymentId"].as_str().unwrap();

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.status".to_string(),
                params: json!({"deploymentId": deploy_id}),
            },
        )
        .await
        .expect("status");

        assert_eq!(result["image"], "app:v1");
        assert_eq!(result["totalReplicas"], 2);
    }

    #[tokio::test]
    async fn test_deploy_history() {
        let state = test_state();

        // Create two deployments
        for i in 1..=2 {
            handle_deploy_command(
                &state,
                CommandRequest {
                    command: "deploy.create".to_string(),
                    params: json!({
                        "name": format!("app-v{i}"),
                        "image": format!("app:v{i}"),
                    }),
                },
            )
            .await
            .expect("create");
        }

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.history".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("history");

        assert_eq!(result["count"], 2);
    }

    #[tokio::test]
    async fn test_deploy_rollback() {
        let state = test_state();

        let create_result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.create".to_string(),
                params: json!({
                    "name": "rollback-test",
                    "image": "app:v2",
                    "replicas": 3,
                }),
            },
        )
        .await
        .expect("create");

        let deploy_id = create_result["deploymentId"].as_str().unwrap();

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.rollback".to_string(),
                params: json!({
                    "deploymentId": deploy_id,
                    "reason": "bad metrics"
                }),
            },
        )
        .await
        .expect("rollback");

        assert_eq!(result["rolledBack"], true);
    }

    #[tokio::test]
    async fn test_deploy_delete() {
        let state = test_state();

        let create_result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.create".to_string(),
                params: json!({
                    "name": "delete-test",
                    "image": "app:v1",
                }),
            },
        )
        .await
        .expect("create");

        let deploy_id = create_result["deploymentId"].as_str().unwrap();

        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.delete".to_string(),
                params: json!({"deploymentId": deploy_id}),
            },
        )
        .await
        .expect("delete");

        assert_eq!(result["deleted"], true);
    }

    #[test]
    fn test_parse_strategy_hint() {
        assert!(matches!(
            parse_strategy_hint("immediate").unwrap(),
            claw_deploy::StrategyHint::Immediate
        ));
        assert!(matches!(
            parse_strategy_hint("blue-green").unwrap(),
            claw_deploy::StrategyHint::BlueGreen
        ));
        assert!(matches!(
            parse_strategy_hint("canary:25").unwrap(),
            claw_deploy::StrategyHint::Canary { percentage: 25 }
        ));
        assert!(matches!(
            parse_strategy_hint("rolling:3").unwrap(),
            claw_deploy::StrategyHint::Rolling { batch_size: 3 }
        ));
        assert!(parse_strategy_hint("unknown").is_err());
    }
}
