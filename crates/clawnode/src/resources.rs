//! Resource limits and capacity management.
//!
//! Provides comprehensive resource limiting to prevent exhaustion attacks:
//! - Memory limits (RAM and GPU VRAM)
//! - CPU limits
//! - Disk usage limits
//! - Network bandwidth limits
//! - Concurrent workload limits
//!
//! All limits are validated against node capacity before workload admission.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::NodeError;

/// Default maximum memory per workload (8 GiB).
pub const DEFAULT_MAX_MEMORY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Default maximum CPU cores per workload.
pub const DEFAULT_MAX_CPU_CORES: f32 = 8.0;

/// Default maximum disk usage per workload (50 GiB).
pub const DEFAULT_MAX_DISK_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Default maximum GPU memory per GPU (entire GPU memory).
pub const DEFAULT_MAX_GPU_MEMORY_MIB: u64 = u64::MAX;

/// Default maximum network bandwidth per workload (1 Gbps).
pub const DEFAULT_MAX_NETWORK_BANDWIDTH_MBPS: u32 = 1000;

/// Default maximum concurrent workloads per node.
pub const DEFAULT_MAX_CONCURRENT_WORKLOADS: u32 = 64;

/// Percentage of resources reserved for system operations.
pub const DEFAULT_SYSTEM_RESERVED_PERCENT: u8 = 10;

/// Resource limits for a single workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
    /// Maximum memory in bytes. None means use node default.
    pub memory_bytes: Option<u64>,

    /// Maximum CPU cores (fractional). None means use node default.
    pub cpu_cores: Option<f32>,

    /// Maximum disk usage in bytes. None means use node default.
    pub disk_bytes: Option<u64>,

    /// Maximum GPU memory in MiB per allocated GPU. None means entire GPU.
    pub gpu_memory_mib: Option<u64>,

    /// Maximum network bandwidth in Mbps. None means use node default.
    pub network_bandwidth_mbps: Option<u32>,

    /// Maximum execution time. None means no limit.
    pub max_execution_time: Option<Duration>,

    /// OOM kill priority (0-1000). Higher = killed first under memory pressure.
    pub oom_score_adj: Option<i32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_bytes: None,
            cpu_cores: None,
            disk_bytes: None,
            gpu_memory_mib: None,
            network_bandwidth_mbps: None,
            max_execution_time: None,
            oom_score_adj: None,
        }
    }
}

impl ResourceLimits {
    /// Create new resource limits with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set memory limit in bytes.
    #[must_use]
    pub const fn with_memory_bytes(mut self, bytes: u64) -> Self {
        self.memory_bytes = Some(bytes);
        self
    }

    /// Set memory limit in MiB.
    #[must_use]
    pub const fn with_memory_mib(mut self, mib: u64) -> Self {
        self.memory_bytes = Some(mib * 1024 * 1024);
        self
    }

    /// Set CPU cores limit.
    #[must_use]
    pub const fn with_cpu_cores(mut self, cores: f32) -> Self {
        self.cpu_cores = Some(cores);
        self
    }

    /// Set disk usage limit in bytes.
    #[must_use]
    pub const fn with_disk_bytes(mut self, bytes: u64) -> Self {
        self.disk_bytes = Some(bytes);
        self
    }

    /// Set GPU memory limit in MiB per GPU.
    #[must_use]
    pub const fn with_gpu_memory_mib(mut self, mib: u64) -> Self {
        self.gpu_memory_mib = Some(mib);
        self
    }

    /// Set network bandwidth limit in Mbps.
    #[must_use]
    pub const fn with_network_bandwidth_mbps(mut self, mbps: u32) -> Self {
        self.network_bandwidth_mbps = Some(mbps);
        self
    }

    /// Set maximum execution time.
    #[must_use]
    pub const fn with_max_execution_time(mut self, duration: Duration) -> Self {
        self.max_execution_time = Some(duration);
        self
    }

    /// Set OOM score adjustment.
    #[must_use]
    pub const fn with_oom_score_adj(mut self, score: i32) -> Self {
        self.oom_score_adj = Some(score);
        self
    }

