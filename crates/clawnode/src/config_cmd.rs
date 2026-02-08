//! Configuration management command handlers
//!
//! Wraps `persist::ConfigStore` with 5 commands:
//! `config.create`, `config.get`, `config.update`, `config.delete`, `config.list`

use crate::commands::{CommandError, CommandRequest};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a config.* command to the appropriate handler.
pub async fn handle_config_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "config.create" => handle_config_create(state, request.params).await,
        "config.get" => handle_config_get(state, request.params).await,
        "config.update" => handle_config_update(state, request.params).await,
        "config.delete" => handle_config_delete(state, request.params).await,
        "config.list" => handle_config_list(state, request.params).await,
        _ => Err(format!("unknown config command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct ConfigCreateParams {
    name: String,
    data: std::collections::HashMap<String, String>,
    #[serde(default)]
    immutable: bool,
}

async fn handle_config_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ConfigCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, immutable = params.immutable, "creating config");

    let mut store = state.config_store.write().await;
    store
        .create(params.name.clone(), params.data, params.immutable)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "success": true,
        "immutable": params.immutable,
    }))
}

#[derive(Debug, Deserialize)]
struct ConfigGetParams {
    name: String,
}

async fn handle_config_get(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ConfigGetParams = serde_json::from_value(params)?;

    let store = state.config_store.read().await;
    let entry = store
        .get(&params.name)
        .ok_or_else(|| format!("config '{}' not found", params.name))?;

    Ok(json!({
        "name": params.name,
        "data": entry.data,
        "immutable": entry.immutable,
        "created_at": entry.created_at.to_rfc3339(),
        "updated_at": entry.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct ConfigUpdateParams {
    name: String,
    data: std::collections::HashMap<String, String>,
}

async fn handle_config_update(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ConfigUpdateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "updating config");

    let mut store = state.config_store.write().await;
    store
        .update(&params.name, params.data)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct ConfigDeleteParams {
    name: String,
}

async fn handle_config_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ConfigDeleteParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting config");

    let mut store = state.config_store.write().await;
    store
        .delete(&params.name)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

#[derive(Debug, Deserialize)]
struct ConfigListParams {
    prefix: Option<String>,
}

async fn handle_config_list(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: ConfigListParams =
        serde_json::from_value(params).unwrap_or(ConfigListParams { prefix: None });

    let store = state.config_store.read().await;
    let entries = store.list(params.prefix.as_deref());

    let configs: Vec<Value> = entries
        .iter()
        .map(|(name, entry)| {
            json!({
                "name": name,
                "immutable": entry.immutable,
                "keys": entry.data.keys().collect::<Vec<_>>(),
                "created_at": entry.created_at.to_rfc3339(),
                "updated_at": entry.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": configs.len(),
        "configs": configs,
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
    async fn test_config_create_and_get() {
        let state = test_state();

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.create".to_string(),
                params: json!({
                    "name": "app.settings",
                    "data": {"log_level": "info", "port": "8080"}
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.get".to_string(),
                params: json!({"name": "app.settings"}),
            },
        )
        .await
        .expect("get");
        assert_eq!(result["data"]["log_level"], "info");
        assert_eq!(result["data"]["port"], "8080");
        assert_eq!(result["immutable"], false);
    }

    #[tokio::test]
    async fn test_config_update() {
        let state = test_state();

        handle_config_command(
            &state,
            CommandRequest {
                command: "config.create".to_string(),
                params: json!({"name": "mutable.cfg", "data": {"k": "v1"}}),
            },
        )
        .await
        .expect("create");

        handle_config_command(
            &state,
            CommandRequest {
                command: "config.update".to_string(),
                params: json!({"name": "mutable.cfg", "data": {"k": "v2"}}),
            },
        )
        .await
        .expect("update");

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.get".to_string(),
                params: json!({"name": "mutable.cfg"}),
            },
        )
        .await
        .expect("get");
        assert_eq!(result["data"]["k"], "v2");
    }

    #[tokio::test]
    async fn test_config_immutable_reject() {
        let state = test_state();

        handle_config_command(
            &state,
            CommandRequest {
                command: "config.create".to_string(),
                params: json!({"name": "locked", "data": {"k": "v"}, "immutable": true}),
            },
        )
        .await
        .expect("create");

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.update".to_string(),
                params: json!({"name": "locked", "data": {"k": "v2"}}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_delete() {
        let state = test_state();

        handle_config_command(
            &state,
            CommandRequest {
                command: "config.create".to_string(),
                params: json!({"name": "to-delete", "data": {}}),
            },
        )
        .await
        .expect("create");

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.delete".to_string(),
                params: json!({"name": "to-delete"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.get".to_string(),
                params: json!({"name": "to-delete"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_list() {
        let state = test_state();

        for name in &["app.db", "app.cache", "sys.net"] {
            handle_config_command(
                &state,
                CommandRequest {
                    command: "config.create".to_string(),
                    params: json!({"name": name, "data": {"x": "y"}}),
                },
            )
            .await
            .expect("create");
        }

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list all");
        assert_eq!(result["count"], 3);

        let result = handle_config_command(
            &state,
            CommandRequest {
                command: "config.list".to_string(),
                params: json!({"prefix": "app."}),
            },
        )
        .await
        .expect("list prefix");
        assert_eq!(result["count"], 2);
    }
}
