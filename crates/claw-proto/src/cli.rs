//! CLI protocol messages for admin/control operations.
//!
//! This module defines the protocol between `claw-cli` and `claw-gateway-server`.
//! CLI connections are distinct from node connections and support administrative
//! operations like listing nodes, managing workloads, and querying status.
//!
//! # Message Flow
//!
//! ```text
//! ┌──────────┐     CliMessage      ┌─────────────────┐
//! │ claw-cli │────────────────────►│  GatewayServer  │
//! │          │◄────────────────────│                 │
//! └──────────┘     CliResponse     └─────────────────┘
//! ```
//!
//! # Example
//!
//! ```rust
//! use claw_proto::cli::{CliMessage, CliResponse};
//!
//! // Request cluster status
//! let request = CliMessage::GetStatus;
//! let json = request.to_json().unwrap();
//! assert!(json.contains("get_status"));
//!
//! // Parse response
//! let response: CliResponse = serde_json::from_str(r#"
//!     {
//!         "type": "status",
//!         "node_count": 5,
//!         "healthy_nodes": 4,
//!         "gpu_count": 12,
//!         "active_workloads": 2,
//!         "total_vram_mib": 98304,
//!         "gateway_version": "0.1.0",
//!         "uptime_secs": 3600
//!     }
//! "#).unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{GpuMetricsProto, NodeCapabilities, NodeId, WorkloadId, WorkloadState};
use crate::workload::{WorkloadSpec, WorkloadStatus};
use crate::ProtoError;

/// Protocol version for CLI communication.
pub const CLI_PROTOCOL_VERSION: u32 = 1;

/// Messages sent from CLI to gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliMessage {
    /// Handshake to identify as CLI client.
    Hello {
        /// Client version.
        version: String,
        /// Protocol version.
        protocol_version: u32,
    },

    /// Request cluster status overview.
    GetStatus,

    /// List all registered nodes.
    ListNodes {
        /// Filter: only show nodes with this state.
        state_filter: Option<NodeState>,
        /// Include detailed capabilities.
        include_capabilities: bool,
    },

    /// Get detailed info for a specific node.
    GetNode {
        /// Node ID.
        node_id: NodeId,
    },

    /// List all workloads.
    ListWorkloads {
        /// Filter by node.
        node_filter: Option<NodeId>,
        /// Filter by state.
        state_filter: Option<WorkloadState>,
    },

    /// Get detailed info for a specific workload.
    GetWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// Start a new workload.
    StartWorkload {
        /// Target node (None for auto-scheduling).
        node_id: Option<NodeId>,
        /// Workload specification.
        spec: WorkloadSpec,
    },

    /// Stop a running workload.
    StopWorkload {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Force stop (no grace period).
        force: bool,
    },

    /// Get workload logs.
    GetLogs {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Number of lines (None for all).
        tail: Option<u32>,
        /// Include stderr.
        include_stderr: bool,
    },

    /// Drain or undrain a node.
    ///
    /// Draining a node prevents new workloads from being scheduled on it,
    /// but allows existing workloads to continue running.
    DrainNode {
        /// Node ID.
        node_id: NodeId,
        /// Whether to drain (true) or undrain (false).
        drain: bool,
    },

    /// Get MOLT network status.
    GetMoltStatus,

    /// List MOLT peers.
    ListMoltPeers,

    /// Get MOLT wallet balance.
    GetMoltBalance,

    /// Ping to check connection.
    Ping {
        /// Timestamp for latency measurement.
        timestamp: DateTime<Utc>,
    },

    // =========================================================================
    // Advanced Scheduling Commands
    // =========================================================================

    /// Clear a scheduling gate for a workload.
    ClearGate {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Gate name to clear.
        gate_name: String,
    },

    /// Get scheduling status for a workload.
    GetSchedulingStatus {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// List workloads that are gated (waiting for conditions).
    ListGatedWorkloads,

    /// Update a node's conditions.
    UpdateNodeCondition {
        /// Node ID.
        node_id: NodeId,
        /// Condition type (e.g., "cuda-12-ready", "model-cached").
        condition_type: String,
        /// Whether the condition is satisfied.
        satisfied: bool,
        /// Optional reason.
        reason: Option<String>,
    },

    /// Set a node label.
    SetNodeLabel {
        /// Node ID.
        node_id: NodeId,
        /// Label key.
        key: String,
        /// Label value.
        value: String,
    },

    /// Remove a node label.
    RemoveNodeLabel {
        /// Node ID.
        node_id: NodeId,
        /// Label key to remove.
        key: String,
    },

    /// Get node conditions and labels.
    GetNodeConditions {
        /// Node ID.
        node_id: NodeId,
    },
}