    /// Validate limits are within acceptable ranges.
    ///
    /// # Errors
    ///
    /// Returns an error if any limit is invalid.
    pub fn validate(&self) -> Result<(), NodeError> {
        if let Some(mem) = self.memory_bytes {
            if mem == 0 {
                return Err(NodeError::ResourceLimitInvalid(
                    "memory_bytes cannot be zero".to_string(),
                ));
            }
        }

        if let Some(cpu) = self.cpu_cores {
            if cpu <= 0.0 {
                return Err(NodeError::ResourceLimitInvalid(
                    "cpu_cores must be positive".to_string(),
                ));
            }
            if cpu > 1024.0 {
                return Err(NodeError::ResourceLimitInvalid(
                    "cpu_cores exceeds maximum (1024)".to_string(),
                ));
            }
        }

        if let Some(disk) = self.disk_bytes {
            if disk == 0 {
                return Err(NodeError::ResourceLimitInvalid(
                    "disk_bytes cannot be zero".to_string(),
                ));
            }
        }

        if let Some(gpu_mem) = self.gpu_memory_mib {
            if gpu_mem == 0 {
                return Err(NodeError::ResourceLimitInvalid(
                    "gpu_memory_mib cannot be zero".to_string(),
                ));
            }
        }

        if let Some(oom) = self.oom_score_adj {
            if !(-1000..=1000).contains(&oom) {
                return Err(NodeError::ResourceLimitInvalid(
                    "oom_score_adj must be between -1000 and 1000".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Node capacity configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCapacity {
    /// Total system memory in bytes.
    pub total_memory_bytes: u64,

    /// Total CPU cores available.
    pub total_cpu_cores: u32,

    /// Total disk space in bytes.
    pub total_disk_bytes: u64,

    /// Total GPU count.
    pub total_gpus: u32,

    /// GPU memory in MiB per GPU (indexed by GPU ID).
    pub gpu_memory_mib: Vec<u64>,

    /// Total network bandwidth in Mbps.
    pub total_network_bandwidth_mbps: u32,

    /// Percentage of resources reserved for system (0-50).
    pub system_reserved_percent: u8,

    /// Maximum concurrent workloads allowed.
    pub max_concurrent_workloads: u32,
}

impl Default for NodeCapacity {
    fn default() -> Self {
        Self {
            total_memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GiB
            total_cpu_cores: 8,
            total_disk_bytes: 500 * 1024 * 1024 * 1024, // 500 GiB
            total_gpus: 0,
            gpu_memory_mib: Vec::new(),
            total_network_bandwidth_mbps: 10_000, // 10 Gbps
            system_reserved_percent: DEFAULT_SYSTEM_RESERVED_PERCENT,
            max_concurrent_workloads: DEFAULT_MAX_CONCURRENT_WORKLOADS,
        }
    }
}

impl NodeCapacity {
    /// Create a new node capacity with detected resources.
    #[must_use]
    pub fn new(
        total_memory_bytes: u64,
        total_cpu_cores: u32,
        total_disk_bytes: u64,
    ) -> Self {
        Self {
            total_memory_bytes,
            total_cpu_cores,
            total_disk_bytes,
            ..Self::default()
        }
    }

    /// Set GPU capacity.
    #[must_use]
    pub fn with_gpus(mut self, gpu_memory_mib: Vec<u64>) -> Self {
        self.total_gpus = gpu_memory_mib.len() as u32;
        self.gpu_memory_mib = gpu_memory_mib;
        self
    }

    /// Set system reserved percentage.
    #[must_use]
    pub fn with_system_reserved_percent(mut self, percent: u8) -> Self {
        self.system_reserved_percent = percent.min(50);
        self
    }

    /// Set maximum concurrent workloads.
    #[must_use]
    pub const fn with_max_concurrent_workloads(mut self, max: u32) -> Self {
        self.max_concurrent_workloads = max;
        self
    }

    /// Get allocatable memory (after system reservation).
    #[must_use]
    pub fn allocatable_memory_bytes(&self) -> u64 {
        let reserved = self.total_memory_bytes * u64::from(self.system_reserved_percent) / 100;
        self.total_memory_bytes.saturating_sub(reserved)
    }

    /// Get allocatable CPU cores (after system reservation).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn allocatable_cpu_cores(&self) -> f32 {
        let total = self.total_cpu_cores as f32;
        let reserved = total * f32::from(self.system_reserved_percent) / 100.0;
        total - reserved
    }

    /// Get allocatable disk space (after system reservation).
    #[must_use]
    pub fn allocatable_disk_bytes(&self) -> u64 {
        let reserved = self.total_disk_bytes * u64::from(self.system_reserved_percent) / 100;
        self.total_disk_bytes.saturating_sub(reserved)
    }

    /// Validate the capacity configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), NodeError> {
        if self.total_memory_bytes == 0 {
            return Err(NodeError::ResourceLimitInvalid(
                "total_memory_bytes cannot be zero".to_string(),
            ));
        }

        if self.total_cpu_cores == 0 {
            return Err(NodeError::ResourceLimitInvalid(
                "total_cpu_cores cannot be zero".to_string(),
            ));
        }

        if self.system_reserved_percent > 50 {
            return Err(NodeError::ResourceLimitInvalid(
                "system_reserved_percent cannot exceed 50".to_string(),
            ));
        }

        if self.max_concurrent_workloads == 0 {
            return Err(NodeError::ResourceLimitInvalid(
                "max_concurrent_workloads cannot be zero".to_string(),
            ));
        }

        if self.total_gpus as usize != self.gpu_memory_mib.len() {
            return Err(NodeError::ResourceLimitInvalid(
                "gpu_memory_mib length must match total_gpus".to_string(),
            ));
        }

        Ok(())
    }
}

/// Effective resource limits after applying node defaults and constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveResourceLimits {
    /// Memory limit in bytes.
    pub memory_bytes: u64,
    /// CPU cores limit.
    pub cpu_cores: f32,
    /// Disk usage limit in bytes.
    pub disk_bytes: u64,
    /// GPU memory limit per GPU in MiB.
    pub gpu_memory_mib: u64,
    /// Network bandwidth limit in Mbps.
    pub network_bandwidth_mbps: u32,
    /// Maximum execution time (if any).
    pub max_execution_time: Option<Duration>,
    /// OOM score adjustment.
    pub oom_score_adj: i32,
}

/// Resolve workload limits against node capacity.
///
/// Applies defaults and ensures limits don't exceed node capacity.
///
/// # Errors
///
/// Returns an error if the requested limits exceed node capacity.
pub fn resolve_limits(
    requested: &ResourceLimits,
    capacity: &NodeCapacity,
    gpu_count: u32,
) -> Result<EffectiveResourceLimits, NodeError> {
    // Memory limit
    let memory_bytes = requested
        .memory_bytes
        .unwrap_or(DEFAULT_MAX_MEMORY_BYTES)
        .min(capacity.allocatable_memory_bytes());

    if let Some(req) = requested.memory_bytes {
        if req > capacity.allocatable_memory_bytes() {
            return Err(NodeError::ResourceExceedsCapacity {
                resource: "memory".to_string(),
                requested: req,
                available: capacity.allocatable_memory_bytes(),
            });
        }
    }

    // CPU limit
    let cpu_cores = requested
        .cpu_cores
        .unwrap_or(DEFAULT_MAX_CPU_CORES)
        .min(capacity.allocatable_cpu_cores());

    if let Some(req) = requested.cpu_cores {
        if req > capacity.allocatable_cpu_cores() {
            return Err(NodeError::ResourceExceedsCapacity {
                resource: "cpu_cores".to_string(),
                requested: req as u64,
                available: capacity.allocatable_cpu_cores() as u64,
            });
        }
    }

    // Disk limit
    let disk_bytes = requested
        .disk_bytes
        .unwrap_or(DEFAULT_MAX_DISK_BYTES)
        .min(capacity.allocatable_disk_bytes());

    if let Some(req) = requested.disk_bytes {
        if req > capacity.allocatable_disk_bytes() {
            return Err(NodeError::ResourceExceedsCapacity {
                resource: "disk".to_string(),
                requested: req,
                available: capacity.allocatable_disk_bytes(),
            });
        }
    }

    // GPU memory limit (per GPU)
    let gpu_memory_mib = if gpu_count > 0 {
        let max_gpu_mem = capacity
            .gpu_memory_mib
            .iter()
            .copied()
            .min()
            .unwrap_or(DEFAULT_MAX_GPU_MEMORY_MIB);

        requested
            .gpu_memory_mib
            .unwrap_or(max_gpu_mem)
            .min(max_gpu_mem)
    } else {
        0
    };

    // Network bandwidth
    let network_bandwidth_mbps = requested
        .network_bandwidth_mbps
        .unwrap_or(DEFAULT_MAX_NETWORK_BANDWIDTH_MBPS)
        .min(capacity.total_network_bandwidth_mbps);

    Ok(EffectiveResourceLimits {
        memory_bytes,
        cpu_cores,
        disk_bytes,
        gpu_memory_mib,
        network_bandwidth_mbps,
        max_execution_time: requested.max_execution_time,
        oom_score_adj: requested.oom_score_adj.unwrap_or(0),
    })
}

/// Current resource usage for a workload.
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Memory usage in bytes.
    pub memory_bytes: u64,
    /// CPU usage (0.0-1.0 per core, can exceed 1.0 for multi-core).
    pub cpu_usage: f32,
    /// Disk usage in bytes.
    pub disk_bytes: u64,
    /// GPU memory usage in MiB per GPU.
    pub gpu_memory_mib: HashMap<u32, u64>,
    /// Network bytes sent.
    pub network_tx_bytes: u64,
    /// Network bytes received.
    pub network_rx_bytes: u64,
    /// Last update timestamp.
    pub last_updated: Option<Instant>,
}

impl ResourceUsage {
    /// Create a new empty usage record.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if memory usage exceeds limit.
    #[must_use]
    pub fn exceeds_memory_limit(&self, limit: u64) -> bool {
        self.memory_bytes > limit
    }

    /// Check if CPU usage exceeds limit.
    #[must_use]
    pub fn exceeds_cpu_limit(&self, limit: f32) -> bool {
        self.cpu_usage > limit
    }

    /// Check if disk usage exceeds limit.
    #[must_use]
    pub fn exceeds_disk_limit(&self, limit: u64) -> bool {
        self.disk_bytes > limit
    }

    /// Get memory usage as a percentage of limit.
    #[must_use]
    pub fn memory_percent(&self, limit: u64) -> f64 {
        if limit == 0 {
            return 0.0;
        }
        (self.memory_bytes as f64 / limit as f64) * 100.0
    }
}

/// Resource usage tracker for the entire node.
#[derive(Debug)]
pub struct ResourceTracker {
    /// Node capacity.
    capacity: NodeCapacity,
    /// Currently allocated memory in bytes.
    allocated_memory_bytes: AtomicU64,
    /// Currently allocated CPU cores (stored as millicores).
    allocated_cpu_millicores: AtomicU64,
    /// Currently allocated disk in bytes.
    allocated_disk_bytes: AtomicU64,
    /// Current workload count.
    workload_count: AtomicU64,
    /// Per-workload usage tracking.
    workload_usage: std::sync::RwLock<HashMap<Uuid, ResourceUsage>>,
    /// Alert thresholds (percentage).
    alert_threshold_percent: u8,
}

impl ResourceTracker {
    /// Create a new resource tracker.
    #[must_use]
    pub fn new(capacity: NodeCapacity) -> Self {
        Self {
            capacity,
            allocated_memory_bytes: AtomicU64::new(0),
            allocated_cpu_millicores: AtomicU64::new(0),
            allocated_disk_bytes: AtomicU64::new(0),
            workload_count: AtomicU64::new(0),
            workload_usage: std::sync::RwLock::new(HashMap::new()),
            alert_threshold_percent: 80,
        }
    }

    /// Set alert threshold percentage.
    #[must_use]
    pub const fn with_alert_threshold(mut self, percent: u8) -> Self {
        self.alert_threshold_percent = percent;
        self
    }

    /// Check if the node can accept a new workload with the given limits.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are insufficient.
    pub fn can_accept_workload(&self, limits: &EffectiveResourceLimits) -> Result<(), NodeError> {
        // Check concurrent workload limit
        let current_count = self.workload_count.load(Ordering::SeqCst);
        if current_count >= u64::from(self.capacity.max_concurrent_workloads) {
            return Err(NodeError::MaxWorkloadsExceeded {
                current: current_count as u32,
                max: self.capacity.max_concurrent_workloads,
            });
        }

        // Check memory
        let current_memory = self.allocated_memory_bytes.load(Ordering::SeqCst);
        let new_total = current_memory.saturating_add(limits.memory_bytes);
        if new_total > self.capacity.allocatable_memory_bytes() {
            return Err(NodeError::InsufficientMemory {
                requested: limits.memory_bytes,
                available: self.capacity.allocatable_memory_bytes().saturating_sub(current_memory),
            });
        }

        // Check CPU
        let current_cpu = self.allocated_cpu_millicores.load(Ordering::SeqCst);
        let requested_millicores = (limits.cpu_cores * 1000.0) as u64;
        let allocatable_millicores = (self.capacity.allocatable_cpu_cores() * 1000.0) as u64;
        if current_cpu.saturating_add(requested_millicores) > allocatable_millicores {
            return Err(NodeError::InsufficientCpu {
                requested: limits.cpu_cores,
                available: (allocatable_millicores.saturating_sub(current_cpu)) as f32 / 1000.0,
            });
        }

        // Check disk
        let current_disk = self.allocated_disk_bytes.load(Ordering::SeqCst);
        if current_disk.saturating_add(limits.disk_bytes) > self.capacity.allocatable_disk_bytes() {
            return Err(NodeError::InsufficientDisk {
                requested: limits.disk_bytes,
                available: self.capacity.allocatable_disk_bytes().saturating_sub(current_disk),
            });
        }

        Ok(())
    }

    /// Reserve resources for a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if resources cannot be reserved.
    pub fn reserve(&self, workload_id: Uuid, limits: &EffectiveResourceLimits) -> Result<(), NodeError> {
        self.can_accept_workload(limits)?;

        self.allocated_memory_bytes.fetch_add(limits.memory_bytes, Ordering::SeqCst);
        let millicores = (limits.cpu_cores * 1000.0) as u64;
        self.allocated_cpu_millicores.fetch_add(millicores, Ordering::SeqCst);
        self.allocated_disk_bytes.fetch_add(limits.disk_bytes, Ordering::SeqCst);
        self.workload_count.fetch_add(1, Ordering::SeqCst);

        // Initialize usage tracking
        if let Ok(mut usage) = self.workload_usage.write() {
            usage.insert(workload_id, ResourceUsage::new());
        }

        Ok(())
    }

    /// Release resources for a workload.
    pub fn release(&self, workload_id: Uuid, limits: &EffectiveResourceLimits) {
        self.allocated_memory_bytes.fetch_sub(
            limits.memory_bytes.min(self.allocated_memory_bytes.load(Ordering::SeqCst)),
            Ordering::SeqCst,
        );

        let millicores = (limits.cpu_cores * 1000.0) as u64;
        self.allocated_cpu_millicores.fetch_sub(
            millicores.min(self.allocated_cpu_millicores.load(Ordering::SeqCst)),
            Ordering::SeqCst,
        );

        self.allocated_disk_bytes.fetch_sub(
            limits.disk_bytes.min(self.allocated_disk_bytes.load(Ordering::SeqCst)),
            Ordering::SeqCst,
        );

        let current = self.workload_count.load(Ordering::SeqCst);
        if current > 0 {
            self.workload_count.fetch_sub(1, Ordering::SeqCst);
        }

        // Remove usage tracking
        if let Ok(mut usage) = self.workload_usage.write() {
            usage.remove(&workload_id);
        }
    }

    /// Update resource usage for a workload.
    pub fn update_usage(&self, workload_id: Uuid, usage: ResourceUsage) {
        if let Ok(mut usages) = self.workload_usage.write() {
            usages.insert(workload_id, usage);
        }
    }

    /// Get resource usage for a workload.
    #[must_use]
    pub fn get_usage(&self, workload_id: Uuid) -> Option<ResourceUsage> {
        self.workload_usage
            .read()
            .ok()
            .and_then(|u| u.get(&workload_id).cloned())
    }

    /// Check for workloads exceeding their limits.
    ///
    /// Returns a list of (workload_id, violation_reason) pairs.
    #[must_use]
    pub fn check_violations(&self, limits: &HashMap<Uuid, EffectiveResourceLimits>) -> Vec<(Uuid, String)> {
        let mut violations = Vec::new();

        if let Ok(usages) = self.workload_usage.read() {
            for (workload_id, usage) in usages.iter() {
                if let Some(limit) = limits.get(workload_id) {
                    if usage.exceeds_memory_limit(limit.memory_bytes) {
                        violations.push((
                            *workload_id,
                            format!(
                                "memory usage {} exceeds limit {}",
                                usage.memory_bytes, limit.memory_bytes
                            ),
                        ));
                    }

                    if usage.exceeds_cpu_limit(limit.cpu_cores) {
                        violations.push((
                            *workload_id,
                            format!(
                                "CPU usage {:.2} exceeds limit {:.2}",
                                usage.cpu_usage, limit.cpu_cores
                            ),
                        ));
                    }

                    if usage.exceeds_disk_limit(limit.disk_bytes) {
                        violations.push((
                            *workload_id,
                            format!(
                                "disk usage {} exceeds limit {}",
                                usage.disk_bytes, limit.disk_bytes
                            ),
                        ));
                    }
                }
            }
        }

        violations
    }

    /// Check if any resources are approaching limits.
    ///
    /// Returns a list of warning messages.
    #[must_use]
    pub fn check_alerts(&self) -> Vec<ResourceAlert> {
        let mut alerts = Vec::new();
        let threshold = f64::from(self.alert_threshold_percent);

        // Memory alert
        let memory_used = self.allocated_memory_bytes.load(Ordering::SeqCst);
        let memory_percent = (memory_used as f64 / self.capacity.allocatable_memory_bytes() as f64) * 100.0;
        if memory_percent >= threshold {
            alerts.push(ResourceAlert {
                resource: ResourceType::Memory,
                current_percent: memory_percent,
                threshold_percent: threshold,
                message: format!(
                    "Memory usage at {:.1}% ({} of {} bytes)",
                    memory_percent,
                    memory_used,
                    self.capacity.allocatable_memory_bytes()
                ),
            });
        }

        // CPU alert
        let cpu_used = self.allocated_cpu_millicores.load(Ordering::SeqCst);
        let cpu_total = (self.capacity.allocatable_cpu_cores() * 1000.0) as u64;
        let cpu_percent = if cpu_total > 0 {
            (cpu_used as f64 / cpu_total as f64) * 100.0
        } else {
            0.0
        };
        if cpu_percent >= threshold {
            alerts.push(ResourceAlert {
                resource: ResourceType::Cpu,
                current_percent: cpu_percent,
                threshold_percent: threshold,
                message: format!(
                    "CPU usage at {:.1}% ({:.2} of {:.2} cores)",
                    cpu_percent,
                    cpu_used as f64 / 1000.0,
                    self.capacity.allocatable_cpu_cores()
                ),
            });
        }

        // Workload count alert
        let workload_count = self.workload_count.load(Ordering::SeqCst);
        let workload_percent = (workload_count as f64 / f64::from(self.capacity.max_concurrent_workloads)) * 100.0;
        if workload_percent >= threshold {
            alerts.push(ResourceAlert {
                resource: ResourceType::Workloads,
                current_percent: workload_percent,
                threshold_percent: threshold,
                message: format!(
                    "Workload count at {:.1}% ({} of {})",
                    workload_percent,
                    workload_count,
                    self.capacity.max_concurrent_workloads
                ),
            });
        }

        alerts
    }

    /// Get current workload count.
    #[must_use]
    pub fn workload_count(&self) -> u64 {
        self.workload_count.load(Ordering::SeqCst)
    }

    /// Get allocated memory in bytes.
    #[must_use]
    pub fn allocated_memory(&self) -> u64 {
        self.allocated_memory_bytes.load(Ordering::SeqCst)
    }

    /// Get allocated CPU cores.
    #[must_use]
    pub fn allocated_cpu_cores(&self) -> f32 {
        self.allocated_cpu_millicores.load(Ordering::SeqCst) as f32 / 1000.0
    }

    /// Get a reference to the node capacity.
    #[must_use]
    pub const fn capacity(&self) -> &NodeCapacity {
        &self.capacity
    }
}

/// Resource type for alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// Memory resources.
    Memory,
    /// CPU resources.
    Cpu,
    /// Disk resources.
    Disk,
    /// GPU resources.
    Gpu,
    /// Network resources.
    Network,
    /// Workload count.
    Workloads,
}

/// Resource usage alert.
#[derive(Debug, Clone)]
pub struct ResourceAlert {
    /// Type of resource.
    pub resource: ResourceType,
    /// Current usage percentage.
    pub current_percent: f64,
    /// Alert threshold percentage.
    pub threshold_percent: f64,
    /// Human-readable message.
    pub message: String,
}

/// Workload execution watchdog for detecting runaway workloads.
#[derive(Debug)]
pub struct ExecutionWatchdog {
    /// Workloads with their start time and max duration.
    workloads: std::sync::RwLock<HashMap<Uuid, (Instant, Option<Duration>)>>,
}

impl Default for ExecutionWatchdog {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionWatchdog {
    /// Create a new execution watchdog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            workloads: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Register a workload with optional max execution time.
    pub fn register(&self, workload_id: Uuid, max_duration: Option<Duration>) {
        if let Ok(mut workloads) = self.workloads.write() {
            workloads.insert(workload_id, (Instant::now(), max_duration));
        }
    }

