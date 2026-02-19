//! Deployment management command handlers
//!
//! Manages rolling deployments using persistent DeployStore and the container
//! runtime (Docker SDK or CLI fallback). Supports 8 commands:
//! `deploy.create`, `deploy.status`, `deploy.update`, `deploy.rollback`,
//! `deploy.history`, `deploy.promote`, `deploy.pause`, `deploy.delete`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{DeployRecord, DeployRevision};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use tracing::{info, warn};

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

/// Pull an image via the container runtime CLI.
async fn pull_image(runtime: &str, image: &str) -> Result<(), CommandError> {
    info!(image = %image, runtime = %runtime, "pulling image");
    let output = Command::new(runtime).args(["pull", image]).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("image pull failed: {stderr}").into());
    }
    Ok(())
}

/// Start a single replica container via CLI, returning the container ID.
async fn start_replica(
    state: &SharedState,
    name: &str,
    image: &str,
    replica_index: u32,
    gpus: u32,
    memory: Option<&str>,
    cpu: Option<f32>,
) -> Result<String, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let container_name = format!("claw-deploy-{name}-{replica_index}");

    let mut cmd = Command::new(&runtime);
    cmd.arg("run").arg("-d");
    cmd.args(["--name", &container_name]);
    cmd.args(["--label", "managed-by=clawbernetes"]);
    cmd.args(["--label", &format!("deploy-name={name}")]);
    cmd.args([
        "--label",
        &format!("deploy-replica={replica_index}"),
    ]);
    cmd.args(["--restart", "unless-stopped"]);

    if gpus > 0 {
        if runtime == "docker" {
            cmd.args(["--gpus", &format!("\"device={}\"", 
                (0..gpus).map(|i| i.to_string()).collect::<Vec<_>>().join(","))]);
        } else if runtime == "podman" {
            cmd.args(["--device", "nvidia.com/gpu=all"]);
        }
    }

    if let Some(mem) = memory {
        cmd.args(["--memory", mem]);
    }
    if let Some(cpu_limit) = cpu {
        cmd.args(["--cpus", &format!("{cpu_limit}")]);
    }

    cmd.arg(image);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("replica start failed: {stderr}").into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Stop and remove a container by ID.
async fn remove_container(state: &SharedState, container_id: &str) {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    // Stop
    let _ = Command::new(&runtime)
        .args(["stop", "--time", "10", container_id])
        .output();
    // Remove
    let _ = Command::new(&runtime)
        .args(["rm", "-f", container_id])
        .output();
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

    // Check if deployment already exists
    {
        let store = state.deploy_store.read().await;
        if store.get(&params.name).is_some() {
            return Err(format!("deployment '{}' already exists", params.name).into());
        }
    }

    let strategy = params.strategy.as_deref().unwrap_or("rolling").to_string();
    let gpus = params.gpus.unwrap_or(0);

    // Pull the image first
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };
    pull_image(&runtime, &params.image).await?;

    // Start replicas
    let mut container_ids = Vec::new();
    for i in 0..params.replicas {
        match start_replica(
            state,
            &params.name,
            &params.image,
            i,
            gpus,
            params.memory.as_deref(),
            params.cpu,
        )
        .await
        {
            Ok(cid) => container_ids.push(cid),
            Err(e) => {
                warn!(replica = i, error = %e, "failed to start replica, cleaning up");
                // Clean up already-started replicas
                for cid in &container_ids {
                    remove_container(state, cid).await;
                }
                return Err(e);
            }
        }
    }

    let now = chrono::Utc::now();
    let record = DeployRecord {
        name: params.name.clone(),
        image: params.image.clone(),
        previous_image: None,
        replicas: params.replicas,
        container_ids: container_ids.clone(),
        gpus_per_replica: gpus,
        memory: params.memory.clone(),
        cpu: params.cpu,
        strategy: strategy.clone(),
        state: "active".to_string(),
        revision: 1,
        history: vec![DeployRevision {
            revision: 1,
            image: params.image.clone(),
            replicas: params.replicas,
            timestamp: now,
            reason: Some("initial deployment".to_string()),
        }],
        created_at: now,
        updated_at: now,
    };

    state
        .deploy_store
        .write()
        .await
        .create(record)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "image": params.image,
        "replicas": params.replicas,
        "containers": container_ids,
        "strategy": strategy,
        "state": "active",
        "revision": 1,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct DeployIdentifyParams {
    name: Option<String>,
}

