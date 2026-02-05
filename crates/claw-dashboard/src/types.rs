//! Dashboard API types for cluster status and live updates.

use chrono::{DateTime, Utc};
use claw_gateway::{HealthSummary, NodeHealthStatus, RegisteredNode, TrackedWorkload};
use claw_proto::{NodeCapabilities, NodeId, WorkloadId, WorkloadState};
use serde::{Deserialize, Serialize};

/// Complete cluster status overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    /// Total number of nodes.
    pub total_nodes: usize,
    /// Node health breakdown.
    pub node_health: NodeHealthSummary,
    /// Total number of workloads.
    pub total_workloads: usize,
    /// Workload state breakdown.
    pub workload_states: WorkloadStateSummary,
    /// GPU utilization summary across the cluster.
    pub gpu_utilization: GpuUtilizationSummary,
    /// Timestamp of this status snapshot.
    pub timestamp: DateTime<Utc>,
    /// Server uptime in seconds.
    pub uptime_secs: u64,
}

/// Summary of node health across the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeHealthSummary {
    /// Number of healthy nodes.
    pub healthy: usize,
    /// Number of unhealthy nodes.
    pub unhealthy: usize,
    /// Number of draining nodes.
    pub draining: usize,
    /// Number of offline nodes.
    pub offline: usize,
}

impl From<HealthSummary> for NodeHealthSummary {
    fn from(s: HealthSummary) -> Self {
        Self {
            healthy: s.healthy,
            unhealthy: s.unhealthy,
            draining: s.draining,
            offline: s.offline,
        }
    }
}

/// Summary of workload states across the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkloadStateSummary {
    /// Number of pending workloads.
    pub pending: usize,
    /// Number of starting workloads.
    pub starting: usize,
    /// Number of running workloads.
    pub running: usize,
    /// Number of stopping workloads.
    pub stopping: usize,
    /// Number of stopped workloads.
    pub stopped: usize,
    /// Number of completed workloads.
    pub completed: usize,
    /// Number of failed workloads.
    pub failed: usize,
}

impl WorkloadStateSummary {
    /// Add a workload state to the summary.
    pub fn add(&mut self, state: WorkloadState) {
        match state {
            WorkloadState::Pending => self.pending += 1,
            WorkloadState::Starting => self.starting += 1,
            WorkloadState::Running => self.running += 1,
            WorkloadState::Stopping => self.stopping += 1,
            WorkloadState::Stopped => self.stopped += 1,
            WorkloadState::Completed => self.completed += 1,
            WorkloadState::Failed => self.failed += 1,
        }
    }

    /// Total number of workloads.
    #[must_use]
    pub fn total(&self) -> usize {
        self.pending
            + self.starting
            + self.running
            + self.stopping
            + self.stopped
            + self.completed
            + self.failed
    }
}

/// GPU utilization summary across the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuUtilizationSummary {
    /// Total number of GPUs.
    pub total_gpus: usize,
    /// Total VRAM in MiB.
    pub total_vram_mib: u64,
    /// Used VRAM in MiB (approximate).
    pub used_vram_mib: u64,
    /// Average GPU utilization percentage.
    pub avg_utilization_percent: f32,
}

/// Status information for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node ID.
    pub id: NodeId,
    /// Human-readable name.
    pub name: String,
    /// Health status.
    pub health: NodeHealthStatus,
    /// Node capabilities.
    pub capabilities: NodeCapabilities,
    /// When the node was registered.
    pub registered_at: DateTime<Utc>,
    /// Last heartbeat timestamp.
    pub last_heartbeat: DateTime<Utc>,
    /// Whether the node is draining.
    pub draining: bool,
    /// Number of workloads assigned to this node.
    pub workload_count: usize,
}

impl NodeStatus {
    /// Create a node status from a registered node.
    pub fn from_registered(node: &RegisteredNode, workload_count: usize) -> Self {
        Self {
            id: node.id,
            name: node.name.clone(),
            health: node.health_status(),
            capabilities: node.capabilities.clone(),
            registered_at: node.registered_at,
            last_heartbeat: node.last_heartbeat,
            draining: node.draining,
            workload_count,
        }
    }
}