    /// Unregister a workload.
    pub fn unregister(&self, workload_id: Uuid) {
        if let Ok(mut workloads) = self.workloads.write() {
            workloads.remove(&workload_id);
        }
    }

    /// Check for workloads that have exceeded their max execution time.
    ///
    /// Returns a list of workload IDs that should be killed.
    #[must_use]
    pub fn check_timeouts(&self) -> Vec<Uuid> {
        let mut timed_out = Vec::new();
        let now = Instant::now();

        if let Ok(workloads) = self.workloads.read() {
            for (id, (start, max_duration)) in workloads.iter() {
                if let Some(max) = max_duration {
                    if now.duration_since(*start) > *max {
                        timed_out.push(*id);
                    }
                }
            }
        }

        timed_out
    }

    /// Get the elapsed time for a workload.
    #[must_use]
    pub fn elapsed(&self, workload_id: Uuid) -> Option<Duration> {
        self.workloads
            .read()
            .ok()
            .and_then(|w| w.get(&workload_id).map(|(start, _)| start.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== ResourceLimits Tests ====================

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert!(limits.memory_bytes.is_none());
        assert!(limits.cpu_cores.is_none());
        assert!(limits.disk_bytes.is_none());
    }

    #[test]
    fn test_resource_limits_builder() {
        let limits = ResourceLimits::new()
            .with_memory_bytes(4 * 1024 * 1024 * 1024)
            .with_cpu_cores(4.0)
            .with_disk_bytes(100 * 1024 * 1024 * 1024)
            .with_gpu_memory_mib(24576)
            .with_network_bandwidth_mbps(1000);

        assert_eq!(limits.memory_bytes, Some(4 * 1024 * 1024 * 1024));
        assert_eq!(limits.cpu_cores, Some(4.0));
        assert_eq!(limits.gpu_memory_mib, Some(24576));
    }

    #[test]
    fn test_resource_limits_memory_mib() {
        let limits = ResourceLimits::new().with_memory_mib(8192);
        assert_eq!(limits.memory_bytes, Some(8192 * 1024 * 1024));
    }

    #[test]
    fn test_resource_limits_validate_success() {
        let limits = ResourceLimits::new()
            .with_memory_bytes(1024)
            .with_cpu_cores(1.0)
            .with_oom_score_adj(500);

        assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_resource_limits_validate_zero_memory() {
        let limits = ResourceLimits::new().with_memory_bytes(0);
        let result = limits.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("memory_bytes cannot be zero"));
    }

    #[test]
    fn test_resource_limits_validate_negative_cpu() {
        let limits = ResourceLimits::new().with_cpu_cores(-1.0);
        let result = limits.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cpu_cores must be positive"));
    }

    #[test]
    fn test_resource_limits_validate_excessive_cpu() {
        let limits = ResourceLimits::new().with_cpu_cores(2000.0);
        let result = limits.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_resource_limits_validate_invalid_oom() {
        let limits = ResourceLimits::new().with_oom_score_adj(2000);
        let result = limits.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("oom_score_adj"));
    }

    // ==================== NodeCapacity Tests ====================

    #[test]
    fn test_node_capacity_default() {
        let capacity = NodeCapacity::default();
        assert_eq!(capacity.total_memory_bytes, 16 * 1024 * 1024 * 1024);
        assert_eq!(capacity.total_cpu_cores, 8);
        assert_eq!(capacity.system_reserved_percent, 10);
    }

    #[test]
    fn test_node_capacity_builder() {
        let capacity = NodeCapacity::new(32 * 1024 * 1024 * 1024, 16, 1024 * 1024 * 1024 * 1024)
            .with_gpus(vec![24576, 24576])
            .with_system_reserved_percent(20)
            .with_max_concurrent_workloads(128);

        assert_eq!(capacity.total_memory_bytes, 32 * 1024 * 1024 * 1024);
        assert_eq!(capacity.total_cpu_cores, 16);
        assert_eq!(capacity.total_gpus, 2);
        assert_eq!(capacity.system_reserved_percent, 20);
        assert_eq!(capacity.max_concurrent_workloads, 128);
    }

    #[test]
    fn test_node_capacity_allocatable_resources() {
        let capacity = NodeCapacity::new(100, 10, 1000)
            .with_system_reserved_percent(10);

        assert_eq!(capacity.allocatable_memory_bytes(), 90);
        assert!((capacity.allocatable_cpu_cores() - 9.0).abs() < 0.01);
        assert_eq!(capacity.allocatable_disk_bytes(), 900);
    }

    #[test]
    fn test_node_capacity_validate_success() {
        let capacity = NodeCapacity::new(1024, 4, 10240)
            .with_gpus(vec![24576])
            .with_max_concurrent_workloads(10);

        assert!(capacity.validate().is_ok());
    }

    #[test]
    fn test_node_capacity_validate_zero_memory() {
        let capacity = NodeCapacity::new(0, 4, 1024);
        assert!(capacity.validate().is_err());
    }

    #[test]
    fn test_node_capacity_validate_excessive_reserved() {
        // Test that excessive values get clamped in builder
        let capacity = NodeCapacity::default().with_system_reserved_percent(60);
        // Should be clamped to 50, so validation passes
        assert_eq!(capacity.system_reserved_percent, 50);
        assert!(capacity.validate().is_ok());

        // Test direct mutation fails validation
        let mut capacity = NodeCapacity::default();
        capacity.system_reserved_percent = 60;
        let result = capacity.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 50"));
    }

    #[test]
    fn test_node_capacity_gpu_mismatch() {
        let mut capacity = NodeCapacity::default();
        capacity.total_gpus = 4;
        capacity.gpu_memory_mib = vec![24576, 24576]; // Only 2 entries

        let result = capacity.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("gpu_memory_mib length"));
    }

    // ==================== resolve_limits Tests ====================

    #[test]
    fn test_resolve_limits_defaults() {
        let requested = ResourceLimits::default();
        let capacity = NodeCapacity::default();

        let result = resolve_limits(&requested, &capacity, 0);
        assert!(result.is_ok());

        let effective = result.unwrap();
        assert!(effective.memory_bytes > 0);
        assert!(effective.cpu_cores > 0.0);
    }

    #[test]
    fn test_resolve_limits_within_capacity() {
        let requested = ResourceLimits::new()
            .with_memory_bytes(1024 * 1024 * 1024)
            .with_cpu_cores(2.0);

        let capacity = NodeCapacity::new(16 * 1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024);

        let result = resolve_limits(&requested, &capacity, 0);
        assert!(result.is_ok());

        let effective = result.unwrap();
        assert_eq!(effective.memory_bytes, 1024 * 1024 * 1024);
        assert_eq!(effective.cpu_cores, 2.0);
    }

    #[test]
    fn test_resolve_limits_exceeds_capacity() {
        let requested = ResourceLimits::new()
            .with_memory_bytes(100 * 1024 * 1024 * 1024); // 100 GiB

        let capacity = NodeCapacity::new(16 * 1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024);

        let result = resolve_limits(&requested, &capacity, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("memory"));
    }

    // ==================== ResourceUsage Tests ====================

    #[test]
    fn test_resource_usage_default() {
        let usage = ResourceUsage::new();
        assert_eq!(usage.memory_bytes, 0);
        assert_eq!(usage.cpu_usage, 0.0);
        assert!(usage.last_updated.is_none());
    }

    #[test]
    fn test_resource_usage_exceeds_limits() {
        let mut usage = ResourceUsage::new();
        usage.memory_bytes = 2000;
        usage.cpu_usage = 3.5;
        usage.disk_bytes = 500;

        assert!(usage.exceeds_memory_limit(1000));
        assert!(!usage.exceeds_memory_limit(3000));

        assert!(usage.exceeds_cpu_limit(2.0));
        assert!(!usage.exceeds_cpu_limit(4.0));

        assert!(usage.exceeds_disk_limit(400));
        assert!(!usage.exceeds_disk_limit(600));
    }

    #[test]
    fn test_resource_usage_memory_percent() {
        let mut usage = ResourceUsage::new();
        usage.memory_bytes = 500;

        assert!((usage.memory_percent(1000) - 50.0).abs() < 0.01);
        assert_eq!(usage.memory_percent(0), 0.0);
    }

    // ==================== ResourceTracker Tests ====================

    #[test]
    fn test_resource_tracker_creation() {
        let capacity = NodeCapacity::default();
        let tracker = ResourceTracker::new(capacity);

        assert_eq!(tracker.workload_count(), 0);
        assert_eq!(tracker.allocated_memory(), 0);
    }

    #[test]
    fn test_resource_tracker_reserve_and_release() {
        let capacity = NodeCapacity::new(16 * 1024 * 1024 * 1024, 8, 100 * 1024 * 1024 * 1024)
            .with_max_concurrent_workloads(10);
        let tracker = ResourceTracker::new(capacity);

        let workload_id = Uuid::new_v4();
        let limits = EffectiveResourceLimits {
            memory_bytes: 1024 * 1024 * 1024,
            cpu_cores: 2.0,
            disk_bytes: 10 * 1024 * 1024 * 1024,
            gpu_memory_mib: 0,
            network_bandwidth_mbps: 1000,
            max_execution_time: None,
            oom_score_adj: 0,
        };

        // Reserve
        let result = tracker.reserve(workload_id, &limits);
        assert!(result.is_ok());
        assert_eq!(tracker.workload_count(), 1);
        assert_eq!(tracker.allocated_memory(), 1024 * 1024 * 1024);

        // Release
        tracker.release(workload_id, &limits);
        assert_eq!(tracker.workload_count(), 0);
        assert_eq!(tracker.allocated_memory(), 0);
    }

    #[test]
    fn test_resource_tracker_max_workloads() {
        let capacity = NodeCapacity::new(1024 * 1024 * 1024 * 1024, 128, 1024 * 1024 * 1024 * 1024)
            .with_max_concurrent_workloads(2);
        let tracker = ResourceTracker::new(capacity);

        let limits = EffectiveResourceLimits {
            memory_bytes: 1024,
            cpu_cores: 0.1,
            disk_bytes: 1024,
            gpu_memory_mib: 0,
            network_bandwidth_mbps: 100,
            max_execution_time: None,
            oom_score_adj: 0,
        };

        // Reserve 2 workloads (max)
        tracker.reserve(Uuid::new_v4(), &limits).unwrap();
        tracker.reserve(Uuid::new_v4(), &limits).unwrap();

        // Third should fail
        let result = tracker.can_accept_workload(&limits);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::MaxWorkloadsExceeded { .. }));
    }

