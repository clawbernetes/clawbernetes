//! Docker runtime implementation using bollard.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::SystemTime;

use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, ListContainersOptions,
    LogsOptions as BollardLogsOptions, RemoveContainerOptions, StatsOptions,
    StopContainerOptions, WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{
    ContainerState as BollardState, ContainerStateStatusEnum, DeviceRequest,
    HealthStatusEnum, HostConfig, Mount, MountTypeEnum, PortBinding,
    RestartPolicy as BollardRestartPolicy, RestartPolicyNameEnum,
};
use bollard::Docker;
use futures::StreamExt;
use tracing::{debug, info, warn};

use super::config::{
    ContainerConfig, GpuRequirements, MountType, RestartPolicy, VolumeMount,
};
use super::error::{ContainerError, ContainerId, ContainerResult};
use super::runtime::{
    ContainerRuntime, ExecOptions, ExecResult, ListOptions, LogsOptions, RemoveOptions,
    RuntimeInfo, StopOptions,
};
use super::status::{
    ContainerState, ContainerStatus, ContainerSummary, HealthStatus, NetworkEndpoint,
    NetworkSettings, PortBinding as StatusPortBinding, ResourceStats,
};

/// Docker container runtime implementation.
pub struct DockerRuntime {
    client: Docker,
    gpu_runtime: Option<String>,
}

impl DockerRuntime {
    /// Connect to Docker daemon using default connection method.
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub fn connect() -> ContainerResult<Self> {
        let client = Docker::connect_with_local_defaults().map_err(|e| {
            ContainerError::ConnectionFailed(format!("failed to connect to Docker: {e}"))
        })?;

        Ok(Self {
            client,
            gpu_runtime: None,
        })
    }

    /// Connect to Docker daemon at a specific URL.
    ///
    /// # Errors
    ///
    /// Returns error if connection fails.
    pub fn connect_with_url(url: &str) -> ContainerResult<Self> {
        let client = Docker::connect_with_http(url, 120, bollard::API_DEFAULT_VERSION).map_err(
            |e| {
                ContainerError::ConnectionFailed(format!(
                    "failed to connect to Docker at {url}: {e}"
                ))
            },
        )?;

        Ok(Self {
            client,
            gpu_runtime: None,
        })
    }

    /// Set GPU runtime (e.g., "nvidia").
    #[must_use]
    pub fn with_gpu_runtime(mut self, runtime: impl Into<String>) -> Self {
        self.gpu_runtime = Some(runtime.into());
        self
    }

    /// Build Docker HostConfig from our config types.
    fn build_host_config(&self, config: &ContainerConfig) -> HostConfig {
        let mut host_config = HostConfig::default();

        // Memory limits
        if config.memory.limit > 0 {
            host_config.memory = Some(config.memory.limit as i64);
        }
        if config.memory.reservation > 0 {
            host_config.memory_reservation = Some(config.memory.reservation as i64);
        }
        if config.memory.swap != 0 {
            host_config.memory_swap = Some(config.memory.swap);
        }
        if let Some(swappiness) = config.memory.swappiness {
            host_config.memory_swappiness = Some(i64::from(swappiness));
        }
        host_config.oom_kill_disable = Some(config.memory.oom_kill_disable);

        // CPU limits
        if config.cpu.shares > 0 {
            host_config.cpu_shares = Some(config.cpu.shares as i64);
        }
        if config.cpu.quota > 0 {
            host_config.cpu_quota = Some(config.cpu.quota as i64);
        }
        if config.cpu.period > 0 {
            host_config.cpu_period = Some(config.cpu.period as i64);
        }
        if let Some(ref cpuset) = config.cpu.cpuset_cpus {
            host_config.cpuset_cpus = Some(cpuset.clone());
        }
        if let Some(ref mems) = config.cpu.cpuset_mems {
            host_config.cpuset_mems = Some(mems.clone());
        }
        if config.cpu.nano_cpus > 0 {
            host_config.nano_cpus = Some(config.cpu.nano_cpus as i64);
        }

        // Network mode
        host_config.network_mode = Some(config.network_mode.as_docker_mode());

        // Volumes/mounts
        let mounts: Vec<Mount> = config
            .volumes
            .iter()
            .map(|v| self.volume_to_mount(v))
            .collect();
        if !mounts.is_empty() {
            host_config.mounts = Some(mounts);
        }

        // Port bindings
        if !config.ports.is_empty() {
            let mut bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
            for (&container_port, &host_port) in &config.ports {
                let key = format!("{container_port}/tcp");
                bindings.insert(
                    key,
                    Some(vec![PortBinding {
                        host_ip: Some("0.0.0.0".to_string()),
                        host_port: Some(host_port.to_string()),
                    }]),
                );
            }
            host_config.port_bindings = Some(bindings);
        }

        // Restart policy
        host_config.restart_policy = Some(self.restart_policy_to_bollard(&config.restart_policy));

        // Privileged mode
        host_config.privileged = Some(config.privileged);

        // Auto remove
        host_config.auto_remove = Some(config.auto_remove);

        // GPU passthrough
        if config.gpu.is_enabled() {
            self.configure_gpu(&mut host_config, &config.gpu);
        }

        host_config
    }

