//! Container configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::{ContainerError, ContainerResult};

/// GPU device configuration for container passthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDevice {
    /// Device index (e.g., 0, 1, 2 for multi-GPU).
    pub index: u32,

    /// Device UUID (e.g., `GPU-12345678-abcd-...`).
    pub uuid: Option<String>,

    /// Memory limit in bytes (0 = no limit).
    pub memory_limit: u64,

    /// Compute capability requirements (e.g., "7.5", "8.0").
    pub compute_capability: Option<String>,

    /// Whether to enable MIG (Multi-Instance GPU) mode.
    pub mig_enabled: bool,

    /// MIG device ID if MIG is enabled.
    pub mig_device_id: Option<String>,
}

impl GpuDevice {
    /// Create a new GPU device configuration by index.
    #[must_use]
    pub fn by_index(index: u32) -> Self {
        Self {
            index,
            uuid: None,
            memory_limit: 0,
            compute_capability: None,
            mig_enabled: false,
            mig_device_id: None,
        }
    }

    /// Create a new GPU device configuration by UUID.
    #[must_use]
    pub fn by_uuid(uuid: impl Into<String>) -> Self {
        Self {
            index: 0,
            uuid: Some(uuid.into()),
            memory_limit: 0,
            compute_capability: None,
            mig_enabled: false,
            mig_device_id: None,
        }
    }

    /// Set memory limit in bytes.
    #[must_use]
    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.memory_limit = limit;
        self
    }

    /// Set memory limit in gigabytes.
    #[must_use]
    pub fn with_memory_limit_gb(mut self, gb: u64) -> Self {
        self.memory_limit = gb * 1024 * 1024 * 1024;
        self
    }

    /// Set compute capability requirement.
    #[must_use]
    pub fn with_compute_capability(mut self, capability: impl Into<String>) -> Self {
        self.compute_capability = Some(capability.into());
        self
    }

    /// Enable MIG mode with device ID.
    #[must_use]
    pub fn with_mig(mut self, device_id: impl Into<String>) -> Self {
        self.mig_enabled = true;
        self.mig_device_id = Some(device_id.into());
        self
    }

    /// Get the device specifier for nvidia-container-runtime.
    #[must_use]
    pub fn device_specifier(&self) -> String {
        if let Some(ref uuid) = self.uuid {
            uuid.clone()
        } else if self.mig_enabled {
            if let Some(ref mig_id) = self.mig_device_id {
                format!("MIG-{mig_id}")
            } else {
                self.index.to_string()
            }
        } else {
            self.index.to_string()
        }
    }
}

impl Default for GpuDevice {
    fn default() -> Self {
        Self::by_index(0)
    }
}

/// GPU requirements for a container.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuRequirements {
    /// List of GPU devices to attach.
    pub devices: Vec<GpuDevice>,

    /// NVIDIA driver capabilities (compute, utility, graphics, etc.).
    pub capabilities: Vec<String>,

    /// Required driver version (minimum).
    pub driver_version: Option<String>,

    /// Required CUDA version.
    pub cuda_version: Option<String>,
}

impl GpuRequirements {
    /// Create empty GPU requirements (no GPUs).
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Request all available GPUs.
    #[must_use]
    pub fn all() -> Self {
        Self {
            devices: vec![], // Empty means "all" when capabilities are set
            capabilities: vec!["compute".to_string(), "utility".to_string()],
            driver_version: None,
            cuda_version: None,
        }
    }

    /// Request a specific number of GPUs.
    #[must_use]
    pub fn count(n: u32) -> Self {
        Self {
            devices: (0..n).map(GpuDevice::by_index).collect(),
            capabilities: vec!["compute".to_string(), "utility".to_string()],
            driver_version: None,
            cuda_version: None,
        }
    }

    /// Request specific GPU devices.
    #[must_use]
    pub fn devices(devices: Vec<GpuDevice>) -> Self {
        Self {
            devices,
            capabilities: vec!["compute".to_string(), "utility".to_string()],
            driver_version: None,
            cuda_version: None,
        }
    }

    /// Set required capabilities.
    #[must_use]
    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Set minimum driver version.
    #[must_use]
    pub fn with_driver_version(mut self, version: impl Into<String>) -> Self {
        self.driver_version = Some(version.into());
        self
    }

    /// Set required CUDA version.
    #[must_use]
    pub fn with_cuda_version(mut self, version: impl Into<String>) -> Self {
        self.cuda_version = Some(version.into());
        self
    }

