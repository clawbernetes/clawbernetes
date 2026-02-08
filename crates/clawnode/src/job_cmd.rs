//! Job scheduling and cron command handlers
//!
//! Provides 9 commands for batch job and cron management:
//! `job.create`, `job.status`, `job.logs`, `job.delete`,
//! `cron.create`, `cron.list`, `cron.trigger`, `cron.suspend`, `cron.resume`

use crate::commands::{CommandError, CommandRequest};
use crate::persist::{CronEntry, JobEntry};
use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

/// Route a job.* or cron.* command to the appropriate handler.
pub async fn handle_job_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
    match request.command.as_str() {
        "job.create" => handle_job_create(state, request.params).await,
        "job.status" => handle_job_status(state, request.params).await,
        "job.logs" => handle_job_logs(state, request.params).await,
        "job.delete" => handle_job_delete(state, request.params).await,
        "cron.create" => handle_cron_create(state, request.params).await,
        "cron.list" => handle_cron_list(state).await,
        "cron.trigger" => handle_cron_trigger(state, request.params).await,
        "cron.suspend" => handle_cron_suspend(state, request.params).await,
        "cron.resume" => handle_cron_resume(state, request.params).await,
        _ => Err(format!("unknown job command: {}", request.command).into()),
    }
}

// ─────────────────────────────────────────────────────────────
// Job Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JobCreateParams {
    name: String,
    image: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default = "default_one")]
    completions: u32,
    #[serde(default = "default_one")]
    parallelism: u32,
    #[serde(rename = "backoffLimit", default = "default_backoff")]
    backoff_limit: u32,
}

fn default_one() -> u32 {
    1
}

fn default_backoff() -> u32 {
    3
}

async fn handle_job_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: JobCreateParams = serde_json::from_value(params)?;

    info!(name = %params.name, image = %params.image, "creating job");

    let entry = JobEntry {
        name: params.name.clone(),
        image: params.image.clone(),
        command: params.command,
        completions: params.completions,
        completed: 0,
        failed: 0,
        parallelism: params.parallelism,
        backoff_limit: params.backoff_limit,
        container_ids: Vec::new(),
        state: "running".to_string(),
        created_at: chrono::Utc::now(),
        finished_at: None,
    };

    let mut store = state.job_store.write().await;
    store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "image": params.image,
        "completions": params.completions,
        "parallelism": params.parallelism,
        "state": "running",
        "success": true,
    }))
}

#[derive(Debug, Deserialize)]
struct JobIdentifyParams {
    name: String,
}

async fn handle_job_status(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: JobIdentifyParams = serde_json::from_value(params)?;

    let store = state.job_store.read().await;
    let job = store
        .get(&params.name)
        .ok_or_else(|| format!("job '{}' not found", params.name))?;

    let duration = match job.finished_at {
        Some(finished) => (finished - job.created_at).num_seconds(),
        None => (chrono::Utc::now() - job.created_at).num_seconds(),
    };

    Ok(json!({
        "name": job.name,
        "image": job.image,
        "state": job.state,
        "completions": format!("{}/{}", job.completed, job.completions),
        "completed": job.completed,
        "failed": job.failed,
        "parallelism": job.parallelism,
        "duration_secs": duration,
        "created_at": job.created_at.to_rfc3339(),
        "finished_at": job.finished_at.map(|t| t.to_rfc3339()),
    }))
}

#[derive(Debug, Deserialize)]
struct JobLogsParams {
    name: String,
    tail: Option<u32>,
}

async fn handle_job_logs(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: JobLogsParams = serde_json::from_value(params)?;

    let store = state.job_store.read().await;
    let job = store
        .get(&params.name)
        .ok_or_else(|| format!("job '{}' not found", params.name))?;

    if job.container_ids.is_empty() {
        return Ok(json!({
            "name": params.name,
            "logs": "",
            "message": "no containers spawned yet",
        }));
    }

    // Aggregate logs from all containers via CLI
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let mut all_logs = String::new();
    for cid in &job.container_ids {
        let mut cmd = std::process::Command::new(&runtime);
        cmd.arg("logs");
        if let Some(tail) = params.tail {
            cmd.args(["--tail", &tail.to_string()]);
        }
        cmd.arg(cid);

        if let Ok(output) = cmd.output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                all_logs.push_str(&format!("=== {cid} ===\n{stdout}\n"));
            }
        }
    }

    Ok(json!({
        "name": params.name,
        "logs": all_logs,
        "containers": job.container_ids.len(),
    }))
}