async fn handle_deploy_status(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let name = params.name.ok_or("name required")?;

    let store = state.deploy_store.read().await;
    let record = store.get(&name).ok_or_else(|| format!("deployment '{name}' not found"))?;

    Ok(json!({
        "name": record.name,
        "image": record.image,
        "previousImage": record.previous_image,
        "replicas": record.replicas,
        "containers": record.container_ids,
        "gpusPerReplica": record.gpus_per_replica,
        "memory": record.memory,
        "cpu": record.cpu,
        "strategy": record.strategy,
        "state": record.state,
        "revision": record.revision,
        "createdAt": record.created_at.to_rfc3339(),
        "updatedAt": record.updated_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct DeployUpdateParams {
    name: String,
    image: Option<String>,
    replicas: Option<u32>,
}

async fn handle_deploy_update(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployUpdateParams = serde_json::from_value(params)?;

    // Read current state
    let (old_image, old_container_ids, old_replicas, gpus, memory, cpu) = {
        let store = state.deploy_store.read().await;
        let record = store
            .get(&params.name)
            .ok_or_else(|| format!("deployment '{}' not found", params.name))?;
        if record.state == "paused" {
            return Err("deployment is paused, resume before updating".into());
        }
        (
            record.image.clone(),
            record.container_ids.clone(),
            record.replicas,
            record.gpus_per_replica,
            record.memory.clone(),
            record.cpu,
        )
    };

    let new_image = params.image.unwrap_or_else(|| old_image.clone());
    let new_replicas = params.replicas.unwrap_or(old_replicas);

    info!(
        name = %params.name,
        old_image = %old_image,
        new_image = %new_image,
        new_replicas = new_replicas,
        "updating deployment"
    );

    // Pull new image if changed
    if new_image != old_image {
        let runtime = {
            let s = state.read().await;
            s.config.container_runtime.clone()
        };
        pull_image(&runtime, &new_image).await?;
    }

    // Start new replicas
    let mut new_container_ids = Vec::new();
    for i in 0..new_replicas {
        match start_replica(
            state,
            &params.name,
            &new_image,
            i,
            gpus,
            memory.as_deref(),
            cpu,
        )
        .await
        {
            Ok(cid) => new_container_ids.push(cid),
            Err(e) => {
                warn!(replica = i, error = %e, "failed to start new replica");
                // Clean up new replicas and keep old ones running
                for cid in &new_container_ids {
                    remove_container(state, cid).await;
                }
                return Err(format!("update failed at replica {i}: {e}").into());
            }
        }
    }

    // Stop old replicas
    for cid in &old_container_ids {
        remove_container(state, cid).await;
    }

    // Update the deploy record
    {
        let mut store = state.deploy_store.write().await;
        if let Some(record) = store.get_mut(&params.name) {
            record.previous_image = Some(old_image.clone());
            record.image = new_image.clone();
            record.replicas = new_replicas;
            record.container_ids = new_container_ids.clone();
            record.revision += 1;
            record.state = "active".to_string();
            record.updated_at = chrono::Utc::now();
            record.history.push(DeployRevision {
                revision: record.revision,
                image: new_image.clone(),
                replicas: new_replicas,
                timestamp: chrono::Utc::now(),
                reason: Some("update".to_string()),
            });
        }
        store.update(&params.name);
    }

    Ok(json!({
        "name": params.name,
        "image": new_image,
        "previousImage": old_image,
        "replicas": new_replicas,
        "containers": new_container_ids,
        "success": true,
    }))
}

async fn handle_deploy_rollback(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    #[derive(Debug, Deserialize)]
    struct RollbackParams {
        name: String,
        reason: Option<String>,
    }

    let params: RollbackParams = serde_json::from_value(params)?;

    // Read current state
    let (previous_image, current_containers, replicas, gpus, memory, cpu) = {
        let store = state.deploy_store.read().await;
        let record = store
            .get(&params.name)
            .ok_or_else(|| format!("deployment '{}' not found", params.name))?;
        let prev = record
            .previous_image
            .clone()
            .ok_or("no previous image to rollback to")?;
        (
            prev,
            record.container_ids.clone(),
            record.replicas,
            record.gpus_per_replica,
            record.memory.clone(),
            record.cpu,
        )
    };

    let reason = params.reason.as_deref().unwrap_or("manual rollback");
    info!(name = %params.name, target_image = %previous_image, reason = %reason, "rolling back");

    // Start replicas with previous image
    let mut new_container_ids = Vec::new();
    for i in 0..replicas {
        match start_replica(
            state,
            &params.name,
            &previous_image,
            i,
            gpus,
            memory.as_deref(),
            cpu,
        )
        .await
        {
            Ok(cid) => new_container_ids.push(cid),
            Err(e) => {
                for cid in &new_container_ids {
                    remove_container(state, cid).await;
                }
                return Err(format!("rollback failed at replica {i}: {e}").into());
            }
        }
    }

    // Stop current containers
    for cid in &current_containers {
        remove_container(state, cid).await;
    }

    // Update record
    {
        let mut store = state.deploy_store.write().await;
        if let Some(record) = store.get_mut(&params.name) {
            let current_image = record.image.clone();
            record.previous_image = Some(current_image);
            record.image = previous_image.clone();
            record.container_ids = new_container_ids.clone();
            record.revision += 1;
            record.state = "active".to_string();
            record.updated_at = chrono::Utc::now();
            record.history.push(DeployRevision {
                revision: record.revision,
                image: previous_image.clone(),
                replicas,
                timestamp: chrono::Utc::now(),
                reason: Some(format!("rollback: {reason}")),
            });
        }
        store.update(&params.name);
    }

    Ok(json!({
        "name": params.name,
        "image": previous_image,
        "containers": new_container_ids,
        "rolledBack": true,
        "reason": reason,
        "success": true,
    }))
}

async fn handle_deploy_history(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;

    if let Some(name) = params.name {
        // History for a specific deployment
        let store = state.deploy_store.read().await;
        let record = store
            .get(&name)
            .ok_or_else(|| format!("deployment '{name}' not found"))?;

        let revisions: Vec<Value> = record
            .history
            .iter()
            .map(|r| {
                json!({
                    "revision": r.revision,
                    "image": r.image,
                    "replicas": r.replicas,
                    "timestamp": r.timestamp.to_rfc3339(),
                    "reason": r.reason,
                })
            })
            .collect();

        Ok(json!({
            "name": name,
            "currentRevision": record.revision,
            "revisions": revisions,
        }))
    } else {
        // List all deployments
        let store = state.deploy_store.read().await;
        let deploys: Vec<Value> = store
            .list()
            .iter()
            .map(|d| {
                json!({
                    "name": d.name,
                    "image": d.image,
                    "state": d.state,
                    "replicas": d.replicas,
                    "revision": d.revision,
                    "createdAt": d.created_at.to_rfc3339(),
                    "updatedAt": d.updated_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(json!({
            "count": deploys.len(),
            "deployments": deploys,
        }))
    }
}

async fn handle_deploy_promote(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let name = params.name.ok_or("name required")?;

    info!(name = %name, "promoting deployment");

    let mut store = state.deploy_store.write().await;
    let record = store
        .get_mut(&name)
        .ok_or_else(|| format!("deployment '{name}' not found"))?;

    // Promote = clear previous_image (no rollback target) and mark active
    record.previous_image = None;
    record.state = "active".to_string();
    record.updated_at = chrono::Utc::now();
    store.update(&name);

    Ok(json!({
        "name": name,
        "promoted": true,
        "state": "active",
    }))
}

async fn handle_deploy_pause(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let name = params.name.ok_or("name required")?;

    info!(name = %name, "pausing deployment");

    let mut store = state.deploy_store.write().await;
    let record = store
        .get_mut(&name)
        .ok_or_else(|| format!("deployment '{name}' not found"))?;

    record.state = "paused".to_string();
    record.updated_at = chrono::Utc::now();
    store.update(&name);

    Ok(json!({
        "name": name,
        "paused": true,
        "message": "deployment paused (updates blocked until resumed)",
    }))
}

async fn handle_deploy_delete(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: DeployIdentifyParams = serde_json::from_value(params)?;
    let name = params.name.ok_or("name required")?;

    info!(name = %name, "deleting deployment");

    // Get container IDs before deleting
    let container_ids = {
        let store = state.deploy_store.read().await;
        let record = store
            .get(&name)
            .ok_or_else(|| format!("deployment '{name}' not found"))?;
        record.container_ids.clone()
    };

    // Stop all containers
    for cid in &container_ids {
        remove_container(state, cid).await;
    }

    // Remove from store
    state.deploy_store.write().await.delete(&name);

    Ok(json!({
        "name": name,
        "deleted": true,
        "containersRemoved": container_ids.len(),
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
    async fn test_deploy_status_not_found() {
        let state = test_state();
        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.status".to_string(),
                params: json!({"name": "nonexistent"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_deploy_history_all_empty() {
        let state = test_state();
        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.history".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("history");
        assert_eq!(result["count"], 0);
    }

    #[tokio::test]
    async fn test_deploy_store_persistence() {
        let state = test_state();

        // Directly insert a record into the store
        let now = chrono::Utc::now();
        let record = DeployRecord {
            name: "test-app".to_string(),
            image: "app:v1".to_string(),
            previous_image: None,
            replicas: 2,
            container_ids: vec!["abc123".to_string()],
            gpus_per_replica: 0,
            memory: None,
            cpu: None,
            strategy: "rolling".to_string(),
            state: "active".to_string(),
            revision: 1,
            history: vec![DeployRevision {
                revision: 1,
                image: "app:v1".to_string(),
                replicas: 2,
                timestamp: now,
                reason: Some("test".to_string()),
            }],
            created_at: now,
            updated_at: now,
        };

        state
            .deploy_store
            .write()
            .await
            .create(record)
            .expect("create");

        // Read it back
        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.status".to_string(),
                params: json!({"name": "test-app"}),
            },
        )
        .await
        .expect("status");

        assert_eq!(result["name"], "test-app");
        assert_eq!(result["image"], "app:v1");
        assert_eq!(result["state"], "active");
        assert_eq!(result["revision"], 1);
    }

    #[tokio::test]
    async fn test_deploy_pause_and_promote() {
        let state = test_state();

        // Insert directly
        let now = chrono::Utc::now();
        let record = DeployRecord {
            name: "pause-test".to_string(),
            image: "app:v1".to_string(),
            previous_image: Some("app:v0".to_string()),
            replicas: 1,
            container_ids: vec![],
            gpus_per_replica: 0,
            memory: None,
            cpu: None,
            strategy: "rolling".to_string(),
            state: "active".to_string(),
            revision: 2,
            history: vec![],
            created_at: now,
            updated_at: now,
        };
        state.deploy_store.write().await.create(record).expect("create");

        // Pause
        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.pause".to_string(),
                params: json!({"name": "pause-test"}),
            },
        )
        .await
        .expect("pause");
        assert_eq!(result["paused"], true);

        // Verify paused state
        let status = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.status".to_string(),
                params: json!({"name": "pause-test"}),
            },
        )
        .await
        .expect("status");
        assert_eq!(status["state"], "paused");

        // Promote
        let result = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.promote".to_string(),
                params: json!({"name": "pause-test"}),
            },
        )
        .await
        .expect("promote");
        assert_eq!(result["promoted"], true);

        // Verify promoted: previous_image cleared
        let status = handle_deploy_command(
            &state,
            CommandRequest {
                command: "deploy.status".to_string(),
                params: json!({"name": "pause-test"}),
            },
        )
        .await
        .expect("status");
        assert_eq!(status["state"], "active");
        assert_eq!(status["previousImage"], Value::Null);
    }
}
