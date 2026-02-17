//! Command handlers for node invocations
//!
//! Handles commands sent from the gateway like gpu.list, workload.run, etc.
//! When the `docker` feature is enabled and the Docker SDK is connected,
//! workload commands use the bollard-based runtime. Otherwise, they fall
//! back to shelling out to docker/podman CLI.

use crate::SharedState;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Command;
use tracing::{debug, info, warn};

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
        "system.which" => handle_system_which(request.params).await,
        "workload.run" => handle_workload_run(state, request.params).await,
        "workload.stop" => handle_workload_stop(state, request.params).await,
        "workload.logs" => handle_workload_logs(state, request.params).await,
        "workload.list" => handle_workload_list(state, request.params).await,
        "workload.inspect" => handle_workload_inspect(state, request.params).await,
        "workload.stats" => handle_workload_stats(state, request.params).await,
        "container.exec" => handle_container_exec(state, request.params).await,
        "node.capabilities" => handle_node_capabilities(state).await,
        "node.health" => handle_node_health(state).await,
        // Config commands (always available)
        "config.create" | "config.get" | "config.update" | "config.delete" | "config.list" => {
            crate::config_cmd::handle_config_command(state, request).await
        }
        // Secret commands (requires `secrets` feature)
        #[cfg(feature = "secrets")]
        "secret.create" | "secret.get" | "secret.delete" | "secret.list" | "secret.rotate" => {
            crate::secrets_cmd::handle_secret_command(state, request).await
        }
        // Metrics, events, and alerts commands (requires `metrics` feature)
        #[cfg(feature = "metrics")]
        "metrics.query" | "metrics.list" | "metrics.snapshot"
        | "events.query" | "events.emit"
        | "alerts.create" | "alerts.list" | "alerts.acknowledge" => {
            crate::metrics_cmd::handle_metrics_command(state, request).await
        }
        // Deploy commands (requires `deploy` feature)
        #[cfg(feature = "deploy")]
        "deploy.create" | "deploy.status" | "deploy.update" | "deploy.rollback"
        | "deploy.history" | "deploy.promote" | "deploy.pause" | "deploy.delete" => {
            crate::deploy_cmd::handle_deploy_command(state, request).await
        }
        // Tier 4 — Jobs & Cron (always available)
        "job.create" | "job.status" | "job.logs" | "job.delete"
        | "cron.create" | "cron.list" | "cron.trigger" | "cron.suspend" | "cron.resume" => {
            crate::job_cmd::handle_job_command(state, request).await
        }
        // Tier 5 — Networking (requires `network` feature)
        #[cfg(feature = "network")]
        "service.create" | "service.get" | "service.delete" | "service.list" | "service.endpoints"
        | "ingress.create" | "ingress.delete" | "network.status"
        | "network.policy.create" | "network.policy.delete" | "network.policy.list" => {
            crate::network_cmd::handle_network_command(state, request).await
        }
        // Tier 6 — Storage (requires `storage` feature)
        #[cfg(feature = "storage")]
        "volume.create" | "volume.mount" | "volume.unmount" | "volume.snapshot"
        | "volume.list" | "volume.delete" | "backup.create" | "backup.restore" | "backup.list" => {
            crate::storage_cmd::handle_storage_command(state, request).await
        }
        // Tier 7 — Auth & RBAC (requires `auth` feature)
        #[cfg(feature = "auth")]
        "auth.create_key" | "auth.revoke_key" | "auth.list_keys"
        | "rbac.create_role" | "rbac.bind" | "rbac.check" | "audit.query" => {
            crate::auth_cmd::handle_auth_command(state, request).await
        }
        // Tier 8 — Namespaces (always available)
        "namespace.create" | "namespace.set_quota" | "namespace.usage" | "namespace.list"
        | "node.label" | "node.taint" | "node.drain" => {
            crate::namespace_cmd::handle_namespace_command(state, request).await
        }
        // Tier 9 — Autoscaling (requires `autoscaler` feature)
        #[cfg(feature = "autoscaler")]
        "autoscale.create" | "autoscale.status" | "autoscale.adjust" | "autoscale.delete" => {
            crate::autoscale_cmd::handle_autoscale_command(state, request).await
        }
        // Tier 10 — MOLT Marketplace (requires `molt` feature)
        #[cfg(feature = "molt")]
        "molt.discover" | "molt.bid" | "molt.status" | "molt.balance" | "molt.reputation" => {
            crate::molt_cmd::handle_molt_command(state, request).await
        }
        // Tier 11 — Policy (always available)
        "policy.create" | "policy.validate" | "policy.list" => {
            crate::policy_cmd::handle_policy_command(state, request).await
        }
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
        "commands": state.commands,
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

