//! Secret management command handlers
//!
//! Wraps `claw-secrets::SecretStore` with 5 commands:
//! `secret.create`, `secret.get`, `secret.delete`, `secret.list`, `secret.rotate`

use crate::commands::{CommandError, CommandRequest};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a secret.* command to the appropriate handler.
pub async fn handle_secret_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "secret.create" => handle_secret_create(state, request.params).await,
        "secret.get" => handle_secret_get(state, request.params).await,
        "secret.delete" => handle_secret_delete(state, request.params).await,
        "secret.list" => handle_secret_list(state, request.params).await,
        "secret.rotate" => handle_secret_rotate(state, request.params).await,
        _ => Err(format!("unknown secret command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct SecretCreateParams {
    name: String,
    data: std::collections::HashMap<String, String>,
    #[serde(default)]
    allowed_workloads: Vec<String>,
}

async fn handle_secret_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "creating secret");

    let id = claw_secrets::SecretId::new(&params.name)
        .map_err(|e| format!("invalid secret name: {e}"))?;

    // Serialize the data map as JSON bytes
    let value_bytes = serde_json::to_vec(&params.data)?;

    let mut policy = claw_secrets::AccessPolicy::new();
    if !params.allowed_workloads.is_empty() {
        policy = claw_secrets::AccessPolicy::allow_workloads(
            params
                .allowed_workloads
                .iter()
                .map(|w| claw_secrets::WorkloadId::new(w))
                .collect(),
        );
    }

    state
        .secret_store
        .put(&id, &value_bytes, policy)
        .map_err(|e| format!("failed to create secret: {e}"))?;

    Ok(json!({
        "name": params.name,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct SecretGetParams {
    name: String,
}

async fn handle_secret_get(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretGetParams = serde_json::from_value(params)?;

    let id = claw_secrets::SecretId::new(&params.name)
        .map_err(|e| format!("invalid secret name: {e}"))?;

    let accessor = claw_secrets::Accessor::Admin("clawnode".to_string());
    let value = state
        .secret_store
        .get(&id, &accessor, "command request")
        .map_err(|e| format!("failed to get secret: {e}"))?;

    // Deserialize JSON bytes back to a map
    let data: std::collections::HashMap<String, String> =
        serde_json::from_slice(value.as_bytes())
            .unwrap_or_else(|_| {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "value".to_string(),
                    String::from_utf8_lossy(value.as_bytes()).to_string(),
                );
                m
            });

    let metadata = state
        .secret_store
        .metadata(&id)
        .map_err(|e| format!("failed to get metadata: {e}"))?;

    Ok(json!({
        "name": params.name,
        "data": data,
        "version": metadata.version,
        "created_at": metadata.created_at.to_rfc3339(),
        "updated_at": metadata.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct SecretDeleteParams {
    name: String,
}

async fn handle_secret_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretDeleteParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting secret");

    let id = claw_secrets::SecretId::new(&params.name)
        .map_err(|e| format!("invalid secret name: {e}"))?;

    state
        .secret_store
        .delete(&id)
        .map_err(|e| format!("failed to delete secret: {e}"))?;

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

#[derive(Debug, Deserialize)]
struct SecretListParams {
    prefix: Option<String>,
}

async fn handle_secret_list(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretListParams =
        serde_json::from_value(params).unwrap_or(SecretListParams { prefix: None });

    let all_ids = state.secret_store.list();

    let filtered: Vec<Value> = all_ids
        .iter()
        .filter(|id| {
            params
                .prefix
                .as_ref()
                .is_none_or(|p| id.as_str().starts_with(p.as_str()))
        })
        .filter_map(|id| {
            let metadata = state.secret_store.metadata(id).ok()?;
            Some(json!({
                "name": id.as_str(),
                "version": metadata.version,
                "created_at": metadata.created_at.to_rfc3339(),
                "updated_at": metadata.updated_at.to_rfc3339(),
            }))
        })
        .collect();

    Ok(json!({
        "count": filtered.len(),
        "secrets": filtered,
    }))
}

#[derive(Debug, Deserialize)]
struct SecretRotateParams {
    name: String,
    #[serde(rename = "newData")]
    new_data: std::collections::HashMap<String, String>,
}

async fn handle_secret_rotate(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: SecretRotateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "rotating secret");

    let id = claw_secrets::SecretId::new(&params.name)
        .map_err(|e| format!("invalid secret name: {e}"))?;

    let new_bytes = serde_json::to_vec(&params.new_data)?;

    state
        .secret_store
        .rotate(&id, &new_bytes)
        .map_err(|e| format!("failed to rotate secret: {e}"))?;

    let metadata = state
        .secret_store
        .metadata(&id)
        .map_err(|e| format!("failed to get metadata: {e}"))?;

    Ok(json!({
        "name": params.name,
        "rotated": true,
        "version": metadata.version,
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
        // We need to keep the dir alive, so leak it for tests
        std::mem::forget(dir);
        SharedState::new(config)
    }

    #[tokio::test]
    async fn test_secret_create_and_get() {
        let state = test_state();

        let create_result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({
                    "name": "db.password",
                    "data": {"password": "s3cret"}
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(create_result["success"], true);

        let get_result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "db.password"}),
            },
        )
        .await
        .expect("get");
        assert_eq!(get_result["data"]["password"], "s3cret");
        assert_eq!(get_result["version"], 1);
    }

    #[tokio::test]
    async fn test_secret_list() {
        let state = test_state();

        // Create two secrets
        for name in &["app.key1", "app.key2", "sys.token"] {
            handle_secret_command(
                &state,
                CommandRequest {
                    command: "secret.create".to_string(),
                    params: json!({"name": name, "data": {"v": "x"}}),
                },
            )
            .await
            .expect("create");
        }

        // List all
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 3);

        // List with prefix
        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.list".to_string(),
                params: json!({"prefix": "app."}),
            },
        )
        .await
        .expect("list with prefix");
        assert_eq!(result["count"], 2);
    }

    #[tokio::test]
    async fn test_secret_rotate() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "rotate.me", "data": {"key": "old"}}),
            },
        )
        .await
        .expect("create");

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.rotate".to_string(),
                params: json!({"name": "rotate.me", "newData": {"key": "new"}}),
            },
        )
        .await
        .expect("rotate");
        assert_eq!(result["rotated"], true);
        assert_eq!(result["version"], 2);

        // Verify the new value
        let get_result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "rotate.me"}),
            },
        )
        .await
        .expect("get");
        assert_eq!(get_result["data"]["key"], "new");
    }

    #[tokio::test]
    async fn test_secret_delete() {
        let state = test_state();

        handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.create".to_string(),
                params: json!({"name": "del.me", "data": {"k": "v"}}),
            },
        )
        .await
        .expect("create");

        let result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.delete".to_string(),
                params: json!({"name": "del.me"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);

        // Verify it's gone
        let get_result = handle_secret_command(
            &state,
            CommandRequest {
                command: "secret.get".to_string(),
                params: json!({"name": "del.me"}),
            },
        )
        .await;
        assert!(get_result.is_err());
    }
}