    /// Check if GPU support is requested.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !self.capabilities.is_empty() || !self.devices.is_empty()
    }
}

/// Memory resource configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Memory limit in bytes.
    pub limit: u64,

    /// Memory reservation (soft limit) in bytes.
    pub reservation: u64,

    /// Swap limit in bytes (0 = same as memory limit, -1 = unlimited).
    pub swap: i64,

    /// Memory swappiness (0-100).
    pub swappiness: Option<u8>,

    /// Whether to kill container when OOM.
    pub oom_kill_disable: bool,
}

impl MemoryConfig {
    /// Create memory config with limit in bytes.
    #[must_use]
    pub fn limit_bytes(bytes: u64) -> Self {
        Self {
            limit: bytes,
            reservation: 0,
            swap: 0,
            swappiness: None,
            oom_kill_disable: false,
        }
    }

    /// Create memory config with limit in megabytes.
    #[must_use]
    pub fn limit_mb(mb: u64) -> Self {
        Self::limit_bytes(mb * 1024 * 1024)
    }

    /// Create memory config with limit in gigabytes.
    #[must_use]
    pub fn limit_gb(gb: u64) -> Self {
        Self::limit_bytes(gb * 1024 * 1024 * 1024)
    }

    /// Set memory reservation (soft limit).
    #[must_use]
    pub fn with_reservation_bytes(mut self, bytes: u64) -> Self {
        self.reservation = bytes;
        self
    }

    /// Set swap limit.
    #[must_use]
    pub fn with_swap_bytes(mut self, bytes: i64) -> Self {
        self.swap = bytes;
        self
    }

    /// Disable swap completely.
    #[must_use]
    pub fn with_no_swap(mut self) -> Self {
        self.swap = -1;
        self
    }

    /// Set swappiness value.
    #[must_use]
    pub fn with_swappiness(mut self, value: u8) -> Self {
        self.swappiness = Some(value.min(100));
        self
    }

    /// Disable OOM killer (container won't be killed on OOM).
    #[must_use]
    pub fn with_oom_kill_disabled(mut self) -> Self {
        self.oom_kill_disable = true;
        self
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            limit: 0, // No limit
            reservation: 0,
            swap: 0,
            swappiness: None,
            oom_kill_disable: false,
        }
    }
}

/// CPU resource configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuConfig {
    /// CPU shares (relative weight, default 1024).
    pub shares: u64,

    /// CPU quota in microseconds (0 = no limit).
    pub quota: u64,

    /// CPU period in microseconds (default 100000).
    pub period: u64,

    /// CPUs to use (e.g., "0-3" or "0,2,4").
    pub cpuset_cpus: Option<String>,

    /// Memory nodes for NUMA (e.g., "0-1").
    pub cpuset_mems: Option<String>,

    /// Number of CPUs (fractional, e.g., 1.5).
    pub nano_cpus: u64,
}

impl CpuConfig {
    /// Create default CPU config (no limits).
    #[must_use]
    pub fn unlimited() -> Self {
        Self::default()
    }

    /// Limit to a number of CPUs (can be fractional).
    #[must_use]
    pub fn cpus(count: f64) -> Self {
        Self {
            shares: 1024,
            quota: 0,
            period: 100_000,
            cpuset_cpus: None,
            cpuset_mems: None,
            nano_cpus: (count * 1_000_000_000.0) as u64,
        }
    }

    /// Pin to specific CPUs by core list.
    #[must_use]
    pub fn pinned(cpuset: impl Into<String>) -> Self {
        Self {
            shares: 1024,
            quota: 0,
            period: 100_000,
            cpuset_cpus: Some(cpuset.into()),
            cpuset_mems: None,
            nano_cpus: 0,
        }
    }

    /// Set CPU shares (relative weight).
    #[must_use]
    pub fn with_shares(mut self, shares: u64) -> Self {
        self.shares = shares;
        self
    }

    /// Set CPU quota and period for hard limits.
    #[must_use]
    pub fn with_quota(mut self, quota_us: u64, period_us: u64) -> Self {
        self.quota = quota_us;
        self.period = period_us;
        self
    }

    /// Set NUMA memory node affinity.
    #[must_use]
    pub fn with_numa_nodes(mut self, nodes: impl Into<String>) -> Self {
        self.cpuset_mems = Some(nodes.into());
        self
    }
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            shares: 1024,
            quota: 0,
            period: 100_000,
            cpuset_cpus: None,
            cpuset_mems: None,
            nano_cpus: 0,
        }
    }
}