async fn handle_job_delete(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: JobIdentifyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "deleting job");

    let mut store = state.job_store.write().await;
    let job = store
        .delete(&params.name)
        .map_err(|e| -> CommandError { e.into() })?;

    // Attempt to stop containers
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    for cid in &job.container_ids {
        let _ = std::process::Command::new(&runtime)
            .args(["stop", cid])
            .output();
    }

    Ok(json!({
        "name": params.name,
        "deleted": true,
        "containers_stopped": job.container_ids.len(),
    }))
}

// ─────────────────────────────────────────────────────────────
// Cron Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CronCreateParams {
    name: String,
    schedule: String,
    image: String,
    #[serde(default)]
    command: Vec<String>,
}

/// Validate a simple cron expression (5 fields: min hour dom mon dow).
fn validate_cron_expression(expr: &str) -> Result<(), String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err("cron expression must have 5 fields: minute hour day-of-month month day-of-week".to_string());
    }
    Ok(())
}

async fn handle_cron_create(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CronCreateParams = serde_json::from_value(params)?;

    validate_cron_expression(&params.schedule)
        .map_err(|e| -> CommandError { e.into() })?;

    info!(name = %params.name, schedule = %params.schedule, "creating cron job");

    let entry = CronEntry {
        name: params.name.clone(),
        schedule: params.schedule.clone(),
        image: params.image.clone(),
        command: params.command,
        suspended: false,
        last_run: None,
        next_run: None,
        created_at: chrono::Utc::now(),
    };

    let mut store = state.cron_store.write().await;
    store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    Ok(json!({
        "name": params.name,
        "schedule": params.schedule,
        "image": params.image,
        "success": true,
    }))
}

async fn handle_cron_list(state: &SharedState) -> Result<Value, CommandError> {
    let store = state.cron_store.read().await;
    let entries: Vec<Value> = store
        .list()
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "schedule": c.schedule,
                "image": c.image,
                "suspended": c.suspended,
                "last_run": c.last_run.map(|t| t.to_rfc3339()),
                "next_run": c.next_run.map(|t| t.to_rfc3339()),
                "created_at": c.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "count": entries.len(),
        "crons": entries,
    }))
}

#[derive(Debug, Deserialize)]
struct CronIdentifyParams {
    name: String,
}

async fn handle_cron_trigger(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CronIdentifyParams = serde_json::from_value(params)?;

    info!(name = %params.name, "triggering cron job");

    let (image, command) = {
        let store = state.cron_store.read().await;
        let cron = store
            .get(&params.name)
            .ok_or_else(|| format!("cron '{}' not found", params.name))?;
        (cron.image.clone(), cron.command.clone())
    };

    // Create a job from the cron template
    let job_name = format!("{}-manual-{}", params.name, chrono::Utc::now().timestamp());
    let entry = JobEntry {
        name: job_name.clone(),
        image: image.clone(),
        command,
        completions: 1,
        completed: 0,
        failed: 0,
        parallelism: 1,
        backoff_limit: 3,
        container_ids: Vec::new(),
        state: "running".to_string(),
        created_at: chrono::Utc::now(),
        finished_at: None,
    };

    let mut job_store = state.job_store.write().await;
    job_store
        .create(entry)
        .map_err(|e| -> CommandError { e.into() })?;

    // Update cron last_run
    let mut cron_store = state.cron_store.write().await;
    if let Some(cron) = cron_store.get_mut(&params.name) {
        cron.last_run = Some(chrono::Utc::now());
        cron_store.update();
    }

    Ok(json!({
        "triggered": true,
        "cronName": params.name,
        "jobName": job_name,
        "image": image,
    }))
}

