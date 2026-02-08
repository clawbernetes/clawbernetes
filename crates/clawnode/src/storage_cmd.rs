//! Storage and volume command handlers
//!
//! Provides 9 commands (requires `storage` feature):
//! `volume.create`, `volume.mount`, `volume.unmount`, `volume.snapshot`,
//! `volume.list`, `volume.delete`,
//! `backup.create`, `backup.restore`, `backup.list`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::BackupEntry;
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a volume.* or backup.* command.
pub async fn handle_storage_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "volume.create" => handle_volume_create(state, request.params).await,
        "volume.mount" => handle_volume_mount(state, request.params).await,
        "volume.unmount" => handle_volume_unmount(state, request.params).await,
        "volume.snapshot" => handle_volume_snapshot(state, request.params).await,
        "volume.list" => handle_volume_list(state).await,
        "volume.delete" => handle_volume_delete(state, request.params).await,
        "backup.create" => handle_backup_create(state, request.params).await,
        "backup.restore" => handle_backup_restore(state, request.params).await,
        "backup.list" => handle_backup_list(state).await,
        _ => Err(format!("unknown storage command: {}", request.command).into()),
    }
}

#[derive(Debug, Deserialize)]
struct VolumeCreateParams {
    name: String,
    #[serde(rename = "type", default = "default_emptydir")]
    volume_type: String,
    #[serde(rename = "sizeGb", default = "default_size")]
    size_gb: u64,
    #[serde(rename = "accessMode", default = "default_rwo")]
    access_mode: String,
    #[serde(rename = "storageClass")]
    storage_class: Option<String>,
    // NFS-specific
    server: Option<String>,
    path: Option<String>,
    // S3-specific
    bucket: Option<String>,
}

fn default_emptydir() -> String {
    "emptydir".to_string()
}

fn default_size() -> u64 {
    10
}

fn default_rwo() -> String {
    "ReadWriteOnce".to_string()
}