    #[test]
    fn test_resource_tracker_insufficient_memory() {
        let capacity = NodeCapacity::new(1024 * 1024, 8, 1024 * 1024 * 1024);
        let tracker = ResourceTracker::new(capacity);

        let limits = EffectiveResourceLimits {
            memory_bytes: 2 * 1024 * 1024, // More than allocatable
            cpu_cores: 1.0,
            disk_bytes: 1024,
            gpu_memory_mib: 0,
            network_bandwidth_mbps: 100,
            max_execution_time: None,
            oom_score_adj: 0,
        };

        let result = tracker.can_accept_workload(&limits);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), NodeError::InsufficientMemory { .. }));
    }

    #[test]
    fn test_resource_tracker_alerts() {
        let capacity = NodeCapacity::new(1000, 10, 10000)
            .with_max_concurrent_workloads(10);
        let tracker = ResourceTracker::new(capacity).with_alert_threshold(80);

        let limits = EffectiveResourceLimits {
            memory_bytes: 800, // 80% of allocatable (900)
            cpu_cores: 8.0,    // 88% of allocatable (9.0)
            disk_bytes: 100,
            gpu_memory_mib: 0,
            network_bandwidth_mbps: 100,
            max_execution_time: None,
            oom_score_adj: 0,
        };

        tracker.reserve(Uuid::new_v4(), &limits).unwrap();

        let alerts = tracker.check_alerts();
        assert!(!alerts.is_empty());
        assert!(alerts.iter().any(|a| matches!(a.resource, ResourceType::Cpu)));
    }

    #[test]
    fn test_resource_tracker_usage_tracking() {
        let capacity = NodeCapacity::default();
        let tracker = ResourceTracker::new(capacity);

        let workload_id = Uuid::new_v4();
        let limits = EffectiveResourceLimits {
            memory_bytes: 1024,
            cpu_cores: 1.0,
            disk_bytes: 1024,
            gpu_memory_mib: 0,
            network_bandwidth_mbps: 100,
            max_execution_time: None,
            oom_score_adj: 0,
        };

        tracker.reserve(workload_id, &limits).unwrap();

        let mut usage = ResourceUsage::new();
        usage.memory_bytes = 512;
        usage.cpu_usage = 0.5;

        tracker.update_usage(workload_id, usage);

        let retrieved = tracker.get_usage(workload_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().memory_bytes, 512);
    }

    // ==================== ExecutionWatchdog Tests ====================

    #[test]
    fn test_watchdog_creation() {
        let watchdog = ExecutionWatchdog::new();
        assert!(watchdog.check_timeouts().is_empty());
    }

    #[test]
    fn test_watchdog_register_and_unregister() {
        let watchdog = ExecutionWatchdog::new();
        let id = Uuid::new_v4();

        watchdog.register(id, Some(Duration::from_secs(60)));
        assert!(watchdog.elapsed(id).is_some());

        watchdog.unregister(id);
        assert!(watchdog.elapsed(id).is_none());
    }

    #[test]
    fn test_watchdog_no_timeout() {
        let watchdog = ExecutionWatchdog::new();
        let id = Uuid::new_v4();

        // Register with no timeout
        watchdog.register(id, None);

        // Should never timeout
        assert!(watchdog.check_timeouts().is_empty());
    }

    #[test]
    fn test_watchdog_timeout_detection() {
        let watchdog = ExecutionWatchdog::new();
        let id = Uuid::new_v4();

        // Register with very short timeout
        watchdog.register(id, Some(Duration::from_millis(1)));

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(10));

        let timed_out = watchdog.check_timeouts();
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], id);
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_resource_limits_serialization() {
        let limits = ResourceLimits::new()
            .with_memory_bytes(1024)
            .with_cpu_cores(2.0);

        let json = serde_json::to_string(&limits).unwrap();
        let parsed: ResourceLimits = serde_json::from_str(&json).unwrap();

        assert_eq!(limits, parsed);
    }

    #[test]
    fn test_node_capacity_serialization() {
        let capacity = NodeCapacity::new(1024, 4, 10240)
            .with_gpus(vec![24576, 24576]);

        let json = serde_json::to_string(&capacity).unwrap();
        let parsed: NodeCapacity = serde_json::from_str(&json).unwrap();

        assert_eq!(capacity, parsed);
    }
}
