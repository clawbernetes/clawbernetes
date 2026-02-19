//! Volume and backup management command handlers
//!
//! Manages persistent volumes and backups using VolumeStore and BackupStore.
//! Volumes can be mounted to containers via Docker bind mounts.

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{BackupEntry, VolumeRecord};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use tracing::info;

pub async fn handle_storage_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "volume.create" => handle_volume_create(state, request.params).await,
        "volume.mount" => handle_volume_mount(state, request.params).await,
        "volume.unmount" => handle_volume_unmount(state, request.params).await,
        "volume.snapshot" => handle_volume_snapshot(state, request.params).await,
        "volume.list" => handle_volume_list(state, request.params).await,
        "volume.delete" => handle_volume_delete(state, request.params).await,
        "backup.create" => handle_backup_create(state, request.params).await,
        "backup.restore" => handle_backup_restore(state, request.params).await,
        "backup.list" => handle_backup_list(state, request.params).await,
        _ => Err(format!("unknown storage command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct VolumeCreateParams {
    name: String,
    #[serde(rename = "type", default = "default_emptydir")]
    volume_type: String,
    #[serde(rename = "hostPath")]
    host_path: Option<String>,
    size: Option<String>,
}

fn default_emptydir() -> String {
    "emptydir".to_string()
}

async fn handle_volume_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, volume_type = %params.volume_type, "creating volume");

    // For hostpath, ensure the directory exists
    let host_path = if params.volume_type == "hostpath" {
        let path = params
            .host_path
            .ok_or("hostPath required for hostpath volume type")?;
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("failed to create host path {path}: {e}"))?;
        Some(path)
    } else if params.volume_type == "emptydir" {
        // Create a temp dir under the state path
        let s = state.read().await;
        let vol_path = s.config.state_path.join("volumes").join(&params.name);
        std::fs::create_dir_all(&vol_path)
            .map_err(|e| format!("failed to create volume dir: {e}"))?;
        Some(vol_path.to_string_lossy().to_string())
    } else {
        params.host_path
    };

    let record = VolumeRecord {
        name: params.name.clone(),
        volume_type: params.volume_type.clone(),
        host_path,
        size: params.size.clone(),
        state: "available".to_string(),
        bound_to: None,
        mount_path: None,
        created_at: chrono::Utc::now(),
    };

    state
        .volume_store
        .write()
        .await
        .create(record)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "type": params.volume_type,
        "state": "available",
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct VolumeMountParams {
    name: String,
    #[serde(rename = "containerId")]
    container_id: String,
    #[serde(rename = "mountPath")]
    mount_path: String,
}

async fn handle_volume_mount(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeMountParams = serde_json::from_value(params)?;

    info!(volume = %params.name, container = %params.container_id, "mounting volume");

    let mut store = state.volume_store.write().await;
    let record = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("volume '{}' not found", params.name))?;

    if record.state == "bound" {
        return Err(format!(
            "volume '{}' already bound to {}",
            params.name,
            record.bound_to.as_deref().unwrap_or("unknown")
        )
        .into());
    }

    record.state = "bound".to_string();
    record.bound_to = Some(params.container_id.clone());
    record.mount_path = Some(params.mount_path.clone());
    store.update(&params.name);

    Ok(json!({
        "name": params.name,
        "state": "bound",
        "container": params.container_id,
        "mountPath": params.mount_path,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct VolumeNameParams {
    name: String,
}

async fn handle_volume_unmount(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeNameParams = serde_json::from_value(params)?;

    info!(volume = %params.name, "unmounting volume");

    let mut store = state.volume_store.write().await;
    let record = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("volume '{}' not found", params.name))?;

    record.state = "available".to_string();
    record.bound_to = None;
    record.mount_path = None;
    store.update(&params.name);

    Ok(json!({
        "name": params.name,
        "state": "available",
        "success": true,
    }))
}

async fn handle_volume_snapshot(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeNameParams = serde_json::from_value(params)?;

    let store = state.volume_store.read().await;
    let record = store
        .get(&params.name)
        .ok_or_else(|| format!("volume '{}' not found", params.name))?;

    let host_path = record
        .host_path
        .clone()
        .ok_or("volume has no host path to snapshot")?;
    drop(store);

    let snapshot_name = format!("{}-snap-{}", params.name, chrono::Utc::now().timestamp());
    let s = state.read().await;
    let snap_path = s.config.state_path.join("snapshots").join(&snapshot_name);
    drop(s);

    info!(volume = %params.name, snapshot = %snapshot_name, "creating snapshot");

    std::fs::create_dir_all(&snap_path).map_err(|e| format!("mkdir failed: {e}"))?;

    // Use cp -a for a filesystem snapshot
    let output = Command::new("cp")
        .args(["-a", &host_path, snap_path.to_str().unwrap_or(".")])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("snapshot failed: {stderr}").into());
    }

    Ok(json!({
        "name": params.name,
        "snapshot": snapshot_name,
        "path": snap_path.to_string_lossy(),
        "success": true,
    }))
}