/// Network mode for container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetworkMode {
    /// Bridge network (default Docker behavior).
    #[default]
    Bridge,

    /// Host network namespace.
    Host,

    /// No networking.
    None,

    /// Container network namespace (share with another container).
    Container(String),

    /// Custom network name.
    Custom(String),
}

impl NetworkMode {
    /// Get the Docker network mode string.
    #[must_use]
    pub fn as_docker_mode(&self) -> String {
        match self {
            Self::Bridge => "bridge".to_string(),
            Self::Host => "host".to_string(),
            Self::None => "none".to_string(),
            Self::Container(id) => format!("container:{id}"),
            Self::Custom(name) => name.clone(),
        }
    }
}

/// Volume mount configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Source path on host or volume name.
    pub source: String,

    /// Target path in container.
    pub target: String,

    /// Whether mount is read-only.
    pub read_only: bool,

    /// Mount type (bind, volume, tmpfs).
    pub mount_type: MountType,
}

/// Mount type for volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MountType {
    /// Bind mount from host path.
    #[default]
    Bind,

    /// Docker volume.
    Volume,

    /// Temporary filesystem.
    Tmpfs,
}

impl VolumeMount {
    /// Create a bind mount.
    #[must_use]
    pub fn bind(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            read_only: false,
            mount_type: MountType::Bind,
        }
    }

    /// Create a volume mount.
    #[must_use]
    pub fn volume(name: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: name.into(),
            target: target.into(),
            read_only: false,
            mount_type: MountType::Volume,
        }
    }

    /// Create a tmpfs mount.
    #[must_use]
    pub fn tmpfs(target: impl Into<String>) -> Self {
        Self {
            source: String::new(),
            target: target.into(),
            read_only: false,
            mount_type: MountType::Tmpfs,
        }
    }

    /// Make mount read-only.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}

/// Restart policy for container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RestartPolicy {
    /// Never restart.
    #[default]
    No,

    /// Always restart.
    Always,

    /// Restart on failure with max retry count.
    OnFailure {
        /// Maximum retry count (0 = unlimited).
        max_retries: u32,
    },

    /// Restart unless manually stopped.
    UnlessStopped,
}

impl RestartPolicy {
    /// Create on-failure policy with retry limit.
    #[must_use]
    pub fn on_failure(max_retries: u32) -> Self {
        Self::OnFailure { max_retries }
    }
}

/// Full container configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    /// Container name (must be unique).
    pub name: String,

    /// Image to run (e.g., "nvidia/cuda:12.0-base").
    pub image: String,

    /// Command to run (overrides image CMD).
    pub command: Option<Vec<String>>,

    /// Entrypoint (overrides image ENTRYPOINT).
    pub entrypoint: Option<Vec<String>>,

    /// Environment variables.
    pub env: HashMap<String, String>,

    /// Working directory in container.
    pub working_dir: Option<String>,

    /// User to run as (e.g., "1000:1000").
    pub user: Option<String>,

    /// Memory configuration.
    pub memory: MemoryConfig,

    /// CPU configuration.
    pub cpu: CpuConfig,

    /// GPU requirements.
    pub gpu: GpuRequirements,

    /// Network mode.
    pub network_mode: NetworkMode,

    /// Volume mounts.
    pub volumes: Vec<VolumeMount>,

    /// Port mappings (container_port -> host_port).
    pub ports: HashMap<u16, u16>,

    /// Labels for container metadata.
    pub labels: HashMap<String, String>,

    /// Restart policy.
    pub restart_policy: RestartPolicy,

    /// Whether to run in privileged mode.
    pub privileged: bool,

    /// Whether to auto-remove container on exit.
    pub auto_remove: bool,

    /// Hostname for the container.
    pub hostname: Option<String>,

    /// Stop timeout in seconds.
    pub stop_timeout: Option<u32>,

    /// Health check command.
    pub healthcheck: Option<HealthCheck>,
}

