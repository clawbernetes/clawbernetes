//! Core types for the Clawbernetes protocol.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::error::ProtoError;
use crate::scheduling::NodeCondition;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NodeCapabilities {
    /// Available GPUs.
    pub gpus: Vec<GpuCapability>,
    /// Total system memory in MiB.
    pub memory_mib: u64,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Supported container runtimes.
    pub runtimes: Vec<String>,
    /// Custom node conditions (e.g., "cuda-ready", "model-cached").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<NodeCondition>,
    /// Node labels for selector matching.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub labels: std::collections::HashMap<String, String>,
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
            conditions: Vec::new(),
            labels: std::collections::HashMap::new(),
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

    /// Add a custom condition.
    #[must_use]
    pub fn with_condition(mut self, condition: NodeCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Calculate total VRAM across all GPUs.
    #[must_use]
    pub fn total_vram_mib(&self) -> u64 {
        self.gpus.iter().map(|g| g.memory_mib).sum()
    }

    /// Find a condition by type.
    #[must_use]
    pub fn get_condition(&self, condition_type: &str) -> Option<&NodeCondition> {
        self.conditions
            .iter()
            .find(|c| c.condition_type == condition_type)
    }

    /// Check if a condition is satisfied (true).
    #[must_use]
    pub fn is_condition_satisfied(&self, condition_type: &str) -> bool {
        self.get_condition(condition_type)
            .is_some_and(|c| c.is_satisfied())
    }

    /// Check if all required labels match.
    #[must_use]
    pub fn matches_selector(&self, selector: &std::collections::HashMap<String, String>) -> bool {
        selector
            .iter()
            .all(|(k, v)| self.labels.get(k).is_some_and(|label_v| label_v == v))
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
        assert!(caps.conditions.is_empty());
        assert!(caps.labels.is_empty());
    }

    #[test]
    fn test_node_capabilities_with_condition() {
        use crate::scheduling::{ConditionStatus, NodeCondition};
        
        let caps = NodeCapabilities::new(8, 16384)
            .with_condition(NodeCondition::new("cuda-ready", ConditionStatus::True))
            .with_label("gpu-type", "nvidia");

        assert!(caps.is_condition_satisfied("cuda-ready"));
        assert!(!caps.is_condition_satisfied("unknown"));
        
        let mut selector = std::collections::HashMap::new();
        selector.insert("gpu-type".into(), "nvidia".into());
        assert!(caps.matches_selector(&selector));
        
        selector.insert("other".into(), "value".into());
        assert!(!caps.matches_selector(&selector));
    }

    #[test]
    fn test_workload_state_is_terminal() {
        assert!(!WorkloadState::Pending.is_terminal());
        assert!(WorkloadState::Failed.is_terminal());
    }
}