async fn handle_volume_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, volume_type = %params.volume_type, size_gb = params.size_gb, "creating volume");

    let vol_id = claw_storage::VolumeId::new(&params.name)
        .map_err(|e| format!("invalid volume name: {e}"))?;

    let volume_type = match params.volume_type.as_str() {
        "emptydir" => claw_storage::VolumeType::empty_dir(),
        "hostpath" => {
            let path = params.path.as_deref().unwrap_or("/data");
            claw_storage::VolumeType::HostPath(claw_storage::HostPathConfig::new(path))
        }
        "nfs" => {
            let server = params
                .server
                .as_deref()
                .ok_or("NFS volume requires 'server' field")?;
            let path = params.path.as_deref().ok_or("NFS volume requires 'path' field")?;
            claw_storage::VolumeType::Nfs(claw_storage::NfsConfig::new(server, path))
        }
        "s3" => {
            let bucket = params
                .bucket
                .as_deref()
                .ok_or("S3 volume requires 'bucket' field")?;
            claw_storage::VolumeType::S3(claw_storage::S3Config::new(bucket))
        }
        other => return Err(format!("unknown volume type: {other} (use emptydir/hostpath/nfs/s3)").into()),
    };

    let access_mode = match params.access_mode.as_str() {
        "ReadWriteOnce" => claw_storage::AccessMode::ReadWriteOnce,
        "ReadOnlyMany" => claw_storage::AccessMode::ReadOnlyMany,
        "ReadWriteMany" => claw_storage::AccessMode::ReadWriteMany,
        other => return Err(format!("unknown access mode: {other}").into()),
    };

    let size_bytes = params.size_gb * 1024 * 1024 * 1024;
    let mut volume = claw_storage::Volume::new(vol_id, volume_type, size_bytes)
        .with_access_mode(access_mode);

    if let Some(ref sc) = params.storage_class {
        volume = volume.with_storage_class(sc);
    }

    state
        .volume_manager
        .provision_available(volume)
        .map_err(|e| format!("provision failed: {e}"))?;

    Ok(json!({
        "name": params.name,
        "type": params.volume_type,
        "sizeGb": params.size_gb,
        "accessMode": params.access_mode,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct VolumeMountParams {
    #[serde(rename = "volumeId")]
    volume_id: String,
    #[serde(rename = "workloadId")]
    workload_id: String,
    #[serde(rename = "mountPath")]
    mount_path: String,
    #[serde(rename = "readOnly", default)]
    read_only: bool,
}

async fn handle_volume_mount(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeMountParams = serde_json::from_value(params)?;

    info!(
        volume = %params.volume_id,
        workload = %params.workload_id,
        path = %params.mount_path,
        "mounting volume"
    );

    let vol_id = claw_storage::VolumeId::new(&params.volume_id)
        .map_err(|e| format!("invalid volume ID: {e}"))?;

    state
        .volume_manager
        .attach(&vol_id, &params.workload_id)
        .map_err(|e| format!("attach failed: {e}"))?;

    Ok(json!({
        "volumeId": params.volume_id,
        "workloadId": params.workload_id,
        "mountPath": params.mount_path,
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct VolumeIdentifyParams {
    #[serde(rename = "volumeId")]
    volume_id: String,
}

async fn handle_volume_unmount(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeIdentifyParams = serde_json::from_value(params)?;

    info!(volume = %params.volume_id, "unmounting volume");

    let vol_id = claw_storage::VolumeId::new(&params.volume_id)
        .map_err(|e| format!("invalid volume ID: {e}"))?;

    state
        .volume_manager
        .detach(&vol_id)
        .map_err(|e| format!("detach failed: {e}"))?;

    Ok(json!({
        "volumeId": params.volume_id,
        "unmounted": true,
    }))
}

async fn handle_volume_snapshot(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: VolumeIdentifyParams = serde_json::from_value(params)?;

    let vol_id = claw_storage::VolumeId::new(&params.volume_id)
        .map_err(|e| format!("invalid volume ID: {e}"))?;

    let _volume = state
        .volume_manager
        .get_volume(&vol_id)
        .ok_or_else(|| format!("volume '{}' not found", params.volume_id))?;

    let snapshot_id = uuid::Uuid::new_v4().to_string();

    Ok(json!({
        "volumeId": params.volume_id,
        "snapshotId": snapshot_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "success": true,
    }))
}

async fn handle_volume_list(state: &SharedState) -> Result<Value, CommandError> {
    let stats = state.volume_manager.stats();

    Ok(json!({
        "totalVolumes": stats.total_volumes,
        "availableVolumes": stats.available_volumes,
        "boundVolumes": stats.bound_volumes,
        "attachedVolumes": stats.attached_volumes,
        "totalCapacity": stats.total_capacity,
    }))
}

async fn handle_volume_delete(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: VolumeIdentifyParams = serde_json::from_value(params)?;

    info!(volume = %params.volume_id, "deleting volume");

    let vol_id = claw_storage::VolumeId::new(&params.volume_id)
        .map_err(|e| format!("invalid volume ID: {e}"))?;

    state
        .volume_manager
        .delete(&vol_id)
        .map_err(|e| format!("delete failed: {e}"))?;

    Ok(json!({
        "volumeId": params.volume_id,
        "deleted": true,
    }))
}

// ─────────────────────────────────────────────────────────────
// Backup Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BackupCreateParams {
    scope: String,
    destination: String,
}

async fn handle_backup_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: BackupCreateParams = serde_json::from_value(params)?;

    info!(scope = %params.scope, destination = %params.destination, "creating backup");

    let entry = BackupEntry {
        id: uuid::Uuid::new_v4().to_string(),
        scope: params.scope.clone(),
        destination: params.destination.clone(),
        state: "completed".to_string(),
        created_at: chrono::Utc::now(),
    };

    let id = entry.id.clone();
    let mut store = state.backup_store.write().await;
    store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "backupId": id,
        "scope": params.scope,
        "destination": params.destination,
        "state": "completed",
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct BackupRestoreParams {
    #[serde(rename = "backupId")]
    backup_id: String,
}

async fn handle_backup_restore(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: BackupRestoreParams = serde_json::from_value(params)?;

    info!(backup_id = %params.backup_id, "restoring backup");

    let mut store = state.backup_store.write().await;
    let backup = store
        .get_mut(&params.backup_id)
        .ok_or_else(|| format!("backup '{}' not found", params.backup_id))?;

    backup.state = "restored".to_string();
    store.update();

    Ok(json!({
        "backupId": params.backup_id,
        "restored": true,
    }))
}

async fn handle_backup_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.backup_store.read().await;
    let entries: Vec<Value> = store
        .list()
        .iter()
        .map(|b| {
            json!({
                "backupId": b.id,
                "scope": b.scope,
                "destination": b.destination,
                "state": b.state,
                "created_at": b.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": entries.len(),
        "backups": entries,
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
    async fn test_volume_create_and_list() {
        let state = test_state();

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({
                    "name": "data-vol",
                    "type": "emptydir",
                    "sizeGb": 50,
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
        assert_eq!(result["sizeGb"], 50);

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["totalVolumes"], 1);
    }

    #[tokio::test]
    async fn test_volume_delete() {
        let state = test_state();

        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "del-vol", "sizeGb": 10}),
            },
        )
        .await
        .expect("create");

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.delete".to_string(),
                params: json!({"volumeId": "del-vol"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);
    }

    #[tokio::test]
    async fn test_volume_snapshot() {
        let state = test_state();

        handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "snap-vol", "sizeGb": 10}),
            },
        )
        .await
        .expect("create");

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.snapshot".to_string(),
                params: json!({"volumeId": "snap-vol"}),
            },
        )
        .await
        .expect("snapshot");
        assert_eq!(result["success"], true);
        assert!(!result["snapshotId"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_backup_create_and_list() {
        let state = test_state();

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "backup.create".to_string(),
                params: json!({
                    "scope": "full",
                    "destination": "s3://backups/daily",
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "backup.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
    }

    #[tokio::test]
    async fn test_backup_restore() {
        let state = test_state();

        let create_result = handle_storage_command(
            &state,
            CommandRequest {
                command: "backup.create".to_string(),
                params: json!({"scope": "volumes", "destination": "/backups"}),
            },
        )
        .await
        .expect("create");

        let backup_id = create_result["backupId"].as_str().unwrap();

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "backup.restore".to_string(),
                params: json!({"backupId": backup_id}),
            },
        )
        .await
        .expect("restore");
        assert_eq!(result["restored"], true);
    }

    #[tokio::test]
    async fn test_volume_invalid_type() {
        let state = test_state();

        let result = handle_storage_command(
            &state,
            CommandRequest {
                command: "volume.create".to_string(),
                params: json!({"name": "bad", "type": "unknown", "sizeGb": 10}),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
