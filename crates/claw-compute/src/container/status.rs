//! Container status and state types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;

use super::error::ContainerId;

/// Container lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerState {
    /// Container is being created.
    Creating,

    /// Container has been created but not started.
    Created,

    /// Container is starting up.
    Starting,

    /// Container is running.
    Running,

    /// Container is paused.
    Paused,

    /// Container is stopping.
    Stopping,

    /// Container has exited.
    Exited,

    /// Container has been removed.
    Removed,

    /// Container is in error state.
    Error,

    /// Container state is unknown.
    Unknown,
}

impl ContainerState {
    /// Check if container is active (running or paused).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Check if container can be started.
    #[must_use]
    pub const fn can_start(&self) -> bool {
        matches!(self, Self::Created | Self::Exited)
    }

    /// Check if container can be stopped.
    #[must_use]
    pub const fn can_stop(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Check if container is terminal (won't change without intervention).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Exited | Self::Removed | Self::Error)
    }

    /// Get state name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopping => "stopping",
            Self::Exited => "exited",
            Self::Removed => "removed",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ContainerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Default for ContainerState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Container health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HealthStatus {
    /// No health check configured.
    #[default]
    None,

    /// Health check is starting up.
    Starting,

    /// Container is healthy.
    Healthy,

    /// Container is unhealthy.
    Unhealthy,
}

impl HealthStatus {
    /// Get status name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Resource usage statistics for a container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceStats {
    /// CPU usage percentage (0.0 - 100.0+ for multi-core).
    pub cpu_percent: f64,

    /// Memory usage in bytes.
    pub memory_bytes: u64,

    /// Memory limit in bytes.
    pub memory_limit: u64,

    /// Network RX bytes.
    pub network_rx_bytes: u64,

    /// Network TX bytes.
    pub network_tx_bytes: u64,

    /// Block I/O read bytes.
    pub block_read_bytes: u64,

    /// Block I/O write bytes.
    pub block_write_bytes: u64,

    /// Number of PIDs in container.
    pub pids: u64,

    /// GPU memory usage per device (device index -> bytes).
    pub gpu_memory: HashMap<u32, u64>,

    /// GPU utilization per device (device index -> percent).
    pub gpu_utilization: HashMap<u32, f64>,

    /// Timestamp of stats collection.
    pub timestamp: Option<SystemTime>,
}

impl ResourceStats {
    /// Get memory usage as percentage.
    #[must_use]
    pub fn memory_percent(&self) -> f64 {
        if self.memory_limit > 0 {
            (self.memory_bytes as f64 / self.memory_limit as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get memory usage in megabytes.
    #[must_use]
    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Get memory limit in megabytes.
    #[must_use]
    pub fn memory_limit_mb(&self) -> f64 {
        self.memory_limit as f64 / (1024.0 * 1024.0)
    }
}

/// Network settings for a running container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkSettings {
    /// IP address (bridge network).
    pub ip_address: Option<String>,

    /// Gateway IP address.
    pub gateway: Option<String>,

    /// MAC address.
    pub mac_address: Option<String>,

    /// Network name to settings map.
    pub networks: HashMap<String, NetworkEndpoint>,
}

/// Network endpoint details.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkEndpoint {
    /// Network ID.
    pub network_id: String,

    /// IP address.
    pub ip_address: String,

    /// Gateway.
    pub gateway: String,

    /// MAC address.
    pub mac_address: String,

    /// Network aliases.
    pub aliases: Vec<String>,
}

/// Port binding information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBinding {
    /// Container port.
    pub container_port: u16,

    /// Protocol (tcp/udp).
    pub protocol: String,

    /// Host IP to bind to.
    pub host_ip: Option<String>,

    /// Host port.
    pub host_port: Option<u16>,
}

/// Full container status including state and resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStatus {
    /// Container ID.
    pub id: String,

    /// Container name.
    pub name: String,

    /// Container image.
    pub image: String,

    /// Current state.
    pub state: ContainerState,

    /// Health status (if health check configured).
    pub health: HealthStatus,

    /// Exit code (if exited).
    pub exit_code: Option<i64>,

    /// Error message (if in error state).
    pub error: Option<String>,

    /// Creation timestamp.
    pub created_at: Option<SystemTime>,

    /// Start timestamp.
    pub started_at: Option<SystemTime>,

    /// Stop/exit timestamp.
    pub finished_at: Option<SystemTime>,

    /// Resource usage stats.
    pub stats: ResourceStats,

    /// Network settings.
    pub network: NetworkSettings,

    /// Port bindings.
    pub ports: Vec<PortBinding>,

    /// Container labels.
    pub labels: HashMap<String, String>,

    /// Restart count.
    pub restart_count: u32,

    /// Platform (OS/arch).
    pub platform: Option<String>,
}

impl ContainerStatus {
    /// Create status for a newly created container.
    #[must_use]
    pub fn created(id: impl Into<String>, name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            image: image.into(),
            state: ContainerState::Created,
            health: HealthStatus::None,
            exit_code: None,
            error: None,
            created_at: Some(SystemTime::now()),
            started_at: None,
            finished_at: None,
            stats: ResourceStats::default(),
            network: NetworkSettings::default(),
            ports: Vec::new(),
            labels: HashMap::new(),
            restart_count: 0,
            platform: None,
        }
    }