    /// Configure GPU passthrough in host config.
    fn configure_gpu(&self, host_config: &mut HostConfig, gpu: &GpuRequirements) {
        // Use nvidia-container-runtime via device requests
        let mut device_request = DeviceRequest::default();

        // Set driver (nvidia)
        device_request.driver = Some("nvidia".to_string());

        // Set capabilities
        if !gpu.capabilities.is_empty() {
            device_request.capabilities = Some(vec![gpu.capabilities.clone()]);
        }

        // Set device IDs or count
        if gpu.devices.is_empty() {
            // Request all GPUs
            device_request.count = Some(-1); // -1 means all
        } else {
            // Request specific devices
            let device_ids: Vec<String> =
                gpu.devices.iter().map(|d| d.device_specifier()).collect();
            device_request.device_ids = Some(device_ids);
        }

        // Set options for driver requirements
        let mut options: HashMap<String, String> = HashMap::new();
        if let Some(ref driver_ver) = gpu.driver_version {
            options.insert("driver".to_string(), driver_ver.clone());
        }
        if let Some(ref cuda_ver) = gpu.cuda_version {
            options.insert("cuda".to_string(), cuda_ver.clone());
        }
        if !options.is_empty() {
            device_request.options = Some(options);
        }

        host_config.device_requests = Some(vec![device_request]);

        // Set runtime if specified
        if let Some(ref runtime) = self.gpu_runtime {
            host_config.runtime = Some(runtime.clone());
        }
    }

    /// Convert our VolumeMount to bollard Mount.
    fn volume_to_mount(&self, volume: &VolumeMount) -> Mount {
        Mount {
            target: Some(volume.target.clone()),
            source: Some(volume.source.clone()),
            typ: Some(match volume.mount_type {
                MountType::Bind => MountTypeEnum::BIND,
                MountType::Volume => MountTypeEnum::VOLUME,
                MountType::Tmpfs => MountTypeEnum::TMPFS,
            }),
            read_only: Some(volume.read_only),
            ..Default::default()
        }
    }

    /// Convert our RestartPolicy to bollard RestartPolicy.
    fn restart_policy_to_bollard(&self, policy: &RestartPolicy) -> BollardRestartPolicy {
        match policy {
            RestartPolicy::No => BollardRestartPolicy {
                name: Some(RestartPolicyNameEnum::NO),
                maximum_retry_count: None,
            },
            RestartPolicy::Always => BollardRestartPolicy {
                name: Some(RestartPolicyNameEnum::ALWAYS),
                maximum_retry_count: None,
            },
            RestartPolicy::OnFailure { max_retries } => BollardRestartPolicy {
                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                maximum_retry_count: Some(i64::from(*max_retries)),
            },
            RestartPolicy::UnlessStopped => BollardRestartPolicy {
                name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            },
        }
    }