/// Handle system.which — resolve binary paths (required by OpenClaw node protocol)
async fn handle_system_which(params: Value) -> Result<Value, CommandError> {
    #[derive(Deserialize)]
    struct WhichParams {
        #[serde(default)]
        bins: Vec<String>,
    }

    let params: WhichParams = serde_json::from_value(params)?;
    let mut results = serde_json::Map::new();

    for bin in &params.bins {
        let output = Command::new("which").arg(bin).output();
        match output {
            Ok(o) if o.status.success() => {
                let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
                results.insert(bin.clone(), json!(path));
            }
            _ => {
                results.insert(bin.clone(), Value::Null);
            }
        }
    }

    Ok(json!({ "bins": results }))
}

// ─────────────────────────────────────────────────────────────
// Node Commands
// ─────────────────────────────────────────────────────────────

async fn handle_node_capabilities(state: &SharedState) -> Result<Value, CommandError> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let state = state.read().await;
    let gpus = state.gpu_manager.list();

    Ok(json!({
        "hostname": state.config.hostname,
        "capabilities": state.capabilities,
        "commands": state.commands,
        "container_runtime": state.config.container_runtime,
        "cpu_count": sys.cpus().len(),
        "total_memory_mb": sys.total_memory() / 1024 / 1024,
        "gpus": gpus.iter().map(|g| json!({
            "index": g.index,
            "name": g.name,
            "memory_mb": g.memory_total_mb,
        })).collect::<Vec<_>>(),
        "gpu_count": gpus.len(),
        "gpu_total_memory_gb": state.gpu_manager.total_memory_gb(),
        "labels": state.config.labels,
    }))
}

