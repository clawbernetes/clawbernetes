//! Core types for the Clawbernetes protocol.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::error::ProtoError;

/// Unique identifier for a Clawbernetes node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Create a new random `NodeId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a `NodeId` from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, ProtoError> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| ProtoError::Validation(format!("invalid node ID: {e}")))
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for NodeId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkloadId(Uuid);

impl WorkloadId {
    /// Create a new random `WorkloadId`.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Parse a `WorkloadId` from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, ProtoError> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| ProtoError::Validation(format!("invalid workload ID: {e}")))
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for WorkloadId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for WorkloadId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl fmt::Display for WorkloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// GPU capability information for protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuCapability {
    /// GPU index on the node.
    pub index: u32,
    /// GPU model name.
    pub name: String,
    /// Total VRAM in MiB.
    pub memory_mib: u64,
    /// GPU UUID.
    pub uuid: String,
}

/// Node capability advertisement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NodeCapabilities {
    /// Available GPUs.
    pub gpus: Vec<GpuCapability>,
    /// Total system memory in MiB.
    pub memory_mib: u64,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Supported container runtimes.
    pub runtimes: Vec<String>,
}

impl NodeCapabilities {
    /// Create new capabilities.
    #[must_use]
    pub fn new(cpu_cores: u32, memory_mib: u64) -> Self {
        Self {
            gpus: Vec::new(),
            memory_mib,
            cpu_cores,
            runtimes: Vec::new(),
        }
    }

    /// Add a GPU capability.
    #[must_use]
    pub fn with_gpu(mut self, gpu: GpuCapability) -> Self {
        self.gpus.push(gpu);
        self
    }

    /// Add a supported runtime.
    #[must_use]
    pub fn with_runtime(mut self, runtime: impl Into<String>) -> Self {
        self.runtimes.push(runtime.into());
        self
    }

    /// Calculate total VRAM across all GPUs.
    #[must_use]
    pub fn total_vram_mib(&self) -> u64 {
        self.gpus.iter().map(|g| g.memory_mib).sum()
    }
}

/// GPU metrics for protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuMetricsProto {
    /// GPU index.
    pub index: u32,
    /// Utilization percentage (0-100).
    pub utilization_percent: u8,
    /// Memory used in MiB.
    pub memory_used_mib: u64,
    /// Memory total in MiB.
    pub memory_total_mib: u64,
    /// Temperature in Celsius.
    pub temperature_celsius: u32,
    /// Power usage in Watts (optional).
    pub power_watts: Option<f32>,
}

/// State of a workload in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadState {
    /// Workload is queued, waiting to start.
    Pending,
    /// Workload is starting up.
    Starting,
    /// Workload is actively running.
    Running,
    /// Workload is in the process of stopping.
    Stopping,
    /// Workload has stopped.
    Stopped,
    /// Workload completed successfully.
    Completed,
    /// Workload failed with an error.
    Failed,
}

impl WorkloadState {
    /// Check if this is a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Stopped)
    }
}

impl fmt::Display for WorkloadState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Pending => "pending",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Completed => "completed",
            Self::Failed => "failed",
        };
        write!(f, "{s}")
    }
}

/// Workload specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadSpec {
    /// Container image.
    pub image: String,
    /// Command to run (optional override).
    pub command: Option<Vec<String>>,
    /// Environment variables.
    pub env: Vec<(String, String)>,
    /// GPU indices to attach.
    pub gpu_indices: Vec<u32>,
    /// Memory limit in MiB.
    pub memory_limit_mib: Option<u64>,
    /// CPU limit (fractional cores).
    pub cpu_limit: Option<f32>,
}

impl WorkloadSpec {
    /// Create a new workload spec with just an image.
    #[must_use]
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            command: None,
            env: Vec::new(),
            gpu_indices: Vec::new(),
            memory_limit_mib: None,
            cpu_limit: None,
        }
    }

    /// Set the command.
    #[must_use]
    pub fn with_command(mut self, cmd: Vec<String>) -> Self {
        self.command = Some(cmd);
        self
    }

    /// Add an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Set GPU indices.
    #[must_use]
    pub fn with_gpus(mut self, indices: Vec<u32>) -> Self {
        self.gpu_indices = indices;
        self
    }

    /// Set memory limit.
    #[must_use]
    pub fn with_memory_limit(mut self, limit_mib: u64) -> Self {
        self.memory_limit_mib = Some(limit_mib);
        self
    }

    /// Set CPU limit.
    #[must_use]
    pub fn with_cpu_limit(mut self, limit: f32) -> Self {
        self.cpu_limit = Some(limit);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_new() {
        let id = NodeId::new();
        assert_eq!(id.as_uuid().get_version_num(), 4);
    }

    #[test]
    fn test_workload_id_new() {
        let id = WorkloadId::new();
        assert_eq!(id.as_uuid().get_version_num(), 4);
    }

    #[test]
    fn test_node_capabilities_default() {
        let caps = NodeCapabilities::default();
        assert!(caps.gpus.is_empty());
        assert_eq!(caps.memory_mib, 0);
    }

    #[test]
    fn test_workload_state_is_terminal() {
        assert!(!WorkloadState::Pending.is_terminal());
        assert!(WorkloadState::Failed.is_terminal());
    }

    #[test]
    fn test_workload_spec_builder() {
        let spec = WorkloadSpec::new("nginx:latest")
            .with_gpus(vec![0])
            .with_memory_limit(1024);
        assert_eq!(spec.image, "nginx:latest");
        assert_eq!(spec.gpu_indices, vec![0]);
    }
}