/// Status information for a single workload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadStatus {
    /// Workload ID.
    pub id: WorkloadId,
    /// Container image.
    pub image: String,
    /// Current state.
    pub state: WorkloadState,
    /// Assigned node ID (if any).
    pub node_id: Option<NodeId>,
    /// When the workload was created.
    pub created_at: DateTime<Utc>,
    /// When the workload was last updated.
    pub updated_at: DateTime<Utc>,
    /// Error message (if failed).
    pub error: Option<String>,
}

impl WorkloadStatus {
    /// Create a workload status from a tracked workload.
    pub fn from_tracked(w: &TrackedWorkload) -> Self {
        // Use finished_at as updated_at if available, otherwise use started_at, otherwise created_at
        let updated_at = w.workload.status.finished_at
            .or(w.workload.status.started_at)
            .unwrap_or(w.workload.created_at);

        // Convert exit_code to error message for failed workloads
        let error = if w.state() == WorkloadState::Failed {
            w.workload.status.exit_code.map(|code| format!("exit code: {code}"))
        } else {
            None
        };

        Self {
            id: w.id(),
            image: w.workload.spec.image.clone(),
            state: w.state(),
            node_id: w.assigned_node,
            created_at: w.workload.created_at,
            updated_at,
            error,
        }
    }
}

/// Metrics snapshot for the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp of this snapshot.
    pub timestamp: DateTime<Utc>,
    /// Node metrics.
    pub nodes: Vec<NodeMetrics>,
    /// Cluster-wide GPU summary.
    pub gpu_summary: GpuUtilizationSummary,
}

/// Metrics for a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// Node ID.
    pub node_id: NodeId,
    /// Node name.
    pub name: String,
    /// Health status.
    pub health: NodeHealthStatus,
    /// GPU metrics (if available).
    pub gpus: Vec<GpuMetrics>,
}

/// Metrics for a single GPU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuMetrics {
    /// GPU index.
    pub index: u32,
    /// GPU model name.
    pub name: String,
    /// Utilization percentage (0-100).
    pub utilization_percent: u8,
    /// Memory used in MiB.
    pub memory_used_mib: u64,
    /// Memory total in MiB.
    pub memory_total_mib: u64,
    /// Temperature in Celsius.
    pub temperature_celsius: u32,
}

/// Log entry from a workload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Timestamp of the log entry.
    pub timestamp: DateTime<Utc>,
    /// Log stream (stdout or stderr).
    pub stream: LogStream,
    /// Log message.
    pub message: String,
}

/// Log stream type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
}

/// Real-time update sent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum LiveUpdate {
    /// Node health changed.
    NodeHealthChanged {
        /// Node ID.
        node_id: NodeId,
        /// New health status.
        health: NodeHealthStatus,
    },

    /// Node registered.
    NodeRegistered {
        /// Node status.
        node: NodeStatus,
    },

    /// Node unregistered.
    NodeUnregistered {
        /// Node ID.
        node_id: NodeId,
    },

    /// Workload state changed.
    WorkloadStateChanged {
        /// Workload ID.
        workload_id: WorkloadId,
        /// New state.
        state: WorkloadState,
        /// Error message (if failed).
        error: Option<String>,
    },

    /// New workload created.
    WorkloadCreated {
        /// Workload status.
        workload: WorkloadStatus,
    },

    /// Workload deleted.
    WorkloadDeleted {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// Workload assigned to node.
    WorkloadAssigned {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Assigned node ID.
        node_id: NodeId,
    },

    /// Metrics update (periodic).
    MetricsUpdate {
        /// Metrics snapshot.
        metrics: MetricsSnapshot,
    },

    /// Log line from workload.
    LogLine {
        /// Log entry.
        entry: LogEntry,
    },

    /// Heartbeat/ping message.
    Heartbeat {
        /// Server timestamp.
        timestamp: DateTime<Utc>,
    },
}

impl LiveUpdate {
    /// Get the event type name for SSE.
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::NodeHealthChanged { .. } => "node_health_changed",
            Self::NodeRegistered { .. } => "node_registered",
            Self::NodeUnregistered { .. } => "node_unregistered",
            Self::WorkloadStateChanged { .. } => "workload_state_changed",
            Self::WorkloadCreated { .. } => "workload_created",
            Self::WorkloadDeleted { .. } => "workload_deleted",
            Self::WorkloadAssigned { .. } => "workload_assigned",
            Self::MetricsUpdate { .. } => "metrics_update",
            Self::LogLine { .. } => "log_line",
            Self::Heartbeat { .. } => "heartbeat",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_proto::GpuCapability;