async fn handle_node_health(state: &SharedState) -> Result<Value, CommandError> {
    use sysinfo::{Disks, System};

    let mut sys = System::new_all();
    sys.refresh_all();
    let disks = Disks::new_with_refreshed_list();

    let state = state.read().await;

    let load_avg = System::load_average();

    // Disk info for root filesystem
    let disk_info: Vec<Value> = disks
        .iter()
        .map(|d| {
            json!({
                "mount": d.mount_point().to_string_lossy(),
                "total_bytes": d.total_space(),
                "available_bytes": d.available_space(),
                "usage_percent": if d.total_space() > 0 {
                    ((d.total_space() - d.available_space()) as f64 / d.total_space() as f64) * 100.0
                } else {
                    0.0
                },
            })
        })
        .collect();

    Ok(json!({
        "status": "healthy",
        "hostname": state.config.hostname,
        "connected": state.connected,
        "cpu_count": sys.cpus().len(),
        "load_average": {
            "one": load_avg.one,
            "five": load_avg.five,
            "fifteen": load_avg.fifteen,
        },
        "memory": {
            "total_mb": sys.total_memory() / 1024 / 1024,
            "used_mb": sys.used_memory() / 1024 / 1024,
            "available_mb": sys.available_memory() / 1024 / 1024,
            "usage_percent": (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0,
        },
        "disks": disk_info,
        "gpu_count": state.gpu_manager.count(),
        "uptime_secs": System::uptime(),
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
    memory: Option<String>,
    cpu: Option<f32>,
    #[serde(rename = "shmSize")]
    shm_size: Option<String>,
}

/// Generate a workload ID for container labeling.
fn generate_workload_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(feature = "docker")]
async fn handle_workload_run(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadRunParams = serde_json::from_value(params)?;

    // If Docker SDK runtime is available, use it
    if let Some(ref docker) = state.docker_runtime {
        return handle_workload_run_sdk(state, docker, &params).await;
    }

    // Fall back to CLI
    handle_workload_run_cli(state, &params).await
}

#[cfg(not(feature = "docker"))]
async fn handle_workload_run(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadRunParams = serde_json::from_value(params)?;
    handle_workload_run_cli(state, &params).await
}

/// Run a workload via the Docker SDK (bollard).
#[cfg(feature = "docker")]
async fn handle_workload_run_sdk(
    state: &SharedState,
    docker: &crate::docker::DockerContainerRuntime,
    params: &WorkloadRunParams,
) -> Result<Value, CommandError> {
    use crate::docker::AsyncContainerRuntime;
    use crate::runtime::ContainerSpec;

    let workload_id = generate_workload_id();

    info!(image = %params.image, workload_id = %workload_id, "running workload via Docker SDK");

    let mut spec = ContainerSpec::new(&params.image)
        .with_label("managed-by", "clawbernetes")
        .with_label("workload-id", &workload_id);

    if let Some(ref cmd) = params.command {
        spec = spec.with_command(cmd.clone());
    }

    if let Some(ref env_vars) = params.env {
        for env in env_vars {
            if let Some((key, value)) = env.split_once('=') {
                spec = spec.with_env(key, value);
            }
        }
    }

    if let Some(gpus) = params.gpus {
        if gpus > 0 {
            let gpu_ids: Vec<u32> = (0..gpus).collect();
            spec = spec.with_gpus(gpu_ids);
        }
    }

    if let Some(ref memory) = params.memory {
        if let Some(bytes) = parse_memory_string(memory) {
            spec = spec.with_memory_limit(bytes);
        }
    }

    if let Some(cpu) = params.cpu {
        spec = spec.with_cpu_limit(cpu);
    }

    if let Some(ref name) = params.name {
        spec = spec.with_label("workload-name", name);
    }

    // Allocate a mesh IP if workload networking is available
    #[cfg(feature = "network")]
    let mesh_ip = {
        let mut wn_guard = state.workload_net.write().await;
        if let Some(ref mut wn) = *wn_guard {
            let ip = wn.allocate_ip(&workload_id)?;
            spec.network = Some(wn.network_name().to_string());
            spec.ip_address = Some(ip.to_string());
            Some(ip.to_string())
        } else {
            None
        }
    };

    let container = match docker.create(&spec).await {
        Ok(c) => c,
        Err(e) => {
            // Release the IP on failure
            #[cfg(feature = "network")]
            {
                let mut wn_guard = state.workload_net.write().await;
                if let Some(ref mut wn) = *wn_guard {
                    wn.release_ip(&workload_id);
                }
            }
            return Err(format!("Docker SDK container creation failed: {e}").into());
        }
    };

    // Track container_id → workload_id mapping for IP release on stop
    #[cfg(feature = "network")]
    if mesh_ip.is_some() {
        let mut wn_guard = state.workload_net.write().await;
        if let Some(ref mut wn) = *wn_guard {
            wn.track_container(&container.id, &workload_id);
        }
    }

    let mut result = json!({
        "containerId": container.id,
        "workloadId": workload_id,
        "image": params.image,
        "name": params.name,
        "success": true,
        "runtime": "docker-sdk",
    });

    #[cfg(feature = "network")]
    if let Some(ip) = mesh_ip {
        result["meshIp"] = json!(ip);
        result["network"] = json!(spec.network);
    }

    Ok(result)
}

/// Run a workload by shelling out to docker/podman CLI.
async fn handle_workload_run_cli(
    state: &SharedState,
    params: &WorkloadRunParams,
) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let workload_id = generate_workload_id();

    info!(image = %params.image, runtime = %runtime, workload_id = %workload_id, "running workload via CLI");

    // Allocate a mesh IP if workload networking is available
    #[cfg(feature = "network")]
    let mesh_ip = {
        let mut wn_guard = state.workload_net.write().await;
        if let Some(ref mut wn) = *wn_guard {
            match wn.allocate_ip(&workload_id) {
                Ok(ip) => Some((ip.to_string(), wn.network_name().to_string())),
                Err(e) => {
                    warn!(error = %e, "failed to allocate workload IP, using default network");
                    None
                }
            }
        } else {
            None
        }
    };

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

    // Lifecycle labels
    cmd.args(["--label", "managed-by=clawbernetes"]);
    cmd.args(["--label", &format!("workload-id={workload_id}")]);

    // Attach to mesh network if IP was allocated
    #[cfg(feature = "network")]
    if let Some((ref ip, ref net)) = mesh_ip {
        cmd.args(["--network", net]);
        cmd.args(["--ip", ip]);
    }

    // GPU access
    if let Some(gpus) = params.gpus {
        if gpus > 0 {
            if runtime == "docker" {
                cmd.args([
                    "--gpus",
                    &format!(
                        "\"device={}\"",
                        (0..gpus)
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                ]);
            } else if runtime == "podman" {
                cmd.args(["--device", "nvidia.com/gpu=all"]);
            }
        }
    }

    // Resource limits
    if let Some(ref memory) = params.memory {
        cmd.args(["--memory", memory]);
    }

    if let Some(cpu) = params.cpu {
        cmd.args(["--cpus", &format!("{cpu}")]);
    }

    if let Some(ref shm_size) = params.shm_size {
        cmd.args(["--shm-size", shm_size]);
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
    let stdout = String::from_utf8_lossy(&output.stdout)
        .to_string()
        .trim()
        .to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        let mut result = json!({
            "containerId": stdout,
            "workloadId": workload_id,
            "image": params.image,
            "name": params.name,
            "success": true,
            "runtime": "cli",
        });

        #[cfg(feature = "network")]
        if let Some((ip, net)) = mesh_ip {
            result["meshIp"] = json!(ip);
            result["network"] = json!(net);
        }

        Ok(result)
    } else {
        // Release IP on failure
        #[cfg(feature = "network")]
        {
            let mut wn_guard = state.workload_net.write().await;
            if let Some(ref mut wn) = *wn_guard {
                wn.release_ip(&workload_id);
            }
        }
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

#[cfg(feature = "docker")]
async fn handle_workload_stop(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadStopParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    if let Some(ref docker) = state.docker_runtime {
        use crate::docker::AsyncContainerRuntime;

        info!(target = %target, "stopping workload via Docker SDK");

        let timeout = if params.force.unwrap_or(false) { 0 } else { 10 };
        docker.stop(target, timeout).await.map_err(|e| {
            format!("stop failed: {e}")
        })?;

        // Release workload IP if allocated
        #[cfg(feature = "network")]
        {
            let mut wn_guard = state.workload_net.write().await;
            if let Some(ref mut wn) = *wn_guard {
                if let Some(ip) = wn.release_by_container(target) {
                    info!(container = %target, ip = %ip, "released workload IP");
                }
            }
        }

        return Ok(json!({
            "stopped": target,
            "success": true,
            "runtime": "docker-sdk",
        }));
    }

    handle_workload_stop_cli(state, target, params.force.unwrap_or(false)).await
}

#[cfg(not(feature = "docker"))]
async fn handle_workload_stop(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadStopParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    handle_workload_stop_cli(state, target, params.force.unwrap_or(false)).await
}

async fn handle_workload_stop_cli(
    state: &SharedState,
    target: &str,
    force: bool,
) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    info!(target = %target, "stopping workload via CLI");

    let mut cmd = Command::new(&runtime);

    if force {
        cmd.args(["kill", target]);
    } else {
        cmd.args(["stop", target]);
    }

    let output = cmd.output()?;

    if output.status.success() {
        // Release workload IP if allocated
        #[cfg(feature = "network")]
        {
            let mut wn_guard = state.workload_net.write().await;
            if let Some(ref mut wn) = *wn_guard {
                if let Some(ip) = wn.release_by_container(target) {
                    info!(container = %target, ip = %ip, "released workload IP");
                }
            }
        }

        Ok(json!({
            "stopped": target,
            "success": true,
            "runtime": "cli",
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

#[cfg(feature = "docker")]
async fn handle_workload_logs(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadLogsParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    if let Some(ref docker) = state.docker_runtime {
        use crate::docker::AsyncContainerRuntime;

        let lines = docker
            .logs(target, params.tail.map(|t| t as usize))
            .await
            .map_err(|e| format!("logs failed: {e}"))?;

        return Ok(json!({
            "logs": lines.join("\n"),
            "container": target,
            "runtime": "docker-sdk",
        }));
    }

    handle_workload_logs_cli(state, target, params.tail).await
}

#[cfg(not(feature = "docker"))]
async fn handle_workload_logs(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadLogsParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    handle_workload_logs_cli(state, target, params.tail).await
}

async fn handle_workload_logs_cli(
    state: &SharedState,
    target: &str,
    tail: Option<u32>,
) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let mut cmd = Command::new(&runtime);
    cmd.arg("logs");

    if let Some(tail) = tail {
        cmd.args(["--tail", &tail.to_string()]);
    }

    cmd.arg(target);

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(json!({
        "logs": stdout,
        "stderr": stderr,
        "container": target,
        "runtime": "cli",
    }))
}

// ─────────────────────────────────────────────────────────────
// New Workload Commands
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WorkloadListParams {
    all: Option<bool>,
}

#[cfg(feature = "docker")]
async fn handle_workload_list(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let _params: WorkloadListParams =
        serde_json::from_value(params).unwrap_or(WorkloadListParams { all: None });

    if let Some(ref docker) = state.docker_runtime {
        use crate::docker::AsyncContainerRuntime;

        let containers = docker
            .list()
            .await
            .map_err(|e| format!("list failed: {e}"))?;

        let workloads: Vec<Value> = containers
            .iter()
            .map(|c| {
                json!({
                    "containerId": c.id,
                    "image": c.image,
                    "state": c.state,
                    "gpus": c.gpu_ids,
                    "createdAt": c.created_at.to_rfc3339(),
                    "labels": c.labels,
                    "workloadId": c.labels.get("workload-id"),
                })
            })
            .collect();

        return Ok(json!({
            "count": workloads.len(),
            "workloads": workloads,
            "runtime": "docker-sdk",
        }));
    }

    handle_workload_list_cli(state).await
}

#[cfg(not(feature = "docker"))]
async fn handle_workload_list(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let _params: WorkloadListParams =
        serde_json::from_value(params).unwrap_or(WorkloadListParams { all: None });
    handle_workload_list_cli(state).await
}

async fn handle_workload_list_cli(state: &SharedState) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let output = Command::new(&runtime)
        .args([
            "ps",
            "-a",
            "--filter",
            "label=managed-by=clawbernetes",
            "--format",
            "{{.ID}}\t{{.Image}}\t{{.Status}}\t{{.Names}}\t{{.Labels}}",
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    let workloads: Vec<Value> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            json!({
                "containerId": parts.first().unwrap_or(&""),
                "image": parts.get(1).unwrap_or(&""),
                "status": parts.get(2).unwrap_or(&""),
                "name": parts.get(3).unwrap_or(&""),
            })
        })
        .collect();

    Ok(json!({
        "count": workloads.len(),
        "workloads": workloads,
        "runtime": "cli",
    }))
}

#[derive(Debug, Deserialize)]
struct WorkloadInspectParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
}

#[cfg(feature = "docker")]
async fn handle_workload_inspect(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: WorkloadInspectParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    if let Some(ref docker) = state.docker_runtime {
        use crate::docker::AsyncContainerRuntime;

        let container = docker
            .get(target)
            .await
            .map_err(|e| format!("inspect failed: {e}"))?;

        return Ok(json!({
            "containerId": container.id,
            "image": container.image,
            "state": container.state,
            "gpus": container.gpu_ids,
            "createdAt": container.created_at.to_rfc3339(),
            "exitCode": container.exit_code,
            "labels": container.labels,
            "workloadId": container.labels.get("workload-id"),
            "runtime": "docker-sdk",
        }));
    }

    handle_workload_inspect_cli(state, target).await
}

#[cfg(not(feature = "docker"))]
async fn handle_workload_inspect(
    state: &SharedState,
    params: Value,
) -> Result<Value, CommandError> {
    let params: WorkloadInspectParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    handle_workload_inspect_cli(state, target).await
}

async fn handle_workload_inspect_cli(
    state: &SharedState,
    target: &str,
) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let output = Command::new(&runtime)
        .args(["inspect", target, "--format", "{{json .}}"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("inspect failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let inspect_data: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| json!({"raw": stdout}));

    Ok(json!({
        "containerId": target,
        "inspect": inspect_data,
        "runtime": "cli",
    }))
}

#[derive(Debug, Deserialize)]
struct WorkloadStatsParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
}

async fn handle_workload_stats(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: WorkloadStatsParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    // Use docker stats --no-stream for a single snapshot
    let output = Command::new(&runtime)
        .args([
            "stats",
            "--no-stream",
            "--format",
            "{{json .}}",
            target,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("stats failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stats_data: Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|_| json!({"raw": stdout.trim()}));

    Ok(json!({
        "container": target,
        "stats": stats_data,
    }))
}

// ─────────────────────────────────────────────────────────────
// Container Exec
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ContainerExecParams {
    #[serde(rename = "containerId")]
    container_id: Option<String>,
    name: Option<String>,
    command: Vec<String>,
    workdir: Option<String>,
}

#[cfg(feature = "docker")]
async fn handle_container_exec(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ContainerExecParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    if params.command.is_empty() {
        return Err("command required".into());
    }

    // Docker SDK doesn't expose exec directly through our AsyncContainerRuntime trait,
    // so we always use CLI for exec
    handle_container_exec_cli(state, target, &params.command, params.workdir.as_deref()).await
}

#[cfg(not(feature = "docker"))]
async fn handle_container_exec(state: &SharedState, params: Value) -> Result<Value, CommandError> {
    let params: ContainerExecParams = serde_json::from_value(params)?;

    let target = params
        .container_id
        .as_deref()
        .or(params.name.as_deref())
        .ok_or("containerId or name required")?;

    if params.command.is_empty() {
        return Err("command required".into());
    }

    handle_container_exec_cli(state, target, &params.command, params.workdir.as_deref()).await
}

async fn handle_container_exec_cli(
    state: &SharedState,
    target: &str,
    command: &[String],
    workdir: Option<&str>,
) -> Result<Value, CommandError> {
    let runtime = {
        let s = state.read().await;
        s.config.container_runtime.clone()
    };

    let mut cmd = Command::new(&runtime);
    cmd.arg("exec");

    if let Some(workdir) = workdir {
        cmd.args(["-w", workdir]);
    }

    cmd.arg(target);
    cmd.args(command);

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
// Helpers
// ─────────────────────────────────────────────────────────────

/// Parse a human-readable memory string like "8g", "512m", "1024k" to bytes.
fn parse_memory_string(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Some(num) = s.strip_suffix('g') {
        num.parse::<u64>().ok().map(|n| n * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("gb") {
        num.parse::<u64>().ok().map(|n| n * 1024 * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>().ok().map(|n| n * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix("mb") {
        num.parse::<u64>().ok().map(|n| n * 1024 * 1024)
    } else if let Some(num) = s.strip_suffix('k') {
        num.parse::<u64>().ok().map(|n| n * 1024)
    } else if let Some(num) = s.strip_suffix("kb") {
        num.parse::<u64>().ok().map(|n| n * 1024)
    } else {
        s.parse::<u64>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_string_gigabytes() {
        assert_eq!(parse_memory_string("8g"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("8gb"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_string("1G"), Some(1024 * 1024 * 1024));
    }

    #[test]
    fn test_parse_memory_string_megabytes() {
        assert_eq!(parse_memory_string("512m"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_string("512mb"), Some(512 * 1024 * 1024));
    }

    #[test]
    fn test_parse_memory_string_kilobytes() {
        assert_eq!(parse_memory_string("1024k"), Some(1024 * 1024));
        assert_eq!(parse_memory_string("1024kb"), Some(1024 * 1024));
    }

    #[test]
    fn test_parse_memory_string_bytes() {
        assert_eq!(parse_memory_string("1048576"), Some(1048576));
    }

    #[test]
    fn test_parse_memory_string_invalid() {
        assert_eq!(parse_memory_string("abc"), None);
        assert_eq!(parse_memory_string(""), None);
    }

    #[test]
    fn test_generate_workload_id() {
        let id1 = generate_workload_id();
        let id2 = generate_workload_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 36); // UUID format
    }
}