    /// Convert bollard state to our ContainerState.
    fn bollard_state_to_state(&self, state: &Option<BollardState>) -> ContainerState {
        match state {
            Some(s) => {
                if s.running == Some(true) {
                    ContainerState::Running
                } else if s.paused == Some(true) {
                    ContainerState::Paused
                } else if s.restarting == Some(true) {
                    ContainerState::Starting
                } else if s.dead == Some(true) {
                    ContainerState::Error
                } else {
                    // Check status enum
                    match &s.status {
                        Some(ContainerStateStatusEnum::CREATED) => ContainerState::Created,
                        Some(ContainerStateStatusEnum::EXITED) => ContainerState::Exited,
                        Some(ContainerStateStatusEnum::RUNNING) => ContainerState::Running,
                        Some(ContainerStateStatusEnum::PAUSED) => ContainerState::Paused,
                        Some(ContainerStateStatusEnum::RESTARTING) => ContainerState::Starting,
                        Some(ContainerStateStatusEnum::DEAD) => ContainerState::Error,
                        Some(ContainerStateStatusEnum::REMOVING) => ContainerState::Stopping,
                        _ => ContainerState::Unknown,
                    }
                }
            }
            None => ContainerState::Unknown,
        }
    }

    /// Parse health status from bollard response.
    fn parse_health_status(&self, health: &Option<bollard::models::Health>) -> HealthStatus {
        match health {
            Some(h) => match &h.status {
                Some(HealthStatusEnum::HEALTHY) => HealthStatus::Healthy,
                Some(HealthStatusEnum::UNHEALTHY) => HealthStatus::Unhealthy,
                Some(HealthStatusEnum::STARTING) => HealthStatus::Starting,
                Some(HealthStatusEnum::NONE) | Some(HealthStatusEnum::EMPTY) | None => {
                    HealthStatus::None
                }
            },
            None => HealthStatus::None,
        }
    }

}

impl ContainerRuntime for DockerRuntime {
    fn create<'a>(
        &'a self,
        config: &'a ContainerConfig,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ContainerId>> + Send + 'a>> {
        Box::pin(async move {
            // Validate config first
            config.validate()?;

            debug!(name = %config.name, image = %config.image, "creating container");

            // Build environment variables
            let env: Vec<String> = config
                .env
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();

            // Build exposed ports
            let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();
            for &port in config.ports.keys() {
                exposed_ports.insert(format!("{port}/tcp"), HashMap::new());
            }

            // Build container config
            let docker_config = Config {
                image: Some(config.image.clone()),
                cmd: config.command.clone(),
                entrypoint: config.entrypoint.clone(),
                env: Some(env),
                working_dir: config.working_dir.clone(),
                user: config.user.clone(),
                hostname: config.hostname.clone(),
                labels: Some(config.labels.clone()),
                exposed_ports: if exposed_ports.is_empty() {
                    None
                } else {
                    Some(exposed_ports)
                },
                host_config: Some(self.build_host_config(config)),
                ..Default::default()
            };

            let options = CreateContainerOptions {
                name: config.name.clone(),
                platform: None,
            };

            let response = self
                .client
                .create_container(Some(options), docker_config)
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::ImageNotFound {
                        image: config.image.clone(),
                    },
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 409,
                        message,
                    } => ContainerError::CreateFailed(format!(
                        "container already exists: {message}"
                    )),
                    _ => ContainerError::CreateFailed(e.to_string()),
                })?;

            info!(id = %response.id, name = %config.name, "container created");