impl ContainerConfig {
    /// Create a new container config with minimal settings.
    #[must_use]
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            command: None,
            entrypoint: None,
            env: HashMap::new(),
            working_dir: None,
            user: None,
            memory: MemoryConfig::default(),
            cpu: CpuConfig::default(),
            gpu: GpuRequirements::none(),
            network_mode: NetworkMode::default(),
            volumes: Vec::new(),
            ports: HashMap::new(),
            labels: HashMap::new(),
            restart_policy: RestartPolicy::default(),
            privileged: false,
            auto_remove: false,
            hostname: None,
            stop_timeout: None,
            healthcheck: None,
        }
    }

    /// Set command to run.
    #[must_use]
    pub fn with_command(mut self, cmd: Vec<String>) -> Self {
        self.command = Some(cmd);
        self
    }

    /// Set entrypoint.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: Vec<String>) -> Self {
        self.entrypoint = Some(entrypoint);
        self
    }

    /// Add environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables.
    #[must_use]
    pub fn with_envs(mut self, envs: HashMap<String, String>) -> Self {
        self.env.extend(envs);
        self
    }

    /// Set working directory.
    #[must_use]
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Set user.
    #[must_use]
    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Set memory configuration.
    #[must_use]
    pub fn with_memory(mut self, memory: MemoryConfig) -> Self {
        self.memory = memory;
        self
    }

    /// Set CPU configuration.
    #[must_use]
    pub fn with_cpu(mut self, cpu: CpuConfig) -> Self {
        self.cpu = cpu;
        self
    }

    /// Set GPU requirements.
    #[must_use]
    pub fn with_gpu(mut self, gpu: GpuRequirements) -> Self {
        self.gpu = gpu;
        self
    }

    /// Set network mode.
    #[must_use]
    pub fn with_network(mut self, mode: NetworkMode) -> Self {
        self.network_mode = mode;
        self
    }

    /// Add volume mount.
    #[must_use]
    pub fn with_volume(mut self, volume: VolumeMount) -> Self {
        self.volumes.push(volume);
        self
    }

    /// Add port mapping.
    #[must_use]
    pub fn with_port(mut self, container_port: u16, host_port: u16) -> Self {
        self.ports.insert(container_port, host_port);
        self
    }

    /// Add label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Set restart policy.
    #[must_use]
    pub fn with_restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    /// Enable privileged mode.
    #[must_use]
    pub fn privileged(mut self) -> Self {
        self.privileged = true;
        self
    }

    /// Enable auto-remove on exit.
    #[must_use]
    pub fn auto_remove(mut self) -> Self {
        self.auto_remove = true;
        self
    }

    /// Set hostname.
    #[must_use]
    pub fn with_hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = Some(hostname.into());
        self
    }

    /// Set stop timeout.
    #[must_use]
    pub fn with_stop_timeout(mut self, seconds: u32) -> Self {
        self.stop_timeout = Some(seconds);
        self
    }

    /// Set health check.
    #[must_use]
    pub fn with_healthcheck(mut self, healthcheck: HealthCheck) -> Self {
        self.healthcheck = Some(healthcheck);
        self
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn validate(&self) -> ContainerResult<()> {
        if self.name.is_empty() {
            return Err(ContainerError::InvalidConfig(
                "container name cannot be empty".to_string(),
            ));
        }

        if self.image.is_empty() {
            return Err(ContainerError::InvalidConfig(
                "image cannot be empty".to_string(),
            ));
        }

        // Validate name format (Docker naming rules)
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(ContainerError::InvalidConfig(format!(
                "invalid container name: {}",
                self.name
            )));
        }

        // Validate memory config
        if self.memory.reservation > self.memory.limit && self.memory.limit > 0 {
            return Err(ContainerError::InvalidConfig(
                "memory reservation cannot exceed limit".to_string(),
            ));
        }

        Ok(())
    }
}

/// Health check configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Command to run for health check.
    pub test: Vec<String>,

    /// Interval between checks in seconds.
    pub interval_secs: u32,

    /// Timeout for health check in seconds.
    pub timeout_secs: u32,

    /// Number of retries before marking unhealthy.
    pub retries: u32,

    /// Start period in seconds (grace period for startup).
    pub start_period_secs: u32,
}

impl HealthCheck {
    /// Create a health check with a command.
    #[must_use]
    pub fn cmd(cmd: Vec<String>) -> Self {
        Self {
            test: cmd,
            interval_secs: 30,
            timeout_secs: 30,
            retries: 3,
            start_period_secs: 0,
        }
    }

    /// Set check interval.
    #[must_use]
    pub fn with_interval(mut self, secs: u32) -> Self {
        self.interval_secs = secs;
        self
    }