/// State of a node.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    /// Node is healthy and accepting workloads.
    Healthy,
    /// Node is unhealthy (missed heartbeats, high temps, etc.).
    Unhealthy,
    /// Node is draining (not accepting new workloads).
    Draining,
    /// Node is offline.
    Offline,
}

impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Draining => write!(f, "draining"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// Responses sent from gateway to CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliResponse {
    /// Handshake accepted.
    Welcome {
        /// Server version.
        server_version: String,
        /// Protocol version.
        protocol_version: u32,
    },

    /// Cluster status overview.
    Status {
        /// Total number of nodes.
        node_count: u32,
        /// Number of healthy nodes.
        healthy_nodes: u32,
        /// Total number of GPUs.
        gpu_count: u32,
        /// Number of active workloads.
        active_workloads: u32,
        /// Total VRAM across all GPUs in MiB.
        total_vram_mib: u64,
        /// Gateway version.
        gateway_version: String,
        /// Server uptime in seconds.
        uptime_secs: u64,
    },

    /// List of nodes.
    Nodes {
        /// Node information.
        nodes: Vec<NodeInfo>,
    },

    /// Single node details.
    Node {
        /// Node information.
        node: NodeInfo,
    },

    /// List of workloads.
    Workloads {
        /// Workload information.
        workloads: Vec<WorkloadInfo>,
    },

    /// Single workload details.
    Workload {
        /// Workload information.
        workload: WorkloadInfo,
    },

    /// Workload started successfully.
    WorkloadStarted {
        /// Assigned workload ID.
        workload_id: WorkloadId,
        /// Assigned node.
        node_id: NodeId,
    },

    /// Workload stopped.
    WorkloadStopped {
        /// Workload ID.
        workload_id: WorkloadId,
    },

    /// Node drain status changed.
    NodeDrained {
        /// Node ID.
        node_id: NodeId,
        /// Whether the node is now draining.
        draining: bool,
    },

    /// Workload logs.
    Logs {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Log lines (stdout).
        stdout_lines: Vec<String>,
        /// Log lines (stderr).
        stderr_lines: Vec<String>,
    },

    /// MOLT network status.
    MoltStatus {
        /// Whether connected to MOLT network.
        connected: bool,
        /// Number of peers.
        peer_count: u32,
        /// Local node ID on MOLT.
        node_id: Option<String>,
        /// Network region.
        region: Option<String>,
    },

    /// MOLT peers list.
    MoltPeers {
        /// Peer information.
        peers: Vec<MoltPeerInfo>,
    },

    /// MOLT wallet balance.
    MoltBalance {
        /// Balance in smallest unit.
        balance: u64,
        /// Pending balance.
        pending: u64,
        /// Staked amount.
        staked: u64,
    },

    /// Pong response.
    Pong {
        /// Original timestamp.
        client_timestamp: DateTime<Utc>,
        /// Server timestamp.
        server_timestamp: DateTime<Utc>,
    },

    // =========================================================================
    // Advanced Scheduling Responses
    // =========================================================================

    /// Gate cleared successfully.
    GateCleared {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Gate that was cleared.
        gate_name: String,
        /// Remaining pending gates.
        pending_gates: Vec<String>,
    },

    /// Scheduling status for a workload.
    SchedulingStatus {
        /// Workload ID.
        workload_id: WorkloadId,
        /// Current workload state.
        state: WorkloadState,
        /// Pending scheduling gates.
        pending_gates: Vec<String>,
        /// Assigned node (if scheduled).
        assigned_node: Option<NodeId>,
        /// Assigned GPU indices (if scheduled).
        assigned_gpus: Vec<u32>,
        /// Worker index for parallel workloads.
        worker_index: Option<u32>,
        /// Why scheduling failed (if applicable).
        schedule_failure_reason: Option<String>,
    },

    /// List of gated workloads.
    GatedWorkloads {
        /// Workloads waiting on gates.
        workloads: Vec<GatedWorkloadInfo>,
    },

    /// Node condition updated.
    NodeConditionUpdated {
        /// Node ID.
        node_id: NodeId,
        /// Condition type.
        condition_type: String,
        /// New status.
        satisfied: bool,
    },

    /// Node label set.
    NodeLabelSet {
        /// Node ID.
        node_id: NodeId,
        /// Label key.
        key: String,
        /// Label value.
        value: String,
    },

    /// Node label removed.
    NodeLabelRemoved {
        /// Node ID.
        node_id: NodeId,
        /// Label key that was removed.
        key: String,
    },

    /// Node conditions and labels.
    NodeConditions {
        /// Node ID.
        node_id: NodeId,
        /// Current conditions.
        conditions: Vec<NodeConditionInfo>,
        /// Current labels.
        labels: Vec<NodeLabelInfo>,
    },

    /// Error response.
    Error {
        /// Error code.
        code: u32,
        /// Error message.
        message: String,
        /// Original request type (if applicable).
        request_type: Option<String>,
    },
}