async fn handle_volume_list(state: &SharedState, _params: Value) -> Result<Value, CommandError> {
    let store = state.volume_store.read().await;
    let volumes: Vec<Value> = store
        .list()
        .iter()
        .map(|v| {
            json!({
                "name": v.name,
                "type": v.volume_type,
                "state": v.state,
                "hostPath": v.host_path,
                "size": v.size,
                "boundTo": v.bound_to,
                "mountPath": v.mount_path,
                "createdAt": v.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": volumes.len(),
        "volumes": volumes,
    }))
}

async fn handle_volume_delete(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeNameParams = serde_json::from_value(params)?;

    // Check not bound
    {
        let store = state.volume_store.read().await;
        if let Some(record) = store.get(&params.name) {
            if record.state == "bound" {
                return Err(format!("volume '{}' is currently bound, unmount first", params.name).into());
            }
        }
    }

    let deleted = state.volume_store.write().await.delete(&params.name);
    if deleted.is_none() {
        return Err(format!("volume '{}' not found", params.name).into());
    }

    Ok(json!({
        "name": params.name,
        "deleted": true,
    }))
}

// ─── Backup commands ───

#[derive(Debug, Deserialize)]
struct BackupCreateParams {
    scope: String,
    destination: Option<String>,
}

async fn handle_backup_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: BackupCreateParams = serde_json::from_value(params)?;

    let id = format!("backup-{}", chrono::Utc::now().timestamp());
    let dest = params.destination.unwrap_or_else(|| {
        let s_path = std::path::PathBuf::from("/tmp/clawbernetes-backups");
        s_path.join(&id).to_string_lossy().to_string()
    });

    info!(id = %id, scope = %params.scope, dest = %dest, "creating backup");

    std::fs::create_dir_all(&dest).map_err(|e| format!("mkdir failed: {e}"))?;

    // Backup the state directory
    let state_path = {
        let s = state.read().await;
        s.config.state_path.clone()
    };

    let output = Command::new("cp")
        .args(["-a", state_path.to_str().unwrap_or("."), &dest])
        .output()?;

    let backup_state = if output.status.success() {
        "completed"
    } else {
        "failed"
    };

    let entry = BackupEntry {
        id: id.clone(),
        scope: params.scope.clone(),
        destination: dest.clone(),
        state: backup_state.to_string(),
        created_at: chrono::Utc::now(),
    };

    state
        .backup_store
        .write()
        .await
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "id": id,
        "scope": params.scope,
        "destination": dest,
        "state": backup_state,
        "success": backup_state == "completed",
    }))
}

#[derive(Debug, Deserialize)]
struct BackupIdParams {
    id: String,
}

async fn handle_backup_restore(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: BackupIdParams = serde_json::from_value(params)?;

    let store = state.backup_store.read().await;
    let entry = store
        .get(&params.id)
        .ok_or_else(|| format!("backup '{}' not found", params.id))?;

    if entry.state != "completed" {
        return Err(format!("backup '{}' is in state '{}', cannot restore", params.id, entry.state).into());
    }

    info!(id = %params.id, source = %entry.destination, "restoring backup");

    // Copy backup back to state path
    let state_path = {
        let s = state.read().await;
        s.config.state_path.clone()
    };

    let output = Command::new("cp")
        .args(["-a", &format!("{}/.", entry.destination), state_path.to_str().unwrap_or(".")])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("restore failed: {stderr}").into());
    }

    Ok(json!({
        "id": params.id,
        "restored": true,
        "source": entry.destination,
    }))
}

async fn handle_backup_list(state: &SharedState, _params: Value) -> Result<Value, CommandError> {
    let store = state.backup_store.read().await;
    let backups: Vec<Value> = store
        .list()
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "scope": b.scope,
                "destination": b.destination,
                "state": b.state,
                "createdAt": b.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": backups.len(),
        "backups": backups,
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
    async fn test_volume_create_emptydir() {
        let state = test_state();
        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "data-vol", "type": "emptydir"}),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
        assert_eq!(result["state"], "available");
    }

    #[tokio::test]
    async fn test_volume_list() {
        let state = test_state();
        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "vol-1"}),
            },
        )
        .await
        .expect("create");

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_volume_mount_unmount() {
        let state = test_state();
        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "mount-test"}),
            },
        )
        .await
        .expect("create");

        // Mount
        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.mount".to_string(),
                params: json!({"name": "mount-test", "containerId": "abc123", "mountPath": "/data"}),
            },
        )
        .await
        .expect("mount");
        assert_eq!(result["state"], "bound");

        // Can't mount again
        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.mount".to_string(),
                params: json!({"name": "mount-test", "containerId": "def456", "mountPath": "/data"}),
            },
        )
        .await;
        assert!(result.is_err());

        // Unmount
        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.unmount".to_string(),
                params: json!({"name": "mount-test"}),
            },
        )
        .await
        .expect("unmount");
        assert_eq!(result["state"], "available");
    }

    #[tokio::test]
    async fn test_volume_delete_bound_fails() {
        let state = test_state();
        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "del-test"}),
            },
        )
        .await
        .expect("create");

        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.mount".to_string(),
                params: json!({"name": "del-test", "containerId": "c1", "mountPath": "/d"}),
            },
        )
        .await
        .expect("mount");

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.delete".to_string(),
                params: json!({"name": "del-test"}),
            },
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_backup_list_empty() {
        let state = test_state();
        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "backup.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 0);
    }
}