    /// Set check timeout.
    #[must_use]
    pub fn with_timeout(mut self, secs: u32) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set retry count.
    #[must_use]
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Set start period.
    #[must_use]
    pub fn with_start_period(mut self, secs: u32) -> Self {
        self.start_period_secs = secs;
        self
    }
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            test: vec!["CMD".to_string(), "true".to_string()],
            interval_secs: 30,
            timeout_secs: 30,
            retries: 3,
            start_period_secs: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GpuDevice Tests
    // =========================================================================

    #[test]
    fn test_gpu_device_by_index() {
        let device = GpuDevice::by_index(2);
        assert_eq!(device.index, 2);
        assert!(device.uuid.is_none());
        assert_eq!(device.device_specifier(), "2");
    }

    #[test]
    fn test_gpu_device_by_uuid() {
        let device = GpuDevice::by_uuid("GPU-12345678-abcd-efgh-ijkl");
        assert!(device.uuid.is_some());
        assert_eq!(
            device.device_specifier(),
            "GPU-12345678-abcd-efgh-ijkl"
        );
    }

    #[test]
    fn test_gpu_device_with_memory_limit() {
        let device = GpuDevice::by_index(0).with_memory_limit_gb(8);
        assert_eq!(device.memory_limit, 8 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_gpu_device_with_mig() {
        let device = GpuDevice::by_index(0).with_mig("1g.5gb");
        assert!(device.mig_enabled);
        assert_eq!(device.device_specifier(), "MIG-1g.5gb");
    }

    #[test]
    fn test_gpu_device_default() {
        let device = GpuDevice::default();
        assert_eq!(device.index, 0);
        assert!(!device.mig_enabled);
    }

    // =========================================================================
    // GpuRequirements Tests
    // =========================================================================

    #[test]
    fn test_gpu_requirements_none() {
        let req = GpuRequirements::none();
        assert!(!req.is_enabled());
    }

    #[test]
    fn test_gpu_requirements_all() {
        let req = GpuRequirements::all();
        assert!(req.is_enabled());
        assert!(req.capabilities.contains(&"compute".to_string()));
    }

    #[test]
    fn test_gpu_requirements_count() {
        let req = GpuRequirements::count(4);
        assert_eq!(req.devices.len(), 4);
        assert!(req.is_enabled());
    }

    #[test]
    fn test_gpu_requirements_with_driver() {
        let req = GpuRequirements::all()
            .with_driver_version("535.86")
            .with_cuda_version("12.2");
        assert_eq!(req.driver_version, Some("535.86".to_string()));
        assert_eq!(req.cuda_version, Some("12.2".to_string()));
    }

    // =========================================================================
    // MemoryConfig Tests
    // =========================================================================

    #[test]
    fn test_memory_config_limit_gb() {
        let mem = MemoryConfig::limit_gb(16);
        assert_eq!(mem.limit, 16 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_memory_config_no_swap() {
        let mem = MemoryConfig::limit_gb(8).with_no_swap();
        assert_eq!(mem.swap, -1);
    }

    #[test]
    fn test_memory_config_swappiness() {
        let mem = MemoryConfig::limit_gb(8).with_swappiness(150);
        assert_eq!(mem.swappiness, Some(100)); // Capped at 100
    }

    #[test]
    fn test_memory_config_oom_disabled() {
        let mem = MemoryConfig::limit_gb(8).with_oom_kill_disabled();
        assert!(mem.oom_kill_disable);
    }

    // =========================================================================
    // CpuConfig Tests
    // =========================================================================

    #[test]
    fn test_cpu_config_cpus() {
        let cpu = CpuConfig::cpus(2.5);
        assert_eq!(cpu.nano_cpus, 2_500_000_000);
    }

    #[test]
    fn test_cpu_config_pinned() {
        let cpu = CpuConfig::pinned("0-3");
        assert_eq!(cpu.cpuset_cpus, Some("0-3".to_string()));
    }

    #[test]
    fn test_cpu_config_with_shares() {
        let cpu = CpuConfig::unlimited().with_shares(2048);
        assert_eq!(cpu.shares, 2048);
    }

    #[test]
    fn test_cpu_config_with_quota() {
        let cpu = CpuConfig::unlimited().with_quota(50_000, 100_000);
        assert_eq!(cpu.quota, 50_000);
        assert_eq!(cpu.period, 100_000);
    }

    // =========================================================================
    // NetworkMode Tests
    // =========================================================================

    #[test]
    fn test_network_mode_bridge() {
        assert_eq!(NetworkMode::Bridge.as_docker_mode(), "bridge");
    }

    #[test]
    fn test_network_mode_host() {
        assert_eq!(NetworkMode::Host.as_docker_mode(), "host");
    }

    #[test]
    fn test_network_mode_container() {
        let mode = NetworkMode::Container("abc123".to_string());
        assert_eq!(mode.as_docker_mode(), "container:abc123");
    }

    #[test]
    fn test_network_mode_custom() {
        let mode = NetworkMode::Custom("my-network".to_string());
        assert_eq!(mode.as_docker_mode(), "my-network");
    }

    // =========================================================================
    // VolumeMount Tests
    // =========================================================================

    #[test]
    fn test_volume_mount_bind() {
        let vol = VolumeMount::bind("/host/path", "/container/path");
        assert_eq!(vol.mount_type, MountType::Bind);
        assert!(!vol.read_only);
    }

    #[test]
    fn test_volume_mount_readonly() {
        let vol = VolumeMount::bind("/host/path", "/container/path").read_only();
        assert!(vol.read_only);
    }

    #[test]
    fn test_volume_mount_tmpfs() {
        let vol = VolumeMount::tmpfs("/tmp");
        assert_eq!(vol.mount_type, MountType::Tmpfs);
        assert!(vol.source.is_empty());
    }

    // =========================================================================
    // RestartPolicy Tests
    // =========================================================================

    #[test]
    fn test_restart_policy_default() {
        assert_eq!(RestartPolicy::default(), RestartPolicy::No);
    }

    #[test]
    fn test_restart_policy_on_failure() {
        let policy = RestartPolicy::on_failure(5);
        assert_eq!(policy, RestartPolicy::OnFailure { max_retries: 5 });
    }

    // =========================================================================
    // ContainerConfig Tests
    // =========================================================================

    #[test]
    fn test_container_config_new() {
        let config = ContainerConfig::new("my-container", "nginx:latest");
        assert_eq!(config.name, "my-container");
        assert_eq!(config.image, "nginx:latest");
    }

    #[test]
    fn test_container_config_builder() {
        let config = ContainerConfig::new("gpu-workload", "nvidia/cuda:12.0-base")
            .with_command(vec!["nvidia-smi".to_string()])
            .with_env("CUDA_VISIBLE_DEVICES", "0,1")
            .with_memory(MemoryConfig::limit_gb(32))
            .with_cpu(CpuConfig::cpus(8.0))
            .with_gpu(GpuRequirements::count(2))
            .with_port(8080, 80)
            .with_label("app", "ml-training")
            .auto_remove();

        assert_eq!(config.command, Some(vec!["nvidia-smi".to_string()]));
        assert_eq!(
            config.env.get("CUDA_VISIBLE_DEVICES"),
            Some(&"0,1".to_string())
        );
        assert!(config.auto_remove);
        assert_eq!(config.gpu.devices.len(), 2);
    }

    #[test]
    fn test_container_config_validate_valid() {
        let config = ContainerConfig::new("valid-name", "image:tag");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_container_config_validate_empty_name() {
        let config = ContainerConfig::new("", "image:tag");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_container_config_validate_empty_image() {
        let config = ContainerConfig::new("name", "");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_container_config_validate_invalid_name() {
        let config = ContainerConfig::new("invalid name!", "image:tag");
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_container_config_validate_memory() {
        let config = ContainerConfig::new("name", "image")
            .with_memory(MemoryConfig {
                limit: 100,
                reservation: 200, // Invalid: reservation > limit
                swap: 0,
                swappiness: None,
                oom_kill_disable: false,
            });
        assert!(config.validate().is_err());
    }

    // =========================================================================
    // HealthCheck Tests
    // =========================================================================

    #[test]
    fn test_healthcheck_cmd() {
        let hc = HealthCheck::cmd(vec!["curl".to_string(), "-f".to_string(), "http://localhost/".to_string()]);
        assert_eq!(hc.test.len(), 3);
        assert_eq!(hc.interval_secs, 30);
    }

    #[test]
    fn test_healthcheck_builder() {
        let hc = HealthCheck::cmd(vec!["test".to_string()])
            .with_interval(10)
            .with_timeout(5)
            .with_retries(5)
            .with_start_period(60);

        assert_eq!(hc.interval_secs, 10);
        assert_eq!(hc.timeout_secs, 5);
        assert_eq!(hc.retries, 5);
        assert_eq!(hc.start_period_secs, 60);
    }
}
