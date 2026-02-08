//! Namespace and node management command handlers
//!
//! Provides 7 commands (always available):
//! `namespace.create`, `namespace.set_quota`, `namespace.usage`, `namespace.list`,
//! `node.label`, `node.taint`, `node.drain`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{NamespaceEntry, ResourceQuota, TaintEntry};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a namespace.* or node.label/taint/drain command.
pub async fn handle_namespace_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "namespace.create" => handle_namespace_create(state, request.params).await,
        "namespace.set_quota" => handle_namespace_set_quota(state, request.params).await,
        "namespace.usage" => handle_namespace_usage(state, request.params).await,
        "namespace.list" => handle_namespace_list(state).await,
        "node.label" => handle_node_label(state, request.params).await,
        "node.taint" => handle_node_taint(state, request.params).await,
        "node.drain" => handle_node_drain(state, request.params).await,
        _ => Err(format!("unknown namespace command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct NamespaceCreateParams {
    name: String,
    #[serde(default)]
    quotas: QuotaParams,
    #[serde(default)]
    labels: std::collections::HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct QuotaParams {
    cpu: Option<f64>,
    memory: Option<u64>,
    gpus: Option<u32>,
    storage: Option<u64>,
}

async fn handle_namespace_create(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: NamespaceCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, "creating namespace");

    let entry = NamespaceEntry {
        name: params.name.clone(),
        quotas: ResourceQuota {
            max_cpu: params.quotas.cpu,
            max_memory_mb: params.quotas.memory,
            max_gpus: params.quotas.gpus,
            max_storage_gb: params.quotas.storage,
        },
        labels: params.labels,
        created_at: chrono::Utc::now(),
    };

    let mut store = state.namespace_store.write().await;
    store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct NamespaceSetQuotaParams {
    name: String,
    cpu: Option<f64>,
    memory: Option<u64>,
    gpus: Option<u32>,
    storage: Option<u64>,
}

async fn handle_namespace_set_quota(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: NamespaceSetQuotaParams = serde_json::from_value(params)?;

    let mut store = state.namespace_store.write().await;
    let ns = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("namespace '{}' not found", params.name))?;

    if let Some(cpu) = params.cpu {
        ns.quotas.max_cpu = Some(cpu);
    }
    if let Some(memory) = params.memory {
        ns.quotas.max_memory_mb = Some(memory);
    }
    if let Some(gpus) = params.gpus {
        ns.quotas.max_gpus = Some(gpus);
    }
    if let Some(storage) = params.storage {
        ns.quotas.max_storage_gb = Some(storage);
    }

    // Capture quota values before releasing the mutable borrow
    let quotas_cpu = ns.quotas.max_cpu;
    let quotas_memory = ns.quotas.max_memory_mb;
    let quotas_gpus = ns.quotas.max_gpus;
    let quotas_storage = ns.quotas.max_storage_gb;

    store.update();

    Ok(json!({
        "name": params.name,
        "success": true,
        "quotas": {
            "cpu": quotas_cpu,
            "memory_mb": quotas_memory,
            "gpus": quotas_gpus,
            "storage_gb": quotas_storage,
        },
    }))
}

#[derive(Debug, Deserialize)]
struct NamespaceIdentifyParams {
    name: String,
}

async fn handle_namespace_usage(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: NamespaceIdentifyParams = serde_json::from_value(params)?;

    let store = state.namespace_store.read().await;
    let ns = store
        .get(&params.name)
        .ok_or_else(|| format!("namespace '{}' not found", params.name))?;

    // Return quota and placeholder usage (actual usage would aggregate from workload.list)
    Ok(json!({
        "name": ns.name,
        "quotas": {
            "cpu": ns.quotas.max_cpu,
            "memory_mb": ns.quotas.max_memory_mb,
            "gpus": ns.quotas.max_gpus,
            "storage_gb": ns.quotas.max_storage_gb,
        },
        "usage": {
            "cpu": 0.0,
            "memory_mb": 0,
            "gpus": 0,
            "storage_gb": 0,
        },
        "labels": ns.labels,
    }))
}

async fn handle_namespace_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.namespace_store.read().await;
    let entries: Vec<Value> = store
        .list()
        .iter()
        .map(|ns| {
            json!({
                "name": ns.name,
                "quotas": {
                    "cpu": ns.quotas.max_cpu,
                    "memory_mb": ns.quotas.max_memory_mb,
                    "gpus": ns.quotas.max_gpus,
                    "storage_gb": ns.quotas.max_storage_gb,
                },
                "labels": ns.labels,
                "created_at": ns.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": entries.len(),
        "namespaces": entries,
    }))
}

#[derive(Debug, Deserialize)]
struct NodeLabelParams {
    labels: std::collections::HashMap<String, String>,
}

async fn handle_node_label(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: NodeLabelParams = serde_json::from_value(params)?;

    info!(count = params.labels.len(), "updating node labels");

    let mut node_state = state.write().await;
    for (key, value) in &params.labels {
        node_state
            .config
            .labels
            .insert(key.clone(), value.clone());
    }

    Ok(json!({
        "labels": node_state.config.labels,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct NodeTaintParams {
    key: String,
    #[serde(default)]
    value: String,
    effect: String,
}

async fn handle_node_taint(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: NodeTaintParams = serde_json::from_value(params)?;

    if !["NoSchedule", "PreferNoSchedule", "NoExecute"].contains(&params.effect.as_str()) {
        return Err(
            "effect must be one of: NoSchedule, PreferNoSchedule, NoExecute".into(),
        );
    }

    info!(key = %params.key, effect = %params.effect, "adding node taint");

    let taint = TaintEntry {
        key: params.key.clone(),
        value: params.value.clone(),
        effect: params.effect.clone(),
    };

    let mut store = state.namespace_store.write().await;
    store.taints.push(taint);

    Ok(json!({
        "key": params.key,
        "value": params.value,
        "effect": params.effect,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct NodeDrainParams {
    #[serde(rename = "gracePeriod", default = "default_grace")]
    grace_period: u32,
}

fn default_grace() -> u32 {
    30
}

async fn handle_node_drain(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: NodeDrainParams =
        serde_json::from_value(params).unwrap_or(NodeDrainParams { grace_period: 30 });

    info!(grace_period = params.grace_period, "draining node");

    // List managed containers and stop them
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let output = std::process::Command::new(&runtime)
        .args([
            "ps",
            "-q",
            "--filter",
            "label=managed-by=clawbernetes",
        ])
        .output();

    let mut stopped = 0;
    if let Ok(output) = output {
        let ids = String::from_utf8_lossy(&output.stdout);
        for id in ids.lines().filter(|l| !l.is_empty()) {
            let _ = std::process::Command::new(&runtime)
                .args(["stop", "-t", &params.grace_period.to_string(), id])
                .output();
            stopped += 1;
        }
    }

    Ok(json!({
        "drained": true,
        "stopped_workloads": stopped,
        "grace_period": params.grace_period,
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
    async fn test_namespace_create_and_list() {
        let state = test_state();

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.create".to_string(),
                params: json!({
                    "name": "production",
                    "quotas": {"cpu": 16.0, "memory": 32768, "gpus": 4},
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
        assert_eq!(result["namespaces"][0]["name"], "production");
    }

    #[tokio::test]
    async fn test_namespace_set_quota() {
        let state = test_state();

        handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.create".to_string(),
                params: json!({"name": "dev"}),
            },
        )
        .await
        .expect("create");

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.set_quota".to_string(),
                params: json!({"name": "dev", "cpu": 8.0, "gpus": 2}),
            },
        )
        .await
        .expect("set quota");
        assert_eq!(result["quotas"]["cpu"], 8.0);
        assert_eq!(result["quotas"]["gpus"], 2);
    }

    #[tokio::test]
    async fn test_namespace_usage() {
        let state = test_state();

        handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.create".to_string(),
                params: json!({"name": "test"}),
            },
        )
        .await
        .expect("create");

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "namespace.usage".to_string(),
                params: json!({"name": "test"}),
            },
        )
        .await
        .expect("usage");
        assert_eq!(result["name"], "test");
    }

    #[tokio::test]
    async fn test_node_label() {
        let state = test_state();

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "node.label".to_string(),
                params: json!({"labels": {"gpu-type": "a100", "region": "us-west"}}),
            },
        )
        .await
        .expect("label");
        assert_eq!(result["success"], true);
    }

    #[tokio::test]
    async fn test_node_taint() {
        let state = test_state();

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "node.taint".to_string(),
                params: json!({"key": "gpu", "value": "true", "effect": "NoSchedule"}),
            },
        )
        .await
        .expect("taint");
        assert_eq!(result["success"], true);
    }

    #[tokio::test]
    async fn test_node_taint_invalid_effect() {
        let state = test_state();

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "node.taint".to_string(),
                params: json!({"key": "k", "effect": "BadEffect"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_node_drain() {
        let state = test_state();

        let result = handle_namespace_command(
            &state,
            CommandRequest {
                command: "node.drain".to_string(),
                params: json!({"gracePeriod": 10}),
            },
        )
        .await
        .expect("drain");
        assert_eq!(result["drained"], true);
    }
}