async fn handle_cron_suspend(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CronIdentifyParams = serde_json::from_value(params)?;

    let mut store = state.cron_store.write().await;
    let cron = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("cron '{}' not found", params.name))?;

    cron.suspended = true;
    store.update();

    Ok(json!({
        "name": params.name,
        "suspended": true,
    }))
}

async fn handle_cron_resume(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: CronIdentifyParams = serde_json::from_value(params)?;

    let mut store = state.cron_store.write().await;
    let cron = store
        .get_mut(&params.name)
        .ok_or_else(|| format!("cron '{}' not found", params.name))?;

    cron.suspended = false;
    store.update();

    Ok(json!({
        "name": params.name,
        "suspended": false,
        "resumed": true,
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
    async fn test_job_create_and_status() {
        let state = test_state();

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "job.create".to_string(),
                params: json!({
                    "name": "train-v1",
                    "image": "pytorch:latest",
                    "command": ["python", "train.py"],
                    "completions": 3,
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);
        assert_eq!(result["completions"], 3);

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "job.status".to_string(),
                params: json!({"name": "train-v1"}),
            },
        )
        .await
        .expect("status");
        assert_eq!(result["state"], "running");
        assert_eq!(result["completions"], "0/3");
    }

    #[tokio::test]
    async fn test_job_delete() {
        let state = test_state();

        handle_job_command(
            &state,
            CommandRequest {
                command: "job.create".to_string(),
                params: json!({"name": "del-job", "image": "test:v1"}),
            },
        )
        .await
        .expect("create");

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "job.delete".to_string(),
                params: json!({"name": "del-job"}),
            },
        )
        .await
        .expect("delete");
        assert_eq!(result["deleted"], true);
    }

    #[tokio::test]
    async fn test_cron_create_and_list() {
        let state = test_state();

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.create".to_string(),
                params: json!({
                    "name": "nightly-backup",
                    "schedule": "0 2 * * *",
                    "image": "backup:v1",
                    "command": ["backup.sh"],
                }),
            },
        )
        .await
        .expect("create");
        assert_eq!(result["success"], true);

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.list".to_string(),
                params: json!({}),
            },
        )
        .await
        .expect("list");
        assert_eq!(result["count"], 1);
        assert_eq!(result["crons"][0]["name"], "nightly-backup");
    }

    #[tokio::test]
    async fn test_cron_suspend_resume() {
        let state = test_state();

        handle_job_command(
            &state,
            CommandRequest {
                command: "cron.create".to_string(),
                params: json!({
                    "name": "hourly",
                    "schedule": "0 * * * *",
                    "image": "task:v1",
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.suspend".to_string(),
                params: json!({"name": "hourly"}),
            },
        )
        .await
        .expect("suspend");
        assert_eq!(result["suspended"], true);

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.resume".to_string(),
                params: json!({"name": "hourly"}),
            },
        )
        .await
        .expect("resume");
        assert_eq!(result["resumed"], true);
        assert_eq!(result["suspended"], false);
    }

    #[tokio::test]
    async fn test_cron_trigger() {
        let state = test_state();

        handle_job_command(
            &state,
            CommandRequest {
                command: "cron.create".to_string(),
                params: json!({
                    "name": "trigger-test",
                    "schedule": "0 0 * * *",
                    "image": "task:v1",
                }),
            },
        )
        .await
        .expect("create");

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.trigger".to_string(),
                params: json!({"name": "trigger-test"}),
            },
        )
        .await
        .expect("trigger");
        assert_eq!(result["triggered"], true);
        assert!(result["jobName"].as_str().unwrap().starts_with("trigger-test-manual-"));
    }

    #[tokio::test]
    async fn test_cron_invalid_schedule() {
        let state = test_state();

        let result = handle_job_command(
            &state,
            CommandRequest {
                command: "cron.create".to_string(),
                params: json!({
                    "name": "bad",
                    "schedule": "invalid",
                    "image": "x",
                }),
            },
        )
        .await;
        assert!(result.is_err());
    }
}