    /// Get container ID.
    #[must_use]
    pub fn container_id(&self) -> ContainerId {
        ContainerId::new_unchecked(&self.id)
    }

    /// Check if container is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state == ContainerState::Running
    }

    /// Check if container has exited successfully.
    #[must_use]
    pub fn exited_successfully(&self) -> bool {
        self.state == ContainerState::Exited && self.exit_code == Some(0)
    }

    /// Get uptime duration (if running).
    #[must_use]
    pub fn uptime(&self) -> Option<std::time::Duration> {
        if self.state == ContainerState::Running {
            self.started_at.and_then(|start| {
                SystemTime::now().duration_since(start).ok()
            })
        } else {
            None
        }
    }

    /// Get runtime duration (from start to finish, if exited).
    #[must_use]
    pub fn runtime(&self) -> Option<std::time::Duration> {
        if let (Some(start), Some(finish)) = (self.started_at, self.finished_at) {
            finish.duration_since(start).ok()
        } else {
            None
        }
    }
}

impl Default for ContainerStatus {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            image: String::new(),
            state: ContainerState::Unknown,
            health: HealthStatus::None,
            exit_code: None,
            error: None,
            created_at: None,
            started_at: None,
            finished_at: None,
            stats: ResourceStats::default(),
            network: NetworkSettings::default(),
            ports: Vec::new(),
            labels: HashMap::new(),
            restart_count: 0,
            platform: None,
        }
    }
}

/// Summary of container for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSummary {
    /// Container ID.
    pub id: String,

    /// Container name.
    pub name: String,

    /// Image name.
    pub image: String,

    /// Current state.
    pub state: ContainerState,

    /// Health status.
    pub health: HealthStatus,

    /// Creation time.
    pub created_at: Option<SystemTime>,

    /// Status message (e.g., "Up 2 hours", "Exited (0) 5 minutes ago").
    pub status_message: String,
}

impl ContainerSummary {
    /// Create from full status.
    #[must_use]
    pub fn from_status(status: &ContainerStatus) -> Self {
        let status_message = match status.state {
            ContainerState::Running => {
                if let Some(uptime) = status.uptime() {
                    format!("Up {}", format_duration(uptime))
                } else {
                    "Running".to_string()
                }
            }
            ContainerState::Exited => {
                let code = status.exit_code.unwrap_or(-1);
                if let Some(runtime) = status.runtime() {
                    format!("Exited ({code}) {}", format_duration(runtime))
                } else {
                    format!("Exited ({code})")
                }
            }
            state => state.name().to_string(),
        };

        Self {
            id: status.id.clone(),
            name: status.name.clone(),
            image: status.image.clone(),
            state: status.state,
            health: status.health,
            created_at: status.created_at,
            status_message,
        }
    }
}