            Ok(ContainerId::new_unchecked(response.id))
        })
    }

    fn start<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            debug!(id = %id, "starting container");

            self.client
                .start_container::<String>(id.as_str(), None)
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::StartFailed {
                        id: id.to_string(),
                        reason: e.to_string(),
                    },
                })?;

            info!(id = %id, "container started");
            Ok(())
        })
    }

    fn stop<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a StopOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            debug!(id = %id, "stopping container");

            let stop_options = StopContainerOptions {
                t: options.timeout_secs.map(i64::from).unwrap_or(10),
            };

            match self
                .client
                .stop_container(id.as_str(), Some(stop_options))
                .await
            {
                Ok(()) => {
                    info!(id = %id, "container stopped");
                    Ok(())
                }
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => Err(ContainerError::NotFound { id: id.to_string() }),
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 304, ..
                }) => {
                    // Already stopped, not an error
                    Ok(())
                }
                Err(e) => Err(ContainerError::StopFailed {
                    id: id.to_string(),
                    reason: e.to_string(),
                }),
            }
        })
    }

    fn remove<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a RemoveOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            debug!(id = %id, force = options.force, "removing container");

            let remove_options = RemoveContainerOptions {
                force: options.force,
                v: options.volumes,
                ..Default::default()
            };

            self.client
                .remove_container(id.as_str(), Some(remove_options))
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::RemoveFailed {
                        id: id.to_string(),
                        reason: e.to_string(),
                    },
                })?;

            info!(id = %id, "container removed");
            Ok(())
        })
    }

    fn status<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ContainerStatus>> + Send + 'a>> {
        Box::pin(async move {
            let options = InspectContainerOptions { size: true };

            let inspect = self
                .client
                .inspect_container(id.as_str(), Some(options))
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::Internal(e.to_string()),
                })?;

            let state = self.bollard_state_to_state(&inspect.state);
            let health = inspect.state.as_ref().and_then(|s| s.health.as_ref());

            let mut status = ContainerStatus {
                id: inspect.id.unwrap_or_default(),
                name: inspect
                    .name
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default(),
                image: inspect
                    .config
                    .as_ref()
                    .and_then(|c| c.image.clone())
                    .unwrap_or_default(),
                state,
                health: self.parse_health_status(&health.cloned()),
                exit_code: inspect.state.as_ref().and_then(|s| s.exit_code),
                error: inspect.state.as_ref().and_then(|s| s.error.clone()),
                created_at: None, // Would need to parse ISO timestamp
                started_at: None,
                finished_at: None,
                stats: ResourceStats::default(),
                network: NetworkSettings::default(),
                ports: Vec::new(),
                labels: inspect
                    .config
                    .as_ref()
                    .and_then(|c| c.labels.clone())
                    .unwrap_or_default(),
                restart_count: inspect.restart_count.map(|c| c as u32).unwrap_or(0),
                platform: inspect.platform,
            };

            // Parse network settings
            if let Some(net) = inspect.network_settings {
                status.network.ip_address = net.ip_address;
                status.network.gateway = net.gateway;
                status.network.mac_address = net.mac_address;

                if let Some(networks) = net.networks {
                    for (name, endpoint) in networks {
                        status.network.networks.insert(
                            name,
                            NetworkEndpoint {
                                network_id: endpoint.network_id.unwrap_or_default(),
                                ip_address: endpoint.ip_address.unwrap_or_default(),
                                gateway: endpoint.gateway.unwrap_or_default(),
                                mac_address: endpoint.mac_address.unwrap_or_default(),
                                aliases: endpoint.aliases.unwrap_or_default(),
                            },
                        );
                    }
                }

                // Parse port bindings
                if let Some(ports) = net.ports {
                    for (port_spec, bindings) in ports {
                        if let Some(bindings) = bindings {
                            for binding in bindings {
                                // Parse port/protocol from spec like "80/tcp"
                                let parts: Vec<&str> = port_spec.split('/').collect();
                                let container_port = parts
                                    .first()
                                    .and_then(|p| p.parse::<u16>().ok())
                                    .unwrap_or(0);
                                let protocol = parts.get(1).unwrap_or(&"tcp").to_string();

                                status.ports.push(StatusPortBinding {
                                    container_port,
                                    protocol,
                                    host_ip: binding.host_ip,
                                    host_port: binding.host_port.and_then(|p| p.parse().ok()),
                                });
                            }
                        }
                    }
                }
            }

            Ok(status)
        })
    }

    fn list<'a>(
        &'a self,
        options: &'a ListOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<Vec<ContainerSummary>>> + Send + 'a>> {
        Box::pin(async move {
            let mut filters: HashMap<String, Vec<String>> = HashMap::new();

            if let Some(ref label) = options.label_filter {
                filters.insert("label".to_string(), vec![label.clone()]);
            }
            if let Some(ref name) = options.name_filter {
                filters.insert("name".to_string(), vec![name.clone()]);
            }
            if let Some(state) = options.status_filter {
                filters.insert("status".to_string(), vec![state.name().to_string()]);
            }

            let list_options = ListContainersOptions {
                all: options.all,
                limit: options.limit.map(|l| l as isize),
                filters,
                ..Default::default()
            };

            let containers = self
                .client
                .list_containers(Some(list_options))
                .await
                .map_err(|e| ContainerError::Internal(e.to_string()))?;

            let summaries: Vec<ContainerSummary> = containers
                .into_iter()
                .map(|c| {
                    let state = match c.state.as_deref() {
                        Some("running") => ContainerState::Running,
                        Some("paused") => ContainerState::Paused,
                        Some("created") => ContainerState::Created,
                        Some("exited") => ContainerState::Exited,
                        Some("dead") => ContainerState::Error,
                        _ => ContainerState::Unknown,
                    };

                    ContainerSummary {
                        id: c.id.unwrap_or_default(),
                        name: c
                            .names
                            .and_then(|n| n.first().cloned())
                            .map(|n| n.trim_start_matches('/').to_string())
                            .unwrap_or_default(),
                        image: c.image.unwrap_or_default(),
                        state,
                        health: HealthStatus::None, // Would need separate inspect for health
                        created_at: c.created.map(|ts| {
                            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(ts as u64)
                        }),
                        status_message: c.status.unwrap_or_default(),
                    }
                })
                .collect();

            Ok(summaries)
        })
    }

    fn stats<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ResourceStats>> + Send + 'a>> {
        Box::pin(async move {
            let stats_options = StatsOptions {
                stream: false,
                one_shot: true,
            };

            let mut stream = self.client.stats(id.as_str(), Some(stats_options));

            if let Some(result) = stream.next().await {
                let stats = result.map_err(|e| ContainerError::Internal(e.to_string()))?;

                // Calculate CPU percentage
                let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
                    - stats.precpu_stats.cpu_usage.total_usage as f64;
                let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
                    - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
                let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
                let cpu_percent = if system_delta > 0.0 {
                    (cpu_delta / system_delta) * num_cpus * 100.0
                } else {
                    0.0
                };

                // Memory stats
                let memory_bytes = stats.memory_stats.usage.unwrap_or(0);
                let memory_limit = stats.memory_stats.limit.unwrap_or(0);

                // Network stats
                let (rx_bytes, tx_bytes) = stats
                    .networks
                    .map(|nets| {
                        nets.values()
                            .fold((0u64, 0u64), |(rx, tx), n| (rx + n.rx_bytes, tx + n.tx_bytes))
                    })
                    .unwrap_or((0, 0));

                // Block I/O stats
                let (read_bytes, write_bytes) = stats
                    .blkio_stats
                    .io_service_bytes_recursive
                    .map(|entries| {
                        entries.iter().fold((0u64, 0u64), |(r, w), e| {
                            match e.op.as_str() {
                                "read" | "Read" => (r + e.value, w),
                                "write" | "Write" => (r, w + e.value),
                                _ => (r, w),
                            }
                        })
                    })
                    .unwrap_or((0, 0));

                Ok(ResourceStats {
                    cpu_percent,
                    memory_bytes,
                    memory_limit,
                    network_rx_bytes: rx_bytes,
                    network_tx_bytes: tx_bytes,
                    block_read_bytes: read_bytes,
                    block_write_bytes: write_bytes,
                    pids: stats.pids_stats.current.unwrap_or(0),
                    gpu_memory: HashMap::new(), // GPU stats require nvidia-smi
                    gpu_utilization: HashMap::new(),
                    timestamp: Some(SystemTime::now()),
                })
            } else {
                Err(ContainerError::NotFound { id: id.to_string() })
            }
        })
    }

    fn logs<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a LogsOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            let log_options = BollardLogsOptions {
                stdout: options.stdout,
                stderr: options.stderr,
                timestamps: options.timestamps,
                tail: options
                    .tail
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "all".to_string()),
                follow: false,
                ..Default::default()
            };

            let mut stream = self.client.logs(id.as_str(), Some(log_options));
            let mut output = Vec::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        output.extend_from_slice(&chunk.into_bytes());
                    }
                    Err(e) => {
                        warn!(id = %id, error = %e, "error reading logs");
                        break;
                    }
                }
            }

            Ok(output)
        })
    }

    fn exec<'a>(
        &'a self,
        id: &'a ContainerId,
        options: &'a ExecOptions,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<ExecResult>> + Send + 'a>> {
        Box::pin(async move {
            let exec_options = CreateExecOptions {
                cmd: Some(options.cmd.clone()),
                env: Some(options.env.clone()),
                working_dir: options.working_dir.clone(),
                user: options.user.clone(),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                tty: Some(options.tty),
                privileged: Some(options.privileged),
                ..Default::default()
            };

            let exec = self
                .client
                .create_exec(id.as_str(), exec_options)
                .await
                .map_err(|e| ContainerError::Internal(format!("failed to create exec: {e}")))?;

            let mut stdout = Vec::new();
            let stderr = Vec::new();

            let output = self
                .client
                .start_exec(&exec.id, None)
                .await
                .map_err(|e| ContainerError::Internal(format!("failed to start exec: {e}")))?;

            if let StartExecResults::Attached { mut output, .. } = output {
                while let Some(result) = output.next().await {
                    match result {
                        Ok(chunk) => {
                            let bytes = chunk.into_bytes();
                            // Note: In non-TTY mode, Docker prefixes output with stream type
                            // For simplicity, we put everything in stdout
                            stdout.extend_from_slice(&bytes);
                        }
                        Err(e) => {
                            warn!(error = %e, "error reading exec output");
                            break;
                        }
                    }
                }
            }

            // Get exit code
            let inspect = self
                .client
                .inspect_exec(&exec.id)
                .await
                .map_err(|e| ContainerError::Internal(format!("failed to inspect exec: {e}")))?;

            let exit_code = inspect.exit_code.unwrap_or(-1);

            Ok(ExecResult {
                exit_code,
                stdout,
                stderr,
            })
        })
    }

    fn wait<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<i64>> + Send + 'a>> {
        Box::pin(async move {
            let wait_options = WaitContainerOptions {
                condition: "not-running",
            };

            let mut stream = self.client.wait_container(id.as_str(), Some(wait_options));

            if let Some(result) = stream.next().await {
                let response = result.map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::Internal(e.to_string()),
                })?;

                Ok(response.status_code)
            } else {
                Err(ContainerError::Internal(
                    "wait stream ended unexpectedly".to_string(),
                ))
            }
        })
    }

    fn pause<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            self.client
                .pause_container(id.as_str())
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::Internal(e.to_string()),
                })?;

            Ok(())
        })
    }

    fn unpause<'a>(
        &'a self,
        id: &'a ContainerId,
    ) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + 'a>> {
        Box::pin(async move {
            self.client
                .unpause_container(id.as_str())
                .await
                .map_err(|e| match e {
                    bollard::errors::Error::DockerResponseServerError {
                        status_code: 404, ..
                    } => ContainerError::NotFound { id: id.to_string() },
                    _ => ContainerError::Internal(e.to_string()),
                })?;

            Ok(())
        })
    }

    fn ping(&self) -> Pin<Box<dyn Future<Output = ContainerResult<()>> + Send + '_>> {
        Box::pin(async move {
            self.client
                .ping()
                .await
                .map_err(|e| ContainerError::ConnectionFailed(e.to_string()))?;

            Ok(())
        })
    }

    fn info(&self) -> RuntimeInfo {
        // This would ideally be async, but the trait requires sync
        // For now, return basic info
        RuntimeInfo {
            name: "docker".to_string(),
            version: String::new(), // Would need async call
            api_version: bollard::API_DEFAULT_VERSION.to_string(),
            gpu_available: self.gpu_runtime.is_some(),
            gpu_runtime: self.gpu_runtime.clone(),
            containers: 0,
            containers_running: 0,
            images: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::config::{CpuConfig, GpuDevice, GpuRequirements, MemoryConfig};

    // =========================================================================
    // Unit Tests (no Docker required)
    // =========================================================================

    fn make_runtime() -> Option<DockerRuntime> {
        DockerRuntime::connect().ok()
    }

    #[test]
    fn test_build_host_config_memory() {
        let Some(runtime) = make_runtime() else {
            return;
        };

        let config = ContainerConfig::new("test", "alpine")
            .with_memory(MemoryConfig::limit_gb(8).with_swappiness(50));

        let host_config = runtime.build_host_config(&config);

        assert_eq!(host_config.memory, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(host_config.memory_swappiness, Some(50));
    }

    #[test]
    fn test_build_host_config_cpu() {
        let Some(runtime) = make_runtime() else {
            return;
        };

        let config = ContainerConfig::new("test", "alpine")
            .with_cpu(CpuConfig::pinned("0-3").with_shares(2048));

        let host_config = runtime.build_host_config(&config);

        assert_eq!(host_config.cpuset_cpus, Some("0-3".to_string()));
        assert_eq!(host_config.cpu_shares, Some(2048));
    }

    #[test]
    fn test_build_host_config_gpu() {
        let Some(runtime) = make_runtime().map(|r| r.with_gpu_runtime("nvidia")) else {
            return;
        };

        let config = ContainerConfig::new("test", "nvidia/cuda:12.0-base")
            .with_gpu(GpuRequirements::count(2).with_cuda_version("12.0"));

        let host_config = runtime.build_host_config(&config);

        assert!(host_config.device_requests.is_some());
        let requests = host_config.device_requests.as_ref().expect("device requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].driver, Some("nvidia".to_string()));
    }

    #[test]
    fn test_build_host_config_volumes() {
        let Some(runtime) = make_runtime() else {
            return;
        };

        let config = ContainerConfig::new("test", "alpine")
            .with_volume(VolumeMount::bind("/host/data", "/container/data").read_only());

        let host_config = runtime.build_host_config(&config);

        let mounts = host_config.mounts.expect("mounts");
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, Some("/host/data".to_string()));
        assert_eq!(mounts[0].target, Some("/container/data".to_string()));
        assert_eq!(mounts[0].read_only, Some(true));
    }

    #[test]
    fn test_build_host_config_restart_policy() {
        let Some(runtime) = make_runtime() else {
            return;
        };

        let config =
            ContainerConfig::new("test", "alpine").with_restart_policy(RestartPolicy::on_failure(5));

        let host_config = runtime.build_host_config(&config);

        let policy = host_config.restart_policy.expect("restart policy");
        assert_eq!(policy.name, Some(RestartPolicyNameEnum::ON_FAILURE));
        assert_eq!(policy.maximum_retry_count, Some(5));
    }

    #[test]
    fn test_bollard_state_to_state() {
        let Some(runtime) = make_runtime() else {
            return;
        };

        let running_state = Some(BollardState {
            running: Some(true),
            ..Default::default()
        });
        assert_eq!(
            runtime.bollard_state_to_state(&running_state),
            ContainerState::Running
        );

        let paused_state = Some(BollardState {
            paused: Some(true),
            ..Default::default()
        });
        assert_eq!(
            runtime.bollard_state_to_state(&paused_state),
            ContainerState::Paused
        );

        let exited_state = Some(BollardState {
            status: Some(ContainerStateStatusEnum::EXITED),
            ..Default::default()
        });
        assert_eq!(
            runtime.bollard_state_to_state(&exited_state),
            ContainerState::Exited
        );

        assert_eq!(
            runtime.bollard_state_to_state(&None),
            ContainerState::Unknown
        );
    }

    #[test]
    fn test_runtime_info() {
        let Some(runtime) = make_runtime().map(|r| r.with_gpu_runtime("nvidia")) else {
            return;
        };

        let info = runtime.info();
        assert_eq!(info.name, "docker");
        assert!(info.gpu_available);
        assert_eq!(info.gpu_runtime, Some("nvidia".to_string()));
    }

    #[test]
    fn test_docker_runtime_with_gpu_runtime() {
        let Some(runtime) = make_runtime().map(|r| r.with_gpu_runtime("nvidia")) else {
            return;
        };

        assert_eq!(runtime.gpu_runtime, Some("nvidia".to_string()));
    }

    // =========================================================================
    // Integration Tests (require Docker)
    // =========================================================================

    #[tokio::test]
    #[ignore = "requires Docker daemon"]
    async fn test_docker_ping() {
        let runtime = DockerRuntime::connect().expect("connect");
        runtime.ping().await.expect("ping should succeed");
    }

    #[tokio::test]
    #[ignore = "requires Docker daemon"]
    async fn test_docker_list_containers() {
        let runtime = DockerRuntime::connect().expect("connect");
        let containers = runtime.list(&ListOptions::all()).await.expect("list");
        // Just verify we can list (may be empty)
        assert!(containers.len() >= 0);
    }
}
