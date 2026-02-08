//! Policy and governance command handlers
//!
//! Provides 3 commands (always available):
//! `policy.create`, `policy.validate`, `policy.list`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::PolicyEntry;
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a policy.* command.
pub async fn handle_policy_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "policy.create" => handle_policy_create(state, request.params).await,
        "policy.validate" => handle_policy_validate(state, request.params).await,
        "policy.list" => handle_policy_list(state).await,
        _ => Err(format!("unknown policy command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct PolicyCreateParams {
    name: String,
    #[serde(rename = "type")]
    policy_type: String,
    #[serde(default)]
    rules: Vec<Value>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

async fn handle_policy_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: PolicyCreateParams = serde_json::from_value(params)?;

    let valid_types = [
        "resource-limit",
        "image-whitelist",
        "label-required",
        "custom",
    ];
    if !valid_types.contains(&params.policy_type.as_str()) {
        return Err(format!(
            "unknown policy type: {} (use {})",
            params.policy_type,
            valid_types.join("/")
        )
        .into());
    }

    info!(name = %params.name, policy_type = %params.policy_type, "creating policy");

    let entry = PolicyEntry {
        name: params.name.clone(),
        policy_type: params.policy_type.clone(),
        rules: params.rules,
        enabled: params.enabled,
        created_at: chrono::Utc::now(),
    };

    let mut store = state.policy_store.write().await;
    store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "type": params.policy_type,
        "enabled": params.enabled,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct PolicyValidateParams {
    #[serde(rename = "workloadSpec")]
    workload_spec: Value,
}

async fn handle_policy_validate(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: PolicyValidateParams = serde_json::from_value(params)?;

    let store = state.policy_store.read().await;
    let policies = store.list_enabled();

    let mut violations: Vec<Value> = Vec::new();

    for policy in &policies {
        match policy.policy_type.as_str() {
            "resource-limit" => {
                // Check resource limits in rules
                for rule in &policy.rules {
                    if let Some(max_gpus) = rule.get("maxGpus").and_then(|v| v.as_u64()) {
                        if let Some(spec_gpus) =
                            params.workload_spec.get("gpus").and_then(|v| v.as_u64())
                        {
                            if spec_gpus > max_gpus {
                                violations.push(json!({
                                    "policy": policy.name,
                                    "rule": "resource-limit",
                                    "message": format!("gpus {} exceeds limit {}", spec_gpus, max_gpus),
                                }));
                            }
                        }
                    }
                    if let Some(max_memory) =
                        rule.get("maxMemoryMb").and_then(|v| v.as_u64())
                    {
                        if let Some(spec_memory) = params
                            .workload_spec
                            .get("memoryMb")
                            .and_then(|v| v.as_u64())
                        {
                            if spec_memory > max_memory {
                                violations.push(json!({
                                    "policy": policy.name,
                                    "rule": "resource-limit",
                                    "message": format!("memory {}MB exceeds limit {}MB", spec_memory, max_memory),
                                }));
                            }
                        }
                    }
                }
            }
            "image-whitelist" => {
                if let Some(image) = params.workload_spec.get("image").and_then(|v| v.as_str()) {
                    let allowed: Vec<&str> = policy
                        .rules
                        .iter()
                        .filter_map(|r| r.get("pattern").and_then(|v| v.as_str()))
                        .collect();
                    if !allowed.is_empty()
                        && !allowed.iter().any(|pattern| image.starts_with(pattern))
                    {
                        violations.push(json!({
                            "policy": policy.name,
                            "rule": "image-whitelist",
                            "message": format!("image '{}' not in whitelist", image),
                        }));
                    }
                }
            }
            "label-required" => {
                let labels = params
                    .workload_spec
                    .get("labels")
                    .and_then(|v| v.as_object());
                for rule in &policy.rules {
                    if let Some(required_key) = rule.get("key").and_then(|v| v.as_str()) {
                        let has_label = labels
                            .is_some_and(|l| l.contains_key(required_key));
                        if !has_label {
                            violations.push(json!({
                                "policy": policy.name,
                                "rule": "label-required",
                                "message": format!("missing required label: {}", required_key),
                            }));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let valid = violations.is_empty();

    Ok(json!({
        "valid": valid,
        "violations": violations,
        "policiesChecked": policies.len(),
    }))
}

async fn handle_policy_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.policy_store.read().await;
    let entries: Vec<Value> = store
        .list()
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "type": p.policy_type,
                "enabled": p.enabled,
                "rules": p.rules.len(),
                "created_at": p.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": entries.len(),
        "policies": entries,
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
    async fn test_policy_create_and_list() {
        let state = test_state();

        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.create".to_string(),
                params: json!({
                    "name": "gpu-limit",
                    "type": "resource-limit",
                    "rules": [{"maxGpus": 8}],
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_policy_validate_pass() {
        let state = test_state();

        handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.create".to_string(),
                params: json!({
                    "name": "limits",
                    "type": "resource-limit",
                    "rules": [{"maxGpus": 8}],
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.validate".to_string(),
                params: json!({"workloadSpec": {"image": "test:v1", "gpus": 4}}),
            },
        )
        .await
        .expect("validate");
        assert_eq!(result["valid"], true);
        assert_eq!(result["violations"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_policy_validate_fail() {
        let state = test_state();

        handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.create".to_string(),
                params: json!({
                    "name": "gpu-cap",
                    "type": "resource-limit",
                    "rules": [{"maxGpus": 4}],
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.validate".to_string(),
                params: json!({"workloadSpec": {"image": "test:v1", "gpus": 8}}),
            },
        )
        .await
        .expect("validate");
        assert_eq!(result["valid"], false);
        assert_eq!(result["violations"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_policy_image_whitelist() {
        let state = test_state();

        handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.create".to_string(),
                params: json!({
                    "name": "approved-images",
                    "type": "image-whitelist",
                    "rules": [{"pattern": "registry.example.com/"}, {"pattern": "docker.io/library/"}],
                }),
            },
        )
        .await
        .expect("create");

        // Allowed image
        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.validate".to_string(),
                params: json!({"workloadSpec": {"image": "registry.example.com/app:v1"}}),
            },
        )
        .await
        .expect("validate");
        assert_eq!(result["valid"], true);

        // Blocked image
        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.validate".to_string(),
                params: json!({"workloadSpec": {"image": "evil.com/malware:latest"}}),
            },
        )
        .await
        .expect("validate");
        assert_eq!(result["valid"], false);
    }

    #[tokio::test]
    async fn test_policy_invalid_type() {
        let state = test_state();

        let result = handle_policy_command(
            &state,
            CommandRequest {
                command: "policy.create".to_string(),
                params: json!({"name": "bad", "type": "unknown", "rules": []}),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