/// Format duration as human-readable string.
fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs} seconds")
    } else if secs < 3600 {
        format!("{} minutes", secs / 60)
    } else if secs < 86400 {
        format!("{} hours", secs / 3600)
    } else {
        format!("{} days", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ContainerState Tests
    // =========================================================================

    #[test]
    fn test_container_state_is_active() {
        assert!(ContainerState::Running.is_active());
        assert!(ContainerState::Paused.is_active());
        assert!(!ContainerState::Created.is_active());
        assert!(!ContainerState::Exited.is_active());
    }

    #[test]
    fn test_container_state_can_start() {
        assert!(ContainerState::Created.can_start());
        assert!(ContainerState::Exited.can_start());
        assert!(!ContainerState::Running.can_start());
    }

    #[test]
    fn test_container_state_can_stop() {
        assert!(ContainerState::Running.can_stop());
        assert!(ContainerState::Paused.can_stop());
        assert!(!ContainerState::Exited.can_stop());
    }

    #[test]
    fn test_container_state_is_terminal() {
        assert!(ContainerState::Exited.is_terminal());
        assert!(ContainerState::Removed.is_terminal());
        assert!(ContainerState::Error.is_terminal());
        assert!(!ContainerState::Running.is_terminal());
    }

    #[test]
    fn test_container_state_display() {
        assert_eq!(ContainerState::Running.to_string(), "running");
        assert_eq!(ContainerState::Exited.to_string(), "exited");
    }

    #[test]
    fn test_container_state_default() {
        assert_eq!(ContainerState::default(), ContainerState::Unknown);
    }

    // =========================================================================
    // HealthStatus Tests
    // =========================================================================

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_health_status_default() {
        assert_eq!(HealthStatus::default(), HealthStatus::None);
    }

    // =========================================================================
    // ResourceStats Tests
    // =========================================================================

    #[test]
    fn test_resource_stats_memory_percent() {
        let stats = ResourceStats {
            memory_bytes: 512 * 1024 * 1024, // 512MB
            memory_limit: 1024 * 1024 * 1024, // 1GB
            ..Default::default()
        };
        assert!((stats.memory_percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_resource_stats_memory_percent_no_limit() {
        let stats = ResourceStats {
            memory_bytes: 512 * 1024 * 1024,
            memory_limit: 0,
            ..Default::default()
        };
        assert_eq!(stats.memory_percent(), 0.0);
    }

    #[test]
    fn test_resource_stats_memory_mb() {
        let stats = ResourceStats {
            memory_bytes: 512 * 1024 * 1024,
            ..Default::default()
        };
        assert!((stats.memory_mb() - 512.0).abs() < 0.01);
    }

    // =========================================================================
    // ContainerStatus Tests
    // =========================================================================

    #[test]
    fn test_container_status_created() {
        let status = ContainerStatus::created("abc123", "my-container", "nginx:latest");
        assert_eq!(status.state, ContainerState::Created);
        assert!(status.created_at.is_some());
    }

    #[test]
    fn test_container_status_is_running() {
        let mut status = ContainerStatus::default();
        status.state = ContainerState::Running;
        assert!(status.is_running());

        status.state = ContainerState::Exited;
        assert!(!status.is_running());
    }

    #[test]
    fn test_container_status_exited_successfully() {
        let mut status = ContainerStatus::default();
        status.state = ContainerState::Exited;
        status.exit_code = Some(0);
        assert!(status.exited_successfully());

        status.exit_code = Some(1);
        assert!(!status.exited_successfully());
    }

    #[test]
    fn test_container_status_uptime() {
        let mut status = ContainerStatus::default();
        status.state = ContainerState::Running;
        status.started_at = Some(SystemTime::now() - std::time::Duration::from_secs(60));
        
        let uptime = status.uptime();
        assert!(uptime.is_some());
        assert!(uptime.map_or(false, |d| d.as_secs() >= 59));
    }

    #[test]
    fn test_container_status_uptime_not_running() {
        let status = ContainerStatus::default();
        assert!(status.uptime().is_none());
    }

    // =========================================================================
    // ContainerSummary Tests
    // =========================================================================

    #[test]
    fn test_container_summary_from_status() {
        let mut status = ContainerStatus::created("abc123", "test", "nginx");
        status.state = ContainerState::Running;
        status.started_at = Some(SystemTime::now() - std::time::Duration::from_secs(120));

        let summary = ContainerSummary::from_status(&status);
        assert_eq!(summary.state, ContainerState::Running);
        assert!(summary.status_message.contains("Up"));
    }

    #[test]
    fn test_container_summary_exited() {
        let mut status = ContainerStatus::created("abc123", "test", "nginx");
        status.state = ContainerState::Exited;
        status.exit_code = Some(0);

        let summary = ContainerSummary::from_status(&status);
        assert!(summary.status_message.contains("Exited (0)"));
    }

    // =========================================================================
    // format_duration Tests
    // =========================================================================

    #[test]
    fn test_format_duration_seconds() {
        let d = std::time::Duration::from_secs(30);
        assert_eq!(format_duration(d), "30 seconds");
    }

    #[test]
    fn test_format_duration_minutes() {
        let d = std::time::Duration::from_secs(120);
        assert_eq!(format_duration(d), "2 minutes");
    }

    #[test]
    fn test_format_duration_hours() {
        let d = std::time::Duration::from_secs(7200);
        assert_eq!(format_duration(d), "2 hours");
    }

    #[test]
    fn test_format_duration_days() {
        let d = std::time::Duration::from_secs(172800);
        assert_eq!(format_duration(d), "2 days");
    }

    // =========================================================================
    // Serialization Tests
    // =========================================================================

    #[test]
    fn test_container_state_serialization() {
        let state = ContainerState::Running;
        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: ContainerState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, deserialized);
    }

    #[test]
    fn test_container_status_serialization() {
        let status = ContainerStatus::created("abc123", "test", "nginx");
        let json = serde_json::to_string(&status).expect("serialize");
        let deserialized: ContainerStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status.id, deserialized.id);
        assert_eq!(status.state, deserialized.state);
    }
}
