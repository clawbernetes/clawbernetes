//! Command handlers for node invocations
//!
//! Handles commands sent from the gateway like gpu.list, workload.run, etc.

use crate::SharedState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use tracing::{debug, error, info};

/// Command request from gateway
#[derive(Debug, Clone, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    pub params: Value,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: Option<String>,
}

/// Command response to gateway
#[derive(Debug, Clone, Serialize)]
pub struct CommandResponse {
    pub success: bool,
    pub payload: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CommandResponse {
    pub fn success(payload: Value) -> Self {
        Self {
            success: true,
            payload,
            error: None,
        }
    }
    
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            payload: Value::Null,
            error: Some(message.into()),
        }
    }
}

/// Handle an incoming command
pub async fn handle_command(
    state: SharedState,
    request: CommandRequest,
) -> CommandResponse {
    debug!(command = %request.command, "handling command");
    
    match request.command.as_str() {
        "gpu.list" => handle_gpu_list(state).await,
        "gpu.metrics" => handle_gpu_metrics(state).await,
        "system.info" => handle_system_info(state).await,
        "system.run" => handle_system_run(state, request.params).await,
        "workload.run" => handle_workload_run(state, request.params).await,
        "workload.stop" => handle_workload_stop(state, request.params).await,
        "workload.logs" => handle_workload_logs(state, request.params).await,
        "container.exec" => handle_container_exec(state, request.params).await,
        _ => CommandResponse::error(format!("unknown command: {}", request.command)),
    }
}

// ─────────────────────────────────────────────────────────────
// GPU Commands
// ─────────────────────────────────────────────────────────────

async fn handle_gpu_list(state: SharedState) -> CommandResponse {
    let state = state.read().await;
    let gpus = state.gpu_manager.list();
    
    CommandResponse::success(json!({
        "count": gpus.len(),
        "gpus": gpus,
        "total_memory_gb": state.gpu_manager.total_memory_gb(),
    }))
}

async fn handle_gpu_metrics(state: SharedState) -> CommandResponse {
    let state = state.read().await;
    
    match state.gpu_manager.get_metrics() {
        Ok(metrics) => CommandResponse::success(json!({
            "count": metrics.len(),
            "metrics": metrics,
        })),
        Err(e) => CommandResponse::error(format!("failed to get GPU metrics: {}", e)),
    }
}

// ─────────────────────────────────────────────────────────────
// System Commands
// ─────────────────────────────────────────────────────────────

async fn handle_system_info(state: SharedState) -> CommandResponse {
    use sysinfo::System;
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let state = state.read().await;
    
    CommandResponse::success(json!({
        "hostname": state.config.hostname,
        "os": System::name().unwrap_or_default(),
        "os_version": System::os_version().unwrap_or_default(),
        "kernel_version": System::kernel_version().unwrap_or_default(),
        "cpu_count": sys.cpus().len(),
        "total_memory_mb": sys.total_memory() / 1024 / 1024,
        "available_memory_mb": sys.available_memory() / 1024 / 1024,
        "used_memory_mb": sys.used_memory() / 1024 / 1024,
        "gpu_count": state.gpu_manager.count(),
        "gpu_total_memory_gb": state.gpu_manager.total_memory_gb(),
        "capabilities": state.gpu_manager.capabilities(),
        "commands": state.gpu_manager.commands(),
    }))
}

#[derive(Debug, Deserialize)]
struct SystemRunParams {
    command: Vec<String>,
    cwd: Option<String>,
    env: Option<Vec<String>>,
    #[serde(rename = "timeoutMs")]
    timeout_ms: Option<u64>,
}

async fn handle_system_run(_state: SharedState, params: Value) -> CommandResponse {
    let params: SystemRunParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("invalid params: {}", e)),
    };
    
    if params.command.is_empty() {
        return CommandResponse::error("command required");
    }
    
    info!(cmd = ?params.command, "executing system.run");
    
    let mut cmd = Command::new(&params.command[0]);
    cmd.args(&params.command[1..]);
    
    if let Some(cwd) = &params.cwd {
        cmd.current_dir(cwd);
    }
    
    if let Some(env_vars) = &params.env {
        for env in env_vars {
            if let Some((key, value)) = env.split_once('=') {
                cmd.env(key, value);
            }
        }
    }
    
    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            
            CommandResponse::success(json!({
                "exitCode": output.status.code().unwrap_or(-1),
                "stdout": stdout,
                "stderr": stderr,
                "success": output.status.success(),
            }))
        }
        Err(e) => CommandResponse::error(format!("failed to execute: {}", e)),
    }
}

// ─────────────────────────────────────────────────────────────
// Workload Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WorkloadRunParams {
    image: String,
    name: Option<String>,
    gpus: Option<u32>,
    command: Option<Vec<String>>,
    env: Option<Vec<String>>,
    volumes: Option<Vec<String>>,
    detach: Option<bool>,
}