/// Information about a registered node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeInfo {
    /// Node ID.
    pub node_id: NodeId,
    /// Human-readable name.
    pub name: String,
    /// Current state.
    pub state: NodeState,
    /// Number of GPUs.
    pub gpu_count: u32,
    /// Total VRAM in MiB.
    pub total_vram_mib: u64,
    /// Number of running workloads.
    pub running_workloads: u32,
    /// Last heartbeat time.
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Detailed capabilities (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<NodeCapabilities>,
    /// Latest GPU metrics (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_metrics: Option<Vec<GpuMetricsProto>>,
}

/// Information about a workload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadInfo {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Node running the workload.
    pub node_id: NodeId,
    /// Current state.
    pub state: WorkloadState,
    /// Container image.
    pub image: String,
    /// When the workload was created.
    pub created_at: DateTime<Utc>,
    /// When the workload started running.
    pub started_at: Option<DateTime<Utc>>,
    /// When the workload finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// Detailed status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<WorkloadStatus>,
}

/// Information about a MOLT peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MoltPeerInfo {
    /// Peer ID.
    pub peer_id: String,
    /// Region.
    pub region: Option<String>,
    /// GPU count.
    pub gpu_count: u32,
    /// Available for workloads.
    pub available: bool,
    /// Latency in milliseconds.
    pub latency_ms: Option<u32>,
}

/// Information about a workload waiting on scheduling gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatedWorkloadInfo {
    /// Workload ID.
    pub workload_id: WorkloadId,
    /// Container image.
    pub image: String,
    /// Pending gates.
    pub pending_gates: Vec<String>,
    /// When the workload was submitted.
    pub submitted_at: DateTime<Utc>,
    /// How long it's been waiting.
    pub waiting_secs: u64,
}

/// Information about a node condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConditionInfo {
    /// Condition type (e.g., "cuda-12-ready").
    pub condition_type: String,
    /// Whether the condition is satisfied.
    pub satisfied: bool,
    /// Reason for the status.
    pub reason: Option<String>,
    /// When the condition was last updated.
    pub last_updated: DateTime<Utc>,
}

/// Information about a node label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeLabelInfo {
    /// Label key.
    pub key: String,
    /// Label value.
    pub value: String,
}

// === Error codes ===

/// Error codes for CLI responses.
pub mod error_codes {
    /// Node not found.
    pub const NODE_NOT_FOUND: u32 = 1001;
    /// Workload not found.
    pub const WORKLOAD_NOT_FOUND: u32 = 1002;
    /// Invalid request.
    pub const INVALID_REQUEST: u32 = 1003;
    /// No capacity available.
    pub const NO_CAPACITY: u32 = 1004;
    /// Permission denied.
    pub const PERMISSION_DENIED: u32 = 1005;
    /// Internal error.
    pub const INTERNAL_ERROR: u32 = 1006;
    /// MOLT not connected.
    pub const MOLT_NOT_CONNECTED: u32 = 1007;
    /// Protocol version mismatch.
    pub const PROTOCOL_MISMATCH: u32 = 1008;
}