    // ==================== NodeHealthSummary Tests ====================

    #[test]
    fn test_node_health_summary_default() {
        let summary = NodeHealthSummary::default();

        assert_eq!(summary.healthy, 0);
        assert_eq!(summary.unhealthy, 0);
        assert_eq!(summary.draining, 0);
        assert_eq!(summary.offline, 0);
    }

    #[test]
    fn test_node_health_summary_from_health_summary() {
        let health = HealthSummary {
            healthy: 5,
            unhealthy: 2,
            draining: 1,
            offline: 0,
        };

        let summary: NodeHealthSummary = health.into();

        assert_eq!(summary.healthy, 5);
        assert_eq!(summary.unhealthy, 2);
        assert_eq!(summary.draining, 1);
        assert_eq!(summary.offline, 0);
    }

    // ==================== WorkloadStateSummary Tests ====================

    #[test]
    fn test_workload_state_summary_default() {
        let summary = WorkloadStateSummary::default();

        assert_eq!(summary.total(), 0);
    }

    #[test]
    fn test_workload_state_summary_add() {
        let mut summary = WorkloadStateSummary::default();

        summary.add(WorkloadState::Pending);
        summary.add(WorkloadState::Pending);
        summary.add(WorkloadState::Running);
        summary.add(WorkloadState::Completed);
        summary.add(WorkloadState::Failed);

        assert_eq!(summary.pending, 2);
        assert_eq!(summary.running, 1);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.total(), 5);
    }

    #[test]
    fn test_workload_state_summary_all_states() {
        let mut summary = WorkloadStateSummary::default();

        summary.add(WorkloadState::Pending);
        summary.add(WorkloadState::Starting);
        summary.add(WorkloadState::Running);
        summary.add(WorkloadState::Stopping);
        summary.add(WorkloadState::Stopped);
        summary.add(WorkloadState::Completed);
        summary.add(WorkloadState::Failed);

        assert_eq!(summary.pending, 1);
        assert_eq!(summary.starting, 1);
        assert_eq!(summary.running, 1);
        assert_eq!(summary.stopping, 1);
        assert_eq!(summary.stopped, 1);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.total(), 7);
    }

    // ==================== GpuUtilizationSummary Tests ====================

    #[test]
    fn test_gpu_utilization_summary_default() {
        let summary = GpuUtilizationSummary::default();

        assert_eq!(summary.total_gpus, 0);
        assert_eq!(summary.total_vram_mib, 0);
        assert_eq!(summary.used_vram_mib, 0);
        assert!((summary.avg_utilization_percent - 0.0).abs() < f32::EPSILON);
    }

    // ==================== LogStream Tests ====================

    #[test]
    fn test_log_stream_serialization() {
        let stdout = LogStream::Stdout;
        let stderr = LogStream::Stderr;

        let stdout_json = serde_json::to_string(&stdout).unwrap();
        let stderr_json = serde_json::to_string(&stderr).unwrap();

        assert_eq!(stdout_json, r#""stdout""#);
        assert_eq!(stderr_json, r#""stderr""#);
    }

    #[test]
    fn test_log_stream_deserialization() {
        let stdout: LogStream = serde_json::from_str(r#""stdout""#).unwrap();
        let stderr: LogStream = serde_json::from_str(r#""stderr""#).unwrap();

        assert_eq!(stdout, LogStream::Stdout);
        assert_eq!(stderr, LogStream::Stderr);
    }

    // ==================== LiveUpdate Tests ====================

    #[test]
    fn test_live_update_event_types() {
        let updates = vec![
            (
                LiveUpdate::NodeHealthChanged {
                    node_id: NodeId::new(),
                    health: NodeHealthStatus::Healthy,
                },
                "node_health_changed",
            ),
            (
                LiveUpdate::NodeRegistered {
                    node: NodeStatus {
                        id: NodeId::new(),
                        name: "test".into(),
                        health: NodeHealthStatus::Healthy,
                        capabilities: NodeCapabilities::default(),
                        registered_at: Utc::now(),
                        last_heartbeat: Utc::now(),
                        draining: false,
                        workload_count: 0,
                    },
                },
                "node_registered",
            ),
            (
                LiveUpdate::NodeUnregistered {
                    node_id: NodeId::new(),
                },
                "node_unregistered",
            ),
            (
                LiveUpdate::WorkloadStateChanged {
                    workload_id: WorkloadId::new(),
                    state: WorkloadState::Running,
                    error: None,
                },
                "workload_state_changed",
            ),
            (
                LiveUpdate::Heartbeat {
                    timestamp: Utc::now(),
                },
                "heartbeat",
            ),
        ];

        for (update, expected_type) in updates {
            assert_eq!(update.event_type(), expected_type);
        }
    }

    #[test]
    fn test_live_update_serialization() {
        let update = LiveUpdate::NodeHealthChanged {
            node_id: NodeId::new(),
            health: NodeHealthStatus::Unhealthy,
        };

        let json = serde_json::to_string(&update).unwrap();

        // The serde tag serialization uses PascalCase by default
        assert!(json.contains("NodeHealthChanged"));
        assert!(json.contains("unhealthy"));
    }

    #[test]
    fn test_live_update_workload_state_with_error() {
        let update = LiveUpdate::WorkloadStateChanged {
            workload_id: WorkloadId::new(),
            state: WorkloadState::Failed,
            error: Some("out of memory".to_string()),
        };

        let json = serde_json::to_string(&update).unwrap();

        assert!(json.contains("failed"));
        assert!(json.contains("out of memory"));
    }

    // ==================== ClusterStatus Tests ====================

    #[test]
    fn test_cluster_status_serialization() {
        let status = ClusterStatus {
            total_nodes: 5,
            node_health: NodeHealthSummary {
                healthy: 4,
                unhealthy: 1,
                draining: 0,
                offline: 0,
            },
            total_workloads: 10,
            workload_states: WorkloadStateSummary {
                pending: 2,
                starting: 1,
                running: 5,
                stopping: 0,
                stopped: 0,
                completed: 2,
                failed: 0,
            },
            gpu_utilization: GpuUtilizationSummary {
                total_gpus: 8,
                total_vram_mib: 65536,
                used_vram_mib: 32768,
                avg_utilization_percent: 75.5,
            },
            timestamp: Utc::now(),
            uptime_secs: 3600,
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ClusterStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_nodes, 5);
        assert_eq!(deserialized.total_workloads, 10);
        assert_eq!(deserialized.gpu_utilization.total_gpus, 8);
    }

    // ==================== NodeStatus Tests ====================

    #[test]
    fn test_node_status_from_registered() {
        use claw_gateway::NodeRegistry;

        let mut registry = NodeRegistry::new();
        let node_id = NodeId::new();
        let caps = NodeCapabilities::new(8, 16384)
            .with_gpu(GpuCapability {
                index: 0,
                name: "RTX 4090".into(),
                memory_mib: 24576,
                uuid: "gpu-uuid".into(),
            });

        registry.register_with_name(node_id, "test-node", caps).unwrap();
        let registered = registry.get_node(node_id).unwrap();

        let status = NodeStatus::from_registered(registered, 3);

        assert_eq!(status.id, node_id);
        assert_eq!(status.name, "test-node");
        assert_eq!(status.health, NodeHealthStatus::Healthy);
        assert!(!status.draining);
        assert_eq!(status.workload_count, 3);
    }

    // ==================== MetricsSnapshot Tests ====================

    #[test]
    fn test_metrics_snapshot_serialization() {
        let snapshot = MetricsSnapshot {
            timestamp: Utc::now(),
            nodes: vec![NodeMetrics {
                node_id: NodeId::new(),
                name: "node-1".into(),
                health: NodeHealthStatus::Healthy,
                gpus: vec![GpuMetrics {
                    index: 0,
                    name: "RTX 4090".into(),
                    utilization_percent: 85,
                    memory_used_mib: 20000,
                    memory_total_mib: 24576,
                    temperature_celsius: 65,
                }],
            }],
            gpu_summary: GpuUtilizationSummary::default(),
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: MetricsSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.nodes.len(), 1);
        assert_eq!(deserialized.nodes[0].gpus.len(), 1);
        assert_eq!(deserialized.nodes[0].gpus[0].utilization_percent, 85);
    }
}
