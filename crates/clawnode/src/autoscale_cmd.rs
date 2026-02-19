//! Autoscaling policy command handlers
//!
//! Manages autoscaling policies using AutoscaleStore.
//! Policies define min/max replicas and scaling triggers for deployments.

use crate::commands::{CommandError, CommandRequest};
use crate::persist::AutoscaleRecord;
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

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
    name: String,
    target: String,
    #[serde(rename = "minReplicas", default = "default_min")]
    min_replicas: u32,
    #[serde(rename = "maxReplicas", default = "default_max")]
    max_replicas: u32,
    /// "target_utilization", "queue_depth", "schedule"
    #[serde(rename = "policyType", default = "default_policy_type")]
    policy_type: String,
    metric: Option<String>,
    threshold: Option<f64>,
}

fn default_min() -> u32 { 1 }
fn default_max() -> u32 { 10 }
fn default_policy_type() -> String { "target_utilization".to_string() }

async fn handle_autoscale_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleCreateParams = serde_json::from_value(params)?;

    info!(
        name = %params.name,
        target = %params.target,
        min = params.min_replicas,
        max = params.max_replicas,
        "creating autoscale policy"
    );

    if params.min_replicas > params.max_replicas {
        return Err("minReplicas cannot exceed maxReplicas".into());
    }

    let now = chrono::Utc::now();
    let record = AutoscaleRecord {
        name: params.name.clone(),
        target: params.target.clone(),
        min_replicas: params.min_replicas,
        max_replicas: params.max_replicas,
        current_replicas: params.min_replicas,
        policy_type: params.policy_type.clone(),
        metric: params.metric.clone(),
        threshold: params.threshold,
        state: "active".to_string(),
        created_at: now,
        updated_at: now,
    };

    state
        .autoscale_store
        .write()
        .await
        .create(record)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "target": params.target,
        "minReplicas": params.min_replicas,
        "maxReplicas": params.max_replicas,
        "policyType": params.policy_type,
        "state": "active",
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct AutoscaleNameParams {
    name: String,
}

async fn handle_autoscale_status(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleNameParams = serde_json::from_value(params)?;

    let store = state.autoscale_store.read().await;
    let record = store
        .get(&params.name)
        .ok_or_else(|| format!("autoscale policy '{}' not found", params.name))?;

    Ok(json!({
        "name": record.name,
        "target": record.target,
        "minReplicas": record.min_replicas,
        "maxReplicas": record.max_replicas,
        "currentReplicas": record.current_replicas,
        "policyType": record.policy_type,
        "metric": record.metric,
        "threshold": record.threshold,
        "state": record.state,
        "createdAt": record.created_at.to_rfc3339(),
        "updatedAt": record.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct AutoscaleAdjustParams {
    name: String,
    replicas: Option<u32>,
    #[serde(rename = "minReplicas")]
    min_replicas: Option<u32>,
    #[serde(rename = "maxReplicas")]
    max_replicas: Option<u32>,
    enabled: Option<bool>,
}

async fn handle_autoscale_adjust(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleAdjustParams = serde_json::from_value(params)?;

    info!(name = %params.name, "adjusting autoscale policy");

    let mut store = state.autoscale_store.write().await;
    let record = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("autoscale policy '{}' not found", params.name))?;

    if let Some(min) = params.min_replicas {
        record.min_replicas = min;
    }
    if let Some(max) = params.max_replicas {
        record.max_replicas = max;
    }
    if let Some(replicas) = params.replicas {
        record.current_replicas = replicas.clamp(record.min_replicas, record.max_replicas);
    }
    if let Some(enabled) = params.enabled {
        record.state = if enabled { "active" } else { "disabled" }.to_string();
    }
    record.updated_at = chrono::Utc::now();
    store.update(&params.name);

    Ok(json!({
        "name": params.name,
        "adjusted": true,
        "success": true,
    }))
}

async fn handle_autoscale_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: AutoscaleNameParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting autoscale policy");

    let deleted = state.autoscale_store.write().await.delete(&params.name);
    if deleted.is_none() {
        return Err(format!("autoscale policy '{}' not found", params.name).into());
    }

    Ok(json!({
        "name": params.name,
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
    async fn test_autoscale_create() {
        let state = test_state();
        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({
                    "name": "gpu-scaler",
                    "target": "my-app",
                    "minReplicas": 1,
                    "maxReplicas": 8,
                    "policyType": "target_utilization",
                    "metric": "gpu_utilization",
                    "threshold": 80.0
                }),
            },
        )
        .await
        .expect("create");

        assert_eq!(result["success"], true);
        assert_eq!(result["name"], "gpu-scaler");
    }

    #[tokio::test]
    async fn test_autoscale_status() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({"name": "test", "target": "app", "minReplicas": 2, "maxReplicas": 5}),
            },
        )
        .await
        .expect("create");

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.status".to_string(),
                params: json!({"name": "test"}),
            },
        )
        .await
        .expect("status");

        assert_eq!(result["minReplicas"], 2);
        assert_eq!(result["maxReplicas"], 5);
        assert_eq!(result["currentReplicas"], 2);
        assert_eq!(result["state"], "active");
    }

    #[tokio::test]
    async fn test_autoscale_adjust() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({"name": "adj", "target": "app", "minReplicas": 1, "maxReplicas": 10}),
            },
        )
        .await
        .expect("create");

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.adjust".to_string(),
                params: json!({"name": "adj", "replicas": 5, "maxReplicas": 20}),
            },
        )
        .await
        .expect("adjust");

        assert_eq!(result["adjusted"], true);

        // Verify
        let status = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.status".to_string(),
                params: json!({"name": "adj"}),
            },
        )
        .await
        .expect("status");

        assert_eq!(status["currentReplicas"], 5);
        assert_eq!(status["maxReplicas"], 20);
    }

    #[tokio::test]
    async fn test_autoscale_adjust_clamp() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({"name": "clamp", "target": "app", "minReplicas": 2, "maxReplicas": 5}),
            },
        )
        .await
        .expect("create");

        // Try to set replicas beyond max
        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.adjust".to_string(),
                params: json!({"name": "clamp", "replicas": 100}),
            },
        )
        .await
        .expect("adjust");

        let status = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.status".to_string(),
                params: json!({"name": "clamp"}),
            },
        )
        .await
        .expect("status");

        assert_eq!(status["currentReplicas"], 5); // Clamped to max
    }

    #[tokio::test]
    async fn test_autoscale_delete() {
        let state = test_state();

        handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({"name": "del-me", "target": "app"}),
            },
        )
        .await
        .expect("create");

        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.delete".to_string(),
                params: json!({"name": "del-me"}),
            },
        )
        .await
        .expect("delete");

        assert_eq!(result["deleted"], true);

        // Verify gone
        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.status".to_string(),
                params: json!({"name": "del-me"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_autoscale_min_exceeds_max() {
        let state = test_state();
        let result = handle_autoscale_command(
            &state,
            CommandRequest {
                command: "autoscale.create".to_string(),
                params: json!({"name": "bad", "target": "app", "minReplicas": 10, "maxReplicas": 5}),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