impl CliMessage {
    /// Create a hello message.
    #[must_use]
    pub fn hello(version: impl Into<String>) -> Self {
        Self::Hello {
            version: version.into(),
            protocol_version: CLI_PROTOCOL_VERSION,
        }
    }

    /// Create a ping message.
    #[must_use]
    pub fn ping() -> Self {
        Self::Ping {
            timestamp: Utc::now(),
        }
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, ProtoError> {
        serde_json::to_string(self).map_err(|e| ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, ProtoError> {
        serde_json::from_str(json).map_err(|e| ProtoError::Decoding(e.to_string()))
    }

    /// Get the request type name for error reporting.
    #[must_use]
    pub fn request_type(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::GetStatus => "get_status",
            Self::ListNodes { .. } => "list_nodes",
            Self::GetNode { .. } => "get_node",
            Self::ListWorkloads { .. } => "list_workloads",
            Self::GetWorkload { .. } => "get_workload",
            Self::StartWorkload { .. } => "start_workload",
            Self::StopWorkload { .. } => "stop_workload",
            Self::GetLogs { .. } => "get_logs",
            Self::DrainNode { .. } => "drain_node",
            Self::GetMoltStatus => "get_molt_status",
            Self::ListMoltPeers => "list_molt_peers",
            Self::GetMoltBalance => "get_molt_balance",
            Self::Ping { .. } => "ping",
            Self::ClearGate { .. } => "clear_gate",
            Self::GetSchedulingStatus { .. } => "get_scheduling_status",
            Self::ListGatedWorkloads => "list_gated_workloads",
            Self::UpdateNodeCondition { .. } => "update_node_condition",
            Self::SetNodeLabel { .. } => "set_node_label",
            Self::RemoveNodeLabel { .. } => "remove_node_label",
            Self::GetNodeConditions { .. } => "get_node_conditions",
        }
    }
}

impl CliResponse {
    /// Create a welcome response.
    #[must_use]
    pub fn welcome(server_version: impl Into<String>) -> Self {
        Self::Welcome {
            server_version: server_version.into(),
            protocol_version: CLI_PROTOCOL_VERSION,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(code: u32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
            request_type: None,
        }
    }

    /// Create an error response with request type.
    #[must_use]
    pub fn error_for_request(code: u32, message: impl Into<String>, request_type: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
            request_type: Some(request_type.into()),
        }
    }

    /// Create a pong response.
    #[must_use]
    pub fn pong(client_timestamp: DateTime<Utc>) -> Self {
        Self::Pong {
            client_timestamp,
            server_timestamp: Utc::now(),
        }
    }

    /// Check if this is an error response.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, ProtoError> {
        serde_json::to_string(self).map_err(|e| ProtoError::Encoding(e.to_string()))
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, ProtoError> {
        serde_json::from_str(json).map_err(|e| ProtoError::Decoding(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_message() {
        let msg = CliMessage::hello("1.0.0");
        let json = msg.to_json().unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("1.0.0"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_get_status_message() {
        let msg = CliMessage::GetStatus;
        let json = msg.to_json().unwrap();
        assert!(json.contains("get_status"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_list_nodes_message() {
        let msg = CliMessage::ListNodes {
            state_filter: Some(NodeState::Healthy),
            include_capabilities: true,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("list_nodes"));
        assert!(json.contains("healthy"));

        let parsed = CliMessage::from_json(&json).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn test_status_response() {
        let resp = CliResponse::Status {
            node_count: 5,
            healthy_nodes: 4,
            gpu_count: 12,
            active_workloads: 3,
            total_vram_mib: 245760,
            gateway_version: "0.1.0".into(),
            uptime_secs: 3600,
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("status"));
        assert!(json.contains("245760"));

        let parsed = CliResponse::from_json(&json).unwrap();
        assert_eq!(resp, parsed);
    }

    #[test]
    fn test_error_response() {
        let resp = CliResponse::error(error_codes::NODE_NOT_FOUND, "Node not found");
        assert!(resp.is_error());

        let json = resp.to_json().unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("1001"));
    }

    #[test]
    fn test_error_response_with_request_type() {
        let resp = CliResponse::error_for_request(
            error_codes::NODE_NOT_FOUND,
            "Node not found",
            "get_node",
        );
        let json = resp.to_json().unwrap();
        assert!(json.contains("get_node"));
    }

    #[test]
    fn test_ping_pong() {
        let ping = CliMessage::ping();
        let json = ping.to_json().unwrap();
        assert!(json.contains("ping"));

        if let CliMessage::Ping { timestamp } = ping {
            let pong = CliResponse::pong(timestamp);
            let pong_json = pong.to_json().unwrap();
            assert!(pong_json.contains("pong"));
        }
    }

    #[test]
    fn test_node_info_serialization() {
        let info = NodeInfo {
            node_id: NodeId::new(),
            name: "test-node".into(),
            state: NodeState::Healthy,
            gpu_count: 4,
            total_vram_mib: 98304,
            running_workloads: 2,
            last_heartbeat: Some(Utc::now()),
            capabilities: None,
            gpu_metrics: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-node"));
        assert!(json.contains("healthy"));
    }

    #[test]
    fn test_workload_info_serialization() {
        let info = WorkloadInfo {
            workload_id: WorkloadId::new(),
            node_id: NodeId::new(),
            state: WorkloadState::Running,
            image: "nvidia/cuda:12.0".into(),
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            finished_at: None,
            status: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("nvidia/cuda"));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_molt_peer_info_serialization() {
        let info = MoltPeerInfo {
            peer_id: "peer-123".into(),
            region: Some("us-west".into()),
            gpu_count: 8,
            available: true,
            latency_ms: Some(25),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("peer-123"));
        assert!(json.contains("us-west"));
    }

    #[test]
    fn test_request_type() {
        assert_eq!(CliMessage::GetStatus.request_type(), "get_status");
        assert_eq!(CliMessage::hello("1.0").request_type(), "hello");
        assert_eq!(CliMessage::ping().request_type(), "ping");
    }

    #[test]
    fn test_node_state_display() {
        assert_eq!(NodeState::Healthy.to_string(), "healthy");
        assert_eq!(NodeState::Unhealthy.to_string(), "unhealthy");
        assert_eq!(NodeState::Draining.to_string(), "draining");
        assert_eq!(NodeState::Offline.to_string(), "offline");
    }

    #[test]
    fn test_welcome_response() {
        let resp = CliResponse::welcome("0.1.0");
        let json = resp.to_json().unwrap();
        assert!(json.contains("welcome"));
        assert!(json.contains("0.1.0"));
    }

    #[test]
    fn test_start_workload_message() {
        let spec = WorkloadSpec::new("nvidia/cuda:12.0")
            .with_command(vec!["nvidia-smi".into()])
            .with_gpu_count(1)
            .with_memory_mb(8192)
            .with_cpu_cores(2);
        let msg = CliMessage::StartWorkload {
            node_id: None,
            spec,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("start_workload"));
        assert!(json.contains("nvidia/cuda"));
    }

    #[test]
    fn test_stop_workload_message() {
        let msg = CliMessage::StopWorkload {
            workload_id: WorkloadId::new(),
            force: true,
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("stop_workload"));
        assert!(json.contains("force"));
    }

    #[test]
    fn test_workload_started_response() {
        let resp = CliResponse::WorkloadStarted {
            workload_id: WorkloadId::new(),
            node_id: NodeId::new(),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("workload_started"));
    }

    #[test]
    fn test_logs_response() {
        let resp = CliResponse::Logs {
            workload_id: WorkloadId::new(),
            stdout_lines: vec!["line 1".into(), "line 2".into()],
            stderr_lines: vec!["error 1".into()],
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("logs"));
        assert!(json.contains("line 1"));
        assert!(json.contains("error 1"));
    }

    #[test]
    fn test_molt_status_response() {
        let resp = CliResponse::MoltStatus {
            connected: true,
            peer_count: 15,
            node_id: Some("molt-node-123".into()),
            region: Some("us-west".into()),
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("molt_status"));
        assert!(json.contains("molt-node-123"));
    }

    #[test]
    fn test_molt_balance_response() {
        let resp = CliResponse::MoltBalance {
            balance: 1_000_000,
            pending: 50_000,
            staked: 500_000,
        };
        let json = resp.to_json().unwrap();
        assert!(json.contains("molt_balance"));
        assert!(json.contains("1000000"));
    }
}