async fn handle_workload_run(state: SharedState, params: Value) -> CommandResponse {
    let params: WorkloadRunParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("invalid params: {}", e)),
    };
    
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };
    
    info!(image = %params.image, runtime = %runtime, "running workload");
    
    let mut cmd = Command::new(&runtime);
    cmd.arg("run");
    
    // Detach by default for workloads
    if params.detach.unwrap_or(true) {
        cmd.arg("-d");
    }
    
    // Container name
    if let Some(name) = &params.name {
        cmd.args(["--name", name]);
    }
    
    // GPU access
    if let Some(gpus) = params.gpus {
        if gpus > 0 {
            if runtime == "docker" {
                cmd.args(["--gpus", &format!("\"device={}\"", 
                    (0..gpus).map(|i| i.to_string()).collect::<Vec<_>>().join(","))]);
            } else if runtime == "podman" {
                // Podman uses --device for GPU access
                cmd.args(["--device", "nvidia.com/gpu=all"]);
            }
        }
    }
    
    // Environment variables
    if let Some(env_vars) = &params.env {
        for env in env_vars {
            cmd.args(["-e", env]);
        }
    }
    
    // Volume mounts
    if let Some(volumes) = &params.volumes {
        for vol in volumes {
            cmd.args(["-v", vol]);
        }
    }
    
    // Image
    cmd.arg(&params.image);
    
    // Command
    if let Some(command) = &params.command {
        cmd.args(command);
    }
    
    debug!(cmd = ?cmd, "executing container run");
    
    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string().trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            
            if output.status.success() {
                CommandResponse::success(json!({
                    "containerId": stdout,
                    "image": params.image,
                    "name": params.name,
                    "success": true,
                }))
            } else {
                CommandResponse::error(format!("container run failed: {}", stderr))
            }
        }
        Err(e) => CommandResponse::error(format!("failed to execute: {}", e)),
    }
}

#[derive(Debug, Deserialize)]
struct WorkloadStopParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    force: Option<bool>,
}

async fn handle_workload_stop(state: SharedState, params: Value) -> CommandResponse {
    let params: WorkloadStopParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("invalid params: {}", e)),
    };
    
    let target = params.container_id.or(params.name)
        .ok_or_else(|| "containerId or name required");
    
    let target = match target {
        Ok(t) => t,
        Err(e) => return CommandResponse::error(e),
    };
    
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };
    
    info!(target = %target, "stopping workload");
    
    let mut cmd = Command::new(&runtime);
    
    if params.force.unwrap_or(false) {
        cmd.args(["kill", &target]);
    } else {
        cmd.args(["stop", &target]);
    }
    
    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                CommandResponse::success(json!({
                    "stopped": target,
                    "success": true,
                }))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                CommandResponse::error(format!("stop failed: {}", stderr))
            }
        }
        Err(e) => CommandResponse::error(format!("failed to execute: {}", e)),
    }
}

#[derive(Debug, Deserialize)]
struct WorkloadLogsParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    tail: Option<u32>,
    follow: Option<bool>,
}

async fn handle_workload_logs(state: SharedState, params: Value) -> CommandResponse {
    let params: WorkloadLogsParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("invalid params: {}", e)),
    };
    
    let target = params.container_id.or(params.name)
        .ok_or_else(|| "containerId or name required");
    
    let target = match target {
        Ok(t) => t,
        Err(e) => return CommandResponse::error(e),
    };
    
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };
    
    let mut cmd = Command::new(&runtime);
    cmd.arg("logs");
    
    if let Some(tail) = params.tail {
        cmd.args(["--tail", &tail.to_string()]);
    }
    
    // Note: follow=true would need streaming support
    cmd.arg(&target);
    
    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            
            CommandResponse::success(json!({
                "logs": stdout,
                "stderr": stderr,
                "container": target,
            }))
        }
        Err(e) => CommandResponse::error(format!("failed to execute: {}", e)),
    }
}

#[derive(Debug, Deserialize)]
struct ContainerExecParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    command: Vec<String>,
    workdir: Option<String>,
}

async fn handle_container_exec(state: SharedState, params: Value) -> CommandResponse {
    let params: ContainerExecParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return CommandResponse::error(format!("invalid params: {}", e)),
    };
    
    let target = params.container_id.or(params.name)
        .ok_or_else(|| "containerId or name required");
    
    let target = match target {
        Ok(t) => t,
        Err(e) => return CommandResponse::error(e),
    };
    
    if params.command.is_empty() {
        return CommandResponse::error("command required");
    }
    
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };
    
    let mut cmd = Command::new(&runtime);
    cmd.arg("exec");
    
    if let Some(workdir) = &params.workdir {
        cmd.args(["-w", workdir]);
    }
    
    cmd.arg(&target);
    cmd.args(&params.command);
    
    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            
            CommandResponse::success(json!({
                "exitCode": output.status.code().unwrap_or(-1),
                "stdout": stdout,
                "stderr": stderr,
                "success": output.status.success(),
            }))
        }
        Err(e) => CommandResponse::error(format!("failed to execute: {}", e)),
    }
}
