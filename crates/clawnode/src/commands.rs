//! Command handlers for node invocations
//!
//! Handles commands sent from the gateway like gpu.list, workload.run, etc.

use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use tracing::{debug, info};

/// Command request from gateway
#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub command: String,
    pub params: Value,
}

/// Command error type
pub type CommandError = Box<dyn std::error::Error + Send + Sync>;

/// Handle an incoming command
pub async fn handle_command(
    state: &SharedState,
    request: CommandRequest,
) -> Result<Value, CommandError> {
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
        _ => Err(format!("unknown command: {}", request.command).into()),
    }
}

// ─────────────────────────────────────────────────────────────
// GPU Commands
// ─────────────────────────────────────────────────────────────

async fn handle_gpu_list(state: &SharedState) -> Result<Value, CommandError> {
    let state = state.read().await;
    let gpus = state.gpu_manager.list();
    
    Ok(json!({
        "count": gpus.len(),
        "gpus": gpus,
        "total_memory_gb": state.gpu_manager.total_memory_gb(),
    }))
}

async fn handle_gpu_metrics(state: &SharedState) -> Result<Value, CommandError> {
    let state = state.read().await;
    let metrics = state.gpu_manager.get_metrics()?;
    
    Ok(json!({
        "count": metrics.len(),
        "metrics": metrics,
    }))
}

// ─────────────────────────────────────────────────────────────
// System Commands
// ─────────────────────────────────────────────────────────────

async fn handle_system_info(state: &SharedState) -> Result<Value, CommandError> {
    use sysinfo::System;
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let state = state.read().await;
    
    Ok(json!({
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

async fn handle_system_run(_state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: SystemRunParams = serde_json::from_value(params)?;
    
    if params.command.is_empty() {
        return Err("command required".into());
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
    
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    Ok(json!({
        "exitCode": output.status.code().unwrap_or(-1),
        "stdout": stdout,
        "stderr": stderr,
        "success": output.status.success(),
    }))
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

async fn handle_workload_run(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadRunParams = serde_json::from_value(params)?;
    
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
    
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string().trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    if output.status.success() {
        Ok(json!({
            "containerId": stdout,
            "image": params.image,
            "name": params.name,
            "success": true,
        }))
    } else {
        Err(format!("container run failed: {}", stderr).into())
    }
}

#[derive(Debug, Deserialize)]
struct WorkloadStopParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    force: Option<bool>,
}

async fn handle_workload_stop(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadStopParams = serde_json::from_value(params)?;
    
    let target = params.container_id.or(params.name)
        .ok_or("containerId or name required")?;
    
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
    
    let output = cmd.output()?;
    
    if output.status.success() {
        Ok(json!({
            "stopped": target,
            "success": true,
        }))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("stop failed: {}", stderr).into())
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

async fn handle_workload_logs(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadLogsParams = serde_json::from_value(params)?;
    
    let target = params.container_id.or(params.name)
        .ok_or("containerId or name required")?;
    
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
    
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    Ok(json!({
        "logs": stdout,
        "stderr": stderr,
        "container": target,
    }))
}

#[derive(Debug, Deserialize)]
struct ContainerExecParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    command: Vec<String>,
    workdir: Option<String>,
}

async fn handle_container_exec(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ContainerExecParams = serde_json::from_value(params)?;
    
    let target = params.container_id.or(params.name)
        .ok_or("containerId or name required")?;
    
    if params.command.is_empty() {
        return Err("command required".into());
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
    
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    
    Ok(json!({
        "exitCode": output.status.code().unwrap_or(-1),
        "stdout": stdout,
        "stderr": stderr,
        "success": output.status.success(),
    }))
}
